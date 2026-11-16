//! Modo web: o servidor de rede (rede.rs) também entrega a própria aplicação ao browser.
//!
//! Um telemóvel, um tablet (Android ou iOS), um terminal POS Android ou outro computador abre
//! `http://IP-DO-SERVIDOR:7799` e trabalha no KanGest sem instalar nada: os ficheiros da aplicação
//! (os mesmos que vão embutidos no executável) saem daqui, e os pedidos seguem pelo protocolo cifrado
//! da rede, com a chave de rede, como os outros postos. Os ficheiros não têm dados: sem a chave de
//! rede o browser não consegue ler nem gravar nada.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

/// Devolve o conteúdo e o tipo MIME de um ficheiro da aplicação (caminho sem "/" inicial).
pub type Activos = Box<dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync>;

static ACTIVOS: OnceLock<Activos> = OnceLock::new();

/// Regista de onde vêm os ficheiros da aplicação (no arranque: os activos embutidos do Tauri).
pub fn registar(f: Activos) {
    let _ = ACTIVOS.set(f);
}

/// Política de segurança das páginas servidas ao browser: só código e ligações do próprio servidor.
pub const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
font-src 'self' data:; connect-src 'self'; worker-src 'self'; manifest-src 'self'; media-src 'self' blob:; object-src 'none'; \
base-uri 'self'; form-action 'none'; frame-ancestors 'none'";

fn tipo_por_extensao(caminho: &str) -> Option<&'static str> {
    let ext = caminho.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => return None,
    })
}

/// Pasta com a aplicação compilada (`dist`), para testes e desenvolvimento: `KANGEST_WEB_DIR`.
fn da_pasta(caminho: &str) -> Option<(Vec<u8>, String)> {
    let pasta = std::env::var_os("KANGEST_WEB_DIR")?;
    let f = std::path::Path::new(&pasta).join(caminho);
    let bytes = std::fs::read(f).ok()?;
    Some((bytes, tipo_por_extensao(caminho).unwrap_or("application/octet-stream").to_string()))
}

fn obter(caminho: &str) -> Option<(Vec<u8>, String)> {
    ACTIVOS.get().and_then(|f| f(caminho)).or_else(|| da_pasta(caminho))
}

/// Há aplicação para servir? (num `tauri dev` os ficheiros não vão embutidos)
pub fn disponivel() -> bool {
    obter("index.html").is_some()
}

/// Caminho pedido → ficheiro da aplicação. Sem extensão (navegação) é sempre a página inicial.
fn ficheiro_de(caminho: &str) -> Option<String> {
    let c = caminho.split(['?', '#']).next().unwrap_or("").trim_start_matches('/');
    if c.contains("..") || c.contains('\\') || c.contains('\0') || c.contains("//") || c.contains('%') {
        return None;
    }
    let ultimo = c.rsplit('/').next().unwrap_or("");
    if c.is_empty() || !ultimo.contains('.') {
        return Some("index.html".into());
    }
    Some(c.to_string())
}

fn comprimivel(tipo: &str) -> bool {
    tipo.starts_with("text/") || tipo.contains("json") || tipo.contains("javascript") || tipo.starts_with("image/svg")
}

/// Versões comprimidas já feitas, pela impressão digital do conteúdo (numa pasta de desenvolvimento o
/// mesmo caminho muda de conteúdo a cada compilação; os embutidos nunca mudam).
fn gzip(caminho: &str, bytes: &[u8]) -> Arc<Vec<u8>> {
    use std::hash::{Hash, Hasher};
    static CACHE: OnceLock<Mutex<HashMap<(String, u64), Arc<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let chave = (caminho.to_string(), h.finish());
    if let Some(v) = cache.lock().unwrap().get(&chave) {
        return v.clone();
    }
    let mut e = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 3), flate2::Compression::default());
    let _ = e.write_all(bytes);
    let v = Arc::new(e.finish().unwrap_or_default());
    let mut c = cache.lock().unwrap();
    // versões antigas do mesmo caminho saem (só acontece em desenvolvimento)
    c.retain(|(k, _), _| k != caminho);
    c.insert(chave, v.clone());
    v
}

fn pagina_simples(estado: &str, titulo: &str, texto: &str) -> Vec<u8> {
    let corpo = format!(
        "<!doctype html><html lang=\"pt\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>KanGest</title><body style=\"font-family:system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1.5rem;color:#1f2937\">\
<h1 style=\"font-size:1.4rem\">{titulo}</h1><p style=\"line-height:1.5\">{texto}</p></body></html>"
    );
    let mut r = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nConnection: close\r\n\r\n",
        corpo.len()
    )
    .into_bytes();
    r.extend_from_slice(corpo.as_bytes());
    r
}

/// Página que explica que o acesso pelo browser está desligado neste servidor.
pub fn desligado() -> Vec<u8> {
    pagina_simples(
        "404 Not Found",
        "Acesso pelo browser desligado",
        "Este computador é um servidor KanGest, mas o acesso pelo browser está desligado. \
Ligue-o no KanGest do servidor, em Configurações → Rede → «Permitir postos pelo browser».",
    )
}

/// Resposta HTTP completa a um GET/HEAD de um ficheiro da aplicação.
pub fn responder(caminho: &str, aceita_gzip: bool, cabeca: bool) -> Vec<u8> {
    let Some(rel) = ficheiro_de(caminho) else {
        return pagina_simples("400 Bad Request", "Pedido inválido", "Endereço inválido.");
    };
    let Some((bytes, mime)) = obter(&rel) else {
        if rel == "index.html" {
            return pagina_simples(
                "503 Service Unavailable",
                "Aplicação web indisponível",
                "Esta instalação do KanGest não traz a aplicação para o browser (versão de desenvolvimento). Use a versão instalada.",
            );
        }
        return pagina_simples("404 Not Found", "Não encontrado", "O ficheiro pedido não existe.");
    };
    // o Tauri responde com a página inicial a qualquer caminho desconhecido: um .js/.png que falte é 404
    if mime.starts_with("text/html") && !rel.ends_with(".html") {
        return pagina_simples("404 Not Found", "Não encontrado", "O ficheiro pedido não existe.");
    }
    let tipo = tipo_por_extensao(&rel).map(String::from).unwrap_or(mime);
    let html = tipo.starts_with("text/html");
    // nomes com impressão digital (assets/x-AbC123.js) nunca mudam; o resto revalida-se sempre
    let cache = if rel.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    let (corpo, codificacao) = if aceita_gzip && comprimivel(&tipo) && bytes.len() > 1024 {
        (gzip(&rel, &bytes), "Content-Encoding: gzip\r\nVary: Accept-Encoding\r\n")
    } else {
        (Arc::new(bytes), "")
    };
    let mut extra = String::new();
    if html {
        extra.push_str(&format!("Content-Security-Policy: {CSP}\r\n"));
    }
    if rel == "sw.js" {
        extra.push_str("Service-Worker-Allowed: /\r\n");
    }
    let mut r = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\n{codificacao}{extra}\
X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\n\
Permissions-Policy: camera=(self), geolocation=(), microphone=()\r\nConnection: close\r\n\r\n",
        corpo.len()
    )
    .into_bytes();
    if !cabeca {
        r.extend_from_slice(&corpo);
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn caminhos() {
        assert_eq!(ficheiro_de("/").as_deref(), Some("index.html"));
        assert_eq!(ficheiro_de("/pos?x=1").as_deref(), Some("index.html"));
        assert_eq!(ficheiro_de("/assets/index-Ab12.js").as_deref(), Some("assets/index-Ab12.js"));
        assert_eq!(ficheiro_de("/manifest.webmanifest").as_deref(), Some("manifest.webmanifest"));
        assert!(ficheiro_de("/../segredo.txt").is_none());
        assert!(ficheiro_de("/assets/..%2f..%2fx.js").is_none());
        assert!(ficheiro_de("/a\\b.js").is_none());
        assert_eq!(tipo_por_extensao("sw.js"), Some("text/javascript; charset=utf-8"));
        assert_eq!(tipo_por_extensao("manifest.webmanifest"), Some("application/manifest+json; charset=utf-8"));
    }
}

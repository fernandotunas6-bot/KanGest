// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Cliente HTTP/1.1 mínimo (HTTP e HTTPS com o TLS do sistema) para as actualizações e para a
//! Facturação Electrónica da AGT. Síncrono: chamar dentro de `spawn_blocking`.
//!
//! Vai por um proxy HTTP quando `crate::internet` o indica: túnel CONNECT para os endereços https e pedido em forma
//! absoluta para os http.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::error::{AppError, AppResult};

const LIMITE: usize = 600 * 1024 * 1024;

pub struct Resposta {
    pub estado: u16,
    pub corpo: Vec<u8>,
}

impl Resposta {
    pub fn texto(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }
}

struct Url {
    https: bool,
    host: String,
    porta: u16,
    caminho: String,
}

fn partir(url: &str) -> AppResult<Url> {
    let (https, resto) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(AppError::msg(format!("Endereço inválido: {url}")));
    };
    let (autoridade, caminho) = match resto.find('/') {
        Some(i) => (&resto[..i], &resto[i..]),
        None => (resto, "/"),
    };
    let (host, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h, p.parse().unwrap_or(if https { 443 } else { 80 })),
        _ => (autoridade, if https { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(AppError::msg(format!("Endereço inválido: {url}")));
    }
    Ok(Url { https, host: host.to_string(), porta, caminho: caminho.to_string() })
}

trait Fluxo: Read + Write {}
impl<T: Read + Write> Fluxo for T {}

fn tcp(host: &str, porta: u16, espera: Duration, proxy: bool) -> AppResult<TcpStream> {
    let quem = if proxy { format!("ao proxy {host}:{porta}") } else { format!("a {host}") };
    let addr = (host, porta)
        .to_socket_addrs()
        .map_err(|e| AppError::msg(format!("Não foi possível resolver {host}: {e}")))?
        .next()
        .ok_or_else(|| AppError::msg(format!("Não foi possível resolver {host}")))?;
    let t = TcpStream::connect_timeout(&addr, Duration::from_secs(15)).map_err(|e| AppError::msg(format!("Sem ligação {quem}: {e}")))?;
    t.set_read_timeout(Some(espera)).ok();
    t.set_write_timeout(Some(Duration::from_secs(60))).ok();
    Ok(t)
}

/// Pede ao proxy um túnel para o destino (CONNECT). Lê a resposta byte a byte: o que vem depois já é do destino.
fn tunel(t: &mut TcpStream, u: &Url, p: &crate::internet::Proxy) -> AppResult<()> {
    let mut req = format!("CONNECT {h}:{po} HTTP/1.1\r\nHost: {h}:{po}\r\nUser-Agent: KanGest/{v}\r\n", h = u.host, po = u.porta, v = env!("CARGO_PKG_VERSION"));
    if let Some(a) = &p.autorizacao {
        req.push_str(&format!("Proxy-Authorization: {a}\r\n"));
    }
    req.push_str("\r\n");
    t.write_all(req.as_bytes()).map_err(|e| AppError::msg(format!("Envio ao proxy falhou: {e}")))?;
    let mut resp = Vec::with_capacity(256);
    let mut b = [0u8; 1];
    while !resp.ends_with(b"\r\n\r\n") {
        match t.read(&mut b) {
            Ok(1) => resp.push(b[0]),
            Ok(_) => return Err(AppError::msg(format!("O proxy {}:{} fechou a ligação.", p.host, p.porta))),
            Err(e) => return Err(AppError::msg(format!("Resposta do proxy interrompida: {e}"))),
        }
        if resp.len() > 16 * 1024 {
            return Err(AppError::msg("Resposta do proxy inválida."));
        }
    }
    let linha = String::from_utf8_lossy(&resp).lines().next().unwrap_or("").to_string();
    match linha.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()) {
        Some(200) => Ok(()),
        Some(407) => Err(AppError::msg(format!(
            "O proxy {}:{} pede utilizador e senha (407): indique-os em Configurações → Rede → Ligação à Internet.",
            p.host, p.porta
        ))),
        _ => Err(AppError::msg(format!("O proxy {}:{} recusou a ligação a {}: {linha}", p.host, p.porta, u.host))),
    }
}

/// Liga ao destino (directamente ou pelo proxy). Devolve também o proxy quando o pedido tem de ir em forma absoluta
/// (http por proxy), com a autorização a pôr nesse pedido.
fn ligar(u: &Url, espera: Duration) -> AppResult<(Box<dyn Fluxo>, Option<crate::internet::Proxy>)> {
    let proxy = crate::internet::proxy_para(&u.host, u.https);
    let mut t = match &proxy {
        Some(p) => tcp(&p.host, p.porta, espera, true)?,
        None => tcp(&u.host, u.porta, espera, false)?,
    };
    if u.https {
        if let Some(p) = &proxy {
            tunel(&mut t, u, p)?;
        }
        let tls = native_tls::TlsConnector::new().map_err(|e| AppError::msg(format!("TLS: {e}")))?;
        let s = tls.connect(&u.host, t).map_err(|e| AppError::msg(format!("Ligação segura a {} falhou: {e}", u.host)))?;
        Ok((Box::new(s), None))
    } else {
        Ok((Box::new(t), proxy))
    }
}

/// Pedido HTTP. Segue até 5 redireccionamentos (só em GET). `cabecalhos` sem Host/Content-Length.
pub fn pedido(metodo: &str, url: &str, cabecalhos: &[(&str, String)], corpo: Option<&[u8]>, espera: Duration) -> AppResult<Resposta> {
    let mut atual = url.to_string();
    for _ in 0..6 {
        let u = partir(&atual)?;
        let (mut f, proxy_http) = ligar(&u, espera)?;
        let porta_normal = u.porta == if u.https { 443 } else { 80 };
        let host = if porta_normal { u.host.clone() } else { format!("{}:{}", u.host, u.porta) };
        // por um proxy, os pedidos http levam o endereço completo
        let alvo = if proxy_http.is_some() { format!("http://{host}{}", u.caminho) } else { u.caminho.clone() };
        let mut req = format!(
            "{metodo} {alvo} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: KanGest/{}\r\nConnection: close\r\nAccept-Encoding: identity\r\n",
            env!("CARGO_PKG_VERSION")
        );
        if let Some(a) = proxy_http.as_ref().and_then(|p| p.autorizacao.as_ref()) {
            req.push_str(&format!("Proxy-Authorization: {a}\r\n"));
        }
        for (k, v) in cabecalhos {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        if let Some(c) = corpo {
            req.push_str(&format!("Content-Length: {}\r\n", c.len()));
        }
        req.push_str("\r\n");
        f.write_all(req.as_bytes()).map_err(|e| AppError::msg(format!("Envio falhou: {e}")))?;
        if let Some(c) = corpo {
            f.write_all(c).map_err(|e| AppError::msg(format!("Envio falhou: {e}")))?;
        }
        f.flush().ok();
        let mut bruto = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    bruto.extend_from_slice(&buf[..n]);
                    if bruto.len() > LIMITE {
                        return Err(AppError::msg("Resposta demasiado grande."));
                    }
                    if completa(&bruto, metodo == "HEAD") {
                        break; // não depende de o servidor fechar a ligação
                    }
                }
                // Alguns servidores TLS fecham sem close_notify: aceita o que já chegou.
                Err(e) if !bruto.is_empty() && matches!(e.kind(), std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionAborted) => break,
                Err(e) => return Err(AppError::msg(format!("Leitura falhou: {e}"))),
            }
        }
        let fim = bruto.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| AppError::msg("Resposta HTTP inválida."))?;
        let cab = String::from_utf8_lossy(&bruto[..fim]).into_owned();
        let mut corpo_r = bruto[fim + 4..].to_vec();
        let mut linhas = cab.split("\r\n");
        let estado: u16 = linhas.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).ok_or_else(|| AppError::msg("Resposta HTTP inválida."))?;
        let mut chunked = false;
        let mut tamanho: Option<usize> = None;
        let mut local: Option<String> = None;
        for l in linhas {
            if let Some((k, v)) = l.split_once(':') {
                let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
                match k.as_str() {
                    "transfer-encoding" if v.to_ascii_lowercase().contains("chunked") => chunked = true,
                    "content-length" => tamanho = v.parse().ok(),
                    "location" => local = Some(v.to_string()),
                    _ => {}
                }
            }
        }
        if matches!(estado, 301 | 302 | 303 | 307 | 308) && metodo == "GET" {
            if let Some(l) = local {
                atual = if l.starts_with("http") { l } else { format!("{}://{}:{}{}", if u.https { "https" } else { "http" }, u.host, u.porta, l) };
                continue;
            }
        }
        if chunked {
            corpo_r = desfazer_chunked(&corpo_r)?;
        } else if let Some(t) = tamanho {
            corpo_r.truncate(t);
        }
        return Ok(Resposta { estado, corpo: corpo_r });
    }
    Err(AppError::msg("Demasiados redireccionamentos."))
}

/// A resposta já chegou toda? (cabeçalhos + Content-Length, ou o bloco final de "chunked").
fn completa(b: &[u8], head: bool) -> bool {
    let Some(fim) = b.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
    let cab = String::from_utf8_lossy(&b[..fim]).to_ascii_lowercase();
    let corpo = &b[fim + 4..];
    let estado: u16 = cab.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    if head || estado == 204 || estado == 304 || (100..200).contains(&estado) {
        return true;
    }
    for l in cab.split("\r\n") {
        if let Some(v) = l.strip_prefix("content-length:") {
            return v.trim().parse::<usize>().map(|n| corpo.len() >= n).unwrap_or(false);
        }
        if l.starts_with("transfer-encoding:") && l.contains("chunked") {
            return corpo.ends_with(b"0\r\n\r\n");
        }
    }
    false
}

/// Junta os blocos de um corpo «chunked». Nunca indexa fora do corpo: também lê os pedidos que chegam à API
/// de integração, vindos de quem quer que seja (e um pânico fecharia a aplicação, que corre com `panic = "abort"`).
pub(crate) fn desfazer_chunked(b: &[u8]) -> AppResult<Vec<u8>> {
    let truncada = || AppError::msg("Resposta HTTP truncada.");
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let resto = b.get(i..).ok_or_else(truncada)?;
        let fim = resto.windows(2).position(|w| w == b"\r\n").ok_or_else(truncada)? + i;
        let linha = String::from_utf8_lossy(&b[i..fim]);
        let n = usize::from_str_radix(linha.split(';').next().unwrap_or("").trim(), 16).map_err(|_| AppError::msg("Resposta HTTP inválida."))?;
        i = fim + 2;
        if n == 0 {
            return Ok(out);
        }
        let fim_dados = i.checked_add(n).filter(|f| *f <= b.len()).ok_or_else(truncada)?;
        out.extend_from_slice(&b[i..fim_dados]);
        i = fim_dados + 2;
    }
}

pub fn get(url: &str) -> AppResult<Resposta> {
    pedido("GET", url, &[], None, Duration::from_secs(120))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Proxy falso: guarda o que recebe e responde com `resposta(pedido)`.
    fn proxy_falso(resposta: fn(&str) -> String) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let escuta = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = escuta.local_addr().unwrap().port();
        let vistos = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let v = vistos.clone();
        std::thread::spawn(move || {
            for c in escuta.incoming().flatten() {
                let mut c = c;
                let mut dados = Vec::new();
                let mut b = [0u8; 1];
                while !dados.ends_with(b"\r\n\r\n") && c.read(&mut b).map(|n| n == 1).unwrap_or(false) {
                    dados.push(b[0]);
                }
                let pedido = String::from_utf8_lossy(&dados).to_string();
                v.lock().unwrap().push(pedido.clone());
                let _ = c.write_all(resposta(&pedido).as_bytes());
            }
        });
        (porta, vistos)
    }

    /// Pedidos pelo proxy: http em forma absoluta com a autorização; https por CONNECT; 407 explicado.
    /// Um só teste porque a configuração do proxy é global.
    #[test]
    fn pedidos_pelo_proxy() {
        let (porta, vistos) = proxy_falso(|p| {
            if p.starts_with("CONNECT") {
                if p.contains("Proxy-Authorization: Basic ") {
                    // túnel aceite e fechado logo: o TLS falha, mas o pedido já passou pelo proxy
                    "HTTP/1.1 200 Connection established\r\n\r\n".into()
                } else {
                    "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic\r\nContent-Length: 0\r\n\r\n".into()
                }
            } else {
                let linha = p.lines().next().unwrap_or("").to_string();
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{linha}", linha.len())
            }
        });
        let proxy = format!("http://127.0.0.1:{porta}");
        crate::internet::definir(crate::internet::Config { proxy: proxy.clone(), utilizador: "ana".into(), do_sistema: false, ..Default::default() });
        // http: o pedido vai ao proxy com o endereço completo e a autorização
        let r = pedido("GET", "http://alvo.exemplo.ao:8080/caminho?x=1", &[], None, Duration::from_secs(5)).unwrap();
        assert_eq!(r.texto(), "GET http://alvo.exemplo.ao:8080/caminho?x=1 HTTP/1.1");
        assert!(vistos.lock().unwrap()[0].contains("Host: alvo.exemplo.ao:8080\r\n") && vistos.lock().unwrap()[0].contains("Proxy-Authorization: Basic YW5hOg=="));
        // https: túnel CONNECT para o destino (o TLS falha porque o proxy falso não liga a lado nenhum)
        let e = pedido("GET", "https://alvo.exemplo.ao/x", &[], None, Duration::from_secs(5)).err().expect("sem TLS do outro lado").to_string();
        assert!(e.contains("Ligação segura a alvo.exemplo.ao"), "{e}");
        assert!(vistos.lock().unwrap()[1].starts_with("CONNECT alvo.exemplo.ao:443 HTTP/1.1"), "{:?}", vistos.lock().unwrap());
        // sem utilizador, o proxy pede autenticação: a mensagem diz onde a pôr
        crate::internet::definir(crate::internet::Config { proxy, do_sistema: false, ..Default::default() });
        let e = pedido("GET", "https://alvo.exemplo.ao/x", &[], None, Duration::from_secs(5)).err().expect("407").to_string();
        assert!(e.contains("407") && e.contains("Ligação à Internet"), "{e}");
        // a rede local nunca vai pelo proxy
        assert!(crate::internet::proxy_para("127.0.0.1", false).is_none());
        crate::internet::definir(crate::internet::Config::default());
    }

    /// Internet verdadeira pelo proxy do sistema (HTTPS_PROXY), como num escritório com proxy. Corre-se à mão:
    ///   KANGEST_URL_TESTE=https://… cargo test --lib internet_pelo_proxy_do_sistema -- --ignored --nocapture
    #[test]
    #[ignore]
    fn internet_pelo_proxy_do_sistema() {
        let url = std::env::var("KANGEST_URL_TESTE").unwrap_or_else(|_| "https://kangest.pages.dev/".into());
        let host = partir(&url).unwrap().host;
        eprintln!("proxy para {host}: {:?}", crate::internet::proxy_para(&host, true));
        let r = pedido("GET", &url, &[], None, Duration::from_secs(20)).unwrap();
        eprintln!("HTTP {} · {} bytes", r.estado, r.corpo.len());
        assert!(r.estado < 500, "o destino respondeu");
    }

    #[test]
    fn url_e_chunked() {
        let u = partir("https://sifphml.minfin.gov.ao/sigt/fe/v1/registarFactura").unwrap();
        assert!(u.https && u.porta == 443 && u.host == "sifphml.minfin.gov.ao" && u.caminho == "/sigt/fe/v1/registarFactura");
        let u = partir("http://192.168.1.5:8080").unwrap();
        assert!(!u.https && u.porta == 8080 && u.caminho == "/");
        assert!(partir("ftp://x").is_err());
        assert_eq!(desfazer_chunked(b"4\r\nWiki\r\n5;x=1\r\npedia\r\n0\r\n\r\n").unwrap(), b"Wikipedia");
        // corpos malformados dão erro, nunca pânico
        for mau in [&b"5\r\n0\r\n\r\n"[..], b"3\r\nabc", b"ffffffffffffffff\r\nx\r\n0\r\n\r\n", b"zz\r\n0\r\n\r\n", b"", b"\r\n", b"2\r\nab"] {
            assert!(desfazer_chunked(mau).is_err(), "{:?}", String::from_utf8_lossy(mau));
        }
    }

    /// Servidor que não fecha a ligação (keep-alive): a leitura termina pelo Content-Length.
    #[test]
    fn sem_fechar_ligacao() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).unwrap();
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nok").unwrap();
            std::thread::sleep(Duration::from_secs(30)); // mantém a ligação aberta
        });
        let t = std::time::Instant::now();
        let r = pedido("GET", &format!("http://127.0.0.1:{porta}/"), &[], None, Duration::from_secs(20)).unwrap();
        assert_eq!(r.texto(), "ok");
        assert!(t.elapsed() < Duration::from_secs(5), "não espera pelo fecho da ligação");
        assert!(completa(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n", false));
        assert!(!completa(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nab", false));
    }

    /// Servidor HTTP local: GET com redireccionamento e corpo chunked; POST com corpo.
    #[test]
    fn pedido_local() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for (k, s) in l.incoming().enumerate().take(3) {
                let mut s = s.unwrap();
                let mut buf = [0u8; 4096];
                let mut req = String::new();
                // lê até ao fim dos cabeçalhos e, no POST, até ao corpo
                while !(req.contains("\r\n\r\n") && (k < 2 || req.ends_with('}'))) {
                    let n = s.read(&mut buf).unwrap();
                    req.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
                let resp = match k {
                    0 => "HTTP/1.1 302 Found\r\nLocation: /novo\r\nContent-Length: 0\r\n\r\n".to_string(),
                    1 => "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n".to_string(),
                    _ => format!("HTTP/1.1 400 Bad\r\nContent-Length: {}\r\n\r\n{}", req.lines().last().unwrap().len(), req.lines().last().unwrap()),
                };
                s.write_all(resp.as_bytes()).unwrap();
            }
        });
        let r = get(&format!("http://127.0.0.1:{porta}/velho")).unwrap();
        assert_eq!((r.estado, r.texto().as_str()), (200, "abc"));
        let r = pedido("POST", &format!("http://127.0.0.1:{porta}/x"), &[("Content-Type", "application/json".into())], Some(b"{\"a\":1}"), Duration::from_secs(5)).unwrap();
        assert_eq!((r.estado, r.texto().as_str()), (400, "{\"a\":1}"));
    }
}

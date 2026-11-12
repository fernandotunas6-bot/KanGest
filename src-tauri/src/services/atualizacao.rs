//! Verificar Atualização (Nb_Atualizacao do KIGEST GC).
//!
//! A origem é um endereço (http/https) ou uma pasta partilhada na rede com o ficheiro
//! `kangest-atualizacao.json`:
//!
//! ```json
//! { "versao": "0.2.0", "data": "2026-12-01", "notas": "…",
//!   "pacotes": { "windows-x86_64": { "ficheiro": "KanGest_0.2.0_x64-setup.exe", "sha256": "…", "assinatura": "…" },
//!                "macos-aarch64":  { "ficheiro": "KanGest_0.2.0_aarch64.dmg", "sha256": "…" } } }
//! ```
//!
//! O pacote é sempre conferido pelo SHA-256 do manifesto. Com uma chave pública configurada
//! (minisign, a mesma do `tauri signer`), a assinatura do pacote passa a ser obrigatória.
//! Antes de instalar é feita uma cópia de segurança da base.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::http;
use crate::state::AppState;

pub const MANIFESTO: &str = "kangest-atualizacao.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub fonte: String,
    #[serde(default)]
    pub chave_publica: String,
    #[serde(default)]
    pub ultima_verificacao: Option<String>,
}

fn ficheiro_cfg(dir: &Path) -> PathBuf {
    dir.join("atualizacao.json")
}

fn ler_cfg(dir: &Path) -> Config {
    std::fs::read_to_string(ficheiro_cfg(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn gravar_cfg(dir: &Path, c: &Config) -> AppResult<()> {
    std::fs::write(ficheiro_cfg(dir), serde_json::to_string_pretty(c)?).map_err(|e| AppError::msg(format!("Não foi possível gravar a configuração: {e}")))
}

pub fn plataforma() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        o => o,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// Compara versões "a.b.c" (partes em falta valem 0; sufixos como "-beta" são ignorados).
pub fn mais_recente(nova: &str, atual: &str) -> bool {
    let p = |v: &str| -> Vec<u64> { v.trim().trim_start_matches('v').split(['.', '-', '+']).take(3).map(|x| x.parse().unwrap_or(0)).collect() };
    let (mut a, mut b) = (p(nova), p(atual));
    a.resize(3, 0);
    b.resize(3, 0);
    a > b
}

fn e_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Endereço do manifesto e base para os pacotes.
fn localizar(fonte: &str) -> (String, String) {
    let f = fonte.trim().trim_end_matches(['/', '\\']);
    if f.to_ascii_lowercase().ends_with(".json") {
        let base = f.rsplit_once(['/', '\\']).map(|x| x.0).unwrap_or("").to_string();
        (f.to_string(), base)
    } else if e_url(f) {
        (format!("{f}/{MANIFESTO}"), f.to_string())
    } else {
        (Path::new(f).join(MANIFESTO).display().to_string(), f.to_string())
    }
}

fn ler_origem(caminho: &str) -> AppResult<Vec<u8>> {
    if e_url(caminho) {
        let r = http::get(caminho)?;
        if r.estado != 200 {
            return Err(AppError::msg(format!("O servidor de actualizações respondeu {} para {caminho}.", r.estado)));
        }
        Ok(r.corpo)
    } else {
        let c = caminho.strip_prefix("file://").unwrap_or(caminho);
        std::fs::read(c).map_err(|e| AppError::msg(format!("Não foi possível ler {c}: {e}")))
    }
}

#[derive(Debug, Deserialize)]
struct Pacote {
    ficheiro: String,
    sha256: String,
    #[serde(default)]
    assinatura: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Manifesto {
    versao: String,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    notas: Option<String>,
    #[serde(default)]
    pacotes: std::collections::HashMap<String, Pacote>,
}

fn obter_manifesto(fonte: &str) -> AppResult<(Manifesto, String)> {
    if fonte.trim().is_empty() {
        return Err(AppError::msg("Indique primeiro a origem das actualizações (endereço ou pasta partilhada)."));
    }
    let (url, base) = localizar(fonte);
    let m: Manifesto = serde_json::from_slice(&ler_origem(&url)?).map_err(|e| AppError::msg(format!("O ficheiro {MANIFESTO} é inválido: {e}")))?;
    Ok((m, base))
}

fn juntar(base: &str, ficheiro: &str) -> String {
    if e_url(ficheiro) || Path::new(ficheiro).is_absolute() {
        ficheiro.to_string()
    } else if e_url(base) {
        format!("{base}/{ficheiro}")
    } else {
        Path::new(base).join(ficheiro).display().to_string()
    }
}

/// Texto minisign: aceita o formato do `tauri signer` (texto em base64) ou o texto directo.
fn texto_minisign(s: &str) -> String {
    use base64::Engine;
    let t = s.trim();
    if t.contains("untrusted comment") || t.starts_with("RW") {
        return t.to_string();
    }
    base64::engine::general_purpose::STANDARD.decode(t).ok().and_then(|b| String::from_utf8(b).ok()).unwrap_or_else(|| t.to_string())
}

pub fn verificar_assinatura(chave_publica: &str, assinatura: &str, dados: &[u8]) -> AppResult<()> {
    let ct = texto_minisign(chave_publica);
    let pk = if ct.contains("untrusted comment") {
        minisign_verify::PublicKey::decode(&ct)
    } else {
        minisign_verify::PublicKey::from_base64(ct.trim())
    }
    .map_err(|e| AppError::msg(format!("Chave pública de actualização inválida: {e}")))?;
    let sig = minisign_verify::Signature::decode(&texto_minisign(assinatura)).map_err(|e| AppError::msg(format!("Assinatura do pacote inválida: {e}")))?;
    pk.verify(dados, &sig, false).map_err(|_| AppError::msg("A assinatura do pacote não confere com a chave pública: actualização recusada."))
}

pub fn sha256_hex(dados: &[u8]) -> String {
    Sha256::digest(dados).iter().map(|b| format!("{b:02x}")).collect()
}

fn pasta(st: &AppState) -> PathBuf {
    st.pasta_dados.join("atualizacoes")
}

// ------------------------------------------------------------------ RPC

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let c = ler_cfg(&st.pasta_dados);
    Ok(json!({
        "versao_atual": env!("CARGO_PKG_VERSION"),
        "plataforma": plataforma(),
        "fonte": c.fonte,
        "chave_publica": !c.chave_publica.trim().is_empty(),
        "ultima_verificacao": c.ultima_verificacao,
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    fonte: String,
    /// `None` mantém a chave; texto vazio apaga-a.
    #[serde(default)]
    chave_publica: Option<String>,
}

pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.atualizacoes")?;
    let mut c = ler_cfg(&st.pasta_dados);
    c.fonte = g.fonte.trim().to_string();
    if let Some(k) = g.chave_publica {
        let k = k.trim().to_string();
        if !k.is_empty() {
            // valida já a chave
            let ct = texto_minisign(&k);
            let ok = if ct.contains("untrusted comment") { minisign_verify::PublicKey::decode(&ct).is_ok() } else { minisign_verify::PublicKey::from_base64(ct.trim()).is_ok() };
            if !ok {
                return Err(AppError::msg("A chave pública não é uma chave minisign válida."));
            }
        }
        c.chave_publica = k;
    }
    gravar_cfg(&st.pasta_dados, &c)?;
    Ok(Value::Null)
}

pub async fn verificar(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let mut c = ler_cfg(&st.pasta_dados);
    let fonte = c.fonte.clone();
    let (m, base) = tokio::task::spawn_blocking(move || obter_manifesto(&fonte)).await.map_err(|e| AppError::msg(e.to_string()))??;
    c.ultima_verificacao = Some(crate::db::agora());
    gravar_cfg(&st.pasta_dados, &c).ok();
    let plat = plataforma();
    let pacote = m.pacotes.get(&plat);
    Ok(json!({
        "versao_atual": env!("CARGO_PKG_VERSION"),
        "versao": m.versao,
        "data": m.data,
        "notas": m.notas,
        "disponivel": mais_recente(&m.versao, env!("CARGO_PKG_VERSION")),
        "pacote": pacote.map(|p| juntar(&base, &p.ficheiro)),
        "plataforma": plat,
    }))
}

pub async fn descarregar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.atualizacoes")?;
    let c = ler_cfg(&st.pasta_dados);
    let destino_dir = pasta(st);
    let (ficheiro, versao) = tokio::task::spawn_blocking(move || -> AppResult<(PathBuf, String)> {
        let (m, base) = obter_manifesto(&c.fonte)?;
        if !mais_recente(&m.versao, env!("CARGO_PKG_VERSION")) {
            return Err(AppError::msg(format!("Já tem a versão mais recente ({}).", env!("CARGO_PKG_VERSION"))));
        }
        let p = m.pacotes.get(&plataforma()).ok_or_else(|| AppError::msg(format!("A versão {} não tem pacote para este sistema ({}).", m.versao, plataforma())))?;
        let dados = ler_origem(&juntar(&base, &p.ficheiro))?;
        if sha256_hex(&dados) != p.sha256.trim().to_ascii_lowercase() {
            return Err(AppError::msg("O pacote descarregado não confere com o SHA-256 do manifesto: actualização recusada."));
        }
        if !c.chave_publica.trim().is_empty() {
            let a = p.assinatura.as_deref().filter(|a| !a.trim().is_empty()).ok_or_else(|| AppError::msg("O pacote não está assinado e esta instalação exige assinatura."))?;
            verificar_assinatura(&c.chave_publica, a, &dados)?;
        }
        std::fs::create_dir_all(&destino_dir).map_err(|e| AppError::msg(e.to_string()))?;
        let nome = Path::new(&p.ficheiro).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "kangest-instalador".into());
        let destino = destino_dir.join(nome);
        std::fs::write(&destino, &dados).map_err(|e| AppError::msg(format!("Não foi possível gravar {}: {e}", destino.display())))?;
        Ok((destino, m.versao))
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    // cópia de segurança antes de instalar (num posto cliente os dados estão no servidor)
    let copia = if crate::rede::config().cliente() {
        None
    } else {
        let c = st.pasta_dados.join("copias").join(format!("kangest-antes-de-{versao}.db"));
        std::fs::create_dir_all(st.pasta_dados.join("copias")).ok();
        super::sistema::copiar(&st.pool, &c).await?;
        Some(c.display().to_string())
    };
    Ok(json!({ "ficheiro": ficheiro.display().to_string(), "versao": versao, "copia": copia }))
}

#[derive(Deserialize)]
pub struct InstalarIn {
    ficheiro: String,
}

/// Abre o instalador descarregado (só ficheiros da pasta de actualizações).
pub async fn instalar(st: &AppState, i: InstalarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.atualizacoes")?;
    let dir = pasta(st).canonicalize().map_err(|_| AppError::msg("Descarregue primeiro a actualização."))?;
    let f = Path::new(&i.ficheiro).canonicalize().map_err(|_| AppError::msg("O instalador já não existe."))?;
    if !f.starts_with(&dir) {
        return Err(AppError::msg("Instalador inválido."));
    }
    // o «start» passa pelo ShellExecute, que pede elevação (UAC) quando o instalador a exige;
    // sem o prefixo «\\?\» do canonicalize, que o cmd não aceita
    #[cfg(target_os = "windows")]
    let r = {
        let mut c = std::process::Command::new("cmd");
        super::impressao::sem_consola(&mut c);
        c.args(["/C", "start", ""]).arg(caminho_windows(&f.to_string_lossy())).spawn()
    };
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&f).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&f).spawn();
    r.map_err(|e| AppError::msg(format!("Não foi possível abrir o instalador: {e}")))?;
    Ok(Value::Null)
}

/// `\\?\C:\x` → `C:\x` e `\\?\UNC\srv\x` → `\\srv\x` (caminhos «verbatim» do canonicalize no Windows).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn caminho_windows(p: &str) -> String {
    if let Some(r) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{r}")
    } else if let Some(r) = p.strip_prefix(r"\\?\") {
        r.to_string()
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn caminhos_verbatim_do_windows() {
        assert_eq!(caminho_windows(r"\\?\C:\Users\Caixa\AppData\KanGest_1.0.1_x64-setup.exe"), r"C:\Users\Caixa\AppData\KanGest_1.0.1_x64-setup.exe");
        assert_eq!(caminho_windows(r"\\?\UNC\SERVIDOR\KanGest\setup.exe"), r"\\SERVIDOR\KanGest\setup.exe");
        assert_eq!(caminho_windows("/Users/x/setup.dmg"), "/Users/x/setup.dmg");
    }

    #[test]
    fn versoes_e_origens() {
        assert!(mais_recente("0.2.0", "0.1.9"));
        assert!(mais_recente("1.0", "0.9.9"));
        assert!(!mais_recente("0.1.0", "0.1.0"));
        assert!(!mais_recente("v0.1.0-beta", "0.1.0"));
        assert_eq!(localizar("https://x.ao/kigest/").0, "https://x.ao/kigest/kangest-atualizacao.json");
        assert_eq!(localizar("https://x.ao/k/m.json"), ("https://x.ao/k/m.json".into(), "https://x.ao/k".into()));
        assert_eq!(juntar("https://x.ao/k", "a.exe"), "https://x.ao/k/a.exe");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Assinatura minisign real (exemplo do próprio minisign-verify).
    #[test]
    fn assinatura_minisign() {
        let pk = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        let sig = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1633700835\tfile:test\tprehashed\nwLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";
        verificar_assinatura(pk, sig, b"test").expect("assinatura válida");
        assert!(verificar_assinatura(pk, sig, b"outro").is_err());
    }

    /// Actualização a partir de uma pasta partilhada: SHA-256 conferido.
    #[test]
    fn manifesto_em_pasta() {
        let dir = std::env::temp_dir().join(format!("kigest-upd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pacote.bin"), b"instalador").unwrap();
        let m = json!({ "versao": "9.9.9", "pacotes": { plataforma(): { "ficheiro": "pacote.bin", "sha256": sha256_hex(b"instalador") } } });
        std::fs::write(dir.join(MANIFESTO), m.to_string()).unwrap();
        let (m, base) = obter_manifesto(dir.to_str().unwrap()).unwrap();
        assert_eq!(m.versao, "9.9.9");
        let p = &m.pacotes[&plataforma()];
        assert_eq!(sha256_hex(&ler_origem(&juntar(&base, &p.ficheiro)).unwrap()), p.sha256);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

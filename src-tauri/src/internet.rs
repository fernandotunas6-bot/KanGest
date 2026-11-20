// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Ligação à Internet desta máquina: proxy e diagnóstico dos serviços externos.
//!
//! Quem sai para a Internet: o servidor de licenças, as actualizações, a Facturação Electrónica da AGT, os avisos
//! automáticos da API (webhooks) e as chaves do Cloudflare Access. Todos passam pelo cliente HTTP (`crate::http`),
//! que pergunta aqui se deve ir por um proxy.
//!
//! Ordem: o proxy escrito em Configurações → Rede → Ligação à Internet (fica em `internet.json` na pasta de dados,
//! como o `rede.json`: é desta máquina, não da empresa; a senha vai cifrada com a chave mestra da máquina); senão, o
//! do sistema: variáveis `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY` (como o curl e o git) e, no Windows, o proxy das
//! Opções da Internet. Endereços da rede local (127.x, 10.x, 172.16–31.x, 192.168.x, nomes sem ponto) nunca vão pelo
//! proxy. Só proxies HTTP (com CONNECT para os endereços https); não há configuração automática (PAC).

use std::net::IpAddr;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// «http://endereço:porta»; vazio = sem proxy próprio desta máquina.
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub utilizador: String,
    /// Cifrada com a chave mestra desta máquina (`crate::segredos`).
    #[serde(default)]
    pub senha: String,
    /// Endereços que não vão pelo proxy, separados por vírgulas (ex.: «.empresa.local, 10.0.0.5»).
    #[serde(default)]
    pub excepcoes: String,
    /// Sem proxy próprio, usar o do sistema.
    #[serde(default = "sim")]
    pub do_sistema: bool,
}

fn sim() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self { proxy: String::new(), utilizador: String::new(), senha: String::new(), excepcoes: String::new(), do_sistema: true }
    }
}

/// Proxy de um pedido: para onde ligar e, se pedir, a autorização.
#[derive(Debug, Clone, PartialEq)]
pub struct Proxy {
    pub host: String,
    pub porta: u16,
    /// Valor do cabeçalho `Proxy-Authorization` («Basic …»).
    pub autorizacao: Option<String>,
    /// De onde veio (para o diagnóstico): «configurado», «HTTPS_PROXY», «Windows»…
    pub origem: String,
}

const FICHEIRO: &str = "internet.json";

fn celula() -> &'static RwLock<Config> {
    static C: OnceLock<RwLock<Config>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(Config::default()))
}

pub fn config() -> Config {
    celula().read().map(|c| c.clone()).unwrap_or_default()
}

/// Lê `internet.json` (no arranque). Sem ficheiro, fica o proxy do sistema.
pub fn carregar(pasta: &Path) -> Config {
    let c = std::fs::read_to_string(pasta.join(FICHEIRO)).ok().and_then(|s| serde_json::from_str::<Config>(&s).ok()).unwrap_or_default();
    definir(c.clone());
    c
}

pub(crate) fn definir(c: Config) {
    if let Ok(mut g) = celula().write() {
        *g = c;
    }
}

pub(crate) fn gravar(pasta: &Path, c: &Config) -> AppResult<()> {
    std::fs::write(pasta.join(FICHEIRO), serde_json::to_string_pretty(c)?).map_err(|e| AppError::msg(format!("Não foi possível gravar {FICHEIRO}: {e}")))?;
    definir(c.clone());
    Ok(())
}

/// «http://utilizador:senha@endereço:porta», «endereço:porta» ou «endereço» (porta 8080) → (endereço, porta, utilizador, senha).
pub fn partir_proxy(texto: &str) -> Result<(String, u16, Option<String>, Option<String>), String> {
    let t = texto.trim().trim_end_matches('/');
    let resto = if let Some(r) = t.strip_prefix("http://") {
        r
    } else if t.contains("://") {
        return Err("Só são aceites proxies HTTP (http://endereço:porta).".into());
    } else {
        t
    };
    let (credenciais, autoridade) = match resto.rsplit_once('@') {
        Some((c, a)) => (Some(c), a),
        None => (None, resto),
    };
    let autoridade = autoridade.split('/').next().unwrap_or("");
    let (host, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && !h.ends_with(':') => (h, p.parse::<u16>().map_err(|_| format!("Porta do proxy inválida: {p}"))?),
        _ => (autoridade, 8080),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host.contains(char::is_whitespace) || porta == 0 {
        return Err("Indique o proxy como http://endereço:porta.".into());
    }
    let (u, s) = match credenciais.map(|c| c.split_once(':').unwrap_or((c, ""))) {
        Some((u, s)) => (Some(u.to_string()).filter(|x| !x.is_empty()), Some(s.to_string()).filter(|x| !x.is_empty())),
        None => (None, None),
    };
    Ok((host.to_string(), porta, u, s))
}

fn basic(utilizador: Option<&str>, senha: Option<&str>) -> Option<String> {
    let u = utilizador.filter(|u| !u.is_empty())?;
    Some(format!("Basic {}", B64.encode(format!("{u}:{}", senha.unwrap_or("")))))
}

/// Endereços que ficam sempre fora do proxy: esta máquina e a rede local.
pub fn local(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") || !h.contains(['.', ':']) {
        return true;
    }
    match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80,
        Err(_) => false,
    }
}

/// Uma lista de excepções (vírgulas, pontos e vírgulas ou espaços) apanha o endereço? Aceita «*», nomes («empresa.ao»
/// apanha também «api.empresa.ao»), «.sufixo», «*.sufixo», IPs e redes IPv4 («10.0.0.0/8»). «<local>» (Windows) são
/// os nomes sem ponto, que já ficam sempre fora.
pub fn excepcao(host: &str, lista: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    lista.split([',', ';', ' ']).map(|e| e.trim().to_ascii_lowercase()).filter(|e| !e.is_empty()).any(|e| {
        if e == "*" {
            return true;
        }
        if let Some((rede, bits)) = e.split_once('/') {
            if let (Ok(IpAddr::V4(r)), Ok(IpAddr::V4(ip)), Ok(b)) = (rede.parse::<IpAddr>(), h.parse::<IpAddr>(), bits.parse::<u32>()) {
                let mascara = if b == 0 { 0 } else { u32::MAX << 32u32.saturating_sub(b.min(32)) };
                return u32::from(r) & mascara == u32::from(ip) & mascara;
            }
            return false;
        }
        let e = e.trim_start_matches("*.").trim_start_matches('.');
        !e.is_empty() && (h == e || h.ends_with(&format!(".{e}")))
    })
}

/// Proxy do Windows (Opções da Internet), lido uma vez: (proxy http, proxy https, excepções).
#[cfg(target_os = "windows")]
fn proxy_windows() -> &'static Option<(String, String, String)> {
    static P: OnceLock<Option<(String, String, String)>> = OnceLock::new();
    P.get_or_init(|| {
        let mut c = std::process::Command::new("reg");
        c.args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings"]);
        crate::services::impressao::sem_consola(&mut c);
        let saida = String::from_utf8_lossy(&c.output().ok()?.stdout).to_string();
        proxy_do_registo(&saida)
    })
}

/// Interpreta a saída de `reg query …\Internet Settings`: ProxyEnable, ProxyServer («h:p» ou «http=h:p;https=h:p»)
/// e ProxyOverride.
pub fn proxy_do_registo(saida: &str) -> Option<(String, String, String)> {
    let valor = |nome: &str| {
        saida.lines().find_map(|l| {
            let mut p = l.split_whitespace();
            (p.next()?.eq_ignore_ascii_case(nome)).then(|| p.skip(1).collect::<Vec<_>>().join(" "))
        })
    };
    let ligado = valor("ProxyEnable").is_some_and(|v| v.trim() == "0x1");
    let servidor = valor("ProxyServer").filter(|s| !s.trim().is_empty())?;
    if !ligado {
        return None;
    }
    let (mut http, mut https) = (String::new(), String::new());
    if servidor.contains('=') {
        for parte in servidor.split(';') {
            match parte.split_once('=') {
                Some((k, v)) if k.trim().eq_ignore_ascii_case("http") => http = v.trim().to_string(),
                Some((k, v)) if k.trim().eq_ignore_ascii_case("https") => https = v.trim().to_string(),
                _ => {}
            }
        }
        if https.is_empty() {
            https = http.clone();
        }
    } else {
        http = servidor.trim().to_string();
        https = http.clone();
    }
    Some((http, https, valor("ProxyOverride").unwrap_or_default()))
}

fn variavel(nomes: &[&str]) -> Option<String> {
    nomes.iter().find_map(|n| std::env::var(n).ok()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Proxy para um pedido a `host` (None = ligação directa).
pub fn proxy_para(host: &str, https: bool) -> Option<Proxy> {
    proxy_com(&config(), host, https)
}

pub(crate) fn proxy_com(c: &Config, host: &str, https: bool) -> Option<Proxy> {
    if local(host) {
        return None;
    }
    if !c.proxy.trim().is_empty() {
        if excepcao(host, &c.excepcoes) {
            return None;
        }
        let (h, porta, u, s) = partir_proxy(&c.proxy).ok()?;
        let senha = (!c.senha.is_empty()).then(|| crate::segredos::decifrar(&c.senha).unwrap_or_default());
        let autorizacao = basic(Some(c.utilizador.as_str()).filter(|x| !x.is_empty()).or(u.as_deref()), senha.as_deref().or(s.as_deref()));
        return Some(Proxy { host: h, porta, autorizacao, origem: "configurado".into() });
    }
    if !c.do_sistema {
        return None;
    }
    let (nome, valor) = if https {
        ("HTTPS_PROXY", variavel(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]))
    } else {
        ("HTTP_PROXY", variavel(&["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]))
    };
    if let Some(v) = valor {
        if excepcao(host, &variavel(&["NO_PROXY", "no_proxy"]).unwrap_or_default()) {
            return None;
        }
        let (h, porta, u, s) = partir_proxy(&v).ok()?;
        return Some(Proxy { host: h, porta, autorizacao: basic(u.as_deref(), s.as_deref()), origem: nome.into() });
    }
    #[cfg(target_os = "windows")]
    if let Some((http, https_p, excepcoes)) = proxy_windows() {
        if excepcao(host, excepcoes) {
            return None;
        }
        let (h, porta, _, _) = partir_proxy(if https { https_p } else { http }).ok()?;
        return Some(Proxy { host: h, porta, autorizacao: None, origem: "Windows".into() });
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn enderecos_do_proxy() {
        assert_eq!(partir_proxy("http://10.0.0.1:3128").unwrap(), ("10.0.0.1".into(), 3128, None, None));
        assert_eq!(partir_proxy("proxy.empresa.ao:8080/").unwrap(), ("proxy.empresa.ao".into(), 8080, None, None));
        assert_eq!(partir_proxy("proxy.empresa.ao").unwrap().1, 8080);
        assert_eq!(partir_proxy("http://ana:s3gr3d@proxy:3128").unwrap(), ("proxy".into(), 3128, Some("ana".into()), Some("s3gr3d".into())));
        assert!(partir_proxy("socks5://proxy:1080").unwrap_err().contains("HTTP"));
        assert!(partir_proxy("http://proxy:porta").is_err());
        assert!(partir_proxy("  ").is_err());
    }

    #[test]
    fn rede_local_e_excepcoes() {
        for h in ["localhost", "127.0.0.1", "192.168.1.10", "10.2.3.4", "172.20.0.1", "servidor", "caixa.local", "[::1]", "fe80::1"] {
            assert!(local(h), "{h} é local");
        }
        for h in ["kangest.pages.dev", "8.8.8.8", "sifp.minfin.gov.ao", "172.32.0.1"] {
            assert!(!local(h), "{h} não é local");
        }
        let lista = "empresa.ao; .interno.ao, *.loja.ao 41.63.0.0/16 <local>";
        assert!(excepcao("empresa.ao", lista) && excepcao("api.empresa.ao", lista) && excepcao("x.interno.ao", lista) && excepcao("pos.loja.ao", lista));
        assert!(excepcao("41.63.10.20", lista) && !excepcao("41.64.10.20", lista));
        assert!(!excepcao("outraempresa.ao", lista) && !excepcao("kangest.pages.dev", lista));
        assert!(excepcao("qualquer.coisa", "*"));
    }

    #[test]
    fn proxy_configurado_ganha_ao_do_sistema() {
        let c = Config { proxy: "http://proxy.empresa.ao:3128".into(), utilizador: "ana".into(), excepcoes: ".minfin.gov.ao".into(), ..Default::default() };
        let p = proxy_com(&c, "kangest.pages.dev", true).unwrap();
        assert_eq!((p.host.as_str(), p.porta, p.origem.as_str()), ("proxy.empresa.ao", 3128, "configurado"));
        assert_eq!(p.autorizacao.as_deref(), Some("Basic YW5hOg=="), "utilizador sem senha");
        assert!(proxy_com(&c, "sifp.minfin.gov.ao", true).is_none(), "excepção");
        assert!(proxy_com(&c, "192.168.1.10", false).is_none(), "rede local nunca vai pelo proxy");
        // sem proxy próprio e sem o do sistema: ligação directa
        assert!(proxy_com(&Config { do_sistema: false, ..Default::default() }, "kangest.pages.dev", true).is_none());
    }

    #[test]
    fn proxy_do_windows() {
        let saida = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    http=10.0.0.1:3128;https=10.0.0.1:3129\r\n    ProxyOverride    REG_SZ    *.empresa.ao;<local>\r\n";
        assert_eq!(proxy_do_registo(saida), Some(("10.0.0.1:3128".into(), "10.0.0.1:3129".into(), "*.empresa.ao;<local>".into())));
        let simples = "    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    proxy:8080\r\n";
        assert_eq!(proxy_do_registo(simples), Some(("proxy:8080".into(), "proxy:8080".into(), String::new())));
        assert_eq!(proxy_do_registo("    ProxyEnable    REG_DWORD    0x0\r\n    ProxyServer    REG_SZ    proxy:8080\r\n"), None, "desligado");
    }
}

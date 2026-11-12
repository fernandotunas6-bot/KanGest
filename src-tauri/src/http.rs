//! Cliente HTTP/1.1 mínimo (HTTP e HTTPS com o TLS do sistema) para as actualizações e para a
//! Facturação Electrónica da AGT. Síncrono: chamar dentro de `spawn_blocking`.

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

fn ligar(u: &Url, espera: Duration) -> AppResult<Box<dyn Fluxo>> {
    let addr = (u.host.as_str(), u.porta)
        .to_socket_addrs()
        .map_err(|e| AppError::msg(format!("Não foi possível resolver {}: {e}", u.host)))?
        .next()
        .ok_or_else(|| AppError::msg(format!("Não foi possível resolver {}", u.host)))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(15)).map_err(|e| AppError::msg(format!("Sem ligação a {}: {e}", u.host)))?;
    tcp.set_read_timeout(Some(espera)).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(60))).ok();
    if u.https {
        let tls = native_tls::TlsConnector::new().map_err(|e| AppError::msg(format!("TLS: {e}")))?;
        let s = tls.connect(&u.host, tcp).map_err(|e| AppError::msg(format!("Ligação segura a {} falhou: {e}", u.host)))?;
        Ok(Box::new(s))
    } else {
        Ok(Box::new(tcp))
    }
}

/// Pedido HTTP. Segue até 5 redireccionamentos (só em GET). `cabecalhos` sem Host/Content-Length.
pub fn pedido(metodo: &str, url: &str, cabecalhos: &[(&str, String)], corpo: Option<&[u8]>, espera: Duration) -> AppResult<Resposta> {
    let mut atual = url.to_string();
    for _ in 0..6 {
        let u = partir(&atual)?;
        let mut f = ligar(&u, espera)?;
        let mut req = format!(
            "{metodo} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: KanGest/{}\r\nConnection: close\r\nAccept-Encoding: identity\r\n",
            u.caminho,
            u.host,
            env!("CARGO_PKG_VERSION")
        );
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

fn desfazer_chunked(b: &[u8]) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let fim = b[i..].windows(2).position(|w| w == b"\r\n").ok_or_else(|| AppError::msg("Resposta HTTP truncada."))? + i;
        let linha = String::from_utf8_lossy(&b[i..fim]);
        let n = usize::from_str_radix(linha.split(';').next().unwrap_or("").trim(), 16).map_err(|_| AppError::msg("Resposta HTTP inválida."))?;
        i = fim + 2;
        if n == 0 {
            return Ok(out);
        }
        if i + n > b.len() {
            return Err(AppError::msg("Resposta HTTP truncada."));
        }
        out.extend_from_slice(&b[i..i + n]);
        i += n + 2;
    }
}

pub fn get(url: &str) -> AppResult<Resposta> {
    pedido("GET", url, &[], None, Duration::from_secs(120))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn url_e_chunked() {
        let u = partir("https://sifphml.minfin.gov.ao/sigt/fe/v1/registarFactura").unwrap();
        assert!(u.https && u.porta == 443 && u.host == "sifphml.minfin.gov.ao" && u.caminho == "/sigt/fe/v1/registarFactura");
        let u = partir("http://192.168.1.5:8080").unwrap();
        assert!(!u.https && u.porta == 8080 && u.caminho == "/");
        assert!(partir("ftp://x").is_err());
        assert_eq!(desfazer_chunked(b"4\r\nWiki\r\n5;x=1\r\npedia\r\n0\r\n\r\n").unwrap(), b"Wikipedia");
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

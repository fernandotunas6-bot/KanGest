//! Cliente SMTP mínimo (documentos por email) com o TLS do sistema: SSL/TLS directo (porta 465),
//! STARTTLS (587) ou sem cifra (só para servidores da rede local). Autenticação PLAIN ou LOGIN.
//! Mensagem MIME em UTF-8 com texto simples e HTML. Síncrono: chamar dentro de `spawn_blocking`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Seguranca {
    /// TLS desde o início (porta 465)
    #[default]
    Ssl,
    /// liga em claro e passa a TLS com STARTTLS (porta 587)
    Starttls,
    /// sem cifra: só servidores da rede local
    Nenhuma,
}

pub struct Servidor {
    pub servidor: String,
    pub porta: u16,
    pub seguranca: Seguranca,
    pub utilizador: String,
    pub senha: String,
    pub remetente: String,
    pub nome: String,
}

pub struct Mensagem {
    pub para: Vec<String>,
    /// cópia oculta (arquivo da empresa)
    pub bcc: Vec<String>,
    pub assunto: String,
    pub texto: String,
    pub html: Option<String>,
}

/// Endereço de email simples (sem nome), sem espaços nem quebras de linha (evita injectar cabeçalhos).
pub fn email_valido(e: &str) -> bool {
    let e = e.trim();
    let Some((local, dominio)) = e.split_once('@') else { return false };
    !local.is_empty()
        && dominio.contains('.')
        && !dominio.starts_with('.')
        && !dominio.ends_with('.')
        && e.len() <= 254
        && e.chars().all(|c| c.is_ascii_graphic() && !matches!(c, '<' | '>' | '(' | ')' | ',' | ';' | ':' | '"' | '[' | ']' | '\\'))
        && e.matches('@').count() == 1
}

/// Lista «a@x.ao; b@y.ao» → endereços validados.
pub fn enderecos(lista: &str) -> AppResult<Vec<String>> {
    let v: Vec<String> = lista.split([',', ';', ' ']).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect();
    if let Some(mau) = v.iter().find(|e| !email_valido(e)) {
        return Err(AppError::msg(format!("Endereço de email inválido: {mau}")));
    }
    Ok(v)
}

/// Cabeçalho em UTF-8 (RFC 2047) quando não é ASCII simples.
fn cabecalho_utf8(s: &str) -> String {
    let limpo: String = s.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    if limpo.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        limpo
    } else {
        format!("=?UTF-8?B?{}?=", B64.encode(limpo.as_bytes()))
    }
}

/// Base64 em linhas de 76 caracteres (corpo MIME).
fn base64_linhas(b: &[u8]) -> String {
    let t = B64.encode(b);
    let mut s = String::with_capacity(t.len() + t.len() / 76 * 2 + 2);
    for (i, c) in t.chars().enumerate() {
        if i > 0 && i % 76 == 0 {
            s.push_str("\r\n");
        }
        s.push(c);
    }
    s
}

/// Mensagem MIME completa (cabeçalhos + corpo), com CRLF.
pub fn compor(srv: &Servidor, m: &Mensagem) -> String {
    let fronteira = format!("kangest-{:x}", rand::random::<u64>());
    let dominio = srv.remetente.split_once('@').map(|x| x.1).unwrap_or("kangest.local");
    let mut s = String::new();
    s.push_str(&format!("From: {} <{}>\r\n", cabecalho_utf8(&srv.nome), srv.remetente));
    s.push_str(&format!("To: {}\r\n", m.para.join(", ")));
    s.push_str(&format!("Subject: {}\r\n", cabecalho_utf8(&m.assunto)));
    s.push_str(&format!("Date: {}\r\n", chrono::Local::now().to_rfc2822()));
    s.push_str(&format!("Message-ID: <{}.{:x}@{dominio}>\r\n", chrono::Local::now().timestamp(), rand::random::<u32>()));
    s.push_str("MIME-Version: 1.0\r\n");
    s.push_str("X-Mailer: KanGest\r\n");
    match &m.html {
        Some(html) => {
            s.push_str(&format!("Content-Type: multipart/alternative; boundary=\"{fronteira}\"\r\n\r\n"));
            for (tipo, corpo) in [("text/plain", m.texto.as_str()), ("text/html", html.as_str())] {
                s.push_str(&format!("--{fronteira}\r\nContent-Type: {tipo}; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"));
                s.push_str(&base64_linhas(corpo.as_bytes()));
                s.push_str("\r\n");
            }
            s.push_str(&format!("--{fronteira}--\r\n"));
        }
        None => {
            s.push_str("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
            s.push_str(&base64_linhas(m.texto.as_bytes()));
            s.push_str("\r\n");
        }
    }
    s
}

trait Fluxo: Read + Write {}
impl<T: Read + Write> Fluxo for T {}

struct Sessao {
    r: BufReader<Box<dyn Fluxo>>,
    servidor: String,
}

impl Sessao {
    /// Lê uma resposta (várias linhas «250-…» até «250 …»): código e texto.
    fn resposta(&mut self) -> AppResult<(u16, String)> {
        let mut texto = String::new();
        loop {
            let mut l = String::new();
            let n = self.r.read_line(&mut l).map_err(|e| AppError::msg(format!("O servidor de email {} não respondeu: {e}", self.servidor)))?;
            if n == 0 {
                return Err(AppError::msg(format!("O servidor de email {} fechou a ligação.", self.servidor)));
            }
            let l = l.trim_end();
            let codigo: u16 = l.get(..3).and_then(|c| c.parse().ok()).ok_or_else(|| AppError::msg(format!("Resposta inválida do servidor de email: {l}")))?;
            texto.push_str(l.get(4..).unwrap_or(""));
            texto.push('\n');
            if l.as_bytes().get(3) != Some(&b'-') {
                return Ok((codigo, texto.trim_end().to_string()));
            }
        }
    }

    fn enviar(&mut self, linha: &str) -> AppResult<()> {
        let f = self.r.get_mut();
        f.write_all(linha.as_bytes()).and_then(|_| f.write_all(b"\r\n")).and_then(|_| f.flush()).map_err(|e| AppError::msg(format!("Falha ao enviar para o servidor de email: {e}")))
    }

    /// Envia um comando e exige um dos códigos esperados.
    fn comando(&mut self, linha: &str, esperado: &[u16], o_que: &str) -> AppResult<String> {
        self.enviar(linha)?;
        let (c, t) = self.resposta()?;
        if esperado.contains(&c) {
            Ok(t)
        } else {
            Err(AppError::msg(format!("{o_que}: o servidor de email respondeu {c} {t}")))
        }
    }
}

fn ligar_tls(servidor: &str, tcp: TcpStream) -> AppResult<Box<dyn Fluxo>> {
    let tls = native_tls::TlsConnector::new().map_err(|e| AppError::msg(format!("TLS: {e}")))?;
    let s = tls.connect(servidor, tcp).map_err(|e| AppError::msg(format!("Ligação segura ao servidor de email {servidor} falhou: {e}")))?;
    Ok(Box::new(s))
}

/// Envia a mensagem. Devolve a resposta final do servidor (identificador da mensagem na fila).
pub fn enviar(srv: &Servidor, m: &Mensagem) -> AppResult<String> {
    if m.para.is_empty() {
        return Err(AppError::msg("Indique o endereço de email do destinatário."));
    }
    if !email_valido(&srv.remetente) {
        return Err(AppError::msg("Configure o email do remetente (Configurações › Email)."));
    }
    let addr = (srv.servidor.as_str(), srv.porta)
        .to_socket_addrs()
        .map_err(|e| AppError::msg(format!("Não foi possível encontrar o servidor de email {}: {e}", srv.servidor)))?
        .next()
        .ok_or_else(|| AppError::msg(format!("Não foi possível encontrar o servidor de email {}", srv.servidor)))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(15)).map_err(|e| AppError::msg(format!("Sem ligação ao servidor de email {}:{}: {e}", srv.servidor, srv.porta)))?;
    tcp.set_read_timeout(Some(Duration::from_secs(30))).ok();
    tcp.set_write_timeout(Some(Duration::from_secs(60))).ok();
    let fluxo: Box<dyn Fluxo> = if srv.seguranca == Seguranca::Ssl { ligar_tls(&srv.servidor, tcp.try_clone().map_err(|e| AppError::msg(e.to_string()))?)? } else { Box::new(tcp.try_clone().map_err(|e| AppError::msg(e.to_string()))?) };
    let mut s = Sessao { r: BufReader::new(fluxo), servidor: srv.servidor.clone() };
    let (c, t) = s.resposta()?;
    if c != 220 {
        return Err(AppError::msg(format!("O servidor de email recusou a ligação: {c} {t}")));
    }
    let mut capacidades = s.comando("EHLO kangest.local", &[250], "Apresentação (EHLO)")?;
    if srv.seguranca == Seguranca::Starttls {
        if !capacidades.lines().any(|l| l.trim().eq_ignore_ascii_case("STARTTLS")) {
            return Err(AppError::msg("O servidor de email não aceita STARTTLS: escolha SSL/TLS (porta 465)."));
        }
        s.comando("STARTTLS", &[220], "Início da ligação segura (STARTTLS)")?;
        // o resto da conversa passa pela ligação cifrada (o que ficou no buffer em claro descarta-se)
        drop(s);
        let fluxo = ligar_tls(&srv.servidor, tcp)?;
        s = Sessao { r: BufReader::new(fluxo), servidor: srv.servidor.clone() };
        capacidades = s.comando("EHLO kangest.local", &[250], "Apresentação (EHLO)")?;
    }
    if !srv.utilizador.is_empty() {
        let auth = capacidades.lines().find(|l| l.to_ascii_uppercase().starts_with("AUTH")).unwrap_or("").to_ascii_uppercase();
        if auth.split_whitespace().any(|x| x == "PLAIN") || !auth.split_whitespace().any(|x| x == "LOGIN") {
            let credenciais = B64.encode(format!("\0{}\0{}", srv.utilizador, srv.senha));
            s.comando(&format!("AUTH PLAIN {credenciais}"), &[235], "Autenticação").map_err(credenciais_erradas)?;
        } else {
            s.comando("AUTH LOGIN", &[334], "Autenticação")?;
            s.comando(&B64.encode(&srv.utilizador), &[334], "Autenticação").map_err(credenciais_erradas)?;
            s.comando(&B64.encode(&srv.senha), &[235], "Autenticação").map_err(credenciais_erradas)?;
        }
    }
    s.comando(&format!("MAIL FROM:<{}>", srv.remetente), &[250], "Remetente")?;
    for d in m.para.iter().chain(m.bcc.iter()) {
        s.comando(&format!("RCPT TO:<{d}>"), &[250, 251], &format!("Destinatário {d}"))?;
    }
    s.comando("DATA", &[354], "Envio da mensagem")?;
    let mut corpo = String::new();
    for l in compor(srv, m).split("\r\n") {
        if l.starts_with('.') {
            corpo.push('.');
        }
        corpo.push_str(l);
        corpo.push_str("\r\n");
    }
    // `compor` termina em CRLF: o split deixa uma linha vazia no fim, que fica antes do ponto final
    let corpo = corpo.trim_end_matches("\r\n").to_string() + "\r\n.";
    let final_ = s.comando(&corpo, &[250], "Envio da mensagem")?;
    let _ = s.enviar("QUIT");
    Ok(final_)
}

fn credenciais_erradas(e: AppError) -> AppError {
    AppError::msg(format!("{e}. Confirme o utilizador e a senha (no Gmail e no Outlook use uma «senha de aplicação»)."))
}

#[cfg(test)]
pub mod testes {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Servidor SMTP falso (sem TLS): conversa normal, AUTH PLAIN, e devolve o que recebeu no DATA.
    pub fn servidor_falso() -> (u16, mpsc::Receiver<(Vec<String>, String)>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(c) = c else { continue };
                let mut w = c.try_clone().unwrap();
                let mut r = BufReader::new(c);
                let mut comandos = Vec::new();
                let mut escrever = |t: &str| {
                    w.write_all(t.as_bytes()).unwrap();
                };
                escrever("220 falso ESMTP\r\n");
                let mut dados = String::new();
                let mut em_dados = false;
                loop {
                    let mut linha = String::new();
                    if r.read_line(&mut linha).unwrap_or(0) == 0 {
                        break;
                    }
                    if em_dados {
                        if linha == ".\r\n" {
                            em_dados = false;
                            escrever("250 2.0.0 Ok: queued as ABC123\r\n");
                        } else {
                            dados.push_str(&linha);
                        }
                        continue;
                    }
                    let cmd = linha.trim_end().to_string();
                    comandos.push(cmd.clone());
                    match cmd.split_whitespace().next().unwrap_or("").to_ascii_uppercase().as_str() {
                        "EHLO" => escrever("250-falso\r\n250-AUTH PLAIN LOGIN\r\n250 8BITMIME\r\n"),
                        "AUTH" => {
                            let ok = cmd.ends_with(&B64.encode("\0loja@kianda.ao\0segredo"));
                            escrever(if ok { "235 2.7.0 Authentication successful\r\n" } else { "535 5.7.8 Authentication failed\r\n" });
                        }
                        "MAIL" | "RCPT" => escrever("250 Ok\r\n"),
                        "DATA" => {
                            em_dados = true;
                            escrever("354 End data with <CR><LF>.<CR><LF>\r\n");
                        }
                        "QUIT" => {
                            escrever("221 Bye\r\n");
                            break;
                        }
                        _ => escrever("502 Command not implemented\r\n"),
                    }
                }
                let _ = tx.send((comandos, dados));
            }
        });
        (porta, rx)
    }

    fn srv(porta: u16, senha: &str) -> Servidor {
        Servidor {
            servidor: "127.0.0.1".into(),
            porta,
            seguranca: Seguranca::Nenhuma,
            utilizador: "loja@kianda.ao".into(),
            senha: senha.into(),
            remetente: "loja@kianda.ao".into(),
            nome: "Comércio Kianda".into(),
        }
    }

    #[test]
    fn envia_mensagem_mime_com_autenticacao() {
        let (porta, rx) = servidor_falso();
        let m = Mensagem {
            para: vec!["cliente@exemplo.ao".into()],
            bcc: vec!["arquivo@kianda.ao".into()],
            assunto: "Factura/Recibo FR 2026/9 — Comércio Kianda".into(),
            texto: "Olá\n.linha que começa por ponto".into(),
            html: Some("<p>Olá</p>".into()),
        };
        let r = enviar(&srv(porta, "segredo"), &m).expect("enviar");
        assert!(r.contains("queued"), "{r}");
        let (comandos, dados) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(comandos.iter().any(|c| c == "RCPT TO:<cliente@exemplo.ao>") && comandos.iter().any(|c| c == "RCPT TO:<arquivo@kianda.ao>"), "{comandos:?}");
        assert!(dados.contains("Subject: =?UTF-8?B?"), "assunto com acentos codificado");
        assert!(!dados.contains("arquivo@kianda.ao"), "a cópia oculta não aparece nos cabeçalhos");
        assert!(dados.contains("multipart/alternative") && dados.contains(&B64.encode("<p>Olá</p>")));
        // senha errada: mensagem clara
        let (porta, _rx) = servidor_falso();
        let e = enviar(&srv(porta, "outra"), &m).unwrap_err().to_string();
        assert!(e.contains("Confirme o utilizador e a senha"), "{e}");
    }

    #[test]
    fn enderecos_validados() {
        assert!(email_valido("geral@kianda.ao"));
        for mau in ["", "sem-arroba", "a@b", "a b@c.ao", "a@b.ao\r\nBcc: x@y.ao", "<a@b.ao>", "a@@b.ao", "a@.ao"] {
            assert!(!email_valido(mau), "{mau:?}");
        }
        assert_eq!(enderecos("a@b.ao; c@d.ao").unwrap().len(), 2);
        assert!(enderecos("a@b.ao, mau").is_err());
    }
}

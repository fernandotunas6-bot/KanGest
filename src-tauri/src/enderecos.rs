//! IPv4 e IPv6 ao mesmo tempo (dupla pilha) em todas as ligações do KanGest: o servidor dos postos em rede, a API,
//! as impressoras de rede e os diagnósticos.
//!
//! - **Abrir uma porta** (`escutar`): para a rede, um só socket IPv6 com `IPV6_V6ONLY` desligado aceita os dois (no
//!   Windows vem ligado por omissão, por isso desliga-se explicitamente); sem IPv6 na máquina, fica o IPv4. Só desta
//!   máquina: `127.0.0.1` e `::1` (o `localhost` de um túnel pode ir a qualquer um dos dois).
//! - **Ligar a um endereço** (`alvo`, `ligar`): um IPv6 escreve-se entre parênteses rectos com a porta
//!   (`[2001:db8::1]:7799`); um nome com os dois tipos de endereço tenta-os todos, pela ordem do sistema.
//! - **Quem ligou** (`canonico`): um cliente IPv4 numa porta de dupla pilha chega como `::ffff:a.b.c.d` — volta a IPv4,
//!   para os registos, os bloqueios e o reconhecimento do túnel na própria máquina (`127.0.0.1`).

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};

/// IPv4 dentro de IPv6 (`::ffff:a.b.c.d`) passa a IPv4; o resto fica igual.
pub fn canonico(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(a) => a.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

fn socket_v6_duplo(porta: u16) -> io::Result<TcpListener> {
    let s = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    s.set_only_v6(false)?;
    s.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, porta)).into())?;
    s.listen(128)?;
    Ok(s.into())
}

/// Abre a porta. `toda_a_rede`: aceita de outros computadores, por IPv4 e IPv6 (um só socket de dupla pilha, ou só
/// IPv4 se a máquina não tiver IPv6). Senão, só desta máquina: `127.0.0.1` (obrigatório) e `::1` (se existir).
/// Com `porta` 0 o sistema escolhe uma e o `::1` usa a mesma.
pub fn escutar(porta: u16, toda_a_rede: bool) -> io::Result<Vec<TcpListener>> {
    if toda_a_rede {
        return match socket_v6_duplo(porta) {
            Ok(l) => Ok(vec![l]),
            // sem IPv6 (desligado no sistema) fica o IPv4; se a porta estiver ocupada, o IPv4 dá o mesmo erro
            Err(_) => Ok(vec![TcpListener::bind((Ipv4Addr::UNSPECIFIED, porta))?]),
        };
    }
    let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, porta))?;
    let porta = v4.local_addr()?.port();
    let mut v = vec![v4];
    if let Ok(v6) = TcpListener::bind((Ipv6Addr::LOCALHOST, porta)) {
        v.push(v6);
    }
    Ok(v)
}

/// Acorda os `accept()` de uma porta aberta com `escutar` (para o servidor ver o sinal de parar e fechar).
pub fn acordar(porta: u16) {
    for ip in [IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V6(Ipv6Addr::LOCALHOST)] {
        let _ = TcpStream::connect_timeout(&SocketAddr::new(ip, porta), Duration::from_secs(2));
    }
}

/// `destino` com a porta, pronto para resolver: `192.168.1.10` → `192.168.1.10:9100`, `2001:db8::1` →
/// `[2001:db8::1]:9100`, `[2001:db8::1]` → `[2001:db8::1]:9100`; com a porta já indicada fica como está.
pub fn alvo(destino: &str, porta_padrao: u16) -> String {
    let s = destino.trim();
    if let Some(resto) = s.strip_prefix('[') {
        return if resto.contains("]:") { s.to_string() } else { format!("[{}]:{porta_padrao}", resto.trim_end_matches(']')) };
    }
    if s.parse::<Ipv6Addr>().is_ok() {
        return format!("[{s}]:{porta_padrao}");
    }
    if s.contains(':') {
        s.to_string()
    } else {
        format!("{s}:{porta_padrao}")
    }
}

/// Liga a `alvo` (`host:porta` ou `[v6]:porta`), tentando cada endereço do nome (IPv6 e IPv4) pela ordem do sistema.
pub fn ligar(alvo: &str, prazo: Duration) -> io::Result<TcpStream> {
    let mut ultimo = io::Error::new(io::ErrorKind::NotFound, format!("Endereço sem resolução: {alvo}"));
    for a in alvo.to_socket_addrs()? {
        match TcpStream::connect_timeout(&a, prazo) {
            Ok(s) => return Ok(s),
            Err(e) => ultimo = e,
        }
    }
    Err(ultimo)
}

/// Endereços desta máquina para ditar aos outros postos: o IPv4 e o IPv6 (global ou da rede local) da interface que
/// sai para a Internet. «Ligar» um socket UDP não envia nada, mas revela o endereço da interface usada.
pub fn locais() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(s) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(a) = s.local_addr() {
                v.push(a.ip().to_string());
            }
        }
    }
    if let Ok(s) = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)) {
        if s.connect("[2001:4860:4860::8888]:80").is_ok() {
            if let Ok(SocketAddr::V6(a)) = s.local_addr() {
                let ip = *a.ip();
                // fe80::/10 (só no próprio cabo) não serve a outros postos sem o número da interface
                if !ip.is_unspecified() && !ip.is_loopback() && (ip.segments()[0] & 0xffc0) != 0xfe80 {
                    v.push(ip.to_string());
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn alvos() {
        assert_eq!(alvo("192.168.1.10", 9100), "192.168.1.10:9100");
        assert_eq!(alvo(" 192.168.1.10:9200 ", 9100), "192.168.1.10:9200");
        assert_eq!(alvo("servidor.loja.ao", 7799), "servidor.loja.ao:7799");
        assert_eq!(alvo("servidor.loja.ao:8000", 7799), "servidor.loja.ao:8000");
        assert_eq!(alvo("2001:db8::1", 7799), "[2001:db8::1]:7799");
        assert_eq!(alvo("::1", 7799), "[::1]:7799");
        assert_eq!(alvo("[2001:db8::1]", 7799), "[2001:db8::1]:7799");
        assert_eq!(alvo("[2001:db8::1]:8000", 7799), "[2001:db8::1]:8000");
    }

    #[test]
    fn ipv4_dentro_de_ipv6() {
        let m: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert_eq!(canonico(m), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(canonico(m).is_loopback());
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(canonico(v6), v6);
    }

    /// Uma porta aberta para a rede responde por IPv4 e (havendo IPv6 na máquina) por IPv6; só desta máquina, pelos dois
    /// endereços locais.
    #[test]
    fn dupla_pilha() {
        let eco = |l: TcpListener| {
            std::thread::spawn(move || {
                for s in l.incoming().flatten().take(2) {
                    let mut s = s;
                    let ip = canonico(s.peer_addr().unwrap().ip()).to_string();
                    let _ = s.write_all(ip.as_bytes());
                }
            });
        };
        let pedir = |a: &str| -> Option<String> {
            let mut s = ligar(a, Duration::from_secs(2)).ok()?;
            let mut r = String::new();
            s.read_to_string(&mut r).ok()?;
            Some(r)
        };
        let rede = escutar(0, true).unwrap();
        let porta = rede[0].local_addr().unwrap().port();
        let tem_v6 = rede[0].local_addr().unwrap().is_ipv6();
        rede.into_iter().for_each(eco);
        assert_eq!(pedir(&format!("127.0.0.1:{porta}")).as_deref(), Some("127.0.0.1"), "IPv4 numa porta de dupla pilha chega como IPv4");
        if tem_v6 {
            assert_eq!(pedir(&alvo("::1", porta)).as_deref(), Some("::1"));
        }

        let local = escutar(0, false).unwrap();
        let porta = local[0].local_addr().unwrap().port();
        let n = local.len();
        local.into_iter().for_each(eco);
        assert_eq!(pedir(&format!("127.0.0.1:{porta}")).as_deref(), Some("127.0.0.1"));
        if n == 2 {
            assert_eq!(pedir(&format!("[::1]:{porta}")).as_deref(), Some("::1"));
        }
    }
}

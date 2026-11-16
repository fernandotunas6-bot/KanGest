//! API de integração: outros sistemas (loja online, contabilidade, BI, apps) ligam-se ao KanGest por HTTP
//! com chaves de acesso.
//!
//! - Servidor HTTP/1.1 próprio, separado do dos postos em rede ([`crate::rede`]); desligado por omissão, porta 7800.
//! - Cada pedido traz a chave em `Authorization: Bearer kg_…` (ou `X-API-Key: kg_…`). Da chave só se guarda o SHA-256
//!   e o prefixo que a identifica; o segredo mostra-se uma única vez, ao criar ou renovar.
//! - A chave age em nome de um utilizador e de um posto: as regras de negócio, a numeração, a assinatura AGT, o stock,
//!   a Facturação Electrónica e a auditoria são as do programa, porque os pedidos passam pelo mesmo despachante `rpc`
//!   (com o contexto da chave, como os postos em rede).
//! - Permissões por chave: leituras próprias da API (`api.*`) e as permissões do programa que uma integração pode ter.
//!   A chave nunca faz mais do que o utilizador em nome de quem age.
//! - IPs permitidos, validade, limite de pedidos por minuto, registo de pedidos e `Idempotency-Key` na emissão.
//!
//! Como na rede local, o servidor fala HTTP sem cifra: para a Internet deve ficar atrás de um túnel ou proxy com HTTPS
//! (ver `docs/API.md`). Os cabeçalhos `CF-Connecting-IP`, `X-Real-IP` e `X-Forwarded-For` só contam quando o pedido
//! vem da própria máquina (o túnel).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::services;
use crate::state::{permissoes_efectivas, AppState, Contexto, PostoAtual, Sessao, CONTEXTO};

pub const PORTA_PADRAO: u16 = 7800;
const LIMITE_CABECALHO: usize = 64 * 1024;
const LIMITE_CORPO: usize = 2 * 1024 * 1024;
const MAX_LIGACOES: usize = 32;
/// Tempo máximo para receber um pedido completo (protege das ligações lentas de propósito).
const PRAZO_PEDIDO: Duration = Duration::from_secs(30);
/// «kg_» + 48 caracteres alfanuméricos (~285 bits); o prefixo identifica a chave sem a revelar.
const CHAVE_LEN: usize = 51;
const PREFIXO_LEN: usize = 11;
/// Falhas de autenticação por IP antes de recusar pedidos durante a janela.
const FALHAS_MAX: u32 = 20;
const FALHAS_JANELA: u64 = 600;

/// Especificação OpenAPI da API, servida em `GET /api/v1/openapi.json`.
const OPENAPI: &str = include_str!("../../docs/openapi.json");

/// Permissões que uma chave de API pode ter: (id, descrição, grupo).
/// As `api.*` são leituras próprias da API; as outras são permissões do programa (ver `state::PERMISSOES`).
pub const PERMISSOES_API: &[(&str, &str, &str)] = &[
    ("api.artigos", "Ler artigos, preços e tabelas", "Leitura"),
    ("api.stock", "Ler existências e movimentos de stock", "Leitura"),
    ("api.entidades", "Ler clientes e fornecedores", "Leitura"),
    ("api.documentos", "Ler documentos (vendas, compras, recibos)", "Leitura"),
    ("api.custos", "Ver preços de custo e margens", "Leitura"),
    ("relatorios.ver", "Resumos de vendas", "Leitura"),
    ("sistema.saft", "Exportar o SAF-T", "Leitura"),
    ("artigos.criar", "Criar artigos", "Artigos e entidades"),
    ("artigos.editar", "Alterar artigos e preços", "Artigos e entidades"),
    ("clientes", "Criar e alterar clientes", "Artigos e entidades"),
    ("fornecedores", "Criar e alterar fornecedores", "Artigos e entidades"),
    ("vendas.fatura_recibo", "Emitir Factura/Recibo (FR)", "Vendas"),
    ("vendas.fatura", "Emitir Factura (FT) e Guia de Transporte (GT)", "Vendas"),
    ("vendas.proforma", "Emitir Proforma, Orçamento e Nota de Encomenda", "Vendas"),
    ("vendas.nota_credito", "Emitir Nota de Crédito", "Vendas"),
    ("vendas.nota_debito", "Emitir Nota de Débito", "Vendas"),
    ("tesouraria.recibo", "Emitir Recibos de cliente", "Vendas"),
    ("pos.alterar_preco", "Vender com preço diferente do da ficha", "Vendas"),
    ("pos.desconto", "Vender com desconto", "Vendas"),
    ("vendas.limite_credito", "Vender acima do limite de crédito do cliente", "Vendas"),
    ("documentos.anular", "Anular documentos", "Vendas"),
    ("stock.entrada", "Entradas de stock", "Stock e compras"),
    ("stock.saida", "Saídas de stock e consumo interno", "Stock e compras"),
    ("stock.transferencia", "Transferências entre armazéns", "Stock e compras"),
    ("compras.fatura", "Documentos de compra (V/Factura…)", "Stock e compras"),
];

fn permitida_api(p: &str) -> bool {
    PERMISSOES_API.iter().any(|x| x.0 == p)
}

fn descricao_permissao(p: &str) -> &str {
    PERMISSOES_API.iter().find(|x| x.0 == p).map(|x| x.1).unwrap_or(p)
}

/// Permissão necessária para emitir cada tipo de documento pela API (`None` = tipo não disponível na API).
fn permissao_do_tipo(tipo: &str) -> Option<&'static str> {
    Some(match tipo {
        "FR" => "vendas.fatura_recibo",
        "FT" | "GT" => "vendas.fatura",
        "PP" | "OR" | "NE" => "vendas.proforma",
        "NC" => "vendas.nota_credito",
        "ND" => "vendas.nota_debito",
        "RC" => "tesouraria.recibo",
        "FC" | "VFR" | "VNC" | "VND" | "VNE" | "VOR" | "VGT" | "AF" => "compras.fatura",
        "ES" => "stock.entrada",
        "SS" | "CI" => "stock.saida",
        "TS" => "stock.transferencia",
        _ => return None,
    })
}

// ============================== configuração e servidor ==============================

#[derive(Debug, Clone)]
pub struct Config {
    pub ativo: bool,
    pub porta: u16,
    /// true: aceita ligações da rede (0.0.0.0); false: só desta máquina (127.0.0.1), ex.: atrás de um túnel.
    pub rede: bool,
    /// Cloudflare Access (opcional): domínio da equipa (ex.: «empresa.cloudflareaccess.com») e AUD da aplicação.
    /// Com os dois, os pedidos que chegam pela Cloudflare têm de trazer o JWT do Access.
    pub cf_equipa: String,
    pub cf_aud: String,
}

async fn ler_config(p: &SqlitePool) -> AppResult<Config> {
    let rows = sqlx::query("SELECT chave, valor FROM config WHERE chave LIKE 'api.%'").fetch_all(p).await?;
    let v = |k: &str| rows.iter().find(|r| r.get::<String, _>("chave") == k).map(|r| r.get::<String, _>("valor"));
    Ok(Config {
        ativo: v("api.ativo").as_deref() == Some("1"),
        porta: v("api.porta").and_then(|x| x.trim().parse().ok()).filter(|p: &u16| *p > 0).unwrap_or(PORTA_PADRAO),
        rede: v("api.rede").as_deref() != Some("0"),
        cf_equipa: v("api.cf_equipa").unwrap_or_default(),
        cf_aud: v("api.cf_aud").unwrap_or_default(),
    })
}

struct Servidor {
    porta: u16,
    rede: bool,
    parar: Arc<AtomicBool>,
    fio: Option<std::thread::JoinHandle<()>>,
}

static SERVIDOR: Mutex<Option<Servidor>> = Mutex::new(None);
static ERRO: Mutex<Option<String>> = Mutex::new(None);
static APP: OnceLock<Arc<AppState>> = OnceLock::new();
static ATENDER: AtomicUsize = AtomicUsize::new(0);

/// Guarda o estado da aplicação para o ecrã de configuração poder abrir o servidor.
pub fn registar_estado(a: Arc<AppState>) {
    let _ = APP.set(a);
}

/// Arranque da aplicação: abre a API se estiver ligada. Não se chama nos postos clientes (a base é a do servidor).
pub fn arrancar(st: Arc<AppState>) {
    registar_estado(st.clone());
    let cfg = match tauri::async_runtime::block_on(ler_config(&st.pool)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("API: configuração ilegível: {e}");
            return;
        }
    };
    if cfg.ativo {
        if let Err(e) = iniciar(st, cfg.porta, cfg.rede) {
            eprintln!("API não arrancou: {e}");
            *ERRO.lock().unwrap() = Some(e.to_string());
        }
    }
}

/// Porta e modo em que a API está a servir (`None` = desligada).
pub fn a_servir() -> Option<(u16, bool)> {
    SERVIDOR.lock().unwrap().as_ref().map(|s| (s.porta, s.rede))
}

/// Abre (ou reabre noutra porta) o servidor da API.
pub fn iniciar(st: Arc<AppState>, porta: u16, rede: bool) -> AppResult<u16> {
    let mut guarda = SERVIDOR.lock().unwrap();
    if guarda.as_ref().is_some_and(|s| s.porta == porta && s.rede == rede) {
        return Ok(porta);
    }
    if let Some(s) = guarda.take() {
        fechar(s);
    }
    let ip = if rede { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
    let escuta = TcpListener::bind((ip, porta)).map_err(|e| {
        AppError::msg(format!("Não foi possível abrir a porta {porta} da API: {e}. Escolha outra porta ou autorize o KanGest na firewall."))
    })?;
    let parar = Arc::new(AtomicBool::new(false));
    let sinal = parar.clone();
    let fio = std::thread::Builder::new()
        .name("kangest-api".into())
        .spawn(move || {
            for fluxo in escuta.incoming() {
                if sinal.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut fluxo) = fluxo else { continue };
                // limite de pedidos em simultâneo: uma integração avariada não deixa o servidor sem fios
                if ATENDER.load(Ordering::SeqCst) >= MAX_LIGACOES {
                    fluxo.set_write_timeout(Some(Duration::from_secs(2))).ok();
                    let r = Resposta::erro(503, "ocupado", "Demasiados pedidos em simultâneo. Tente de novo dentro de momentos.").com("Retry-After", "2");
                    let _ = fluxo.write_all(&r.bytes());
                    continue;
                }
                ATENDER.fetch_add(1, Ordering::SeqCst);
                let st = st.clone();
                let lancado = std::thread::Builder::new().name("kangest-api-pedido".into()).stack_size(crate::PILHA_PEDIDOS).spawn(move || {
                    atender(&st, fluxo);
                    ATENDER.fetch_sub(1, Ordering::SeqCst);
                });
                if lancado.is_err() {
                    ATENDER.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })
        .map_err(|e| AppError::msg(format!("Não foi possível iniciar o servidor da API: {e}")))?;
    *guarda = Some(Servidor { porta, rede, parar, fio: Some(fio) });
    *ERRO.lock().unwrap() = None;
    Ok(porta)
}

fn fechar(mut s: Servidor) {
    s.parar.store(true, Ordering::SeqCst);
    // acorda o accept() com uma ligação local: o ciclo vê o sinal, termina e fecha a porta
    let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, s.porta)), Duration::from_secs(2));
    if let Some(f) = s.fio.take() {
        let _ = f.join();
    }
}

/// Fecha o servidor da API (se estiver aberto).
pub fn parar() {
    let s = SERVIDOR.lock().unwrap().take();
    if let Some(s) = s {
        fechar(s);
    }
}

// ============================== HTTP ==============================

struct Pedido {
    metodo: String,
    /// Caminho sem a consulta (ex.: `/api/v1/artigos`).
    caminho: String,
    /// Caminho com a consulta, como chegou (para o registo).
    alvo: String,
    consulta: HashMap<String, String>,
    cabecalhos: HashMap<String, String>,
    corpo: Vec<u8>,
}

struct Resposta {
    estado: u16,
    tipo: &'static str,
    corpo: Vec<u8>,
    extra: Vec<(&'static str, String)>,
    /// Mensagem de erro para o registo de pedidos (não vai na resposta quando é um erro interno).
    erro: Option<String>,
}

impl Resposta {
    fn json(estado: u16, v: &Value) -> Self {
        Self { estado, tipo: "application/json; charset=utf-8", corpo: v.to_string().into_bytes(), extra: Vec::new(), erro: None }
    }

    fn erro(estado: u16, codigo: &str, mensagem: impl Into<String>) -> Self {
        let m = mensagem.into();
        let mut r = Self::json(estado, &json!({ "erro": { "codigo": codigo, "mensagem": m } }));
        r.erro = Some(m);
        r
    }

    fn com(mut self, cabecalho: &'static str, valor: impl Into<String>) -> Self {
        self.extra.push((cabecalho, valor.into()));
        self
    }

    fn bytes(&self) -> Vec<u8> {
        let mut c = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nX-KanGest-API: v1\r\nConnection: close\r\n",
            self.estado,
            texto_estado(self.estado),
            self.tipo,
            self.corpo.len()
        );
        for (k, v) in &self.extra {
            c.push_str(&format!("{k}: {v}\r\n"));
        }
        c.push_str("\r\n");
        let mut b = c.into_bytes();
        b.extend_from_slice(&self.corpo);
        b
    }
}

fn texto_estado(c: u16) -> &'static str {
    match c {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

impl From<AppError> for Resposta {
    fn from(e: AppError) -> Self {
        match e {
            AppError::NaoAutenticado => Resposta::erro(401, "nao_autenticado", "Chave de API em falta ou inválida."),
            AppError::SemPermissao => Resposta::erro(403, "sem_permissao", "A chave (ou o utilizador em nome de quem age) não tem permissão para esta operação."),
            AppError::NaoEncontrado => Resposta::erro(404, "nao_encontrado", "Registo não encontrado."),
            AppError::Json(e) => Resposta::erro(400, "pedido_invalido", format!("Parâmetros inválidos: {e}")),
            AppError::Msg(m) => Resposta::erro(422, "validacao", m),
            e @ (AppError::Db(_) | AppError::Migracao(_)) => {
                let mut r = Resposta::erro(500, "erro_interno", "Erro interno do KanGest. O pormenor fica no registo de pedidos da API.");
                r.erro = Some(e.to_string());
                r
            }
        }
    }
}

impl From<sqlx::Error> for Resposta {
    fn from(e: sqlx::Error) -> Self {
        AppError::Db(e).into()
    }
}

/// Descodifica `%XX` (e `+` como espaço nas consultas).
fn descodificar(s: &str, mais_espaco: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' if mais_espaco => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn ler_consulta(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|x| !x.is_empty())
        .map(|par| match par.split_once('=') {
            Some((k, v)) => (descodificar(k, true), descodificar(v, true)),
            None => (descodificar(par, true), String::new()),
        })
        .collect()
}

/// Lê um pedido HTTP/1.1 (Content-Length ou chunked, `Expect: 100-continue`).
fn ler_pedido(f: &mut TcpStream) -> Result<Pedido, Resposta> {
    let inicio = Instant::now();
    let mut dados: Vec<u8> = Vec::with_capacity(4096);
    let mut buf = [0u8; 16 * 1024];
    let mut ler = |f: &mut TcpStream, dados: &mut Vec<u8>| -> Result<usize, Resposta> {
        if inicio.elapsed() > PRAZO_PEDIDO {
            return Err(Resposta::erro(400, "pedido_invalido", "O pedido demorou demasiado a chegar."));
        }
        let n = f.read(&mut buf).map_err(|_| Resposta::erro(400, "pedido_invalido", "Pedido interrompido."))?;
        dados.extend_from_slice(&buf[..n]);
        Ok(n)
    };
    let fim = loop {
        if let Some(p) = dados.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if dados.len() > LIMITE_CABECALHO {
            return Err(Resposta::erro(431, "cabecalho_grande", "Cabeçalhos do pedido demasiado grandes."));
        }
        if ler(f, &mut dados)? == 0 {
            return Err(Resposta::erro(400, "pedido_invalido", "Pedido incompleto."));
        }
    };
    let cab = String::from_utf8_lossy(&dados[..fim]).into_owned();
    let mut linhas = cab.split("\r\n");
    let primeira = linhas.next().unwrap_or("");
    let mut partes = primeira.split_whitespace();
    let (Some(metodo), Some(alvo), Some(versao)) = (partes.next(), partes.next(), partes.next()) else {
        return Err(Resposta::erro(400, "pedido_invalido", "Linha de pedido HTTP inválida."));
    };
    if !versao.starts_with("HTTP/1.") {
        return Err(Resposta::erro(400, "pedido_invalido", "Só se aceita HTTP/1.x."));
    }
    let mut cabecalhos = HashMap::new();
    for l in linhas {
        if let Some((k, v)) = l.split_once(':') {
            cabecalhos.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let chunked = cabecalhos.get("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
    let tamanho: Option<usize> = match cabecalhos.get("content-length") {
        Some(v) => Some(v.parse().map_err(|_| Resposta::erro(400, "pedido_invalido", "Content-Length inválido."))?),
        None => None,
    };
    if tamanho.is_some_and(|t| t > LIMITE_CORPO) {
        return Err(Resposta::erro(413, "pedido_grande", "O corpo do pedido excede 2 MB."));
    }
    let mut corpo = dados[fim + 4..].to_vec();
    let falta = chunked || tamanho.is_some_and(|t| corpo.len() < t);
    if falta && cabecalhos.get("expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) {
        let _ = f.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    if chunked {
        while !corpo.ends_with(b"0\r\n\r\n") {
            if corpo.len() > LIMITE_CORPO + 64 * 1024 {
                return Err(Resposta::erro(413, "pedido_grande", "O corpo do pedido excede 2 MB."));
            }
            if ler(f, &mut corpo)? == 0 {
                return Err(Resposta::erro(400, "pedido_invalido", "Corpo do pedido incompleto."));
            }
        }
        corpo = crate::http::desfazer_chunked(&corpo).map_err(|_| Resposta::erro(400, "pedido_invalido", "Corpo «chunked» inválido."))?;
        if corpo.len() > LIMITE_CORPO {
            return Err(Resposta::erro(413, "pedido_grande", "O corpo do pedido excede 2 MB."));
        }
    } else if let Some(t) = tamanho {
        while corpo.len() < t {
            if ler(f, &mut corpo)? == 0 {
                return Err(Resposta::erro(400, "pedido_invalido", "Corpo do pedido incompleto."));
            }
        }
        corpo.truncate(t);
    } else {
        corpo.clear();
    }
    let (caminho, consulta) = match alvo.split_once('?') {
        Some((c, q)) => (c, ler_consulta(q)),
        None => (alvo, HashMap::new()),
    };
    Ok(Pedido {
        metodo: metodo.to_ascii_uppercase(),
        caminho: caminho.to_string(),
        alvo: alvo.chars().take(300).collect(),
        consulta,
        cabecalhos,
        corpo,
    })
}

// ============================== endereços IP ==============================

#[derive(Debug, Clone, PartialEq)]
enum Rede {
    V4(u32, u8),
    V6(u128, u8),
}

/// IPv4 dentro de IPv6 (`::ffff:a.b.c.d`) passa a IPv4.
fn normalizar(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(a) => a.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

/// Lista de IPs e redes («192.168.1.10, 10.0.0.0/8, ::1»), separados por vírgulas, pontos e vírgulas ou espaços.
fn ler_redes(s: &str) -> Result<Vec<Rede>, String> {
    s.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|x| !x.is_empty())
        .map(|x| {
            let (ip, bits) = match x.split_once('/') {
                Some((i, b)) => (i, Some(b.parse::<u8>().map_err(|_| format!("Rede inválida: {x}"))?)),
                None => (x, None),
            };
            match normalizar(ip.parse::<IpAddr>().map_err(|_| format!("Endereço IP inválido: {x}"))?) {
                IpAddr::V4(a) => {
                    let b = bits.unwrap_or(32);
                    if b > 32 {
                        return Err(format!("Rede inválida: {x}"));
                    }
                    Ok(Rede::V4(u32::from(a), b))
                }
                IpAddr::V6(a) => {
                    let b = bits.unwrap_or(128);
                    if b > 128 {
                        return Err(format!("Rede inválida: {x}"));
                    }
                    Ok(Rede::V6(u128::from(a), b))
                }
            }
        })
        .collect()
}

fn ip_permitido(redes: &[Rede], ip: IpAddr) -> bool {
    if redes.is_empty() {
        return true;
    }
    let ip = normalizar(ip);
    redes.iter().any(|r| match (r, ip) {
        (Rede::V4(base, bits), IpAddr::V4(a)) => {
            let m = if *bits == 0 { 0 } else { u32::MAX << (32 - *bits as u32) };
            (u32::from(a) & m) == (base & m)
        }
        (Rede::V6(base, bits), IpAddr::V6(a)) => {
            let m = if *bits == 0 { 0 } else { u128::MAX << (128 - *bits as u32) };
            (u128::from(a) & m) == (base & m)
        }
        _ => false,
    })
}

/// IP de quem pediu. Atrás de um túnel ou proxy na própria máquina, vale o IP que ele indica.
fn ip_cliente(par: IpAddr, cab: &HashMap<String, String>) -> IpAddr {
    let par = normalizar(par);
    if !par.is_loopback() {
        return par;
    }
    for h in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        if let Some(ip) = cab.get(h).and_then(|v| v.split(',').next()).and_then(|x| x.trim().parse::<IpAddr>().ok()) {
            return normalizar(ip);
        }
    }
    par
}

static FALHAS: Mutex<Option<HashMap<IpAddr, (u64, u32)>>> = Mutex::new(None);
static JANELAS: Mutex<Option<HashMap<i64, (u64, i64)>>> = Mutex::new(None);

fn agora_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Segundos que faltam para um IP com demasiadas chaves erradas voltar a ser atendido.
fn ip_bloqueado(ip: IpAddr) -> Option<u64> {
    let agora = agora_s();
    let f = FALHAS.lock().unwrap();
    let (inicio, n) = *f.as_ref()?.get(&ip)?;
    (n >= FALHAS_MAX && agora.saturating_sub(inicio) < FALHAS_JANELA).then(|| FALHAS_JANELA - agora.saturating_sub(inicio))
}

fn falha_ip(ip: IpAddr) {
    let agora = agora_s();
    let mut f = FALHAS.lock().unwrap();
    let mapa = f.get_or_insert_with(HashMap::new);
    if mapa.len() > 10_000 {
        mapa.retain(|_, (inicio, _)| agora.saturating_sub(*inicio) < FALHAS_JANELA);
    }
    let e = mapa.entry(ip).or_insert((agora, 0));
    if agora.saturating_sub(e.0) >= FALHAS_JANELA {
        *e = (agora, 0);
    }
    e.1 += 1;
}

// ============================== Cloudflare Access ==============================

type ChavesCf = Vec<(String, rsa::RsaPublicKey)>;

/// Chaves públicas do Cloudflare Access da equipa, guardadas uma hora (a Cloudflare roda-as de tempos a tempos).
static CF_CHAVES: Mutex<Option<(String, Instant, ChavesCf)>> = Mutex::new(None);

fn b64url(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok()
}

/// Chaves RSA de um JWKS (`{"keys": [{kid, kty, n, e}]}`).
fn chaves_de_jwks(v: &Value) -> ChavesCf {
    v["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| k["kty"] == "RSA")
        .filter_map(|k| {
            let n = rsa::BigUint::from_bytes_be(&b64url(k["n"].as_str()?)?);
            let e = rsa::BigUint::from_bytes_be(&b64url(k["e"].as_str()?)?);
            Some((k["kid"].as_str().unwrap_or("").to_string(), rsa::RsaPublicKey::new(n, e).ok()?))
        })
        .collect()
}

/// Chaves da equipa: da cache ou de `https://<equipa>/cdn-cgi/access/certs`.
fn chaves_cf(equipa: &str, renovar: bool) -> Result<ChavesCf, String> {
    if let Some((e, quando, k)) = CF_CHAVES.lock().unwrap().as_ref() {
        if e == equipa && !renovar && quando.elapsed() < Duration::from_secs(3600) {
            return Ok(k.clone());
        }
    }
    let r = crate::http::get(&format!("https://{equipa}/cdn-cgi/access/certs")).map_err(|e| e.to_string())?;
    if r.estado != 200 {
        return Err(format!("o Cloudflare Access respondeu {}", r.estado));
    }
    let v: Value = serde_json::from_slice(&r.corpo).map_err(|_| "resposta ilegível do Cloudflare Access".to_string())?;
    let k = chaves_de_jwks(&v);
    if k.is_empty() {
        return Err("o Cloudflare Access não devolveu chaves".into());
    }
    *CF_CHAVES.lock().unwrap() = Some((equipa.to_string(), Instant::now(), k.clone()));
    Ok(k)
}

/// Para os testes: chaves da equipa sem ir à rede.
#[cfg(test)]
pub fn cf_chaves_teste(equipa: &str, jwks: &Value) {
    *CF_CHAVES.lock().unwrap() = Some((equipa.to_string(), Instant::now(), chaves_de_jwks(jwks)));
}

/// Confirma o JWT que o Cloudflare Access junta aos pedidos que autorizou: assinatura RS256 de uma chave da
/// equipa, AUD da aplicação, emissor `https://<equipa>` e validade (com 60 s de folga no relógio).
fn verificar_jwt_cf(token: &str, chaves: &ChavesCf, equipa: &str, aud: &str, agora: u64) -> Result<(), String> {
    use rsa::signature::Verifier;
    let mut partes = token.split('.');
    let (Some(h), Some(p), Some(s), None) = (partes.next(), partes.next(), partes.next(), partes.next()) else {
        return Err("JWT malformado".into());
    };
    let cab: Value = b64url(h).and_then(|b| serde_json::from_slice(&b).ok()).ok_or("JWT malformado")?;
    if cab["alg"] != "RS256" {
        return Err("algoritmo do JWT não aceite".into());
    }
    let kid = cab["kid"].as_str().unwrap_or("");
    let assinatura = b64url(s).and_then(|b| rsa::pkcs1v15::Signature::try_from(b.as_slice()).ok()).ok_or("assinatura malformada")?;
    let dados = format!("{h}.{p}");
    let confere = chaves
        .iter()
        .filter(|(k, _)| kid.is_empty() || k == kid)
        .any(|(_, chave)| rsa::pkcs1v15::VerifyingKey::<Sha256>::new(chave.clone()).verify(dados.as_bytes(), &assinatura).is_ok());
    if !confere {
        return Err("assinatura do JWT não confere".into());
    }
    let c: Value = b64url(p).and_then(|b| serde_json::from_slice(&b).ok()).ok_or("JWT malformado")?;
    let aud_ok = match &c["aud"] {
        Value::String(a) => a == aud,
        Value::Array(v) => v.iter().any(|x| x == aud),
        _ => false,
    };
    if !aud_ok {
        return Err("o JWT é de outra aplicação (AUD)".into());
    }
    if c["iss"].as_str() != Some(format!("https://{equipa}").as_str()) {
        return Err("o JWT é de outra equipa".into());
    }
    let exp = c["exp"].as_u64().ok_or("JWT sem validade")?;
    if exp + 60 < agora {
        return Err("JWT expirado".into());
    }
    if c["nbf"].as_u64().is_some_and(|nbf| nbf > agora + 60) {
        return Err("JWT ainda não válido".into());
    }
    Ok(())
}

/// Pedidos que chegam pela Cloudflare (trazem CF-Ray ou CF-Connecting-IP) têm de trazer o JWT do Access,
/// quando ele está configurado. Os pedidos da rede local (sem esses cabeçalhos) só precisam da chave de API.
fn exigir_access(cfg: &Config, p: &Pedido) -> Result<(), Resposta> {
    if cfg.cf_equipa.is_empty() || cfg.cf_aud.is_empty() {
        return Ok(());
    }
    let pela_cloudflare = p.cabecalhos.contains_key("cf-ray") || p.cabecalhos.contains_key("cf-connecting-ip");
    if !pela_cloudflare {
        return Ok(());
    }
    let recusa = |m: String| Resposta::erro(403, "cloudflare_access", format!("Pedido sem a autorização do Cloudflare Access: {m}."));
    let token = p.cabecalhos.get("cf-access-jwt-assertion").cloned().or_else(|| {
        p.cabecalhos.get("cookie").and_then(|c| c.split(';').find_map(|x| x.trim().strip_prefix("CF_Authorization=").map(String::from)))
    });
    let Some(token) = token else { return Err(recusa("falta o JWT".into())) };
    let indisponivel = |e: String| Resposta::erro(503, "cloudflare_access", format!("Não foi possível confirmar o Cloudflare Access: {e}."));
    let mut chaves = chaves_cf(&cfg.cf_equipa, false).map_err(indisponivel)?;
    // chave que ainda não conhecemos (a Cloudflare roda-as): volta a buscá-las, no máximo uma vez a cada 5 minutos,
    // para um JWT inventado não pôr o KanGest a ir à Cloudflare a cada pedido
    let kid = token.split('.').next().and_then(b64url).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|h| h["kid"].as_str().map(String::from)).unwrap_or_default();
    if !kid.is_empty() && !chaves.iter().any(|(k, _)| *k == kid) {
        static ULTIMA: AtomicI64 = AtomicI64::new(0);
        let s = agora_s() as i64;
        if s - ULTIMA.load(Ordering::SeqCst) >= 300 {
            ULTIMA.store(s, Ordering::SeqCst);
            chaves = chaves_cf(&cfg.cf_equipa, true).map_err(indisponivel)?;
        }
    }
    verificar_jwt_cf(&token, &chaves, &cfg.cf_equipa, &cfg.cf_aud, agora_s()).map_err(recusa)
}

// ============================== chaves ==============================

fn gerar_chave() -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    let s: String = rand::rngs::OsRng.sample_iter(&Alphanumeric).take(CHAVE_LEN - 3).map(char::from).collect();
    format!("kg_{s}")
}

fn formato_valido(t: &str) -> bool {
    t.len() == CHAVE_LEN && t.starts_with("kg_") && t.as_bytes()[3..].iter().all(u8::is_ascii_alphanumeric)
}

fn hash_chave(t: &str) -> String {
    Sha256::digest(t.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Comparação em tempo constante (não revela quantos caracteres coincidem).
fn iguais(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn extrair_chave(cab: &HashMap<String, String>) -> Option<String> {
    if let Some(a) = cab.get("authorization").map(|a| a.trim()) {
        if a.get(..7).is_some_and(|b| b.eq_ignore_ascii_case("bearer ")) {
            return a.get(7..).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        }
    }
    cab.get("x-api-key").map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Chave autenticada: as permissões efectivas e o contexto (sessão e posto) em que os pedidos correm.
struct Chave {
    id: i64,
    nome: String,
    prefixo: String,
    permissoes: Vec<String>,
    limite_minuto: i64,
    sessao: Sessao,
    posto: PostoAtual,
}

impl Chave {
    fn tem(&self, p: &str) -> bool {
        self.permissoes.iter().any(|x| x == p)
    }

    fn exigir(&self, p: &str) -> Result<(), Resposta> {
        if self.tem(p) {
            Ok(())
        } else {
            Err(Resposta::erro(
                403,
                "sem_permissao",
                format!("Sem permissão: «{}» ({p}). Dê-a à chave e ao utilizador em nome de quem ela age.", descricao_permissao(p)),
            ))
        }
    }

    fn contexto(&self) -> Contexto {
        Contexto { sessao: Arc::new(Mutex::new(Some(self.sessao.clone()))), posto: self.posto.clone(), api_chave: Some(self.id) }
    }
}

type Recusa = (Resposta, Option<i64>);

async fn autenticar(st: &AppState, token: &str, ip: IpAddr) -> Result<Chave, Recusa> {
    let invalida = || (Resposta::erro(401, "chave_invalida", "Chave de API inválida.").com("WWW-Authenticate", "Bearer realm=\"KanGest\""), None);
    if !formato_valido(token) {
        return Err(invalida());
    }
    let row = sqlx::query(
        "SELECT c.*, u.nome AS u_nome, u.login AS u_login, u.perfil AS u_perfil, u.permissoes AS u_permissoes,
                u.limite_desconto AS u_limite, u.ativo AS u_ativo
         FROM api_chaves c JOIN utilizadores u ON u.id = c.utilizador_id WHERE c.prefixo = ?",
    )
    .bind(&token[..PREFIXO_LEN])
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| (Resposta::from(e), None))?;
    let Some(row) = row else { return Err(invalida()) };
    if !iguais(row.get::<String, _>("hash").as_bytes(), hash_chave(token).as_bytes()) {
        return Err(invalida());
    }
    // a partir daqui a chave é verdadeira: as mensagens podem dizer porque é recusada
    let id: i64 = row.get("id");
    let recusa = |estado: u16, codigo: &str, msg: String| Err((Resposta::erro(estado, codigo, msg), Some(id)));
    if row.get::<Option<String>, _>("revogada_em").is_some() {
        return recusa(401, "chave_revogada", "Esta chave de API foi revogada.".into());
    }
    if row.get::<i64, _>("ativa") == 0 {
        return recusa(401, "chave_inactiva", "Esta chave de API está desactivada.".into());
    }
    if let Some(exp) = row.get::<Option<String>, _>("expira_em").filter(|e| !e.is_empty()) {
        if exp < db::hoje() {
            return recusa(401, "chave_expirada", format!("Esta chave de API expirou em {exp}."));
        }
    }
    let redes = match ler_redes(row.get::<Option<String>, _>("ips").as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(_) => return recusa(403, "ip_nao_autorizado", "A lista de IPs desta chave é inválida.".into()),
    };
    if !ip_permitido(&redes, ip) {
        return recusa(403, "ip_nao_autorizado", format!("O endereço {ip} não está autorizado a usar esta chave."));
    }
    if row.get::<i64, _>("u_ativo") == 0 {
        return recusa(403, "utilizador_inactivo", "O utilizador em nome de quem esta chave age está inactivo.".into());
    }
    let perfil: String = row.get("u_perfil");
    let (nivel, do_utilizador) = permissoes_efectivas(&st.pool, &perfil, row.get::<Option<String>, _>("u_permissoes").as_deref()).await;
    let pedidas: Vec<String> = serde_json::from_str(&row.get::<String, _>("permissoes")).unwrap_or_default();
    let permissoes: Vec<String> = pedidas.into_iter().filter(|p| permitida_api(p) && (p.starts_with("api.") || do_utilizador.contains(p))).collect();
    let posto = st.ler_posto(row.get::<Option<i64>, _>("posto_id").unwrap_or(1)).await.map_err(|e| (Resposta::from(e), Some(id)))?;
    let sessao = Sessao {
        id: row.get("utilizador_id"),
        nome: row.get("u_nome"),
        login: row.get("u_login"),
        perfil,
        permissoes: permissoes.clone(),
        limite_desconto: row.get("u_limite"),
        nivel,
        mudar_senha: false,
        bloqueada: false,
        perfil_descricao: None,
    };
    Ok(Chave { id, nome: row.get("nome"), prefixo: row.get("prefixo"), permissoes, limite_minuto: row.get("limite_minuto"), sessao, posto })
}

/// Limite de pedidos por minuto de cada chave (janela fixa do minuto do relógio).
fn limite(c: &Chave) -> Result<Option<i64>, Resposta> {
    if c.limite_minuto <= 0 {
        return Ok(None);
    }
    let agora = agora_s();
    let minuto = agora / 60;
    let mut j = JANELAS.lock().unwrap();
    let e = j.get_or_insert_with(HashMap::new).entry(c.id).or_insert((minuto, 0));
    if e.0 != minuto {
        *e = (minuto, 0);
    }
    e.1 += 1;
    if e.1 > c.limite_minuto {
        let espera = 60 - agora % 60;
        return Err(Resposta::erro(429, "limite_pedidos", format!("Limite de {} pedidos por minuto atingido. Tente daqui a {espera} s.", c.limite_minuto))
            .com("Retry-After", espera.to_string())
            .com("X-RateLimit-Limit", c.limite_minuto.to_string())
            .com("X-RateLimit-Remaining", "0"));
    }
    Ok(Some(c.limite_minuto - e.1))
}

// ============================== atendimento ==============================

fn atender(st: &AppState, mut fluxo: TcpStream) {
    let inicio = Instant::now();
    fluxo.set_read_timeout(Some(Duration::from_secs(20))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let par = fluxo.peer_addr().map(|a| a.ip()).unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let (resposta, registo) = match ler_pedido(&mut fluxo) {
        Err(r) => {
            let ip = normalizar(par);
            (r, Some((ip, "?".to_string(), "?".to_string(), None, false)))
        }
        Ok(p) => {
            let ip = ip_cliente(par, &p.cabecalhos);
            let (r, chave, autenticado) = tauri::async_runtime::block_on(processar(st, &p, ip));
            let publico = p.metodo == "GET" && matches!(p.caminho.trim_end_matches('/'), "/api/v1/saude" | "/api/v1/openapi.json");
            (r, (!publico).then(|| (ip, p.metodo.clone(), p.alvo.clone(), chave, autenticado)))
        }
    };
    // regista antes de responder: o número do registo vai na resposta (X-KanGest-Pedido e, nos erros, «pedido»), para
    // quem integra poder citar o pedido e o administrador o encontrar em Sistema → API e Integrações
    let mut resposta = resposta;
    if let Some((ip, metodo, alvo, chave, autenticado)) = registo {
        let ms = inicio.elapsed().as_millis() as i64;
        if let Some(id) = tauri::async_runtime::block_on(registar(&st.pool, chave, autenticado, ip, &metodo, &alvo, &resposta, ms)) {
            resposta = com_numero(resposta, id);
        }
    }
    let _ = fluxo.write_all(&resposta.bytes());
    let _ = fluxo.flush();
    tauri::async_runtime::block_on(limpar_registo(&st.pool));
}

/// Junta o número do registo à resposta: no cabeçalho e, nas respostas de erro, dentro de «erro».
fn com_numero(mut r: Resposta, id: i64) -> Resposta {
    if r.estado >= 400 {
        if let Ok(mut v) = serde_json::from_slice::<Value>(&r.corpo) {
            if let Some(e) = v.get_mut("erro").and_then(Value::as_object_mut) {
                e.insert("pedido".into(), json!(id));
                r.corpo = v.to_string().into_bytes();
            }
        }
    }
    r.com("X-KanGest-Pedido", id.to_string())
}

static ULTIMA_LIMPEZA: AtomicI64 = AtomicI64::new(0);

/// Grava o pedido no registo e devolve o número dele.
#[allow(clippy::too_many_arguments)]
async fn registar(p: &SqlitePool, chave: Option<i64>, autenticado: bool, ip: IpAddr, metodo: &str, alvo: &str, r: &Resposta, ms: i64) -> Option<i64> {
    let agora = db::agora();
    let id = sqlx::query("INSERT INTO api_pedidos (data_hora, chave_id, ip, metodo, caminho, estado, duracao_ms, erro) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&agora)
        .bind(chave)
        .bind(ip.to_string())
        .bind(metodo)
        .bind(alvo)
        .bind(r.estado as i64)
        .bind(ms)
        .bind(r.erro.as_deref().map(|e| e.chars().take(500).collect::<String>()))
        .execute(p)
        .await
        .ok()
        .map(|x| x.last_insert_rowid());
    if autenticado {
        let _ = sqlx::query("UPDATE api_chaves SET ultimo_uso = ?, ultimo_ip = ?, pedidos = pedidos + 1 WHERE id = ?")
            .bind(&agora)
            .bind(ip.to_string())
            .bind(chave)
            .execute(p)
            .await;
    }
    id
}

/// Registo e idempotência guardam-se 90 dias (limpeza no máximo uma vez por hora, depois de responder).
async fn limpar_registo(p: &SqlitePool) {
    let s = agora_s() as i64;
    if s - ULTIMA_LIMPEZA.load(Ordering::SeqCst) >= 3600 {
        ULTIMA_LIMPEZA.store(s, Ordering::SeqCst);
        let antes = (chrono::Local::now() - chrono::Duration::days(90)).format("%Y-%m-%dT%H:%M:%S").to_string();
        let _ = sqlx::query("DELETE FROM api_pedidos WHERE data_hora < ?").bind(&antes).execute(p).await;
        let _ = sqlx::query("DELETE FROM api_idempotencia WHERE criado_em < ?").bind(&antes).execute(p).await;
    }
}

/// Todas as rotas da API (método e caminho depois de /api/v1; «{x}» é um segmento qualquer). É esta a lista que o
/// docs/openapi.json descreve: o teste `openapi_igual_as_rotas` falha se as duas se afastarem.
pub(crate) const ROTAS: &[(&str, &str)] = &[
    ("GET", "/saude"),
    ("GET", "/openapi.json"),
    ("GET", "/"),
    ("GET", "/tabelas"),
    ("GET", "/artigos"),
    ("POST", "/artigos"),
    ("GET", "/artigos/codigo/{codigo}"),
    ("GET", "/artigos/{id}"),
    ("GET", "/artigos/{id}/foto"),
    ("PUT", "/artigos/{id}"),
    ("PATCH", "/artigos/{id}"),
    ("GET", "/stock"),
    ("GET", "/stock/movimentos"),
    ("GET", "/clientes"),
    ("POST", "/clientes"),
    ("GET", "/clientes/{id}"),
    ("PUT", "/clientes/{id}"),
    ("PATCH", "/clientes/{id}"),
    ("GET", "/clientes/{id}/pendentes"),
    ("GET", "/fornecedores"),
    ("POST", "/fornecedores"),
    ("GET", "/fornecedores/{id}"),
    ("PUT", "/fornecedores/{id}"),
    ("PATCH", "/fornecedores/{id}"),
    ("GET", "/fornecedores/{id}/pendentes"),
    ("GET", "/documentos"),
    ("POST", "/documentos"),
    ("GET", "/documentos/{id}"),
    ("POST", "/documentos/{id}/anular"),
    ("GET", "/relatorios/vendas"),
    ("GET", "/saft"),
];

/// O caminho (em segmentos) encaixa no modelo da rota?
fn encaixa(modelo: &str, segs: &[&str]) -> bool {
    let partes: Vec<&str> = modelo.split('/').filter(|s| !s.is_empty()).collect();
    partes.len() == segs.len() && partes.iter().zip(segs).all(|(m, s)| (m.starts_with('{') && m.ends_with('}')) || m == s)
}

fn rota_existe(s: &[&str]) -> bool {
    ROTAS.iter().any(|(_, modelo)| encaixa(modelo, s))
}

/// Trata um pedido: (resposta, chave usada, se a chave foi aceite).
async fn processar(st: &AppState, p: &Pedido, ip: IpAddr) -> (Resposta, Option<i64>, bool) {
    let Some(resto) = p.caminho.strip_prefix("/api/v1") else {
        return (Resposta::erro(404, "nao_encontrado", "Caminho desconhecido. A API está em /api/v1."), None, false);
    };
    if !resto.is_empty() && !resto.starts_with('/') {
        return (Resposta::erro(404, "nao_encontrado", "Caminho desconhecido. A API está em /api/v1."), None, false);
    }
    let segs: Vec<String> = resto.split('/').filter(|s| !s.is_empty()).map(|s| descodificar(s, false)).collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    match (p.metodo.as_str(), segs.as_slice()) {
        ("GET", ["saude"]) => return (Resposta::json(200, &json!({ "ok": true, "api": "v1" })), None, false),
        ("GET", ["openapi.json"]) => {
            let mut r = Resposta::json(200, &Value::Null);
            r.corpo = OPENAPI.as_bytes().to_vec();
            return (r, None, false);
        }
        _ => {}
    }
    if !rota_existe(&segs) {
        return (Resposta::erro(404, "nao_encontrado", format!("Rota desconhecida: {} {}.", p.metodo, p.caminho)), None, false);
    }
    // Cloudflare Access (opcional): o que chega pela Cloudflare tem de vir autorizado por ela
    match ler_config(&st.pool).await {
        Ok(cfg) => {
            if let Err(r) = exigir_access(&cfg, p) {
                falha_ip(ip);
                return (r, None, false);
            }
        }
        Err(e) => return (Resposta::from(e), None, false),
    }
    if let Some(espera) = ip_bloqueado(ip) {
        return (
            Resposta::erro(429, "demasiadas_falhas", format!("Demasiadas chaves inválidas deste endereço. Tente daqui a {} min.", espera.div_ceil(60))).com("Retry-After", espera.to_string()),
            None,
            false,
        );
    }
    let Some(token) = extrair_chave(&p.cabecalhos) else {
        falha_ip(ip);
        return (
            Resposta::erro(401, "nao_autenticado", "Falta a chave de API: envie «Authorization: Bearer kg_…» ou «X-API-Key: kg_…».").com("WWW-Authenticate", "Bearer realm=\"KanGest\""),
            None,
            false,
        );
    };
    let chave = match autenticar(st, &token, ip).await {
        Ok(c) => c,
        Err((r, id)) => {
            if r.estado == 401 && id.is_none() {
                falha_ip(ip);
            }
            return (r, id, false);
        }
    };
    // a API faz parte do plano Empresa (ou do módulo «api» da licença)
    match services::licenca::calcular(st).await {
        Ok(e) => {
            if let Err(m) = services::licenca::api_incluida(&e) {
                return (Resposta::erro(403, "plano_sem_api", m), Some(chave.id), true);
            }
        }
        Err(e) => return (Resposta::from(e), Some(chave.id), true),
    }
    let restantes = match limite(&chave) {
        Ok(r) => r,
        Err(r) => return (r, Some(chave.id), true),
    };
    let mut r = match rotear(st, &chave, p, &segs).await {
        Ok(r) => r,
        Err(r) => r,
    };
    if let Some(n) = restantes {
        r = r.com("X-RateLimit-Limit", chave.limite_minuto.to_string()).com("X-RateLimit-Remaining", n.to_string());
    }
    (r, Some(chave.id), true)
}

type Res = Result<Resposta, Resposta>;

fn tipo_ent(t: &str) -> &'static str {
    if t == "fornecedores" {
        "F"
    } else {
        "C"
    }
}

fn perm_ent(t: &str) -> &'static str {
    if t == "fornecedores" {
        "fornecedores"
    } else {
        "clientes"
    }
}

async fn rotear(st: &AppState, c: &Chave, p: &Pedido, segs: &[&str]) -> Res {
    match (p.metodo.as_str(), segs) {
        ("GET", []) => info(st, c).await,
        ("GET", ["tabelas"]) => tabelas(st).await,
        ("GET", ["artigos"]) => {
            c.exigir("api.artigos")?;
            artigos(st, c, p).await
        }
        ("POST", ["artigos"]) => {
            c.exigir("artigos.criar")?;
            artigo_gravar(st, c, p, None).await
        }
        ("GET", ["artigos", "codigo", codigo]) => {
            c.exigir("api.artigos")?;
            artigo_codigo(st, c, p, codigo).await
        }
        ("GET", ["artigos", id, "foto"]) => {
            c.exigir("api.artigos")?;
            artigo_foto(st, id).await
        }
        ("GET", ["artigos", id]) => {
            c.exigir("api.artigos")?;
            artigo(st, c, p, id_de(id)?).await
        }
        ("PUT" | "PATCH", ["artigos", id]) => {
            c.exigir("artigos.editar")?;
            artigo_gravar(st, c, p, Some(id_de(id)?)).await
        }
        ("GET", ["stock"]) => {
            c.exigir("api.stock")?;
            stock(st, c, p).await
        }
        ("GET", ["stock", "movimentos"]) => {
            c.exigir("api.stock")?;
            movimentos(st, c, p).await
        }
        ("GET", [t @ ("clientes" | "fornecedores")]) => {
            c.exigir("api.entidades")?;
            entidades(st, p, tipo_ent(t)).await
        }
        ("POST", [t @ ("clientes" | "fornecedores")]) => {
            c.exigir(perm_ent(t))?;
            entidade_gravar(st, c, p, tipo_ent(t), None).await
        }
        ("GET", [t @ ("clientes" | "fornecedores"), id]) => {
            c.exigir("api.entidades")?;
            entidade(st, tipo_ent(t), id_de(id)?).await.map(|v| Resposta::json(200, &v))
        }
        ("PUT" | "PATCH", [t @ ("clientes" | "fornecedores"), id]) => {
            c.exigir(perm_ent(t))?;
            entidade_gravar(st, c, p, tipo_ent(t), Some(id_de(id)?)).await
        }
        ("GET", [t @ ("clientes" | "fornecedores"), id, "pendentes"]) => {
            c.exigir("api.entidades")?;
            let id = id_de(id)?;
            entidade(st, tipo_ent(t), id).await?;
            let v = executar(st, c, "entidades.pendentes", json!({ "id": id })).await?;
            Ok(Resposta::json(200, &json!({ "dados": v })))
        }
        ("GET", ["documentos"]) => {
            c.exigir("api.documentos")?;
            documentos(st, c, p).await
        }
        ("POST", ["documentos"]) => emitir(st, c, p).await,
        ("GET", ["documentos", id]) => {
            c.exigir("api.documentos")?;
            let v = executar(st, c, "documentos.obter", json!({ "id": id_de(id)? })).await?;
            Ok(Resposta::json(200, &formatar_documento(v, c)))
        }
        ("POST", ["documentos", id, "anular"]) => {
            c.exigir("documentos.anular")?;
            let id = id_de(id)?;
            let corpo = corpo_objeto(p)?;
            let motivo = corpo.get("motivo").and_then(Value::as_str).unwrap_or("").to_string();
            executar(st, c, "documentos.anular", json!({ "id": id, "motivo": motivo })).await?;
            let v = executar(st, c, "documentos.obter", json!({ "id": id })).await?;
            Ok(Resposta::json(200, &formatar_documento(v, c)))
        }
        ("GET", ["relatorios", "vendas"]) => {
            c.exigir("relatorios.ver")?;
            vendas(st, c, p).await
        }
        ("GET", ["saft"]) => {
            c.exigir("sistema.saft")?;
            saft(st, c, p).await
        }
        _ => Err(Resposta::erro(405, "metodo_nao_permitido", format!("O método {} não é aceite em {}.", p.metodo, p.caminho))),
    }
}

/// Fotografia do artigo (a imagem em si, para a loja online): 404 se não tiver.
async fn artigo_foto(st: &AppState, id: &str) -> Res {
    let id: i64 = id.parse().map_err(|_| Resposta::erro(404, "nao_encontrado", "Artigo não encontrado."))?;
    let d: Option<String> = sqlx::query_scalar("SELECT imagem FROM artigo_fotos WHERE artigo_id = ?")
        .bind(id)
        .fetch_optional(&st.pool)
        .await
        .map_err(|e| Resposta::erro(500, "erro_interno", e.to_string()))?;
    let d = d.ok_or_else(|| Resposta::erro(404, "nao_encontrado", "O artigo não tem fotografia."))?;
    let (tipo, bytes) = services::fotos::abrir_data_url(&d, usize::MAX / 2).map_err(|e| Resposta::erro(500, "erro_interno", e.to_string()))?;
    let tipo: &'static str = match tipo.as_str() {
        "image/png" => "image/png",
        "image/jpeg" => "image/jpeg",
        _ => "image/webp",
    };
    Ok(Resposta { estado: 200, tipo, corpo: bytes, extra: Vec::new(), erro: None })
}

// ============================== auxiliares das rotas ==============================

/// Corre um método do programa com a sessão e o posto da chave.
async fn executar(st: &AppState, c: &Chave, metodo: &str, params: Value) -> AppResult<Value> {
    CONTEXTO.scope(c.contexto(), services::dispatch(st, metodo, params)).await
}

fn id_de(s: &str) -> Result<i64, Resposta> {
    s.parse::<i64>().ok().filter(|i| *i > 0).ok_or_else(|| Resposta::erro(404, "nao_encontrado", format!("Identificador inválido: {s}")))
}

fn q<'a>(p: &'a Pedido, k: &str) -> Option<&'a str> {
    p.consulta.get(k).map(|s| s.trim()).filter(|v| !v.is_empty())
}

fn q_i64(p: &Pedido, k: &str) -> Result<Option<i64>, Resposta> {
    match q(p, k) {
        None => Ok(None),
        Some(v) => v.parse().map(Some).map_err(|_| Resposta::erro(400, "pedido_invalido", format!("O parâmetro «{k}» tem de ser um número."))),
    }
}

fn q_bool(p: &Pedido, k: &str) -> Option<bool> {
    q(p, k).map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "sim" | "s"))
}

fn q_data(p: &Pedido, k: &str) -> Result<Option<String>, Resposta> {
    match q(p, k) {
        None => Ok(None),
        Some(v) => {
            let v = v.replace(' ', "T");
            if chrono::NaiveDate::parse_from_str(v.get(..10).unwrap_or(""), "%Y-%m-%d").is_err() {
                return Err(Resposta::erro(400, "pedido_invalido", format!("O parâmetro «{k}» tem de ser uma data AAAA-MM-DD (ou AAAA-MM-DDTHH:MM:SS).")));
            }
            Ok(Some(v))
        }
    }
}

fn q_limite(p: &Pedido) -> Result<i64, Resposta> {
    Ok(q_i64(p, "limite")?.unwrap_or(100).clamp(1, 1000))
}

fn corpo_objeto(p: &Pedido) -> Result<Map<String, Value>, Resposta> {
    if p.corpo.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(&p.corpo) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(Resposta::erro(400, "pedido_invalido", "O corpo do pedido tem de ser um objecto JSON.")),
        Err(e) => Err(Resposta::erro(400, "pedido_invalido", format!("JSON inválido: {e}"))),
    }
}

fn validar_campos(m: &Map<String, Value>, permitidos: &[&str]) -> Result<(), Resposta> {
    let desconhecidos: Vec<&str> = m.keys().map(String::as_str).filter(|k| !permitidos.contains(k)).collect();
    if desconhecidos.is_empty() {
        Ok(())
    } else {
        Err(Resposta::erro(400, "campo_desconhecido", format!("Campo(s) desconhecido(s): {}.", desconhecidos.join(", "))))
    }
}

/// Página de uma lista ordenada por `id`: `{dados, mais, seguinte}`; `seguinte` é o `desde_id` da página seguinte.
fn pagina(mut linhas: Vec<Value>, limite: i64) -> Value {
    let mais = linhas.len() as i64 > limite;
    if mais {
        linhas.truncate(limite as usize);
    }
    let seguinte = if mais { linhas.last().and_then(|l| l["id"].as_i64()) } else { None };
    json!({ "dados": linhas, "mais": mais, "seguinte": seguinte })
}

const CAMPOS_CUSTO: &[&str] = &["custo", "custo_medio", "valor_custo", "lucro", "margem"];

/// Sem a permissão «api.custos», os preços de custo e as margens não saem da API.
fn custos(v: &mut Value, c: &Chave) {
    fn tirar(v: &mut Value) {
        match v {
            Value::Object(m) => {
                for k in CAMPOS_CUSTO {
                    m.remove(*k);
                }
                m.values_mut().for_each(tirar);
            }
            Value::Array(a) => a.iter_mut().for_each(tirar),
            _ => {}
        }
    }
    if !c.tem("api.custos") {
        tirar(v);
    }
}

/// Colunas 0/1 passam a `true`/`false`.
fn booleanos(v: &mut Value, chaves: &[&str]) {
    if let Value::Object(m) = v {
        for k in chaves {
            if let Some(x) = m.get_mut(*k) {
                if let Some(n) = x.as_i64() {
                    *x = Value::Bool(n != 0);
                }
            }
        }
    }
}

// ============================== rotas ==============================

async fn info(st: &AppState, c: &Chave) -> Res {
    let emp: Option<(String, String)> = sqlx::query_as("SELECT nome, nif FROM empresa WHERE id = 1").fetch_optional(&st.pool).await?;
    let (nome, nif) = emp.unwrap_or_default();
    Ok(Resposta::json(
        200,
        &json!({
            "app": "KanGest",
            "versao": env!("CARGO_PKG_VERSION"),
            "api": "v1",
            "hora": db::agora(),
            "empresa": { "nome": nome, "nif": nif },
            "chave": {
                "id": c.id, "nome": c.nome, "prefixo": c.prefixo, "permissoes": c.permissoes,
                "limite_minuto": c.limite_minuto, "utilizador": c.sessao.nome,
            },
            "posto": { "id": c.posto.id, "descricao": c.posto.descricao, "armazem_id": c.posto.armazem_id, "serie_prefixo": c.posto.serie_prefixo },
        }),
    ))
}

async fn tabelas(st: &AppState) -> Res {
    let p = &st.pool;
    Ok(Resposta::json(
        200,
        &json!({
            "familias": db::query_json(p, "SELECT id, descricao FROM familias ORDER BY descricao").await?,
            "subfamilias": db::query_json(p, "SELECT id, familia_id, descricao FROM subfamilias ORDER BY descricao").await?,
            "unidades": db::query_json(p, "SELECT id, descricao, abreviatura FROM unidades ORDER BY descricao").await?,
            "taxas_iva": db::query_json(p, "SELECT id, codigo, descricao, percentagem FROM taxas_iva WHERE ativo = 1 ORDER BY percentagem DESC").await?,
            "motivos_isencao": db::query_json(p, "SELECT codigo, descricao FROM motivos_isencao ORDER BY codigo").await?,
            "armazens": db::query_json(p, "SELECT id, nome FROM armazens WHERE ativo = 1 ORDER BY id").await?,
            "modos_pagamento": db::query_json(p, "SELECT codigo, descricao FROM modos_pagamento WHERE ativo = 1 ORDER BY ordem").await?,
            "tipos_documento": db::query_json(p, "SELECT codigo, descricao, classe, grupo FROM tipos_documento WHERE ativo = 1 ORDER BY ordem").await?,
            "prazos_pagamento": db::query_json(p, "SELECT codigo, descricao, dias FROM prazos_pagamento ORDER BY dias").await?,
            "retencoes": db::query_json(p, "SELECT codigo, descricao, percentagem, base FROM retencoes WHERE ativo = 1 ORDER BY codigo").await?,
            "moedas": db::query_json(p, "SELECT codigo, descricao, simbolo, cambio FROM moedas WHERE ativo = 1 ORDER BY codigo").await?,
        }),
    ))
}

// ---------------------------------------------------------------- artigos

const ARTIGO_SQL: &str = "
    SELECT a.id, a.referencia, a.codigo_barras, a.codigo_balanca, a.descricao, a.descricao_curta, a.tipo,
           a.familia_id, f.descricao AS familia, a.subfamilia_id, sf.descricao AS subfamilia,
           a.unidade_id, u.abreviatura AS unidade, a.taxa_iva_id, t.percentagem AS taxa_iva, a.motivo_isencao,
           a.preco1, a.preco2, a.preco3, a.preco4,
           CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco1_iva,
           CAST(ROUND(a.preco2 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco2_iva,
           CAST(ROUND(a.preco3 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco3_iva,
           CAST(ROUND(a.preco4 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco4_iva,
           a.promo_desconto, a.promo_inicio, a.promo_fim,
           CASE WHEN a.promo_desconto > 0 AND COALESCE(a.promo_inicio, '') <= date('now', 'localtime')
                     AND (a.promo_fim IS NULL OR a.promo_fim = '' OR a.promo_fim >= date('now', 'localtime'))
                THEN a.promo_desconto ELSE 0 END AS promo_atual,
           a.qtd_minima, a.qtd_maxima, a.controla_stock, a.permite_negativo, a.controla_lote, a.composto, a.ativo,
           a.custo, a.custo_medio, ?1 AS armazem_id, COALESCE(s.quantidade, 0) AS stock,
           a.atributos, EXISTS (SELECT 1 FROM artigo_fotos fo WHERE fo.artigo_id = a.id) AS tem_foto, a.criado_em, a.alterado_em
    FROM artigos a
    JOIN taxas_iva t ON t.id = a.taxa_iva_id
    LEFT JOIN unidades u ON u.id = a.unidade_id
    LEFT JOIN familias f ON f.id = a.familia_id
    LEFT JOIN subfamilias sf ON sf.id = a.subfamilia_id
    LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1";

const BOOL_ARTIGO: &[&str] = &["controla_stock", "permite_negativo", "controla_lote", "composto", "ativo", "tem_foto"];

fn artigo_json(row: &sqlx::sqlite::SqliteRow, c: &Chave) -> Value {
    let mut v = db::row_json(row);
    booleanos(&mut v, BOOL_ARTIGO);
    custos(&mut v, c);
    // dados do ramo (ISBN/autor, tamanho/cor, marca/modelo…): objecto, não texto
    v["atributos"] = v["atributos"].as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or_else(|| json!({}));
    v
}

fn armazem_de(p: &Pedido, c: &Chave) -> Result<i64, Resposta> {
    Ok(q_i64(p, "armazem_id")?.unwrap_or(c.posto.armazem_id))
}

async fn artigos(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let limite = q_limite(p)?;
    let alterado = q_data(p, "alterado_desde")?;
    // na sincronização (alterado_desde) os desactivados também vêm, para o outro sistema os esconder
    let inativos = q_bool(p, "inativos").unwrap_or(alterado.is_some());
    let rows = sqlx::query(&format!(
        "{ARTIGO_SQL}
         WHERE a.id > ?2
           AND (?3 IS NULL OR a.descricao LIKE ?3 OR a.referencia LIKE ?3 OR IFNULL(a.codigo_barras, '') LIKE ?3)
           AND (?4 IS NULL OR a.familia_id = ?4)
           AND (?5 = 1 OR a.ativo = 1)
           AND (?6 IS NULL OR a.alterado_em >= ?6)
         ORDER BY a.id LIMIT ?7"
    ))
    .bind(armazem_de(p, c)?)
    .bind(q_i64(p, "desde_id")?.unwrap_or(0))
    .bind(q(p, "q").map(|x| format!("%{x}%")))
    .bind(q_i64(p, "familia_id")?)
    .bind(inativos as i64)
    .bind(alterado)
    .bind(limite + 1)
    .fetch_all(&st.pool)
    .await?;
    Ok(Resposta::json(200, &pagina(rows.iter().map(|r| artigo_json(r, c)).collect(), limite)))
}

async fn artigo(st: &AppState, c: &Chave, p: &Pedido, id: i64) -> Res {
    let row = sqlx::query(&format!("{ARTIGO_SQL} WHERE a.id = ?2"))
        .bind(armazem_de(p, c)?)
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| Resposta::erro(404, "nao_encontrado", format!("O artigo {id} não existe.")))?;
    Ok(Resposta::json(200, &artigo_json(&row, c)))
}

/// Pelo código de barras ou referência, por um código extra (embalagem) ou pela etiqueta da balança:
/// `leitura.quantidade` diz quanto vale a leitura quando não é 1.
async fn artigo_codigo(st: &AppState, c: &Chave, p: &Pedido, codigo: &str) -> Res {
    let (id, leitura) = services::catalogo::ler_codigo(&st.pool, codigo)
        .await?
        .ok_or_else(|| Resposta::erro(404, "nao_encontrado", format!("Nenhum artigo activo com o código {codigo}.")))?;
    let row = sqlx::query(&format!("{ARTIGO_SQL} WHERE a.id = ?2")).bind(armazem_de(p, c)?).bind(id).fetch_one(&st.pool).await?;
    let mut v = artigo_json(&row, c);
    if let Some(l) = leitura {
        v["leitura"] = l;
    }
    Ok(Resposta::json(200, &v))
}

const CAMPOS_ARTIGO: &[&str] = &[
    "referencia",
    "codigo_barras",
    "codigo_balanca",
    "descricao",
    "descricao_curta",
    "tipo",
    "familia_id",
    "subfamilia_id",
    "unidade_id",
    "taxa_iva_id",
    "motivo_isencao",
    "custo",
    "preco1",
    "preco2",
    "preco3",
    "preco4",
    "preco1_iva",
    "preco2_iva",
    "preco3_iva",
    "preco4_iva",
    "qtd_minima",
    "qtd_maxima",
    "controla_stock",
    "permite_negativo",
    "ativo",
    "controla_lote",
    "aviso_validade_dias",
    "composto",
    "componentes",
    "promo_desconto",
    "promo_inicio",
    "promo_fim",
    "atributos",
];

/// Preço sem IVA (cêntimos) a partir do preço com IVA.
fn sem_iva(com_iva: f64, taxa: f64) -> i64 {
    (com_iva * 100.0 / (100.0 + taxa)).round() as i64
}

/// Cria (`id` = None) ou altera um artigo. Na alteração, os campos omitidos ficam como estão.
async fn artigo_gravar(st: &AppState, c: &Chave, p: &Pedido, id: Option<i64>) -> Res {
    let corpo = corpo_objeto(p)?;
    validar_campos(&corpo, CAMPOS_ARTIGO)?;
    for k in ["custo", "preco1", "preco2", "preco3", "preco4", "preco1_iva", "preco2_iva", "preco3_iva", "preco4_iva"] {
        if !valor_ok(corpo.get(k), 1e13) {
            return Err(Resposta::erro(422, "validacao", format!("{k} inválido (cêntimos, não negativo, até 100 mil milhões de Kz).")));
        }
    }
    if !valor_ok(corpo.get("qtd_minima"), MAX_QTD) || !valor_ok(corpo.get("qtd_maxima"), MAX_QTD) || !valor_ok(corpo.get("promo_desconto"), 100.0) {
        return Err(Resposta::erro(422, "validacao", "Quantidade mínima/máxima ou desconto da promoção inválido."));
    }
    let mut params = match id {
        None => Map::new(),
        Some(id) => {
            let row = sqlx::query("SELECT * FROM artigos WHERE id = ?")
                .bind(id)
                .fetch_optional(&st.pool)
                .await?
                .ok_or_else(|| Resposta::erro(404, "nao_encontrado", format!("O artigo {id} não existe.")))?;
            let mut atual = db::row_json(&row);
            booleanos(&mut atual, BOOL_ARTIGO);
            let mut m: Map<String, Value> = atual.as_object().cloned().unwrap_or_default().into_iter().filter(|(k, _)| CAMPOS_ARTIGO.contains(&k.as_str())).collect();
            m.insert("id".into(), json!(id));
            m
        }
    };
    for (k, v) in &corpo {
        if !k.ends_with("_iva") {
            params.insert(k.clone(), v.clone());
        }
    }
    // preços com IVA → sem IVA, à taxa do artigo
    let taxa_id = params.get("taxa_iva_id").and_then(Value::as_i64).ok_or_else(|| Resposta::erro(422, "validacao", "Indique a taxa de IVA (taxa_iva_id; ver GET /api/v1/tabelas)."))?;
    let taxa: Option<f64> = sqlx::query_scalar("SELECT percentagem FROM taxas_iva WHERE id = ?").bind(taxa_id).fetch_optional(&st.pool).await?;
    let taxa = taxa.ok_or_else(|| Resposta::erro(422, "validacao", format!("Taxa de IVA inválida: {taxa_id}.")))?;
    for n in 1..=4 {
        if let Some(v) = corpo.get(&format!("preco{n}_iva")).filter(|v| !v.is_null()) {
            if corpo.contains_key(&format!("preco{n}")) {
                return Err(Resposta::erro(422, "validacao", format!("Indique preco{n} ou preco{n}_iva, não os dois.")));
            }
            let x = v.as_f64().filter(|x| *x >= 0.0).ok_or_else(|| Resposta::erro(422, "validacao", format!("preco{n}_iva tem de ser um valor em cêntimos.")))?;
            params.insert(format!("preco{n}"), json!(sem_iva(x, taxa)));
        }
    }
    if !params.contains_key("preco1") {
        return Err(Resposta::erro(422, "validacao", "Indique o preço: preco1 (sem IVA) ou preco1_iva (com IVA), em cêntimos."));
    }
    let r = executar(st, c, "artigos.guardar", Value::Object(params)).await?;
    let novo = r["id"].as_i64().unwrap_or_default();
    let artigo = sqlx::query(&format!("{ARTIGO_SQL} WHERE a.id = ?2")).bind(c.posto.armazem_id).bind(novo).fetch_one(&st.pool).await?;
    let corpo = artigo_json(&artigo, c);
    Ok(if id.is_none() { Resposta::json(201, &corpo).com("Location", format!("/api/v1/artigos/{novo}")) } else { Resposta::json(200, &corpo) })
}

// ---------------------------------------------------------------- stock

async fn stock(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let limite = q_limite(p)?;
    let armazem = q_i64(p, "armazem_id")?;
    let rows = sqlx::query(
        "SELECT a.id, a.referencia, a.codigo_barras, a.descricao, u.abreviatura AS unidade, a.qtd_minima, a.qtd_maxima, a.custo_medio
         FROM artigos a LEFT JOIN unidades u ON u.id = a.unidade_id
         WHERE a.controla_stock = 1 AND a.composto = 0 AND a.ativo = 1 AND a.id > ?1
           AND (?2 IS NULL OR a.id = ?2)
           AND (?3 IS NULL OR a.descricao LIKE ?3 OR a.referencia LIKE ?3 OR IFNULL(a.codigo_barras, '') LIKE ?3)
           AND (?4 = 0 OR COALESCE((SELECT SUM(s.quantidade) FROM stock s WHERE s.artigo_id = a.id AND (?5 IS NULL OR s.armazem_id = ?5)), 0) <= a.qtd_minima)
         ORDER BY a.id LIMIT ?6",
    )
    .bind(q_i64(p, "desde_id")?.unwrap_or(0))
    .bind(q_i64(p, "artigo_id")?)
    .bind(q(p, "q").map(|x| format!("%{x}%")))
    .bind(q_bool(p, "abaixo_minimo").unwrap_or(false) as i64)
    .bind(armazem)
    .bind(limite + 1)
    .fetch_all(&st.pool)
    .await?;
    let ids: Vec<i64> = rows.iter().take(limite as usize).map(|r| r.get("id")).collect();
    let mut por_artigo: HashMap<i64, Vec<Value>> = HashMap::new();
    let mut reservas: HashMap<(i64, i64), f64> = HashMap::new();
    if !ids.is_empty() {
        let lista = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        // reservado: notas de encomenda de clientes por facturar
        for r in sqlx::query(&format!(
            "SELECT l.artigo_id, COALESCE(d.armazem_id, 1) AS armazem_id, SUM(l.quantidade) AS reservado
             FROM documentos d JOIN documento_linhas l ON l.documento_id = d.id
             WHERE d.tipo_codigo = 'NE' AND d.estado = 'N' AND l.artigo_id IN ({lista}) GROUP BY 1, 2"
        ))
        .fetch_all(&st.pool)
        .await?
        {
            reservas.insert((r.get("artigo_id"), r.get("armazem_id")), r.get("reservado"));
        }
        for r in sqlx::query(&format!(
            "SELECT s.artigo_id, s.armazem_id, ar.nome AS armazem, s.quantidade, s.ultima_entrada, s.ultima_saida
             FROM stock s JOIN armazens ar ON ar.id = s.armazem_id
             WHERE s.artigo_id IN ({lista}) AND (?1 IS NULL OR s.armazem_id = ?1) ORDER BY s.armazem_id"
        ))
        .bind(armazem)
        .fetch_all(&st.pool)
        .await?
        {
            let (aid, arm): (i64, i64) = (r.get("artigo_id"), r.get("armazem_id"));
            let qtd: f64 = r.get("quantidade");
            let res = reservas.get(&(aid, arm)).copied().unwrap_or(0.0);
            por_artigo.entry(aid).or_default().push(json!({
                "armazem_id": arm, "armazem": r.get::<String, _>("armazem"), "quantidade": qtd, "reservado": res, "disponivel": qtd - res,
                "ultima_entrada": r.get::<Option<String>, _>("ultima_entrada"), "ultima_saida": r.get::<Option<String>, _>("ultima_saida"),
            }));
        }
    }
    let linhas: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let armazens = por_artigo.remove(&id).unwrap_or_default();
            let soma = |k: &str| armazens.iter().map(|a| a[k].as_f64().unwrap_or(0.0)).sum::<f64>();
            let mut v = json!({
                "id": id, "referencia": r.get::<String, _>("referencia"), "codigo_barras": r.get::<Option<String>, _>("codigo_barras"),
                "descricao": r.get::<String, _>("descricao"), "unidade": r.get::<Option<String>, _>("unidade"),
                "qtd_minima": r.get::<f64, _>("qtd_minima"), "qtd_maxima": r.get::<f64, _>("qtd_maxima"),
                "quantidade": soma("quantidade"), "reservado": soma("reservado"), "disponivel": soma("disponivel"),
                "custo_medio": r.get::<i64, _>("custo_medio"), "armazens": armazens,
            });
            custos(&mut v, c);
            v
        })
        .collect();
    Ok(Resposta::json(200, &pagina(linhas, limite)))
}

async fn movimentos(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let limite = q_limite(p)?;
    let rows = sqlx::query(
        "SELECT m.id, m.data_hora, m.artigo_id, a.referencia, a.descricao, m.armazem_id, m.quantidade, m.saldo, m.custo,
                m.documento_id, d.numero_doc, m.descricao AS movimento
         FROM movimentos_stock m JOIN artigos a ON a.id = m.artigo_id LEFT JOIN documentos d ON d.id = m.documento_id
         WHERE m.id > ?1 AND (?2 IS NULL OR m.artigo_id = ?2) AND (?3 IS NULL OR m.armazem_id = ?3) AND (?4 IS NULL OR m.data_hora >= ?4)
         ORDER BY m.id LIMIT ?5",
    )
    .bind(q_i64(p, "desde_id")?.unwrap_or(0))
    .bind(q_i64(p, "artigo_id")?)
    .bind(q_i64(p, "armazem_id")?)
    .bind(q_data(p, "de")?)
    .bind(limite + 1)
    .fetch_all(&st.pool)
    .await?;
    let mut v = pagina(rows.iter().map(db::row_json).collect(), limite);
    custos(&mut v, c);
    Ok(Resposta::json(200, &v))
}

// ---------------------------------------------------------------- clientes e fornecedores

const ENTIDADE_SQL: &str = "
    SELECT e.id, e.tipo, e.nome, e.nif, e.endereco, e.localidade, e.provincia, e.pais, e.telefone, e.email,
           e.limite_credito, e.desconto, e.nivel_preco, e.estado, e.ativo, e.observacoes, e.criado_em, e.alterado_em,
           (SELECT COALESCE(SUM(d.valor_pendente), 0) - COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d
             WHERE d.entidade_id = e.id AND d.estado = 'N') AS saldo,
           (SELECT COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d WHERE d.entidade_id = e.id AND d.estado = 'N') AS credito
    FROM entidades e";

const BOOL_ENTIDADE: &[&str] = &["estado", "ativo"];

async fn entidades(st: &AppState, p: &Pedido, tipo: &str) -> Res {
    let limite = q_limite(p)?;
    let alterado = q_data(p, "alterado_desde")?;
    let inativos = q_bool(p, "inativos").unwrap_or(alterado.is_some());
    let rows = sqlx::query(&format!(
        "{ENTIDADE_SQL}
         WHERE e.id > ?1 AND (e.tipo = ?2 OR e.tipo = 'A')
           AND (?3 IS NULL OR e.nome LIKE ?3 OR e.nif LIKE ?3 OR IFNULL(e.telefone, '') LIKE ?3 OR IFNULL(e.email, '') LIKE ?3)
           AND (?4 IS NULL OR e.nif = ?4) AND (?5 = 1 OR e.ativo = 1) AND (?6 IS NULL OR e.alterado_em >= ?6)
         ORDER BY e.id LIMIT ?7"
    ))
    .bind(q_i64(p, "desde_id")?.unwrap_or(0))
    .bind(tipo)
    .bind(q(p, "q").map(|x| format!("%{x}%")))
    .bind(q(p, "nif"))
    .bind(inativos as i64)
    .bind(alterado)
    .bind(limite + 1)
    .fetch_all(&st.pool)
    .await?;
    let linhas = rows
        .iter()
        .map(|r| {
            let mut v = db::row_json(r);
            booleanos(&mut v, BOOL_ENTIDADE);
            v
        })
        .collect();
    Ok(Resposta::json(200, &pagina(linhas, limite)))
}

async fn entidade(st: &AppState, tipo: &str, id: i64) -> Result<Value, Resposta> {
    let nao = || Resposta::erro(404, "nao_encontrado", format!("{} {id} não existe.", if tipo == "F" { "O fornecedor" } else { "O cliente" }));
    let row = sqlx::query(&format!("{ENTIDADE_SQL} WHERE e.id = ?1 AND (e.tipo = ?2 OR e.tipo = 'A')"))
        .bind(id)
        .bind(tipo)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(nao)?;
    let mut v = db::row_json(&row);
    booleanos(&mut v, BOOL_ENTIDADE);
    Ok(v)
}

const CAMPOS_ENTIDADE: &[&str] =
    &["tipo", "nome", "nif", "endereco", "localidade", "provincia", "pais", "estado", "observacoes", "telefone", "email", "limite_credito", "desconto", "nivel_preco"];

/// Cria (`id` = None) ou altera um cliente/fornecedor. Na alteração, os campos omitidos ficam como estão.
async fn entidade_gravar(st: &AppState, c: &Chave, p: &Pedido, tipo: &str, id: Option<i64>) -> Res {
    let corpo = corpo_objeto(p)?;
    validar_campos(&corpo, CAMPOS_ENTIDADE)?;
    if !valor_ok(corpo.get("limite_credito"), MAX_VALOR) || !valor_ok(corpo.get("desconto"), 100.0) {
        return Err(Resposta::erro(422, "validacao", "limite_credito (cêntimos) ou desconto (0 a 100) inválido."));
    }
    if let Some(t) = corpo.get("tipo") {
        if t.as_str() != Some(tipo) && t.as_str() != Some("A") {
            return Err(Resposta::erro(422, "validacao", format!("O tipo tem de ser «{tipo}» ou «A» (cliente e fornecedor).")));
        }
    }
    let mut params = match id {
        None => {
            let mut m = Map::new();
            m.insert("tipo".into(), json!(tipo));
            m
        }
        Some(id) => {
            let atual = entidade(st, tipo, id).await?;
            let mut m: Map<String, Value> = atual.as_object().cloned().unwrap_or_default().into_iter().filter(|(k, _)| CAMPOS_ENTIDADE.contains(&k.as_str())).collect();
            m.insert("id".into(), json!(id));
            m
        }
    };
    for (k, v) in corpo {
        params.insert(k, v);
    }
    let r = executar(st, c, "entidades.guardar", Value::Object(params)).await?;
    let novo = r["id"].as_i64().unwrap_or_default();
    let v = entidade(st, tipo, novo).await?;
    let caminho = if tipo == "F" { "fornecedores" } else { "clientes" };
    Ok(if id.is_none() { Resposta::json(201, &v).com("Location", format!("/api/v1/{caminho}/{novo}")) } else { Resposta::json(200, &v) })
}

// ---------------------------------------------------------------- documentos

async fn documentos(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let limite = q_limite(p)?;
    let tipos = q(p, "tipo").map(|t| t.to_uppercase().replace(' ', ""));
    let rows = sqlx::query(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_descricao, t.grupo, d.serie, d.numero,
                d.data_emissao, d.data_hora, d.data_vencimento, d.entidade_id, d.entidade_nome, d.entidade_nif,
                d.total_bruto, d.total_desconto, d.total_liquido, d.total_iva, d.total_retencao, d.total, d.valor_pendente,
                d.estado, d.motivo_anulacao, d.hash_controlo, d.doc_origem_id, d.referencia, d.observacoes, d.moeda, d.cambio,
                d.posto_id, d.armazem_id, d.vendedor_id, d.api_chave_id, d.origem, d.importado, u.nome AS utilizador
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo JOIN utilizadores u ON u.id = d.utilizador_id
         WHERE d.id > ?1
           AND (?2 IS NULL OR instr(',' || ?2 || ',', ',' || d.tipo_codigo || ',') > 0)
           AND (?3 IS NULL OR t.grupo = ?3)
           AND (?4 IS NULL OR d.data_emissao >= ?4) AND (?5 IS NULL OR d.data_emissao <= ?5)
           AND (?6 IS NULL OR d.entidade_id = ?6) AND (?7 IS NULL OR d.estado = ?7)
           AND (?8 IS NULL OR d.api_chave_id = ?8)
         ORDER BY d.id LIMIT ?9",
    )
    .bind(q_i64(p, "desde_id")?.unwrap_or(0))
    .bind(tipos)
    .bind(q(p, "grupo").map(str::to_uppercase))
    .bind(q_data(p, "de")?.map(|d| d.chars().take(10).collect::<String>()))
    .bind(q_data(p, "ate")?.map(|d| d.chars().take(10).collect::<String>()))
    .bind(q_i64(p, "entidade_id")?)
    .bind(q(p, "estado").map(str::to_uppercase))
    .bind(q_bool(p, "desta_chave").unwrap_or(false).then_some(c.id))
    .bind(limite + 1)
    .fetch_all(&st.pool)
    .await?;
    let mut linhas: Vec<Value> = rows.iter().map(db::row_json).collect();
    if q_bool(p, "linhas").unwrap_or(false) && !linhas.is_empty() {
        let ids: Vec<i64> = linhas.iter().take(limite as usize).filter_map(|d| d["id"].as_i64()).collect();
        let lista = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let mut det: HashMap<i64, Vec<Value>> = HashMap::new();
        for r in sqlx::query(&format!(
            "SELECT documento_id, linha, artigo_id, referencia, descricao, quantidade, unidade, preco_unitario, desconto_pct,
                    valor_desconto, taxa_iva_pct, motivo_isencao, valor_liquido, valor_iva, total, lote, validade, custo
             FROM documento_linhas WHERE documento_id IN ({lista}) ORDER BY documento_id, linha"
        ))
        .fetch_all(&st.pool)
        .await?
        {
            det.entry(r.get("documento_id")).or_default().push(db::row_json(&r));
        }
        let mut pags: HashMap<i64, Vec<Value>> = HashMap::new();
        for r in sqlx::query(&format!("SELECT documento_id, modo_codigo, valor FROM documento_pagamentos WHERE documento_id IN ({lista}) ORDER BY id"))
            .fetch_all(&st.pool)
            .await?
        {
            pags.entry(r.get("documento_id")).or_default().push(json!({ "modo_codigo": r.get::<String, _>("modo_codigo"), "valor": r.get::<i64, _>("valor") }));
        }
        for d in linhas.iter_mut() {
            let id = d["id"].as_i64().unwrap_or_default();
            d["linhas"] = json!(det.remove(&id).unwrap_or_default());
            d["pagamentos"] = json!(pags.remove(&id).unwrap_or_default());
        }
    }
    let mut v = pagina(linhas, limite);
    custos(&mut v, c);
    Ok(Resposta::json(200, &v))
}

/// Documento completo para a API: campos do documento + linhas, pagamentos, liquidações, retenções, IVA e AGT.
fn formatar_documento(v: Value, c: &Chave) -> Value {
    let mut doc = v.get("documento").cloned().unwrap_or(Value::Null);
    let Value::Object(m) = &mut doc else { return doc };
    for k in ["linhas", "pagamentos", "liquidacoes", "retencoes", "resumo_iva"] {
        m.insert(k.into(), v.get(k).cloned().unwrap_or(json!([])));
    }
    if c.tem("api.custos") {
        // custos adicionais de compra (frete, desalfandegamento…)
        m.insert("custos_adicionais".into(), v.get("custos").cloned().unwrap_or(json!([])));
    }
    if let Some(fe) = v.get("fe").filter(|f| f.is_object()) {
        let numero = m.get("numero_doc").and_then(Value::as_str).unwrap_or("").to_string();
        let nif = fe["nif"].as_str().unwrap_or("");
        m.insert("fe".into(), json!({ "estado": fe["estado"], "url_consulta": crate::services::impressao::url_agt(nif, &numero) }));
    }
    custos(&mut doc, c);
    doc
}

/// Maior valor monetário aceite num campo (cêntimos): 10 biliões de Kz. Com as quantidades e o número de linhas
/// também limitados, os totais nunca transbordam (`i64`) nem chegam absurdos a um documento assinado.
const MAX_VALOR: f64 = 1e15;
const MAX_QTD: f64 = 10_000_000.0;
const MAX_LINHAS: usize = 2000;

fn valor_ok(v: Option<&Value>, max: f64) -> bool {
    match v.filter(|v| !v.is_null()) {
        None => true,
        Some(v) => v.as_f64().is_some_and(|x| x.is_finite() && (0.0..=max).contains(&x)),
    }
}

/// Limites de sanidade dos valores vindos de fora, antes de chegarem ao motor de emissão.
fn validar_valores(corpo: &Map<String, Value>) -> Result<(), Resposta> {
    let erro = |m: String| Err(Resposta::erro(422, "validacao", m));
    if let Some(l) = corpo.get("linhas").filter(|v| !v.is_null()) {
        let Some(linhas) = l.as_array() else { return erro("«linhas» tem de ser uma lista.".into()) };
        if linhas.len() > MAX_LINHAS {
            return erro(format!("No máximo {MAX_LINHAS} linhas por documento."));
        }
        for (n, x) in linhas.iter().enumerate() {
            let n = n + 1;
            let q = x.get("quantidade").and_then(Value::as_f64).filter(|q| q.is_finite() && q.abs() <= MAX_QTD);
            let Some(q) = q else { return erro(format!("Linha {n}: quantidade inválida (um número até 10 000 000).")) };
            for k in ["preco_unitario", "preco_com_iva"] {
                let p = x.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                if !valor_ok(x.get(k), MAX_VALOR) || p * q.abs() > MAX_VALOR {
                    return erro(format!("Linha {n}: {k} inválido (cêntimos, não negativo)."));
                }
            }
            if !valor_ok(x.get("desconto_pct"), 100.0) {
                return erro(format!("Linha {n}: desconto_pct tem de estar entre 0 e 100."));
            }
        }
    }
    for lista in ["pagamentos", "liquidar", "custos", "retencoes"] {
        if let Some(v) = corpo.get(lista).filter(|v| !v.is_null()) {
            let Some(a) = v.as_array() else { return erro(format!("«{lista}» tem de ser uma lista.")) };
            if a.len() > 500 || a.iter().any(|x| !valor_ok(x.get("valor"), MAX_VALOR) || !valor_ok(x.get("percentagem"), 100.0)) {
                return erro(format!("Valores inválidos em «{lista}»."));
            }
        }
    }
    if !valor_ok(corpo.get("desconto_global_valor"), MAX_VALOR) || !valor_ok(corpo.get("valor"), MAX_VALOR) || !valor_ok(corpo.get("desconto_global_pct"), 100.0) {
        return erro("Valor ou desconto global inválido.".into());
    }
    if !valor_ok(corpo.get("cambio"), 1e9) {
        return erro("Câmbio inválido.".into());
    }
    Ok(())
}

/// Campos da linha que a API aceita além dos do programa: o artigo pela referência ou pelo código de barras
/// e o preço com IVA.
async fn resolver_linhas(st: &AppState, corpo: &mut Map<String, Value>) -> Result<(), Resposta> {
    let Some(linhas) = corpo.get_mut("linhas").and_then(Value::as_array_mut) else { return Ok(()) };
    for (n, l) in linhas.iter_mut().enumerate() {
        let n = n + 1;
        let o = l.as_object_mut().ok_or_else(|| Resposta::erro(422, "validacao", format!("A linha {n} não é um objecto.")))?;
        if o.get("artigo_id").is_none_or(Value::is_null) {
            let id = match (o.get("referencia").and_then(Value::as_str), o.get("codigo_barras").and_then(Value::as_str)) {
                (Some(r), _) => {
                    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ? AND ativo = 1 ORDER BY id LIMIT 1").bind(r.trim()).fetch_optional(&st.pool).await?;
                    id.ok_or_else(|| Resposta::erro(422, "validacao", format!("Linha {n}: nenhum artigo activo com a referência «{}».", r.trim())))?
                }
                // código de barras: também os das embalagens (a quantidade multiplica pelas unidades) e as etiquetas da balança
                (None, Some(cb)) => {
                    let (id, leitura) = services::catalogo::ler_codigo(&st.pool, cb)
                        .await?
                        .ok_or_else(|| Resposta::erro(422, "validacao", format!("Linha {n}: nenhum artigo activo com o código de barras «{}».", cb.trim())))?;
                    if let Some(f) = leitura.as_ref().and_then(|l| l["quantidade"].as_f64()) {
                        let q = o.get("quantidade").and_then(Value::as_f64).unwrap_or(1.0);
                        o.insert("quantidade".into(), json!(((q * f) * 1000.0).round() / 1000.0));
                    }
                    id
                }
                _ => return Err(Resposta::erro(422, "validacao", format!("Linha {n} sem artigo: indique artigo_id, referencia ou codigo_barras."))),
            };
            o.insert("artigo_id".into(), json!(id));
        }
        if let Some(piva) = o.remove("preco_com_iva").filter(|v| !v.is_null()) {
            if o.get("preco_unitario").is_some_and(|v| !v.is_null()) {
                return Err(Resposta::erro(422, "validacao", format!("Linha {n}: indique preco_unitario ou preco_com_iva, não os dois.")));
            }
            let piva = piva.as_f64().filter(|x| *x >= 0.0).ok_or_else(|| Resposta::erro(422, "validacao", format!("Linha {n}: preco_com_iva tem de ser um valor em cêntimos.")))?;
            let taxa: Option<f64> = sqlx::query_scalar("SELECT t.percentagem FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.id = ?")
                .bind(o.get("artigo_id").and_then(Value::as_i64))
                .fetch_optional(&st.pool)
                .await?;
            let taxa = taxa.ok_or_else(|| Resposta::erro(422, "validacao", format!("Linha {n}: artigo não encontrado.")))?;
            o.insert("preco_unitario".into(), json!(sem_iva(piva, taxa)));
        }
        o.remove("referencia");
        o.remove("codigo_barras");
    }
    Ok(())
}

/// Cliente indicado por NIF (`cliente: {nif, nome, …}`): usa o que existe ou cria-o (exige a permissão «clientes»).
/// Sem NIF, a venda é a Consumidor Final.
async fn resolver_cliente(st: &AppState, c: &Chave, corpo: &mut Map<String, Value>) -> Result<(), Resposta> {
    if corpo.get("entidade_id").is_some_and(|v| !v.is_null()) {
        corpo.remove("cliente");
        return Ok(());
    }
    let Some(cli) = corpo.remove("cliente").filter(|v| !v.is_null()) else { return Ok(()) };
    let o = cli.as_object().ok_or_else(|| Resposta::erro(422, "validacao", "«cliente» tem de ser um objecto {nif, nome, …}."))?;
    let nif = o.get("nif").and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty()).unwrap_or("999999999").to_string();
    if nif == "999999999" {
        corpo.insert("entidade_id".into(), json!(1));
        return Ok(());
    }
    let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM entidades WHERE nif = ? AND ativo = 1 AND tipo IN ('C', 'A') ORDER BY id LIMIT 1")
        .bind(&nif)
        .fetch_optional(&st.pool)
        .await?;
    let id = match existe {
        Some(id) => id,
        None => {
            c.exigir("clientes")?;
            if o.get("nome").and_then(Value::as_str).is_none_or(|n| n.trim().is_empty()) {
                return Err(Resposta::erro(422, "validacao", format!("O cliente com o NIF {nif} não existe: indique também cliente.nome para o criar.")));
            }
            let mut params = Map::new();
            for k in ["nome", "endereco", "localidade", "provincia", "pais", "telefone", "email"] {
                if let Some(v) = o.get(k).filter(|v| !v.is_null()) {
                    params.insert(k.into(), v.clone());
                }
            }
            params.insert("nif".into(), json!(nif));
            params.insert("tipo".into(), json!("C"));
            executar(st, c, "entidades.guardar", Value::Object(params)).await?["id"].as_i64().unwrap_or(1)
        }
    };
    corpo.insert("entidade_id".into(), json!(id));
    Ok(())
}

/// Com documento de origem (nota de crédito, conversão de proforma ou encomenda) o programa não volta a pedir
/// autorização de preço e desconto, porque o ecrã copia as linhas da origem. Pela API as linhas vêm de fora: sem as
/// permissões de preço e desconto, cada artigo tem de estar na origem e o preço e o desconto são os da origem.
async fn precos_da_origem(st: &AppState, c: &Chave, corpo: &mut Map<String, Value>) -> Result<(), Resposta> {
    let Some(oid) = corpo.get("doc_origem_id").and_then(Value::as_i64) else { return Ok(()) };
    let (pode_preco, pode_desconto) = (c.tem("pos.alterar_preco"), c.tem("pos.desconto"));
    if pode_preco && pode_desconto {
        return Ok(());
    }
    let global = corpo.get("desconto_global_pct").and_then(Value::as_f64).unwrap_or(0.0) > 0.0 || corpo.get("desconto_global_valor").and_then(Value::as_i64).unwrap_or(0) > 0;
    if global && !pode_desconto {
        return Err(c.exigir("pos.desconto").unwrap_err());
    }
    let mut origem: HashMap<i64, (i64, f64)> = HashMap::new();
    for (artigo, preco, desconto) in
        sqlx::query_as::<_, (Option<i64>, i64, f64)>("SELECT artigo_id, preco_unitario, desconto_pct FROM documento_linhas WHERE documento_id = ? ORDER BY linha").bind(oid).fetch_all(&st.pool).await?
    {
        if let Some(a) = artigo {
            origem.entry(a).or_insert((preco, desconto));
        }
    }
    let Some(linhas) = corpo.get_mut("linhas").and_then(Value::as_array_mut) else { return Ok(()) };
    for (n, l) in linhas.iter_mut().enumerate() {
        let n = n + 1;
        let Some(o) = l.as_object_mut() else { continue };
        let artigo = o.get("artigo_id").and_then(Value::as_i64).unwrap_or_default();
        let Some(&(preco, desconto)) = origem.get(&artigo) else {
            return Err(Resposta::erro(422, "validacao", format!("Linha {n}: o artigo não está no documento de origem.")));
        };
        match o.get("preco_unitario").filter(|v| !v.is_null()).map(Value::as_i64) {
            None => {
                o.insert("preco_unitario".into(), json!(preco));
            }
            Some(p) if p != Some(preco) && !pode_preco => {
                return Err(Resposta::erro(
                    403,
                    "sem_permissao",
                    format!("Linha {n}: preço diferente do documento de origem. Omita o preço ou dê à chave «Vender com preço diferente do da ficha» (pos.alterar_preco)."),
                ))
            }
            _ => {}
        }
        match o.get("desconto_pct").filter(|v| !v.is_null()).map(Value::as_f64) {
            None => {
                o.insert("desconto_pct".into(), json!(desconto));
            }
            Some(d) if d.is_none_or(|d| d > desconto + 1e-9) && !pode_desconto => {
                return Err(Resposta::erro(
                    403,
                    "sem_permissao",
                    format!("Linha {n}: desconto maior do que no documento de origem. Dê à chave «Vender com desconto» (pos.desconto)."),
                ))
            }
            _ => {}
        }
    }
    Ok(())
}

fn trinco_idempotencia() -> &'static tokio::sync::Mutex<()> {
    static T: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    T.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Emite um documento (FR, FT, PP, NC, RC, ES…). Com `Idempotency-Key`, repetir o pedido devolve o mesmo documento.
async fn emitir(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let mut corpo = corpo_objeto(p)?;
    let tipo = corpo
        .get("tipo")
        .and_then(Value::as_str)
        .map(|t| t.trim().to_uppercase())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Resposta::erro(422, "validacao", "Indique o tipo do documento (ex.: FR, FT, PP; ver GET /api/v1/tabelas)."))?;
    let perm = permissao_do_tipo(&tipo).ok_or_else(|| Resposta::erro(422, "validacao", format!("O tipo de documento {tipo} não se emite pela API.")))?;
    c.exigir(perm)?;
    validar_valores(&corpo)?;
    corpo.insert("tipo".into(), json!(tipo));
    // pela API não há PIN de supervisor nem recuperação de documentos manuais: a chave tem (ou não) a permissão
    corpo.remove("autorizacao_pin");
    corpo.remove("manual");
    let idem = match p.cabecalhos.get("idempotency-key").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        Some(k) if k.chars().count() > 100 => return Err(Resposta::erro(400, "pedido_invalido", "A Idempotency-Key tem no máximo 100 caracteres.")),
        k => k,
    };
    let _trinco = match &idem {
        Some(_) => Some(trinco_idempotencia().lock().await),
        None => None,
    };
    if let Some(k) = &idem {
        let feito: Option<i64> = sqlx::query_scalar("SELECT documento_id FROM api_idempotencia WHERE chave_id = ? AND idem = ?").bind(c.id).bind(k).fetch_optional(&st.pool).await?;
        if let Some(did) = feito {
            let v = executar(st, c, "documentos.obter", json!({ "id": did })).await?;
            return Ok(Resposta::json(200, &formatar_documento(v, c)).com("Idempotent-Replayed", "true").com("Location", format!("/api/v1/documentos/{did}")));
        }
    }
    resolver_cliente(st, c, &mut corpo).await?;
    resolver_linhas(st, &mut corpo).await?;
    // as embalagens e a balança mudam as quantidades: volta a confirmar os limites
    validar_valores(&corpo)?;
    precos_da_origem(st, c, &mut corpo).await?;
    let v = match executar(st, c, "documentos.emitir", Value::Object(corpo)).await {
        Ok(v) => v,
        // no programa pede-se o PIN de um supervisor; pela API só conta a permissão da chave
        Err(AppError::Msg(m)) if m.contains("supervisor (PIN)") => {
            let base = m.replace(" É necessária a autorização de um supervisor (PIN).", "").replace(": é necessária a autorização de um supervisor (PIN).", "");
            let perm = if m.contains("Limite de crédito") {
                "«Vender acima do limite de crédito do cliente» (vendas.limite_credito)"
            } else if m.contains("excede o seu limite") {
                "um limite de desconto maior no utilizador da chave"
            } else {
                "«Vender com preço diferente do da ficha» (pos.alterar_preco) ou «Vender com desconto» (pos.desconto)"
            };
            return Err(Resposta::erro(403, "sem_permissao", format!("{}. Pela API não há PIN de supervisor: é preciso {perm}.", base.trim_end_matches('.'))));
        }
        Err(e) => return Err(e.into()),
    };
    let id = v["documento"]["id"].as_i64().ok_or_else(|| Resposta::erro(500, "erro_interno", "O documento foi emitido mas não devolveu o número interno."))?;
    if let Some(k) = &idem {
        sqlx::query("INSERT OR IGNORE INTO api_idempotencia (chave_id, idem, documento_id) VALUES (?, ?, ?)").bind(c.id).bind(k).bind(id).execute(&st.pool).await?;
    }
    Ok(Resposta::json(201, &formatar_documento(v, c)).com("Location", format!("/api/v1/documentos/{id}")))
}

// ---------------------------------------------------------------- relatórios e SAF-T

const AGRUPAMENTOS: &[&str] = &[
    "dia",
    "mes",
    "artigo",
    "familia",
    "operador",
    "cliente",
    "pagamento",
    "vendedor",
    "posto",
    "documento",
    "iva",
    "iva_dia",
    "iva_familia",
    "artigo_operador",
    "artigo_entidade",
];

async fn vendas(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let de = q_data(p, "de")?.ok_or_else(|| Resposta::erro(400, "pedido_invalido", "Indique o período: de=AAAA-MM-DD&ate=AAAA-MM-DD."))?;
    let ate = q_data(p, "ate")?.ok_or_else(|| Resposta::erro(400, "pedido_invalido", "Indique o período: de=AAAA-MM-DD&ate=AAAA-MM-DD."))?;
    let agrupar = q(p, "agrupar").unwrap_or("dia");
    if !AGRUPAMENTOS.contains(&agrupar) {
        return Err(Resposta::erro(400, "pedido_invalido", format!("agrupar tem de ser um de: {}.", AGRUPAMENTOS.join(", "))));
    }
    let mut v = executar(
        st,
        c,
        "relatorios.vendas",
        json!({ "de": &de[..10], "ate": &ate[..10], "agrupar": agrupar, "entidade_id": q_i64(p, "entidade_id")? }),
    )
    .await?;
    custos(&mut v, c);
    Ok(Resposta::json(200, &json!({ "de": &de[..10], "ate": &ate[..10], "agrupar": agrupar, "dados": v })))
}

async fn saft(st: &AppState, c: &Chave, p: &Pedido) -> Res {
    let de = q_data(p, "de")?.ok_or_else(|| Resposta::erro(400, "pedido_invalido", "Indique o período: de=AAAA-MM-DD&ate=AAAA-MM-DD."))?;
    let ate = q_data(p, "ate")?.ok_or_else(|| Resposta::erro(400, "pedido_invalido", "Indique o período: de=AAAA-MM-DD&ate=AAAA-MM-DD."))?;
    let tipo = q(p, "tipo").unwrap_or("facturacao");
    let i = services::saft::SaftIn::novo(&de[..10], &ate[..10]);
    let g = CONTEXTO.scope(c.contexto(), services::saft::gerar(st, &i, tipo)).await?;
    Ok(Resposta {
        estado: 200,
        tipo: "application/xml; charset=utf-8",
        corpo: g.xml.into_bytes(),
        extra: vec![("Content-Disposition", format!("attachment; filename=\"{}\"", g.nome))],
        erro: None,
    })
}

// ============================== métodos do ecrã «API e Integrações» ==============================

fn estado_chave(v: &Value) -> &'static str {
    if !v["revogada_em"].is_null() {
        "REVOGADA"
    } else if v["ativa"].as_i64() == Some(0) {
        "INACTIVA"
    } else if v["expira_em"].as_str().filter(|e| !e.is_empty()).is_some_and(|e| e < db::hoje().as_str()) {
        "EXPIRADA"
    } else {
        "ACTIVA"
    }
}

/// Estado do servidor, chaves, utilizadores e postos para o ecrã de configuração.
pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.api")?;
    let p = &st.pool;
    let cfg = ler_config(p).await?;
    let rows = sqlx::query(
        "SELECT c.id, c.nome, c.prefixo, c.utilizador_id, u.nome AS utilizador, u.perfil AS perfil, u.permissoes AS u_permissoes,
                u.ativo AS utilizador_ativo, c.posto_id, ps.descricao AS posto, c.permissoes, c.ips, c.limite_minuto, c.expira_em,
                c.ativa, c.revogada_em, c.criado_em, uc.nome AS criado_por, c.ultimo_uso, c.ultimo_ip, c.pedidos,
                (SELECT COUNT(*) FROM documentos d WHERE d.api_chave_id = c.id) AS documentos
         FROM api_chaves c JOIN utilizadores u ON u.id = c.utilizador_id
         LEFT JOIN postos ps ON ps.id = c.posto_id LEFT JOIN utilizadores uc ON uc.id = c.criado_por
         ORDER BY c.revogada_em IS NOT NULL, c.id DESC",
    )
    .fetch_all(p)
    .await?;
    let mut chaves = Vec::with_capacity(rows.len());
    for r in &rows {
        let (_, do_utilizador) = permissoes_efectivas(p, &r.get::<String, _>("perfil"), r.get::<Option<String>, _>("u_permissoes").as_deref()).await;
        let mut v = db::row_json(r);
        let perms: Vec<String> = serde_json::from_str(v["permissoes"].as_str().unwrap_or("[]")).unwrap_or_default();
        // permissões da chave que o utilizador não tem: não têm efeito
        let sem_efeito: Vec<&String> = perms.iter().filter(|x| !x.starts_with("api.") && !do_utilizador.contains(x)).collect();
        v["sem_efeito"] = json!(sem_efeito);
        v["permissoes"] = json!(perms);
        v["estado"] = json!(estado_chave(&v));
        if let Value::Object(m) = &mut v {
            m.remove("u_permissoes");
        }
        chaves.push(v);
    }
    // utilizadores activos com as permissões efectivas (o ecrã avisa das permissões da chave que não teriam efeito)
    let mut utilizadores = Vec::new();
    for u in sqlx::query("SELECT id, nome, login, perfil, permissoes FROM utilizadores WHERE ativo = 1 ORDER BY nome").fetch_all(p).await? {
        let perfil: String = u.get("perfil");
        let (_, perms) = permissoes_efectivas(p, &perfil, u.get::<Option<String>, _>("permissoes").as_deref()).await;
        utilizadores.push(json!({ "id": u.get::<i64, _>("id"), "nome": u.get::<String, _>("nome"), "login": u.get::<String, _>("login"), "perfil": perfil, "permissoes": perms }));
    }
    let lic = services::licenca::calcular(st).await?;
    let incluida = services::licenca::api_incluida(&lic);
    let servir = a_servir();
    let enderecos: Vec<String> = if cfg.rede { crate::rede::enderecos_locais() } else { Vec::new() };
    Ok(json!({
        "ativo": cfg.ativo,
        "porta": cfg.porta,
        "rede": cfg.rede,
        "cf_equipa": cfg.cf_equipa,
        "cf_aud": cfg.cf_aud,
        "a_servir": servir.map(|s| s.0),
        "licenca": {
            "incluida": incluida.is_ok(),
            "experiencia": lic.plano == "EXPERIENCIA",
            "plano": services::licenca::nome_plano(&lic.plano),
            "mensagem": incluida.err(),
        },
        "erro": ERRO.lock().unwrap().clone(),
        "enderecos": enderecos,
        "modo_rede": crate::rede::config().modo,
        "permissoes": PERMISSOES_API.iter().map(|(id, d, g)| json!({ "id": id, "descricao": d, "grupo": g })).collect::<Vec<_>>(),
        "chaves": chaves,
        "utilizadores": utilizadores,
        "postos": db::query_json(p, "SELECT id, descricao, armazem_id, serie_prefixo FROM postos ORDER BY id").await?,
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    ativo: bool,
    porta: Option<u16>,
    rede: Option<bool>,
    /// Cloudflare Access: domínio da equipa e AUD da aplicação (vazios = sem Access).
    cf_equipa: Option<String>,
    cf_aud: Option<String>,
}

/// Liga, desliga ou muda a porta da API (só o administrador).
pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let porta = g.porta.unwrap_or(PORTA_PADRAO);
    if porta < 1024 {
        return Err(AppError::msg("Escolha uma porta entre 1024 e 65535."));
    }
    if crate::rede::servidor_activo() == Some(porta) || (crate::rede::config().modo == "servidor" && crate::rede::config().porta == porta) {
        return Err(AppError::msg(format!("A porta {porta} é a dos postos em rede. Escolha outra para a API (ex.: {PORTA_PADRAO}).")));
    }
    if crate::rede::config().cliente() {
        return Err(AppError::msg("Esta máquina é um posto em rede: a API liga-se no servidor, onde está a base de dados."));
    }
    let rede = g.rede.unwrap_or(true);
    let cf_equipa = g.cf_equipa.as_deref().unwrap_or("").trim().trim_start_matches("https://").trim_end_matches('/').to_lowercase();
    let cf_aud = g.cf_aud.as_deref().unwrap_or("").trim().to_string();
    if cf_equipa.is_empty() != cf_aud.is_empty() {
        return Err(AppError::msg("Para o Cloudflare Access indique o domínio da equipa e o AUD da aplicação (ou deixe os dois vazios)."));
    }
    if !cf_equipa.is_empty() && (!cf_equipa.ends_with(".cloudflareaccess.com") || cf_equipa.contains('/') || cf_equipa.contains(' ')) {
        return Err(AppError::msg("O domínio da equipa do Cloudflare Access acaba em «.cloudflareaccess.com» (ex.: empresa.cloudflareaccess.com)."));
    }
    if !cf_aud.is_empty() && (cf_aud.len() < 32 || !cf_aud.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(AppError::msg("O AUD da aplicação do Cloudflare Access é um código hexadecimal longo (Application Audience Tag)."));
    }
    if g.ativo {
        services::licenca::api_incluida(&services::licenca::calcular(st).await?).map_err(AppError::msg)?;
        let app = APP.get().cloned().ok_or_else(|| AppError::msg("Feche e reabra o KanGest para ligar a API."))?;
        if let Err(e) = iniciar(app, porta, rede) {
            *ERRO.lock().unwrap() = Some(e.to_string());
            return Err(e);
        }
    } else {
        parar();
        *ERRO.lock().unwrap() = None;
    }
    let mut tx = st.pool.begin().await?;
    for (k, v) in [
        ("api.ativo", if g.ativo { "1" } else { "0" }.to_string()),
        ("api.porta", porta.to_string()),
        ("api.rede", if rede { "1" } else { "0" }.to_string()),
        ("api.cf_equipa", cf_equipa.clone()),
        ("api.cf_aud", cf_aud.clone()),
    ] {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    let detalhe = if g.ativo { format!("API ligada na porta {porta} ({})", if rede { "rede" } else { "só esta máquina" }) } else { "API desligada".to_string() };
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API', ?)").bind(s.id).bind(detalhe).execute(&mut *tx).await?;
    tx.commit().await?;
    estado(st).await
}

#[derive(Deserialize)]
pub struct ChaveIn {
    id: Option<i64>,
    nome: String,
    utilizador_id: i64,
    posto_id: Option<i64>,
    #[serde(default)]
    permissoes: Vec<String>,
    ips: Option<String>,
    limite_minuto: Option<i64>,
    expira_em: Option<String>,
    ativa: Option<bool>,
}

struct ChaveValida {
    nome: String,
    permissoes: String,
    ips: Option<String>,
    limite: i64,
    expira: Option<String>,
}

async fn validar_chave(st: &AppState, c: &ChaveIn) -> AppResult<ChaveValida> {
    let nome = c.nome.trim().to_string();
    if nome.is_empty() || nome.chars().count() > 80 {
        return Err(AppError::msg("Dê um nome à chave (ex.: «Loja online»), até 80 caracteres."));
    }
    let ativo: Option<i64> = sqlx::query_scalar("SELECT ativo FROM utilizadores WHERE id = ?").bind(c.utilizador_id).fetch_optional(&st.pool).await?;
    if ativo != Some(1) {
        return Err(AppError::msg("Escolha um utilizador activo: a chave age em nome dele."));
    }
    if let Some(p) = c.posto_id {
        let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM postos WHERE id = ?").bind(p).fetch_optional(&st.pool).await?;
        if existe.is_none() {
            return Err(AppError::msg("O posto escolhido não existe."));
        }
    }
    let mut perms: Vec<String> = Vec::new();
    for x in &c.permissoes {
        if !permitida_api(x) {
            return Err(AppError::msg(format!("Permissão desconhecida para chaves de API: {x}")));
        }
        if !perms.contains(x) {
            perms.push(x.clone());
        }
    }
    if perms.is_empty() {
        return Err(AppError::msg("Escolha pelo menos uma permissão para a chave."));
    }
    let ips = c.ips.as_deref().map(str::trim).filter(|x| !x.is_empty());
    let ips = match ips {
        Some(t) => {
            ler_redes(t).map_err(AppError::msg)?;
            Some(t.split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace()).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", "))
        }
        None => None,
    };
    let expira = c.expira_em.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(String::from);
    if let Some(e) = &expira {
        if chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").is_err() {
            return Err(AppError::msg("Validade inválida (AAAA-MM-DD)."));
        }
        if c.id.is_none() && e.as_str() < db::hoje().as_str() {
            return Err(AppError::msg("A validade já passou."));
        }
    }
    Ok(ChaveValida { nome, permissoes: serde_json::to_string(&perms)?, ips, limite: c.limite_minuto.unwrap_or(120).clamp(0, 100_000), expira })
}

/// Cria uma chave. O segredo devolvido (`chave`) não volta a ser mostrado.
pub async fn chave_criar(st: &AppState, c: ChaveIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let v = validar_chave(st, &c).await?;
    for _ in 0..5 {
        let token = gerar_chave();
        let prefixo = token[..PREFIXO_LEN].to_string();
        let r = sqlx::query(
            "INSERT INTO api_chaves (nome, prefixo, hash, utilizador_id, posto_id, permissoes, ips, limite_minuto, expira_em, ativa, criado_por)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&v.nome)
        .bind(&prefixo)
        .bind(hash_chave(&token))
        .bind(c.utilizador_id)
        .bind(c.posto_id)
        .bind(&v.permissoes)
        .bind(&v.ips)
        .bind(v.limite)
        .bind(&v.expira)
        .bind(c.ativa.unwrap_or(true) as i64)
        .bind(s.id)
        .execute(&st.pool)
        .await;
        match r {
            Ok(r) => {
                let id = r.last_insert_rowid();
                sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_CHAVE', ?)")
                    .bind(s.id)
                    .bind(format!("Chave de API criada: {} ({prefixo}…)", v.nome))
                    .execute(&st.pool)
                    .await?;
                return Ok(json!({ "id": id, "prefixo": prefixo, "chave": token }));
            }
            // prefixo repetido (raríssimo): gera outra
            Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE") => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::msg("Não foi possível gerar uma chave única. Tente de novo."))
}

/// Altera nome, utilizador, posto, permissões, IPs, limite, validade ou estado (não o segredo).
pub async fn chave_guardar(st: &AppState, c: ChaveIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let id = c.id.ok_or_else(|| AppError::msg("Falta o identificador da chave."))?;
    let revogada: Option<Option<String>> = sqlx::query_scalar("SELECT revogada_em FROM api_chaves WHERE id = ?").bind(id).fetch_optional(&st.pool).await?;
    match revogada {
        None => return Err(AppError::NaoEncontrado),
        Some(Some(_)) => return Err(AppError::msg("Uma chave revogada não se altera: crie uma chave nova.")),
        Some(None) => {}
    }
    let v = validar_chave(st, &c).await?;
    sqlx::query("UPDATE api_chaves SET nome = ?, utilizador_id = ?, posto_id = ?, permissoes = ?, ips = ?, limite_minuto = ?, expira_em = ?, ativa = ? WHERE id = ?")
        .bind(&v.nome)
        .bind(c.utilizador_id)
        .bind(c.posto_id)
        .bind(&v.permissoes)
        .bind(&v.ips)
        .bind(v.limite)
        .bind(&v.expira)
        .bind(c.ativa.unwrap_or(true) as i64)
        .bind(id)
        .execute(&st.pool)
        .await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_CHAVE', ?)")
        .bind(s.id)
        .bind(format!("Chave de API alterada: {}", v.nome))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdChaveIn {
    id: i64,
}

/// Gera um segredo novo para a mesma chave (o antigo deixa logo de funcionar).
pub async fn chave_renovar(st: &AppState, i: IdChaveIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let row: Option<(String, Option<String>)> = sqlx::query_as("SELECT nome, revogada_em FROM api_chaves WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    let (nome, revogada) = row.ok_or(AppError::NaoEncontrado)?;
    if revogada.is_some() {
        return Err(AppError::msg("Uma chave revogada não se renova: crie uma chave nova."));
    }
    for _ in 0..5 {
        let token = gerar_chave();
        let prefixo = token[..PREFIXO_LEN].to_string();
        let r = sqlx::query("UPDATE api_chaves SET prefixo = ?, hash = ? WHERE id = ?").bind(&prefixo).bind(hash_chave(&token)).bind(i.id).execute(&st.pool).await;
        match r {
            Ok(_) => {
                sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_CHAVE', ?)")
                    .bind(s.id)
                    .bind(format!("Segredo da chave de API renovado: {nome} ({prefixo}…)"))
                    .execute(&st.pool)
                    .await?;
                return Ok(json!({ "id": i.id, "prefixo": prefixo, "chave": token }));
            }
            Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE") => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::msg("Não foi possível gerar uma chave única. Tente de novo."))
}

/// Revoga uma chave de vez: deixa de funcionar e não se reactiva.
pub async fn chave_revogar(st: &AppState, i: IdChaveIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let nome: Option<String> = sqlx::query_scalar("SELECT nome FROM api_chaves WHERE id = ? AND revogada_em IS NULL").bind(i.id).fetch_optional(&st.pool).await?;
    let nome = nome.ok_or_else(|| AppError::msg("A chave não existe ou já foi revogada."))?;
    sqlx::query("UPDATE api_chaves SET ativa = 0, revogada_em = ? WHERE id = ?").bind(db::agora()).bind(i.id).execute(&st.pool).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_CHAVE', ?)")
        .bind(s.id)
        .bind(format!("Chave de API revogada: {nome}"))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "id": i.id }))
}

#[derive(Deserialize)]
pub struct PedidosIn {
    chave_id: Option<i64>,
    #[serde(default)]
    erros: bool,
    limite: Option<i64>,
}

/// Últimos pedidos feitos à API (registo de 90 dias).
pub async fn pedidos(st: &AppState, f: PedidosIn) -> AppResult<Value> {
    st.exigir_perm("sistema.api")?;
    let rows = sqlx::query(
        "SELECT p.id, p.data_hora, p.chave_id, c.nome AS chave, c.prefixo, p.ip, p.metodo, p.caminho, p.estado, p.duracao_ms, p.erro
         FROM api_pedidos p LEFT JOIN api_chaves c ON c.id = p.chave_id
         WHERE (?1 IS NULL OR p.chave_id = ?1) AND (?2 = 0 OR p.estado >= 400)
         ORDER BY p.id DESC LIMIT ?3",
    )
    .bind(f.chave_id)
    .bind(f.erros as i64)
    .bind(f.limite.unwrap_or(200).clamp(1, 2000))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

/// Para os testes: chave RSA, JWKS e JWT como os do Cloudflare Access.
#[cfg(test)]
pub mod teste_cf {
    use super::*;
    use base64::Engine;
    use rsa::signature::{SignatureEncoding, Signer};
    use rsa::traits::PublicKeyParts;

    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
    }

    /// Uma só chave para todos os testes (gerar RSA demora).
    pub fn chave() -> &'static rsa::RsaPrivateKey {
        static K: OnceLock<rsa::RsaPrivateKey> = OnceLock::new();
        K.get_or_init(|| rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("chave RSA de teste"))
    }

    pub fn jwks(chave: &rsa::RsaPrivateKey, kid: &str) -> Value {
        let pk = chave.to_public_key();
        json!({ "keys": [{ "kid": kid, "kty": "RSA", "alg": "RS256", "n": b64(&pk.n().to_bytes_be()), "e": b64(&pk.e().to_bytes_be()) }] })
    }

    pub fn jwt(chave: &rsa::RsaPrivateKey, kid: &str, claims: &Value) -> String {
        let h = b64(json!({ "alg": "RS256", "kid": kid, "typ": "JWT" }).to_string().as_bytes());
        let p = b64(claims.to_string().as_bytes());
        let assinatura = rsa::pkcs1v15::SigningKey::<Sha256>::new(chave.clone()).sign(format!("{h}.{p}").as_bytes());
        format!("{h}.{p}.{}", b64(&assinatura.to_bytes()))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A documentação (docs/openapi.json, gerada por scripts/gerar-openapi.py) e o código têm as mesmas rotas.
    #[test]
    fn openapi_igual_as_rotas() {
        let v: Value = serde_json::from_str(OPENAPI).unwrap();
        // os nomes dos parâmetros podem ser outros: compara-se a forma do caminho
        let forma = |c: &str| c.split('/').map(|s| if s.starts_with('{') { "{}" } else { s }).collect::<Vec<_>>().join("/");
        let mut doc: Vec<(String, String)> = v["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(c, ops)| {
                ops.as_object().unwrap().keys().filter(|m| ["get", "post", "put", "patch", "delete"].contains(&m.as_str())).map(move |m| (m.to_uppercase(), forma(c)))
            })
            .collect();
        let mut codigo: Vec<(String, String)> = ROTAS.iter().map(|(m, c)| (m.to_string(), forma(c))).collect();
        doc.sort();
        codigo.sort();
        assert_eq!(doc, codigo, "docs/openapi.json e api::ROTAS não descrevem as mesmas rotas (actualize scripts/gerar-openapi.py)");
        // e o encaminhamento reconhece cada uma
        for (_, c) in ROTAS {
            let segs: Vec<&str> = c.split('/').filter(|s| !s.is_empty()).map(|s| if s.starts_with('{') { "1" } else { s }).collect();
            assert!(rota_existe(&segs), "{c}");
        }
        assert!(!rota_existe(&["artigos", "1", "outra"]) && !rota_existe(&["naoexiste"]));
    }

    #[test]
    fn jwt_do_cloudflare_access() {
        use super::teste_cf::{chave, jwks, jwt};
        let equipa = "empresa.cloudflareaccess.com";
        let aud = "f".repeat(64);
        let chaves = chaves_de_jwks(&jwks(chave(), "k1"));
        assert_eq!(chaves.len(), 1);
        let agora = 1_800_000_000u64;
        let claims = |aud: &str, iss: &str, exp: u64| json!({ "aud": [aud], "iss": iss, "exp": exp, "iat": agora, "sub": "", "common_name": "integracao.access" });
        let iss = format!("https://{equipa}");
        let bom = jwt(chave(), "k1", &claims(&aud, &iss, agora + 300));
        assert!(verificar_jwt_cf(&bom, &chaves, equipa, &aud, agora).is_ok());
        // aud como texto também serve
        let aud_txt = jwt(chave(), "k1", &json!({ "aud": aud, "iss": iss, "exp": agora + 300 }));
        assert!(verificar_jwt_cf(&aud_txt, &chaves, equipa, &aud, agora).is_ok());
        // recusados: outra aplicação, outra equipa, expirado, alterado, outra chave, sem assinatura
        assert!(verificar_jwt_cf(&jwt(chave(), "k1", &claims(&"0".repeat(64), &iss, agora + 300)), &chaves, equipa, &aud, agora).unwrap_err().contains("AUD"));
        assert!(verificar_jwt_cf(&jwt(chave(), "k1", &claims(&aud, "https://outra.cloudflareaccess.com", agora + 300)), &chaves, equipa, &aud, agora).unwrap_err().contains("equipa"));
        assert!(verificar_jwt_cf(&jwt(chave(), "k1", &claims(&aud, &iss, agora - 120)), &chaves, equipa, &aud, agora).unwrap_err().contains("expirado"));
        let mut partes: Vec<String> = bom.split('.').map(String::from).collect();
        partes[1] = super::teste_cf::jwt(chave(), "k1", &claims(&aud, &iss, agora + 9999)).split('.').nth(1).unwrap().to_string();
        assert!(verificar_jwt_cf(&partes.join("."), &chaves, equipa, &aud, agora).unwrap_err().contains("assinatura"));
        assert!(verificar_jwt_cf(&jwt(chave(), "k2", &claims(&aud, &iss, agora + 300)), &chaves, equipa, &aud, agora).is_err(), "kid desconhecido");
        let sem = format!("{}.{}.", bom.split('.').next().unwrap(), bom.split('.').nth(1).unwrap());
        assert!(verificar_jwt_cf(&sem, &chaves, equipa, &aud, agora).is_err());
        assert!(verificar_jwt_cf("isto.nao.e.jwt", &chaves, equipa, &aud, agora).is_err());
    }

    #[test]
    fn redes_e_ips() {
        let r = ler_redes("192.168.1.0/24, 10.0.0.5; ::1\n2001:db8::/32").unwrap();
        assert_eq!(r.len(), 4);
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(ip_permitido(&r, ip("192.168.1.77")));
        assert!(!ip_permitido(&r, ip("192.168.2.1")));
        assert!(ip_permitido(&r, ip("10.0.0.5")));
        assert!(!ip_permitido(&r, ip("10.0.0.6")));
        assert!(ip_permitido(&r, ip("::ffff:192.168.1.3")), "IPv4 dentro de IPv6");
        assert!(ip_permitido(&r, ip("::1")));
        assert!(ip_permitido(&r, ip("2001:db8:1::9")));
        assert!(!ip_permitido(&r, ip("2001:db9::1")));
        assert!(ip_permitido(&[], ip("8.8.8.8")), "sem lista: qualquer IP");
        assert!(ip_permitido(&ler_redes("0.0.0.0/0").unwrap(), ip("8.8.8.8")));
        assert!(ler_redes("192.168.1.0/33").is_err());
        assert!(ler_redes("abc").is_err());
    }

    #[test]
    fn ip_atras_de_tunel() {
        let mut cab = HashMap::new();
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        assert_eq!(ip_cliente(local, &cab), local);
        cab.insert("x-forwarded-for".to_string(), "41.63.1.2, 10.0.0.1".to_string());
        assert_eq!(ip_cliente(local, &cab), "41.63.1.2".parse::<IpAddr>().unwrap());
        cab.insert("cf-connecting-ip".to_string(), "41.63.9.9".to_string());
        assert_eq!(ip_cliente(local, &cab), "41.63.9.9".parse::<IpAddr>().unwrap());
        // de fora da máquina, os cabeçalhos não contam (ninguém se faz passar por outro IP)
        let fora: IpAddr = "192.168.1.20".parse().unwrap();
        assert_eq!(ip_cliente(fora, &cab), fora);
    }

    #[test]
    fn chaves_e_descodificacao() {
        let k = gerar_chave();
        assert!(formato_valido(&k), "{k}");
        assert_ne!(k, gerar_chave());
        assert!(!formato_valido("kg_curta"));
        assert!(!formato_valido(&k.replace("kg_", "xx_")));
        assert_eq!(hash_chave("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(iguais(b"abc", b"abc") && !iguais(b"abc", b"abd") && !iguais(b"abc", b"ab"));
        assert_eq!(descodificar("Arroz%205kg+x", true), "Arroz 5kg x");
        assert_eq!(descodificar("a+b", false), "a+b");
        assert_eq!(descodificar("100%", true), "100%");
        assert_eq!(descodificar("%C3%A1gua", true), "água");
        let c = ler_consulta("q=%C3%A1gua&limite=5&vazio");
        assert_eq!((c["q"].as_str(), c["limite"].as_str(), c["vazio"].as_str()), ("água", "5", ""));
        let mut cab = HashMap::new();
        cab.insert("authorization".to_string(), format!("bearer {k}"));
        assert_eq!(extrair_chave(&cab).as_deref(), Some(k.as_str()));
        cab.clear();
        cab.insert("x-api-key".to_string(), k.clone());
        assert_eq!(extrair_chave(&cab).as_deref(), Some(k.as_str()));
        assert_eq!(sem_iva(1140.0, 14.0), 1000);
        assert_eq!(sem_iva(999.0, 0.0), 999);
    }
}

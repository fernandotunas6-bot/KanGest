//! Postos em rede: uma máquina guarda a base de dados (modo **servidor**) e os outros
//! postos (modo **cliente**) trabalham sobre ela, em vez de cada instalação ter a sua base.
//!
//! O servidor expõe o mesmo despachante `rpc` por HTTP/1.1 na rede local: cada pedido traz a
//! chave de rede, o n.º do posto e um identificador de posto (token) com que o servidor guarda
//! a sessão desse posto. Nada é partilhado entre postos a não ser a base de dados.
//!
//! A ligação é cifrada: cada pedido e cada resposta vão em AES-256-GCM, com uma chave derivada da chave de rede
//! (Argon2id com o sal próprio do servidor). A chave de rede nunca viaja; um pedido só se abre com ela, leva a hora e
//! um número único (contra repetições) e a resposta só se abre com o número do pedido. O posto acerta a hora pela do
//! servidor, por isso relógios desacertados não impedem a venda. Mesmo assim, não expor a porta à Internet.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::services;
use crate::state::{AppState, Contexto, PostoAtual, CONTEXTO};

pub const PORTA_PADRAO: u16 = 7799;
/// Métodos que são sempre desta máquina, mesmo em modo cliente: configuração de rede,
/// impressão (a impressora é local) e gravação de ficheiros exportados.
const LOCAIS: [&str; 5] = ["rede.", "impressao.", "sistema.exportar", "atualizacao.", "licenca.abrirSite"];

// ============================== configuração ==============================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// local | servidor | cliente
    #[serde(default = "modo_padrao")]
    pub modo: String,
    #[serde(default = "porta_padrao")]
    pub porta: u16,
    /// Endereço do servidor (modo cliente): "192.168.1.10" ou "192.168.1.10:7799".
    #[serde(default)]
    pub servidor: String,
    #[serde(default)]
    pub chave: String,
    /// Sal da cifra (servidor): aleatório, criado na primeira vez que o servidor abre; os postos lêem-no do servidor.
    #[serde(default)]
    pub sal: String,
    /// Só nas compilações de desenvolvimento (auditoria visual): aceita pedidos sem cifra.
    #[serde(default)]
    pub sem_cifra: bool,
}

fn modo_padrao() -> String {
    "local".into()
}
fn porta_padrao() -> u16 {
    PORTA_PADRAO
}

impl Default for Config {
    fn default() -> Self {
        Self { modo: modo_padrao(), porta: PORTA_PADRAO, servidor: String::new(), chave: String::new(), sal: String::new(), sem_cifra: false }
    }
}

impl Config {
    pub fn cliente(&self) -> bool {
        self.modo == "cliente" && !self.servidor.trim().is_empty()
    }
}

static APP: OnceLock<Arc<AppState>> = OnceLock::new();

/// Guarda o estado da aplicação para o servidor de rede o poder usar nas suas threads.
pub fn registar_estado(a: Arc<AppState>) {
    let _ = APP.set(a);
}

static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();

fn celula() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(Config::default()))
}

pub fn config() -> Config {
    celula().lock().unwrap().clone()
}

fn ficheiro(dir: &Path) -> std::path::PathBuf {
    dir.join("rede.json")
}

/// Lê `rede.json` da pasta de dados e passa a ser a configuração em uso.
pub fn carregar(dir: &Path) -> Config {
    let c = std::fs::read_to_string(ficheiro(dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .unwrap_or_default();
    *celula().lock().unwrap() = c.clone();
    c
}

fn gravar(dir: &Path, c: &Config) -> AppResult<()> {
    std::fs::write(ficheiro(dir), serde_json::to_string_pretty(c)?)
        .map_err(|e| AppError::msg(format!("Não foi possível gravar {}: {e}", ficheiro(dir).display())))?;
    *celula().lock().unwrap() = c.clone();
    Ok(())
}

// ============================== postos ligados (lado servidor) ==============================

struct Ligacao {
    sessao: Arc<Mutex<Option<crate::state::Sessao>>>,
    posto_id: i64,
    posto: String,
    utilizador: Option<String>,
    endereco: String,
    ultimo: String,
    pedidos: u64,
}

static LIGACOES: OnceLock<Mutex<HashMap<String, Ligacao>>> = OnceLock::new();
static PORTA_ACTIVA: AtomicUsize = AtomicUsize::new(0);
static ATENDER: AtomicUsize = AtomicUsize::new(0);

fn ligacoes() -> &'static Mutex<HashMap<String, Ligacao>> {
    LIGACOES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn servidor_activo() -> Option<u16> {
    match PORTA_ACTIVA.load(Ordering::Relaxed) {
        0 => None,
        p => Some(p as u16),
    }
}

/// Identificador desta máquina perante o servidor (muda a cada arranque; só serve para a sessão).
fn token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        use rand::Rng;
        let mut r = rand::thread_rng();
        (0..24).map(|_| char::from(b'a' + r.gen_range(0..26))).collect()
    })
}

// ============================== servidor ==============================

/// Abre a porta e começa a servir os postos em rede. Uma vez aberta, só fecha ao sair da aplicação.
pub fn iniciar_servidor(estado: Arc<AppState>, cfg: &Config) -> AppResult<u16> {
    if cfg.chave.trim().chars().count() < 6 {
        return Err(AppError::msg("Defina uma chave de rede com pelo menos 6 caracteres antes de abrir o servidor."));
    }
    let mut cfg = cfg.clone();
    if let Some(p) = servidor_activo() {
        if p == cfg.porta {
            // a chave de rede pode ter mudado: passa a valer já, sem reabrir a porta
            preparar_cifra(&estado, &mut cfg)?;
            *celula().lock().unwrap() = cfg;
            return Ok(p);
        }
        return Err(AppError::msg(format!("O servidor já está a servir na porta {p}. Feche e reabra a aplicação para mudar de porta.")));
    }
    let escuta = TcpListener::bind(("0.0.0.0", cfg.porta))
        .map_err(|e| AppError::msg(format!("Não foi possível abrir a porta {}: {e}. Escolha outra porta ou autorize a aplicação na firewall.", cfg.porta)))?;
    preparar_cifra(&estado, &mut cfg)?;
    std::thread::Builder::new()
        .name("kigest-rede".into())
        .spawn(move || {
            for fluxo in escuta.incoming() {
                let Ok(fluxo) = fluxo else { continue };
                // limite de pedidos em paralelo: um posto avariado não derruba o servidor
                if ATENDER.load(Ordering::Relaxed) >= 48 {
                    continue;
                }
                let Some(cifra) = CIFRA.lock().unwrap().clone() else { continue };
                ATENDER.fetch_add(1, Ordering::Relaxed);
                let st = estado.clone();
                std::thread::spawn(move || {
                    atender(&st, &cifra, fluxo);
                    ATENDER.fetch_sub(1, Ordering::Relaxed);
                });
            }
        })
        .map_err(|e| AppError::msg(format!("Não foi possível iniciar o servidor de rede: {e}")))?;
    PORTA_ACTIVA.store(cfg.porta as usize, Ordering::Relaxed);
    // a configuração em uso passa a ser esta (o arranque e o ecrã de rede contam com isso)
    *celula().lock().unwrap() = cfg.clone();
    Ok(cfg.porta)
}

// ============================== cifra ==============================

const AAD_PEDIDO: &[u8] = b"KanGest-rede-v2:pedido";
const AAD_RESPOSTA: &[u8] = b"KanGest-rede-v2:resposta:";
/// Diferença máxima entre a hora do pedido e a do servidor.
const DESVIO_MS: u64 = 300_000;

/// Chave do servidor: a derivada da chave de rede, o sal (para os postos) e, em desenvolvimento, a chave em texto.
struct Cifra {
    chave: [u8; 32],
    sal: String,
    texto: String,
    sem_cifra: bool,
}

static CIFRA: Mutex<Option<Arc<Cifra>>> = Mutex::new(None);

/// Deriva a chave do servidor; o sal é criado na primeira vez e fica no rede.json desta máquina.
fn preparar_cifra(estado: &AppState, cfg: &mut Config) -> AppResult<()> {
    if B64.decode(&cfg.sal).map_or(true, |s| s.len() < 16) {
        cfg.sal = B64.encode(aleatorio::<16>()?);
        gravar(&estado.pasta_dados, cfg)?;
    }
    let sal = B64.decode(&cfg.sal).unwrap_or_default();
    let c = Cifra {
        chave: derivar(&cfg.chave, &sal)?,
        sal: cfg.sal.clone(),
        texto: cfg.chave.trim().to_string(),
        sem_cifra: cfg!(debug_assertions) && cfg.sem_cifra,
    };
    *CIFRA.lock().unwrap() = Some(Arc::new(c));
    Ok(())
}

fn aleatorio<const N: usize>() -> AppResult<[u8; N]> {
    use ring::rand::SecureRandom;
    let mut b = [0u8; N];
    ring::rand::SystemRandom::new().fill(&mut b).map_err(|_| AppError::msg("Sem números aleatórios do sistema."))?;
    Ok(b)
}

/// Chave AES-256 a partir da chave de rede (Argon2id, lento de propósito contra tentativas às cegas).
/// Os parâmetros ficam escritos aqui: têm de ser iguais em todas as versões que falam entre si.
fn derivar(chave: &str, sal: &[u8]) -> AppResult<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let erro = |e: argon2::Error| AppError::msg(format!("Chave de rede: {e}"));
    let params = Params::new(19_456, 2, 1, Some(32)).map_err(erro)?;
    let mut k = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(chave.trim().as_bytes(), sal, &mut k).map_err(erro)?;
    Ok(k)
}

fn aes(chave: &[u8; 32]) -> ring::aead::LessSafeKey {
    ring::aead::LessSafeKey::new(ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, chave).expect("chave AES-256"))
}

/// `{"v": 2, "n": <número único>, "c": <cifrado + etiqueta>}`.
fn selar(chave: &[u8; 32], aad: &[u8], claro: &[u8]) -> AppResult<([u8; 12], Value)> {
    let n = aleatorio::<12>()?;
    let mut buf = claro.to_vec();
    aes(chave)
        .seal_in_place_append_tag(ring::aead::Nonce::assume_unique_for_key(n), ring::aead::Aad::from(aad), &mut buf)
        .map_err(|_| AppError::msg("Não foi possível cifrar o pedido."))?;
    Ok((n, json!({ "v": 2, "n": B64.encode(n), "c": B64.encode(&buf) })))
}

/// Abre um envelope `v: 2`: (número único, texto claro). `None` se a chave não for a mesma ou o conteúdo foi mexido.
fn abrir(chave: &[u8; 32], aad: &[u8], v: &Value) -> Option<([u8; 12], Vec<u8>)> {
    if v["v"] != 2 {
        return None;
    }
    let n: [u8; 12] = B64.decode(v["n"].as_str()?).ok()?.try_into().ok()?;
    let mut buf = B64.decode(v["c"].as_str()?).ok()?;
    let claro = aes(chave).open_in_place(ring::aead::Nonce::assume_unique_for_key(n), ring::aead::Aad::from(aad), &mut buf).ok()?;
    Some((n, claro.to_vec()))
}

fn agora_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Números únicos já vistos (10 minutos): um pedido copiado da rede não se pode repetir.
static VISTOS: Mutex<Option<HashMap<[u8; 12], u64>>> = Mutex::new(None);

fn repetido(n: [u8; 12]) -> bool {
    let agora = agora_ms();
    let mut v = VISTOS.lock().unwrap();
    let mapa = v.get_or_insert_with(HashMap::new);
    if mapa.len() > 5000 {
        mapa.retain(|_, t| agora.saturating_sub(*t) < 2 * DESVIO_MS);
    }
    mapa.insert(n, agora).is_some()
}

fn resposta(codigo: u16, corpo: &Value) -> Vec<u8> {
    let texto = corpo.to_string();
    let estado = if codigo == 200 { "200 OK" } else if codigo == 401 { "401 Unauthorized" } else { "400 Bad Request" };
    format!(
        "HTTP/1.1 {estado}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{texto}",
        texto.len()
    )
    .into_bytes()
}

/// Lê um pedido HTTP simples: devolve (linha do pedido, corpo).
fn ler_pedido(fluxo: &mut TcpStream) -> Result<(String, String), String> {
    let mut dados = Vec::with_capacity(2048);
    let mut tampao = [0u8; 2048];
    let mut fim_cabecalho = None;
    while fim_cabecalho.is_none() {
        let n = fluxo.read(&mut tampao).map_err(|e| format!("Leitura interrompida: {e}"))?;
        if n == 0 {
            return Err("Pedido vazio.".into());
        }
        dados.extend_from_slice(&tampao[..n]);
        fim_cabecalho = dados.windows(4).position(|w| w == b"\r\n\r\n");
        if dados.len() > 32 * 1024 * 1024 {
            return Err("Pedido demasiado grande.".into());
        }
    }
    let corte = fim_cabecalho.unwrap() + 4;
    let cabecalho = String::from_utf8_lossy(&dados[..corte]).to_string();
    let primeira = cabecalho.lines().next().unwrap_or("").to_string();
    let tamanho: usize = cabecalho
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut corpo = dados[corte..].to_vec();
    while corpo.len() < tamanho {
        let n = fluxo.read(&mut tampao).map_err(|e| format!("Leitura interrompida: {e}"))?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&tampao[..n]);
    }
    Ok((primeira, String::from_utf8_lossy(&corpo).to_string()))
}

#[derive(Deserialize)]
struct Pedido {
    /// Só no protocolo antigo, sem cifra (aceite apenas em desenvolvimento).
    #[serde(default)]
    chave: String,
    token: String,
    #[serde(default = "um")]
    posto: i64,
    metodo: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    descricao_posto: Option<String>,
    /// Hora do pedido (ms desde 1970), já acertada pela do servidor.
    #[serde(default)]
    t: i64,
}

fn um() -> i64 {
    1
}

fn atender(st: &AppState, cifra: &Cifra, mut fluxo: TcpStream) {
    fluxo.set_read_timeout(Some(Duration::from_secs(30))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let endereco = fluxo.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    let saida = match ler_pedido(&mut fluxo) {
        Err(e) => resposta(400, &json!({ "ok": false, "erro": e })),
        // serve para confirmar, do outro lado, que há aqui um KanGest à escuta; o sal e a hora são públicos
        Ok((linha, _)) if linha.starts_with("GET") => resposta(
            200,
            &json!({ "ok": true, "app": "kigest", "versao": env!("CARGO_PKG_VERSION"), "cifra": "aes-256-gcm", "sal": cifra.sal, "agora": agora_ms() }),
        ),
        Ok((_, corpo)) => match serde_json::from_str::<Value>(&corpo) {
            Err(e) => resposta(400, &json!({ "ok": false, "erro": format!("Pedido inválido: {e}") })),
            Ok(v) if v.get("c").is_some() => atender_cifrado(st, cifra, &v, &endereco),
            // protocolo antigo, em claro: só nas compilações de desenvolvimento e se pedido no rede.json
            Ok(v) if cifra.sem_cifra => match serde_json::from_value::<Pedido>(v) {
                Err(e) => resposta(400, &json!({ "ok": false, "erro": format!("Pedido inválido: {e}") })),
                Ok(p) if p.chave.trim() != cifra.texto => resposta(401, &json!({ "ok": false, "erro": "Chave de rede inválida." })),
                Ok(p) => resposta(200, &executar(st, p, &endereco)),
            },
            Ok(_) => resposta(400, &json!({ "ok": false, "erro": "Este posto tem uma versão antiga do KanGest (ligação sem cifra). Actualize-o." })),
        },
    };
    let _ = fluxo.write_all(&saida);
    let _ = fluxo.flush();
}

/// Abre o pedido, confirma a hora e que não é repetido, executa e devolve a resposta cifrada.
fn atender_cifrado(st: &AppState, cifra: &Cifra, v: &Value, endereco: &str) -> Vec<u8> {
    let Some((n, claro)) = abrir(&cifra.chave, AAD_PEDIDO, v) else {
        return resposta(401, &json!({ "ok": false, "erro": "Chave de rede inválida (tem de ser igual à do servidor) ou pedido adulterado." }));
    };
    let agora = agora_ms();
    let r = match serde_json::from_slice::<Pedido>(&claro) {
        Err(e) => json!({ "ok": false, "erro": format!("Pedido inválido: {e}") }),
        Ok(p) if p.t.abs_diff(agora as i64) > DESVIO_MS => {
            // o posto acerta-se pela hora do servidor e repete o pedido
            json!({ "ok": false, "erro": "A hora deste posto e a do servidor diferem mais de 5 minutos.", "relogio": agora })
        }
        Ok(_) if repetido(n) => json!({ "ok": false, "erro": "Pedido repetido: recusado." }),
        Ok(p) => executar(st, p, endereco),
    };
    let aad = [AAD_RESPOSTA, &n[..]].concat();
    match selar(&cifra.chave, &aad, r.to_string().as_bytes()) {
        Ok((_, envelope)) => resposta(200, &envelope),
        Err(e) => resposta(400, &json!({ "ok": false, "erro": e.to_string() })),
    }
}

fn executar(st: &AppState, p: Pedido, endereco: &str) -> Value {
    // a sessão de cada posto vive no servidor, identificada pelo token do posto
    let sessao = {
        let mut l = ligacoes().lock().unwrap();
        limpar_antigas(&mut l);
        let e = l.entry(p.token.clone()).or_insert_with(|| Ligacao {
            sessao: Arc::new(Mutex::new(None)),
            posto_id: p.posto,
            posto: p.descricao_posto.clone().unwrap_or_default(),
            utilizador: None,
            endereco: endereco.to_string(),
            ultimo: crate::db::agora(),
            pedidos: 0,
        });
        e.posto_id = p.posto;
        e.endereco = endereco.to_string();
        e.ultimo = crate::db::agora();
        e.pedidos += 1;
        if let Some(d) = &p.descricao_posto {
            e.posto = d.clone();
        }
        e.sessao.clone()
    };

    let r = tauri::async_runtime::block_on(async {
        let posto = st.ler_posto(p.posto).await?;
        let ctx = Contexto { sessao: sessao.clone(), posto, api_chave: None };
        CONTEXTO.scope(ctx, services::dispatch(st, &p.metodo, p.params)).await
    });

    if let Ok(mut l) = ligacoes().lock() {
        if let Some(e) = l.get_mut(&p.token) {
            e.utilizador = sessao.lock().unwrap().as_ref().map(|s| s.nome.clone());
        }
    }
    match r {
        Ok(v) => json!({ "ok": true, "dados": v }),
        Err(e) => json!({ "ok": false, "erro": e.to_string() }),
    }
}

/// Esquece os postos sem pedidos há mais de 12 horas (a sessão desse posto cai).
fn limpar_antigas(l: &mut HashMap<String, Ligacao>) {
    let limite = (chrono::Local::now() - chrono::Duration::hours(12)).format("%Y-%m-%dT%H:%M:%S").to_string();
    l.retain(|_, e| e.ultimo > limite);
}

// ============================== cliente ==============================

fn destino(cfg: &Config) -> AppResult<std::net::SocketAddr> {
    let s = cfg.servidor.trim();
    let alvo = if s.contains(':') { s.to_string() } else { format!("{s}:{}", if cfg.porta == 0 { PORTA_PADRAO } else { cfg.porta }) };
    alvo.to_socket_addrs()
        .map_err(|e| AppError::msg(format!("Endereço do servidor inválido ({alvo}): {e}")))?
        .next()
        .ok_or_else(|| AppError::msg(format!("Endereço do servidor inválido: {alvo}")))
}

/// Um pedido HTTP ao servidor: (código, corpo JSON).
fn trocar(addr: std::net::SocketAddr, cfg: &Config, pedido: &str) -> AppResult<(u16, Value)> {
    let mut fluxo = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| AppError::msg(format!("Sem ligação ao servidor {}: {e}", cfg.servidor)))?;
    fluxo.set_read_timeout(Some(Duration::from_secs(60))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    fluxo
        .write_all(pedido.as_bytes())
        .map_err(|e| AppError::msg(format!("Falha a falar com o servidor: {e}")))?;
    let mut bruto = Vec::with_capacity(4096);
    fluxo
        .read_to_end(&mut bruto)
        .map_err(|e| AppError::msg(format!("Resposta do servidor interrompida: {e}")))?;
    let texto = String::from_utf8_lossy(&bruto);
    let (cabecalho, corpo) = texto.split_once("\r\n\r\n").ok_or_else(|| AppError::msg("Resposta do servidor ilegível."))?;
    let primeira = cabecalho.lines().next().unwrap_or("").trim();
    let codigo = primeira.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let v = serde_json::from_str(corpo.trim()).map_err(|_| AppError::msg(format!("O servidor respondeu de forma inesperada: {primeira}")))?;
    Ok((codigo, v))
}

/// Chave do posto para o servidor em uso e a diferença entre o relógio do servidor e o deste posto.
struct ChaveCliente {
    id: String,
    chave: [u8; 32],
    desvio: i64,
}

/// O Argon2 só volta a correr quando muda o servidor, a chave de rede ou o sal do servidor.
static CHAVE_CLIENTE: Mutex<Option<ChaveCliente>> = Mutex::new(None);

fn chave_cliente(cfg: &Config, addr: std::net::SocketAddr, renovar: bool) -> AppResult<([u8; 32], i64)> {
    let id = format!("{addr}|{}", cfg.chave.trim());
    if !renovar {
        if let Some(c) = CHAVE_CLIENTE.lock().unwrap().as_ref().filter(|c| c.id == id) {
            return Ok((c.chave, c.desvio));
        }
    }
    let (_, v) = trocar(addr, cfg, &format!("GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", cfg.servidor.trim()))?;
    if v["app"] != "kigest" {
        return Err(AppError::msg(format!("Em {} não responde um servidor KanGest.", cfg.servidor.trim())));
    }
    let sal = v["sal"].as_str().and_then(|s| B64.decode(s).ok()).filter(|s| s.len() >= 16).ok_or_else(|| {
        AppError::msg("O servidor tem uma versão antiga do KanGest: actualize-o primeiro (a ligação entre postos passou a ser cifrada).")
    })?;
    let desvio = v["agora"].as_i64().map_or(0, |t| t - agora_ms() as i64);
    let chave = derivar(&cfg.chave, &sal)?;
    *CHAVE_CLIENTE.lock().unwrap() = Some(ChaveCliente { id, chave, desvio });
    Ok((chave, desvio))
}

/// Envia um pedido ao servidor e devolve o resultado (ou o erro que ele deu).
fn pedir(cfg: &Config, metodo: &str, params: Value, posto_id: i64, descricao: &str) -> AppResult<Value> {
    pedir_com_token(cfg, token(), metodo, params, posto_id, descricao)
}

/// Pedido cifrado com o token de sessão indicado. Volta a tentar uma vez se o servidor mudou de sal (ex.: reinstalado)
/// e uma vez se os relógios se afastaram.
pub(crate) fn pedir_com_token(cfg: &Config, token: &str, metodo: &str, params: Value, posto_id: i64, descricao: &str) -> AppResult<Value> {
    let addr = destino(cfg)?;
    let (mut renovar, mut acertar) = (false, None::<i64>);
    for _ in 0..3 {
        let (chave, desvio) = chave_cliente(cfg, addr, renovar)?;
        let t = agora_ms() as i64 + acertar.unwrap_or(desvio);
        let claro = json!({ "token": token, "posto": posto_id, "metodo": metodo, "params": &params, "descricao_posto": descricao, "t": t });
        let (n, envelope) = selar(&chave, AAD_PEDIDO, claro.to_string().as_bytes())?;
        let corpo = envelope.to_string();
        let (codigo, v) = trocar(
            addr,
            cfg,
            &format!(
                "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                cfg.servidor.trim(),
                corpo.len()
            ),
        )?;
        if v.get("c").is_none() {
            // em claro só vêm os erros de antes de abrir o pedido; um «ok» em claro não é do servidor
            if codigo == 401 && !renovar {
                renovar = true;
                continue;
            }
            return Err(AppError::msg(v["erro"].as_str().unwrap_or("O servidor respondeu sem cifra: resposta recusada.").to_string()));
        }
        let aad = [AAD_RESPOSTA, &n[..]].concat();
        let (_, aberto) = abrir(&chave, &aad, &v).ok_or_else(|| AppError::msg("A resposta do servidor não confere (outra chave ou alterada pelo caminho)."))?;
        let r: Value = serde_json::from_slice(&aberto)?;
        if let (Some(hora), None) = (r["relogio"].as_i64(), acertar) {
            let d = hora - agora_ms() as i64;
            acertar = Some(d);
            if let Some(c) = CHAVE_CLIENTE.lock().unwrap().as_mut() {
                c.desvio = d;
            }
            continue;
        }
        return if r["ok"] == true {
            Ok(r.get("dados").cloned().unwrap_or(Value::Null))
        } else {
            Err(AppError::msg(r["erro"].as_str().unwrap_or("Erro no servidor.").to_string()))
        };
    }
    Err(AppError::msg("O servidor recusou o pedido: confirme a chave de rede e a hora dos dois computadores."))
}

/// Ajudas para os testes mandarem pedidos à mão (repetidos, alterados, com a hora errada).
#[cfg(test)]
pub(crate) mod teste {
    use super::*;

    pub(crate) fn envelope(cfg: &Config, claro: &Value) -> ([u8; 12], Value) {
        let (chave, _) = chave_cliente(cfg, destino(cfg).unwrap(), false).unwrap();
        selar(&chave, AAD_PEDIDO, claro.to_string().as_bytes()).unwrap()
    }

    pub(crate) fn abrir_resposta(cfg: &Config, n: [u8; 12], v: &Value) -> Option<Value> {
        let (chave, _) = chave_cliente(cfg, destino(cfg).unwrap(), false).unwrap();
        let (_, claro) = abrir(&chave, &[AAD_RESPOSTA, &n[..]].concat(), v)?;
        serde_json::from_slice(&claro).ok()
    }

    /// Faz de conta que o relógio deste posto se adiantou `ms`.
    pub(crate) fn desacertar(ms: i64) {
        if let Some(c) = CHAVE_CLIENTE.lock().unwrap().as_mut() {
            c.desvio -= ms;
        }
    }

    pub(crate) fn agora() -> i64 {
        agora_ms() as i64
    }
}

/// Despacha um método: localmente, ou no servidor quando esta máquina é um posto cliente.
/// É o ponto único por onde passam os pedidos do interface e os internos (ex.: impressão).
///
/// Devolve um futuro em caixa porque o ciclo é recursivo: a impressão de um talão passa por
/// aqui para ir buscar o documento (que em modo cliente está no servidor).
pub fn chamar<'a>(
    st: &'a AppState,
    metodo: &'a str,
    params: Value,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<Value>> + Send + 'a>> {
    Box::pin(despachar(st, metodo, params))
}

async fn despachar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    let cfg = config();
    if !cfg.cliente() || LOCAIS.iter().any(|l| metodo.starts_with(l)) {
        return services::dispatch(st, metodo, params).await;
    }
    let posto = st.posto();
    let descricao = posto.descricao.clone();
    // o pedido é bloqueante (rede local, respostas em milissegundos): sai do executor
    let r = tokio::task::block_in_place(|| pedir(&cfg, metodo, params, posto.id, &descricao))?;
    // a sessão também fica neste posto: os métodos locais (impressão, gaveta, rede, actualizações) exigem-na
    if let Some(s) = sessao_remota(metodo, &r) {
        st.definir_sessao(s);
    }
    // depois de entrar ou de mudar de posto, actualiza o posto que o servidor conhece
    if matches!(metodo, "auth.login" | "posto.selecionar" | "posto.guardar") {
        sincronizar_posto(st).await.ok();
    }
    Ok(r)
}

/// Sessão a guardar no posto cliente depois de um pedido ao servidor:
/// `Some(Some(s))` ao entrar ou ao confirmar a sessão, `Some(None)` ao sair, `None` nos restantes pedidos.
pub fn sessao_remota(metodo: &str, resposta: &Value) -> Option<Option<crate::state::Sessao>> {
    match metodo {
        // a sessão (e o seu estado: senha provisória, bloqueada) acompanha o servidor nos postos em rede
        "auth.login" | "auth.loginPin" | "auth.sessao" | "auth.alterarSenha" | "auth.bloquear" | "auth.desbloquear" => {
            Some(serde_json::from_value(resposta.clone()).ok())
        }
        "auth.logout" => Some(None),
        _ => None,
    }
}

#[cfg(test)]
mod testes_sessao {
    use serde_json::{json, Value};

    #[test]
    fn sessao_do_posto_cliente() {
        let s = json!({ "id": 2, "nome": "Joana", "login": "joana", "perfil": "ADMIN", "permissoes": ["impressao"], "limite_desconto": 10.0 });
        let r = super::sessao_remota("auth.login", &s).unwrap().unwrap();
        assert_eq!((r.id, r.perfil.as_str()), (2, "ADMIN"));
        assert!(super::sessao_remota("auth.sessao", &Value::Null).unwrap().is_none(), "sem sessão no servidor");
        assert!(super::sessao_remota("auth.logout", &Value::Null).unwrap().is_none());
        assert!(super::sessao_remota("documentos.emitir", &s).is_none(), "os outros pedidos não mexem na sessão");
    }
}

/// Em modo cliente, traz do servidor a configuração do posto desta máquina.
pub async fn sincronizar_posto(st: &AppState) -> AppResult<PostoAtual> {
    let cfg = config();
    if !cfg.cliente() {
        return Ok(st.posto());
    }
    let posto = st.posto();
    let descricao = posto.descricao.clone();
    let v = tokio::task::block_in_place(|| pedir(&cfg, "posto.obter", Value::Null, posto.id, &descricao))?;
    let atual = &v["atual"];
    let p = PostoAtual {
        id: atual["id"].as_i64().unwrap_or(posto.id),
        descricao: atual["descricao"].as_str().unwrap_or(&posto.descricao).to_string(),
        armazem_id: atual["armazem_id"].as_i64().unwrap_or(1),
        serie_prefixo: atual["serie_prefixo"].as_str().unwrap_or("").to_string(),
        opcoes: atual["opcoes"].clone(),
    };
    st.definir_posto(p.clone());
    Ok(p)
}

// ============================== métodos para o interface ==============================

/// Endereços desta máquina na rede local, para ditar aos outros postos.
pub(crate) fn enderecos_locais() -> Vec<String> {
    let mut v = Vec::new();
    // truque sem dependências: "ligar" um socket UDP não envia nada, mas revela o IP da interface usada
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(a) = s.local_addr() {
                v.push(a.ip().to_string());
            }
        }
    }
    v
}

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let c = config();
    let ligados: Vec<Value> = ligacoes()
        .lock()
        .unwrap()
        .values()
        .map(|e| {
            json!({
                "posto_id": e.posto_id, "posto": e.posto, "utilizador": e.utilizador,
                "endereco": e.endereco, "ultimo": e.ultimo, "pedidos": e.pedidos,
            })
        })
        .collect();
    Ok(json!({
        "modo": c.modo, "porta": c.porta, "servidor": c.servidor, "chave": c.chave,
        "servidor_activo": servidor_activo(),
        "enderecos": enderecos_locais(),
        "posto": st.posto(),
        "ligados": ligados,
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    modo: String,
    #[serde(default = "porta_padrao")]
    porta: u16,
    #[serde(default)]
    servidor: String,
    #[serde(default)]
    chave: String,
}

/// Grava a configuração de rede desta máquina (só o administrador).
pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    if !["local", "servidor", "cliente"].contains(&g.modo.as_str()) {
        return Err(AppError::msg("Modo de rede inválido."));
    }
    let actual = config();
    let c = Config {
        modo: g.modo,
        porta: if g.porta == 0 { PORTA_PADRAO } else { g.porta },
        servidor: g.servidor.trim().to_string(),
        chave: g.chave.trim().to_string(),
        sal: actual.sal,
        sem_cifra: actual.sem_cifra,
    };
    if c.modo != "local" && c.chave.chars().count() < 6 {
        return Err(AppError::msg("A chave de rede tem de ter pelo menos 6 caracteres e ser igual em todos os postos."));
    }
    if c.modo == "cliente" {
        if c.servidor.is_empty() {
            return Err(AppError::msg("Indique o endereço do servidor (ex.: 192.168.1.10)."));
        }
        // confirma logo que o servidor responde e aceita a chave
        let posto = st.posto();
        let antes = config();
        *celula().lock().unwrap() = c.clone();
        let teste = tokio::task::block_in_place(|| pedir(&c, "setup.estado", Value::Null, posto.id, &posto.descricao));
        if let Err(e) = teste {
            *celula().lock().unwrap() = antes;
            return Err(e);
        }
    }
    gravar(&st.pasta_dados, &c)?;
    let mut aviso = Value::Null;
    if c.modo == "servidor" {
        match APP.get().cloned() {
            Some(arc) => {
                iniciar_servidor(arc, &c)?;
            }
            None => aviso = json!("Feche e reabra a aplicação para abrir o servidor."),
        }
    } else if servidor_activo().is_some() {
        aviso = json!("O servidor continua aberto até fechar e reabrir a aplicação.");
    }
    if c.cliente() {
        sincronizar_posto(st).await.ok();
    }
    Ok(json!({ "aviso": aviso, "estado": estado(st).await? }))
}

/// Confirma que o servidor responde e aceita a chave desta máquina.
pub async fn testar(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let c = config();
    if !c.cliente() {
        return Err(AppError::msg("Esta máquina não está configurada como posto cliente."));
    }
    let posto = st.posto();
    let r = tokio::task::block_in_place(|| pedir(&c, "setup.estado", Value::Null, posto.id, &posto.descricao))?;
    Ok(json!({ "ok": true, "empresa": r["empresa"] }))
}

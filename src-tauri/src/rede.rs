//! Postos em rede: uma máquina guarda a base de dados (modo **servidor**) e os outros
//! postos (modo **cliente**) trabalham sobre ela, em vez de cada instalação ter a sua base.
//!
//! O servidor expõe o mesmo despachante `rpc` por HTTP/1.1 na rede local: cada pedido traz a
//! chave de rede, o n.º do posto e um identificador de posto (token) com que o servidor guarda
//! a sessão desse posto. Nada é partilhado entre postos a não ser a base de dados.
//!
//! Protocolo 2 (KanGest 1.2): o corpo de cada pedido e de cada resposta vai cifrado e autenticado
//! (AES-256-GCM) com uma chave derivada da chave de rede (PBKDF2-HMAC-SHA256, 100 000
//! iterações, com o sal do servidor). A chave de rede nunca passa na rede; um pedido com outra chave
//! não se consegue abrir. Cada pedido leva a hora e um identificador único: pedidos repetidos ou
//! com o relógio muito desacertado são recusados, e um endereço que erre a chave muitas vezes fica
//! bloqueado uns minutos. Mesmo assim, a porta é para a rede local: não a exponha à Internet.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::services;
use crate::state::{AppState, Contexto, PostoAtual, CONTEXTO};

pub const PORTA_PADRAO: u16 = 7799;
/// Comprimento mínimo de uma chave de rede nova.
pub const CHAVE_MINIMA: usize = 10;
/// Métodos que são sempre desta máquina, mesmo em modo cliente: configuração de rede,
/// impressão (a impressora é local) e gravação de ficheiros exportados.
const LOCAIS: [&str; 7] = ["rede.", "impressao.", "sistema.exportar", "atualizacao.", "licenca.abrirSite", "partilha.", "internet."];

/// Métodos que mexem na máquina onde correm (configuração de rede e de Internet, actualizações,
/// ficheiros no disco, abrir o browser): o servidor recusa-os quando chegam pela rede. A impressão
/// fica de fora de propósito: um posto no browser imprime pela impressora configurada no seu posto
/// (de rede, por IP, ou ligada ao servidor).
/// O acesso pela Internet (`web.`) liga um túnel nesta máquina: também só aqui.
const SO_NA_MAQUINA: [&str; 7] = ["rede.", "sistema.exportar", "atualizacao.", "licenca.abrirSite", "partilha.", "internet.", "web."];

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
    /// Sal da derivação da chave (servidor; gerado ao abrir o servidor pela primeira vez).
    #[serde(default)]
    pub sal: String,
    /// Servidor: também entrega a aplicação ao browser (telemóveis, tablets, terminais Android, iPad).
    #[serde(default = "verdadeiro")]
    pub web: bool,
}

fn verdadeiro() -> bool {
    true
}

fn modo_padrao() -> String {
    "local".into()
}
fn porta_padrao() -> u16 {
    PORTA_PADRAO
}

impl Default for Config {
    fn default() -> Self {
        Self { modo: modo_padrao(), porta: PORTA_PADRAO, servidor: String::new(), chave: String::new(), sal: String::new(), web: true }
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

// ============================== cifra do protocolo ==============================

pub const PROTOCOLO: u32 = 2;
const AAD_PEDIDO: &[u8] = b"kangest-rede-v2-pedido";
const AAD_RESPOSTA: &[u8] = b"kangest-rede-v2-resposta";
/// Diferença máxima entre os relógios do posto e do servidor.
const DESVIO_MAXIMO_MS: i64 = 15 * 60 * 1000;
/// Tentativas falhadas (chave errada) antes de bloquear o endereço, e por quanto tempo.
const TENTATIVAS: u32 = 10;
const BLOQUEIO: Duration = Duration::from_secs(10 * 60);

mod cifra {
    use std::num::NonZeroU32;

    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
    use ring::rand::{SecureRandom, SystemRandom};
    use serde_json::{json, Value};

    pub fn aleatorio(n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n];
        SystemRandom::new().fill(&mut b).expect("gerador aleatório do sistema");
        b
    }

    pub fn novo_sal() -> String {
        B64.encode(aleatorio(16))
    }

    /// Chave de 256 bits a partir da chave de rede e do sal do servidor (com cache: PBKDF2 é lento de propósito).
    pub fn derivar(chave: &str, sal: &str) -> [u8; 32] {
        use std::collections::HashMap;
        use std::sync::{Mutex, OnceLock};
        type Cache = Mutex<HashMap<(String, String), [u8; 32]>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let k = (chave.to_string(), sal.to_string());
        if let Some(v) = cache.lock().unwrap().get(&k) {
            return *v;
        }
        let mut out = [0u8; 32];
        ring::pbkdf2::derive(
            ring::pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(100_000).unwrap(),
            format!("kangest-rede:{sal}").as_bytes(),
            chave.as_bytes(),
            &mut out,
        );
        cache.lock().unwrap().insert(k, out);
        out
    }

    fn chave(k: &[u8; 32]) -> LessSafeKey {
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, k).expect("chave AES-256"))
    }

    /// `{"v":2,"n":nonce,"c":texto cifrado}`
    pub fn selar(k: &[u8; 32], aad: &[u8], texto: &[u8]) -> Value {
        let n: [u8; NONCE_LEN] = aleatorio(NONCE_LEN).try_into().unwrap();
        let mut dados = texto.to_vec();
        chave(k).seal_in_place_append_tag(Nonce::assume_unique_for_key(n), Aad::from(aad), &mut dados).expect("cifrar");
        json!({ "v": super::PROTOCOLO, "n": B64.encode(n), "c": B64.encode(dados) })
    }

    pub fn abrir(k: &[u8; 32], aad: &[u8], v: &Value) -> Option<Vec<u8>> {
        let n = B64.decode(v["n"].as_str()?).ok()?;
        let mut dados = B64.decode(v["c"].as_str()?).ok()?;
        let nonce = Nonce::try_assume_unique_for_key(&n).ok()?;
        chave(k).open_in_place(nonce, Aad::from(aad), &mut dados).ok().map(|t| t.to_vec())
    }
}

fn agora_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Endereços que erraram a chave: (falhas, primeira falha, bloqueado até).
type Tentativas = Mutex<HashMap<String, (u32, std::time::Instant, Option<std::time::Instant>)>>;

fn tentativas() -> &'static Tentativas {
    static T: OnceLock<Tentativas> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn bloqueado(ip: &str) -> bool {
    let mut t = tentativas().lock().unwrap();
    match t.get(ip).and_then(|e| e.2) {
        Some(ate) if ate > std::time::Instant::now() => true,
        Some(_) => {
            t.remove(ip);
            false
        }
        None => false,
    }
}

fn falhou(ip: &str) {
    let agora = std::time::Instant::now();
    let mut t = tentativas().lock().unwrap();
    let e = t.entry(ip.to_string()).or_insert((0, agora, None));
    if agora.duration_since(e.1) > BLOQUEIO {
        *e = (0, agora, None);
    }
    e.0 += 1;
    if e.0 >= TENTATIVAS {
        e.2 = Some(agora + BLOQUEIO);
    }
}

/// Identificadores de pedidos já atendidos (contra a repetição de um pedido capturado).
fn ja_visto(id: &str) -> bool {
    static VISTOS: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    let mut v = VISTOS.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    let agora = agora_ms();
    if v.len() > 50_000 {
        v.retain(|_, t| agora - *t < 2 * DESVIO_MAXIMO_MS);
    }
    v.insert(id.to_string(), agora).is_some()
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
    if let Some(p) = servidor_activo() {
        if p == cfg.porta {
            return Ok(p);
        }
        return Err(AppError::msg(format!("O servidor já está a servir na porta {p}. Feche e reabra a aplicação para mudar de porta.")));
    }
    let escuta = TcpListener::bind(("0.0.0.0", cfg.porta))
        .map_err(|e| AppError::msg(format!("Não foi possível abrir a porta {}: {e}. Escolha outra porta ou autorize a aplicação na firewall.", cfg.porta)))?;
    // o sal fica guardado no rede.json: os postos já ligados continuam a derivar a mesma chave
    let mut cfg = cfg.clone();
    if cfg.sal.trim().is_empty() {
        cfg.sal = cifra::novo_sal();
        gravar(&estado.pasta_dados, &cfg)?;
    }
    let chave = Seguranca { chave: cifra::derivar(cfg.chave.trim(), &cfg.sal), sal: cfg.sal.clone() };
    std::thread::Builder::new()
        .name("kigest-rede".into())
        .spawn(move || {
            for fluxo in escuta.incoming() {
                let Ok(fluxo) = fluxo else { continue };
                // limite de pedidos em paralelo: um posto avariado não derruba o servidor
                if ATENDER.load(Ordering::Relaxed) >= 48 {
                    continue;
                }
                ATENDER.fetch_add(1, Ordering::Relaxed);
                let st = estado.clone();
                let k = chave.clone();
                std::thread::spawn(move || {
                    atender(&st, &k, fluxo);
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

/// Chave derivada e sal do servidor.
#[derive(Clone)]
struct Seguranca {
    chave: [u8; 32],
    sal: String,
}

fn resposta(codigo: u16, corpo: &Value) -> Vec<u8> {
    let texto = corpo.to_string();
    let estado = match codigo {
        200 => "200 OK",
        401 => "401 Unauthorized",
        429 => "429 Too Many Requests",
        _ => "400 Bad Request",
    };
    format!(
        "HTTP/1.1 {estado}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{texto}",
        texto.len()
    )
    .into_bytes()
}

/// Pedido HTTP lido da ligação.
struct PedidoHttp {
    metodo: String,
    /// Caminho pedido (sem a query string).
    caminho: String,
    /// O browser aceita HTML (navegação) — distingue a página da aplicação da resposta aos postos.
    aceita_html: bool,
    aceita_gzip: bool,
    /// IP de quem pediu, dito pelo túnel da Cloudflare (`CF-Connecting-IP`); só conta se a ligação vier do próprio computador.
    ip_cloudflare: Option<std::net::IpAddr>,
    corpo: String,
}

/// Endereço de quem pede. Pelo túnel da Cloudflare (acesso pela Internet) a ligação chega do próprio computador
/// (o `cloudflared`) e o IP verdadeiro vem no `CF-Connecting-IP`: sem isto, todos os aparelhos de fora pareceriam
/// o mesmo e o bloqueio por chave errada trancaria toda a gente. O cabeçalho só se aceita vindo do próprio
/// computador — de outro posto da rede seria falsificável.
fn endereco_real(ligacao: Option<std::net::IpAddr>, ip_cloudflare: Option<std::net::IpAddr>) -> String {
    match (ligacao, ip_cloudflare) {
        (Some(l), Some(cf)) if l.is_loopback() => cf.to_string(),
        (Some(l), _) => l.to_string(),
        (None, _) => String::new(),
    }
}

/// Lê um pedido HTTP simples (linha do pedido, cabeçalhos que interessam e corpo).
fn ler_pedido(fluxo: &mut TcpStream) -> Result<PedidoHttp, String> {
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
        if fim_cabecalho.is_none() && dados.len() > 64 * 1024 {
            return Err("Cabeçalho demasiado grande.".into());
        }
    }
    let corte = fim_cabecalho.unwrap() + 4;
    let cabecalho = String::from_utf8_lossy(&dados[..corte]).to_string();
    let primeira = cabecalho.lines().next().unwrap_or("").to_string();
    let mut partes = primeira.split_whitespace();
    let metodo = partes.next().unwrap_or("").to_ascii_uppercase();
    let caminho = partes.next().unwrap_or("/").split('?').next().unwrap_or("/").to_string();
    let valor = |nome: &str| {
        cabecalho.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(nome).then(|| v.trim().to_string())
        })
    };
    let tamanho: usize = valor("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if tamanho > 32 * 1024 * 1024 {
        return Err("Pedido demasiado grande.".into());
    }
    let aceita_html = valor("accept").is_some_and(|a| a.contains("text/html"));
    let aceita_gzip = valor("accept-encoding").is_some_and(|a| a.contains("gzip"));
    let ip_cloudflare = valor("cf-connecting-ip").and_then(|v| v.parse().ok());
    let mut corpo = dados[corte..].to_vec();
    while corpo.len() < tamanho {
        let n = fluxo.read(&mut tampao).map_err(|e| format!("Leitura interrompida: {e}"))?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&tampao[..n]);
    }
    Ok(PedidoHttp { metodo, caminho, aceita_html, aceita_gzip, ip_cloudflare, corpo: String::from_utf8_lossy(&corpo).to_string() })
}

#[derive(Deserialize)]
struct Pedido {
    token: String,
    #[serde(default = "um")]
    posto: i64,
    metodo: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    descricao_posto: Option<String>,
    /// Hora do posto (ms desde 1970, UTC) e identificador único do pedido.
    t: i64,
    id: String,
}

fn um() -> i64 {
    1
}

fn atender(st: &AppState, seg: &Seguranca, mut fluxo: TcpStream) {
    fluxo.set_read_timeout(Some(Duration::from_secs(30))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let ligacao = fluxo.peer_addr().ok().map(|a| a.ip());
    let pedido = ler_pedido(&mut fluxo);
    let endereco = endereco_real(ligacao, pedido.as_ref().ok().and_then(|p| p.ip_cloudflare));
    let saida = match pedido {
        Err(e) => resposta(400, &json!({ "ok": false, "erro": e })),
        Ok(p) if p.metodo == "GET" || p.metodo == "HEAD" => {
            let web = config().web;
            if p.caminho == "/rede/info" || (p.caminho == "/" && !p.aceita_html) {
                // confirma, do outro lado, que há aqui um KanGest à escuta, e dá o sal para derivar a chave
                resposta(
                    200,
                    &json!({ "ok": true, "app": "kigest", "versao": env!("CARGO_PKG_VERSION"), "protocolo": PROTOCOLO, "sal": seg.sal, "web": web }),
                )
            } else if !web {
                crate::web::desligado()
            } else {
                // a aplicação para o browser (telemóvel, tablet, outro computador)
                crate::web::responder(&p.caminho, p.aceita_gzip, p.metodo == "HEAD")
            }
        }
        Ok(p) if p.metodo != "POST" || !(p.caminho == "/" || p.caminho == "/rpc") => {
            resposta(400, &json!({ "ok": false, "erro": "Pedido inválido." }))
        }
        Ok(_) if bloqueado(&endereco) => {
            resposta(429, &json!({ "ok": false, "erro": "Demasiadas tentativas com a chave de rede errada a partir deste posto. Aguarde 10 minutos." }))
        }
        Ok(PedidoHttp { corpo, .. }) => match serde_json::from_str::<Value>(&corpo) {
            Err(e) => resposta(400, &json!({ "ok": false, "erro": format!("Pedido inválido: {e}") })),
            // postos com o protocolo antigo (chave em claro): não se atendem
            Ok(v) if v.get("chave").is_some() => resposta(
                400,
                &json!({ "ok": false, "erro": format!("Este posto tem uma versão antiga do KanGest. Actualize-o para a versão {} (a do servidor) para voltar a trabalhar em rede.", env!("CARGO_PKG_VERSION")) }),
            ),
            Ok(v) => match cifra::abrir(&seg.chave, AAD_PEDIDO, &v) {
                None => {
                    falhou(&endereco);
                    resposta(401, &json!({ "ok": false, "erro": "Chave de rede inválida." }))
                }
                Some(texto) => {
                    let r = match serde_json::from_slice::<Pedido>(&texto) {
                        Err(e) => json!({ "ok": false, "erro": format!("Pedido inválido: {e}") }),
                        Ok(p) if (agora_ms() - p.t).abs() > DESVIO_MAXIMO_MS => json!({ "ok": false, "id": p.id, "erro": format!(
                            "O relógio deste posto difere do servidor em {} minutos. Acerte a data e a hora (nos dois computadores) e tente de novo.",
                            (agora_ms() - p.t).abs() / 60_000
                        ) }),
                        Ok(p) if ja_visto(&p.id) => json!({ "ok": false, "id": p.id, "erro": "Pedido repetido recusado." }),
                        Ok(p) => {
                            let id = p.id.clone();
                            let mut r = executar(st, p, &endereco);
                            r["id"] = json!(id);
                            r
                        }
                    };
                    resposta(200, &cifra::selar(&seg.chave, AAD_RESPOSTA, r.to_string().as_bytes()))
                }
            },
        },
    };
    let _ = fluxo.write_all(&saida);
    let _ = fluxo.flush();
}

fn executar(st: &AppState, p: Pedido, endereco: &str) -> Value {
    if SO_NA_MAQUINA.iter().any(|l| p.metodo.starts_with(l)) {
        return json!({ "ok": false, "erro": "Esta função só se usa no próprio computador (na aplicação KanGest instalada), não a partir de outro posto." });
    }
    // o browser leva o ficheiro que acabou de gerar no servidor (SAF-T, cópia de segurança)
    if p.metodo == "ficheiros.descarregar" {
        return descarregar(&p.token, &p.params);
    }
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
        Ok(v) => {
            if let Some(f) = v["ficheiro"].as_str() {
                registar_exportado(&p.token, f);
            }
            json!({ "ok": true, "dados": v })
        }
        Err(e) => json!({ "ok": false, "erro": e.to_string() }),
    }
}

/// Ficheiros que cada posto gerou no servidor e pode levar (só esse posto, durante uma hora).
fn exportados() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    static E: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(HashMap::new()))
}

const EXPORTADO_VALIDO: Duration = Duration::from_secs(3600);
/// Maior ficheiro que se entrega pela rede (vai em base64 dentro da resposta cifrada).
const EXPORTADO_MAXIMO: u64 = 200 * 1024 * 1024;

fn registar_exportado(token: &str, ficheiro: &str) {
    let mut e = exportados().lock().unwrap();
    e.retain(|_, t| t.elapsed() < EXPORTADO_VALIDO);
    e.insert(format!("{token}\n{ficheiro}"), std::time::Instant::now());
}

fn descarregar(token: &str, params: &Value) -> Value {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let ficheiro = params["ficheiro"].as_str().unwrap_or("");
    let valido = exportados().lock().unwrap().get(&format!("{token}\n{ficheiro}")).is_some_and(|t| t.elapsed() < EXPORTADO_VALIDO);
    if !valido {
        return json!({ "ok": false, "erro": "Ficheiro indisponível: gere-o de novo neste posto." });
    }
    let caminho = Path::new(ficheiro);
    match std::fs::metadata(caminho) {
        Ok(m) if m.len() > EXPORTADO_MAXIMO => {
            json!({ "ok": false, "erro": format!("O ficheiro tem {} MB: é grande demais para levar pelo browser. Copie-o do servidor: {ficheiro}", m.len() / 1_048_576) })
        }
        Ok(_) => match std::fs::read(caminho) {
            Ok(b) => {
                let nome = caminho.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "ficheiro".into());
                json!({ "ok": true, "dados": { "nome": nome, "base64": STANDARD.encode(b) } })
            }
            Err(e) => json!({ "ok": false, "erro": format!("Não foi possível ler o ficheiro: {e}") }),
        },
        Err(e) => json!({ "ok": false, "erro": format!("Não foi possível ler o ficheiro: {e}") }),
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

/// Pedido HTTP simples ao servidor; devolve o JSON do corpo da resposta.
fn http(cfg: &Config, metodo: &str, corpo: Option<&str>) -> AppResult<Value> {
    let addr = destino(cfg)?;
    let mut fluxo = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| AppError::msg(format!("Sem ligação ao servidor {}: {e}", cfg.servidor)))?;
    fluxo.set_read_timeout(Some(Duration::from_secs(60))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let corpo = corpo.unwrap_or("");
    // servidores antigos respondem com a informação a qualquer GET; os novos servem a aplicação web em "/"
    let caminho = if metodo == "GET" { "/rede/info" } else { "/" };
    let pedido = format!(
        "{metodo} {caminho} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        cfg.servidor.trim(),
        corpo.len()
    );
    fluxo
        .write_all(pedido.as_bytes())
        .map_err(|e| AppError::msg(format!("Falha a falar com o servidor: {e}")))?;
    let mut bruto = Vec::with_capacity(4096);
    fluxo
        .read_to_end(&mut bruto)
        .map_err(|e| AppError::msg(format!("Resposta do servidor interrompida: {e}")))?;
    let texto = String::from_utf8_lossy(&bruto);
    let (cabecalho, corpo) = texto.split_once("\r\n\r\n").ok_or_else(|| AppError::msg("Resposta do servidor ilegível."))?;
    serde_json::from_str(corpo.trim())
        .map_err(|_| AppError::msg(format!("O servidor respondeu de forma inesperada: {}", cabecalho.lines().next().unwrap_or("").trim())))
}

/// Sal do servidor (para derivar a chave), guardado depois da primeira vez.
fn sal_do_servidor(cfg: &Config, renovar: bool) -> AppResult<String> {
    static SAIS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let sais = SAIS.get_or_init(|| Mutex::new(HashMap::new()));
    let alvo = destino(cfg)?.to_string();
    if !renovar {
        if let Some(s) = sais.lock().unwrap().get(&alvo) {
            return Ok(s.clone());
        }
    }
    let v = http(cfg, "GET", None)?;
    let sal = v["sal"].as_str().filter(|s| !s.is_empty()).map(String::from).ok_or_else(|| {
        AppError::msg(format!(
            "O servidor tem uma versão antiga do KanGest (sem rede cifrada). Actualize o servidor para a versão {} ou superior.",
            env!("CARGO_PKG_VERSION")
        ))
    })?;
    sais.lock().unwrap().insert(alvo, sal.clone());
    Ok(sal)
}

/// Envia um pedido ao servidor e devolve o resultado (ou o erro que ele deu).
fn pedir(cfg: &Config, metodo: &str, params: Value, posto_id: i64, descricao: &str) -> AppResult<Value> {
    pedir_com_token(cfg, token(), metodo, params, posto_id, descricao)
}

/// Pedido cifrado (protocolo 2) em nome de um posto (`token` identifica a sessão dele no servidor).
pub fn pedir_com_token(cfg: &Config, token: &str, metodo: &str, params: Value, posto_id: i64, descricao: &str) -> AppResult<Value> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    for tentativa in 0..2 {
        let sal = sal_do_servidor(cfg, tentativa > 0)?;
        let k = cifra::derivar(cfg.chave.trim(), &sal);
        let id = URL_SAFE_NO_PAD.encode(cifra::aleatorio(16));
        let texto = json!({
            "token": token, "posto": posto_id, "metodo": metodo, "params": params.clone(),
            "descricao_posto": descricao, "t": agora_ms(), "id": id,
        })
        .to_string();
        let corpo = cifra::selar(&k, AAD_PEDIDO, texto.as_bytes()).to_string();
        let v = http(cfg, "POST", Some(&corpo))?;
        if v.get("c").is_some() {
            let aberto = cifra::abrir(&k, AAD_RESPOSTA, &v).ok_or_else(|| AppError::msg("Resposta do servidor ilegível (a chave de rede é diferente?)."))?;
            let r: Value = serde_json::from_slice(&aberto)?;
            if r["id"].as_str() != Some(id.as_str()) {
                return Err(AppError::msg("A resposta do servidor não corresponde ao pedido."));
            }
            return if r["ok"] == Value::Bool(true) {
                Ok(r.get("dados").cloned().unwrap_or(Value::Null))
            } else {
                Err(AppError::msg(r["erro"].as_str().unwrap_or("Erro no servidor.").to_string()))
            };
        }
        let erro = v["erro"].as_str().unwrap_or("Erro no servidor.").to_string();
        // o sal pode ter mudado (servidor reinstalado): tenta uma vez com o sal novo
        if tentativa == 0 && erro.contains("Chave de rede inválida") {
            continue;
        }
        return Err(AppError::msg(erro));
    }
    Err(AppError::msg("Chave de rede inválida."))
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
    // o posto escolhido é desta máquina: fica gravado aqui, não no servidor
    if metodo == "posto.selecionar" {
        if let Some(id) = r["id"].as_i64() {
            st.definir_posto(PostoAtual { id, ..st.posto() });
            let _ = std::fs::write(st.pasta_dados.join("posto.json"), json!({ "posto_id": id }).to_string());
        }
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
    let s = st.sessao()?;
    let c = config();
    // a chave de rede só a vê quem pode mudar a configuração
    let admin = s.tem("sistema.configuracoes");
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
        "modo": c.modo, "porta": c.porta, "servidor": c.servidor,
        "chave": if admin { c.chave.clone() } else { String::new() },
        "chave_curta": c.modo != "local" && c.chave.chars().count() < CHAVE_MINIMA,
        "protocolo": PROTOCOLO,
        "servidor_activo": servidor_activo(),
        "enderecos": enderecos_locais(),
        "web": c.web,
        "web_disponivel": crate::web::disponivel(),
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
    #[serde(default)]
    web: Option<bool>,
}

/// Grava a configuração de rede desta máquina (só o administrador).
pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    if !["local", "servidor", "cliente"].contains(&g.modo.as_str()) {
        return Err(AppError::msg("Modo de rede inválido."));
    }
    let antes = config();
    // o sal mantém-se (os postos já ligados continuam a derivar a mesma chave)
    let c = Config {
        modo: g.modo,
        porta: if g.porta == 0 { PORTA_PADRAO } else { g.porta },
        servidor: g.servidor.trim().to_string(),
        chave: g.chave.trim().to_string(),
        sal: antes.sal.clone(),
        web: g.web.unwrap_or(antes.web),
    };
    // chave nova: pelo menos CHAVE_MINIMA caracteres; uma chave antiga mais curta pode manter-se
    if c.modo != "local" && c.chave.chars().count() < CHAVE_MINIMA && !(c.chave == antes.chave && c.chave.chars().count() >= 6) {
        return Err(AppError::msg(format!(
            "A chave de rede tem de ter pelo menos {CHAVE_MINIMA} caracteres e ser igual em todos os postos. Use «Gerar chave»."
        )));
    }
    if c.modo == "cliente" {
        if c.servidor.is_empty() {
            return Err(AppError::msg("Indique o endereço do servidor (ex.: 192.168.1.10)."));
        }
        // confirma logo que o servidor responde e aceita a chave
        let posto = st.posto();
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

/// Testes: um pedido cifrado tal como iria na rede (para o repetir) e a resposta aberta.
#[cfg(test)]
pub fn teste_pedido_cifrado(cfg: &Config, token: &str, metodo: &str, params: Value, posto: i64) -> (String, [u8; 32]) {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let sal = sal_do_servidor(cfg, false).unwrap();
    let k = cifra::derivar(cfg.chave.trim(), &sal);
    let texto = json!({ "token": token, "posto": posto, "metodo": metodo, "params": params, "t": agora_ms(), "id": URL_SAFE_NO_PAD.encode(cifra::aleatorio(16)) });
    (cifra::selar(&k, AAD_PEDIDO, texto.to_string().as_bytes()).to_string(), k)
}

#[cfg(test)]
pub fn teste_abrir_resposta(k: &[u8; 32], http: &str) -> Value {
    let corpo = http.split_once("\r\n\r\n").expect("resposta HTTP").1;
    let v: Value = serde_json::from_str(corpo.trim()).expect("JSON");
    serde_json::from_slice(&cifra::abrir(k, AAD_RESPOSTA, &v).expect("resposta cifrada com a chave do posto")).unwrap()
}

#[cfg(test)]
mod testes_endereco {
    use super::endereco_real;

    #[test]
    fn ip_do_tunel_so_vindo_do_proprio_computador() {
        let ip = |s: &str| Some(s.parse().unwrap());
        // pelo túnel (cloudflared no próprio computador): vale o IP de quem está do outro lado
        assert_eq!(endereco_real(ip("127.0.0.1"), ip("41.63.10.20")), "41.63.10.20");
        assert_eq!(endereco_real(ip("::1"), ip("2001:db8::7")), "2001:db8::7");
        // de outro posto da rede, o cabeçalho é ignorado (seria falsificável)
        assert_eq!(endereco_real(ip("192.168.1.30"), ip("41.63.10.20")), "192.168.1.30");
        // sem cabeçalho, a ligação
        assert_eq!(endereco_real(ip("127.0.0.1"), None), "127.0.0.1");
        assert_eq!(endereco_real(None, None), "");
    }
}

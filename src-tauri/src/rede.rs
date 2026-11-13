//! Postos em rede: uma máquina guarda a base de dados (modo **servidor**) e os outros
//! postos (modo **cliente**) trabalham sobre ela, em vez de cada instalação ter a sua base.
//!
//! O servidor expõe o mesmo despachante `rpc` por HTTP/1.1 na rede local: cada pedido traz a
//! chave de rede, o n.º do posto e um identificador de posto (token) com que o servidor guarda
//! a sessão desse posto. Nada é partilhado entre postos a não ser a base de dados.
//!
//! Fica deliberadamente simples: HTTP em texto na rede local, sem TLS (como o SQL Server do
//! KIGEST original), protegido por uma chave de rede. Não expor a porta à Internet.

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
}

fn modo_padrao() -> String {
    "local".into()
}
fn porta_padrao() -> u16 {
    PORTA_PADRAO
}

impl Default for Config {
    fn default() -> Self {
        Self { modo: modo_padrao(), porta: PORTA_PADRAO, servidor: String::new(), chave: String::new() }
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
    if let Some(p) = servidor_activo() {
        if p == cfg.porta {
            return Ok(p);
        }
        return Err(AppError::msg(format!("O servidor já está a servir na porta {p}. Feche e reabra a aplicação para mudar de porta.")));
    }
    let escuta = TcpListener::bind(("0.0.0.0", cfg.porta))
        .map_err(|e| AppError::msg(format!("Não foi possível abrir a porta {}: {e}. Escolha outra porta ou autorize a aplicação na firewall.", cfg.porta)))?;
    let chave = cfg.chave.trim().to_string();
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
    chave: String,
    token: String,
    #[serde(default = "um")]
    posto: i64,
    metodo: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    descricao_posto: Option<String>,
}

fn um() -> i64 {
    1
}

fn atender(st: &AppState, chave: &str, mut fluxo: TcpStream) {
    fluxo.set_read_timeout(Some(Duration::from_secs(30))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let endereco = fluxo.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
    let saida = match ler_pedido(&mut fluxo) {
        Err(e) => resposta(400, &json!({ "ok": false, "erro": e })),
        Ok((linha, corpo)) => {
            if linha.starts_with("GET") {
                // serve para confirmar, do outro lado, que há aqui um KanGest à escuta
                resposta(200, &json!({ "ok": true, "app": "kigest", "versao": env!("CARGO_PKG_VERSION") }))
            } else {
                match serde_json::from_str::<Pedido>(&corpo) {
                    Err(e) => resposta(400, &json!({ "ok": false, "erro": format!("Pedido inválido: {e}") })),
                    Ok(p) if p.chave != chave => resposta(401, &json!({ "ok": false, "erro": "Chave de rede inválida." })),
                    Ok(p) => resposta(200, &executar(st, p, &endereco)),
                }
            }
        }
    };
    let _ = fluxo.write_all(&saida);
    let _ = fluxo.flush();
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
        let ctx = Contexto { sessao: sessao.clone(), posto };
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

/// Envia um pedido ao servidor e devolve o resultado (ou o erro que ele deu).
fn pedir(cfg: &Config, metodo: &str, params: Value, posto_id: i64, descricao: &str) -> AppResult<Value> {
    let addr = destino(cfg)?;
    let corpo = json!({
        "chave": cfg.chave, "token": token(), "posto": posto_id,
        "metodo": metodo, "params": params, "descricao_posto": descricao,
    })
    .to_string();
    let mut fluxo = TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .map_err(|e| AppError::msg(format!("Sem ligação ao servidor {}: {e}", cfg.servidor)))?;
    fluxo.set_read_timeout(Some(Duration::from_secs(60))).ok();
    fluxo.set_write_timeout(Some(Duration::from_secs(30))).ok();
    let pedido = format!(
        "POST / HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
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
    let v: Value = serde_json::from_str(corpo.trim())
        .map_err(|_| AppError::msg(format!("O servidor respondeu de forma inesperada: {}", cabecalho.lines().next().unwrap_or("").trim())))?;
    if v["ok"] == Value::Bool(true) {
        Ok(v.get("dados").cloned().unwrap_or(Value::Null))
    } else {
        Err(AppError::msg(v["erro"].as_str().unwrap_or("Erro no servidor.").to_string()))
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
fn enderecos_locais() -> Vec<String> {
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
    let c = Config { modo: g.modo, porta: if g.porta == 0 { PORTA_PADRAO } else { g.porta }, servidor: g.servidor.trim().to_string(), chave: g.chave.trim().to_string() };
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

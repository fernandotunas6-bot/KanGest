//! Cópias de segurança (Fr_CopiaSeguranca do KIGEST).
//!
//! - Automáticas: ao abrir o programa e de N em N horas enquanto está aberto (por omissão 4), e no
//!   fecho de caixa. Ficam em `copias/` na pasta de dados (guardam-se as últimas N, por omissão 30).
//! - Se houver uma pasta externa configurada (pen, disco de rede, pasta do OneDrive ou Google Drive),
//!   cada cópia é também gravada lá, opcionalmente cifrada com uma senha (AES-256-GCM).
//! - Cada cópia é verificada (`PRAGMA integrity_check`) logo depois de criada.
//! - Restauro pelo programa: a cópia é verificada, a base actual é guardada e a troca faz-se ao
//!   reiniciar, antes de a base de dados ser aberta (`aplicar_restauro_pendente`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MANTER_PADRAO: usize = 30;
const INTERVALO_PADRAO: i64 = 4;
const MAGIA_CIFRADA: &[u8] = b"KGCOPIA1";
const MARCA_RESTAURO: &str = "restaurar.json";

/// Cópia consistente da base (mesmo com a aplicação em uso) via `VACUUM INTO`.
pub async fn copiar(pool: &SqlitePool, destino: &Path) -> AppResult<()> {
    if destino.exists() {
        std::fs::remove_file(destino).map_err(|e| AppError::msg(format!("Não foi possível substituir {}: {e}", destino.display())))?;
    }
    sqlx::query("VACUUM INTO ?").bind(destino.to_string_lossy().to_string()).execute(pool).await?;
    Ok(())
}

pub async fn backup(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let nome = format!("kangest-copia-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let destino = st.pasta_exportacao.join(nome);
    copiar(&st.pool, &destino).await?;
    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);
    Ok(json!({ "ficheiro": destino.display().to_string(), "bytes": bytes }))
}

fn pasta_copias(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("copias")
}

// ------------------------------------------------------------------ configuração

struct Cfg {
    intervalo_horas: i64,
    manter: usize,
    pasta_externa: String,
    cifrar_externa: bool,
}

async fn cfg_valor(pool: &SqlitePool, k: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(pool).await?)
}

async fn cfg_gravar(pool: &SqlitePool, k: &str, v: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(pool).await?;
    Ok(())
}

async fn ler_cfg(pool: &SqlitePool) -> AppResult<Cfg> {
    Ok(Cfg {
        intervalo_horas: cfg_valor(pool, "copias.intervalo_horas").await?.and_then(|v| v.parse().ok()).unwrap_or(INTERVALO_PADRAO).clamp(0, 168),
        manter: cfg_valor(pool, "copias.manter").await?.and_then(|v| v.parse().ok()).unwrap_or(MANTER_PADRAO).clamp(3, 365),
        pasta_externa: cfg_valor(pool, "copias.pasta_externa").await?.unwrap_or_default(),
        cifrar_externa: cfg_valor(pool, "copias.cifrar_externa").await?.as_deref() == Some("1"),
    })
}

// ------------------------------------------------------------------ cifra das cópias externas

fn chave_senha(senha: &str, sal: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 32];
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, std::num::NonZeroU32::new(100_000).unwrap(), sal, senha.as_bytes(), &mut k);
    k
}

/// `KGCOPIA1` + sal (16) + nonce (12) + base de dados cifrada (AES-256-GCM).
fn cifrar_bytes(dados: &[u8], senha: &str) -> Vec<u8> {
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};
    let mut sal = [0u8; 16];
    let mut n = [0u8; 12];
    SystemRandom::new().fill(&mut sal).expect("aleatório");
    SystemRandom::new().fill(&mut n).expect("aleatório");
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &chave_senha(senha, &sal)).expect("chave"));
    let mut c = dados.to_vec();
    k.seal_in_place_append_tag(Nonce::assume_unique_for_key(n), Aad::from(MAGIA_CIFRADA), &mut c).expect("cifrar");
    let mut out = Vec::with_capacity(c.len() + 36);
    out.extend_from_slice(MAGIA_CIFRADA);
    out.extend_from_slice(&sal);
    out.extend_from_slice(&n);
    out.extend_from_slice(&c);
    out
}

fn decifrar_bytes(dados: &[u8], senha: &str) -> AppResult<Vec<u8>> {
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    let m = MAGIA_CIFRADA.len();
    if dados.len() < m + 28 + 16 || &dados[..m] != MAGIA_CIFRADA {
        return Err(AppError::msg("O ficheiro não é uma cópia cifrada do KanGest."));
    }
    let (sal, resto) = dados[m..].split_at(16);
    let (n, c) = resto.split_at(12);
    let k = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &chave_senha(senha, sal)).expect("chave"));
    let mut buf = c.to_vec();
    let nonce = Nonce::try_assume_unique_for_key(n).map_err(|_| AppError::msg("Cópia cifrada danificada."))?;
    let t = k
        .open_in_place(nonce, Aad::from(MAGIA_CIFRADA), &mut buf)
        .map_err(|_| AppError::msg("Senha errada (ou cópia danificada)."))?;
    Ok(t.to_vec())
}

fn e_cifrada(f: &Path) -> bool {
    std::fs::File::open(f)
        .and_then(|mut x| {
            use std::io::Read;
            let mut b = [0u8; 8];
            x.read_exact(&mut b).map(|_| b)
        })
        .map(|b| b == MAGIA_CIFRADA)
        .unwrap_or(false)
}

// ------------------------------------------------------------------ verificação

/// Abre a cópia só para leitura e confirma que é uma base do KanGest íntegra.
pub async fn verificar_ficheiro(f: &Path) -> AppResult<()> {
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::{ConnectOptions, Connection};
    let mut c = SqliteConnectOptions::new()
        .filename(f)
        .read_only(true)
        .connect()
        .await
        .map_err(|e| AppError::msg(format!("{} não é uma cópia de segurança do KanGest ({e}).", f.display())))?;
    let r: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut c)
        .await
        .map_err(|e| AppError::msg(format!("{} não é uma cópia de segurança do KanGest ({e}).", f.display())))?;
    let tabelas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('documentos', 'config', 'empresa', '_sqlx_migrations')")
        .fetch_one(&mut c)
        .await?;
    c.close().await.ok();
    if r != "ok" {
        return Err(AppError::msg(format!("A cópia está danificada ({r}).")));
    }
    if tabelas < 4 {
        return Err(AppError::msg("O ficheiro não é uma cópia de segurança do KanGest."));
    }
    Ok(())
}

// ------------------------------------------------------------------ cópias automáticas

fn listar_db(dir: &Path, prefixo: &str) -> Vec<(std::time::SystemTime, PathBuf)> {
    let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.starts_with(prefixo) && (n.ends_with(".db") || n.ends_with(".kgc"))
                })
                .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn podar(dir: &Path, prefixo: &str, manter: usize) {
    let mut v = listar_db(dir, prefixo);
    while v.len() > manter {
        let f = v.remove(0).1;
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(f.with_extension("verificada"));
    }
}

/// Faz uma cópia agora: local, verificada e (se configurada) na pasta externa. Devolve o resumo.
pub async fn fazer_copia(pool: &SqlitePool, pasta_dados: &Path, motivo: &str) -> AppResult<Value> {
    let c = ler_cfg(pool).await?;
    let dir = pasta_copias(pasta_dados);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    let carimbo = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let destino = dir.join(format!("kangest-{carimbo}.db"));
    copiar(pool, &destino).await?;
    verificar_ficheiro(&destino).await?;
    std::fs::write(destino.with_extension("verificada"), crate::db::agora()).ok();
    podar(&dir, "kangest-", c.manter);
    cfg_gravar(pool, "copias.ultima", &crate::db::agora()).await?;
    // pasta externa (a pen pode não estar ligada: fica registado o erro, a cópia local já está feita)
    let mut externa = Value::Null;
    if !c.pasta_externa.trim().is_empty() {
        let r = copiar_para_externa(pool, &destino, &c, &carimbo).await;
        match r {
            Ok(f) => {
                cfg_gravar(pool, "copias.ultima_externa", &crate::db::agora()).await?;
                cfg_gravar(pool, "copias.erro_externa", "").await?;
                externa = json!(f);
            }
            Err(e) => {
                cfg_gravar(pool, "copias.erro_externa", &format!("{} — {e}", crate::db::agora())).await?;
            }
        }
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (NULL, 'COPIA_SEGURANCA', ?)")
        .bind(format!("{motivo}: {}", destino.file_name().unwrap_or_default().to_string_lossy()))
        .execute(pool)
        .await?;
    Ok(json!({ "ficheiro": destino.display().to_string(), "externa": externa }))
}

async fn copiar_para_externa(pool: &SqlitePool, local: &Path, c: &Cfg, carimbo: &str) -> AppResult<String> {
    let dir = PathBuf::from(c.pasta_externa.trim());
    if !dir.is_dir() {
        return Err(AppError::msg(format!("a pasta externa {} não está acessível (pen desligada?)", dir.display())));
    }
    let nif: String = sqlx::query_scalar("SELECT nif FROM empresa WHERE id = 1").fetch_optional(pool).await?.unwrap_or_default();
    let prefixo = format!("kangest-{}-", nif.trim());
    let dados = std::fs::read(local).map_err(|e| AppError::msg(e.to_string()))?;
    let (nome, conteudo) = if c.cifrar_externa {
        let senha = crate::segredos::ler(pool, "copias.senha").await?.filter(|s| !s.is_empty()).ok_or_else(|| AppError::msg("falta a senha das cópias cifradas"))?;
        (format!("{prefixo}{carimbo}.kgc"), cifrar_bytes(&dados, &senha))
    } else {
        (format!("{prefixo}{carimbo}.db"), dados)
    };
    let destino = dir.join(&nome);
    std::fs::write(&destino, conteudo).map_err(|e| AppError::msg(format!("não foi possível gravar em {}: {e}", dir.display())))?;
    podar(&dir, &prefixo, c.manter);
    Ok(destino.display().to_string())
}

/// Ao abrir o programa: uma cópia se a última tem mais do que o intervalo (ou é de outro dia).
pub async fn backup_automatico(pool: &SqlitePool, pasta_dados: &Path) -> AppResult<Option<Value>> {
    if precisa_copia(pool, pasta_dados, true).await? {
        return Ok(Some(fazer_copia(pool, pasta_dados, "Cópia automática ao abrir").await?));
    }
    Ok(None)
}

async fn precisa_copia(pool: &SqlitePool, pasta_dados: &Path, ao_abrir: bool) -> AppResult<bool> {
    let c = ler_cfg(pool).await?;
    let ultima = listar_db(&pasta_copias(pasta_dados), "kangest-").last().map(|x| x.0);
    let Some(ultima) = ultima else { return Ok(true) };
    let idade = std::time::SystemTime::now().duration_since(ultima).unwrap_or_default();
    let dia = |t: std::time::SystemTime| chrono::DateTime::<chrono::Local>::from(t).date_naive();
    if ao_abrir && dia(ultima) != chrono::Local::now().date_naive() {
        return Ok(true);
    }
    Ok(c.intervalo_horas > 0 && idade >= std::time::Duration::from_secs(c.intervalo_horas as u64 * 3600))
}

/// Cópias periódicas enquanto o programa está aberto (só na máquina que guarda a base de dados).
pub fn iniciar_agendamento(st: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10 * 60)).await;
            if crate::rede::config().cliente() {
                continue;
            }
            match precisa_copia(&st.pool, &st.pasta_dados, false).await {
                Ok(true) => {
                    if let Err(e) = fazer_copia(&st.pool, &st.pasta_dados, "Cópia automática periódica").await {
                        eprintln!("Cópia de segurança periódica falhou: {e}");
                    }
                }
                Ok(false) => {}
                Err(e) => eprintln!("Cópia de segurança periódica: {e}"),
            }
        }
    });
}

/// Cópia num momento importante (ex.: fecho de caixa). Não impede a operação se falhar.
pub async fn copia_evento(st: &AppState, motivo: &str) {
    if crate::rede::config().cliente() {
        return;
    }
    if let Err(e) = fazer_copia(&st.pool, &st.pasta_dados, motivo).await {
        eprintln!("Cópia de segurança ({motivo}) falhou: {e}");
    }
}

// ------------------------------------------------------------------ RPC

pub async fn copias(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let dir = pasta_copias(&st.pasta_dados);
    let c = ler_cfg(&st.pool).await?;
    let item = |(t, f): &(std::time::SystemTime, PathBuf)| {
        json!({
            "ficheiro": f.display().to_string(),
            "nome": f.file_name().unwrap_or_default().to_string_lossy(),
            "bytes": std::fs::metadata(f).map(|m| m.len()).unwrap_or(0),
            "data": chrono::DateTime::<chrono::Local>::from(*t).format("%Y-%m-%dT%H:%M:%S").to_string(),
            "verificada": f.with_extension("verificada").exists(),
            "cifrada": f.extension().is_some_and(|e| e == "kgc"),
        })
    };
    let mut locais: Vec<Value> = listar_db(&dir, "kangest").iter().map(item).collect();
    locais.reverse();
    let externa_dir = PathBuf::from(c.pasta_externa.trim());
    let mut externas: Vec<Value> = if !c.pasta_externa.trim().is_empty() && externa_dir.is_dir() {
        listar_db(&externa_dir, "kangest-").iter().map(item).collect()
    } else {
        Vec::new()
    };
    externas.reverse();
    Ok(json!({
        "pasta": dir.display().to_string(),
        "copias": locais,
        "externas": externas,
        "config": {
            "intervalo_horas": c.intervalo_horas, "manter": c.manter, "pasta_externa": c.pasta_externa,
            "cifrar_externa": c.cifrar_externa,
            "tem_senha": crate::segredos::ler(&st.pool, "copias.senha").await.ok().flatten().is_some_and(|s| !s.is_empty()),
        },
        "estado": {
            "ultima": cfg_valor(&st.pool, "copias.ultima").await?,
            "ultima_externa": cfg_valor(&st.pool, "copias.ultima_externa").await?,
            "erro_externa": cfg_valor(&st.pool, "copias.erro_externa").await?.filter(|e| !e.is_empty()),
            "pasta_externa_acessivel": !c.pasta_externa.trim().is_empty() && externa_dir.is_dir(),
        },
        "posto_cliente": crate::rede::config().cliente(),
    }))
}

#[derive(Deserialize)]
pub struct CopiasCfgIn {
    intervalo_horas: i64,
    manter: usize,
    #[serde(default)]
    pasta_externa: String,
    #[serde(default)]
    cifrar_externa: bool,
    /// `None` mantém a senha guardada.
    #[serde(default)]
    senha: Option<String>,
}

pub async fn copias_guardar(st: &AppState, g: CopiasCfgIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.backup")?;
    let pasta = g.pasta_externa.trim().to_string();
    if !pasta.is_empty() {
        let d = PathBuf::from(&pasta);
        if !d.is_dir() {
            return Err(AppError::msg(format!("A pasta {pasta} não existe ou não está acessível.")));
        }
        // confirma que se consegue escrever lá
        let teste = d.join(".kangest-teste");
        std::fs::write(&teste, b"ok").map_err(|e| AppError::msg(format!("Não é possível gravar em {pasta}: {e}")))?;
        let _ = std::fs::remove_file(teste);
    }
    if let Some(senha) = g.senha.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        if senha.chars().count() < 8 {
            return Err(AppError::msg("A senha das cópias cifradas tem de ter pelo menos 8 caracteres."));
        }
        crate::segredos::gravar(&st.pool, "copias.senha", senha).await?;
    }
    if g.cifrar_externa && crate::segredos::ler(&st.pool, "copias.senha").await?.is_none_or(|x| x.is_empty()) {
        return Err(AppError::msg("Indique a senha das cópias cifradas. Guarde-a num sítio seguro: sem ela a cópia não se restaura."));
    }
    cfg_gravar(&st.pool, "copias.intervalo_horas", &g.intervalo_horas.clamp(0, 168).to_string()).await?;
    cfg_gravar(&st.pool, "copias.manter", &g.manter.clamp(3, 365).to_string()).await?;
    cfg_gravar(&st.pool, "copias.pasta_externa", &pasta).await?;
    cfg_gravar(&st.pool, "copias.cifrar_externa", if g.cifrar_externa { "1" } else { "0" }).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'COPIAS_CONFIG', ?)")
        .bind(s.id)
        .bind(format!("De {} em {} h, manter {}, externa: {}", g.intervalo_horas, g.intervalo_horas, g.manter, if pasta.is_empty() { "não" } else { &pasta }))
        .execute(&st.pool)
        .await?;
    copias(st).await
}

pub async fn copia_agora(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    if crate::rede::config().cliente() {
        return Err(AppError::msg("Neste posto os dados estão no servidor: as cópias fazem-se no servidor."));
    }
    fazer_copia(&st.pool, &st.pasta_dados, "Cópia manual").await
}

#[derive(Deserialize)]
pub struct RestaurarIn {
    ficheiro: String,
    #[serde(default)]
    senha: Option<String>,
}

/// Prepara o restauro de uma cópia: confere-a, guarda a base actual e marca a troca para o
/// próximo arranque (a base em uso não pode ser substituída com o programa aberto).
async fn preparar_restauro(st: &AppState, r: &RestaurarIn, guardar_atual: bool) -> AppResult<Value> {
    if crate::state::contexto().is_some() || crate::rede::config().cliente() {
        return Err(AppError::msg("O restauro faz-se na máquina que guarda a base de dados (o servidor), não num posto em rede."));
    }
    let origem = PathBuf::from(r.ficheiro.trim());
    if !origem.is_file() {
        return Err(AppError::msg(format!("Não encontro o ficheiro {}.", origem.display())));
    }
    let dir = pasta_copias(&st.pasta_dados);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    let pronto = dir.join("restauro-pendente.db");
    if e_cifrada(&origem) {
        let senha = match r.senha.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
            Some(x) => x.to_string(),
            None => crate::segredos::ler(&st.pool, "copias.senha").await.ok().flatten().filter(|x| !x.is_empty()).ok_or_else(|| AppError::msg("Esta cópia está cifrada: indique a senha."))?,
        };
        let dados = std::fs::read(&origem).map_err(|e| AppError::msg(e.to_string()))?;
        std::fs::write(&pronto, decifrar_bytes(&dados, &senha)?).map_err(|e| AppError::msg(e.to_string()))?;
    } else {
        std::fs::copy(&origem, &pronto).map_err(|e| AppError::msg(format!("Não foi possível copiar {}: {e}", origem.display())))?;
    }
    if let Err(e) = verificar_ficheiro(&pronto).await {
        let _ = std::fs::remove_file(&pronto);
        return Err(e);
    }
    let seguranca = if guardar_atual {
        let f = dir.join(format!("kangest-antes-do-restauro-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
        copiar(&st.pool, &f).await?;
        Some(f.display().to_string())
    } else {
        None
    };
    std::fs::write(
        st.pasta_dados.join(MARCA_RESTAURO),
        json!({ "ficheiro": pronto.display().to_string(), "origem": origem.display().to_string(), "pedido_em": crate::db::agora() }).to_string(),
    )
    .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(json!({ "reiniciar": true, "copia_da_base_atual": seguranca }))
}

pub async fn restaurar(st: &AppState, r: RestaurarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.backup")?;
    if !s.tem("sistema.configuracoes") {
        return Err(AppError::SemPermissao);
    }
    let v = preparar_restauro(st, &r, true).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'RESTAURO_PEDIDO', ?)")
        .bind(s.id)
        .bind(format!("Restauro de {} ao reiniciar", r.ficheiro.trim()))
        .execute(&st.pool)
        .await?;
    Ok(v)
}

/// Primeiro arranque num computador novo: restaurar uma cópia em vez de configurar do zero.
pub async fn restaurar_inicial(st: &AppState, r: RestaurarIn) -> AppResult<Value> {
    let empresa: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM empresa").fetch_one(&st.pool).await?;
    if empresa > 0 {
        return Err(AppError::msg("Este KanGest já está configurado: restaure em Configurações → Sistema."));
    }
    preparar_restauro(st, &r, false).await
}

/// Ao arrancar, antes de abrir a base: aplica um restauro marcado. Devolve a origem restaurada.
pub fn aplicar_restauro_pendente(pasta_dados: &Path) -> AppResult<Option<String>> {
    let marca = pasta_dados.join(MARCA_RESTAURO);
    let Ok(texto) = std::fs::read_to_string(&marca) else { return Ok(None) };
    let v: Value = serde_json::from_str(&texto).unwrap_or(Value::Null);
    let _ = std::fs::remove_file(&marca);
    let Some(pronto) = v["ficheiro"].as_str().map(PathBuf::from).filter(|p| p.is_file()) else { return Ok(None) };
    let base = pasta_dados.join("kigest.db");
    // o WAL e o índice da base antiga não podem ser aplicados à cópia restaurada
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(base.with_extension(ext));
    }
    // troca o ficheiro (rename) em vez de escrever por cima: uma ligação à base antiga que ainda esteja a
    // fechar escreve no ficheiro antigo, nunca na cópia restaurada
    let novo = base.with_extension("db-restaurar");
    std::fs::copy(&pronto, &novo).map_err(|e| AppError::msg(format!("Não foi possível restaurar a cópia: {e}")))?;
    std::fs::rename(&novo, &base).map_err(|e| AppError::msg(format!("Não foi possível restaurar a cópia: {e}")))?;
    let _ = std::fs::remove_file(&pronto);
    Ok(v["origem"].as_str().map(String::from))
}

#[derive(serde::Deserialize)]
pub struct ExportarIn {
    nome: String,
    conteudo: String,
}

/// Grava um ficheiro de texto (CSV, mapas) na pasta de exportação e devolve o caminho.
/// O nome é limpo de separadores para não sair da pasta.
pub async fn exportar(st: &AppState, e: ExportarIn) -> AppResult<Value> {
    st.sessao()?;
    let nome: String = e
        .nome
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if nome.is_empty() || !nome.contains('.') {
        return Err(AppError::msg("Nome de ficheiro inválido."));
    }
    let caminho = st.pasta_exportacao.join(&nome);
    // BOM UTF-8: o Excel abre os acentos correctamente
    let mut bytes = Vec::with_capacity(e.conteudo.len() + 3);
    if nome.to_lowercase().ends_with(".csv") {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(e.conteudo.as_bytes());
    std::fs::write(&caminho, bytes).map_err(|err| AppError::msg(format!("Não foi possível gravar {}: {err}", caminho.display())))?;
    Ok(json!({ "ficheiro": caminho.display().to_string() }))
}

/// Ecrã «Sobre»: versão, pasta de dados, tamanho da base e contagens principais.
pub async fn info(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let base = st.pasta_dados.join("kigest.db");
    let bytes = std::fs::metadata(&base).map(|m| m.len()).unwrap_or(0);
    let conta = |sql: &'static str| async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&st.pool).await };
    let sqlite: String = sqlx::query_scalar("SELECT sqlite_version()").fetch_one(&st.pool).await?;
    let migracoes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&st.pool).await.unwrap_or(0);
    let ultima_copia = listar_db(&pasta_copias(&st.pasta_dados), "kangest-").last().map(|x| x.1.file_name().unwrap_or_default().to_string_lossy().to_string());
    Ok(json!({
        "versao": env!("CARGO_PKG_VERSION"),
        "plataforma": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "sqlite": sqlite,
        "migracoes": migracoes,
        "pasta_dados": st.pasta_dados.display().to_string(),
        "pasta_exportacao": st.pasta_exportacao.display().to_string(),
        "base_bytes": bytes,
        "ultima_copia": ultima_copia,
        "artigos": conta("SELECT COUNT(*) FROM artigos").await?,
        "entidades": conta("SELECT COUNT(*) FROM entidades").await?,
        "documentos": conta("SELECT COUNT(*) FROM documentos").await?,
        "importados": conta("SELECT COUNT(*) FROM documentos WHERE importado = 1").await?,
        "utilizadores": conta("SELECT COUNT(*) FROM utilizadores").await?,
    }))
}

// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

use std::path::Path;

use serde_json::{Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};

use crate::error::AppResult;

/// O início de uma base SQLite em claro. Uma base cifrada (SQLCipher) começa por bytes aleatórios.
const CABECALHO: &[u8; 16] = b"SQLite format 3\0";

/// A base (ou cópia) está em claro: o ficheiro começa pelo cabeçalho do SQLite.
pub fn em_claro(f: &Path) -> bool {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open(f).and_then(|mut x| x.read_exact(&mut b)).is_ok() && &b == CABECALHO
}

/// O OpenSSL (o SQLCipher usa-o no Windows e no Linux; no macOS usa o CommonCrypto da Apple) inicializa-se sem a
/// limpeza à saída do programa (`OPENSSL_INIT_NO_ATEXIT`, que o `openssl_sys::init` pede): com ligações à base ainda a
/// fechar noutras threads, essa limpeza libertava memória que ainda estava em uso («double free or corruption» no fim
/// dos testes, de vez em quando). O sistema recupera essa memória de qualquer forma. Tem de vir antes do primeiro
/// uso do OpenSSL, por isso está aqui, por onde passam todas as ligações cifradas.
fn iniciar_cripto() {
    #[cfg(not(target_os = "macos"))]
    openssl_sys::init();
}

/// As opções de uma ligação à base cifrada com a chave (`PRAGMA key` vai sempre primeiro: o sqlx põe-no à cabeça).
pub fn com_chave(o: SqliteConnectOptions, chave: &str) -> SqliteConnectOptions {
    iniciar_cripto();
    o.pragma("key", format!("\"x'{chave}'\""))
}

/// A base abre com esta chave (lê o catálogo: com a chave errada o SQLCipher responde «file is not a database»).
async fn abre_com(f: &Path, chave: &str) -> bool {
    use sqlx::{ConnectOptions, Connection};
    let Ok(mut c) = com_chave(SqliteConnectOptions::new().filename(f).read_only(true), chave).connect().await else { return false };
    let ok = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master").fetch_one(&mut c).await.is_ok();
    c.close().await.ok();
    ok
}

/// As tabelas e o número de linhas de cada uma (para conferir uma base copiada).
async fn contagens(c: &mut sqlx::SqliteConnection, esquema: &str) -> AppResult<Vec<(String, i64)>> {
    let tabelas: Vec<String> =
        sqlx::query_scalar(&format!("SELECT name FROM {esquema}.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"))
            .fetch_all(&mut *c)
            .await?;
    let mut v = Vec::new();
    for t in tabelas {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {esquema}.\"{}\"", t.replace('"', "\"\""))).fetch_one(&mut *c).await?;
        v.push((t, n));
    }
    Ok(v)
}

/// Copia a base ligada em `c` (o esquema `main`) para `destino`, com a chave `chave_destino` (`None`: em claro), com
/// o `sqlcipher_export`. Confere que a cópia tem as mesmas tabelas e linhas.
async fn exportar(c: &mut sqlx::SqliteConnection, destino: &Path, chave_destino: Option<&str>) -> AppResult<()> {
    use crate::error::AppError;
    if destino.exists() {
        std::fs::remove_file(destino).map_err(|e| AppError::msg(format!("Não foi possível substituir {}: {e}", destino.display())))?;
    }
    let chave = chave_destino.map(|k| format!("x'{k}'")).unwrap_or_default();
    sqlx::query("ATTACH DATABASE ?1 AS destino KEY ?2").bind(destino.to_string_lossy().to_string()).bind(chave).execute(&mut *c).await?;
    let r = async {
        sqlx::query("BEGIN").execute(&mut *c).await?;
        sqlx::query("SELECT sqlcipher_export('destino')").execute(&mut *c).await?;
        let versao: i64 = sqlx::query_scalar("PRAGMA main.user_version").fetch_one(&mut *c).await?;
        sqlx::query(&format!("PRAGMA destino.user_version = {versao}")).execute(&mut *c).await?;
        let (a, b) = (contagens(c, "main").await?, contagens(c, "destino").await?);
        sqlx::query("COMMIT").execute(&mut *c).await?;
        if a != b {
            return Err(AppError::msg("A cópia da base não ficou igual ao original."));
        }
        Ok(())
    }
    .await;
    if r.is_err() {
        let _ = sqlx::query("ROLLBACK").execute(&mut *c).await;
    }
    let _ = sqlx::query("DETACH DATABASE destino").execute(&mut *c).await;
    if r.is_err() {
        let _ = std::fs::remove_file(destino);
    }
    r
}

/// Cópia da base **em claro** (portátil): a cópia exportada e as cópias externas (pen, OneDrive), que têm de se
/// poder restaurar noutro computador, onde a chave deste não existe. As cópias locais (`VACUUM INTO`) saem cifradas.
pub async fn exportar_em_claro(pool: &SqlitePool, destino: &Path) -> AppResult<()> {
    let mut c = pool.acquire().await?;
    exportar(&mut c, destino, None).await
}

/// Cifra uma base em claro no próprio sítio (versões anteriores, ou uma cópia em claro restaurada): exporta para um
/// ficheiro ao lado, confere-o, e troca-o pelo original (um corte a meio deixa o original intacto).
async fn cifrar_em_sitio(f: &Path, chave: &str) -> AppResult<()> {
    use crate::error::AppError;
    use sqlx::{ConnectOptions, Connection};
    let novo = f.with_extension("db-cifrando");
    {
        // create_if_missing: sem ele o ATTACH não pode criar o ficheiro novo
        let mut c = SqliteConnectOptions::new().filename(f).create_if_missing(true).connect().await?;
        // o que estiver no WAL entra na base antes de a copiar
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut c).await?;
        let r = exportar(&mut c, &novo, Some(chave)).await;
        c.close().await.ok();
        r?;
    }
    if !abre_com(&novo, chave).await {
        let _ = std::fs::remove_file(&novo);
        return Err(AppError::msg("A base cifrada não abriu com a chave: fica como estava."));
    }
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(f.with_extension(ext));
    }
    std::fs::rename(&novo, f).map_err(|e| AppError::msg(format!("Não foi possível trocar a base pela cifrada: {e}")))?;
    eprintln!("Base de dados cifrada: {}", f.display());
    Ok(())
}

/// Uma base cifrada com outra chave (a pasta de dados veio de outro computador ou de outro utilizador do sistema)
/// não abre aqui. Fica de parte, nunca apagada, e o programa arranca com uma base nova, onde se restaura uma cópia
/// externa ou exportada (que são portáteis). Devolve o nome com que ficou.
fn por_de_parte(f: &Path) -> AppResult<std::path::PathBuf> {
    use crate::error::AppError;
    let carimbo = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let destino = f.with_extension(format!("db.ilegivel-{carimbo}"));
    std::fs::rename(f, &destino).map_err(|e| AppError::msg(format!("A base não abre com a chave deste computador e não foi possível pô-la de parte: {e}")))?;
    for ext in ["db-wal", "db-shm"] {
        let w = f.with_extension(ext);
        if w.exists() {
            let _ = std::fs::rename(&w, destino.with_extension(format!("{}-{ext}", destino.extension().and_then(|x| x.to_str()).unwrap_or(""))));
        }
    }
    eprintln!("A base não abre com a chave deste computador: ficou em {}. Restaure uma cópia externa ou exportada.", destino.display());
    Ok(destino)
}

pub async fn abrir(caminho: &Path) -> AppResult<SqlitePool> {
    let chave = crate::segredos::chave_base()?;
    if caminho.is_file() && std::fs::metadata(caminho).map(|m| m.len()).unwrap_or(0) > 0 {
        if em_claro(caminho) {
            cifrar_em_sitio(caminho, &chave).await?;
        } else if !abre_com(caminho, &chave).await {
            por_de_parte(caminho)?;
        }
    }
    // filename() evita problemas com espaços no caminho (ex.: "Application Support").
    let opts = com_chave(SqliteConnectOptions::new(), &chave)
        .filename(caminho)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    // linhas de auditoria ainda sem selo (as de antes da 0037, na primeira vez): ver auditoria.rs
    if let Err(e) = crate::auditoria::selar(&pool).await {
        eprintln!("Auditoria: não foi possível selar o registo ao abrir ({e}).");
    }
    Ok(pool)
}

/// Transacção de escrita. Com `BEGIN IMMEDIATE` o bloqueio de escrita fica preso logo no início, à espera do
/// `busy_timeout` se outra ligação estiver a escrever. Com o `BEGIN` normal (diferido), uma transacção que já leu e
/// depois tenta escrever recebe logo «database is locked», sem esperar: acontecia com dois postos a gravar ao mesmo
/// tempo, ou numa gravação feita logo a seguir a outra recusada (que ainda se estava a desfazer noutra ligação).
pub async fn transacao(pool: &SqlitePool) -> AppResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
    Ok(pool.begin_with("BEGIN IMMEDIATE").await?)
}

/// Número da migração mais recente que esta versão conhece. Uma base com migrações mais novas é de uma versão
/// posterior e não abre aqui (o `migrate` recusa migrações aplicadas que não conhece).
pub fn ultima_migracao() -> i64 {
    sqlx::migrate!("./migrations").migrations.iter().map(|m| m.version).max().unwrap_or(0)
}

/// Converte uma linha SQLite num objecto JSON (para leituras genéricas).
pub fn row_json(row: &SqliteRow) -> Value {
    let mut obj = Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        let raw = match row.try_get_raw(i) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let v = if raw.is_null() {
            Value::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" | "BOOLEAN" => row.try_get::<i64, _>(i).map(Value::from).unwrap_or(Value::Null),
                "REAL" | "NUMERIC" => row.try_get::<f64, _>(i).map(Value::from).unwrap_or(Value::Null),
                _ => row.try_get::<String, _>(i).map(Value::from).unwrap_or(Value::Null),
            }
        };
        obj.insert(col.name().to_string(), v);
    }
    Value::Object(obj)
}

pub fn rows_json(rows: &[SqliteRow]) -> Value {
    Value::Array(rows.iter().map(row_json).collect())
}

pub async fn query_json(pool: &SqlitePool, sql: &str) -> AppResult<Value> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows_json(&rows))
}

pub fn agora() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn hoje() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn ontem() -> String {
    (chrono::Local::now().date_naive() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()
}

/// Consulta com um único parâmetro inteiro, devolvida em JSON.
pub async fn query_json_bind(pool: &SqlitePool, sql: &str, p: i64) -> AppResult<Value> {
    let rows = sqlx::query(sql).bind(p).fetch_all(pool).await?;
    Ok(rows_json(&rows))
}

#[cfg(test)]
mod testes_cifra {
    use super::*;
    use sqlx::{ConnectOptions, Connection};

    fn pasta(nome: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("kangest-cifra-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn contem(f: &Path, texto: &str) -> bool {
        std::fs::read(f).unwrap().windows(texto.len()).any(|w| w == texto.as_bytes())
    }

    #[tokio::test]
    async fn base_nova_sai_cifrada_e_so_abre_com_a_chave() {
        let d = pasta("nova");
        let f = d.join("kigest.db");
        let pool = abrir(&f).await.unwrap();
        sqlx::query("INSERT INTO config (chave, valor) VALUES ('teste', 'Cliente Exemplo Lda')").execute(&pool).await.unwrap();
        pool.close().await;
        assert!(!em_claro(&f), "sem o cabeçalho em claro");
        assert!(!contem(&f, "Cliente Exemplo Lda"), "os dados não se lêem no ficheiro");
        let k = crate::segredos::chave_base().unwrap();
        assert!(abre_com(&f, &k).await);
        assert!(!abre_com(&f, &"0".repeat(64)).await, "não abre com outra chave");
        let mut c = SqliteConnectOptions::new().filename(&f).read_only(true).connect().await.unwrap();
        assert!(sqlx::query("SELECT COUNT(*) FROM sqlite_master").fetch_one(&mut c).await.is_err(), "não abre sem chave");
        c.close().await.ok();
        // reabrir: os dados estão lá
        let pool = abrir(&f).await.unwrap();
        let v: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'teste'").fetch_one(&pool).await.unwrap();
        assert_eq!(v, "Cliente Exemplo Lda");
        pool.close().await;
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn base_antiga_em_claro_e_cifrada_ao_abrir_sem_perder_nada() {
        let d = pasta("antiga");
        let f = d.join("kigest.db");
        // uma base em claro, como a das versões anteriores (com WAL por aplicar)
        {
            let mut c = SqliteConnectOptions::new().filename(&f).create_if_missing(true).journal_mode(SqliteJournalMode::Wal).connect().await.unwrap();
            sqlx::query("CREATE TABLE clientes_antigos (nome TEXT)").execute(&mut c).await.unwrap();
            for i in 0..50 {
                sqlx::query("INSERT INTO clientes_antigos VALUES (?)").bind(format!("Mercearia Exemplo {i}")).execute(&mut c).await.unwrap();
            }
            sqlx::query("PRAGMA user_version = 7").execute(&mut c).await.unwrap();
            c.close().await.unwrap();
        }
        assert!(em_claro(&f));
        let pool = abrir(&f).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clientes_antigos").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 50);
        let uv: i64 = sqlx::query_scalar("PRAGMA user_version").fetch_one(&pool).await.unwrap();
        assert_eq!(uv, 7);
        pool.close().await;
        assert!(!em_claro(&f) && !contem(&f, "Mercearia Exemplo"));
        assert!(!f.with_extension("db-cifrando").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn base_de_outra_chave_fica_de_parte_e_arranca_uma_nova() {
        let d = pasta("outra");
        let f = d.join("kigest.db");
        {
            let mut c = com_chave(SqliteConnectOptions::new(), &"ab".repeat(32)).filename(&f).create_if_missing(true).connect().await.unwrap();
            sqlx::query("CREATE TABLE x (y TEXT)").execute(&mut c).await.unwrap();
            c.close().await.unwrap();
        }
        let pool = abrir(&f).await.unwrap();
        pool.close().await;
        let partes: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        let de_parte = partes.iter().find(|n| n.starts_with("kigest.db.ilegivel-")).expect("a base antiga ficou de parte");
        assert!(abre_com(&d.join(de_parte), &"ab".repeat(32)).await, "intacta: abre com a chave dela");
        assert!(abre_com(&f, &crate::segredos::chave_base().unwrap()).await, "a nova abre com a chave deste computador");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[tokio::test]
    async fn exportacao_em_claro_e_portatil_e_vacuum_into_fica_cifrado() {
        let d = pasta("copias");
        let f = d.join("kigest.db");
        let pool = abrir(&f).await.unwrap();
        sqlx::query("INSERT INTO config (chave, valor) VALUES ('teste', 'Loja Exemplo')").execute(&pool).await.unwrap();
        let e = d.join("exportada.db");
        exportar_em_claro(&pool, &e).await.unwrap();
        assert!(em_claro(&e), "portátil: abre noutro computador");
        let mut c = SqliteConnectOptions::new().filename(&e).read_only(true).connect().await.unwrap();
        let v: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'teste'").fetch_one(&mut c).await.unwrap();
        assert_eq!(v, "Loja Exemplo");
        c.close().await.ok();
        let local = d.join("local.db");
        sqlx::query("VACUUM INTO ?").bind(local.to_string_lossy().to_string()).execute(&pool).await.unwrap();
        assert!(!em_claro(&local) && abre_com(&local, &crate::segredos::chave_base().unwrap()).await, "a cópia local sai cifrada");
        pool.close().await;
        let _ = std::fs::remove_dir_all(&d);
    }
}

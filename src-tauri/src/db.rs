// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

use std::path::Path;

use serde_json::{Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};

use crate::error::AppResult;

pub async fn abrir(caminho: &Path) -> AppResult<SqlitePool> {
    // filename() evita problemas com espaços no caminho (ex.: "Application Support").
    let opts = SqliteConnectOptions::new()
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
mod ensaio_sqlcipher {
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::{ConnectOptions, Connection};

    async fn abrir(f: &std::path::Path, chave: Option<&str>) -> Result<sqlx::SqliteConnection, sqlx::Error> {
        let mut o = SqliteConnectOptions::new().filename(f).create_if_missing(true);
        if let Some(k) = chave {
            o = o.pragma("key", format!("\"x'{k}'\""));
        }
        let mut c = o.connect().await?;
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sqlite_master").fetch_one(&mut c).await?;
        Ok(c)
    }

    #[tokio::test]
    async fn base_cifrada_abre_so_com_a_chave_certa() {
        let d = std::env::temp_dir().join(format!("kangest-sqlcipher-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("ensaio.db");
        let k = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let versao: String = {
            let mut c = abrir(&f, Some(k)).await.unwrap();
            let v: String = sqlx::query_scalar("PRAGMA cipher_version").fetch_one(&mut c).await.unwrap();
            sqlx::query("CREATE TABLE t (x TEXT)").execute(&mut c).await.unwrap();
            sqlx::query("INSERT INTO t VALUES ('Cliente Exemplo')").execute(&mut c).await.unwrap();
            c.close().await.unwrap();
            v
        };
        assert!(!versao.is_empty(), "é o SQLCipher");
        let bytes = std::fs::read(&f).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"), "o ficheiro não começa pelo cabeçalho em claro");
        assert!(!bytes.windows(15).any(|w| w == b"Cliente Exemplo"), "os dados não se lêem no ficheiro");
        let mut c = abrir(&f, Some(k)).await.expect("abre com a chave certa");
        let x: String = sqlx::query_scalar("SELECT x FROM t").fetch_one(&mut c).await.unwrap();
        assert_eq!(x, "Cliente Exemplo");
        c.close().await.unwrap();
        let errada = "ff112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        assert!(abrir(&f, Some(errada)).await.is_err(), "não abre com outra chave");
        assert!(abrir(&f, None).await.is_err(), "não abre sem chave");
        let _ = std::fs::remove_dir_all(&d);
        eprintln!("SQLCipher {versao}");
    }
}


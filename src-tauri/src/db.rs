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
    Ok(pool)
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

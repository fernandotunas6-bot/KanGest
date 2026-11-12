//! Documentos suspensos do POS (Fr_GravaSuspenso / Fr_Suspenso do KIGEST).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::catalogo::IdIn;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct GuardarIn {
    entidade_id: Option<i64>,
    entidade_nome: Option<String>,
    descricao: Option<String>,
    total: i64,
    /// Carrinho (linhas, descontos…) tal como o POS o mostra.
    dados: Value,
}

pub async fn guardar(st: &AppState, i: GuardarIn) -> AppResult<Value> {
    let s = st.exigir_perm("pos.suspenso")?;
    if i.dados.get("linhas").and_then(|l| l.as_array()).map(|l| l.is_empty()).unwrap_or(true) {
        return Err(AppError::msg("Não há linhas para suspender."));
    }
    let id = sqlx::query(
        "INSERT INTO vendas_suspensas (posto_id, utilizador_id, entidade_id, entidade_nome, descricao, total, dados) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(st.posto().id)
    .bind(s.id)
    .bind(i.entidade_id)
    .bind(&i.entidade_nome)
    .bind(i.descricao.as_deref().map(str::trim).filter(|d| !d.is_empty()))
    .bind(i.total)
    .bind(i.dados.to_string())
    .execute(&st.pool)
    .await?
    .last_insert_rowid();
    Ok(json!({ "id": id }))
}

pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT v.id, v.posto_id, v.entidade_nome, v.descricao, v.total, v.criado_em, u.nome AS utilizador
         FROM vendas_suspensas v JOIN utilizadores u ON u.id = v.utilizador_id
         WHERE v.posto_id = ? ORDER BY v.id DESC",
    )
    .bind(st.posto().id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

/// Abre (e retira da lista) um documento suspenso.
pub async fn abrir(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("pos.suspenso")?;
    let mut tx = st.pool.begin().await?;
    let row = sqlx::query("SELECT * FROM vendas_suspensas WHERE id = ?")
        .bind(i.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::msg("Documento suspenso não encontrado (pode já ter sido aberto noutro posto)."))?;
    sqlx::query("DELETE FROM vendas_suspensas WHERE id = ?").bind(i.id).execute(&mut *tx).await?;
    tx.commit().await?;
    let dados: String = row.get("dados");
    Ok(json!({
        "id": i.id,
        "entidade_id": row.get::<Option<i64>, _>("entidade_id"),
        "entidade_nome": row.get::<Option<String>, _>("entidade_nome"),
        "descricao": row.get::<Option<String>, _>("descricao"),
        "total": row.get::<i64, _>("total"),
        "dados": serde_json::from_str::<Value>(&dados)?,
    }))
}

pub async fn eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("pos.suspenso")?;
    sqlx::query("DELETE FROM vendas_suspensas WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(Value::Null)
}

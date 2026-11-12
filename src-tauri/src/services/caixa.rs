use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::catalogo::IdIn;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn sessao_aberta(st: &AppState) -> AppResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT id FROM caixa_sessoes WHERE posto_id = ? AND estado = 'ABERTA' ORDER BY id DESC LIMIT 1")
        .bind(st.posto().id)
        .fetch_optional(&st.pool)
        .await?)
}

async fn resumo(st: &AppState, sid: i64) -> AppResult<Value> {
    let p = &st.pool;
    let s = sqlx::query(
        "SELECT c.*, u.nome AS utilizador, ps.descricao AS posto FROM caixa_sessoes c
         JOIN utilizadores u ON u.id = c.utilizador_id LEFT JOIN postos ps ON ps.id = c.posto_id WHERE c.id = ?",
    )
    .bind(sid)
    .fetch_optional(p)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let por_modo = sqlx::query(
        "SELECT m.codigo, m.descricao, COALESCE(SUM(dp.valor), 0) AS total
         FROM modos_pagamento m
         LEFT JOIN documento_pagamentos dp ON dp.modo_codigo = m.codigo
           AND dp.documento_id IN (SELECT id FROM documentos WHERE caixa_sessao_id = ?1 AND estado = 'N')
         GROUP BY m.codigo ORDER BY m.ordem",
    )
    .bind(sid)
    .fetch_all(p)
    .await?;
    let (docs, vendido): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(total), 0) FROM documentos WHERE caixa_sessao_id = ? AND estado = 'N' AND tipo_codigo IN ('FR','RC')",
    )
    .bind(sid)
    .fetch_one(p)
    .await?;
    let (entradas, saidas): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN tipo='ENTRADA' THEN valor END), 0), COALESCE(SUM(CASE WHEN tipo='SAIDA' THEN valor END), 0)
         FROM caixa_movimentos WHERE sessao_id = ? AND COALESCE(modo_codigo, 'NU') = 'NU'",
    )
    .bind(sid)
    .fetch_one(p)
    .await?;
    let movimentos = sqlx::query("SELECT * FROM caixa_movimentos WHERE sessao_id = ? ORDER BY id").bind(sid).fetch_all(p).await?;
    // Artigos vendidos na sessão (o talão de fecho do KIGEST lista-os)
    let artigos = sqlx::query(
        "SELECT l.referencia, l.descricao, SUM(l.quantidade) AS quantidade, SUM(l.total) AS total
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE d.caixa_sessao_id = ? AND d.estado = 'N' AND d.tipo_codigo = 'FR'
         GROUP BY l.referencia, l.descricao ORDER BY total DESC",
    )
    .bind(sid)
    .fetch_all(p)
    .await?;
    let numerario: i64 = por_modo.iter().filter(|r| r.get::<String, _>("codigo") == "NU").map(|r| r.get::<i64, _>("total")).sum();
    let inicial: i64 = s.get("valor_inicial");
    Ok(json!({
        "sessao": db::row_json(&s),
        "por_modo": db::rows_json(&por_modo),
        "documentos": docs,
        "total_vendido": vendido,
        "entradas": entradas,
        "saidas": saidas,
        "movimentos": db::rows_json(&movimentos),
        "artigos": db::rows_json(&artigos),
        "numerario_esperado": inicial + numerario + entradas - saidas,
    }))
}

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    match sessao_aberta(st).await? {
        Some(id) => resumo(st, id).await,
        None => Ok(Value::Null),
    }
}

/// Resumo de qualquer sessão (reimpressão de fechos anteriores).
pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    resumo(st, i.id).await
}

/// Fechos de caixa (menu "Fechos de Caixa (POS)" do GC).
pub async fn sessoes(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT c.id, c.posto_id, ps.descricao AS posto, u.nome AS utilizador, c.aberta_em, c.fechada_em, c.valor_inicial,
                c.valor_contado, c.estado,
                (SELECT COALESCE(SUM(total), 0) FROM documentos d WHERE d.caixa_sessao_id = c.id AND d.estado = 'N' AND d.tipo_codigo IN ('FR','RC')) AS total_vendido
         FROM caixa_sessoes c JOIN utilizadores u ON u.id = c.utilizador_id LEFT JOIN postos ps ON ps.id = c.posto_id
         ORDER BY c.aberta_em DESC, c.id DESC LIMIT 200",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct AbrirIn {
    valor_inicial: i64,
}

pub async fn abrir(st: &AppState, i: AbrirIn) -> AppResult<Value> {
    let s = st.sessao()?;
    if sessao_aberta(st).await?.is_some() {
        return Err(AppError::msg("Já existe um caixa aberto neste posto."));
    }
    let id = sqlx::query("INSERT INTO caixa_sessoes (posto_id, utilizador_id, aberta_em, valor_inicial) VALUES (?, ?, ?, ?)")
        .bind(st.posto().id)
        .bind(s.id)
        .bind(db::agora())
        .bind(i.valor_inicial.max(0))
        .execute(&st.pool)
        .await?
        .last_insert_rowid();
    resumo(st, id).await
}

#[derive(Deserialize)]
pub struct MovIn {
    tipo: String,
    valor: i64,
    descricao: String,
    rubrica: Option<String>,
    modo: Option<String>,
}

pub async fn movimento(st: &AppState, m: MovIn) -> AppResult<Value> {
    st.exigir_perm("caixa.movimentos")?;
    let sid = sessao_aberta(st).await?.ok_or_else(|| AppError::msg("Não há caixa aberto."))?;
    if m.tipo != "ENTRADA" && m.tipo != "SAIDA" {
        return Err(AppError::msg("Tipo de movimento inválido."));
    }
    if m.valor <= 0 || m.descricao.trim().is_empty() {
        return Err(AppError::msg("Indique o valor e a descrição."));
    }
    sqlx::query("INSERT INTO caixa_movimentos (sessao_id, data_hora, tipo, valor, descricao, rubrica, modo_codigo) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(sid)
        .bind(db::agora())
        .bind(&m.tipo)
        .bind(m.valor)
        .bind(m.descricao.trim())
        .bind(m.rubrica.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(m.modo.as_deref().unwrap_or("NU"))
        .execute(&st.pool)
        .await?;
    resumo(st, sid).await
}

#[derive(Deserialize)]
pub struct FecharIn {
    valor_contado: i64,
}

pub async fn fechar(st: &AppState, f: FecharIn) -> AppResult<Value> {
    st.exigir_perm("pos.fecho_caixa")?;
    let sid = sessao_aberta(st).await?.ok_or_else(|| AppError::msg("Não há caixa aberto."))?;
    sqlx::query("UPDATE caixa_sessoes SET estado = 'FECHADA', fechada_em = ?, valor_contado = ? WHERE id = ?")
        .bind(db::agora())
        .bind(f.valor_contado)
        .bind(sid)
        .execute(&st.pool)
        .await?;
    resumo(st, sid).await
}

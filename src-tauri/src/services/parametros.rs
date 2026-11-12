//! Parametrização de documentos (Fr_Parametrizacao), Opções de Venda (Fr_ConfigContas: forma de
//! pagamento → conta) e Definição de Faturas e Recibos (opções do impresso).
//!
//! Só se alteram textos e opções de apresentação; o comportamento fiscal de cada tipo
//! (stock, conta corrente, assinatura AGT, SAF-T) é fixo.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const OPCOES: &[(&str, &str)] = &[
    ("mostrar_bancos", "doc.mostrar_bancos"),
    ("valor_extenso", "doc.valor_extenso"),
    ("assinaturas", "doc.assinaturas"),
    ("mostrar_operador", "doc.mostrar_operador"),
];

/// Opções do impresso (A4/A5) para juntar ao documento.
pub async fn opcoes(p: &SqlitePool) -> AppResult<Value> {
    let mut o = serde_json::Map::new();
    for (k, chave) in OPCOES {
        let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(p).await?;
        o.insert((*k).into(), json!(v.as_deref() == Some("1")));
    }
    Ok(Value::Object(o))
}

pub async fn obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.parametrizacao")?;
    let p = &st.pool;
    Ok(json!({
        "tipos": db::query_json(p, "SELECT * FROM tipos_documento ORDER BY CASE grupo WHEN 'VENDAS' THEN 0 WHEN 'COMPRAS' THEN 1 ELSE 2 END, ordem, codigo").await?,
        "modos": db::query_json(p, "SELECT m.*, c.descricao AS conta FROM modos_pagamento m LEFT JOIN contas c ON c.id = m.conta_id WHERE m.codigo <> 'CS' ORDER BY m.ordem").await?,
        "contas": db::query_json(p, "SELECT id, descricao, tipo FROM contas WHERE ativo = 1 ORDER BY descricao").await?,
        "opcoes": opcoes(p).await?,
    }))
}

#[derive(Deserialize)]
pub struct TipoIn {
    codigo: String,
    descricao: String,
    vias: i64,
    #[serde(default)]
    validade_dias: i64,
    #[serde(default = "verdadeiro")]
    repetir_linhas: bool,
    #[serde(default)]
    prefixo_extenso: Option<String>,
    #[serde(default)]
    texto_rodape: Option<String>,
    #[serde(default = "a4")]
    formato: String,
}
fn verdadeiro() -> bool {
    true
}
fn a4() -> String {
    "A4".into()
}

fn limpo(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(String::from)
}

pub async fn tipo_guardar(st: &AppState, t: TipoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.parametrizacao")?;
    let desc = t.descricao.trim();
    if desc.is_empty() {
        return Err(AppError::msg("A descrição do documento é obrigatória."));
    }
    if !(1..=4).contains(&t.vias) {
        return Err(AppError::msg("O número de vias tem de estar entre 1 e 4."));
    }
    if !(0..=3650).contains(&t.validade_dias) {
        return Err(AppError::msg("A validade tem de estar entre 0 e 3650 dias."));
    }
    if t.formato != "A4" && t.formato != "A5" {
        return Err(AppError::msg("Formato inválido (A4 ou A5)."));
    }
    let n = sqlx::query(
        "UPDATE tipos_documento SET descricao = ?, vias = ?, validade_dias = ?, repetir_linhas = ?, prefixo_extenso = ?, texto_rodape = ?, formato = ? WHERE codigo = ?",
    )
    .bind(desc)
    .bind(t.vias)
    .bind(t.validade_dias)
    .bind(t.repetir_linhas as i64)
    .bind(limpo(&t.prefixo_extenso))
    .bind(limpo(&t.texto_rodape))
    .bind(&t.formato)
    .bind(&t.codigo)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NaoEncontrado);
    }
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct ModoIn {
    codigo: String,
    conta_id: i64,
    #[serde(default = "verdadeiro")]
    ativo: bool,
}

/// Opções de Venda: em que conta da tesouraria entra cada forma de pagamento.
pub async fn modo_guardar(st: &AppState, m: ModoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.parametrizacao")?;
    if m.codigo == "CS" {
        return Err(AppError::msg("A compensação de saldo não movimenta dinheiro."));
    }
    let conta: Option<i64> = sqlx::query_scalar("SELECT id FROM contas WHERE id = ? AND ativo = 1").bind(m.conta_id).fetch_optional(&st.pool).await?;
    if conta.is_none() {
        return Err(AppError::msg("Conta inválida."));
    }
    if !m.ativo && m.codigo == "NU" {
        return Err(AppError::msg("O numerário não pode ser desactivado."));
    }
    let n = sqlx::query("UPDATE modos_pagamento SET conta_id = ?, ativo = ? WHERE codigo = ?")
        .bind(m.conta_id)
        .bind(m.ativo as i64)
        .bind(&m.codigo)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NaoEncontrado);
    }
    Ok(Value::Null)
}

/// Definição de Faturas e Recibos: o que aparece no impresso.
pub async fn opcoes_guardar(st: &AppState, v: Value) -> AppResult<Value> {
    st.exigir_perm("sistema.parametrizacao")?;
    let mut tx = st.pool.begin().await?;
    for (k, chave) in OPCOES {
        if let Some(b) = v.get(*k).and_then(|x| x.as_bool()) {
            sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
                .bind(chave)
                .bind(if b { "1" } else { "0" })
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(Value::Null)
}

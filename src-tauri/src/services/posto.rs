//! Configuração de postos (Fr_ConfigPosto / Fr_Impressao do KIGEST POS).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query("SELECT p.*, a.nome AS armazem FROM postos p LEFT JOIN armazens a ON a.id = p.armazem_id ORDER BY p.id")
        .fetch_all(&st.pool)
        .await?;
    let postos: Vec<Value> = rows
        .iter()
        .map(|r| {
            let opcoes: Option<String> = r.get("opcoes");
            json!({
                "id": r.get::<i64, _>("id"),
                "descricao": r.get::<String, _>("descricao"),
                "armazem_id": r.get::<i64, _>("armazem_id"),
                "armazem": r.get::<Option<String>, _>("armazem"),
                "serie_prefixo": r.get::<String, _>("serie_prefixo"),
                "opcoes": opcoes.and_then(|o| serde_json::from_str::<Value>(&o).ok()).unwrap_or(Value::Null),
            })
        })
        .collect();
    Ok(json!({ "atual": st.posto(), "postos": postos }))
}

#[derive(Deserialize)]
pub struct PostoIn {
    id: Option<i64>,
    descricao: String,
    armazem_id: i64,
    serie_prefixo: Option<String>,
    opcoes: Option<Value>,
}

pub async fn guardar(st: &AppState, p: PostoIn) -> AppResult<Value> {
    st.exigir_perm("pos.config_posto")?;
    let desc = p.descricao.trim();
    if desc.is_empty() {
        return Err(AppError::msg("Indique a descrição do posto."));
    }
    let prefixo = p.serie_prefixo.unwrap_or_default().trim().to_uppercase();
    if prefixo.len() > 8 || !prefixo.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::msg("O prefixo de série só pode ter letras e números (máx. 8)."));
    }
    let armazem: Option<i64> = sqlx::query_scalar("SELECT id FROM armazens WHERE id = ? AND ativo = 1").bind(p.armazem_id).fetch_optional(&st.pool).await?;
    if armazem.is_none() {
        return Err(AppError::msg("Armazém inválido."));
    }
    let opcoes = p.opcoes.map(|o| o.to_string());
    let id = match p.id {
        Some(id) => {
            sqlx::query("UPDATE postos SET descricao = ?, armazem_id = ?, serie_prefixo = ?, opcoes = COALESCE(?, opcoes) WHERE id = ?")
                .bind(desc)
                .bind(p.armazem_id)
                .bind(&prefixo)
                .bind(&opcoes)
                .bind(id)
                .execute(&st.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO postos (descricao, armazem_id, serie_prefixo, opcoes) VALUES (?, ?, ?, COALESCE(?, (SELECT opcoes FROM postos WHERE id = 1)))")
            .bind(desc)
            .bind(p.armazem_id)
            .bind(&prefixo)
            .bind(&opcoes)
            .execute(&st.pool)
            .await?
            .last_insert_rowid(),
    };
    if id == st.posto().id {
        st.carregar_posto(id).await?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct SelecionarIn {
    id: i64,
}

/// Define qual posto é esta máquina (gravado em `posto.json` na pasta de dados).
pub async fn selecionar(st: &AppState, s: SelecionarIn) -> AppResult<Value> {
    st.exigir_perm("pos.config_posto")?;
    let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM postos WHERE id = ?").bind(s.id).fetch_optional(&st.pool).await?;
    if existe.is_none() {
        return Err(AppError::NaoEncontrado);
    }
    let p = st.carregar_posto(s.id).await?;
    let ficheiro = st.pasta_dados.join("posto.json");
    std::fs::write(&ficheiro, json!({ "posto_id": s.id }).to_string())
        .map_err(|e| AppError::msg(format!("Não foi possível gravar {}: {e}", ficheiro.display())))?;
    Ok(serde_json::to_value(p)?)
}

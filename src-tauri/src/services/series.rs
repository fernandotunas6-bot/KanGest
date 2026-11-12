//! Séries de documentos (Fr_SeriesDocumento / Fr_AdicionarSerie do KIGEST): série por defeito,
//! período de validade, séries de documentos manuais, bloqueio e série por posto.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct FiltroIn {
    tipo: Option<String>,
}

pub async fn listar(st: &AppState, f: FiltroIn) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT s.*, t.descricao AS tipo_desc, t.grupo, p.descricao AS posto,
                (SELECT COUNT(*) FROM documentos d WHERE d.tipo_codigo = s.tipo_codigo AND d.serie = s.serie) AS documentos
         FROM series s JOIN tipos_documento t ON t.codigo = s.tipo_codigo LEFT JOIN postos p ON p.id = s.posto_id
         WHERE (?1 IS NULL OR s.tipo_codigo = ?1)
         ORDER BY t.ordem, s.tipo_codigo, s.manual, s.serie DESC",
    )
    .bind(f.tipo.filter(|t| !t.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct SerieIn {
    id: Option<i64>,
    tipo_codigo: String,
    serie: String,
    descricao: Option<String>,
    data_inicio: Option<String>,
    data_fim: Option<String>,
    #[serde(default)]
    padrao: bool,
    #[serde(default)]
    manual: bool,
    posto_id: Option<i64>,
    ativa: Option<bool>,
}

fn data_ok(d: &Option<String>) -> AppResult<Option<String>> {
    match d.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(None),
        Some(x) if chrono::NaiveDate::parse_from_str(x, "%Y-%m-%d").is_ok() => Ok(Some(x.to_string())),
        Some(x) => Err(AppError::msg(format!("Data inválida: {x}"))),
    }
}

pub async fn guardar(st: &AppState, s: SerieIn) -> AppResult<Value> {
    st.exigir_perm("sistema.series")?;
    let codigo = s.serie.trim().to_uppercase();
    if codigo.is_empty() || codigo.len() > 20 || !codigo.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::msg("A série só pode ter letras, números e hífen (até 20 caracteres), sem espaços nem «/»."));
    }
    let (ini, fim) = (data_ok(&s.data_inicio)?, data_ok(&s.data_fim)?);
    if let (Some(a), Some(b)) = (&ini, &fim) {
        if a > b {
            return Err(AppError::msg("A data inicial é posterior à data final."));
        }
    }
    if s.manual && s.padrao {
        return Err(AppError::msg("Uma série de documentos manuais não pode ser a série por defeito."));
    }
    let tipo: Option<i64> = sqlx::query_scalar("SELECT assinado FROM tipos_documento WHERE codigo = ?").bind(&s.tipo_codigo).fetch_optional(&st.pool).await?;
    if tipo.is_none() {
        return Err(AppError::msg("Tipo de documento inválido."));
    }
    let descricao = s.descricao.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| format!("Série {codigo}"));
    let mut tx = st.pool.begin().await?;
    let id = match s.id {
        Some(id) => {
            let atual = sqlx::query("SELECT tipo_codigo, serie, ultimo_numero FROM series WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AppError::NaoEncontrado)?;
            let com_docs = atual.get::<i64, _>("ultimo_numero") > 0;
            if com_docs && (atual.get::<String, _>("serie") != codigo || atual.get::<String, _>("tipo_codigo") != s.tipo_codigo) {
                return Err(AppError::msg("A série já tem documentos: não pode mudar o código nem o tipo de documento."));
            }
            sqlx::query(
                "UPDATE series SET tipo_codigo = ?, serie = ?, descricao = ?, data_inicio = ?, data_fim = ?, padrao = ?, manual = ?, posto_id = ?, ativa = ? WHERE id = ?",
            )
            .bind(&s.tipo_codigo)
            .bind(&codigo)
            .bind(&descricao)
            .bind(&ini)
            .bind(&fim)
            .bind(s.padrao as i64)
            .bind(s.manual as i64)
            .bind(s.posto_id)
            .bind(s.ativa.unwrap_or(true) as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        }
        None => {
            let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM series WHERE tipo_codigo = ? AND serie = ?").bind(&s.tipo_codigo).bind(&codigo).fetch_optional(&mut *tx).await?;
            if existe.is_some() {
                return Err(AppError::msg(format!("A série {codigo} já existe para {}.", s.tipo_codigo)));
            }
            sqlx::query(
                "INSERT INTO series (tipo_codigo, serie, descricao, data_inicio, data_fim, padrao, manual, posto_id, ativa) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&s.tipo_codigo)
            .bind(&codigo)
            .bind(&descricao)
            .bind(&ini)
            .bind(&fim)
            .bind(s.padrao as i64)
            .bind(s.manual as i64)
            .bind(s.posto_id)
            .bind(s.ativa.unwrap_or(true) as i64)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid()
        }
    };
    if s.padrao {
        // uma só série por defeito por tipo de documento (e posto)
        sqlx::query("UPDATE series SET padrao = 0 WHERE tipo_codigo = ? AND id <> ? AND ((posto_id IS NULL AND ? IS NULL) OR posto_id = ?)")
            .bind(&s.tipo_codigo)
            .bind(id)
            .bind(s.posto_id)
            .bind(s.posto_id)
            .execute(&mut *tx)
            .await?;
    }
    let u = st.sessao()?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'SERIE', ?)")
        .bind(u.id)
        .bind(format!("{} {codigo}{}{}", s.tipo_codigo, if s.padrao { " (padrão)" } else { "" }, if s.manual { " (manual)" } else { "" }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "id": id, "serie": codigo }))
}

#[derive(Deserialize)]
pub struct EstadoIn {
    id: i64,
    ativa: Option<bool>,
    padrao: Option<bool>,
}

/// Bloquear / desbloquear uma série, ou defini-la como série por defeito.
pub async fn estado(st: &AppState, e: EstadoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.series")?;
    let row = sqlx::query("SELECT tipo_codigo, posto_id, manual FROM series WHERE id = ?").bind(e.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let mut tx = st.pool.begin().await?;
    if let Some(a) = e.ativa {
        sqlx::query("UPDATE series SET ativa = ? WHERE id = ?").bind(a as i64).bind(e.id).execute(&mut *tx).await?;
    }
    if e.padrao == Some(true) {
        if row.get::<i64, _>("manual") == 1 {
            return Err(AppError::msg("Uma série de documentos manuais não pode ser a série por defeito."));
        }
        let posto: Option<i64> = row.get("posto_id");
        sqlx::query("UPDATE series SET padrao = 0 WHERE tipo_codigo = ? AND ((posto_id IS NULL AND ? IS NULL) OR posto_id = ?)")
            .bind(row.get::<String, _>("tipo_codigo"))
            .bind(posto)
            .bind(posto)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE series SET padrao = 1, ativa = 1 WHERE id = ?").bind(e.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

/// Só se eliminam séries sem documentos.
pub async fn eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("sistema.series")?;
    let n = sqlx::query("DELETE FROM series WHERE id = ? AND ultimo_numero = 0 AND NOT EXISTS (SELECT 1 FROM documentos d WHERE d.tipo_codigo = series.tipo_codigo AND d.serie = series.serie)")
        .bind(i.id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::msg("A série já tem documentos: bloqueie-a em vez de a eliminar."));
    }
    Ok(Value::Null)
}

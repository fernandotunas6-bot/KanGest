use serde::Deserialize;
use serde_json::{json, Value};

use super::catalogo::IdIn;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct Filtro {
    tipo: Option<String>, // C | F
    q: Option<String>,
}

pub async fn listar(st: &AppState, f: Filtro) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT e.*,
            (SELECT COALESCE(SUM(d.valor_pendente), 0) - COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d
              WHERE d.entidade_id = e.id AND d.estado = 'N') AS saldo,
            (SELECT COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d
              WHERE d.entidade_id = e.id AND d.estado = 'N') AS credito
         FROM entidades e
         WHERE e.ativo = 1
           AND (?1 IS NULL OR e.tipo = ?1 OR e.tipo = 'A')
           AND (e.nome LIKE ?2 OR e.nif LIKE ?2 OR IFNULL(e.telefone,'') LIKE ?2)
         ORDER BY e.id = 1 DESC, e.nome LIMIT 500",
    )
    .bind(f.tipo)
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let row = sqlx::query("SELECT * FROM entidades WHERE id = ?")
        .bind(i.id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    Ok(db::row_json(&row))
}

#[derive(Deserialize)]
pub struct EntidadeIn {
    id: Option<i64>,
    tipo: Option<String>,
    nome: String,
    nif: Option<String>,
    endereco: Option<String>,
    localidade: Option<String>,
    provincia: Option<String>,
    /// País (código ISO de 2 letras; AO por omissão).
    pais: Option<String>,
    /// Entidade pública (SAF-T «Faturação para o Estado»).
    estado: Option<bool>,
    observacoes: Option<String>,
    telefone: Option<String>,
    email: Option<String>,
    limite_credito: Option<i64>,
    desconto: Option<f64>,
    nivel_preco: Option<i64>,
}

pub async fn guardar(st: &AppState, e: EntidadeIn) -> AppResult<Value> {
    st.exigir_perm(if e.tipo.as_deref() == Some("F") { "fornecedores" } else { "clientes" })?;
    if e.nome.trim().is_empty() {
        return Err(AppError::msg("O nome é obrigatório."));
    }
    if e.id == Some(1) {
        return Err(AppError::msg("O cliente \"Consumidor Final\" não pode ser alterado."));
    }
    let nif = e.nif.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "999999999".into());
    if nif != "999999999" {
        let dup: Option<String> = sqlx::query_scalar("SELECT nome FROM entidades WHERE nif = ? AND id <> ? AND ativo = 1")
            .bind(&nif)
            .bind(e.id.unwrap_or(0))
            .fetch_optional(&st.pool)
            .await?;
        if let Some(n) = dup {
            return Err(AppError::msg(format!("O NIF {nif} já está registado para \"{n}\".")));
        }
    }
    let tipo = e.tipo.unwrap_or_else(|| "C".into());
    let pais = e.pais.map(|p| p.trim().to_uppercase()).filter(|p| !p.is_empty()).unwrap_or_else(|| "AO".into());
    if pais.len() != 2 || !pais.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::msg("O país indica-se pelo código de 2 letras (ex.: AO, PT, ZA)."));
    }
    let sql = if e.id.is_some() {
        "UPDATE entidades SET tipo=?1, nome=?2, nif=?3, endereco=?4, localidade=?5, provincia=?6, telefone=?7,
           email=?8, limite_credito=?9, desconto=?10, nivel_preco=?11, pais=?12, estado=?13, observacoes=?14 WHERE id = ?15"
    } else {
        "INSERT INTO entidades (tipo, nome, nif, endereco, localidade, provincia, telefone, email, limite_credito, desconto, nivel_preco, pais, estado, observacoes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
    };
    let mut q = sqlx::query(sql)
        .bind(tipo)
        .bind(e.nome.trim())
        .bind(nif)
        .bind(e.endereco)
        .bind(e.localidade)
        .bind(e.provincia)
        .bind(e.telefone)
        .bind(e.email)
        .bind(e.limite_credito.unwrap_or(0))
        .bind(e.desconto.unwrap_or(0.0))
        .bind(e.nivel_preco.unwrap_or(1).clamp(1, 4))
        .bind(pais)
        .bind(e.estado.unwrap_or(false) as i64)
        .bind(e.observacoes.as_deref().map(str::trim).filter(|x| !x.is_empty()));
    if let Some(id) = e.id {
        q = q.bind(id);
    }
    let res = q.execute(&st.pool).await?;
    Ok(json!({ "id": e.id.unwrap_or_else(|| res.last_insert_rowid()) }))
}

/// Documentos em dívida de uma entidade (para emitir recibo).
pub async fn pendentes(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT id, numero_doc, tipo_codigo, data_emissao, data_vencimento, total, valor_pendente
         FROM documentos WHERE entidade_id = ? AND estado = 'N' AND valor_pendente > 0
         ORDER BY data_emissao, id",
    )
    .bind(i.id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

// ------------------------------------------------------------------ endereços de entrega (Fr_EnderecosExtras)
pub async fn enderecos(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    db::query_json_bind(&st.pool, "SELECT * FROM entidade_enderecos WHERE entidade_id = ? AND ativo = 1 ORDER BY descricao", i.id).await
}

#[derive(Deserialize)]
pub struct EnderecoIn {
    id: Option<i64>,
    entidade_id: i64,
    descricao: String,
    endereco: String,
    localidade: Option<String>,
    provincia: Option<String>,
    pais: Option<String>,
    ativo: Option<bool>,
}

pub async fn endereco_guardar(st: &AppState, e: EnderecoIn) -> AppResult<Value> {
    st.sessao()?;
    if e.descricao.trim().is_empty() || e.endereco.trim().is_empty() {
        return Err(AppError::msg("Indique a descrição e o endereço."));
    }
    let pais = e.pais.map(|p| p.trim().to_uppercase()).filter(|p| p.len() == 2).unwrap_or_else(|| "AO".into());
    let id = match e.id {
        Some(id) => {
            sqlx::query("UPDATE entidade_enderecos SET descricao = ?, endereco = ?, localidade = ?, provincia = ?, pais = ?, ativo = ? WHERE id = ? AND entidade_id = ?")
                .bind(e.descricao.trim())
                .bind(e.endereco.trim())
                .bind(e.localidade)
                .bind(e.provincia)
                .bind(&pais)
                .bind(e.ativo.unwrap_or(true) as i64)
                .bind(id)
                .bind(e.entidade_id)
                .execute(&st.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO entidade_enderecos (entidade_id, descricao, endereco, localidade, provincia, pais) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(e.entidade_id)
            .bind(e.descricao.trim())
            .bind(e.endereco.trim())
            .bind(e.localidade)
            .bind(e.provincia)
            .bind(&pais)
            .execute(&st.pool)
            .await?
            .last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

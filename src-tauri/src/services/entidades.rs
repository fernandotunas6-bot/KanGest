// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

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
    /// também as desactivadas
    #[serde(default)]
    inativos: Option<bool>,
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
         WHERE (?3 = 1 OR e.ativo = 1)
           AND (?1 IS NULL OR e.tipo = ?1 OR e.tipo = 'A')
           AND (e.nome LIKE ?2 OR e.nif LIKE ?2 OR IFNULL(e.telefone,'') LIKE ?2 OR IFNULL(e.email,'') LIKE ?2)
         ORDER BY e.id = 1 DESC, e.ativo DESC, e.nome LIMIT 2000",
    )
    .bind(f.tipo)
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.inativos.unwrap_or(false) as i64)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    // com o saldo e o crédito a favor, como na lista (o POS oferece pagar com o crédito do cliente)
    let row = sqlx::query(
        "SELECT e.*,
            (SELECT COALESCE(SUM(d.valor_pendente), 0) - COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d
              WHERE d.entidade_id = e.id AND d.estado = 'N') AS saldo,
            (SELECT COALESCE(SUM(d.credito_disponivel), 0) FROM documentos d
              WHERE d.entidade_id = e.id AND d.estado = 'N') AS credito
         FROM entidades e WHERE e.id = ?",
    )
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
    #[serde(default, deserialize_with = "super::bool_flexivel")]
    estado: Option<bool>,
    observacoes: Option<String>,
    telefone: Option<String>,
    email: Option<String>,
    limite_credito: Option<i64>,
    desconto: Option<f64>,
    nivel_preco: Option<i64>,
    /// retenção na fonte habitual (código da tabela de retenções; "" tira; `None` mantém)
    #[serde(default)]
    retencao: Option<String>,
    /// dados pessoais e de contacto (`None` mantém; «» apaga)
    #[serde(default)]
    pessoa_contacto: Option<String>,
    #[serde(default)]
    telefone2: Option<String>,
    #[serde(default)]
    data_nascimento: Option<String>,
    #[serde(default)]
    sexo: Option<String>,
    #[serde(default)]
    documento_identificacao: Option<String>,
    #[serde(default)]
    nacionalidade: Option<String>,
    #[serde(default)]
    profissao: Option<String>,
}

/// Formato do NIF angolano: 10 algarismos (empresas e NIF atribuído pela AGT), o n.º do BI das pessoas
/// singulares (9 algarismos, 2 letras da província e 3 algarismos, ex.: 001234567LA041) ou 999999999
/// (consumidor final).
pub fn nif_angolano_valido(nif: &str) -> bool {
    let c: Vec<char> = nif.trim().chars().collect();
    let algarismos = |x: &[char]| x.iter().all(|c| c.is_ascii_digit());
    nif.trim() == "999999999"
        || (c.len() == 10 && algarismos(&c))
        || (c.len() == 14 && algarismos(&c[..9]) && c[9..11].iter().all(|c| c.is_ascii_uppercase()) && algarismos(&c[11..]))
}

pub async fn guardar(st: &AppState, e: EntidadeIn) -> AppResult<Value> {
    st.exigir_perm(if e.tipo.as_deref() == Some("F") { "fornecedores" } else { "clientes" })?;
    // dados pessoais: validados antes de gravar
    let pessoais = dados_pessoais(&e)?;
    if e.nome.trim().is_empty() {
        return Err(AppError::msg("O nome é obrigatório."));
    }
    if e.id == Some(1) {
        return Err(AppError::msg("O cliente \"Consumidor Final\" não pode ser alterado."));
    }
    let nif = e
        .nif
        .map(|n| n.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '.').collect::<String>().to_uppercase())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "999999999".into());
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
    // NIF angolano: só se recusa um NIF novo ou alterado (as fichas antigas com NIF mal escrito continuam a abrir)
    if pais == "AO" && !nif_angolano_valido(&nif) {
        let anterior: Option<String> = match e.id {
            Some(id) => sqlx::query_scalar("SELECT nif FROM entidades WHERE id = ?").bind(id).fetch_optional(&st.pool).await?,
            None => None,
        };
        if anterior.as_deref() != Some(nif.as_str()) {
            return Err(AppError::msg(format!(
                "NIF inválido ({nif}). Em Angola o NIF tem 10 algarismos (empresas) ou é o n.º do BI: 9 algarismos, 2 letras e 3 algarismos (ex.: 001234567LA041)."
            )));
        }
    }
    // retenção habitual: tem de ser uma das retenções activas («» tira a que havia)
    let retencao: Option<Option<String>> = match e.retencao.as_deref().map(str::trim) {
        None => None,
        Some("") => Some(None),
        Some(c) => {
            let existe: Option<String> = sqlx::query_scalar("SELECT codigo FROM retencoes WHERE codigo = ? AND ativo = 1").bind(c).fetch_optional(&st.pool).await?;
            Some(Some(existe.ok_or_else(|| AppError::msg(format!("Retenção na fonte desconhecida: {c}.")))?))
        }
    };
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
    let id = e.id.unwrap_or_else(|| res.last_insert_rowid());
    if let Some(r) = retencao {
        sqlx::query("UPDATE entidades SET retencao = ? WHERE id = ?").bind(r).bind(id).execute(&st.pool).await?;
    }
    for (coluna, valor) in pessoais {
        sqlx::query(&format!("UPDATE entidades SET {coluna} = ? WHERE id = ?")).bind(valor).bind(id).execute(&st.pool).await?;
    }
    Ok(json!({ "id": id }))
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

// ============================== eliminar e acções em lote ==============================

/// A permissão que uma entidade pede: clientes ou fornecedores (as duas coisas pede a de clientes).
async fn exigir_da_entidade(st: &AppState, id: i64) -> AppResult<()> {
    if id == 1 {
        return Err(AppError::msg("O cliente \"Consumidor Final\" não pode ser eliminado nem desactivado."));
    }
    let tipo: String = sqlx::query_scalar("SELECT tipo FROM entidades WHERE id = ?").bind(id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    st.exigir_perm(if tipo == "F" { "fornecedores" } else { "clientes" })?;
    Ok(())
}

/// Elimina uma entidade que nunca foi usada. Com documentos, avenças, viaturas, pontos ou outra ligação fica só
/// **desactivada**: sai das listas e da escolha nos documentos, e os documentos antigos continuam com ela.
pub async fn eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    exigir_da_entidade(st, i.id).await?;
    let documentos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documentos WHERE entidade_id = ?").bind(i.id).fetch_one(&st.pool).await?;
    if documentos == 0 {
        let mut tx = db::transacao(&st.pool).await?;
        sqlx::query("DELETE FROM entidade_enderecos WHERE entidade_id = ?").bind(i.id).execute(&mut *tx).await?;
        // as chaves estrangeiras recusam se houver outra ligação (avenças, viaturas, pontos…): então desactiva-se
        if sqlx::query("DELETE FROM entidades WHERE id = ?").bind(i.id).execute(&mut *tx).await.is_ok() {
            tx.commit().await?;
            return Ok(json!({ "eliminado": true }));
        }
        tx.rollback().await?;
    }
    sqlx::query("UPDATE entidades SET ativo = 0 WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "eliminado": false, "desactivado": true, "documentos": documentos }))
}

#[derive(Deserialize)]
pub struct LoteIn {
    ids: Vec<i64>,
    /// activar | desactivar
    accao: String,
}

/// Activar ou desactivar várias entidades de uma vez (cada uma fica na auditoria).
pub async fn lote(st: &AppState, l: LoteIn) -> AppResult<Value> {
    let ativo = match l.accao.as_str() {
        "activar" => 1,
        "desactivar" => 0,
        _ => return Err(AppError::msg("Acção desconhecida.")),
    };
    if l.ids.is_empty() {
        return Err(AppError::msg("Escolha pelo menos uma ficha."));
    }
    let mut alterados = 0;
    for &id in &l.ids {
        exigir_da_entidade(st, id).await?;
        // ao reactivar, o NIF não pode estar já noutra ficha activa
        if ativo == 1 {
            let dup: Option<String> = sqlx::query_scalar(
                "SELECT o.nome FROM entidades e JOIN entidades o ON o.nif = e.nif AND o.id <> e.id AND o.ativo = 1 WHERE e.id = ? AND e.nif <> '999999999'",
            )
            .bind(id)
            .fetch_optional(&st.pool)
            .await?;
            if let Some(n) = dup {
                return Err(AppError::msg(format!("Não se reactiva: o NIF já está na ficha activa \"{n}\".")));
            }
        }
        let antes = super::auditoria::ficha(st, "entidades", id).await;
        let n = sqlx::query("UPDATE entidades SET ativo = ? WHERE id = ? AND ativo <> ?").bind(ativo).bind(id).bind(ativo).execute(&st.pool).await?.rows_affected();
        if n > 0 {
            alterados += 1;
            let agora = super::auditoria::ficha(st, "entidades", id).await;
            super::auditoria::registar_ficha(st, "entidades", id, antes, agora, None).await?;
        }
    }
    Ok(json!({ "alterados": alterados }))
}

/// A ficha activa que já tem este NIF (para avisar enquanto se escreve), ou `null`.
#[derive(Deserialize)]
pub struct NifIn {
    nif: String,
    #[serde(default)]
    id: Option<i64>,
}

pub async fn nif_usado(st: &AppState, n: NifIn) -> AppResult<Value> {
    st.sessao()?;
    let nif: String = n.nif.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '.').collect::<String>().to_uppercase();
    if nif.is_empty() || nif == "999999999" {
        return Ok(Value::Null);
    }
    let r: Option<(i64, String)> = sqlx::query_as("SELECT id, nome FROM entidades WHERE nif = ? AND id <> ? AND ativo = 1 LIMIT 1")
        .bind(&nif)
        .bind(n.id.unwrap_or(0))
        .fetch_optional(&st.pool)
        .await?;
    Ok(r.map(|(id, nome)| json!({ "id": id, "nome": nome })).unwrap_or(Value::Null))
}

/// Os dados pessoais que vieram no pedido, validados: (coluna, valor; `None` apaga). Os que não vieram não mudam.
fn dados_pessoais(e: &EntidadeIn) -> AppResult<Vec<(&'static str, Option<String>)>> {
    let limpo = |v: &Option<String>| v.as_deref().map(|x| x.trim().to_string());
    let mut r = Vec::new();
    for (coluna, valor, maximo) in [
        ("pessoa_contacto", &e.pessoa_contacto, 120),
        ("telefone2", &e.telefone2, 40),
        ("documento_identificacao", &e.documento_identificacao, 40),
        ("nacionalidade", &e.nacionalidade, 60),
        ("profissao", &e.profissao, 80),
    ] {
        if let Some(v) = limpo(valor) {
            if v.chars().count() > maximo {
                return Err(AppError::msg(format!("Texto demasiado longo em {} (até {maximo} caracteres).", coluna.replace('_', " "))));
            }
            r.push((coluna, Some(v).filter(|x| !x.is_empty())));
        }
    }
    if let Some(d) = limpo(&e.data_nascimento) {
        if !d.is_empty() {
            let data = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|_| AppError::msg("Data de nascimento inválida."))?;
            if data > chrono::Local::now().date_naive() || data < chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap() {
                return Err(AppError::msg("A data de nascimento tem de estar entre 1900 e hoje."));
            }
        }
        r.push(("data_nascimento", Some(d).filter(|x| !x.is_empty())));
    }
    if let Some(sx) = limpo(&e.sexo) {
        let sx = sx.to_uppercase();
        if !matches!(sx.as_str(), "" | "M" | "F") {
            return Err(AppError::msg("Sexo: M ou F."));
        }
        r.push(("sexo", Some(sx).filter(|x| !x.is_empty())));
    }
    Ok(r)
}

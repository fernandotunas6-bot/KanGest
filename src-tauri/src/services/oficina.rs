//! Oficina (KIGEST GC → Oficina): registo de viaturas e ordens de serviço, com facturação.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct FiltroIn {
    q: Option<String>,
    entidade_id: Option<i64>,
    estado: Option<String>,
}

pub async fn viaturas(st: &AppState, f: FiltroIn) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    let rows = sqlx::query(
        "SELECT v.*, e.nome AS cliente, e.nif AS cliente_nif,
                (SELECT COUNT(*) FROM of_ordens o WHERE o.viatura_id = v.id) AS ordens
         FROM of_viaturas v LEFT JOIN entidades e ON e.id = v.entidade_id
         WHERE v.ativo = 1 AND (?2 IS NULL OR v.entidade_id = ?2)
           AND (IFNULL(v.matricula,'') LIKE ?1 OR v.marca LIKE ?1 OR IFNULL(v.modelo,'') LIKE ?1 OR IFNULL(v.chassi,'') LIKE ?1 OR IFNULL(e.nome,'') LIKE ?1)
         ORDER BY v.matricula",
    )
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.entidade_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct ViaturaIn {
    id: Option<i64>,
    entidade_id: Option<i64>,
    matricula: Option<String>,
    marca: String,
    modelo: Option<String>,
    cor: Option<String>,
    ano: Option<i64>,
    chassi: Option<String>,
    motor: Option<String>,
    combustivel: Option<String>,
    categoria: Option<String>,
    km: Option<i64>,
    observacoes: Option<String>,
    ativo: Option<bool>,
}

fn txt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

pub async fn viatura_guardar(st: &AppState, v: ViaturaIn) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    if v.marca.trim().is_empty() {
        return Err(AppError::msg("Indique a marca da viatura."));
    }
    let matricula = txt(&v.matricula).map(|m| m.to_uppercase());
    if let Some(m) = &matricula {
        let dup: Option<i64> = sqlx::query_scalar("SELECT id FROM of_viaturas WHERE matricula = ? AND id <> ? AND ativo = 1")
            .bind(m)
            .bind(v.id.unwrap_or(0))
            .fetch_optional(&st.pool)
            .await?;
        if dup.is_some() {
            return Err(AppError::msg(format!("Já existe uma viatura com a matrícula {m}.")));
        }
    }
    let sql = if v.id.is_some() {
        "UPDATE of_viaturas SET entidade_id=?1, matricula=?2, marca=?3, modelo=?4, cor=?5, ano=?6, chassi=?7, motor=?8, combustivel=?9,
            categoria=?10, km=?11, observacoes=?12, ativo=?13 WHERE id=?14"
    } else {
        "INSERT INTO of_viaturas (entidade_id, matricula, marca, modelo, cor, ano, chassi, motor, combustivel, categoria, km, observacoes, ativo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
    };
    let mut q = sqlx::query(sql)
        .bind(v.entidade_id)
        .bind(&matricula)
        .bind(v.marca.trim())
        .bind(txt(&v.modelo))
        .bind(txt(&v.cor))
        .bind(v.ano)
        .bind(txt(&v.chassi).map(|c| c.to_uppercase()))
        .bind(txt(&v.motor))
        .bind(txt(&v.combustivel))
        .bind(txt(&v.categoria))
        .bind(v.km.unwrap_or(0).max(0))
        .bind(txt(&v.observacoes))
        .bind(v.ativo.unwrap_or(true) as i64);
    if let Some(id) = v.id {
        q = q.bind(id);
    }
    let res = q.execute(&st.pool).await?;
    Ok(json!({ "id": v.id.unwrap_or_else(|| res.last_insert_rowid()) }))
}

/// Valores já usados (marcas, modelos, cores) para sugestões nos formulários.
pub async fn sugestoes(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    let lista = |col: &'static str| {
        let sql = format!("SELECT DISTINCT {col} FROM of_viaturas WHERE {col} IS NOT NULL AND {col} <> '' ORDER BY {col}");
        async move { sqlx::query_scalar::<_, String>(&sql).fetch_all(&st.pool).await }
    };
    Ok(json!({
        "marcas": lista("marca").await?,
        "modelos": lista("modelo").await?,
        "cores": lista("cor").await?,
        "tecnicos": sqlx::query_scalar::<_, String>("SELECT DISTINCT tecnico FROM of_ordens WHERE tecnico IS NOT NULL AND tecnico <> '' ORDER BY tecnico").fetch_all(&st.pool).await?,
    }))
}

pub async fn ordens(st: &AppState, f: FiltroIn) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    let rows = sqlx::query(
        "SELECT o.*, v.matricula, v.marca, v.modelo, e.nome AS cliente, d.numero_doc AS documento,
                (SELECT COUNT(*) FROM of_ordem_linhas l WHERE l.ordem_id = o.id) AS n_linhas
         FROM of_ordens o JOIN of_viaturas v ON v.id = o.viatura_id
         LEFT JOIN entidades e ON e.id = o.entidade_id LEFT JOIN documentos d ON d.id = o.documento_id
         WHERE (?2 IS NULL OR o.estado = ?2)
           AND (o.numero LIKE ?1 OR IFNULL(v.matricula,'') LIKE ?1 OR IFNULL(e.nome,'') LIKE ?1 OR IFNULL(o.avaria,'') LIKE ?1)
         ORDER BY o.id DESC LIMIT 500",
    )
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.estado.filter(|s| !s.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

pub async fn ordem(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    let o = sqlx::query(
        "SELECT o.*, v.matricula, v.marca, v.modelo, v.cor, v.chassi, v.ano, v.combustivel, v.categoria,
                e.nome AS cliente, e.nif AS cliente_nif, e.endereco AS cliente_morada, e.localidade AS cliente_localidade,
                d.numero_doc AS documento, u.nome AS utilizador
         FROM of_ordens o JOIN of_viaturas v ON v.id = o.viatura_id
         LEFT JOIN entidades e ON e.id = o.entidade_id LEFT JOIN documentos d ON d.id = o.documento_id
         JOIN utilizadores u ON u.id = o.utilizador_id WHERE o.id = ?",
    )
    .bind(i.id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let linhas = sqlx::query(
        "SELECT l.*, a.referencia, a.tipo AS artigo_tipo FROM of_ordem_linhas l LEFT JOIN artigos a ON a.id = l.artigo_id WHERE l.ordem_id = ? ORDER BY l.id",
    )
    .bind(i.id)
    .fetch_all(&st.pool)
    .await?;
    let mut v = db::row_json(&o);
    v["linhas"] = db::rows_json(&linhas);
    Ok(v)
}

#[derive(Deserialize)]
pub struct LinhaIn {
    artigo_id: Option<i64>,
    descricao: String,
    quantidade: Option<f64>,
    mecanico: Option<String>,
}

#[derive(Deserialize)]
pub struct OrdemIn {
    id: Option<i64>,
    viatura_id: i64,
    entidade_id: Option<i64>,
    km_entrada: Option<i64>,
    km_saida: Option<i64>,
    referencia: Option<String>,
    avaria: Option<String>,
    diagnostico: Option<String>,
    tecnico: Option<String>,
    estado: Option<String>,
    observacoes: Option<String>,
    #[serde(default)]
    linhas: Vec<LinhaIn>,
}

pub async fn ordem_guardar(st: &AppState, o: OrdemIn) -> AppResult<Value> {
    let s = st.exigir_perm("oficina")?;
    let estado = o.estado.clone().unwrap_or_else(|| "ABERTA".into());
    if !["ABERTA", "EM_CURSO", "CONCLUIDA", "CANCELADA"].contains(&estado.as_str()) {
        return Err(AppError::msg("Estado inválido."));
    }
    let viatura = sqlx::query("SELECT entidade_id, km FROM of_viaturas WHERE id = ?").bind(o.viatura_id).fetch_optional(&st.pool).await?.ok_or_else(|| AppError::msg("Viatura não encontrada."))?;
    let entidade = o.entidade_id.or(viatura.get::<Option<i64>, _>("entidade_id"));
    if let (Some(e), Some(sai)) = (o.km_entrada, o.km_saida) {
        if sai < e {
            return Err(AppError::msg("A quilometragem de saída não pode ser inferior à de entrada."));
        }
    }
    let mut tx = st.pool.begin().await?;
    let id = match o.id {
        Some(id) => {
            let atual: String = sqlx::query_scalar("SELECT estado FROM of_ordens WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NaoEncontrado)?;
            if atual == "FACTURADA" {
                return Err(AppError::msg("A ordem de serviço já foi facturada e não pode ser alterada."));
            }
            sqlx::query(
                "UPDATE of_ordens SET viatura_id=?, entidade_id=?, km_entrada=?, km_saida=?, referencia=?, avaria=?, diagnostico=?, tecnico=?, estado=?,
                    data_conclusao = CASE WHEN ? = 'CONCLUIDA' THEN COALESCE(data_conclusao, ?) ELSE data_conclusao END, observacoes=? WHERE id=?",
            )
            .bind(o.viatura_id)
            .bind(entidade)
            .bind(o.km_entrada)
            .bind(o.km_saida)
            .bind(txt(&o.referencia))
            .bind(txt(&o.avaria))
            .bind(txt(&o.diagnostico))
            .bind(txt(&o.tecnico))
            .bind(&estado)
            .bind(&estado)
            .bind(db::agora())
            .bind(txt(&o.observacoes))
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM of_ordem_linhas WHERE ordem_id = ?").bind(id).execute(&mut *tx).await?;
            id
        }
        None => {
            let ano = &db::hoje()[..4];
            let n: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(CAST(substr(numero, 9) AS INTEGER)), 0) + 1 FROM of_ordens WHERE numero LIKE 'OS ' || ? || '/%'")
                .bind(ano)
                .fetch_one(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO of_ordens (numero, data_entrada, viatura_id, entidade_id, km_entrada, km_saida, referencia, avaria, diagnostico, tecnico, estado, observacoes, utilizador_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("OS {ano}/{n}"))
            .bind(db::agora())
            .bind(o.viatura_id)
            .bind(entidade)
            .bind(o.km_entrada.or(Some(viatura.get::<i64, _>("km"))))
            .bind(o.km_saida)
            .bind(txt(&o.referencia))
            .bind(txt(&o.avaria))
            .bind(txt(&o.diagnostico))
            .bind(txt(&o.tecnico))
            .bind(&estado)
            .bind(txt(&o.observacoes))
            .bind(s.id)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid()
        }
    };
    for l in &o.linhas {
        if l.descricao.trim().is_empty() {
            continue;
        }
        sqlx::query("INSERT INTO of_ordem_linhas (ordem_id, artigo_id, descricao, quantidade, mecanico) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(l.artigo_id)
            .bind(l.descricao.trim())
            .bind(l.quantidade.unwrap_or(1.0).max(0.0))
            .bind(txt(&l.mecanico))
            .execute(&mut *tx)
            .await?;
    }
    // a quilometragem da viatura acompanha a última OS
    if let Some(km) = o.km_saida.or(o.km_entrada) {
        sqlx::query("UPDATE of_viaturas SET km = MAX(km, ?) WHERE id = ?").bind(km).bind(o.viatura_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct AssociarIn {
    ordem_id: i64,
    documento_id: i64,
}

/// "Associar Ordem de Serviço": liga a OS à factura emitida e marca-a como facturada.
pub async fn associar(st: &AppState, a: AssociarIn) -> AppResult<Value> {
    st.exigir_perm("oficina")?;
    let doc = sqlx::query("SELECT tipo_codigo, estado FROM documentos WHERE id = ?")
        .bind(a.documento_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::msg("Documento não encontrado."))?;
    if !matches!(doc.get::<String, _>("tipo_codigo").as_str(), "FT" | "FR") || doc.get::<String, _>("estado") == "A" {
        return Err(AppError::msg("A ordem de serviço só pode ser associada a uma factura ou factura-recibo válida."));
    }
    let n = sqlx::query("UPDATE of_ordens SET documento_id = ?, estado = 'FACTURADA', data_conclusao = COALESCE(data_conclusao, ?) WHERE id = ? AND estado <> 'FACTURADA'")
        .bind(a.documento_id)
        .bind(db::agora())
        .bind(a.ordem_id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::msg("A ordem de serviço já está facturada."));
    }
    Ok(Value::Null)
}

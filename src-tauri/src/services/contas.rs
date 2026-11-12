//! Conta corrente de clientes e fornecedores: Contas a Receber / a Pagar, Documentos Pendentes,
//! Aviso de Vencimento e Extracto de Conta Corrente (KIGEST GC → Tesouraria).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::agt;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct PendentesIn {
    /// 'C' = contas a receber (clientes) | 'F' = contas a pagar (fornecedores)
    tipo: String,
    vencidos: Option<bool>,
    entidade_id: Option<i64>,
}

pub async fn pendentes(st: &AppState, f: PendentesIn) -> AppResult<Value> {
    st.sessao()?;
    let grupo = if f.tipo == "F" { "COMPRAS" } else { "VENDAS" };
    let hoje = db::hoje();
    let docs = sqlx::query(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_desc, d.data_emissao, d.data_vencimento, d.entidade_id,
                d.entidade_nome, d.entidade_nif, d.total, d.valor_pendente, d.doc_externo,
                CAST(julianday(?1) - julianday(COALESCE(d.data_vencimento, d.data_emissao)) AS INTEGER) AS dias_atraso
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ?2
           AND (?3 = 0 OR COALESCE(d.data_vencimento, d.data_emissao) < ?1)
           AND (?4 IS NULL OR d.entidade_id = ?4)
         ORDER BY COALESCE(d.data_vencimento, d.data_emissao), d.id",
    )
    .bind(&hoje)
    .bind(grupo)
    .bind(f.vencidos.unwrap_or(false) as i64)
    .bind(f.entidade_id)
    .fetch_all(&st.pool)
    .await?;
    let por_entidade = sqlx::query(
        "SELECT d.entidade_id, d.entidade_nome, d.entidade_nif, COUNT(*) AS documentos, SUM(d.valor_pendente) AS total,
                SUM(CASE WHEN COALESCE(d.data_vencimento, d.data_emissao) < ?1 THEN d.valor_pendente ELSE 0 END) AS vencido
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ?2
         GROUP BY d.entidade_id, d.entidade_nome, d.entidade_nif ORDER BY total DESC",
    )
    .bind(&hoje)
    .bind(grupo)
    .fetch_all(&st.pool)
    .await?;
    Ok(json!({ "documentos": db::rows_json(&docs), "entidades": db::rows_json(&por_entidade) }))
}

#[derive(Deserialize)]
pub struct ExtratoIn {
    entidade_id: i64,
    de: Option<String>,
    ate: Option<String>,
    /// VENDAS (cliente) | COMPRAS (fornecedor); por omissão pelo tipo da entidade.
    grupo: Option<String>,
}

/// Grupo da conta corrente: o pedido, ou o do tipo da entidade (fornecedor → COMPRAS).
fn grupo_de(pedido: Option<&str>, tipo_entidade: &str) -> &'static str {
    match pedido {
        Some("COMPRAS") => "COMPRAS",
        Some("VENDAS") => "VENDAS",
        _ if tipo_entidade == "F" => "COMPRAS",
        _ => "VENDAS",
    }
}

/// Extracto de conta corrente com saldo acumulado: documentos, compensações de crédito,
/// devoluções de dinheiro e regularizações.
pub async fn extrato(st: &AppState, f: ExtratoIn) -> AppResult<Value> {
    st.sessao()?;
    let entidade = sqlx::query("SELECT id, nome, nif, tipo FROM entidades WHERE id = ?")
        .bind(f.entidade_id)
        .fetch_one(&st.pool)
        .await?;
    let grupo = grupo_de(f.grupo.as_deref(), &entidade.get::<String, _>("tipo"));
    let rows = sqlx::query(
        "SELECT * FROM (
            SELECT d.id, d.data_emissao, d.data_hora AS ordem, d.numero_doc, t.descricao AS tipo_desc, d.doc_externo, d.estado,
                   CASE WHEN t.conta_corrente = 1 THEN d.total - d.total_retencao ELSE 0 END AS debito,
                   CASE WHEN t.conta_corrente = -1 THEN d.total ELSE 0 END AS credito,
                   d.valor_pendente, d.credito_disponivel, 'DOCUMENTO' AS origem
            FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
            WHERE d.entidade_id = ?1 AND t.conta_corrente <> 0 AND d.estado <> 'A' AND t.grupo = ?4
            UNION ALL
            SELECT r.id, r.data_emissao, r.data_hora || 'z', r.numero_doc, 'Compensação de crédito (' || c.numero_doc || ')', NULL, r.estado,
                   u.valor, 0, 0, 0, 'COMPENSACAO'
            FROM credito_utilizacoes u JOIN documentos r ON r.id = u.documento_id JOIN documentos c ON c.id = u.credito_id
            JOIN tipos_documento t ON t.codigo = c.tipo_codigo
            WHERE c.entidade_id = ?1 AND r.estado <> 'A' AND t.grupo = ?4
            UNION ALL
            SELECT c.id, v.data, v.data || 'z', c.numero_doc, 'Devolução de dinheiro', NULL, c.estado, v.valor, 0, 0, 0, 'DEVOLUCAO'
            FROM credito_devolucoes v JOIN documentos c ON c.id = v.credito_id JOIN tipos_documento t ON t.codigo = c.tipo_codigo
            LEFT JOIN movimentos_tesouraria m ON m.id = v.movimento_id
            WHERE c.entidade_id = ?1 AND t.grupo = ?4 AND (v.movimento_id IS NULL OR m.anulado = 0)
            UNION ALL
            SELECT g.id, g.data, g.data_hora, 'REG ' || g.id, 'Regularização: ' || g.justificacao, NULL, 'N',
                   CASE WHEN g.tipo = 'DEBITO' THEN g.valor ELSE 0 END, CASE WHEN g.tipo = 'CREDITO' THEN g.valor ELSE 0 END, 0, 0, 'REGULARIZACAO'
            FROM cc_regularizacoes g WHERE g.entidade_id = ?1 AND g.grupo = ?4 AND g.anulado = 0
         ) x
         WHERE (?2 IS NULL OR x.data_emissao >= ?2) AND (?3 IS NULL OR x.data_emissao <= ?3)
         ORDER BY x.data_emissao, x.ordem",
    )
    .bind(f.entidade_id)
    .bind(f.de.filter(|s| !s.is_empty()))
    .bind(f.ate.filter(|s| !s.is_empty()))
    .bind(grupo)
    .fetch_all(&st.pool)
    .await?;
    let mut saldo = 0i64;
    let mut linhas = Vec::with_capacity(rows.len());
    for r in &rows {
        saldo += r.get::<i64, _>("debito") - r.get::<i64, _>("credito");
        let mut v = db::row_json(r);
        v["saldo"] = json!(saldo);
        linhas.push(v);
    }
    let (pendente, credito): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(d.valor_pendente), 0), COALESCE(SUM(d.credito_disponivel), 0)
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.entidade_id = ? AND d.estado = 'N' AND t.grupo = ?",
    )
    .bind(f.entidade_id)
    .bind(grupo)
    .fetch_one(&st.pool)
    .await?;
    Ok(json!({ "entidade": db::row_json(&entidade), "grupo": grupo, "movimentos": linhas, "saldo": saldo, "pendente": pendente, "credito": credito }))
}

#[derive(Deserialize)]
pub struct EntidadeIn {
    entidade_id: i64,
    grupo: Option<String>,
}

/// Créditos por utilizar (adiantamentos e notas de crédito não compensadas).
pub async fn creditos(st: &AppState, f: EntidadeIn) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_desc, d.data_emissao, d.total, d.credito_disponivel
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.entidade_id = ? AND d.estado = 'N' AND d.credito_disponivel > 0 AND t.grupo = ?
         ORDER BY d.data_emissao, d.id",
    )
    .bind(f.entidade_id)
    .bind(grupo_de(f.grupo.as_deref(), ""))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct DevolverIn {
    credito_id: i64,
    valor: i64,
    conta_id: i64,
    descricao: Option<String>,
    centro_custo_id: Option<i64>,
}

/// Devolução de dinheiro (Fr_DevolucaoDinheiro): paga ao cliente o crédito de uma nota de crédito
/// ou de um adiantamento (ou regista o reembolso recebido de um fornecedor).
pub async fn devolver(st: &AppState, d: DevolverIn) -> AppResult<Value> {
    let doc = sqlx::query(
        "SELECT d.numero_doc, d.entidade_id, d.entidade_nome, d.credito_disponivel, d.estado, t.grupo
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.id = ?",
    )
    .bind(d.credito_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let grupo: String = doc.get("grupo");
    let s = st.exigir_perm(if grupo == "COMPRAS" { "tesouraria.pagamento" } else { "tesouraria.recibo" })?;
    let disponivel: i64 = doc.get("credito_disponivel");
    if doc.get::<String, _>("estado") != "N" || d.valor <= 0 || d.valor > disponivel {
        return Err(AppError::msg(format!("Valor inválido: o crédito disponível é {}.", agt::total_fmt(disponivel))));
    }
    let numero: String = doc.get("numero_doc");
    let mut tx = st.pool.begin().await?;
    let mov = sqlx::query(
        "INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, centro_custo_id, entidade_id, documento, utilizador_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(db::hoje())
    .bind(db::agora())
    .bind(d.conta_id)
    .bind(if grupo == "COMPRAS" { "ENTRADA" } else { "SAIDA" })
    .bind(d.valor)
    .bind(d.descricao.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| format!("Devolução {numero} — {}", doc.get::<String, _>("entidade_nome"))))
    .bind(d.centro_custo_id)
    .bind(doc.get::<Option<i64>, _>("entidade_id"))
    .bind(&numero)
    .bind(s.id)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO credito_devolucoes (credito_id, movimento_id, valor, data) VALUES (?, ?, ?, ?)")
        .bind(d.credito_id)
        .bind(mov)
        .bind(d.valor)
        .bind(db::hoje())
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE documentos SET credito_disponivel = credito_disponivel - ? WHERE id = ?").bind(d.valor).bind(d.credito_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'DEVOLUCAO', ?)").bind(s.id).bind(format!("{numero}: {}", agt::total_fmt(d.valor))).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({ "movimento_id": mov }))
}

#[derive(Deserialize)]
pub struct RegularizarIn {
    entidade_id: i64,
    grupo: Option<String>,
    /// CREDITO: abate dívida (perdão / acerto) · DEBITO: anula crédito a favor da entidade
    tipo: String,
    valor: i64,
    justificacao: String,
}

/// Regularização de conta corrente (Fr_RegularizacaoContaCorrente). Não é documento fiscal:
/// abate a dívida dos documentos mais antigos (crédito) ou consome créditos por utilizar (débito).
pub async fn regularizar(st: &AppState, r: RegularizarIn) -> AppResult<Value> {
    let s = st.exigir_perm("tesouraria.regularizar")?;
    let ent = sqlx::query("SELECT tipo FROM entidades WHERE id = ?").bind(r.entidade_id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let grupo = grupo_de(r.grupo.as_deref(), &ent.get::<String, _>("tipo"));
    st.exigir_perm(if grupo == "COMPRAS" { "tesouraria.pagamento" } else { "tesouraria.recibo" })?;
    if r.justificacao.trim().len() < 5 {
        return Err(AppError::msg("Indique a justificação da regularização."));
    }
    if r.valor <= 0 || !matches!(r.tipo.as_str(), "CREDITO" | "DEBITO") {
        return Err(AppError::msg("Indique o tipo e o valor da regularização."));
    }
    let credito = r.tipo == "CREDITO";
    let mut tx = st.pool.begin().await?;
    let docs: Vec<(i64, i64)> = sqlx::query_as(if credito {
        "SELECT d.id, d.valor_pendente FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.entidade_id = ? AND d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ? ORDER BY COALESCE(d.data_vencimento, d.data_emissao), d.id"
    } else {
        "SELECT d.id, d.credito_disponivel FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.entidade_id = ? AND d.estado = 'N' AND d.credito_disponivel > 0 AND t.grupo = ? ORDER BY d.data_emissao, d.id"
    })
    .bind(r.entidade_id)
    .bind(grupo)
    .fetch_all(&mut *tx)
    .await?;
    let disponivel: i64 = docs.iter().map(|x| x.1).sum();
    if r.valor > disponivel {
        return Err(AppError::msg(if credito {
            format!("O valor excede a dívida em aberto ({}).", agt::total_fmt(disponivel))
        } else {
            format!("O valor excede o crédito por utilizar ({}).", agt::total_fmt(disponivel))
        }));
    }
    let id = sqlx::query("INSERT INTO cc_regularizacoes (data, data_hora, entidade_id, grupo, tipo, valor, justificacao, utilizador_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(db::hoje())
        .bind(db::agora())
        .bind(r.entidade_id)
        .bind(grupo)
        .bind(&r.tipo)
        .bind(r.valor)
        .bind(r.justificacao.trim())
        .bind(s.id)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    let mut resto = r.valor;
    for (did, v) in docs {
        if resto == 0 {
            break;
        }
        let usa = resto.min(v);
        sqlx::query(if credito {
            "UPDATE documentos SET valor_pendente = valor_pendente - ? WHERE id = ?"
        } else {
            "UPDATE documentos SET credito_disponivel = credito_disponivel - ? WHERE id = ?"
        })
        .bind(usa)
        .bind(did)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO cc_regularizacao_docs (regularizacao_id, documento_id, valor) VALUES (?, ?, ?)").bind(id).bind(did).bind(usa).execute(&mut *tx).await?;
        resto -= usa;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'REGULARIZACAO', ?)")
        .bind(s.id)
        .bind(format!("{} {} entidade {}: {}", r.tipo, agt::total_fmt(r.valor), r.entidade_id, r.justificacao.trim()))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct AnularRegIn {
    id: i64,
    motivo: String,
}

pub async fn regularizacao_anular(st: &AppState, a: AnularRegIn) -> AppResult<Value> {
    let s = st.exigir_perm("tesouraria.regularizar")?;
    if a.motivo.trim().len() < 3 {
        return Err(AppError::msg("Indique o motivo."));
    }
    let row = sqlx::query("SELECT tipo, anulado FROM cc_regularizacoes WHERE id = ?").bind(a.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    if row.get::<i64, _>("anulado") == 1 {
        return Err(AppError::msg("A regularização já está anulada."));
    }
    let credito = row.get::<String, _>("tipo") == "CREDITO";
    let mut tx = st.pool.begin().await?;
    for (did, v) in sqlx::query_as::<_, (i64, i64)>("SELECT documento_id, valor FROM cc_regularizacao_docs WHERE regularizacao_id = ?").bind(a.id).fetch_all(&mut *tx).await? {
        sqlx::query(if credito {
            "UPDATE documentos SET valor_pendente = valor_pendente + ? WHERE id = ? AND estado = 'N'"
        } else {
            "UPDATE documentos SET credito_disponivel = credito_disponivel + ? WHERE id = ? AND estado = 'N'"
        })
        .bind(v)
        .bind(did)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE cc_regularizacoes SET anulado = 1 WHERE id = ?").bind(a.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'REGULARIZACAO_ANULAR', ?)").bind(s.id).bind(format!("REG {}: {}", a.id, a.motivo.trim())).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

pub async fn regularizacoes(st: &AppState, f: EntidadeIn) -> AppResult<Value> {
    st.sessao()?;
    db::query_json_bind(
        &st.pool,
        "SELECT g.*, u.nome AS utilizador FROM cc_regularizacoes g JOIN utilizadores u ON u.id = g.utilizador_id WHERE g.entidade_id = ? ORDER BY g.id DESC",
        f.entidade_id,
    )
    .await
}

#[derive(Deserialize)]
pub struct VencidosIn {
    /// 'C' clientes | 'F' fornecedores
    tipo: Option<String>,
    /// documentos com vencimento neste intervalo (vazio = todos os vencidos até hoje)
    de: Option<String>,
    ate: Option<String>,
    entidade_id: Option<i64>,
}

/// Aviso de Vencimento (Fr_AvisoVencimento): documentos vencidos agrupados por entidade, para carta de aviso.
pub async fn vencidos(st: &AppState, f: VencidosIn) -> AppResult<Value> {
    st.sessao()?;
    let hoje = db::hoje();
    let grupo = if f.tipo.as_deref() == Some("F") { "COMPRAS" } else { "VENDAS" };
    let ate = f.ate.filter(|x| !x.is_empty()).map(|a| if a.as_str() >= hoje.as_str() { db::ontem() } else { a }).unwrap_or_else(db::ontem);
    let rows = sqlx::query(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_desc, d.data_emissao, COALESCE(d.data_vencimento, d.data_emissao) AS vencimento,
                d.entidade_id, d.total, d.valor_pendente, d.doc_externo,
                CAST(julianday(?1) - julianday(COALESCE(d.data_vencimento, d.data_emissao)) AS INTEGER) AS dias_atraso,
                e.nome, e.nif, e.endereco, e.localidade, e.telefone, e.email
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo JOIN entidades e ON e.id = d.entidade_id
         WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ?2
           AND COALESCE(d.data_vencimento, d.data_emissao) <= ?3 AND (?4 IS NULL OR COALESCE(d.data_vencimento, d.data_emissao) >= ?4)
           AND (?5 IS NULL OR d.entidade_id = ?5)
         ORDER BY e.nome, COALESCE(d.data_vencimento, d.data_emissao), d.id",
    )
    .bind(&hoje)
    .bind(grupo)
    .bind(&ate)
    .bind(f.de.filter(|x| !x.is_empty()))
    .bind(f.entidade_id)
    .fetch_all(&st.pool)
    .await?;
    let mut entidades: Vec<Value> = Vec::new();
    for r in &rows {
        let eid: i64 = r.get("entidade_id");
        let doc = json!({
            "id": r.get::<i64, _>("id"), "numero_doc": r.get::<String, _>("numero_doc"), "tipo_desc": r.get::<String, _>("tipo_desc"),
            "data_emissao": r.get::<String, _>("data_emissao"), "vencimento": r.get::<String, _>("vencimento"), "total": r.get::<i64, _>("total"),
            "valor_pendente": r.get::<i64, _>("valor_pendente"), "dias_atraso": r.get::<i64, _>("dias_atraso"), "doc_externo": r.get::<Option<String>, _>("doc_externo"),
        });
        match entidades.last_mut().filter(|e| e["entidade_id"] == eid) {
            Some(e) => {
                e["total"] = json!(e["total"].as_i64().unwrap_or(0) + r.get::<i64, _>("valor_pendente"));
                e["documentos"].as_array_mut().unwrap().push(doc);
            }
            None => entidades.push(json!({
                "entidade_id": eid, "nome": r.get::<String, _>("nome"), "nif": r.get::<String, _>("nif"),
                "endereco": r.get::<Option<String>, _>("endereco"), "localidade": r.get::<Option<String>, _>("localidade"),
                "telefone": r.get::<Option<String>, _>("telefone"), "email": r.get::<Option<String>, _>("email"),
                "total": r.get::<i64, _>("valor_pendente"), "documentos": [doc],
            })),
        }
    }
    Ok(json!({ "data": hoje, "entidades": entidades }))
}

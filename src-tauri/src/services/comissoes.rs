// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Comissões de vendedores.
//!
//! - Cada vendedor (funcionário escolhido no documento de venda) tem uma percentagem; 0 = sem comissão.
//! - Uma família pode ter a sua percentagem, que substitui a do vendedor nas linhas dessa família (ex.: 5 % nos
//!   serviços, 1 % nos telemóveis). Só conta para vendedores com comissão.
//! - Base: o valor sem IVA das linhas das FT, FR e ND do período, menos as NC (atribuídas ao vendedor da NC ou, sem
//!   ele, ao do documento de origem). Documentos anulados e histórico importado não contam.
//! - O total do mês pode ser lançado nas alterações do processamento de salários (rubrica 019, sujeita a INSS e IRT).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const RUBRICA: &str = "019";

/// Taxas actuais: vendedores (funcionários activos que podem vender) e famílias.
pub async fn taxas(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let p = &st.pool;
    let vendedores = db::query_json(
        p,
        "SELECT f.id, f.nome, f.numero, COALESCE(t.pct, 0) AS pct FROM funcionarios f
         LEFT JOIN comissao_taxas t ON t.tipo = 'VENDEDOR' AND t.ref_id = f.id
         WHERE f.ativo = 1 AND f.independente = 0 ORDER BY f.nome",
    )
    .await?;
    let familias = db::query_json(
        p,
        "SELECT fa.id, fa.descricao, t.pct FROM familias fa
         LEFT JOIN comissao_taxas t ON t.tipo = 'FAMILIA' AND t.ref_id = fa.id ORDER BY fa.descricao",
    )
    .await?;
    Ok(json!({ "vendedores": vendedores, "familias": familias }))
}

#[derive(Deserialize)]
pub struct TaxaIn {
    tipo: String,
    id: i64,
    /// Sem valor (família) ou 0 (vendedor): tira a taxa.
    pct: Option<f64>,
}

pub async fn taxa_guardar(st: &AppState, t: TaxaIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    let p = &st.pool;
    let (tabela, nome) = match t.tipo.as_str() {
        "VENDEDOR" => ("funcionarios", "nome"),
        "FAMILIA" => ("familias", "descricao"),
        _ => return Err(AppError::msg("Tipo de taxa inválido.")),
    };
    let descricao: Option<String> = sqlx::query_scalar(&format!("SELECT {nome} FROM {tabela} WHERE id = ?")).bind(t.id).fetch_optional(p).await?;
    let descricao = descricao.ok_or(AppError::NaoEncontrado)?;
    match t.pct.filter(|x| x.is_finite()) {
        Some(x) if !(0.0..=100.0).contains(&x) => return Err(AppError::msg("A comissão é uma percentagem entre 0 e 100.")),
        Some(x) if x > 0.0 || t.tipo == "FAMILIA" => {
            sqlx::query("INSERT INTO comissao_taxas (tipo, ref_id, pct) VALUES (?, ?, ?) ON CONFLICT(tipo, ref_id) DO UPDATE SET pct = excluded.pct")
                .bind(&t.tipo)
                .bind(t.id)
                .bind((x * 100.0).round() / 100.0)
                .execute(p)
                .await?;
        }
        _ => {
            sqlx::query("DELETE FROM comissao_taxas WHERE tipo = ? AND ref_id = ?").bind(&t.tipo).bind(t.id).execute(p).await?;
        }
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'COMISSAO', ?)")
        .bind(s.id)
        .bind(format!("Comissão de {descricao}: {}", t.pct.map(|x| format!("{x} %")).unwrap_or_else(|| "sem taxa própria".into())))
        .execute(p)
        .await?;
    Ok(Value::Null)
}

/// Comissões de um período: por vendedor, com o detalhe por família.
pub async fn calcular(p: &SqlitePool, de: &str, ate: &str) -> AppResult<Value> {
    let mut vendedor_pct: BTreeMap<i64, f64> = BTreeMap::new();
    let mut familia_pct: BTreeMap<i64, f64> = BTreeMap::new();
    for r in sqlx::query("SELECT tipo, ref_id, pct FROM comissao_taxas").fetch_all(p).await? {
        let (tipo, id, pct): (String, i64, f64) = (r.get("tipo"), r.get("ref_id"), r.get("pct"));
        if tipo == "VENDEDOR" { vendedor_pct.insert(id, pct) } else { familia_pct.insert(id, pct) };
    }
    let linhas = sqlx::query(
        "SELECT v.id AS vendedor_id, v.nome AS vendedor, a.familia_id, COALESCE(fa.descricao, '(sem família)') AS familia,
                COUNT(DISTINCT d.id) AS documentos,
                SUM(CASE WHEN d.tipo_codigo = 'NC' THEN -l.valor_liquido ELSE l.valor_liquido END) AS base
         FROM documento_linhas l
         JOIN documentos d ON d.id = l.documento_id
         LEFT JOIN documentos o ON o.id = d.doc_origem_id
         JOIN funcionarios v ON v.id = COALESCE(d.vendedor_id, o.vendedor_id)
         LEFT JOIN artigos a ON a.id = l.artigo_id
         LEFT JOIN familias fa ON fa.id = a.familia_id
         WHERE d.tipo_codigo IN ('FT', 'FR', 'ND', 'NC') AND d.estado <> 'A' AND d.importado = 0
           AND d.data_emissao BETWEEN ? AND ?
         GROUP BY v.id, a.familia_id ORDER BY v.nome, familia",
    )
    .bind(de)
    .bind(ate)
    .fetch_all(p)
    .await?;
    let mut por_vendedor: BTreeMap<(String, i64), Value> = BTreeMap::new();
    for l in &linhas {
        let vendedor: i64 = l.get("vendedor_id");
        let nome: String = l.get("vendedor");
        let base: i64 = l.get::<Option<i64>, _>("base").unwrap_or(0);
        let familia_id: Option<i64> = l.get("familia_id");
        let taxa_vendedor = vendedor_pct.get(&vendedor).copied().unwrap_or(0.0);
        // a taxa da família só vale para quem tem comissão
        let pct = if taxa_vendedor > 0.0 { familia_id.and_then(|f| familia_pct.get(&f).copied()).unwrap_or(taxa_vendedor) } else { 0.0 };
        let comissao = (base as f64 * pct / 100.0).round() as i64;
        let v = por_vendedor.entry((nome.clone(), vendedor)).or_insert_with(|| {
            json!({ "vendedor_id": vendedor, "vendedor": nome, "pct": taxa_vendedor, "base": 0, "comissao": 0, "documentos": 0, "familias": [] })
        });
        v["base"] = json!(v["base"].as_i64().unwrap_or(0) + base);
        v["comissao"] = json!(v["comissao"].as_i64().unwrap_or(0) + comissao);
        v["documentos"] = json!(v["documentos"].as_i64().unwrap_or(0) + l.get::<i64, _>("documentos"));
        if let Some(f) = v["familias"].as_array_mut() {
            f.push(json!({ "familia_id": familia_id, "familia": l.get::<String, _>("familia"), "base": base, "pct": pct, "comissao": comissao }));
        }
    }
    let vendedores: Vec<Value> = por_vendedor.into_values().collect();
    let total: i64 = vendedores.iter().map(|v| v["comissao"].as_i64().unwrap_or(0)).sum();
    let base: i64 = vendedores.iter().map(|v| v["base"].as_i64().unwrap_or(0)).sum();
    Ok(json!({ "de": de, "ate": ate, "vendedores": vendedores, "base": base, "total": total }))
}

#[derive(Deserialize)]
pub struct PeriodoIn {
    de: String,
    ate: String,
}

pub async fn relatorio(st: &AppState, f: PeriodoIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let (Ok(_), Ok(_)) = (NaiveDate::parse_from_str(&f.de, "%Y-%m-%d"), NaiveDate::parse_from_str(&f.ate, "%Y-%m-%d")) else {
        return Err(AppError::msg("Período inválido."));
    };
    calcular(&st.pool, &f.de, &f.ate).await
}

#[derive(Deserialize)]
pub struct MesIn {
    ano: i32,
    mes: u32,
}

/// Lança as comissões do mês nas alterações do processamento de salários desse mês. Lançar de novo substitui as
/// comissões ainda não processadas; um mês já processado não muda.
pub async fn lancar(st: &AppState, m: MesIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    let inicio = NaiveDate::from_ymd_opt(m.ano, m.mes, 1).ok_or_else(|| AppError::msg("Mês inválido."))?;
    let fim = NaiveDate::from_ymd_opt(if m.mes == 12 { m.ano + 1 } else { m.ano }, if m.mes == 12 { 1 } else { m.mes + 1 }, 1)
        .and_then(|d| d.pred_opt())
        .ok_or_else(|| AppError::msg("Mês inválido."))?;
    if super::rh::processado(&st.pool, m.ano as i64, m.mes as i64).await? {
        return Err(AppError::msg("Os salários deste mês já foram processados. Anule o processamento para lançar as comissões."));
    }
    let (de, ate) = (inicio.format("%Y-%m-%d").to_string(), fim.format("%Y-%m-%d").to_string());
    let r = calcular(&st.pool, &de, &ate).await?;
    let mes_txt = format!("{:02}/{}", inicio.month(), inicio.year());
    let mut tx = db::transacao(&st.pool).await?;
    sqlx::query("DELETE FROM rh_alteracoes WHERE rubrica = ? AND ano = ? AND mes = ? AND processado = 0")
        .bind(RUBRICA)
        .bind(m.ano)
        .bind(m.mes)
        .execute(&mut *tx)
        .await?;
    let (mut lancados, mut total) = (0, 0i64);
    for v in r["vendedores"].as_array().into_iter().flatten() {
        let comissao = v["comissao"].as_i64().unwrap_or(0);
        if comissao <= 0 {
            continue;
        }
        sqlx::query("INSERT INTO rh_alteracoes (funcionario_id, ano, mes, rubrica, quantidade, valor, descricao, data) VALUES (?, ?, ?, ?, 0, ?, ?, ?)")
            .bind(v["vendedor_id"].as_i64())
            .bind(m.ano)
            .bind(m.mes)
            .bind(RUBRICA)
            .bind(comissao)
            .bind(format!("Comissões de vendas de {mes_txt}"))
            .bind(db::hoje())
            .execute(&mut *tx)
            .await?;
        lancados += 1;
        total += comissao;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'COMISSAO', ?)")
        .bind(s.id)
        .bind(format!("Comissões de {mes_txt} lançadas nos salários: {lancados} vendedores, {:.2} Kz", total as f64 / 100.0))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "lancados": lancados, "total": total, "mes": mes_txt }))
}

// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Modo TV: o ecrã das vendas para uma televisão na loja ou na sede, como o Painel de Gestão da Livraria. Vendas de hoje
//! e do mês, hora a hora (com o mesmo dia da semana passada), a meta do mês, as lojas em ranking contra a meta, as
//! últimas vendas e os artigos mais vendidos de hoje. Um utilizador de uma loja só vê a sua.
//!
//! Numa televisão à vista de clientes não aparecem nomes de clientes: as últimas vendas mostram a loja, o primeiro
//! artigo e o valor.
//!
//! As metas mensais são por loja (armazém), em config('tv.metas') = {"<armazem_id>": cêntimos}; a da empresa é a soma.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const SINAL: &str = "CASE WHEN d.tipo_codigo = 'NC' THEN -1 ELSE 1 END";
const VENDAS: &str = "d.tipo_codigo IN ('FR','FT','ND','NC') AND d.estado <> 'A'";
const CHAVE_METAS: &str = "tv.metas";

pub async fn despachar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    use super::p;
    match metodo {
        "painel" => painel(st).await,
        "metas" => metas(st).await,
        "metasGuardar" => metas_guardar(st, p(params)?).await,
        _ => Err(AppError::msg(format!("Método desconhecido: tv.{metodo}"))),
    }
}

async fn ler_metas(st: &AppState) -> AppResult<BTreeMap<i64, i64>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE_METAS).fetch_optional(&st.pool).await?;
    let mapa: BTreeMap<String, i64> = v.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Ok(mapa.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.max(0)))).collect())
}

/// Dias do mês de uma data AAAA-MM-DD.
fn dias_do_mes(dia: &str) -> i64 {
    let d = NaiveDate::parse_from_str(dia, "%Y-%m-%d").unwrap_or_default();
    let (ano, mes) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(ano, mes, 1).and_then(|s| s.pred_opt()).map_or(30, |u| i64::from(u.day()))
}

async fn painel(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let p = &st.pool;
    let loja_id = st.loja();
    // o id vem da sessão (inteiro), não do pedido
    let loja = loja_id.map(|a| format!(" AND d.armazem_id = {a}")).unwrap_or_default();
    let hoje = db::hoje();
    let mes = format!("{}-01", &hoje[..7]);
    let dia: i64 = hoje[8..10].parse().unwrap_or(1);
    let dias_mes = dias_do_mes(&hoje);
    let semana_passada = (NaiveDate::parse_from_str(&hoje, "%Y-%m-%d").unwrap_or_default() - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();

    let (vendas_hoje, docs_hoje): (i64, i64) =
        sqlx::query_as(&format!("SELECT COALESCE(SUM({SINAL} * d.total), 0), COUNT(*) FROM documentos d WHERE {VENDAS} AND d.data_emissao = ?{loja}"))
            .bind(&hoje)
            .fetch_one(p)
            .await?;
    let (vendas_mes, docs_mes): (i64, i64) =
        sqlx::query_as(&format!("SELECT COALESCE(SUM({SINAL} * d.total), 0), COUNT(*) FROM documentos d WHERE {VENDAS} AND d.data_emissao >= ?{loja}"))
            .bind(&mes)
            .fetch_one(p)
            .await?;

    // hora a hora: hoje e o mesmo dia da semana passada (até à mesma hora, para comparar)
    let horas = sqlx::query_as::<_, (String, i64, i64)>(&format!(
        "SELECT d.data_emissao, CAST(substr(d.data_hora, 12, 2) AS INTEGER) AS h, SUM({SINAL} * d.total)
         FROM documentos d WHERE {VENDAS} AND d.data_emissao IN (?1, ?2){loja} GROUP BY d.data_emissao, h"
    ))
    .bind(&hoje)
    .bind(&semana_passada)
    .fetch_all(p)
    .await?;
    let mut por_hora = vec![(0i64, 0i64); 24];
    for (data, h, v) in horas {
        if let Some(x) = usize::try_from(h).ok().and_then(|h| por_hora.get_mut(h)) {
            if data == hoje {
                x.0 += v;
            } else {
                x.1 += v;
            }
        }
    }
    let hora_agora: usize = db::agora().get(11..13).and_then(|h| h.parse().ok()).unwrap_or(23);
    let ate_agora_semana_passada: i64 = por_hora.iter().take(hora_agora + 1).map(|x| x.1).sum();

    // lojas: hoje e o mês por armazém, contra a meta de cada uma
    let metas = ler_metas(st).await?;
    let filtro_loja = loja_id.map(|a| format!(" AND a.id = {a}")).unwrap_or_default();
    let lojas = sqlx::query_as::<_, (i64, String, i64, i64)>(&format!(
        "SELECT a.id, a.nome,
                COALESCE(SUM(CASE WHEN d.data_emissao = ?1 THEN {SINAL} * d.total END), 0),
                COALESCE(SUM({SINAL} * d.total), 0)
         FROM armazens a LEFT JOIN documentos d ON d.armazem_id = a.id AND {VENDAS} AND d.data_emissao >= ?2
         WHERE a.ativo = 1{filtro_loja} GROUP BY a.id"
    ))
    .bind(&hoje)
    .bind(&mes)
    .fetch_all(p)
    .await?;
    let mut lojas: Vec<Value> = lojas
        .into_iter()
        .map(|(id, nome, hoje_l, mes_l)| {
            let meta = metas.get(&id).copied().unwrap_or(0);
            let pct = (meta > 0).then(|| (mes_l as f64 / meta as f64 * 1000.0).round() / 10.0);
            json!({ "id": id, "nome": nome, "hoje": hoje_l, "mes": mes_l, "meta": meta, "percentagem": pct })
        })
        .collect();
    // com meta, pela percentagem; sem meta, pelo vendido no mês
    lojas.sort_by(|a, b| {
        let chave = |v: &Value| (v["percentagem"].as_f64().unwrap_or(-1.0), v["mes"].as_i64().unwrap_or(0));
        let (ca, cb) = (chave(a), chave(b));
        cb.0.total_cmp(&ca.0).then(cb.1.cmp(&ca.1))
    });
    let meta: i64 = lojas.iter().filter_map(|l| l["meta"].as_i64()).sum();
    let projeccao = vendas_mes * dias_mes / dia.max(1);

    // últimas vendas: loja, primeiro artigo e valor (sem nomes de clientes)
    let ultimas = sqlx::query(&format!(
        "SELECT d.numero_doc AS numero, d.data_hora AS quando, {SINAL} * d.total AS total, a.nome AS loja,
                (SELECT l.descricao FROM documento_linhas l WHERE l.documento_id = d.id ORDER BY l.id LIMIT 1) AS artigo,
                (SELECT COUNT(*) FROM documento_linhas l WHERE l.documento_id = d.id) AS artigos
         FROM documentos d LEFT JOIN armazens a ON a.id = d.armazem_id
         WHERE {VENDAS}{loja} ORDER BY d.id DESC LIMIT 12"
    ))
    .fetch_all(p)
    .await?;
    let top = sqlx::query(&format!(
        "SELECT l.descricao, SUM({SINAL} * l.quantidade) AS quantidade, SUM({SINAL} * l.total) AS total
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE {VENDAS} AND d.data_emissao = ?{loja} GROUP BY COALESCE(l.artigo_id, l.descricao) ORDER BY total DESC LIMIT 5"
    ))
    .bind(&hoje)
    .fetch_all(p)
    .await?;
    let empresa: Option<String> = sqlx::query_scalar("SELECT nome FROM empresa WHERE id = 1").fetch_optional(p).await?;

    Ok(json!({
        "agora": db::agora(), "empresa": empresa, "loja_unica": loja_id.is_some(),
        "dia": dia, "dias_mes": dias_mes,
        "hoje": { "total": vendas_hoje, "documentos": docs_hoje, "semana_passada_ate_agora": ate_agora_semana_passada },
        "mes": { "total": vendas_mes, "documentos": docs_mes, "meta": meta, "projeccao": projeccao,
                 "percentagem": (meta > 0).then(|| (vendas_mes as f64 / meta as f64 * 1000.0).round() / 10.0),
                 "ritmo_ideal": if meta > 0 { meta * dia / dias_mes } else { 0 } },
        "horas": por_hora.iter().enumerate().map(|(h, (a, b))| json!({ "hora": h, "hoje": a, "semana_passada": b })).collect::<Vec<_>>(),
        "lojas": lojas,
        "ultimas": db::rows_json(&ultimas),
        "top": db::rows_json(&top),
    }))
}

async fn metas(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let metas = ler_metas(st).await?;
    let lojas = sqlx::query_as::<_, (i64, String)>("SELECT id, nome FROM armazens WHERE ativo = 1 ORDER BY id").fetch_all(&st.pool).await?;
    Ok(json!({ "lojas": lojas.into_iter().map(|(id, nome)| json!({ "id": id, "nome": nome, "meta": metas.get(&id).copied().unwrap_or(0) })).collect::<Vec<_>>() }))
}

#[derive(Deserialize)]
pub struct MetasIn {
    /// cêntimos por armazém; 0 tira a meta
    metas: BTreeMap<String, i64>,
}

async fn metas_guardar(st: &AppState, m: MetasIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    let validos: Vec<i64> = sqlx::query_scalar("SELECT id FROM armazens").fetch_all(&st.pool).await?;
    let mut limpo = BTreeMap::new();
    for (k, v) in m.metas {
        let id: i64 = k.parse().map_err(|_| AppError::msg("Loja inválida."))?;
        if !validos.contains(&id) {
            return Err(AppError::msg("Loja inválida."));
        }
        if !(0..=1_000_000_000_000).contains(&v) {
            return Err(AppError::msg("A meta tem de ser um valor entre 0 e 10 mil milhões de kwanzas."));
        }
        if v > 0 {
            limpo.insert(id.to_string(), v);
        }
    }
    let texto = serde_json::to_string(&limpo).unwrap_or_else(|_| "{}".into());
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(CHAVE_METAS)
        .bind(&texto)
        .execute(&st.pool)
        .await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'METAS_TV', ?)").bind(s.id).bind(&texto).execute(&st.pool).await?;
    Ok(json!({ "mensagem": "Metas guardadas." }))
}

#[cfg(test)]
mod testes {
    #[test]
    fn dias_do_mes() {
        assert_eq!(super::dias_do_mes("2026-11-22"), 30);
        assert_eq!(super::dias_do_mes("2026-12-01"), 31);
        assert_eq!(super::dias_do_mes("2028-02-10"), 29);
    }
}

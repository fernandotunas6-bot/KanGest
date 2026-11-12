use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// Vendas líquidas: facturas (FR/FT/ND) somam, notas de crédito subtraem.
const SINAL: &str = "CASE WHEN d.tipo_codigo = 'NC' THEN -1 ELSE 1 END";
const VENDAS: &str = "d.tipo_codigo IN ('FR','FT','ND','NC') AND d.estado <> 'A'";
// ?3 opcional: limita o relatório a uma entidade (cliente)
const ENT: &str = "AND (?3 IS NULL OR d.entidade_id = ?3)";

pub async fn dashboard(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let p = &st.pool;
    let hoje = db::hoje();
    let mes = format!("{}-01", &hoje[..7]);
    let (vendas_hoje, docs_hoje): (i64, i64) = sqlx::query_as(&format!(
        "SELECT COALESCE(SUM({SINAL} * d.total), 0), COUNT(*) FROM documentos d WHERE {VENDAS} AND d.data_emissao = ?"
    ))
    .bind(&hoje)
    .fetch_one(p)
    .await?;
    let (vendas_mes, docs_mes, iva_mes): (i64, i64, i64) = sqlx::query_as(&format!(
        "SELECT COALESCE(SUM({SINAL} * d.total), 0), COUNT(*), COALESCE(SUM({SINAL} * d.total_iva), 0)
         FROM documentos d WHERE {VENDAS} AND d.data_emissao >= ?"
    ))
    .bind(&mes)
    .fetch_one(p)
    .await?;
    let (a_receber, a_pagar): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN t.grupo = 'VENDAS' THEN d.valor_pendente END), 0),
                COALESCE(SUM(CASE WHEN t.grupo = 'COMPRAS' THEN d.valor_pendente END), 0)
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.estado = 'N'",
    )
    .fetch_one(p)
    .await?;
    let stock_baixo: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM artigos a LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?
         WHERE a.controla_stock = 1 AND a.ativo = 1 AND COALESCE(s.quantidade, 0) <= a.qtd_minima",
    )
    .bind(st.posto().armazem_id)
    .fetch_one(p)
    .await?;
    let serie = sqlx::query(&format!(
        "SELECT d.data_emissao AS dia, SUM({SINAL} * d.total) AS total, COUNT(*) AS docs
         FROM documentos d WHERE {VENDAS} AND d.data_emissao >= date(?, '-29 day')
         GROUP BY d.data_emissao ORDER BY d.data_emissao"
    ))
    .bind(&hoje)
    .fetch_all(p)
    .await?;
    let top = sqlx::query(&format!(
        "SELECT l.referencia, l.descricao, SUM({SINAL} * l.quantidade) AS quantidade, SUM({SINAL} * l.total) AS total
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE {VENDAS} AND d.data_emissao >= ? GROUP BY l.referencia, l.descricao ORDER BY total DESC LIMIT 8"
    ))
    .bind(&mes)
    .fetch_all(p)
    .await?;
    let ultimos = sqlx::query(
        "SELECT d.id, d.numero_doc, d.entidade_nome, d.total, d.data_hora, d.estado FROM documentos d
         JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE t.classe IN ('VENDA','RECIBO')
         ORDER BY d.data_hora DESC, d.id DESC LIMIT 8",
    )
    .fetch_all(p)
    .await?;
    Ok(json!({
        "vendas_hoje": vendas_hoje, "docs_hoje": docs_hoje,
        "vendas_mes": vendas_mes, "docs_mes": docs_mes, "iva_mes": iva_mes,
        "ticket_medio": if docs_mes > 0 { vendas_mes / docs_mes } else { 0 },
        "a_receber": a_receber, "a_pagar": a_pagar, "stock_baixo": stock_baixo,
        "serie": db::rows_json(&serie), "top_artigos": db::rows_json(&top), "ultimos": db::rows_json(&ultimos),
    }))
}

#[derive(Deserialize)]
pub struct VendasIn {
    de: String,
    ate: String,
    /// dia | artigo | familia | operador | vendedor | posto | pagamento | cliente | artigo_operador | artigo_entidade | documento | iva
    agrupar: String,
    #[serde(default)]
    entidade_id: Option<i64>,
}

pub async fn vendas(st: &AppState, f: VendasIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let taxa_c = taxa_grupo_c(&st.pool).await?;
    let sql = match f.agrupar.as_str() {
        // IRT Grupo C: imposto sobre o volume de vendas (sem IVA) de cada mês, à taxa configurada
        "irt_c" => format!(
            "SELECT substr(d.data_emissao, 1, 7) AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total,
                    CAST(ROUND(SUM({SINAL}*d.total_liquido) * {taxa_c} / 100.0) AS INTEGER) AS irt
             FROM documentos d WHERE {VENDAS} AND d.importado = 0 AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY 1"
        ),
        // total por mês (AAAA-MM), com o histórico importado do KIGEST
        "mes" => format!(
            "SELECT substr(d.data_emissao, 1, 7) AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY 1"
        ),
        "dia" => format!(
            "SELECT d.data_emissao AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY 1"
        ),
        "artigo" => format!(
            "SELECT l.referencia || ' — ' || l.descricao AS chave, SUM({SINAL}*l.quantidade) AS quantidade,
                    SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total,
                    SUM({SINAL}*(l.valor_liquido - CAST(ROUND(l.custo * l.quantidade) AS INTEGER))) AS lucro
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY l.referencia, l.descricao ORDER BY total DESC"
        ),
        "familia" => format!(
            "SELECT COALESCE(f.descricao, '(sem família)') AS chave, SUM({SINAL}*l.quantidade) AS quantidade,
                    SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             LEFT JOIN artigos a ON a.id = l.artigo_id LEFT JOIN familias f ON f.id = a.familia_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY total DESC"
        ),
        "operador" => format!(
            "SELECT u.nome AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d JOIN utilizadores u ON u.id = d.utilizador_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY u.nome ORDER BY total DESC"
        ),
        "cliente" => format!(
            "SELECT d.entidade_nome || ' (' || d.entidade_nif || ')' AS chave, COUNT(*) AS docs,
                    SUM({SINAL}*d.total_liquido) AS liquido, SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY total DESC"
        ),
        "pagamento" => format!(
            "SELECT m.descricao AS chave, COUNT(DISTINCT d.id) AS docs, SUM(p.valor) AS total
             FROM documento_pagamentos p JOIN documentos d ON d.id = p.documento_id
             JOIN modos_pagamento m ON m.codigo = p.modo_codigo
             WHERE d.estado <> 'A' AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY m.descricao ORDER BY total DESC"
        ),
        // total por vendedor (funcionário escolhido no documento)
        "vendedor" => format!(
            "SELECT COALESCE(f.nome, '(sem vendedor)') AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d LEFT JOIN funcionarios f ON f.id = d.vendedor_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY total DESC"
        ),
        // vendas por ponto de venda (posto)
        "posto" => format!(
            "SELECT COALESCE(ps.descricao, 'Sem posto') AS chave, COUNT(*) AS docs, SUM({SINAL}*d.total_liquido) AS liquido,
                    SUM({SINAL}*d.total_iva) AS iva, SUM({SINAL}*d.total) AS total
             FROM documentos d LEFT JOIN postos ps ON ps.id = d.posto_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY 1 ORDER BY total DESC"
        ),
        // artigos por operador: o que cada vendedor vendeu
        "artigo_operador" => format!(
            "SELECT u.nome || ' · ' || l.descricao AS chave, SUM({SINAL}*l.quantidade) AS quantidade,
                    SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN utilizadores u ON u.id = d.utilizador_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT}
             GROUP BY u.nome, l.descricao ORDER BY u.nome, total DESC"
        ),
        // artigos por entidade: o que cada cliente comprou
        "artigo_entidade" => format!(
            "SELECT d.entidade_nome || ' · ' || l.descricao AS chave, SUM({SINAL}*l.quantidade) AS quantidade,
                    SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT}
             GROUP BY d.entidade_nome, l.descricao ORDER BY d.entidade_nome, total DESC"
        ),
        // detalhe documento a documento (quantidades, base, IVA e total de cada um)
        "documento" => format!(
            "SELECT d.data_emissao || '  ' || d.numero_doc || ' · ' || d.entidade_nome || ' · ' || u.nome AS chave,
                    SUM({SINAL}*l.quantidade) AS quantidade, SUM({SINAL}*l.valor_liquido) AS liquido,
                    SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN utilizadores u ON u.id = d.utilizador_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT}
             GROUP BY d.id ORDER BY d.data_emissao, d.id"
        ),
        "iva" => format!(
            "SELECT 'IVA ' || l.taxa_iva_pct || '%' || COALESCE(' — ' || l.motivo_isencao, '') AS chave,
                    SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT} GROUP BY l.taxa_iva_pct, l.motivo_isencao ORDER BY l.taxa_iva_pct DESC"
        ),
        // mapa de imposto por dia (uma linha por dia e taxa)
        "iva_dia" => format!(
            "SELECT d.data_emissao || '  ·  IVA ' || l.taxa_iva_pct || '%' || COALESCE(' — ' || l.motivo_isencao, '') AS chave,
                    COUNT(DISTINCT d.id) AS docs, SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT}
             GROUP BY d.data_emissao, l.taxa_iva_pct, l.motivo_isencao ORDER BY d.data_emissao, l.taxa_iva_pct DESC"
        ),
        // mapa de imposto por família (uma linha por família e taxa)
        "iva_familia" => format!(
            "SELECT COALESCE(f.descricao, '(sem família)') || '  ·  IVA ' || l.taxa_iva_pct || '%' || COALESCE(' — ' || l.motivo_isencao, '') AS chave,
                    SUM({SINAL}*l.quantidade) AS quantidade, SUM({SINAL}*l.valor_liquido) AS liquido, SUM({SINAL}*l.valor_iva) AS iva, SUM({SINAL}*l.total) AS total
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             LEFT JOIN artigos a ON a.id = l.artigo_id LEFT JOIN familias f ON f.id = a.familia_id
             WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2 {ENT}
             GROUP BY f.descricao, l.taxa_iva_pct, l.motivo_isencao ORDER BY f.descricao, l.taxa_iva_pct DESC"
        ),
        _ => return Err(AppError::msg("Agrupamento inválido.")),
    };
    let rows = sqlx::query(&sql).bind(&f.de).bind(&f.ate).bind(f.entidade_id).fetch_all(&st.pool).await?;
    Ok(db::rows_json(&rows))
}

// ============================== relatório síntese ==============================

#[derive(Deserialize)]
pub struct SinteseIn {
    ano: i32,
}

/// Relatório síntese: vendas de cada mês de um ano ao lado do ano anterior (como o «Relatório Síntese»
/// da livraria), com o histórico importado do KIGEST. O mês corrente vai assinalado (valor parcial).
pub async fn sintese(st: &AppState, s: SinteseIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    if !(1990..=2100).contains(&s.ano) {
        return Err(AppError::msg("Ano inválido."));
    }
    let rows: Vec<(i64, i64, i64, i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT CAST(substr(d.data_emissao, 1, 4) AS INTEGER), CAST(substr(d.data_emissao, 6, 2) AS INTEGER), COUNT(*),
                COALESCE(SUM({SINAL}*d.total), 0), COALESCE(SUM({SINAL}*d.total_liquido), 0),
                COALESCE(SUM(CASE WHEN d.importado = 1 THEN {SINAL}*d.total ELSE 0 END), 0)
         FROM documentos d WHERE {VENDAS} AND d.data_emissao >= ?1 AND d.data_emissao < ?2 GROUP BY 1, 2"
    ))
    .bind(format!("{}-01-01", s.ano - 1))
    .bind(format!("{}-01-01", s.ano + 1))
    .fetch_all(&st.pool)
    .await?;
    let valor = |ano: i32, mes: i64| rows.iter().find(|r| r.0 == ano as i64 && r.1 == mes);
    let meses: Vec<Value> = (1..=12)
        .map(|m| {
            let a = valor(s.ano, m);
            let b = valor(s.ano - 1, m);
            json!({
                "mes": m,
                "atual": a.map(|r| r.3), "docs_atual": a.map(|r| r.2).unwrap_or(0), "liquido_atual": a.map(|r| r.4),
                "anterior": b.map(|r| r.3), "docs_anterior": b.map(|r| r.2).unwrap_or(0), "liquido_anterior": b.map(|r| r.4),
            })
        })
        .collect();
    let historico: i64 = rows.iter().map(|r| r.5).sum();
    let anos: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT DISTINCT CAST(substr(d.data_emissao, 1, 4) AS INTEGER) FROM documentos d WHERE {VENDAS} ORDER BY 1 DESC"
    ))
    .fetch_all(&st.pool)
    .await?;
    let hoje = db::hoje();
    let mes_em_curso = (hoje[..4] == s.ano.to_string()).then(|| hoje[5..7].parse::<i64>().unwrap_or(0));
    Ok(json!({ "ano": s.ano, "anterior": s.ano - 1, "meses": meses, "anos": anos, "mes_em_curso": mes_em_curso, "historico_kigest": historico }))
}

// ============================== stock ==============================

#[derive(Deserialize)]
pub struct StockIn {
    armazem_id: Option<i64>,
    /// ruptura | minimo | excesso | validade | parado
    situacao: String,
    /// validade: dias até expirar; parado: dias sem saída
    #[serde(default)]
    dias: Option<i64>,
}

/// Relatórios de stock do GC: ruptura, abaixo do mínimo, excesso, validade e artigos parados.
pub async fn stock(st: &AppState, f: StockIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let armazem = f.armazem_id.unwrap_or(st.posto().armazem_id);
    let dias = f.dias.unwrap_or(30);

    // Validade olha para os lotes, não para o saldo do artigo.
    if f.situacao == "validade" {
        let rows = sqlx::query(
            "SELECT a.referencia, a.descricao, ar.nome AS armazem, l.lote, l.validade, l.quantidade,
                    CAST(julianday(l.validade) - julianday(date('now','localtime')) AS INTEGER) AS dias_validade,
                    CAST(ROUND(l.quantidade * l.custo) AS INTEGER) AS valor_custo
             FROM lotes l JOIN artigos a ON a.id = l.artigo_id JOIN armazens ar ON ar.id = l.armazem_id
             WHERE l.armazem_id = ?1 AND l.quantidade > 0 AND l.validade IS NOT NULL
               AND julianday(l.validade) - julianday(date('now','localtime')) <= ?2
             ORDER BY l.validade, a.descricao",
        )
        .bind(armazem)
        .bind(dias)
        .fetch_all(&st.pool)
        .await?;
        return Ok(db::rows_json(&rows));
    }

    // Lista de preços (ListaPreco do KIGEST): todos os artigos activos, por família e subfamília
    if f.situacao == "precos" {
        let pvp = |n: u8| format!("CAST(ROUND(a.preco{n} * (1 + t.percentagem / 100.0)) AS INTEGER) AS pvp{n}");
        let rows = sqlx::query(&format!(
            "SELECT a.referencia, a.descricao, u.abreviatura AS unidade, f.descricao AS familia, sf.descricao AS subfamilia,
                    a.custo, t.percentagem AS taxa_pct, a.preco1, {}, {}, {}, {},
                    COALESCE(s.quantidade, 0) AS quantidade, 0 AS valor_custo,
                    CASE WHEN a.promo_desconto > 0 AND COALESCE(a.promo_inicio, '') <= date('now', 'localtime')
                              AND (a.promo_fim IS NULL OR a.promo_fim = '' OR a.promo_fim >= date('now', 'localtime'))
                         THEN a.promo_desconto ELSE 0 END AS promo_atual
             FROM artigos a
             JOIN taxas_iva t ON t.id = a.taxa_iva_id
             LEFT JOIN unidades u ON u.id = a.unidade_id
             LEFT JOIN familias f ON f.id = a.familia_id
             LEFT JOIN subfamilias sf ON sf.id = a.subfamilia_id
             LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1
             WHERE a.ativo = 1
             ORDER BY COALESCE(f.descricao, 'zzz'), COALESCE(sf.descricao, ''), a.descricao",
            pvp(1),
            pvp(2),
            pvp(3),
            pvp(4)
        ))
        .bind(armazem)
        .fetch_all(&st.pool)
        .await?;
        return Ok(db::rows_json(&rows));
    }
    // Lista de stock do armazém (ListaStock / ListaStockArmazem do KIGEST): todos os artigos com controlo de stock
    if f.situacao == "existencias" {
        let rows = sqlx::query(
            "SELECT a.referencia, a.descricao, u.abreviatura AS unidade, f.descricao AS familia,
                    COALESCE(s.quantidade, 0) AS quantidade, a.custo_medio, a.qtd_minima,
                    CAST(ROUND(COALESCE(s.quantidade, 0) * a.custo_medio) AS INTEGER) AS valor_custo
             FROM artigos a
             LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1
             LEFT JOIN unidades u ON u.id = a.unidade_id
             LEFT JOIN familias f ON f.id = a.familia_id
             WHERE a.ativo = 1 AND a.controla_stock = 1
             ORDER BY COALESCE(f.descricao, 'zzz'), a.descricao",
        )
        .bind(armazem)
        .fetch_all(&st.pool)
        .await?;
        return Ok(db::rows_json(&rows));
    }
    // Previsão de venda (PrevisaoVenda do KIGEST): o stock existente ao custo e ao preço de venda (PVP 1 s/ IVA)
    if f.situacao == "previsao" {
        let rows = sqlx::query(
            "SELECT a.referencia, a.descricao, u.abreviatura AS unidade, f.descricao AS familia,
                    s.quantidade, a.custo_medio, a.preco1,
                    CAST(ROUND(s.quantidade * a.custo_medio) AS INTEGER) AS valor_custo,
                    CAST(ROUND(s.quantidade * a.preco1) AS INTEGER) AS valor_venda,
                    CAST(ROUND(s.quantidade * a.preco1) AS INTEGER) - CAST(ROUND(s.quantidade * a.custo_medio) AS INTEGER) AS lucro
             FROM artigos a
             JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1
             LEFT JOIN unidades u ON u.id = a.unidade_id
             LEFT JOIN familias f ON f.id = a.familia_id
             WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade > 0
             ORDER BY COALESCE(f.descricao, 'zzz'), a.descricao",
        )
        .bind(armazem)
        .fetch_all(&st.pool)
        .await?;
        return Ok(db::rows_json(&rows));
    }

    let condicao = match f.situacao.as_str() {
        "ruptura" => "COALESCE(s.quantidade, 0) <= 0",
        "minimo" => "COALESCE(s.quantidade, 0) > 0 AND COALESCE(s.quantidade, 0) <= a.qtd_minima",
        "excesso" => "a.qtd_maxima > 0 AND COALESCE(s.quantidade, 0) > a.qtd_maxima",
        "parado" => "COALESCE(s.quantidade, 0) > 0 AND (s.ultima_saida IS NULL OR julianday(date('now','localtime')) - julianday(s.ultima_saida) >= ?2)",
        _ => return Err(AppError::msg("Situação de stock inválida.")),
    };
    let rows = sqlx::query(&format!(
        "SELECT a.referencia, a.descricao, u.abreviatura AS unidade, ar.nome AS armazem,
                COALESCE(s.quantidade, 0) AS quantidade, a.qtd_minima, a.qtd_maxima, a.custo_medio, a.preco1,
                CAST(ROUND(COALESCE(s.quantidade, 0) * a.custo_medio) AS INTEGER) AS valor_custo,
                s.ultima_entrada, s.ultima_saida,
                CAST(julianday(date('now','localtime')) - julianday(s.ultima_saida) AS INTEGER) AS dias_sem_saida
         FROM artigos a
         LEFT JOIN unidades u ON u.id = a.unidade_id
         LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1
         LEFT JOIN armazens ar ON ar.id = ?1
         WHERE a.controla_stock = 1 AND a.ativo = 1 AND ({condicao})
         ORDER BY a.descricao"
    ))
    .bind(armazem)
    .bind(dias)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

// ============================== rastreabilidade de lotes ==============================

#[derive(Deserialize)]
pub struct RastreioIn {
    #[serde(default)]
    lote: Option<String>,
    #[serde(default)]
    artigo_id: Option<i64>,
}

/// Rastreio de lotes: de quem veio cada lote e para quem foi, documento a documento.
pub async fn rastreio(st: &AppState, f: RastreioIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let lote = f.lote.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
    if lote.is_none() && f.artigo_id.is_none() {
        return Err(AppError::msg("Indique o lote ou o artigo a rastrear."));
    }
    let lotes = sqlx::query(
        "SELECT l.id, l.lote, l.data_fabrico, l.validade, l.quantidade AS saldo, l.custo,
                a.id AS artigo_id, a.referencia, a.descricao, ar.nome AS armazem
         FROM lotes l JOIN artigos a ON a.id = l.artigo_id JOIN armazens ar ON ar.id = l.armazem_id
         WHERE (?1 IS NULL OR l.lote LIKE ?1) AND (?2 IS NULL OR l.artigo_id = ?2)
         ORDER BY a.descricao, l.validade IS NULL, l.validade",
    )
    .bind(&lote)
    .bind(f.artigo_id)
    .fetch_all(&st.pool)
    .await?;

    let mut saida = Vec::with_capacity(lotes.len());
    for l in &lotes {
        let id: i64 = sqlx::Row::get(l, "id");
        // Os movimentos de lote guardam o documento; a entidade diz de quem veio ou para quem foi.
        let movs = sqlx::query(
            "SELECT d.id, d.numero_doc, d.data_emissao, d.entidade_nome, d.entidade_nif, d.estado,
                    t.descricao AS tipo, t.grupo, m.quantidade
             FROM lote_movimentos m JOIN documentos d ON d.id = m.documento_id
             JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             WHERE m.lote_id = ?1 ORDER BY d.data_emissao, d.id",
        )
        .bind(id)
        .fetch_all(&st.pool)
        .await?;
        let mut obj = db::row_json(l);
        obj["movimentos"] = db::rows_json(&movs);
        saida.push(obj);
    }
    Ok(Value::Array(saida))
}

// ============================== mapa de fornecedor ==============================

#[derive(Deserialize)]
pub struct FornecedorIn {
    de: String,
    ate: String,
    /// Com fornecedor escolhido, devolve também os documentos um a um.
    #[serde(default)]
    entidade_id: Option<i64>,
}

/// Mapa de fornecedor: compras, pagamentos, saldo em dívida e (por fornecedor) os documentos.
pub async fn fornecedores(st: &AppState, f: FornecedorIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let resumo = sqlx::query(
        "SELECT e.id, e.nome, e.nif, e.telefone,
                SUM(CASE WHEN t.classe = 'COMPRA' THEN (CASE WHEN t.credito = 1 THEN -d.total ELSE d.total END) ELSE 0 END) AS compras,
                SUM(CASE WHEN t.classe = 'COMPRA' THEN (CASE WHEN t.credito = 1 THEN -d.total_iva ELSE d.total_iva END) ELSE 0 END) AS iva,
                SUM(CASE WHEN t.classe = 'PAGAMENTO' THEN d.total ELSE 0 END) AS pagamentos,
                COUNT(CASE WHEN t.classe = 'COMPRA' THEN 1 END) AS docs
         FROM documentos d
         JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         JOIN entidades e ON e.id = d.entidade_id
         WHERE t.grupo = 'COMPRAS' AND d.estado <> 'A' AND d.data_emissao BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR e.id = ?3)
         GROUP BY e.id HAVING compras <> 0 OR pagamentos <> 0
         ORDER BY compras DESC",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .bind(f.entidade_id)
    .fetch_all(&st.pool)
    .await?;

    // Dívida actual (toda, não só do período) para fechar o mapa com o saldo real.
    let pendente = sqlx::query(
        "SELECT d.entidade_id AS id, SUM(d.valor_pendente) AS pendente
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE t.grupo = 'COMPRAS' AND d.estado = 'N' AND d.valor_pendente > 0 AND (?1 IS NULL OR d.entidade_id = ?1)
         GROUP BY d.entidade_id",
    )
    .bind(f.entidade_id)
    .fetch_all(&st.pool)
    .await?;

    let documentos = match f.entidade_id {
        None => Value::Array(vec![]),
        Some(id) => {
            let rows = sqlx::query(
                "SELECT d.id, d.numero_doc, d.doc_externo, d.data_emissao, d.data_vencimento, t.descricao AS tipo, t.classe,
                        d.total, d.total_iva, d.valor_pendente, d.estado, d.moeda, d.cambio
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE t.grupo = 'COMPRAS' AND d.entidade_id = ?1 AND d.data_emissao BETWEEN ?2 AND ?3
                 ORDER BY d.data_emissao, d.id",
            )
            .bind(id)
            .bind(&f.de)
            .bind(&f.ate)
            .fetch_all(&st.pool)
            .await?;
            db::rows_json(&rows)
        }
    };
    Ok(json!({ "resumo": db::rows_json(&resumo), "pendentes": db::rows_json(&pendente), "documentos": documentos }))
}


/// Taxa do IRT Grupo C sobre o volume de vendas (configurável; 6,5% por defeito).
pub async fn taxa_grupo_c(p: &sqlx::SqlitePool) -> AppResult<f64> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'irt.grupo_c_taxa'").fetch_optional(p).await?;
    Ok(v.and_then(|x| x.parse::<f64>().ok()).filter(|t| (0.0..=100.0).contains(t)).unwrap_or(6.5))
}

#[derive(Deserialize)]
pub struct TaxaIn {
    taxa: f64,
}

pub async fn irt_c_obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    Ok(json!({ "taxa": taxa_grupo_c(&st.pool).await? }))
}

pub async fn irt_c_guardar(st: &AppState, t: TaxaIn) -> AppResult<Value> {
    st.exigir_perm("sistema.parametrizacao")?;
    if !(0.0..=100.0).contains(&t.taxa) {
        return Err(AppError::msg("Taxa inválida."));
    }
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('irt.grupo_c_taxa', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(t.taxa.to_string())
        .execute(&st.pool)
        .await?;
    Ok(Value::Null)
}

//! Sugestão de encomendas a fornecedores e exportação de lançamentos para a contabilidade.
//!
//! - **Sugestão de encomendas** (como as regras de reposição do Odoo): artigos abaixo do mínimo, ou
//!   cujo stock não cobre os próximos dias ao ritmo de venda dos últimos N dias, com a quantidade a
//!   encomendar (até ao máximo, ou para cobrir esses dias), descontando o que já está encomendado,
//!   agrupados pelo fornecedor habitual (o da última compra).
//! - **Lançamentos contabilísticos**: diário dos documentos do período (vendas, notas de crédito e de
//!   débito, recibos, compras e pagamentos), com débito e crédito por conta do PGC angolano. As contas
//!   configuram-se (`contabilidade.contas`); o ficheiro serve para o programa do contabilista.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ------------------------------------------------------------------ sugestão de encomendas

#[derive(Deserialize)]
pub struct SugestaoIn {
    /// Dias de vendas para medir a rotação (por omissão 30).
    #[serde(default)]
    dias: Option<i64>,
    /// Dias que o stock deve cobrir (por omissão 15).
    #[serde(default)]
    cobertura: Option<i64>,
    #[serde(default)]
    armazem_id: Option<i64>,
}

pub async fn sugestao(st: &AppState, i: SugestaoIn) -> AppResult<Value> {
    st.exigir_perm("compras.fatura")?;
    let dias = i.dias.unwrap_or(30).clamp(1, 365);
    let cobertura = i.cobertura.unwrap_or(15).clamp(1, 365);
    let desde = (chrono::Local::now().date_naive() - chrono::Duration::days(dias)).format("%Y-%m-%d").to_string();
    let rows = sqlx::query(
        "WITH vendido AS (
             SELECT l.artigo_id, SUM(CASE WHEN t.credito = 1 THEN -l.quantidade ELSE l.quantidade END) AS q
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             WHERE t.grupo = 'VENDAS' AND t.stock <> 0 AND d.estado <> 'A' AND d.data_emissao >= ?1
               AND (?2 IS NULL OR d.armazem_id = ?2)
             GROUP BY l.artigo_id),
         encomendado AS (
             SELECT l.artigo_id, SUM(l.quantidade) AS q FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE d.tipo_codigo = 'VNE' AND d.estado = 'N' AND (?2 IS NULL OR d.armazem_id = ?2) GROUP BY l.artigo_id),
         existencias AS (
             SELECT artigo_id, SUM(quantidade) AS q FROM stock WHERE (?2 IS NULL OR armazem_id = ?2) GROUP BY artigo_id),
         ultima_compra AS (
             SELECT l.artigo_id, d.entidade_id, d.entidade_nome, l.preco_unitario,
                    ROW_NUMBER() OVER (PARTITION BY l.artigo_id ORDER BY d.data_emissao DESC, d.id DESC) AS n
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             WHERE t.grupo = 'COMPRAS' AND t.classe = 'COMPRA' AND t.credito = 0 AND d.estado <> 'A')
         SELECT a.id, a.referencia, a.descricao, a.qtd_minima, a.qtd_maxima, a.custo,
                COALESCE(e.q, 0.0) AS stock, COALESCE(v.q, 0.0) AS vendido, COALESCE(o.q, 0.0) AS encomendado,
                u.entidade_id AS fornecedor_id, u.entidade_nome AS fornecedor, u.preco_unitario AS ultimo_custo
         FROM artigos a
         LEFT JOIN existencias e ON e.artigo_id = a.id
         LEFT JOIN vendido v ON v.artigo_id = a.id
         LEFT JOIN encomendado o ON o.artigo_id = a.id
         LEFT JOIN ultima_compra u ON u.artigo_id = a.id AND u.n = 1
         WHERE a.ativo = 1 AND a.controla_stock = 1 AND a.tipo = 'P' AND a.composto = 0",
    )
    .bind(&desde)
    .bind(i.armazem_id)
    .fetch_all(&st.pool)
    .await?;
    let mut linhas = Vec::new();
    for r in rows {
        let (stock, vendido, encomendado): (f64, f64, f64) = (r.get("stock"), r.get("vendido"), r.get("encomendado"));
        let (minimo, maximo): (f64, f64) = (r.get("qtd_minima"), r.get("qtd_maxima"));
        let media = (vendido / dias as f64).max(0.0);
        let disponivel = stock + encomendado;
        let abaixo_minimo = minimo > 0.0 && disponivel <= minimo;
        let necessidade = media * cobertura as f64;
        let nao_cobre = media > 0.0 && disponivel < necessidade;
        if !(abaixo_minimo || nao_cobre) {
            continue;
        }
        // até ao máximo (se definido), senão para cobrir os dias pedidos (e pelo menos o mínimo)
        let alvo = if maximo > 0.0 { maximo } else { necessidade.max(minimo) };
        let sugerido = (alvo - disponivel).ceil();
        if sugerido <= 0.0 {
            continue;
        }
        let custo = r.get::<Option<i64>, _>("ultimo_custo").unwrap_or_else(|| r.get("custo"));
        linhas.push(json!({
            "artigo_id": r.get::<i64, _>("id"), "referencia": r.get::<String, _>("referencia"), "descricao": r.get::<String, _>("descricao"),
            "stock": stock, "encomendado": encomendado, "minimo": minimo, "maximo": maximo,
            "media_diaria": (media * 1000.0).round() / 1000.0, "dias_cobertura": if media > 0.0 { Some((stock / media).floor()) } else { None },
            "sugerido": sugerido, "custo": custo, "valor": (sugerido * custo as f64).round() as i64,
            "fornecedor_id": r.get::<Option<i64>, _>("fornecedor_id"), "fornecedor": r.get::<Option<String>, _>("fornecedor"),
            "motivo": if abaixo_minimo { "abaixo do mínimo" } else { "não cobre os dias pedidos" },
        }));
    }
    linhas.sort_by(|a, b| {
        a["fornecedor"].as_str().unwrap_or("~").cmp(b["fornecedor"].as_str().unwrap_or("~")).then(a["descricao"].as_str().cmp(&b["descricao"].as_str()))
    });
    Ok(json!({ "dias": dias, "cobertura": cobertura, "linhas": linhas }))
}

// ------------------------------------------------------------------ lançamentos contabilísticos

/// Contas por omissão (PGC angolano); a empresa pode mudá-las em `contabilidade.contas`.
fn contas_padrao() -> BTreeMap<String, String> {
    [
        ("clientes", "31.1.1"),
        ("fornecedores", "32.1.1"),
        ("caixa", "45.1"),
        ("bancos", "43.1"),
        ("vendas_produtos", "61.1"),
        ("vendas_servicos", "62.1"),
        ("compras", "21.1"),
        ("iva_liquidado", "34.5.3"),
        ("iva_dedutivel", "34.5.2"),
        ("retencoes_clientes", "34.1"),
        ("retencoes_fornecedores", "34.3"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

async fn contas(st: &AppState) -> AppResult<BTreeMap<String, String>> {
    let mut c = contas_padrao();
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'contabilidade.contas'").fetch_optional(&st.pool).await?;
    if let Some(m) = v.and_then(|t| serde_json::from_str::<BTreeMap<String, String>>(&t).ok()) {
        for (k, v) in m.into_iter().filter(|(_, v)| !v.trim().is_empty()) {
            c.insert(k, v.trim().to_string());
        }
    }
    Ok(c)
}

pub async fn contas_obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    Ok(json!({ "contas": contas(st).await?, "padrao": contas_padrao() }))
}

pub async fn contas_guardar(st: &AppState, v: BTreeMap<String, String>) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let validas: BTreeMap<String, String> = v.into_iter().filter(|(k, _)| contas_padrao().contains_key(k)).collect();
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('contabilidade.contas', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(serde_json::to_string(&validas)?)
        .execute(&st.pool)
        .await?;
    contas_obter(st).await
}

#[derive(Deserialize)]
pub struct LancamentosIn {
    de: String,
    ate: String,
}

/// Diário do período: uma linha por conta movimentada em cada documento.
pub async fn lancamentos(st: &AppState, i: LancamentosIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let valida = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok();
    if !valida(&i.de) || !valida(&i.ate) || i.de > i.ate {
        return Err(AppError::msg("Período inválido."));
    }
    let c = contas(st).await?;
    let docs = sqlx::query(
        "SELECT d.id, d.numero_doc, d.data_emissao, d.tipo_codigo, d.entidade_nome, d.entidade_nif, d.total, d.total_liquido, d.total_iva,
                d.total_retencao, t.grupo, t.classe, t.credito, t.pago_no_acto,
                COALESCE((SELECT SUM(l.valor_liquido) FROM documento_linhas l JOIN artigos a ON a.id = l.artigo_id
                          WHERE l.documento_id = d.id AND a.tipo = 'S'), 0) AS servicos
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.data_emissao BETWEEN ?1 AND ?2 AND d.estado <> 'A' AND d.importado = 0
           AND t.classe IN ('VENDA', 'RECIBO', 'COMPRA', 'PAGAMENTO')
         ORDER BY d.data_emissao, d.id",
    )
    .bind(&i.de)
    .bind(&i.ate)
    .fetch_all(&st.pool)
    .await?;
    let mut linhas: Vec<Value> = Vec::new();
    let mut por_conta: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let (mut debito_total, mut credito_total) = (0i64, 0i64);
    for d in docs {
        let id: i64 = d.get("id");
        let numero: String = d.get("numero_doc");
        let data: String = d.get("data_emissao");
        let nome: String = d.get("entidade_nome");
        let nif: String = d.get("entidade_nif");
        let (grupo, classe): (String, String) = (d.get("grupo"), d.get("classe"));
        let credito = d.get::<i64, _>("credito") == 1;
        let pago = d.get::<i64, _>("pago_no_acto") == 1;
        let (total, liquido, iva, retencao, servicos): (i64, i64, i64, i64, i64) =
            (d.get("total"), d.get("total_liquido"), d.get("total_iva"), d.get("total_retencao"), d.get("servicos"));
        // meios de pagamento do documento (para as vendas a pronto, recibos e pagamentos)
        let pagamentos: Vec<(String, i64)> = sqlx::query_as("SELECT modo_codigo, valor FROM documento_pagamentos WHERE documento_id = ?").bind(id).fetch_all(&st.pool).await?;
        let conta_meio = |modo: &str| if modo == "NU" { c["caixa"].clone() } else { c["bancos"].clone() };
        let mut mov: Vec<(String, i64, i64, String)> = Vec::new(); // conta, débito, crédito, descrição
        match (grupo.as_str(), classe.as_str()) {
            ("VENDAS", "VENDA") => {
                let (produtos, serv) = (liquido - servicos, servicos);
                // a crédito ou a pronto: o cliente (ou o meio de pagamento) contra vendas e IVA liquidado;
                // nas notas de crédito os lados trocam
                let deb = |v: i64| if credito { (0, v) } else { (v, 0) };
                let contrapartida: Vec<(String, i64)> = if pago && !credito {
                    pagamentos.iter().map(|(m, v)| (conta_meio(m), *v)).collect()
                } else {
                    vec![(c["clientes"].clone(), total - retencao)]
                };
                for (conta, v) in contrapartida {
                    let (d1, c1) = deb(v);
                    mov.push((conta, d1, c1, nome.clone()));
                }
                if retencao > 0 {
                    let (d1, c1) = deb(retencao);
                    mov.push((c["retencoes_clientes"].clone(), d1, c1, "Retenção na fonte".into()));
                }
                for (conta, v, desc) in [(&c["vendas_produtos"], produtos, "Vendas"), (&c["vendas_servicos"], serv, "Prestações de serviços"), (&c["iva_liquidado"], iva, "IVA liquidado")] {
                    if v != 0 {
                        let (d1, c1) = deb(v);
                        mov.push((conta.clone(), c1, d1, desc.into()));
                    }
                }
            }
            ("VENDAS", "RECIBO") => {
                // a compensação de saldo (CS) não é dinheiro: abate a crédito do próprio cliente
                for (m, v) in pagamentos.iter().filter(|p| p.0 != "CS") {
                    mov.push((conta_meio(m), *v, 0, nome.clone()));
                }
                let cs: i64 = pagamentos.iter().filter(|p| p.0 == "CS").map(|p| p.1).sum();
                mov.push((c["clientes"].clone(), 0, total - cs, nome.clone()));
            }
            ("COMPRAS", "COMPRA") => {
                let deb = |v: i64| if credito { (0, v) } else { (v, 0) };
                for (conta, v, desc) in [(&c["compras"], liquido, "Compras"), (&c["iva_dedutivel"], iva, "IVA dedutível")] {
                    if v != 0 {
                        let (d1, c1) = deb(v);
                        mov.push((conta.clone(), d1, c1, desc.into()));
                    }
                }
                if retencao > 0 {
                    let (d1, c1) = deb(retencao);
                    mov.push((c["retencoes_fornecedores"].clone(), c1, d1, "Retenção na fonte".into()));
                }
                let (d1, c1) = deb(total - retencao);
                mov.push((c["fornecedores"].clone(), c1, d1, nome.clone()));
            }
            ("COMPRAS", "PAGAMENTO") => {
                let cs: i64 = pagamentos.iter().filter(|p| p.0 == "CS").map(|p| p.1).sum();
                mov.push((c["fornecedores"].clone(), total - cs, 0, nome.clone()));
                for (m, v) in pagamentos.iter().filter(|p| p.0 != "CS") {
                    mov.push((conta_meio(m), 0, *v, nome.clone()));
                }
            }
            _ => {}
        }
        for (conta, deb, cred, desc) in mov.into_iter().filter(|m| m.1 != 0 || m.2 != 0) {
            debito_total += deb;
            credito_total += cred;
            let e = por_conta.entry(conta.clone()).or_insert((0, 0));
            e.0 += deb;
            e.1 += cred;
            linhas.push(json!({
                "data": data, "documento": numero, "diario": if grupo == "VENDAS" { "Vendas" } else { "Compras" },
                "conta": conta, "descricao": desc, "nif": nif, "debito": deb, "credito": cred,
            }));
        }
    }
    let balancete: Vec<Value> = por_conta.into_iter().map(|(conta, (d, c))| json!({ "conta": conta, "debito": d, "credito": c, "saldo": d - c })).collect();
    Ok(json!({
        "de": i.de, "ate": i.ate, "linhas": linhas, "balancete": balancete,
        "debito": debito_total, "credito": credito_total, "equilibrado": debito_total == credito_total,
        "gerado": db::agora(),
    }))
}

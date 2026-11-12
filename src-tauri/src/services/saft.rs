//! Exportação SAF-T (AO) — estrutura 1.01_01.
//! * Facturação (TaxAccountingBasis = F): Header, MasterFiles (Customer, Product, TaxTable) e
//!   SourceDocuments (SalesInvoices, MovementOfGoods, WorkingDocuments, Payments).
//! * Aquisição (TaxAccountingBasis = A): Header, MasterFiles (Supplier, TaxTable) e
//!   SourceDocuments/PurchaseInvoices.
//! Validar no validador da AGT antes de submeter.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use crate::agt::{self, total_fmt as m};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const VERSAO: &str = env!("CARGO_PKG_VERSION");

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

/// Elemento simples `<tag>valor</tag>` (valor escapado).
fn el(x: &mut String, ind: usize, tag: &str, val: &str) {
    let _ = writeln!(x, "{:ind$}<{tag}>{}</{tag}>", "", esc(val), ind = ind);
}

fn qtd(q: f64) -> String {
    let s = format!("{q:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Deserialize)]
pub struct SaftIn {
    de: String,
    ate: String,
}

struct Doc {
    id: i64,
    tipo: String,
    numero: String,
    estado: String,
    data: String,
    data_hora: String,
    entidade_id: i64,
    login: String,
    total_liquido: i64,
    total_iva: i64,
    total: i64,
    hash: String,
    doc_origem: Option<String>,
    observacoes: Option<String>,
    motivo_anulacao: Option<String>,
    motivo: Option<String>,
    transporte: Option<String>,
    /// SourceBilling / SourcePayment: P (produzido), I (integrado), M (manual / recuperação)
    fonte: String,
}

async fn docs(p: &SqlitePool, tipos: &str, de: &str, ate: &str, estado: bool) -> AppResult<Vec<Doc>> {
    // Documentos importados do KIGEST ficam de fora: pertencem ao SAF-T do sistema anterior.
    let sql = format!(
        "SELECT d.id, d.tipo_codigo, d.numero_doc, d.estado, d.data_emissao, d.data_hora, COALESCE(d.entidade_id, 1) AS ent,
                u.login, d.total_liquido, d.total_iva, d.total, d.hash, o.numero_doc AS origem, d.observacoes, d.motivo_anulacao,
                d.motivo, d.transporte, d.origem AS fonte
         FROM documentos d JOIN utilizadores u ON u.id = d.utilizador_id
         LEFT JOIN documentos o ON o.id = d.doc_origem_id
         LEFT JOIN entidades e ON e.id = d.entidade_id
         WHERE d.tipo_codigo IN ({tipos}) AND d.data_emissao BETWEEN ?1 AND ?2 AND d.importado = 0
           AND (?3 = 0 OR e.estado = 1)
         ORDER BY d.tipo_codigo, d.serie, d.numero"
    );
    let rows = sqlx::query(&sql).bind(de).bind(ate).bind(estado as i64).fetch_all(p).await?;
    Ok(rows
        .iter()
        .map(|r| Doc {
            id: r.get("id"),
            tipo: r.get("tipo_codigo"),
            numero: r.get("numero_doc"),
            estado: r.get("estado"),
            data: r.get("data_emissao"),
            data_hora: r.get("data_hora"),
            entidade_id: r.get("ent"),
            login: r.get("login"),
            total_liquido: r.get("total_liquido"),
            total_iva: r.get("total_iva"),
            total: r.get("total"),
            hash: r.get("hash"),
            doc_origem: r.get("origem"),
            observacoes: r.get("observacoes"),
            motivo_anulacao: r.get("motivo_anulacao"),
            motivo: r.get("motivo"),
            transporte: r.get("transporte"),
            fonte: r.get("fonte"),
        })
        .collect())
}

/// Retenções na fonte do documento (WithholdingTax).
async fn retencoes(x: &mut String, p: &SqlitePool, d: &Doc, ind: usize) -> AppResult<()> {
    for r in sqlx::query("SELECT codigo, descricao, valor FROM documento_retencoes WHERE documento_id = ? ORDER BY id")
        .bind(d.id)
        .fetch_all(p)
        .await?
    {
        let _ = writeln!(x, "{:ind$}<WithholdingTax>", "");
        el(x, ind + 2, "WithholdingTaxType", &r.get::<String, _>("codigo"));
        el(x, ind + 2, "WithholdingTaxDescription", &r.get::<String, _>("descricao"));
        el(x, ind + 2, "WithholdingTaxAmount", &m(r.get::<i64, _>("valor")));
        let _ = writeln!(x, "{:ind$}</WithholdingTax>", "");
    }
    Ok(())
}

fn estado_saft(d: &Doc) -> &'static str {
    match d.estado.as_str() {
        "A" => "A",
        "F" => "F",
        _ => "N",
    }
}

async fn linhas(x: &mut String, p: &SqlitePool, d: &Doc, ind: usize, credito: bool, movimento: bool) -> AppResult<()> {
    let rows = sqlx::query(
        "SELECT l.*, COALESCE(a.referencia, l.referencia) AS ref, COALESCE(t.codigo, 'NOR') AS taxa_codigo, mi.descricao AS motivo_desc
         FROM documento_linhas l
         LEFT JOIN artigos a ON a.id = l.artigo_id
         LEFT JOIN taxas_iva t ON t.id = l.taxa_iva_id
         LEFT JOIN motivos_isencao mi ON mi.codigo = l.motivo_isencao
         WHERE l.documento_id = ? ORDER BY l.linha",
    )
    .bind(d.id)
    .fetch_all(p)
    .await?;
    for l in rows {
        let qtd_l: f64 = l.get("quantidade");
        let liquido: i64 = l.get("valor_liquido");
        let unit = if qtd_l > 0.0 { liquido as f64 / 100.0 / qtd_l } else { 0.0 };
        let i = ind;
        let _ = writeln!(x, "{:i$}<Line>", "");
        el(x, i + 2, "LineNumber", &l.get::<i64, _>("linha").to_string());
        el(x, i + 2, "ProductCode", &l.get::<String, _>("ref"));
        el(x, i + 2, "ProductDescription", &l.get::<String, _>("descricao"));
        el(x, i + 2, "Quantity", &qtd(qtd_l));
        el(x, i + 2, "UnitOfMeasure", &l.get::<Option<String>, _>("unidade").unwrap_or_else(|| "UN".into()));
        el(x, i + 2, "UnitPrice", &format!("{unit:.6}"));
        if !movimento {
            el(x, i + 2, "TaxPointDate", &d.data);
        }
        if d.tipo == "NC" {
            if let Some(o) = &d.doc_origem {
                let _ = writeln!(x, "{:w$}<References>", "", w = i + 2);
                el(x, i + 4, "Reference", o);
                el(x, i + 4, "Reason", d.motivo.as_deref().or(d.observacoes.as_deref()).unwrap_or("Devolução / rectificação"));
                let _ = writeln!(x, "{:w$}</References>", "", w = i + 2);
            }
        }
        el(x, i + 2, "Description", &l.get::<String, _>("descricao"));
        el(x, i + 2, if credito { "CreditAmount" } else { "DebitAmount" }, &m(liquido));
        let pct: f64 = l.get("taxa_iva_pct");
        let _ = writeln!(x, "{:w$}<Tax>", "", w = i + 2);
        el(x, i + 4, "TaxType", "IVA");
        el(x, i + 4, "TaxCountryRegion", "AO");
        el(x, i + 4, "TaxCode", &l.get::<String, _>("taxa_codigo"));
        el(x, i + 4, "TaxPercentage", &format!("{pct:.2}"));
        let _ = writeln!(x, "{:w$}</Tax>", "", w = i + 2);
        if pct == 0.0 {
            let codigo: Option<String> = l.get("motivo_isencao");
            let desc: Option<String> = l.get("motivo_desc");
            el(x, i + 2, "TaxExemptionReason", desc.as_deref().unwrap_or("Isento"));
            el(x, i + 2, "TaxExemptionCode", codigo.as_deref().unwrap_or("M00"));
        }
        el(x, i + 2, "SettlementAmount", &m(l.get::<i64, _>("valor_desconto")));
        let _ = writeln!(x, "{:i$}</Line>", "");
    }
    Ok(())
}

fn totais(x: &mut String, ind: usize, d: &Doc) {
    let _ = writeln!(x, "{:ind$}<DocumentTotals>", "");
    el(x, ind + 2, "TaxPayable", &m(d.total_iva));
    el(x, ind + 2, "NetTotal", &m(d.total_liquido));
    el(x, ind + 2, "GrossTotal", &m(d.total));
    let _ = writeln!(x, "{:ind$}</DocumentTotals>", "");
}

fn status(x: &mut String, ind: usize, tag: &str, d: &Doc, origem_tag: &str) {
    let _ = writeln!(x, "{:ind$}<DocumentStatus>", "");
    el(x, ind + 2, tag, estado_saft(d));
    el(x, ind + 2, &format!("{tag}Date"), &d.data_hora);
    if d.estado == "A" {
        el(x, ind + 2, "Reason", d.motivo_anulacao.as_deref().unwrap_or("Anulado"));
    }
    el(x, ind + 2, "SourceID", &d.login);
    el(x, ind + 2, origem_tag, if matches!(d.fonte.as_str(), "P" | "I" | "M") { &d.fonte } else { "P" });
    let _ = writeln!(x, "{:ind$}</DocumentStatus>", "");
}

/// Cabeçalho do ficheiro (comum aos SAF-T de facturação e de aquisição).
fn cabecalho(x: &mut String, emp: &sqlx::sqlite::SqliteRow, nif: &str, de: &str, ate: &str, cert: &str, basis: &str) {
    x.push_str("  <Header>\n");
    el(x, 4, "AuditFileVersion", "1.01_01");
    el(x, 4, "CompanyID", nif);
    el(x, 4, "TaxRegistrationNumber", nif);
    el(x, 4, "TaxAccountingBasis", basis);
    el(x, 4, "CompanyName", &emp.get::<String, _>("nome"));
    if let Some(bn) = emp.get::<Option<String>, _>("designacao_comercial").filter(|s| !s.is_empty()) {
        el(x, 4, "BusinessName", &bn);
    }
    x.push_str("    <CompanyAddress>\n");
    el(x, 6, "AddressDetail", &emp.get::<Option<String>, _>("endereco").unwrap_or_else(|| "Desconhecido".into()));
    el(x, 6, "City", &emp.get::<Option<String>, _>("localidade").unwrap_or_else(|| "Desconhecido".into()));
    if let Some(pv) = emp.get::<Option<String>, _>("provincia").filter(|s| !s.is_empty()) {
        el(x, 6, "Province", &pv);
    }
    el(x, 6, "Country", "AO");
    x.push_str("    </CompanyAddress>\n");
    el(x, 4, "FiscalYear", &de[..4]);
    el(x, 4, "StartDate", de);
    el(x, 4, "EndDate", ate);
    el(x, 4, "CurrencyCode", "AOA");
    el(x, 4, "DateCreated", &db::hoje());
    el(x, 4, "TaxEntity", "Global");
    el(x, 4, "ProductCompanyTaxID", nif);
    el(x, 4, "SoftwareValidationNumber", cert);
    el(x, 4, "ProductID", "KanGest/KanGest");
    el(x, 4, "ProductVersion", VERSAO);
    if let Some(t) = emp.get::<Option<String>, _>("telefone").filter(|s| !s.is_empty()) {
        el(x, 4, "Telephone", &t);
    }
    if let Some(e) = emp.get::<Option<String>, _>("email").filter(|s| !s.is_empty()) {
        el(x, 4, "Email", &e);
    }
    x.push_str("  </Header>\n");
}

fn tabela_iva(x: &mut String, taxas: &[sqlx::sqlite::SqliteRow]) {
    x.push_str("    <TaxTable>\n");
    for t in taxas {
        x.push_str("      <TaxTableEntry>\n");
        el(x, 8, "TaxType", "IVA");
        el(x, 8, "TaxCountryRegion", "AO");
        el(x, 8, "TaxCode", &t.get::<String, _>("codigo"));
        el(x, 8, "Description", &t.get::<String, _>("descricao"));
        el(x, 8, "TaxPercentage", &format!("{:.2}", t.get::<f64, _>("percentagem")));
        x.push_str("      </TaxTableEntry>\n");
    }
    x.push_str("    </TaxTable>\n");
}

async fn preparar(st: &AppState, i: &SaftIn) -> AppResult<(sqlx::sqlite::SqliteRow, String, String)> {
    st.exigir_perm("sistema.saft")?;
    let valida = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok();
    if !valida(&i.de) || !valida(&i.ate) || i.de > i.ate {
        return Err(AppError::msg("Período inválido."));
    }
    let p = &st.pool;
    let emp = sqlx::query("SELECT * FROM empresa WHERE id = 1").fetch_one(p).await?;
    let cert: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?")
        .bind(agt::CERTIFICADO_CONFIG)
        .fetch_optional(p)
        .await?
        .unwrap_or_else(|| "0".into());
    let nif: String = emp.get("nif");
    Ok((emp, nif, cert))
}

fn gravar(st: &AppState, nome: &str, x: &str) -> AppResult<String> {
    let caminho = st.pasta_exportacao.join(nome);
    std::fs::write(&caminho, x.as_bytes()).map_err(|e| AppError::msg(format!("Não foi possível gravar {}: {e}", caminho.display())))?;
    Ok(caminho.display().to_string())
}

pub async fn exportar(st: &AppState, i: SaftIn) -> AppResult<Value> {
    exportar_facturacao(st, i, false).await
}

/// SAF-T de «Faturação para o Estado» (TaxAccountingBasis E): só documentos de entidades públicas.
pub async fn exportar_estado(st: &AppState, i: SaftIn) -> AppResult<Value> {
    exportar_facturacao(st, i, true).await
}

async fn exportar_facturacao(st: &AppState, i: SaftIn, estado: bool) -> AppResult<Value> {
    let (emp, nif, cert) = preparar(st, &i).await?;
    let p = &st.pool;
    // HashControl: versão da chave; "0" enquanto o programa não estiver validado pela AGT.
    let hc = if cert.trim().is_empty() || cert.trim() == "0" { "0" } else { "1" };

    let vendas = docs(p, "'FT','FR','NC','ND','AF'", &i.de, &i.ate, estado).await?;
    let guias = docs(p, "'GT'", &i.de, &i.ate, estado).await?;
    let trabalho = docs(p, "'PP','OR','NE'", &i.de, &i.ate, estado).await?;
    let recibos = docs(p, "'RC'", &i.de, &i.ate, estado).await?;

    let mut x = String::with_capacity(64 * 1024);
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<AuditFile xmlns=\"urn:OECD:StandardAuditFile-Tax:AO_1.01_01\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n");

    cabecalho(&mut x, &emp, &nif, &i.de, &i.ate, &cert, if estado { "E" } else { "F" });

    // ---------------- MasterFiles ----------------
    x.push_str("  <MasterFiles>\n");
    let ids: BTreeSet<i64> = vendas.iter().chain(&guias).chain(&trabalho).chain(&recibos).map(|d| d.entidade_id).collect();
    for id in &ids {
        let c = sqlx::query("SELECT * FROM entidades WHERE id = ?").bind(id).fetch_optional(p).await?;
        let Some(c) = c else { continue };
        x.push_str("    <Customer>\n");
        el(&mut x, 6, "CustomerID", &id.to_string());
        el(&mut x, 6, "AccountID", "Desconhecido");
        el(&mut x, 6, "CustomerTaxID", &c.get::<String, _>("nif"));
        el(&mut x, 6, "CompanyName", &c.get::<String, _>("nome"));
        x.push_str("      <BillingAddress>\n");
        el(&mut x, 8, "AddressDetail", &c.get::<Option<String>, _>("endereco").filter(|s| !s.is_empty()).unwrap_or_else(|| "Desconhecido".into()));
        el(&mut x, 8, "City", &c.get::<Option<String>, _>("localidade").filter(|s| !s.is_empty()).unwrap_or_else(|| "Desconhecido".into()));
        el(&mut x, 8, "Country", &c.get::<String, _>("pais"));
        x.push_str("      </BillingAddress>\n");
        el(&mut x, 6, "SelfBillingIndicator", "0");
        x.push_str("    </Customer>\n");
    }
    let produtos = sqlx::query(
        "SELECT DISTINCT a.tipo, a.referencia, a.descricao, a.codigo_barras, f.descricao AS familia
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN artigos a ON a.id = l.artigo_id
         LEFT JOIN familias f ON f.id = a.familia_id
         LEFT JOIN entidades e ON e.id = d.entidade_id
         WHERE d.tipo_codigo IN ('FT','FR','NC','ND','AF','GT','PP','OR','NE') AND d.importado = 0 AND d.data_emissao BETWEEN ?1 AND ?2
           AND (?3 = 0 OR e.estado = 1)
         ORDER BY a.referencia",
    )
    .bind(&i.de)
    .bind(&i.ate)
    .bind(estado as i64)
    .fetch_all(p)
    .await?;
    for a in &produtos {
        let referencia: String = a.get("referencia");
        x.push_str("    <Product>\n");
        el(&mut x, 6, "ProductType", &a.get::<String, _>("tipo"));
        el(&mut x, 6, "ProductCode", &referencia);
        if let Some(f) = a.get::<Option<String>, _>("familia") {
            el(&mut x, 6, "ProductGroup", &f);
        }
        el(&mut x, 6, "ProductDescription", &a.get::<String, _>("descricao"));
        el(&mut x, 6, "ProductNumberCode", &a.get::<Option<String>, _>("codigo_barras").filter(|s| !s.is_empty()).unwrap_or(referencia));
        x.push_str("    </Product>\n");
    }
    tabela_iva(&mut x, &sqlx::query("SELECT * FROM taxas_iva ORDER BY id").fetch_all(p).await?);
    x.push_str("  </MasterFiles>\n");

    // ---------------- SourceDocuments ----------------
    x.push_str("  <SourceDocuments>\n");

    // SalesInvoices
    let validos = vendas.iter().filter(|d| d.estado != "A");
    let (debito, credito) = validos.fold((0i64, 0i64), |acc, d| if d.tipo == "NC" { (acc.0 + d.total_liquido, acc.1) } else { (acc.0, acc.1 + d.total_liquido) });
    x.push_str("    <SalesInvoices>\n");
    el(&mut x, 6, "NumberOfEntries", &vendas.len().to_string());
    el(&mut x, 6, "TotalDebit", &m(debito));
    el(&mut x, 6, "TotalCredit", &m(credito));
    for d in &vendas {
        x.push_str("      <Invoice>\n");
        el(&mut x, 8, "InvoiceNo", &d.numero);
        status(&mut x, 8, "InvoiceStatus", d, "SourceBilling");
        el(&mut x, 8, "Hash", &d.hash);
        el(&mut x, 8, "HashControl", hc);
        el(&mut x, 8, "Period", &d.data[5..7].trim_start_matches('0').to_string());
        el(&mut x, 8, "InvoiceDate", &d.data);
        el(&mut x, 8, "InvoiceType", &d.tipo);
        x.push_str("        <SpecialRegimes>\n");
        el(&mut x, 10, "SelfBillingIndicator", if d.tipo == "AF" { "1" } else { "0" });
        el(&mut x, 10, "CashVATSchemeIndicator", "0");
        el(&mut x, 10, "ThirdPartiesBillingIndicator", "0");
        x.push_str("        </SpecialRegimes>\n");
        el(&mut x, 8, "SourceID", &d.login);
        el(&mut x, 8, "SystemEntryDate", &d.data_hora);
        el(&mut x, 8, "CustomerID", &d.entidade_id.to_string());
        linhas(&mut x, p, d, 8, d.tipo != "NC", false).await?;
        totais(&mut x, 8, d);
        retencoes(&mut x, p, d, 8).await?;
        x.push_str("      </Invoice>\n");
    }
    x.push_str("    </SalesInvoices>\n");

    // MovementOfGoods (guias de transporte)
    let n_linhas: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id LEFT JOIN entidades e ON e.id = d.entidade_id
         WHERE d.tipo_codigo = 'GT' AND d.estado <> 'A' AND d.importado = 0 AND d.data_emissao BETWEEN ?1 AND ?2 AND (?3 = 0 OR e.estado = 1)",
    )
    .bind(&i.de)
    .bind(&i.ate)
    .bind(estado as i64)
    .fetch_one(p)
    .await?;
    let qtd_total: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(l.quantidade), 0.0) FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id LEFT JOIN entidades e ON e.id = d.entidade_id
         WHERE d.tipo_codigo = 'GT' AND d.estado <> 'A' AND d.importado = 0 AND d.data_emissao BETWEEN ?1 AND ?2 AND (?3 = 0 OR e.estado = 1)",
    )
    .bind(&i.de)
    .bind(&i.ate)
    .bind(estado as i64)
    .fetch_one(p)
    .await?;
    if !guias.is_empty() {
        x.push_str("    <MovementOfGoods>\n");
        el(&mut x, 6, "NumberOfMovementLines", &n_linhas.to_string());
        el(&mut x, 6, "TotalQuantityIssued", &qtd(qtd_total));
        for d in &guias {
            let t: Value = d.transporte.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
            let campo = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            x.push_str("      <StockMovement>\n");
            el(&mut x, 8, "DocumentNumber", &d.numero);
            status(&mut x, 8, "MovementStatus", d, "SourceBilling");
            el(&mut x, 8, "Hash", &d.hash);
            el(&mut x, 8, "HashControl", hc);
            el(&mut x, 8, "Period", &d.data[5..7].trim_start_matches('0').to_string());
            el(&mut x, 8, "MovementDate", &d.data);
            el(&mut x, 8, "MovementType", "GT");
            el(&mut x, 8, "SystemEntryDate", &d.data_hora);
            el(&mut x, 8, "CustomerID", &d.entidade_id.to_string());
            el(&mut x, 8, "SourceID", &d.login);
            for (tag, local) in [("ShipTo", campo("descarga_local")), ("ShipFrom", campo("carga_local"))] {
                let _ = writeln!(x, "        <{tag}>");
                x.push_str("          <Address>\n");
                el(&mut x, 12, "AddressDetail", if local.is_empty() { "Desconhecido" } else { &local });
                el(&mut x, 12, "City", "Desconhecido");
                el(&mut x, 12, "Country", "AO");
                x.push_str("          </Address>\n");
                let _ = writeln!(x, "        </{tag}>");
            }
            let inicio = campo("carga_data");
            el(&mut x, 8, "MovementStartTime", if inicio.len() >= 16 { &inicio } else { &d.data_hora });
            linhas(&mut x, p, d, 8, true, true).await?;
            totais(&mut x, 8, d);
            x.push_str("      </StockMovement>\n");
        }
        x.push_str("    </MovementOfGoods>\n");
    }

    // WorkingDocuments (proformas e orçamentos)
    let validos: Vec<&Doc> = trabalho.iter().filter(|d| d.estado != "A").collect();
    x.push_str("    <WorkingDocuments>\n");
    el(&mut x, 6, "NumberOfEntries", &trabalho.len().to_string());
    el(&mut x, 6, "TotalDebit", "0.00");
    el(&mut x, 6, "TotalCredit", &m(validos.iter().map(|d| d.total_liquido).sum()));
    for d in &trabalho {
        x.push_str("      <WorkDocument>\n");
        el(&mut x, 8, "DocumentNumber", &d.numero);
        status(&mut x, 8, "WorkStatus", d, "SourceBilling");
        el(&mut x, 8, "Hash", &d.hash);
        el(&mut x, 8, "HashControl", hc);
        el(&mut x, 8, "Period", &d.data[5..7].trim_start_matches('0').to_string());
        el(&mut x, 8, "WorkDate", &d.data);
        el(&mut x, 8, "WorkType", &d.tipo);
        el(&mut x, 8, "SourceID", &d.login);
        el(&mut x, 8, "SystemEntryDate", &d.data_hora);
        el(&mut x, 8, "CustomerID", &d.entidade_id.to_string());
        linhas(&mut x, p, d, 8, true, false).await?;
        totais(&mut x, 8, d);
        x.push_str("      </WorkDocument>\n");
    }
    x.push_str("    </WorkingDocuments>\n");

    // Payments (recibos)
    let validos: Vec<&Doc> = recibos.iter().filter(|d| d.estado != "A").collect();
    x.push_str("    <Payments>\n");
    el(&mut x, 6, "NumberOfEntries", &recibos.len().to_string());
    el(&mut x, 6, "TotalDebit", "0.00");
    el(&mut x, 6, "TotalCredit", &m(validos.iter().map(|d| d.total).sum()));
    for d in &recibos {
        x.push_str("      <Payment>\n");
        el(&mut x, 8, "PaymentRefNo", &d.numero);
        el(&mut x, 8, "Period", &d.data[5..7].trim_start_matches('0').to_string());
        el(&mut x, 8, "TransactionDate", &d.data);
        el(&mut x, 8, "PaymentType", "RC");
        status(&mut x, 8, "PaymentStatus", d, "SourcePayment");
        for pg in sqlx::query("SELECT modo_codigo, valor FROM documento_pagamentos WHERE documento_id = ?").bind(d.id).fetch_all(p).await? {
            x.push_str("        <PaymentMethod>\n");
            el(&mut x, 10, "PaymentMechanism", &pg.get::<String, _>("modo_codigo"));
            el(&mut x, 10, "PaymentAmount", &m(pg.get::<i64, _>("valor")));
            el(&mut x, 10, "PaymentDate", &d.data);
            x.push_str("        </PaymentMethod>\n");
        }
        el(&mut x, 8, "SourceID", &d.login);
        el(&mut x, 8, "SystemEntryDate", &d.data_hora);
        el(&mut x, 8, "CustomerID", &d.entidade_id.to_string());
        let liq = sqlx::query(
            "SELECT r.valor, o.numero_doc, o.data_emissao FROM recibo_documentos r JOIN documentos o ON o.id = r.documento_id WHERE r.recibo_id = ?",
        )
        .bind(d.id)
        .fetch_all(p)
        .await?;
        for (n, l) in liq.iter().enumerate() {
            x.push_str("        <Line>\n");
            el(&mut x, 10, "LineNumber", &(n + 1).to_string());
            x.push_str("          <SourceDocumentID>\n");
            el(&mut x, 12, "OriginatingON", &l.get::<String, _>("numero_doc"));
            el(&mut x, 12, "InvoiceDate", &l.get::<String, _>("data_emissao"));
            x.push_str("          </SourceDocumentID>\n");
            el(&mut x, 10, "CreditAmount", &m(l.get::<i64, _>("valor")));
            x.push_str("        </Line>\n");
        }
        x.push_str("        <DocumentTotals>\n");
        el(&mut x, 10, "TaxPayable", "0.00");
        el(&mut x, 10, "NetTotal", &m(d.total));
        el(&mut x, 10, "GrossTotal", &m(d.total));
        x.push_str("        </DocumentTotals>\n");
        x.push_str("      </Payment>\n");
    }
    x.push_str("    </Payments>\n");
    x.push_str("  </SourceDocuments>\n");
    x.push_str("</AuditFile>\n");

    let ficheiro = gravar(st, &format!("SAFT_AO_{}{nif}_{}_{}.xml", if estado { "ESTADO_" } else { "" }, i.de, i.ate), &x)?;
    Ok(json!({
        "ficheiro": ficheiro,
        "facturas": vendas.len(),
        "documentos_trabalho": trabalho.len(),
        "guias": guias.len(),
        "recibos": recibos.len(),
        "bytes": x.len(),
    }))
}

// =====================================================================================
// SAF-T de aquisição de bens e serviços (TaxAccountingBasis = A)
// =====================================================================================

/// PurchaseType do SAF-T para cada tipo de documento de compra.
/// A V/Nota de Débito não tem tipo próprio na tabela da AGT e é reportada como factura (FT).
fn tipo_compra(codigo: &str) -> Option<&'static str> {
    match codigo {
        "FC" | "VND" => Some("FT"),
        "VFR" => Some("FR"),
        "VNC" => Some("NC"),
        "AF" => Some("AF"),
        _ => None,
    }
}

/// Tipo de dedução do IVA de uma linha quando o documento não o indica:
/// bens → existências (ou importação), serviços → serviços (ou contratados no estrangeiro).
fn deducao_auto(tipo_artigo: Option<&str>, estrangeira: bool) -> &'static str {
    match (tipo_artigo, estrangeira) {
        (Some("S"), false) => "SERV",
        (Some("S"), true) => "SCE",
        (Some("P"), true) | (None, true) => "IMPT",
        (Some("P"), false) => "INV",
        _ => "OBC",
    }
}

fn pais(c: Option<String>) -> String {
    c.map(|p| p.trim().to_uppercase())
        .filter(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_uppercase()))
        .unwrap_or_else(|| "AO".into())
}

pub async fn exportar_compras(st: &AppState, i: SaftIn) -> AppResult<Value> {
    let (emp, nif, cert) = preparar(st, &i).await?;
    let p = &st.pool;

    // Documentos registados no período (o período contabilístico é o do registo; a data é a do fornecedor).
    let docs = sqlx::query(
        "SELECT d.id, d.tipo_codigo, d.numero_doc, d.doc_externo, d.doc_externo_hash, d.doc_externo_data, d.deducao_tipo,
                d.hash_controlo, d.data_emissao, COALESCE(d.entidade_id, 1) AS ent, u.login,
                d.total_liquido, d.total_iva, d.total, d.moeda, d.cambio, d.total_moeda,
                COALESCE(o.doc_externo, o.numero_doc) AS origem, COALESCE(o.doc_externo_data, o.data_emissao) AS origem_data
         FROM documentos d JOIN utilizadores u ON u.id = d.utilizador_id
         LEFT JOIN documentos o ON o.id = d.doc_origem_id
         WHERE d.tipo_codigo IN ('FC','VFR','VNC','VND','AF') AND d.estado <> 'A' AND d.importado = 0
           AND d.data_emissao BETWEEN ?1 AND ?2
         ORDER BY d.data_emissao, d.id",
    )
    .bind(&i.de)
    .bind(&i.ate)
    .fetch_all(p)
    .await?;

    let mut x = String::with_capacity(32 * 1024);
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<AuditFile xmlns=\"urn:OECD:StandardAuditFile-Tax:AO_1.01_01\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n");
    cabecalho(&mut x, &emp, &nif, &i.de, &i.ate, &cert, "A");

    // ---------------- MasterFiles: fornecedores ----------------
    x.push_str("  <MasterFiles>\n");
    let ids: BTreeSet<i64> = docs.iter().map(|d| d.get::<i64, _>("ent")).collect();
    let autofacturacao: BTreeSet<i64> = docs.iter().filter(|d| d.get::<String, _>("tipo_codigo") == "AF").map(|d| d.get::<i64, _>("ent")).collect();
    for id in &ids {
        let Some(f) = sqlx::query("SELECT * FROM entidades WHERE id = ?").bind(id).fetch_optional(p).await? else { continue };
        x.push_str("    <Supplier>\n");
        el(&mut x, 6, "SupplierID", &id.to_string());
        el(&mut x, 6, "AccountID", "Desconhecido");
        el(&mut x, 6, "SupplierTaxID", &f.get::<String, _>("nif"));
        el(&mut x, 6, "CompanyName", &f.get::<String, _>("nome"));
        x.push_str("      <BillingAddress>\n");
        el(&mut x, 8, "AddressDetail", &f.get::<Option<String>, _>("endereco").filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Desconhecido".into()));
        el(&mut x, 8, "City", &f.get::<Option<String>, _>("localidade").filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Desconhecido".into()));
        if let Some(pv) = f.get::<Option<String>, _>("provincia").filter(|s| !s.trim().is_empty()) {
            el(&mut x, 8, "Province", &pv);
        }
        el(&mut x, 8, "Country", &pais(f.get("pais")));
        x.push_str("      </BillingAddress>\n");
        if let Some(t) = f.get::<Option<String>, _>("telefone").filter(|s| !s.trim().is_empty()) {
            el(&mut x, 6, "Telephone", &t.chars().take(20).collect::<String>());
        }
        if let Some(e) = f.get::<Option<String>, _>("email").filter(|s| s.contains('@')) {
            el(&mut x, 6, "Email", &e);
        }
        el(&mut x, 6, "SelfBillingIndicator", if autofacturacao.contains(id) { "1" } else { "0" });
        x.push_str("    </Supplier>\n");
    }
    tabela_iva(&mut x, &sqlx::query("SELECT * FROM taxas_iva ORDER BY id").fetch_all(p).await?);
    x.push_str("  </MasterFiles>\n");

    // ---------------- SourceDocuments: PurchaseInvoices ----------------
    x.push_str("  <SourceDocuments>\n");
    x.push_str("    <PurchaseInvoices>\n");
    el(&mut x, 6, "NumberOfEntries", &docs.len().to_string());
    let (mut total_iva, mut total_liquido) = (0i64, 0i64);
    for d in &docs {
        let id: i64 = d.get("id");
        let codigo: String = d.get("tipo_codigo");
        let Some(tipo) = tipo_compra(&codigo) else { continue };
        let proprio = codigo == "AF";
        let numero: String = if proprio { d.get("numero_doc") } else { d.get::<Option<String>, _>("doc_externo").unwrap_or_else(|| d.get("numero_doc")) };
        let hash: String = if proprio {
            d.get::<Option<String>, _>("hash_controlo").unwrap_or_else(|| "0".into())
        } else {
            d.get::<Option<String>, _>("doc_externo_hash").filter(|h| !h.is_empty()).unwrap_or_else(|| "0".into())
        };
        let registo: String = d.get("data_emissao");
        let data: String = d.get::<Option<String>, _>("doc_externo_data").unwrap_or_else(|| registo.clone());
        let moeda: String = d.get("moeda");
        let estrangeira = moeda != "AOA";
        total_iva += d.get::<i64, _>("total_iva");
        total_liquido += d.get::<i64, _>("total_liquido");

        x.push_str("      <Invoice>\n");
        el(&mut x, 8, "InvoiceNo", &numero.chars().take(60).collect::<String>());
        if tipo == "NC" {
            if let Some(o) = d.get::<Option<String>, _>("origem") {
                x.push_str("        <References>\n");
                el(&mut x, 10, "Reference", &o);
                if let Some(od) = d.get::<Option<String>, _>("origem_data") {
                    el(&mut x, 10, "InvoiceDate", &od);
                }
                x.push_str("        </References>\n");
            }
        }
        el(&mut x, 8, "Hash", &hash);
        el(&mut x, 8, "SourceID", &d.get::<String, _>("login"));
        el(&mut x, 8, "Period", registo[5..7].trim_start_matches('0'));
        el(&mut x, 8, "InvoiceDate", &data);
        el(&mut x, 8, "PurchaseType", tipo);
        el(&mut x, 8, "SupplierID", &d.get::<i64, _>("ent").to_string());
        x.push_str("        <DocumentTotals>\n");
        el(&mut x, 10, "TaxPayable", &m(d.get("total_iva")));
        el(&mut x, 10, "NetTotal", &m(d.get("total_liquido")));
        el(&mut x, 10, "GrossTotal", &m(d.get("total")));
        // IVA dedutível agrupado pelo tipo de dedução (só linhas com imposto).
        let fixo: Option<String> = d.get("deducao_tipo");
        let mut grupos: std::collections::BTreeMap<String, (i64, i64)> = Default::default();
        for l in sqlx::query(
            "SELECT l.valor_liquido, l.valor_iva, a.tipo FROM documento_linhas l LEFT JOIN artigos a ON a.id = l.artigo_id
             WHERE l.documento_id = ? AND l.valor_iva <> 0",
        )
        .bind(id)
        .fetch_all(p)
        .await?
        {
            let t = fixo.clone().unwrap_or_else(|| deducao_auto(l.get::<Option<String>, _>("tipo").as_deref(), estrangeira).to_string());
            let e = grupos.entry(t).or_default();
            e.0 += l.get::<i64, _>("valor_liquido");
            e.1 += l.get::<i64, _>("valor_iva");
        }
        for (t, (base, iva)) in &grupos {
            x.push_str("          <Deductible>\n");
            el(&mut x, 12, "TaxBase", &m(*base));
            el(&mut x, 12, "DeductibleAmount", &m(*iva));
            el(&mut x, 12, "DeductiblePercentage", "100.00");
            el(&mut x, 12, "DeductibleTaxType", t);
            x.push_str("          </Deductible>\n");
        }
        if estrangeira {
            if let Some(tm) = d.get::<Option<i64>, _>("total_moeda") {
                x.push_str("          <Currency>\n");
                el(&mut x, 12, "CurrencyCode", &moeda);
                el(&mut x, 12, "CurrencyAmount", &m(tm));
                el(&mut x, 12, "ExchangeRate", &format!("{:.6}", d.get::<f64, _>("cambio")));
                x.push_str("          </Currency>\n");
            }
        }
        x.push_str("        </DocumentTotals>\n");
        for r in sqlx::query("SELECT codigo, descricao, valor FROM documento_retencoes WHERE documento_id = ? ORDER BY id")
            .bind(id)
            .fetch_all(p)
            .await?
        {
            x.push_str("        <WithholdingTax>\n");
            el(&mut x, 10, "WithholdingTaxType", &r.get::<String, _>("codigo"));
            el(&mut x, 10, "WithholdingTaxDescription", &r.get::<String, _>("descricao").chars().take(60).collect::<String>());
            el(&mut x, 10, "WithholdingTaxAmount", &m(r.get::<i64, _>("valor")));
            x.push_str("        </WithholdingTax>\n");
        }
        x.push_str("      </Invoice>\n");
    }
    x.push_str("    </PurchaseInvoices>\n");
    x.push_str("  </SourceDocuments>\n");
    x.push_str("</AuditFile>\n");

    let ficheiro = gravar(st, &format!("SAFT_AO_AQUISICAO_{nif}_{}_{}.xml", i.de, i.ate), &x)?;
    Ok(json!({
        "ficheiro": ficheiro,
        "documentos": docs.len(),
        "fornecedores": ids.len(),
        "sem_assinatura": docs.iter().filter(|d| d.get::<String, _>("tipo_codigo") != "AF" && d.get::<Option<String>, _>("doc_externo_hash").filter(|h| !h.is_empty()).is_none()).count(),
        "total_liquido": total_liquido,
        "total_iva": total_iva,
        "bytes": x.len(),
    }))
}

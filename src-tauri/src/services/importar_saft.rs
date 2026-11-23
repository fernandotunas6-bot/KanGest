// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Importação universal por SAF-T (AO): o ficheiro que todos os sistemas de facturação certificados em Angola exportam
//! para a AGT. Copia de qualquer sistema, sem conhecer a base de cada marca:
//!
//! - os **clientes** e os **fornecedores** (`MasterFiles/Customer`, `Supplier`): não duplica, procura pelo NIF; o
//!   «Consumidor final» (999999999) fica de fora, o KanGest tem o seu;
//! - os **artigos** (`MasterFiles/Product`): não duplica, procura pela referência (`ProductCode`). A lista de artigos
//!   do SAF-T não traz preços: o **último preço de venda** e a **taxa de IVA** vêm das facturas do próprio ficheiro
//!   (`SourceDocuments/SalesInvoices`, sem as anuladas nem as notas de crédito). Num artigo que já existe só se
//!   preenche o que lhe falta (código de barras, preço a zero).
//!
//! Os documentos ficam de fora: a facturação recomeça no KanGest, com as suas séries. O ficheiro lê-se por fluxo
//! (`quick-xml`), sem o carregar todo na memória. «Simular» corre tudo e desfaz no fim, como na importação do KIGEST.

use std::collections::HashMap;
use std::path::Path;

use quick_xml::events::Event;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Os campos de um registo do SAF-T pelo nome local (o primeiro que aparece: a morada de facturação vem antes da de
/// entrega).
#[derive(Debug, Default, Clone)]
pub(crate) struct Registo(HashMap<String, String>);

impl Registo {
    fn por(&mut self, k: &str, v: String) {
        self.0.entry(k.to_string()).or_insert(v);
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.0.get(k).map(|s| s.as_str()).filter(|s| !s.is_empty())
    }
}

/// O último preço de venda de um artigo nas facturas do ficheiro.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preco {
    data: String,
    /// sem IVA, em cêntimos
    preco: i64,
    taxa: f64,
    isencao: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Saft {
    pub empresa: Registo,
    pub clientes: Vec<Registo>,
    pub fornecedores: Vec<Registo>,
    pub produtos: Vec<Registo>,
    pub precos: HashMap<String, Preco>,
    pub facturas: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Tipo {
    Empresa,
    Cliente,
    Fornecedor,
    Produto,
}

/// Valor em kwanzas do SAF-T («1000.00», «12.5») em cêntimos.
fn centimos(s: &str) -> Option<i64> {
    s.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| (x * 100.0).round() as i64)
}

/// Lê o SAF-T por fluxo. Recusa um ficheiro que não seja SAF-T.
pub(crate) fn ler(caminho: &Path) -> AppResult<Saft> {
    let mut r = quick_xml::Reader::from_file(caminho).map_err(|e| AppError::msg(format!("Não foi possível abrir {}: {e}", caminho.display())))?;
    // sem trim_text: o texto de um elemento chega aos pedaços (antes e depois de «&amp;») e apara-se só no fim
    let mut buf = Vec::new();
    let mut pilha: Vec<String> = Vec::new();
    let mut texto = String::new();
    let mut s = Saft::default();
    let mut atual: Option<(Tipo, Registo)> = None;
    let mut factura: Option<Registo> = None;
    let mut linha: Option<Registo> = None;
    let mut linhas: Vec<Registo> = Vec::new();
    let mut e_saft = false;
    loop {
        let ev = r.read_event_into(&mut buf).map_err(|e| AppError::msg(format!("{} não é um SAF-T válido: {e}", caminho.display())))?;
        match ev {
            Event::Start(e) => {
                let n = e.local_name().as_ref().to_string();
                let pai = pilha.last().map(|x| x.as_str()).unwrap_or("");
                match (n.as_str(), pai) {
                    ("AuditFile", _) => e_saft = true,
                    ("Header", "AuditFile") => atual = Some((Tipo::Empresa, Registo::default())),
                    ("Customer", "MasterFiles") => atual = Some((Tipo::Cliente, Registo::default())),
                    ("Supplier", "MasterFiles") => atual = Some((Tipo::Fornecedor, Registo::default())),
                    ("Product", "MasterFiles") => atual = Some((Tipo::Produto, Registo::default())),
                    ("Invoice", "SalesInvoices") => {
                        factura = Some(Registo::default());
                        linhas.clear();
                    }
                    ("Line", "Invoice") if factura.is_some() => linha = Some(Registo::default()),
                    _ => {}
                }
                pilha.push(n);
                texto.clear();
            }
            Event::Text(t) => texto.push_str(&t.xml10_content()),
            Event::CData(c) => texto.push_str(&c.into_inner()),
            Event::GeneralRef(g) => match g.resolve_char_ref() {
                Ok(Some(c)) => texto.push(c),
                _ => texto.push_str(match &*g {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                }),
            },
            Event::End(_) => {
                let n = pilha.pop().unwrap_or_default();
                let v = texto.trim().to_string();
                texto.clear();
                if !v.is_empty() {
                    if let Some(l) = linha.as_mut() {
                        l.por(&n, v);
                    } else if let Some(f) = factura.as_mut() {
                        f.por(&n, v);
                    } else if let Some((_, reg)) = atual.as_mut() {
                        reg.por(&n, v);
                    }
                }
                match n.as_str() {
                    "Header" | "Customer" | "Supplier" | "Product" => {
                        if let Some((tipo, reg)) = atual.take() {
                            match tipo {
                                Tipo::Empresa => s.empresa = reg,
                                Tipo::Cliente => s.clientes.push(reg),
                                Tipo::Fornecedor => s.fornecedores.push(reg),
                                Tipo::Produto => s.produtos.push(reg),
                            }
                        }
                    }
                    "Line" => {
                        if let Some(l) = linha.take() {
                            linhas.push(l);
                        }
                    }
                    "Invoice" => {
                        if let Some(f) = factura.take() {
                            // as anuladas e as notas de crédito e de débito não dizem o preço de venda
                            let anulada = f.get("InvoiceStatus") == Some("A");
                            let venda = matches!(f.get("InvoiceType"), Some("FT" | "FR" | "VD" | "FS" | "FA"));
                            if !anulada && venda {
                                s.facturas += 1;
                                let data = f.get("InvoiceDate").unwrap_or("").to_string();
                                for l in linhas.drain(..) {
                                    let (Some(codigo), Some(preco)) = (l.get("ProductCode"), l.get("UnitPrice").and_then(centimos)) else { continue };
                                    let p = Preco {
                                        data: data.clone(),
                                        preco,
                                        taxa: l.get("TaxPercentage").and_then(|x| x.parse().ok()).unwrap_or(0.0),
                                        isencao: l.get("TaxExemptionCode").map(String::from),
                                    };
                                    let mais_recente = s.precos.get(codigo).is_none_or(|q| q.data <= p.data);
                                    if mais_recente {
                                        s.precos.insert(codigo.to_string(), p);
                                    }
                                }
                            }
                            linhas.clear();
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !e_saft {
        return Err(AppError::msg(format!("{} não é um ficheiro SAF-T (falta o AuditFile).", caminho.display())));
    }
    Ok(s)
}

#[derive(Deserialize)]
pub struct AnalisarIn {
    caminho: String,
}

/// O que o ficheiro traz, antes de importar.
pub async fn analisar(st: &AppState, i: AnalisarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let caminho = std::path::PathBuf::from(i.caminho.trim());
    let s = tokio::task::spawn_blocking(move || ler(&caminho)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let e = &s.empresa;
    let consumidor = |r: &Registo| r.get("CustomerTaxID").is_none_or(|n| n == "999999999");
    Ok(json!({
        "empresa": { "nome": e.get("CompanyName"), "nif": e.get("TaxRegistrationNumber"), "inicio": e.get("StartDate"), "fim": e.get("EndDate"), "programa": e.get("ProductID") },
        "clientes": s.clientes.iter().filter(|r| !consumidor(r)).count(),
        "fornecedores": s.fornecedores.len(),
        "artigos": s.produtos.len(),
        "com_preco": s.produtos.iter().filter(|p| p.get("ProductCode").is_some_and(|c| s.precos.contains_key(c))).count(),
        "facturas": s.facturas,
    }))
}

#[derive(Deserialize)]
pub struct ExecutarIn {
    caminho: String,
    #[serde(default)]
    simular: bool,
    #[serde(default = "sim")]
    clientes: bool,
    #[serde(default = "sim")]
    fornecedores: bool,
    #[serde(default = "sim")]
    artigos: bool,
}
fn sim() -> bool {
    true
}

/// Uma entidade do SAF-T: a que já existe com o mesmo NIF (passa a «ambos» se era só do outro tipo), ou uma nova.
async fn entidade(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, r: &Registo, tipo: &str, campo_nif: &str, campo_id: &str) -> AppResult<Option<bool>> {
    let nif = r.get(campo_nif).unwrap_or("999999999").trim().to_string();
    let Some(nome) = r.get("CompanyName").map(str::trim).filter(|n| !n.is_empty()) else { return Ok(None) };
    if nif == "999999999" || nif.is_empty() {
        return Ok(None);
    }
    let existente: Option<(i64, String)> = sqlx::query_as("SELECT id, tipo FROM entidades WHERE nif = ? AND nif <> '999999999' ORDER BY id LIMIT 1")
        .bind(&nif)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some((id, t)) = existente {
        if t != tipo && t != "A" {
            sqlx::query("UPDATE entidades SET tipo = 'A' WHERE id = ?").bind(id).execute(&mut **tx).await?;
        }
        return Ok(Some(false));
    }
    let pais = r.get("Country").map(|p| p.chars().take(2).collect::<String>().to_uppercase()).filter(|p| p.len() == 2).unwrap_or_else(|| "AO".into());
    sqlx::query("INSERT INTO entidades (tipo, nome, nif, endereco, localidade, pais, telefone, email, id_origem) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(tipo)
        .bind(nome)
        .bind(&nif)
        .bind(r.get("AddressDetail"))
        .bind(r.get("City"))
        .bind(pais)
        .bind(r.get("Telephone"))
        .bind(r.get("Email"))
        .bind(r.get(campo_id).map(|x| format!("saft:{tipo}:{x}")))
        .execute(&mut **tx)
        .await?;
    Ok(Some(true))
}

/// Importa os clientes, os fornecedores e os artigos (ou simula: corre tudo e desfaz no fim).
pub async fn executar(st: &AppState, i: ExecutarIn) -> AppResult<Value> {
    let sessao = st.exigir_perm("sistema.importar")?;
    let caminho = std::path::PathBuf::from(i.caminho.trim());
    let origem = caminho.display().to_string();
    let s = tokio::task::spawn_blocking(move || ler(&caminho)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let mut tx = db::transacao(&st.pool).await?;
    let mut avisos: Vec<String> = Vec::new();
    let (mut cli_novos, mut cli_exist, mut forn_novos, mut forn_exist) = (0, 0, 0, 0);
    if i.clientes {
        for r in &s.clientes {
            match entidade(&mut tx, r, "C", "CustomerTaxID", "CustomerID").await? {
                Some(true) => cli_novos += 1,
                Some(false) => cli_exist += 1,
                None => {}
            }
        }
    }
    if i.fornecedores {
        for r in &s.fornecedores {
            match entidade(&mut tx, r, "F", "SupplierTaxID", "SupplierID").await? {
                Some(true) => forn_novos += 1,
                Some(false) => forn_exist += 1,
                None => {}
            }
        }
    }
    let (mut art_novos, mut art_exist, mut art_preco, mut sem_motivo, mut taxa_desconhecida) = (0, 0, 0, 0, 0);
    if i.artigos {
        let taxas: Vec<(i64, f64, String)> = sqlx::query_as("SELECT id, percentagem, codigo FROM taxas_iva WHERE ativo = 1 ORDER BY id").fetch_all(&mut *tx).await?;
        let motivos: Vec<String> = sqlx::query_scalar("SELECT codigo FROM motivos_isencao").fetch_all(&mut *tx).await?;
        let normal = taxas.iter().find(|t| t.2 == "NOR").map(|t| t.0).unwrap_or(1);
        for p in &s.produtos {
            let (Some(codigo), Some(descricao)) = (p.get("ProductCode").map(str::trim), p.get("ProductDescription").map(str::trim)) else { continue };
            let preco = s.precos.get(codigo);
            let taxa = match preco {
                Some(x) => match taxas.iter().find(|t| (t.1 - x.taxa).abs() < 0.001) {
                    Some(t) => t.0,
                    None => {
                        taxa_desconhecida += 1;
                        normal
                    }
                },
                None => normal,
            };
            let percentagem = taxas.iter().find(|t| t.0 == taxa).map(|t| t.1).unwrap_or(14.0);
            let motivo = if percentagem == 0.0 {
                let m = preco.and_then(|x| x.isencao.clone()).filter(|m| motivos.contains(m));
                if m.is_none() {
                    sem_motivo += 1;
                }
                m
            } else {
                None
            };
            let barras = p.get("ProductNumberCode").filter(|b| *b != codigo).map(String::from);
            let valor = preco.map(|x| x.preco).unwrap_or(0);
            let pvp = (valor as f64 * (100.0 + percentagem) / 100.0).round() as i64;
            if preco.is_some() {
                art_preco += 1;
            }
            let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ?").bind(codigo).fetch_optional(&mut *tx).await?;
            match existente {
                // o que já está no KanGest manda: só se preenche o que falta, e só se toca no artigo quando
                // alguma coisa muda (a data de alteração leva-o à loja online e à API)
                Some(id) => {
                    art_exist += 1;
                    sqlx::query(
                        "UPDATE artigos SET codigo_barras = COALESCE(codigo_barras, ?1),
                            preco1 = CASE WHEN preco1 = 0 THEN ?2 ELSE preco1 END,
                            pvp1 = CASE WHEN preco1 = 0 THEN ?3 ELSE pvp1 END
                         WHERE id = ?4 AND ((codigo_barras IS NULL AND ?1 IS NOT NULL) OR (preco1 = 0 AND ?2 <> 0))",
                    )
                    .bind(&barras)
                    .bind(valor)
                    .bind(pvp)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                }
                None => {
                    art_novos += 1;
                    let servico = matches!(p.get("ProductType"), Some("S" | "O" | "E" | "I"));
                    sqlx::query(
                        "INSERT INTO artigos (referencia, codigo_barras, descricao, tipo, unidade_id, taxa_iva_id, motivo_isencao, preco1, pvp1,
                            controla_stock, id_origem) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(codigo)
                    .bind(&barras)
                    .bind(descricao)
                    .bind(if servico { "S" } else { "P" })
                    .bind(if servico { 6 } else { 1 })
                    .bind(taxa)
                    .bind(&motivo)
                    .bind(valor)
                    .bind(pvp)
                    .bind(!servico as i64)
                    .bind(format!("saft:{codigo}"))
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }
    if sem_motivo > 0 {
        avisos.push(format!("{sem_motivo} artigo(s) isento(s) sem o motivo de isenção reconhecido: complete-o em Artigos antes de facturar."));
    }
    if taxa_desconhecida > 0 {
        avisos.push(format!("{taxa_desconhecida} artigo(s) com uma taxa de IVA que o KanGest não tem: ficaram com o IVA normal; confira-os em Artigos."));
    }
    let sem_preco = s.produtos.len().saturating_sub(art_preco);
    if i.artigos && sem_preco > 0 {
        avisos.push(format!("{sem_preco} artigo(s) sem vendas no ficheiro: ficaram sem preço; ponha-o em Artigos."));
    }
    let relatorio = json!({
        "simulacao": i.simular,
        "empresa": s.empresa.get("CompanyName"),
        "clientes": { "novos": cli_novos, "existentes": cli_exist },
        "fornecedores": { "novos": forn_novos, "existentes": forn_exist },
        "artigos": { "novos": art_novos, "existentes": art_exist, "com_preco": art_preco },
        "avisos": avisos,
    });
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'IMPORTAR', ?)")
        .bind(sessao.id)
        .bind(format!("SAF-T {origem}"))
        .execute(&mut *tx)
        .await?;
    if i.simular {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    Ok(relatorio)
}

/// Os ficheiros SAF-T (AO) nas pastas do utilizador e nas pens: XML com o AuditFile de Angola no início.
pub(crate) fn procurar(pastas: &[std::path::PathBuf]) -> Vec<Value> {
    use std::io::Read;
    let mut achados = Vec::new();
    for p in pastas {
        let Ok(it) = std::fs::read_dir(p) else { continue };
        for e in it.flatten().take(500) {
            let f = e.path();
            if !f.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) {
                continue;
            }
            let mut cabeca = vec![0u8; 4096];
            let n = std::fs::File::open(&f).and_then(|mut x| x.read(&mut cabeca)).unwrap_or(0);
            let t = String::from_utf8_lossy(&cabeca[..n]);
            if t.contains("AuditFile") && t.contains("StandardAuditFile-Tax:AO") {
                let modificado = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%dT%H:%M:%S").to_string())
                    .unwrap_or_default();
                achados.push(json!({ "caminho": f.display().to_string(), "nome": e.file_name().to_string_lossy(), "modificado": modificado }));
            }
        }
    }
    achados
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um SAF-T (AO) pequeno, como o de outro sistema: dois clientes (um deles o consumidor final), um fornecedor,
    /// três artigos (um sem vendas, um isento) e facturas (uma anulada, uma nota de crédito, duas datas).
    const SAFT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AuditFile xmlns="urn:OECD:StandardAuditFile-Tax:AO_1.01_01">
  <Header><AuditFileVersion>1.01_01</AuditFileVersion><CompanyID>5000000001</CompanyID><TaxRegistrationNumber>5000000001</TaxRegistrationNumber>
    <CompanyName>Loja Antiga &amp; Filhos, Lda</CompanyName><StartDate>2026-01-01</StartDate><EndDate>2026-10-31</EndDate><ProductID>OutroSistema/Empresa</ProductID></Header>
  <MasterFiles>
    <Customer><CustomerID>C1</CustomerID><CustomerTaxID>999999999</CustomerTaxID><CompanyName>Consumidor final</CompanyName></Customer>
    <Customer><CustomerID>C2</CustomerID><CustomerTaxID>5417000123</CustomerTaxID><CompanyName>Mercearia Boa Vista</CompanyName>
      <BillingAddress><AddressDetail>Rua 1, Maianga</AddressDetail><City>Luanda</City><Country>AO</Country></BillingAddress>
      <ShipToAddress><AddressDetail>Armazém</AddressDetail><City>Viana</City></ShipToAddress><Telephone>923000111</Telephone></Customer>
    <Supplier><SupplierID>F1</SupplierID><SupplierTaxID>5410000999</SupplierTaxID><CompanyName>Distribuidora Kwanza</CompanyName></Supplier>
    <Product><ProductType>P</ProductType><ProductCode>ARR5</ProductCode><ProductDescription>Arroz 5kg</ProductDescription><ProductNumberCode>5601234567890</ProductNumberCode></Product>
    <Product><ProductType>P</ProductType><ProductCode>LIV1</ProductCode><ProductDescription><![CDATA[Livro «Ensino» & Escola]]></ProductDescription><ProductNumberCode>LIV1</ProductNumberCode></Product>
    <Product><ProductType>S</ProductType><ProductCode>ENT</ProductCode><ProductDescription>Entrega ao domicílio</ProductDescription></Product>
  </MasterFiles>
  <SourceDocuments><SalesInvoices>
    <Invoice><InvoiceNo>FT A/1</InvoiceNo><DocumentStatus><InvoiceStatus>N</InvoiceStatus></DocumentStatus><InvoiceDate>2026-03-01</InvoiceDate><InvoiceType>FT</InvoiceType>
      <Line><ProductCode>ARR5</ProductCode><UnitPrice>900.00</UnitPrice><Tax><TaxType>IVA</TaxType><TaxPercentage>14</TaxPercentage></Tax></Line>
      <Line><ProductCode>LIV1</ProductCode><UnitPrice>2500</UnitPrice><Tax><TaxPercentage>0</TaxPercentage></Tax><TaxExemptionCode>M11</TaxExemptionCode></Line></Invoice>
    <Invoice><InvoiceNo>FR A/9</InvoiceNo><DocumentStatus><InvoiceStatus>N</InvoiceStatus></DocumentStatus><InvoiceDate>2026-09-15</InvoiceDate><InvoiceType>FR</InvoiceType>
      <Line><ProductCode>ARR5</ProductCode><UnitPrice>1000.00</UnitPrice><Tax><TaxPercentage>14</TaxPercentage></Tax></Line></Invoice>
    <Invoice><InvoiceNo>FR A/10</InvoiceNo><DocumentStatus><InvoiceStatus>A</InvoiceStatus></DocumentStatus><InvoiceDate>2026-10-01</InvoiceDate><InvoiceType>FR</InvoiceType>
      <Line><ProductCode>ARR5</ProductCode><UnitPrice>1.00</UnitPrice><Tax><TaxPercentage>14</TaxPercentage></Tax></Line></Invoice>
    <Invoice><InvoiceNo>NC A/1</InvoiceNo><DocumentStatus><InvoiceStatus>N</InvoiceStatus></DocumentStatus><InvoiceDate>2026-10-02</InvoiceDate><InvoiceType>NC</InvoiceType>
      <Line><ProductCode>ARR5</ProductCode><UnitPrice>5.00</UnitPrice><Tax><TaxPercentage>14</TaxPercentage></Tax></Line></Invoice>
  </SalesInvoices></SourceDocuments>
</AuditFile>"#;

    fn ficheiro(nome: &str, conteudo: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("kangest-saft-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("saft.xml");
        std::fs::write(&f, conteudo).unwrap();
        f
    }

    #[test]
    fn le_o_saft_por_fluxo() {
        let f = ficheiro("ler", SAFT);
        let s = ler(&f).unwrap();
        assert_eq!(s.empresa.get("CompanyName"), Some("Loja Antiga & Filhos, Lda"), "entidade &amp; resolvida");
        assert_eq!(s.empresa.get("TaxRegistrationNumber"), Some("5000000001"));
        assert_eq!(s.clientes.len(), 2);
        assert_eq!(s.clientes[1].get("AddressDetail"), Some("Rua 1, Maianga"), "a morada de facturação, não a de entrega");
        assert_eq!(s.fornecedores.len(), 1);
        assert_eq!(s.produtos.len(), 3);
        assert_eq!(s.produtos[1].get("ProductDescription"), Some("Livro «Ensino» & Escola"), "CDATA");
        assert_eq!(s.facturas, 2, "sem a anulada nem a nota de crédito");
        assert_eq!(s.precos["ARR5"], Preco { data: "2026-09-15".into(), preco: 100000, taxa: 14.0, isencao: None }, "o preço mais recente");
        assert_eq!(s.precos["LIV1"].isencao.as_deref(), Some("M11"));
        assert!(!s.precos.contains_key("ENT"), "sem vendas");
        let _ = std::fs::remove_file(&f);
        // um XML que não é SAF-T é recusado
        let g = ficheiro("outro", "<?xml version=\"1.0\"?><Encomenda><Linha/></Encomenda>");
        assert!(ler(&g).unwrap_err().to_string().contains("não é um ficheiro SAF-T"));
        // e o SAF-T de Angola encontra-se numa pasta
        let achados = procurar(&[f.parent().unwrap().to_path_buf(), g.parent().unwrap().to_path_buf()]);
        assert!(achados.is_empty(), "o ficheiro SAF-T foi apagado e o outro não é SAF-T: {achados:?}");
        let h = ficheiro("procurar", SAFT);
        assert_eq!(procurar(&[h.parent().unwrap().to_path_buf()]).len(), 1);
    }

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }

    #[tokio::test]
    async fn importa_simula_e_nao_duplica() {
        let pasta = std::env::temp_dir().join(format!("kangest-saft-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Nova, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        // um artigo que já existe no KanGest, com preço: fica como está; o código de barras em falta preenche-se
        rpc(&st, "artigos.guardar", json!({ "referencia": "ARR5", "descricao": "Arroz 5 kg (KanGest)", "taxa_iva_id": 1, "preco1": 120000 })).await;
        let f = ficheiro("base", SAFT);
        let caminho = f.display().to_string();

        let a = rpc(&st, "importar.analisarSaft", json!({ "caminho": caminho })).await;
        assert_eq!(a["empresa"]["nome"], "Loja Antiga & Filhos, Lda");
        assert_eq!((a["clientes"].as_i64(), a["fornecedores"].as_i64(), a["artigos"].as_i64(), a["com_preco"].as_i64()), (Some(1), Some(1), Some(3), Some(2)));

        // simular: nada fica gravado
        let s = rpc(&st, "importar.executarSaft", json!({ "caminho": caminho, "simular": true })).await;
        assert_eq!(s["simulacao"], true);
        assert_eq!(s["artigos"]["novos"], 2);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artigos").fetch_one(&st.pool).await.unwrap();
        assert_eq!(n, 1, "a simulação desfaz-se");

        let r = rpc(&st, "importar.executarSaft", json!({ "caminho": caminho })).await;
        assert_eq!(r["clientes"]["novos"], 1, "sem o consumidor final");
        assert_eq!(r["fornecedores"]["novos"], 1);
        assert_eq!((r["artigos"]["novos"].as_i64(), r["artigos"]["existentes"].as_i64()), (Some(2), Some(1)));
        assert!(r["avisos"].to_string().contains("sem vendas"), "{}", r["avisos"]);
        let (desc, barras, preco): (String, Option<String>, i64) = sqlx::query_as("SELECT descricao, codigo_barras, preco1 FROM artigos WHERE referencia = 'ARR5'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((desc.as_str(), barras.as_deref(), preco), ("Arroz 5 kg (KanGest)", Some("5601234567890"), 120000), "o que já existia manda");
        let (taxa, motivo, preco, pvp): (i64, Option<String>, i64, i64) =
            sqlx::query_as("SELECT taxa_iva_id, motivo_isencao, preco1, pvp1 FROM artigos WHERE referencia = 'LIV1'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((taxa, motivo.as_deref(), preco, pvp), (2, Some("M11"), 250000, 250000), "isento com o motivo do ficheiro");
        let (tipo, stock): (String, i64) = sqlx::query_as("SELECT tipo, controla_stock FROM artigos WHERE referencia = 'ENT'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((tipo.as_str(), stock), ("S", 0), "serviço sem stock");
        let (nome, morada): (String, Option<String>) = sqlx::query_as("SELECT nome, endereco FROM entidades WHERE nif = '5417000123'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((nome.as_str(), morada.as_deref()), ("Mercearia Boa Vista", Some("Rua 1, Maianga")));

        // outra vez: nada se duplica
        let r2 = rpc(&st, "importar.executarSaft", json!({ "caminho": caminho })).await;
        assert_eq!((r2["clientes"]["novos"].as_i64(), r2["clientes"]["existentes"].as_i64()), (Some(0), Some(1)));
        assert_eq!((r2["artigos"]["novos"].as_i64(), r2["artigos"]["existentes"].as_i64()), (Some(0), Some(3)));
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entidades WHERE nif = '5417000123'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn valores_em_centimos() {
        assert_eq!(centimos("1000.00"), Some(100000));
        assert_eq!(centimos("12.5"), Some(1250));
        assert_eq!(centimos(" 0.995 "), Some(100));
        assert_eq!(centimos("abc"), None);
    }
}

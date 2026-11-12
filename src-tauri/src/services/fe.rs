//! Facturação Electrónica da AGT (Decreto Presidencial 71/25), serviços REST `sigt/fe/v1`
//! documentados em quiosqueagt.minfin.gov.ao/doc-agt.
//!
//! - registarFactura: envia até 30 documentos por pedido; devolve `requestID` (validação assíncrona).
//! - obterEstado: resultado da validação por documento (V válido / I inválido).
//! - solicitarSerie / listarSeries: séries de numeração atribuídas pela AGT.
//!
//! Assinaturas JWS RS256 (compactas, `header.payload.assinatura`, Base64URL sem `=`):
//! - jwsSoftwareSignature: chave do produtor do software sobre `softwareInfoDetail`;
//! - jwsDocumentSignature: chave do contribuinte (emitida pela AGT) sobre os campos do documento;
//! - jwsSignature: chave do contribuinte sobre os campos do pedido (obterEstado, séries).
//!
//! Autenticação HTTP Basic com as credenciais dadas pela AGT ao produtor do software.
//! Os documentos pendentes ficam em fila (`fe_envios`) e são enviados quando há ligação
//! (contingência): nada bloqueia a facturação se a AGT não responder.

use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::{Row, SqlitePool};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::http;
use crate::state::AppState;

/// Documentos comunicados (vendas e autofacturação; os recibos levam `paymentReceipt`).
pub const TIPOS_FE: [&str; 6] = ["FT", "FR", "NC", "ND", "AF", "RC"];
const SCHEMA: &str = "2.0";
const MAX_LOTE: usize = 30;

// ------------------------------------------------------------------ configuração

pub struct Cfg {
    pub ativo: bool,
    pub ambiente: String,
    pub desde: String,
    utilizador: String,
    senha: String,
    pub numero_validacao: String,
    pub produto: String,
    pub estabelecimento: String,
    chave_software: Option<String>,
    chave_contribuinte: Option<String>,
    pub nif: String,
    /// Endereço alternativo dos serviços (testes); vazio = endereços oficiais da AGT.
    endereco: String,
}

async fn valor(p: &SqlitePool, k: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(p).await?)
}

pub async fn cfg(p: &SqlitePool) -> AppResult<Cfg> {
    let g = |k: &'static str| async move { valor(p, k).await.map(|v| v.unwrap_or_default()) };
    let nif: Option<String> = sqlx::query_scalar("SELECT nif FROM empresa WHERE id = 1").fetch_optional(p).await?;
    Ok(Cfg {
        ativo: g("fe.ativo").await? == "1",
        ambiente: Some(g("fe.ambiente").await?).filter(|a| a == "PROD").unwrap_or_else(|| "HML".into()),
        desde: g("fe.desde").await?,
        utilizador: g("fe.utilizador").await?,
        senha: g("fe.senha").await?,
        numero_validacao: g("fe.numero_validacao").await?,
        produto: Some(g("fe.produto").await?).filter(|x| !x.is_empty()).unwrap_or_else(|| "KanGest".into()),
        estabelecimento: Some(g("fe.estabelecimento").await?).filter(|x| !x.is_empty()).unwrap_or_else(|| "SEDE".into()),
        chave_software: valor(p, "fe.chave_software").await?.filter(|x| !x.is_empty()),
        chave_contribuinte: valor(p, "fe.chave_contribuinte").await?.filter(|x| !x.is_empty()),
        nif: nif.unwrap_or_default(),
        endereco: g("fe.endereco").await?,
    })
}

/// Facturação electrónica ligada (as séries dos tipos comunicados têm de ser da AGT).
pub async fn ativo(p: &SqlitePool) -> AppResult<bool> {
    Ok(valor(p, "fe.ativo").await?.as_deref() == Some("1"))
}

fn base_url(ambiente: &str) -> &'static str {
    if ambiente == "PROD" {
        "https://sifp.minfin.gov.ao/sigt/fe/v1"
    } else {
        "https://sifphml.minfin.gov.ao/sigt/fe/v1"
    }
}

pub fn chave_pem(pem: &str) -> AppResult<RsaPrivateKey> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    let k = RsaPrivateKey::from_pkcs8_pem(pem.trim())
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem.trim()))
        .map_err(|_| AppError::msg("Chave privada inválida (PEM PKCS#8 ou PKCS#1)."))?;
    use rsa::traits::PublicKeyParts;
    if k.size() * 8 < 2048 {
        return Err(AppError::msg("A chave RSA tem de ter pelo menos 2048 bits."));
    }
    Ok(k)
}

// ------------------------------------------------------------------ JWS e JSON

/// JWS compacto RS256 sobre o texto JSON exacto (`payload`).
pub fn jws(chave: &RsaPrivateKey, payload: &str) -> String {
    let h = B64URL.encode(br#"{"typ":"JOSE","alg":"RS256"}"#);
    let p = B64URL.encode(payload.as_bytes());
    let entrada = format!("{h}.{p}");
    let sig = SigningKey::<Sha256>::new(chave.clone()).sign(entrada.as_bytes());
    format!("{entrada}.{}", B64URL.encode(sig.to_bytes()))
}

/// Cêntimos → número JSON mínimo (570 / 570.5 / 570.55), como no exemplo da AGT.
pub fn num(c: i64) -> String {
    if c % 100 == 0 {
        format!("{}", c / 100)
    } else {
        let s = format!("{:.2}", c as f64 / 100.0);
        s.trim_end_matches('0').to_string()
    }
}
fn numv(c: i64) -> Value {
    serde_json::from_str(&num(c)).unwrap_or(json!(0))
}
fn q(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}
fn qtdv(x: f64) -> Value {
    let s = format!("{x:.6}");
    serde_json::from_str(s.trim_end_matches('0').trim_end_matches('.')).unwrap_or(json!(0))
}

fn uuid_v4() -> String {
    let mut b: [u8; 16] = rand::random();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

fn agora_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

struct Assinador {
    software: RsaPrivateKey,
    contribuinte: RsaPrivateKey,
}

fn assinador(c: &Cfg) -> AppResult<Assinador> {
    let s = c.chave_software.as_deref().ok_or_else(|| AppError::msg("Falta a chave privada do produtor do software (Facturação Electrónica)."))?;
    let k = c.chave_contribuinte.as_deref().ok_or_else(|| AppError::msg("Falta a chave privada do contribuinte emitida pela AGT (Facturação Electrónica)."))?;
    Ok(Assinador { software: chave_pem(s)?, contribuinte: chave_pem(k)? })
}

fn software_info(c: &Cfg, a: &Assinador) -> Value {
    let det = format!(
        "{{\"productId\":{},\"productVersion\":{},\"softwareValidationNumber\":{}}}",
        q(&c.produto),
        q(env!("CARGO_PKG_VERSION")),
        q(&c.numero_validacao)
    );
    json!({
        "softwareInfoDetail": {
            "productId": c.produto,
            "productVersion": env!("CARGO_PKG_VERSION"),
            "softwareValidationNumber": c.numero_validacao,
        },
        "jwsSoftwareSignature": jws(&a.software, &det),
    })
}

/// Campos assinados do documento, pela ordem da especificação.
pub fn payload_documento(doc_no: &str, nif: &str, tipo: &str, data: &str, cliente_nif: &str, pais: &str, nome: &str, iva: i64, liquido: i64, total: i64) -> String {
    format!(
        "{{\"documentNo\":{},\"taxRegistrationNumber\":{},\"documentType\":{},\"documentDate\":{},\"customerTaxID\":{},\"customerCountry\":{},\"companyName\":{},\"documentTotals\":{{\"taxPayable\":{},\"netTotal\":{},\"grossTotal\":{}}}}}",
        q(doc_no),
        q(nif),
        q(tipo),
        q(data),
        q(cliente_nif),
        q(pais),
        q(nome),
        num(iva),
        num(liquido),
        num(total)
    )
}

// ------------------------------------------------------------------ documento → JSON da AGT

fn retencao_agt(codigo: &str) -> &'static str {
    match codigo {
        "IRT" => "IRT",
        "II" => "II",
        "IS" => "IS",
        "IVA" => "IVA",
        "IPU" | "IP" => "IP",
        "IAC" => "IAC",
        _ => "OU",
    }
}

async fn montar(p: &SqlitePool, c: &Cfg, a: &Assinador, doc_id: i64, corrige: Option<&str>) -> AppResult<(String, Value)> {
    let d = sqlx::query(
        "SELECT d.*, COALESCE(e.pais, 'AO') AS pais, o.numero_doc AS origem_no
         FROM documentos d LEFT JOIN entidades e ON e.id = d.entidade_id LEFT JOIN documentos o ON o.id = d.doc_origem_id
         WHERE d.id = ?",
    )
    .bind(doc_id)
    .fetch_optional(p)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let tipo: String = d.get("tipo_codigo");
    let doc_no: String = d.get("numero_doc");
    let data: String = d.get("data_emissao");
    let pais_raw: String = d.get("pais");
    let pais = if pais_raw.len() == 2 { pais_raw.to_ascii_uppercase() } else { "AO".to_string() };
    let cliente_nif: String = Some(d.get::<String, _>("entidade_nif")).filter(|x| !x.trim().is_empty()).unwrap_or_else(|| "999999999".into());
    let nome: String = Some(d.get::<String, _>("entidade_nome")).filter(|x| !x.trim().is_empty()).unwrap_or_else(|| "Consumidor Final".into());
    let recibo = tipo == "RC";
    let (iva, liquido, total): (i64, i64, i64) = if recibo { (0, d.get("total"), d.get("total")) } else { (d.get("total_iva"), d.get("total_liquido"), d.get("total")) };
    let assinatura = jws(&a.contribuinte, &payload_documento(&doc_no, &c.nif, &tipo, &data, &cliente_nif, &pais, &nome, iva, liquido, total));
    let data_hora: String = d.get("data_hora");
    let entrada: String = data_hora.replace(' ', "T").chars().take(19).collect();

    let mut doc = json!({
        "documentNo": doc_no,
        "documentStatus": if corrige.is_some() { "C" } else { "N" },
        "jwsDocumentSignature": assinatura,
        "documentDate": data,
        "documentType": tipo,
        "systemEntryDate": entrada,
        "customerCountry": pais,
        "customerTaxID": cliente_nif,
        "companyName": nome,
    });
    if let Some(r) = corrige {
        doc["rejectedDocumentNo"] = json!(r);
    }
    if recibo {
        let liq = sqlx::query("SELECT o.numero_doc, o.data_emissao, r.valor FROM recibo_documentos r JOIN documentos o ON o.id = r.documento_id WHERE r.recibo_id = ? ORDER BY r.id")
            .bind(doc_id)
            .fetch_all(p)
            .await?;
        let fontes: Vec<Value> = liq
            .iter()
            .enumerate()
            .map(|(i, r)| {
                json!({
                    "lineNo": i + 1,
                    "sourceDocumentID": { "OriginatingON": r.get::<String, _>("numero_doc"), "documentDate": r.get::<String, _>("data_emissao") },
                    "creditAmount": numv(r.get::<i64, _>("valor")),
                })
            })
            .collect();
        doc["paymentReceipt"] = json!({ "sourceDocuments": fontes });
    } else {
        let nc = tipo == "NC";
        let linhas = sqlx::query(
            "SELECT l.*, COALESCE(a.tipo, 'P') AS artigo_tipo, t.codigo AS taxa_codigo
             FROM documento_linhas l LEFT JOIN artigos a ON a.id = l.artigo_id LEFT JOIN taxas_iva t ON t.id = l.taxa_iva_id
             WHERE l.documento_id = ? ORDER BY l.linha, l.id",
        )
        .bind(doc_id)
        .fetch_all(p)
        .await?;
        let origem: Option<String> = d.get("origem_no");
        let motivo: Option<String> = d.get("motivo");
        let mut out = Vec::new();
        for (i, l) in linhas.iter().enumerate() {
            let qtd: f64 = l.get("quantidade");
            let base: i64 = l.get("preco_unitario");
            let liquido_l: i64 = l.get("valor_liquido");
            let desconto: i64 = l.get("valor_desconto");
            let unit = if qtd.abs() > 1e-9 { (liquido_l as f64 / qtd).round() as i64 } else { base };
            let pct: f64 = l.get("taxa_iva_pct");
            let motivo_isencao: Option<String> = l.get("motivo_isencao");
            let codigo_taxa = if pct == 0.0 { "ISE".to_string() } else { l.get::<Option<String>, _>("taxa_codigo").filter(|x| ["NOR", "INT", "RED", "OUT"].contains(&x.as_str())).unwrap_or_else(|| "NOR".into()) };
            let mut imposto = json!({
                "taxType": "IVA",
                "taxCountryRegion": "AO",
                "taxCode": codigo_taxa,
                "taxPercentage": qtdv(pct),
                "taxContribution": numv(l.get::<i64, _>("valor_iva")),
            });
            if pct == 0.0 {
                imposto["taxExemptionCode"] = json!(motivo_isencao.unwrap_or_else(|| "M00".into()));
            }
            let mut linha = json!({
                "lineNumber": i + 1,
                "operationType": if l.get::<String, _>("artigo_tipo") == "S" { "SG" } else { "TB" },
                "productCode": l.get::<String, _>("referencia"),
                "productDescription": l.get::<String, _>("descricao").chars().take(200).collect::<String>(),
                "quantity": qtdv(qtd),
                "unitOfMeasure": l.get::<Option<String>, _>("unidade").filter(|x| !x.is_empty()).unwrap_or_else(|| "UN".into()),
                "unitPriceBase": numv(base),
                "unitPrice": numv(unit),
                "settlementAmount": numv(desconto),
                "taxes": [imposto],
            });
            linha[if nc { "debitAmount" } else { "creditAmount" }] = numv(liquido_l);
            if nc {
                linha["referenceInfo"] = json!({ "reference": origem.clone().unwrap_or_default(), "reason": motivo.clone().unwrap_or_default().chars().take(60).collect::<String>() });
            }
            out.push(linha);
        }
        doc["lines"] = json!(out);
    }
    let mut totais = json!({ "taxPayable": numv(iva), "netTotal": numv(liquido), "grossTotal": numv(total) });
    let moeda: Option<String> = d.try_get("moeda").ok();
    if let Some(m) = moeda.filter(|m| !m.is_empty() && m != "AOA") {
        let tm: Option<i64> = d.try_get("total_moeda").ok().flatten();
        let cambio: f64 = d.try_get("cambio").unwrap_or(1.0);
        if let Some(tm) = tm {
            totais["currency"] = json!({ "currencyCode": m, "currencyAmount": numv(tm), "exchangeRate": qtdv(cambio) });
        }
    }
    doc["documentTotals"] = totais;
    let ret = sqlx::query("SELECT codigo, descricao, valor FROM documento_retencoes WHERE documento_id = ?").bind(doc_id).fetch_all(p).await?;
    if !ret.is_empty() {
        doc["withholdingTaxList"] = json!(ret
            .iter()
            .map(|r| json!({
                "withholdingTaxType": retencao_agt(&r.get::<String, _>("codigo")),
                "withholdingTaxDescription": r.get::<String, _>("descricao").chars().take(120).collect::<String>(),
                "withholdingTaxAmount": numv(r.get::<i64, _>("valor")),
            }))
            .collect::<Vec<_>>());
    }
    Ok((doc_no, doc))
}

// ------------------------------------------------------------------ chamadas à AGT

fn chamar(c: &Cfg, servico: &str, corpo: &Value) -> AppResult<(u16, Value)> {
    use base64::engine::general_purpose::STANDARD;
    if c.utilizador.is_empty() {
        return Err(AppError::msg("Faltam as credenciais da AGT para a Facturação Electrónica."));
    }
    let base = if c.endereco.is_empty() { base_url(&c.ambiente).to_string() } else { c.endereco.trim_end_matches('/').to_string() };
    let url = format!("{base}/{servico}");
    let auth = format!("Basic {}", STANDARD.encode(format!("{}:{}", c.utilizador, c.senha)));
    let corpo = serde_json::to_vec(corpo)?;
    let r = http::pedido(
        "POST",
        &url,
        &[("Authorization", auth), ("Content-Type", "application/json".into()), ("Accept", "application/json".into())],
        Some(&corpo),
        Duration::from_secs(60),
    )?;
    let v: Value = serde_json::from_slice(&r.corpo).unwrap_or_else(|_| json!({ "texto": r.texto().chars().take(500).collect::<String>() }));
    Ok((r.estado, v))
}

fn erros_de(v: &Value) -> Vec<(Option<String>, String)> {
    let mut out = Vec::new();
    for chave in ["errorList", "requestErrorList"] {
        if let Some(a) = v.get(chave).and_then(|x| x.as_array()) {
            for e in a {
                let cod = e.get("idError").or_else(|| e.get("errorCode")).and_then(|x| x.as_str()).unwrap_or("");
                let desc = e.get("descriptionError").or_else(|| e.get("errorDescription")).and_then(|x| x.as_str()).unwrap_or("");
                if cod.is_empty() && desc.is_empty() {
                    continue;
                }
                out.push((e.get("documentNo").and_then(|x| x.as_str()).map(String::from), format!("{cod} {desc}").trim().to_string()));
            }
        }
    }
    out
}

/// Um ciclo de cada vez (a tarefa de fundo e o botão «Enviar agora» não enviam o mesmo lote duas vezes).
static CICLO: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Envia os pendentes (lotes de 30) e consulta os enviados. Devolve um resumo.
pub async fn ciclo(p: &SqlitePool) -> AppResult<Value> {
    let _vez = CICLO.lock().await;
    let c = cfg(p).await?;
    if !c.ativo {
        return Ok(json!({ "ativo": false }));
    }
    // documentos novos entram na fila
    if !c.desde.is_empty() {
        let tipos = TIPOS_FE.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(",");
        sqlx::query(&format!(
            "INSERT OR IGNORE INTO fe_envios (documento_id, ambiente)
             SELECT d.id, ? FROM documentos d
             WHERE d.tipo_codigo IN ({tipos}) AND d.importado = 0 AND d.data_emissao >= ?
               AND NOT EXISTS (SELECT 1 FROM fe_envios f WHERE f.documento_id = d.id)"
        ))
        .bind(&c.ambiente)
        .bind(&c.desde)
        .execute(p)
        .await?;
    }
    let a = assinador(&c)?;
    let mut enviados = 0usize;
    let mut avisos: Vec<String> = Vec::new();
    // no máximo 20 lotes (600 documentos) por ciclo; o resto segue no próximo
    for _ in 0..20 {
        let fila = sqlx::query("SELECT documento_id, corrige FROM fe_envios WHERE estado = 'PENDENTE' ORDER BY documento_id LIMIT ?").bind(MAX_LOTE as i64).fetch_all(p).await?;
        if fila.is_empty() {
            break;
        }
        let mut docs = Vec::new();
        let mut ids = Vec::new();
        for r in &fila {
            let id: i64 = r.get("documento_id");
            let corrige: Option<String> = r.get("corrige");
            match montar(p, &c, &a, id, corrige.as_deref()).await {
                Ok((no, v)) => {
                    docs.push(v);
                    ids.push((id, no));
                }
                Err(e) => {
                    sqlx::query("UPDATE fe_envios SET estado = 'INVALIDO', erros = ? WHERE documento_id = ?").bind(json!([format!("Local: {e}")]).to_string()).bind(id).execute(p).await?;
                }
            }
        }
        if docs.is_empty() {
            continue;
        }
        let corpo = json!({
            "schemaVersion": SCHEMA,
            "submissionUUID": uuid_v4(),
            "taxRegistrationNumber": c.nif,
            "submissionTimeStamp": agora_utc(),
            "softwareInfo": software_info(&c, &a),
            "numberOfEntries": docs.len(),
            "documents": docs,
        });
        let c2 = cfg(p).await?;
        let resp = tokio::task::spawn_blocking(move || chamar(&c2, "registarFactura", &corpo)).await.map_err(|e| AppError::msg(e.to_string()))?;
        match resp {
            Ok((200, v)) => {
                let req = v.get("requestID").and_then(|x| x.as_str().map(String::from).or_else(|| x.as_i64().map(|n| n.to_string()))).unwrap_or_default();
                if req.is_empty() {
                    // sem n.º de pedido não há como consultar a validação: os documentos ficam em fila
                    let msg = format!("A AGT respondeu sem n.º de pedido: {}", v.to_string().chars().take(200).collect::<String>());
                    for (id, _) in &ids {
                        sqlx::query("UPDATE fe_envios SET erros = ?, tentativas = tentativas + 1 WHERE documento_id = ?").bind(json!([msg]).to_string()).bind(id).execute(p).await?;
                    }
                    avisos.push(msg);
                    break;
                }
                for (id, _) in &ids {
                    sqlx::query("UPDATE fe_envios SET estado = 'ENVIADO', request_id = ?, enviado_em = ?, tentativas = tentativas + 1, erros = NULL WHERE documento_id = ?")
                        .bind(&req)
                        .bind(db::agora())
                        .bind(id)
                        .execute(p)
                        .await?;
                }
                enviados += ids.len();
            }
            Ok((estado, v)) => {
                let erros = erros_de(&v);
                let gerais: Vec<String> = erros.iter().filter(|e| e.0.is_none()).map(|e| e.1.clone()).collect();
                let mut rejeitados = 0;
                for (id, no) in &ids {
                    let deste: Vec<String> = erros.iter().filter(|e| e.0.as_deref() == Some(no.as_str())).map(|e| e.1.clone()).collect();
                    if !deste.is_empty() {
                        rejeitados += 1;
                        sqlx::query("UPDATE fe_envios SET estado = 'INVALIDO', erros = ?, tentativas = tentativas + 1 WHERE documento_id = ?").bind(json!(deste).to_string()).bind(id).execute(p).await?;
                    } else {
                        let msg = if gerais.is_empty() { vec![format!("HTTP {estado}: {}", v.to_string().chars().take(300).collect::<String>())] } else { gerais.clone() };
                        sqlx::query("UPDATE fe_envios SET erros = ?, tentativas = tentativas + 1 WHERE documento_id = ?").bind(json!(msg).to_string()).bind(id).execute(p).await?;
                    }
                }
                avisos.push(format!(
                    "A AGT recusou o pedido (HTTP {estado}){}",
                    gerais.first().or(erros.first().map(|e| &e.1)).map(|g| format!(": {g}")).unwrap_or_default()
                ));
                if rejeitados == 0 {
                    break; // erro geral (credenciais, certificação) ou que não identifica documentos: não insistir neste ciclo
                }
            }
            Err(e) => {
                // sem ligação: contingência, os documentos ficam em fila
                for (id, _) in &ids {
                    sqlx::query("UPDATE fe_envios SET erros = ?, tentativas = tentativas + 1 WHERE documento_id = ?").bind(json!([e.to_string()]).to_string()).bind(id).execute(p).await?;
                }
                avisos.push(e.to_string());
                break;
            }
        }
    }
    // consulta dos pedidos enviados
    let pedidos: Vec<String> = sqlx::query_scalar("SELECT DISTINCT request_id FROM fe_envios WHERE estado = 'ENVIADO' AND request_id IS NOT NULL AND request_id <> '' LIMIT 20").fetch_all(p).await?;
    let mut validados = 0usize;
    for req in pedidos {
        let assinado = format!("{{\"taxRegistrationNumber\":{},\"requestID\":{}}}", q(&c.nif), q(&req));
        let corpo = json!({
            "schemaVersion": SCHEMA,
            "submissionUUID": uuid_v4(),
            "taxRegistrationNumber": c.nif,
            "submissionTimeStamp": agora_utc(),
            "softwareInfo": software_info(&c, &a),
            "jwsSignature": jws(&a.contribuinte, &assinado),
            "requestID": req,
        });
        let c2 = cfg(p).await?;
        let r = tokio::task::spawn_blocking(move || chamar(&c2, "obterEstado", &corpo)).await.map_err(|e| AppError::msg(e.to_string()))?;
        let Ok((200, v)) = r else { continue };
        let codigo = v.get("resultCode").and_then(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))).unwrap_or(8);
        match codigo {
            0..=2 => {
                for e in v.get("documentStatusList").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                    let no = e.get("documentNo").and_then(|x| x.as_str()).unwrap_or("");
                    let erros: Vec<String> = erros_de(&e).into_iter().map(|x| x.1).collect();
                    // E09: a factura já existe na AGT (validada noutro pedido) — conta como válida
                    let ja_existe = erros.iter().any(|x| x.starts_with("E09"));
                    let valido = e.get("documentStatus").and_then(|x| x.as_str()) == Some("V") || ja_existe;
                    let erros = if ja_existe { Vec::new() } else { erros };
                    sqlx::query(
                        "UPDATE fe_envios SET estado = ?, erros = ?, verificado_em = ?
                         WHERE request_id = ? AND documento_id = (SELECT id FROM documentos WHERE numero_doc = ?)",
                    )
                    .bind(if valido { "VALIDO" } else { "INVALIDO" })
                    .bind(if erros.is_empty() { None } else { Some(json!(erros).to_string()) })
                    .bind(db::agora())
                    .bind(&req)
                    .bind(no)
                    .execute(p)
                    .await?;
                    if valido {
                        validados += 1;
                    }
                }
            }
            9 => {
                sqlx::query("UPDATE fe_envios SET estado = 'PENDENTE', request_id = NULL WHERE request_id = ?").bind(&req).execute(p).await?;
            }
            _ => {} // 7 prematuro / 8 em processamento: volta a consultar no próximo ciclo
        }
    }
    let contagem = db::query_json(p, "SELECT estado, COUNT(*) AS n FROM fe_envios GROUP BY estado").await?;
    Ok(json!({ "ativo": true, "enviados": enviados, "validados": validados, "avisos": avisos, "contagem": contagem }))
}

/// Tarefa de fundo: um ciclo a cada 2 minutos (não corre nos postos cliente).
pub fn iniciar(st: std::sync::Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(120)).await;
            if crate::rede::config().cliente() {
                continue;
            }
            if let Err(e) = ciclo(&st.pool).await {
                eprintln!("Facturação electrónica: {e}");
            }
        }
    });
}

// ------------------------------------------------------------------ RPC

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    let c = cfg(&st.pool).await?;
    Ok(json!({
        "ativo": c.ativo,
        "ambiente": c.ambiente,
        "desde": c.desde,
        "utilizador": c.utilizador,
        "senha": !c.senha.is_empty(),
        "numero_validacao": c.numero_validacao,
        "produto": c.produto,
        "versao": env!("CARGO_PKG_VERSION"),
        "estabelecimento": c.estabelecimento,
        "chave_software": c.chave_software.is_some(),
        "chave_contribuinte": c.chave_contribuinte.is_some(),
        "nif": c.nif,
        "endereco": base_url(&c.ambiente),
        "contagem": db::query_json(&st.pool, "SELECT estado, COUNT(*) AS n FROM fe_envios GROUP BY estado").await?,
        "series_agt": db::query_json(&st.pool, "SELECT tipo_codigo, serie, ativa, padrao FROM series WHERE fe_agt = 1 ORDER BY tipo_codigo, serie").await?,
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    ativo: bool,
    ambiente: String,
    utilizador: String,
    #[serde(default)]
    senha: Option<String>,
    numero_validacao: String,
    #[serde(default)]
    produto: Option<String>,
    estabelecimento: String,
    #[serde(default)]
    chave_software: Option<String>,
    #[serde(default)]
    chave_contribuinte: Option<String>,
}

pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.fe")?;
    if g.ambiente != "HML" && g.ambiente != "PROD" {
        return Err(AppError::msg("Ambiente inválido (homologação ou produção)."));
    }
    for k in [&g.chave_software, &g.chave_contribuinte].into_iter().flatten() {
        if !k.trim().is_empty() {
            chave_pem(k)?;
        }
    }
    let mut pares: Vec<(&str, String)> = vec![
        ("fe.ambiente", g.ambiente.clone()),
        ("fe.utilizador", g.utilizador.trim().to_string()),
        ("fe.numero_validacao", g.numero_validacao.trim().to_string()),
        ("fe.estabelecimento", Some(g.estabelecimento.trim().to_string()).filter(|x| !x.is_empty()).unwrap_or_else(|| "SEDE".into())),
    ];
    if let Some(p) = g.produto.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        pares.push(("fe.produto", p.to_string()));
    }
    if let Some(x) = g.senha.filter(|x| !x.is_empty()) {
        pares.push(("fe.senha", x));
    }
    if let Some(x) = g.chave_software.map(|x| x.trim().to_string()).filter(|x| !x.is_empty()) {
        pares.push(("fe.chave_software", x));
    }
    if let Some(x) = g.chave_contribuinte.map(|x| x.trim().to_string()).filter(|x| !x.is_empty()) {
        pares.push(("fe.chave_contribuinte", x));
    }
    let atual = cfg(&st.pool).await?;
    if g.ativo {
        // activar exige tudo o que a AGT pede
        let tem = |k: &str, atual: bool| atual || pares.iter().any(|(c, v)| *c == k && !v.is_empty());
        if g.utilizador.trim().is_empty() || !tem("fe.senha", !atual.senha.is_empty()) {
            return Err(AppError::msg("Indique o utilizador e a senha dados pela AGT ao produtor do software."));
        }
        if g.numero_validacao.trim().is_empty() {
            return Err(AppError::msg("Indique o número de validação (certificação) do software para a Facturação Electrónica."));
        }
        if !tem("fe.chave_software", atual.chave_software.is_some()) || !tem("fe.chave_contribuinte", atual.chave_contribuinte.is_some()) {
            return Err(AppError::msg("Carregue a chave do produtor do software e a chave do contribuinte emitida pela AGT."));
        }
        if atual.desde.is_empty() {
            pares.push(("fe.desde", db::hoje()));
        }
    }
    pares.push(("fe.ativo", if g.ativo { "1".into() } else { "0".into() }));
    let mut tx = st.pool.begin().await?;
    for (k, v) in &pares {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'FE_CONFIG', ?)")
        .bind(s.id)
        .bind(format!("Facturação electrónica {} ({})", if g.ativo { "ligada" } else { "desligada" }, g.ambiente))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Value::Null)
}

pub async fn processar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    ciclo(&st.pool).await
}

#[derive(Deserialize)]
pub struct EnviosIn {
    #[serde(default)]
    estado: Option<String>,
}

pub async fn envios(st: &AppState, f: EnviosIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    Ok(db::rows_json(
        &sqlx::query(
            "SELECT f.*, d.numero_doc, d.data_emissao, d.entidade_nome, d.total, d.estado AS estado_doc
             FROM fe_envios f JOIN documentos d ON d.id = f.documento_id
             WHERE (?1 IS NULL OR f.estado = ?1) ORDER BY f.documento_id DESC LIMIT 500",
        )
        .bind(f.estado.filter(|x| !x.is_empty()))
        .fetch_all(&st.pool)
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct IdIn {
    documento_id: i64,
}

/// Volta a pôr em fila um documento que não chegou a ser aceite pelo pedido (erro geral ou sem ligação).
pub async fn reenviar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    let n = sqlx::query("UPDATE fe_envios SET estado = 'PENDENTE', request_id = NULL WHERE documento_id = ? AND estado IN ('PENDENTE', 'ENVIADO')")
        .bind(i.documento_id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::msg("Só se reenviam documentos pendentes. Um documento rejeitado corrige-se com um documento novo («Corrigir»)."));
    }
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct CorrigirIn {
    /// documento rejeitado pela AGT
    rejeitado_id: i64,
    /// documento novo que o substitui (outro número)
    numero_novo: String,
}

/// Associa um documento novo como correcção de um rejeitado (documentStatus C + rejectedDocumentNo).
pub async fn corrigir(st: &AppState, c: CorrigirIn) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    let rej: (String, String) = sqlx::query_as("SELECT d.numero_doc, f.estado FROM fe_envios f JOIN documentos d ON d.id = f.documento_id WHERE f.documento_id = ?")
        .bind(c.rejeitado_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if rej.1 != "INVALIDO" {
        return Err(AppError::msg("Só os documentos rejeitados pela AGT se corrigem."));
    }
    let novo: i64 = sqlx::query_scalar("SELECT id FROM documentos WHERE numero_doc = ?")
        .bind(c.numero_novo.trim())
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::msg(format!("O documento {} não existe.", c.numero_novo.trim())))?;
    if novo == c.rejeitado_id {
        return Err(AppError::msg("A correcção tem de ser um documento novo, com outro número."));
    }
    sqlx::query(
        "INSERT INTO fe_envios (documento_id, corrige, ambiente) VALUES (?1, ?2, (SELECT valor FROM config WHERE chave = 'fe.ambiente'))
         ON CONFLICT(documento_id) DO UPDATE SET corrige = ?2, estado = CASE WHEN estado IN ('PENDENTE','INVALIDO') THEN 'PENDENTE' ELSE estado END",
    )
    .bind(novo)
    .bind(&rej.0)
    .execute(&st.pool)
    .await?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct SerieIn {
    tipo: String,
    ano: i64,
    #[serde(default)]
    contingencia: bool,
}

/// Pede à AGT uma série nova e regista-a como série por defeito do tipo.
pub async fn solicitar_serie(st: &AppState, s: SerieIn) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    if !TIPOS_FE.contains(&s.tipo.as_str()) {
        return Err(AppError::msg("Esse tipo de documento não é comunicado à AGT."));
    }
    let c = cfg(&st.pool).await?;
    let a = assinador(&c)?;
    let assinado = format!(
        "{{\"taxRegistrationNumber\":{},\"establishmentNumber\":{},\"seriesYear\":{},\"documentType\":{}}}",
        q(&c.nif),
        q(&c.estabelecimento),
        s.ano,
        q(&s.tipo)
    );
    let corpo = json!({
        "schemaVersion": SCHEMA,
        "submissionUUID": uuid_v4(),
        "taxRegistrationNumber": c.nif,
        "submissionTimeStamp": agora_utc(),
        "softwareInfo": software_info(&c, &a),
        "seriesYear": s.ano,
        "documentType": s.tipo,
        "establishmentNumber": c.estabelecimento,
        "seriesContingencyIndicator": if s.contingencia { "C" } else { "N" },
        "jwsSignature": jws(&a.contribuinte, &assinado),
    });
    let (estado, v) = tokio::task::spawn_blocking(move || chamar(&c, "solicitarSerie", &corpo)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let codigo = v.pointer("/seriesFEResult/seriesCode").and_then(|x| x.as_str()).map(String::from);
    let Some(codigo) = codigo.filter(|_| estado == 200) else {
        let e = erros_de(&v).into_iter().map(|x| x.1).collect::<Vec<_>>().join("; ");
        return Err(AppError::msg(format!("A AGT não criou a série (HTTP {estado}){}", if e.is_empty() { String::new() } else { format!(": {e}") })));
    };
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE series SET padrao = 0 WHERE tipo_codigo = ?").bind(&s.tipo).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO series (tipo_codigo, serie, descricao, data_inicio, data_fim, padrao, fe_agt) VALUES (?1, ?2, ?3, ?4, ?5, 1, 1)
         ON CONFLICT(tipo_codigo, serie) DO UPDATE SET padrao = 1, fe_agt = 1, ativa = 1",
    )
    .bind(&s.tipo)
    .bind(&codigo)
    .bind(format!("Série AGT {}{}", s.ano, if s.contingencia { " (contingência)" } else { "" }))
    .bind(format!("{}-01-01", s.ano))
    .bind(format!("{}-12-31", s.ano))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(json!({ "serie": codigo, "resultado": v.get("seriesFEResult") }))
}

pub async fn listar_series(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.fe")?;
    let c = cfg(&st.pool).await?;
    let a = assinador(&c)?;
    let corpo = json!({
        "schemaVersion": SCHEMA,
        "taxRegistrationNumber": c.nif,
        "submissionTimeStamp": agora_utc(),
        "softwareInfo": software_info(&c, &a),
        "establishmentNumber": c.estabelecimento,
        "jwsSignature": jws(&a.contribuinte, &format!("{{\"taxRegistrationNumber\":{}}}", q(&c.nif))),
    });
    let (estado, v) = tokio::task::spawn_blocking(move || chamar(&c, "listarSeries", &corpo)).await.map_err(|e| AppError::msg(e.to_string()))??;
    if estado != 200 {
        let e = erros_de(&v).into_iter().map(|x| x.1).collect::<Vec<_>>().join("; ");
        return Err(AppError::msg(format!("A AGT respondeu HTTP {estado}{}", if e.is_empty() { String::new() } else { format!(": {e}") })));
    }
    let lista = v.pointer("/seriesListResult/seriesInfo").or_else(|| v.get("seriesInfo")).or_else(|| v.get("seriresInfo")).cloned().unwrap_or(json!([]));
    Ok(lista)
}

#[cfg(test)]
mod testes {
    use super::*;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    #[test]
    fn numeros_e_payload() {
        assert_eq!(num(57000), "570");
        assert_eq!(num(57050), "570.5");
        assert_eq!(num(57055), "570.55");
        assert_eq!(num(-1234), "-12.34");
        assert_eq!(qtdv(2.0), json!(2));
        assert_eq!(qtdv(0.125), json!(0.125));
        // payload pela ordem da especificação, números mínimos como no exemplo da AGT
        assert_eq!(
            payload_documento("FT FT6325S2C/10006", "5001636863", "FT", "2025-09-02", "999999999", "AO", "Cliente Exemplo Lda", 7000, 50000, 57000),
            r#"{"documentNo":"FT FT6325S2C/10006","taxRegistrationNumber":"5001636863","documentType":"FT","documentDate":"2025-09-02","customerTaxID":"999999999","customerCountry":"AO","companyName":"Cliente Exemplo Lda","documentTotals":{"taxPayable":70,"netTotal":500,"grossTotal":570}}"#
        );
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
    }

    #[test]
    fn jws_rs256_verificavel() {
        let k = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let t = jws(&k, r#"{"productId":"KanGest"}"#);
        let partes: Vec<&str> = t.split('.').collect();
        assert_eq!(partes.len(), 3);
        assert!(!t.contains('=') && !t.contains('+') && !t.contains('/'));
        assert_eq!(B64URL.decode(partes[0]).unwrap(), br#"{"typ":"JOSE","alg":"RS256"}"#);
        assert_eq!(B64URL.decode(partes[1]).unwrap(), br#"{"productId":"KanGest"}"#);
        let sig = Signature::try_from(B64URL.decode(partes[2]).unwrap().as_slice()).unwrap();
        VerifyingKey::<Sha256>::new(k.to_public_key()).verify(format!("{}.{}", partes[0], partes[1]).as_bytes(), &sig).expect("RS256 válida");
        let pequena = RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
        use rsa::pkcs8::EncodePrivateKey;
        let pem = pequena.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        assert!(chave_pem(&pem).unwrap_err().to_string().contains("2048"));
    }
}

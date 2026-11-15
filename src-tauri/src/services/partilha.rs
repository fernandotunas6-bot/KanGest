//! Recibo digital: o documento por email (servidor SMTP da empresa) e por WhatsApp.
//!
//! - **Email**: mensagem em HTML (e texto simples) com o documento completo — linhas, totais,
//!   resumo do IVA, meios de pagamento, pontos de fidelização, o rodapé da AGT e, na facturação
//!   electrónica, a ligação de consulta da AGT. Cópia oculta opcional para o arquivo da empresa.
//! - **WhatsApp**: o resumo do documento numa ligação `wa.me` para o telefone do cliente. A
//!   ligação abre-se no posto onde está o operador (`partilha.abrir` é um método local).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::catalogo::IdIn;
use super::impressao::{data_curta, kz, lista, num, pct, qtd, real, rodape_agt, txt, MOTIVOS};
use crate::error::{AppError, AppResult};
use crate::smtp::{self, Seguranca};
use crate::state::AppState;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailCfg {
    pub servidor: String,
    pub porta: u16,
    pub seguranca: Seguranca,
    pub utilizador: String,
    /// endereço que envia (normalmente o mesmo do utilizador)
    pub remetente: String,
    /// nome que o cliente vê (por omissão, o nome da empresa)
    pub nome: String,
    /// cópia oculta de cada documento enviado (arquivo da empresa)
    pub copia: String,
}

impl Default for EmailCfg {
    fn default() -> Self {
        EmailCfg { servidor: String::new(), porta: 465, seguranca: Seguranca::Ssl, utilizador: String::new(), remetente: String::new(), nome: String::new(), copia: String::new() }
    }
}

pub(crate) async fn cfg(p: &sqlx::SqlitePool) -> AppResult<EmailCfg> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'email'").fetch_optional(p).await?;
    Ok(v.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
}

pub async fn config_obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let c = cfg(&st.pool).await?;
    let tem_senha = crate::segredos::ler(&st.pool, "email.senha").await.ok().flatten().is_some_and(|s| !s.is_empty());
    let mut v = json!(c);
    v["tem_senha"] = json!(tem_senha);
    Ok(v)
}

#[derive(Deserialize)]
pub struct ConfigIn {
    #[serde(flatten)]
    cfg: EmailCfg,
    /// vazio ou ausente: mantém a senha gravada
    senha: Option<String>,
}

pub async fn config_guardar(st: &AppState, i: ConfigIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let mut c = i.cfg;
    c.servidor = c.servidor.trim().to_string();
    c.utilizador = c.utilizador.trim().to_string();
    c.remetente = c.remetente.trim().to_string();
    c.nome = c.nome.trim().to_string();
    c.copia = c.copia.trim().to_string();
    if c.servidor.is_empty() || c.servidor.contains(char::is_whitespace) {
        return Err(AppError::msg("Indique o servidor de email (SMTP), ex.: smtp.gmail.com."));
    }
    if c.porta == 0 {
        return Err(AppError::msg("Indique a porta do servidor de email (465 para SSL/TLS, 587 para STARTTLS)."));
    }
    if !smtp::email_valido(&c.remetente) {
        return Err(AppError::msg("Indique o email do remetente."));
    }
    smtp::enderecos(&c.copia)?;
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('email', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(serde_json::to_string(&c)?)
        .execute(&st.pool)
        .await?;
    if let Some(s) = i.senha.filter(|s| !s.is_empty()) {
        crate::segredos::gravar(&st.pool, "email.senha", &s).await?;
    }
    config_obter(st).await
}

/// Configuração completa (com a senha) para enviar.
async fn servidor(st: &AppState) -> AppResult<(smtp::Servidor, Vec<String>)> {
    let c = cfg(&st.pool).await?;
    if c.servidor.is_empty() || !smtp::email_valido(&c.remetente) {
        return Err(AppError::msg("O envio por email não está configurado (Configurações › Email)."));
    }
    let senha = crate::segredos::ler(&st.pool, "email.senha").await?.unwrap_or_default();
    let nome = if c.nome.is_empty() {
        sqlx::query_scalar::<_, String>("SELECT COALESCE(NULLIF(designacao_comercial, ''), nome) FROM empresa WHERE id = 1").fetch_optional(&st.pool).await?.unwrap_or_default()
    } else {
        c.nome.clone()
    };
    let copia = smtp::enderecos(&c.copia)?;
    Ok((smtp::Servidor { servidor: c.servidor, porta: c.porta, seguranca: c.seguranca, utilizador: c.utilizador, senha, remetente: c.remetente, nome }, copia))
}

async fn enviar(srv: smtp::Servidor, m: smtp::Mensagem) -> AppResult<String> {
    tokio::task::spawn_blocking(move || smtp::enviar(&srv, &m)).await.map_err(|e| AppError::msg(e.to_string()))?
}

#[derive(Deserialize)]
pub struct TesteIn {
    para: String,
}

pub async fn testar(st: &AppState, i: TesteIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let (srv, _) = servidor(st).await?;
    let m = smtp::Mensagem {
        para: smtp::enderecos(&i.para)?,
        bcc: vec![],
        assunto: "Teste de email do KanGest".into(),
        texto: format!("O envio de documentos por email de {} está a funcionar.", srv.nome),
        html: None,
    };
    let r = enviar(srv, m).await?;
    Ok(json!({ "resposta": r }))
}

// ------------------------------------------------------------------ documento → texto e HTML

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn empresa_nome(d: &Value) -> String {
    let e = &d["empresa"];
    let dc = txt(e, "designacao_comercial");
    if dc.is_empty() {
        txt(e, "nome")
    } else {
        dc
    }
}

/// Ligação de consulta na AGT (facturação electrónica), como o QR code do impresso.
fn url_agt(d: &Value) -> Option<String> {
    let fe = &d["fe"];
    if !fe.is_object() {
        return None;
    }
    Some(format!(
        "https://quiosqueagt.minfin.gov.ao/facturacao-eletronica/consultar-fe?emissor={}&document={}",
        url_codificar(&txt(fe, "nif")),
        txt(&d["documento"], "numero_doc").replace(' ', "%20")
    ))
}

fn pontos(d: &Value) -> Option<String> {
    let f = &d["fidelizacao"];
    if !f.is_object() || (num(f, "ganhos") == 0 && num(f, "gastos") == 0) {
        return None;
    }
    let nc = num(&d["documento"], "credito") == 1;
    let mut partes = Vec::new();
    if num(f, "ganhos") > 0 {
        partes.push(format!("{} +{}", if nc { "devolvidos" } else { "ganhos" }, num(f, "ganhos")));
    }
    if num(f, "gastos") > 0 {
        partes.push(format!("{} {}", if nc { "retirados" } else { "usados" }, num(f, "gastos")));
    }
    partes.push(format!("saldo {}", num(f, "saldo")));
    Some(format!("Pontos de fidelização: {}", partes.join(" · ")))
}

/// Resumo em texto (corpo simples do email e mensagem de WhatsApp, onde *x* é negrito).
fn texto(d: &Value, whatsapp: bool) -> String {
    let doc = &d["documento"];
    let b = |s: String| if whatsapp { format!("*{s}*") } else { s };
    let mut t = Vec::new();
    t.push(b(empresa_nome(d)));
    t.push(format!("{} {} · {}", txt(doc, "tipo_desc"), txt(doc, "numero_doc"), data_curta(&txt(doc, "data_emissao"))));
    if num(doc, "entidade_id") != 1 {
        t.push(format!("Cliente: {} · NIF {}", txt(doc, "entidade_nome"), txt(doc, "entidade_nif")));
    }
    t.push(String::new());
    let linhas = lista(d, "linhas");
    let maximo = if whatsapp { 15 } else { usize::MAX };
    for l in linhas.iter().take(maximo) {
        t.push(format!("{} × {} · {} Kz", qtd(real(l, "quantidade")), txt(l, "descricao"), kz(num(l, "total"))));
    }
    if linhas.len() > maximo {
        t.push(format!("… e mais {} linha(s)", linhas.len() - maximo));
    }
    t.push(String::new());
    t.push(b(format!("Total: {} Kz", kz(num(doc, "total")))) + &format!(" (IVA {} Kz)", kz(num(doc, "total_iva"))));
    let ret = num(doc, "total_retencao");
    if ret > 0 {
        t.push(format!("Retenção na fonte: {} Kz · a pagar {} Kz", kz(ret), kz(num(doc, "total") - ret)));
    }
    let pags: Vec<String> = lista(d, "pagamentos").iter().map(|p| format!("{} {} Kz", txt(p, "descricao"), kz(num(p, "valor") + if txt(p, "modo_codigo") == "NU" { num(doc, "troco") } else { 0 }))).collect();
    if !pags.is_empty() {
        t.push(format!("Pago: {}", pags.join(" · ")));
    }
    if num(doc, "troco") > 0 {
        t.push(format!("Troco: {} Kz", kz(num(doc, "troco"))));
    }
    if let Some(p) = pontos(d) {
        t.push(p);
    }
    t.push(String::new());
    if let Some(r) = rodape_agt(d) {
        t.push(r);
    }
    if let Some(u) = url_agt(d) {
        t.push(format!("Consultar na AGT: {u}"));
    }
    t.push("Obrigado pela preferência!".into());
    t.join("\n")
}

/// Documento em HTML para email (estilos em linha, tabelas: lê-se bem nos clientes de email).
fn html(d: &Value) -> String {
    let doc = &d["documento"];
    let e = &d["empresa"];
    let mut h = String::new();
    let cel = "padding:6px 8px;border-bottom:1px solid #e5e7eb;";
    let dir = "text-align:right;";
    h.push_str("<!doctype html><html><body style=\"margin:0;padding:24px;background:#f5f5f4;font-family:Arial,Helvetica,sans-serif;color:#1c1917;font-size:14px\">");
    h.push_str("<div style=\"max-width:640px;margin:0 auto;background:#fff;border:1px solid #e7e5e4;border-radius:12px;padding:24px\">");
    // empresa
    h.push_str(&format!("<div style=\"font-size:18px;font-weight:bold\">{}</div>", esc(&empresa_nome(d))));
    let mut contactos = vec![format!("NIF {}", txt(e, "nif"))];
    for k in ["endereco", "localidade", "telefone", "email"] {
        let v = txt(e, k);
        if !v.is_empty() {
            contactos.push(v);
        }
    }
    h.push_str(&format!("<div style=\"color:#57534e;font-size:12px;margin-top:2px\">{}</div>", esc(&contactos.join(" · "))));
    // documento e cliente
    h.push_str(&format!(
        "<div style=\"margin-top:20px;font-size:16px;font-weight:bold\">{} {}</div><div style=\"color:#57534e\">Data: {}</div>",
        esc(&txt(doc, "tipo_desc")),
        esc(&txt(doc, "numero_doc")),
        esc(&data_curta(&txt(doc, "data_emissao")))
    ));
    h.push_str(&format!(
        "<div style=\"margin-top:8px\">Cliente: <b>{}</b> · NIF {}</div>",
        esc(&txt(doc, "entidade_nome")),
        esc(&txt(doc, "entidade_nif"))
    ));
    if !txt(doc, "doc_origem").is_empty() {
        h.push_str(&format!("<div style=\"color:#57534e\">Documento de origem: {}</div>", esc(&txt(doc, "doc_origem"))));
    }
    // linhas
    h.push_str("<table style=\"width:100%;border-collapse:collapse;margin-top:16px;font-size:13px\"><thead><tr style=\"background:#f5f5f4\">");
    let esq = "text-align:left;";
    for (t, a) in [("Descrição", esq), ("Qtd", dir), ("Preço", dir), ("Desc.", dir), ("IVA", dir), ("Total", dir)] {
        h.push_str(&format!("<th style=\"{cel}{a}font-weight:600\">{t}</th>"));
    }
    h.push_str("</tr></thead><tbody>");
    for l in lista(d, "linhas") {
        let desc = real(l, "desconto_pct");
        h.push_str(&format!(
            "<tr><td style=\"{cel}\">{}</td><td style=\"{cel}{dir}\">{}</td><td style=\"{cel}{dir}\">{}</td><td style=\"{cel}{dir}\">{}</td><td style=\"{cel}{dir}\">{}</td><td style=\"{cel}{dir}\">{}</td></tr>",
            esc(&txt(l, "descricao")),
            qtd(real(l, "quantidade")),
            kz(num(l, "preco_unitario")),
            if desc > 0.0 { pct(desc) } else { String::new() },
            pct(real(l, "taxa_iva_pct")),
            kz(num(l, "total"))
        ));
    }
    h.push_str("</tbody></table>");
    // totais
    let linha_total = |k: &str, v: String, forte: bool| {
        format!(
            "<tr><td style=\"padding:3px 8px;{}\">{}</td><td style=\"padding:3px 8px;{dir}{}\">{} Kz</td></tr>",
            if forte { "font-weight:bold;font-size:16px;" } else { "color:#57534e;" },
            esc(k),
            if forte { "font-weight:bold;font-size:16px;" } else { "" },
            v
        )
    };
    h.push_str("<table style=\"margin:12px 0 0 auto;border-collapse:collapse;min-width:280px\">");
    h.push_str(&linha_total("Total sem imposto", kz(num(doc, "total_bruto")), false));
    if num(doc, "total_desconto") > 0 {
        h.push_str(&linha_total("Descontos", kz(num(doc, "total_desconto")), false));
    }
    h.push_str(&linha_total("IVA", kz(num(doc, "total_iva")), false));
    h.push_str(&linha_total(&format!("Total ({})", if txt(doc, "moeda").is_empty() { "AOA".into() } else { txt(doc, "moeda") }), kz(num(doc, "total")), true));
    for r in lista(d, "retencoes") {
        h.push_str(&linha_total(&format!("Retenção {}", txt(r, "descricao")), kz(num(r, "valor")), false));
    }
    for p in lista(d, "pagamentos") {
        let extra = if txt(p, "modo_codigo") == "NU" { num(doc, "troco") } else { 0 };
        h.push_str(&linha_total(&txt(p, "descricao"), kz(num(p, "valor") + extra), false));
    }
    if num(doc, "troco") > 0 {
        h.push_str(&linha_total("Troco", kz(num(doc, "troco")), false));
    }
    h.push_str("</table>");
    if let Some(p) = pontos(d) {
        h.push_str(&format!("<div style=\"margin-top:12px;padding:8px 12px;background:#fffbeb;border-radius:8px;color:#92400e\">★ {}</div>", esc(&p)));
    }
    // resumo do IVA
    let resumo = lista(d, "resumo_iva");
    if !resumo.is_empty() {
        h.push_str("<table style=\"width:100%;border-collapse:collapse;margin-top:16px;font-size:12px;color:#57534e\"><tr>");
        for (t, a) in [("Taxa", esq), ("Incidência", dir), ("Imposto", dir)] {
            h.push_str(&format!("<th style=\"{cel}{a}font-weight:600\">{t}</th>"));
        }
        h.push_str("</tr>");
        for r in resumo {
            let motivo = txt(r, "motivo_isencao");
            let motivo_txt = MOTIVOS.iter().find(|(c, _)| *c == motivo).map(|(_, t)| format!(" {motivo} – {t}")).unwrap_or_else(|| if motivo.is_empty() { String::new() } else { format!(" {motivo}") });
            h.push_str(&format!(
                "<tr><td style=\"{cel}\">{}{}</td><td style=\"{cel}{dir}\">{}</td><td style=\"{cel}{dir}\">{}</td></tr>",
                pct(real(r, "taxa")),
                esc(&motivo_txt),
                kz(num(r, "base")),
                kz(num(r, "iva"))
            ));
        }
        h.push_str("</table>");
    }
    // rodapé fiscal
    h.push_str("<div style=\"margin-top:20px;font-size:11px;color:#78716c\">");
    if let Some(r) = rodape_agt(d) {
        h.push_str(&format!("<div>{}</div>", esc(&r)));
    }
    if let Some(u) = url_agt(d) {
        h.push_str(&format!("<div style=\"margin-top:4px\"><a href=\"{}\" style=\"color:#0f766e\">Consultar este documento na AGT (facturação electrónica)</a></div>", esc(&u)));
    }
    h.push_str("<div style=\"margin-top:4px\">Cópia digital do documento. O original foi emitido e assinado pelo programa de facturação.</div>");
    let rodape = txt(e, "rodape");
    if !rodape.is_empty() {
        h.push_str(&format!("<div style=\"margin-top:4px\">{}</div>", esc(&rodape)));
    }
    h.push_str("</div></div></body></html>");
    h
}

#[derive(Deserialize)]
pub struct EnviarIn {
    id: i64,
    para: String,
}

/// Envia o documento por email ao cliente (e a cópia oculta do arquivo).
pub async fn enviar_documento(st: &AppState, i: EnviarIn) -> AppResult<Value> {
    let s = st.sessao()?;
    let para = smtp::enderecos(&i.para)?;
    if para.is_empty() {
        return Err(AppError::msg("Indique o email do cliente."));
    }
    let d = super::documentos::obter(st, IdIn { id: i.id }).await?;
    let doc = &d["documento"];
    if txt(doc, "estado") == "A" {
        return Err(AppError::msg("O documento está anulado."));
    }
    let (srv, copia) = servidor(st).await?;
    let assunto = format!("{} {} — {}", txt(doc, "tipo_desc"), txt(doc, "numero_doc"), empresa_nome(&d));
    let m = smtp::Mensagem { para: para.clone(), bcc: copia, assunto, texto: texto(&d, false), html: Some(html(&d)) };
    let resposta = enviar(srv, m).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'EMAIL', ?)")
        .bind(s.id)
        .bind(format!("{} enviado para {}", txt(doc, "numero_doc"), para.join(", ")))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "para": para, "resposta": resposta }))
}

// ------------------------------------------------------------------ WhatsApp

/// Codificação para URL (RFC 3986): tudo o que não é letra, algarismo ou -_.~ vira %XX (UTF-8).
fn url_codificar(s: &str) -> String {
    let mut r = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            r.push(b as char);
        } else {
            r.push_str(&format!("%{b:02X}"));
        }
    }
    r
}

/// Telefone para o WhatsApp (só algarismos, com o indicativo): os números angolanos de 9
/// algarismos ganham o 244; «+351 …» e «00351 …» ficam com o indicativo indicado.
pub fn telefone_whatsapp(t: &str) -> Option<String> {
    let digitos: String = t.chars().filter(char::is_ascii_digit).collect();
    let d = digitos.strip_prefix("00").unwrap_or(&digitos).to_string();
    let d = if d.len() == 9 && d.starts_with('9') && !t.trim_start().starts_with('+') { format!("244{d}") } else { d };
    (d.len() >= 10 && d.len() <= 15).then_some(d)
}

#[derive(Deserialize)]
pub struct MensagemIn {
    id: i64,
    telefone: Option<String>,
}

/// Texto do documento para o WhatsApp, a ligação `wa.me` e os contactos do cliente.
pub async fn mensagem(st: &AppState, i: MensagemIn) -> AppResult<Value> {
    st.sessao()?;
    let d = super::documentos::obter(st, IdIn { id: i.id }).await?;
    let entidade = num(&d["documento"], "entidade_id");
    let (tel, email): (Option<String>, Option<String>) = if entidade != 1 {
        sqlx::query_as("SELECT telefone, email FROM entidades WHERE id = ?").bind(entidade).fetch_optional(&st.pool).await?.unwrap_or((None, None))
    } else {
        (None, None)
    };
    // o telefone escrito pelo operador tem de ser válido; o da ficha do cliente, se não for, fica por indicar
    let indicado = i.telefone.filter(|t| !t.trim().is_empty());
    let numero = match &indicado {
        Some(t) => Some(telefone_whatsapp(t).ok_or_else(|| AppError::msg("Telefone inválido para o WhatsApp: indique o número com 9 algarismos (ou com o indicativo do país)."))?),
        None => tel.as_deref().and_then(telefone_whatsapp),
    };
    let t = texto(&d, true);
    let url = format!("https://wa.me/{}?text={}", numero.clone().unwrap_or_default(), url_codificar(&t));
    Ok(json!({ "texto": t, "url": url, "telefone": tel.unwrap_or_default(), "numero": numero, "email": email.unwrap_or_default() }))
}

#[derive(Deserialize)]
pub struct AbrirIn {
    url: String,
}

/// Abre a ligação do WhatsApp no navegador ou na aplicação deste posto (método local).
pub async fn abrir(i: AbrirIn) -> AppResult<Value> {
    if !i.url.starts_with("https://wa.me/") || i.url.chars().any(|c| c.is_whitespace() || c == '"' || c.is_control()) {
        return Err(AppError::msg("Endereço não permitido."));
    }
    super::licenca::abrir_no_navegador(&i.url)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn telefones_para_o_whatsapp() {
        assert_eq!(telefone_whatsapp("923 456 789").as_deref(), Some("244923456789"));
        assert_eq!(telefone_whatsapp("+244 923-456-789").as_deref(), Some("244923456789"));
        assert_eq!(telefone_whatsapp("00351 912 345 678").as_deref(), Some("351912345678"));
        assert_eq!(telefone_whatsapp("222 000"), None);
        assert_eq!(telefone_whatsapp(""), None);
    }

    #[test]
    fn codifica_para_url() {
        assert_eq!(url_codificar("Total: 5 928,00 Kz\n*ok*"), "Total%3A%205%20928%2C00%20Kz%0A%2Aok%2A");
        assert_eq!(url_codificar("Açaí"), "A%C3%A7a%C3%AD");
    }
}

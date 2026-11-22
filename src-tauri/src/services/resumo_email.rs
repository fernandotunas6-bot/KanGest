// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Resumo diário ao dono, por email (opcional): todas as manhãs, à hora escolhida, o resumo de ontem, as previsões e
//! os alertas do painel, da empresa toda, pelo servidor de email da empresa (Configurações › Email).
//!
//! Liga-se no painel («Resumo de ontem» › Por email). Envia-o a máquina que guarda a base de dados (o servidor da
//! rede, ou o computador único), uma vez por dia; se o envio falhar, tenta de novo uma hora depois. As contas são as
//! do painel (services/inteligencia.rs), feitas neste computador; o email só leva o resultado.

use std::sync::Arc;

use chrono::{Duration, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::impressao::{kz, pct, qtd};
use super::inteligencia::{self, data, dia_semana};
use super::partilha;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::smtp;
use crate::state::AppState;

const CHAVE: &str = "inteligencia.resumo_email";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ResumoCfg {
    pub ativo: bool,
    /// para onde vai (um ou mais emails, separados por vírgulas)
    pub para: String,
    /// a partir de que hora sai (0–23)
    pub hora: u32,
    /// o último dia em que saiu
    pub enviado: Option<String>,
}

impl Default for ResumoCfg {
    fn default() -> Self {
        ResumoCfg { ativo: false, para: String::new(), hora: 8, enviado: None }
    }
}

async fn cfg(p: &sqlx::SqlitePool) -> AppResult<ResumoCfg> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE).fetch_optional(p).await?;
    Ok(v.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
}

async fn gravar(p: &sqlx::SqlitePool, c: &ResumoCfg) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(CHAVE)
        .bind(serde_json::to_string(c)?)
        .execute(p)
        .await?;
    Ok(())
}

pub async fn config_obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let c = cfg(&st.pool).await?;
    let email = partilha::cfg(&st.pool).await?;
    Ok(json!({
        "ativo": c.ativo, "para": c.para, "hora": c.hora, "enviado": c.enviado,
        "email_configurado": !email.servidor.is_empty() && smtp::email_valido(&email.remetente),
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    ativo: bool,
    #[serde(default)]
    para: String,
    #[serde(default)]
    hora: Option<u32>,
}

pub async fn config_guardar(st: &AppState, i: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let mut c = cfg(&st.pool).await?;
    let para = smtp::enderecos(&i.para)?;
    if i.ativo {
        if para.is_empty() {
            return Err(AppError::msg("Indique o email para onde vai o resumo."));
        }
        partilha::servidor(st).await?; // o servidor de email da empresa tem de estar configurado
    }
    c.ativo = i.ativo;
    c.para = para.join(", ");
    c.hora = i.hora.unwrap_or(c.hora).min(23);
    gravar(&st.pool, &c).await?;
    config_obter(st).await
}

/// Envia já o resumo (para experimentar), para os emails gravados.
pub async fn enviar_agora(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let para = smtp::enderecos(&cfg(&st.pool).await?.para)?;
    if para.is_empty() {
        return Err(AppError::msg("Indique primeiro o email para onde vai o resumo."));
    }
    enviar(st, &para).await?;
    Ok(json!({ "ok": true, "mensagem": format!("Resumo enviado para {}.", para.join(", ")) }))
}

async fn enviar(st: &AppState, para: &[String]) -> AppResult<()> {
    let (srv, _) = partilha::servidor(st).await?;
    let m = mensagem(st, para).await?;
    partilha::enviar(srv, m).await?;
    Ok(())
}

/// Envia o resumo do dia, se estiver ligado, já for a hora e ainda não tiver saído hoje. `Ok(true)` quando saiu.
pub(crate) async fn tentar(st: &AppState, hora_agora: u32) -> AppResult<bool> {
    let mut c = cfg(&st.pool).await?;
    let hoje = db::hoje();
    if !c.ativo || hora_agora < c.hora || c.enviado.as_deref() == Some(hoje.as_str()) {
        return Ok(false);
    }
    let para = smtp::enderecos(&c.para)?;
    if para.is_empty() {
        return Ok(false);
    }
    enviar(st, &para).await?;
    c.enviado = Some(hoje);
    gravar(&st.pool, &c).await?;
    Ok(true)
}

/// O envio diário, enquanto o programa está aberto, na máquina que guarda a base de dados.
pub fn iniciar(st: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut falhou_em: Option<std::time::Instant> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;
            if crate::rede::config().cliente() || falhou_em.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(3600)) {
                continue;
            }
            match tentar(&st, chrono::Local::now().hour()).await {
                Ok(_) => falhou_em = None,
                Err(e) => {
                    eprintln!("Resumo diário por email: {e}");
                    falhou_em = Some(std::time::Instant::now());
                }
            }
        }
    });
}

// ------------------------------------------------------------------ a mensagem

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn kzs(c: i64) -> String {
    format!("{} Kz", kz(c))
}
fn plural(n: i64, um: &str, varios: &str) -> String {
    format!("{n} {}", if n == 1 { um } else { varios })
}
fn nos_dias(d: &str) -> String {
    d.strip_suffix("-feira").map(|x| format!("{x}s-feiras")).unwrap_or_else(|| format!("{d}s"))
}
fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// A frase de cada alerta e o pormenor (como no painel).
pub(crate) fn frase_alerta(a: &Value) -> (String, String) {
    let n = i(&a["n"]);
    let itens = a["itens"].as_array().cloned().unwrap_or_default();
    let nomes = |k: usize| itens.iter().take(k).map(|x| s(&x["nome"]).to_string()).collect::<Vec<_>>().join(", ");
    let ex = &a["extra"];
    match s(&a["tipo"]) {
        "abaixo_custo" => (format!("{} abaixo do custo", plural(n, "venda", "vendas")), format!("Últimos 7 dias · perda de {} · {}", kzs(i(&a["valor"])), nomes(3))),
        "caixa_diferenca" => (
            format!("{} com diferença", plural(n, "fecho de caixa", "fechos de caixa")),
            format!(
                "Últimos 7 dias · {}",
                itens.iter().map(|x| format!("{}: {} {}", s(&x["nome"]), if i(&x["valor"]) < 0 { "falta" } else { "sobra" }, kzs(i(&x["valor"]).abs()))).collect::<Vec<_>>().join(" · ")
            ),
        ),
        "anulacoes" => (
            format!("{n} vendas anuladas esta semana"),
            format!(
                "O habitual é {} por semana · {}",
                qtd(ex["habitual"].as_f64().unwrap_or(0.0)),
                itens.iter().map(|x| format!("{}: {}", s(&x["nome"]), i(&x["n"]))).collect::<Vec<_>>().join(", ")
            ),
        ),
        "vendas_quebra" => (
            format!("Vendas da semana {}% abaixo do habitual", i(&ex["variacao"]).abs()),
            format!("{} face a {} por semana", kzs(i(&a["valor"])), kzs(i(&ex["habitual"]))),
        ),
        "margem_queda" => (
            format!("Margem a cair para {}", pct(ex["actual"].as_f64().unwrap_or(0.0))),
            format!("Últimos 30 dias · nos 3 meses anteriores era {}", pct(ex["anterior"].as_f64().unwrap_or(0.0))),
        ),
        "preco_abaixo_custo" => (
            format!("{} de venda abaixo do custo", plural(n, "preço", "preços")),
            itens.iter().take(2).map(|x| format!("{}: {} (custo {})", s(&x["nome"]), kzs(i(&x["valor"])), kzs(i(&x["custo"])))).collect::<Vec<_>>().join(" · "),
        ),
        "devedores" => (
            format!("{} em atraso há mais de 30 dias", plural(n, "cliente", "clientes")),
            match itens.first() {
                Some(x) => format!("{} no total · {}: {}, {} dias", kzs(i(&a["valor"])), s(&x["nome"]), kzs(i(&x["valor"])), i(&x["dias"])),
                None => format!("{} no total", kzs(i(&a["valor"]))),
            },
        ),
        "clientes_parados" => (
            format!("{} de comprar", if n == 1 { "1 bom cliente deixou".to_string() } else { format!("{n} bons clientes deixaram") }),
            itens.iter().take(2).map(|x| format!("{}: há {} dias (comprava de {} em {})", s(&x["nome"]), i(&x["dias"]), i(&x["intervalo"]), i(&x["intervalo"]))).collect::<Vec<_>>().join(" · "),
        ),
        "stock_parado" => (format!("{} há 90 dias", plural(n, "artigo parado", "artigos parados")), format!("{} ao custo · {}", kzs(i(&a["valor"])), nomes(3))),
        t => (t.to_string(), String::new()),
    }
}

struct Seccao {
    titulo: String,
    frase: Option<String>,
    /// (rótulo, valor); um rótulo sem valor é uma linha de alerta (o valor vai por baixo, em pormenor)
    linhas: Vec<(String, String)>,
    alertas: bool,
}

pub(crate) async fn mensagem(st: &AppState, para: &[String]) -> AppResult<smtp::Mensagem> {
    let hoje = data(&db::hoje()).unwrap_or_else(|| chrono::Local::now().date_naive());
    let ontem = hoje - Duration::days(1);
    let h = inteligencia::historico(st, hoje, None).await?;
    let r = inteligencia::resumo_do_dia(st, &h, ontem, None).await?;
    let fecho = inteligencia::fecho_do_mes(&h, hoje);
    let rup = inteligencia::ruptura(st, hoje, &h, None).await?;
    let tes = inteligencia::tesouraria(st, hoje, None).await?;
    let alertas = inteligencia::alertas(st, &h, hoje, None).await?;
    let empresa: String = sqlx::query_scalar("SELECT COALESCE(NULLIF(designacao_comercial, ''), nome) FROM empresa WHERE id = 1")
        .fetch_optional(&st.pool)
        .await?
        .unwrap_or_else(|| "A sua empresa".to_string());
    let ds = dia_semana(ontem);
    let dmy = |d: NaiveDate| d.format("%d/%m/%Y").to_string();

    // ontem
    let (vendas, docs) = (i(&r["vendas"]), i(&r["docs"]));
    let frase = if docs == 0 {
        format!("Não houve vendas.{}", r["habitual"].as_i64().filter(|h| *h > 0).map(|h| format!(" Às {}, costuma vender {}.", nos_dias(ds), kzs(h))).unwrap_or_default())
    } else {
        let face = match (r["variacao"].as_i64(), r["habitual"].as_i64()) {
            (Some(v), Some(hab)) if v != 0 => format!(", {}% {} do habitual às {} ({})", v.abs(), if v > 0 { "acima" } else { "abaixo" }, nos_dias(ds), kzs(hab)),
            (Some(_), Some(_)) => format!(", como é habitual às {}", nos_dias(ds)),
            _ => String::new(),
        };
        format!("Vendeu {} em {}{face}.", kzs(vendas), plural(docs, "documento", "documentos"))
    };
    let mut linhas = Vec::new();
    if docs > 0 {
        linhas.push(("Ticket médio".to_string(), kzs(i(&r["ticket"]))));
        if let Some(m) = r["margem"].as_f64() {
            linhas.push(("Margem".to_string(), format!("{} · lucro {}", pct(m), kzs(i(&r["lucro"])))));
        }
        if r["top_artigo"].is_object() {
            linhas.push(("Mais vendido".to_string(), format!("{} ({})", s(&r["top_artigo"]["descricao"]), qtd(r["top_artigo"]["quantidade"].as_f64().unwrap_or(0.0)))));
        }
        if r["melhor_cliente"].is_object() {
            linhas.push(("Melhor cliente".to_string(), format!("{} · {}", s(&r["melhor_cliente"]["nome"]), kzs(i(&r["melhor_cliente"]["total"])))));
        }
    }
    let mut seccoes = vec![Seccao { titulo: format!("Ontem, {ds}, {}", dmy(ontem)), frase: Some(frase), linhas, alertas: false }];

    // previsões
    let mut prev = Vec::new();
    if fecho.is_null() {
        prev.push(("Fecho do mês".to_string(), "aparece com 14 dias de vendas".to_string()));
    } else {
        let ant = fecho["mes_anterior"].as_i64().map(|m| {
            let v = fecho["variacao"].as_i64().map(|v| format!(", {}{v}%", if v > 0 { "+" } else { "" })).unwrap_or_default();
            format!(" · mês anterior {}{v}", kzs(m))
        });
        prev.push(("Fecho do mês".to_string(), format!("≈ {} ({} até hoje{})", kzs(i(&fecho["previsto"])), kzs(i(&fecho["vendas_ate_hoje"])), ant.unwrap_or_default())));
    }
    prev.push(("Próximos 30 dias".to_string(), format!("entram {}, saem {}", kzs(i(&tes["entra"])), kzs(i(&tes["sai"])))));
    if i(&tes["entra_atraso"]) > 0 || i(&tes["sai_atraso"]) > 0 {
        let mut a = Vec::new();
        if i(&tes["entra_atraso"]) > 0 {
            a.push(format!("{} a receber", kzs(i(&tes["entra_atraso"]))));
        }
        if i(&tes["sai_atraso"]) > 0 {
            a.push(format!("{} a pagar", kzs(i(&tes["sai_atraso"]))));
        }
        prev.push(("Já em atraso".to_string(), a.join(", ")));
    }
    let n_rup = i(&rup["n"]);
    prev.push((
        "Vão esgotar em 7 dias".to_string(),
        if n_rup == 0 {
            "nenhum artigo, ao ritmo actual".to_string()
        } else {
            let nomes = rup["artigos"].as_array().map(|a| a.iter().take(3).map(|x| s(&x["descricao"]).to_string()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            format!("{}: {nomes}{}", plural(n_rup, "artigo", "artigos"), if n_rup > 3 { "…" } else { "" })
        },
    ));
    seccoes.push(Seccao { titulo: "Previsões".to_string(), frase: None, linhas: prev, alertas: false });

    // alertas
    seccoes.push(if alertas.is_empty() {
        Seccao { titulo: "Alertas".to_string(), frase: Some("Nada fora do habitual: vendas, margens, fechos de caixa, preços e clientes dentro do normal.".to_string()), linhas: Vec::new(), alertas: true }
    } else {
        Seccao { titulo: format!("Alertas ({})", alertas.len()), frase: None, linhas: alertas.iter().map(frase_alerta).collect(), alertas: true }
    });

    // texto simples
    let mut texto = format!("{empresa}: resumo diário do KanGest\n");
    for sc in &seccoes {
        texto.push_str(&format!("\n{}\n", sc.titulo.to_uppercase()));
        if let Some(f) = &sc.frase {
            texto.push_str(&format!("{f}\n"));
        }
        for (a, b) in &sc.linhas {
            texto.push_str(&if sc.alertas { format!("- {a}\n  {b}\n") } else { format!("- {a}: {b}\n") });
        }
    }
    texto.push_str("\nCalculado pelo KanGest a partir dos dados da empresa; nada sai para fora além deste email.\nPara deixar de o receber: Painel › Resumo de ontem › Por email.\n");

    // HTML (as cores do KanGest)
    let mut html = String::from(
        "<!doctype html><html lang=\"pt\"><body style=\"margin:0;padding:24px;background:#f5f4ef;font-family:Arial,Helvetica,sans-serif;color:#1d2422;font-size:14px\">\
         <div style=\"max-width:600px;margin:0 auto;background:#ffffff;border:1px solid #e6e2d8;border-radius:12px;overflow:hidden\">",
    );
    html.push_str(&format!(
        "<div style=\"background:#146b63;color:#ffffff;padding:18px 24px\"><div style=\"font-size:12px;opacity:.85\">KanGest · resumo diário</div>\
         <div style=\"font-size:18px;font-weight:bold;margin-top:2px\">{}</div></div><div style=\"padding:4px 24px 24px\">",
        esc(&empresa)
    ));
    for sc in &seccoes {
        html.push_str(&format!("<div style=\"font-size:15px;font-weight:bold;margin:22px 0 8px\">{}</div>", esc(&sc.titulo)));
        if let Some(f) = &sc.frase {
            html.push_str(&format!("<div style=\"font-size:15px;line-height:1.5;margin-bottom:8px\">{}</div>", esc(f)));
        }
        if !sc.linhas.is_empty() {
            html.push_str("<table style=\"width:100%;border-collapse:collapse;font-size:13px\">");
            for (a, b) in &sc.linhas {
                let cel = "padding:7px 0;border-bottom:1px solid #f0ede6;vertical-align:top";
                html.push_str(&if sc.alertas {
                    format!("<tr><td style=\"{cel}\"><div>{}</div><div style=\"color:#6b7370;font-size:12px;margin-top:2px\">{}</div></td></tr>", esc(a), esc(b))
                } else {
                    format!("<tr><td style=\"{cel};color:#6b7370\">{}</td><td style=\"{cel};text-align:right\">{}</td></tr>", esc(a), esc(b))
                });
            }
            html.push_str("</table>");
        }
    }
    html.push_str(
        "<div style=\"margin-top:24px;font-size:11px;color:#8a908d;line-height:1.5\">Calculado pelo KanGest a partir dos dados da empresa; nada sai para fora \
         além deste email. Para deixar de o receber: Painel › Resumo de ontem › Por email.</div></div></div></body></html>",
    );

    Ok(smtp::Mensagem {
        para: para.to_vec(),
        bcc: Vec::new(),
        assunto: if docs == 0 {
            format!("KanGest: resumo de ontem ({ds}), sem vendas")
        } else {
            format!("KanGest: resumo de ontem ({ds}), {}", kzs(vendas))
        },
        texto,
        html: Some(html),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }
    async fn rpc_err(st: &AppState, m: &str, p: Value) -> String {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => panic!("{m} devia falhar, devolveu {v}"),
            Err(e) => e.to_string(),
        }
    }
    fn html_da_mensagem(dados: &str) -> String {
        use base64::Engine;
        let i = dados.find("Content-Type: text/html").expect("parte HTML");
        let resto = &dados[i..];
        let corpo = &resto[resto.find("\r\n\r\n").unwrap() + 4..];
        let b64: String = corpo.lines().take_while(|l| !l.starts_with("--")).collect::<Vec<_>>().join("");
        String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64.trim()).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn resumo_diario_por_email() {
        let pasta = std::env::temp_dir().join(format!("kangest-resumo-email-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
        let mut st = AppState::new(pool);
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.expect("posto 1");
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Papelaria <Sol> & Filhos", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Caderno A4", "taxa_iva_id": 1, "preco1": 100000, "custo": 60000 })).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": art, "quantidade": 10, "preco_unitario": 60000 }] })).await;
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
        let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 228000 }] })).await;
        // só nesta base de teste: a venda passa para ontem
        sqlx::query("DROP TRIGGER trg_doc_assinado_upd").execute(&st.pool).await.unwrap();
        sqlx::query("UPDATE documentos SET data_emissao = date('now', 'localtime', '-1 day') WHERE id = ?").bind(fr["documento"]["id"].as_i64().unwrap()).execute(&st.pool).await.unwrap();

        // desligado por omissão; ligar exige destinatário e o servidor de email da empresa
        let c = rpc(&st, "inteligencia.resumoConfig", Value::Null).await;
        assert_eq!((c["ativo"].as_bool(), c["hora"].as_i64(), c["email_configurado"].as_bool()), (Some(false), Some(8), Some(false)));
        assert!(rpc_err(&st, "inteligencia.resumoGuardar", json!({ "ativo": true, "para": "" })).await.contains("Indique o email"));
        assert!(rpc_err(&st, "inteligencia.resumoGuardar", json!({ "ativo": true, "para": "dono@papelaria.ao" })).await.contains("não está configurado"));
        assert!(rpc_err(&st, "inteligencia.resumoGuardar", json!({ "ativo": true, "para": "dono@papelaria" })).await.contains("inválido"));
        let (porta, rx) = crate::smtp::testes::servidor_falso();
        rpc(&st, "email.configGuardar", json!({ "servidor": "127.0.0.1", "porta": porta, "seguranca": "NENHUMA", "utilizador": "loja@kianda.ao", "remetente": "loja@papelaria.ao", "senha": "segredo" })).await;
        let c = rpc(&st, "inteligencia.resumoGuardar", json!({ "ativo": true, "para": " dono@papelaria.ao ; socia@papelaria.ao ", "hora": 7 })).await;
        assert_eq!((c["ativo"].as_bool(), c["para"].as_str(), c["hora"].as_i64()), (Some(true), Some("dono@papelaria.ao, socia@papelaria.ao"), Some(7)));

        // enviar já: o resumo de ontem, as previsões e os alertas, com o nome da empresa escapado
        let r = rpc(&st, "inteligencia.resumoEnviar", Value::Null).await;
        assert_eq!(r["mensagem"], "Resumo enviado para dono@papelaria.ao, socia@papelaria.ao.");
        let (cmds, dados) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(cmds.iter().any(|c| c == "RCPT TO:<dono@papelaria.ao>") && cmds.iter().any(|c| c == "RCPT TO:<socia@papelaria.ao>"), "{cmds:?}");
        let html = html_da_mensagem(&dados);
        assert!(html.contains("Papelaria &lt;Sol&gt; &amp; Filhos") && !html.contains("<Sol>"), "{html}");
        assert!(html.contains("Vendeu 2.280,00 Kz em 2 documentos") || html.contains("Vendeu 2.280,00 Kz em 1 documento"), "{html}");
        assert!(html.contains("Caderno A4 (2)") && html.contains("Previsões") && html.contains("Alertas"), "{html}");

        // o envio automático: antes da hora não sai; depois sai uma vez por dia
        assert!(!tentar(&st, 6).await.unwrap());
        assert!(tentar(&st, 7).await.unwrap());
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(!tentar(&st, 9).await.unwrap(), "já saiu hoje");
        assert_eq!(rpc(&st, "inteligencia.resumoConfig", Value::Null).await["enviado"], db::hoje());
        // desligar
        rpc(&st, "inteligencia.resumoGuardar", json!({ "ativo": false, "para": "dono@papelaria.ao" })).await;
        sqlx::query("UPDATE config SET valor = json_set(valor, '$.enviado', NULL) WHERE chave = ?").bind(CHAVE).execute(&st.pool).await.unwrap();
        assert!(!tentar(&st, 12).await.unwrap(), "desligado não sai");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn frases_dos_alertas() {
        let a = json!({ "tipo": "caixa_diferenca", "n": 1, "valor": 250000, "itens": [{ "nome": "Caixa 1 · Ana", "valor": -250000 }], "extra": null });
        assert_eq!(frase_alerta(&a), ("1 fecho de caixa com diferença".to_string(), "Últimos 7 dias · Caixa 1 · Ana: falta 2.500,00 Kz".to_string()));
        let a = json!({ "tipo": "clientes_parados", "n": 2, "valor": 0, "itens": [{ "nome": "Mercearia", "dias": 60, "intervalo": 20 }], "extra": null });
        assert_eq!(frase_alerta(&a).0, "2 bons clientes deixaram de comprar");
        assert_eq!(nos_dias("sexta-feira"), "sextas-feiras");
        assert_eq!(nos_dias("sábado"), "sábados");
    }
}

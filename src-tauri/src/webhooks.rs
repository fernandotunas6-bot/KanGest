//! Avisos automáticos da API (webhooks): quando há documentos emitidos ou anulados, ou mudanças de stock ou de
//! artigos, o KanGest avisa os sistemas ligados (loja online, ERP…) com um POST JSON assinado.
//!
//! - Os eventos entram na fila por gatilhos da base de dados (migração 0021), só quando há webhooks activos: o código
//!   que emite documentos ou mexe no stock não muda.
//! - Cada evento dá uma entrega por webhook interessado; o aviso leva o estado no momento do envio.
//! - Assinatura: `X-KanGest-Assinatura: t=<segundos>,v1=<HMAC-SHA256 em hex de "<t>.<corpo>">` com o segredo do webhook.
//! - Falhas: nova tentativa ao fim de 1, 5, 15 e 30 minutos e de 1, 3, 6 e 12 horas; depois fica «falhada» e
//!   reenvia-se no ecrã.
//! - Faz parte da API: só envia quando a licença inclui a API (plano Empresa ou módulo «api»).

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::services;
use crate::state::AppState;

/// Eventos que um webhook pode receber: (id, descrição).
pub const EVENTOS: &[(&str, &str)] = &[
    ("documento.emitido", "Documento emitido (vendas, compras, recibos, stock)"),
    ("documento.anulado", "Documento anulado"),
    ("stock.alterado", "Existências de um artigo alteradas"),
    ("artigo.alterado", "Artigo criado ou alterado (preços, descrição, activo)"),
];

/// Minutos de espera antes de cada nova tentativa; acabadas, a entrega fica «falhada».
const ESPERAS_MIN: [i64; 8] = [1, 5, 15, 30, 60, 180, 360, 720];

/// Trabalho em segundo plano: de 10 em 10 segundos, na máquina que guarda a base de dados.
pub fn iniciar(st: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if crate::rede::config().cliente() {
                continue;
            }
            if let Err(e) = ciclo(&st).await {
                eprintln!("Avisos da API (webhooks): {e}");
            }
        }
    });
}

fn gerar_segredo() -> String {
    use rand::distributions::Alphanumeric;
    use rand::Rng;
    let s: String = rand::rngs::OsRng.sample_iter(&Alphanumeric).take(40).map(char::from).collect();
    format!("whsec_{s}")
}

/// `t=<segundos>,v1=<HMAC-SHA256 em hex de "<t>.<corpo>">`.
pub fn assinatura(segredo: &str, t: u64, corpo: &str) -> String {
    let chave = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, segredo.as_bytes());
    let tag = ring::hmac::sign(&chave, format!("{t}.{corpo}").as_bytes());
    let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    format!("t={t},v1={hex}")
}

fn agora_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Um ciclo: distribui os eventos novos pelos webhooks, envia as entregas devidas e arruma a fila.
/// Devolve quantas entregas foram aceites.
pub async fn ciclo(st: &AppState) -> AppResult<usize> {
    let p = &st.pool;
    let ha: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM api_webhooks)").fetch_one(p).await?;
    if !ha {
        return Ok(0);
    }
    distribuir(p).await?;
    // a API (e os seus avisos) faz parte do plano Empresa: sem ela, as entregas esperam
    if services::licenca::api_incluida(&services::licenca::calcular(st).await?).is_err() {
        return Ok(0);
    }
    let devidas = sqlx::query(
        "SELECT e.id, e.tentativas, w.id AS webhook_id, w.url, w.segredo, v.tipo, v.objecto_id, v.criado_em
         FROM api_entregas e JOIN api_webhooks w ON w.id = e.webhook_id JOIN api_eventos v ON v.id = e.evento_id
         WHERE e.estado = 'PENDENTE' AND w.ativo = 1 AND (e.proxima_em IS NULL OR e.proxima_em <= ?)
         ORDER BY e.id LIMIT 25",
    )
    .bind(db::agora())
    .fetch_all(p)
    .await?;
    let mut feitas = 0;
    for d in &devidas {
        if entregar(p, d).await? {
            feitas += 1;
        }
    }
    // eventos sem entregas por fazer saem da fila; os de mais de 30 dias apagam-se (com as entregas)
    sqlx::query(
        "UPDATE api_eventos SET pendente = 0 WHERE pendente = 1 AND distribuido = 1
           AND NOT EXISTS (SELECT 1 FROM api_entregas e WHERE e.evento_id = api_eventos.id AND e.estado = 'PENDENTE')",
    )
    .execute(p)
    .await?;
    let antes = (chrono::Local::now() - chrono::Duration::days(30)).format("%Y-%m-%dT%H:%M:%S").to_string();
    sqlx::query("DELETE FROM api_eventos WHERE pendente = 0 AND criado_em < ?").bind(antes).execute(p).await?;
    Ok(feitas)
}

/// Cria as entregas dos eventos novos para os webhooks activos que os pediram.
async fn distribuir(p: &SqlitePool) -> AppResult<()> {
    let webhooks: Vec<(i64, Vec<String>)> = sqlx::query_as::<_, (i64, String)>("SELECT id, eventos FROM api_webhooks WHERE ativo = 1")
        .fetch_all(p)
        .await?
        .into_iter()
        .map(|(id, e)| (id, serde_json::from_str(&e).unwrap_or_default()))
        .collect();
    let eventos: Vec<(i64, String)> = sqlx::query_as("SELECT id, tipo FROM api_eventos WHERE distribuido = 0 ORDER BY id LIMIT 1000").fetch_all(p).await?;
    if eventos.is_empty() {
        return Ok(());
    }
    let mut tx = p.begin().await?;
    for (evento, tipo) in &eventos {
        for (webhook, quer) in &webhooks {
            if quer.contains(tipo) {
                sqlx::query("INSERT OR IGNORE INTO api_entregas (webhook_id, evento_id) VALUES (?, ?)").bind(webhook).bind(evento).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE api_eventos SET distribuido = 1 WHERE id = ?").bind(evento).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// O que o aviso leva: o estado actual do documento, do stock ou do artigo (sem custos).
async fn dados_evento(p: &SqlitePool, tipo: &str, objecto: Option<i64>) -> AppResult<Value> {
    let Some(id) = objecto else {
        return Ok(json!({ "mensagem": "Teste de ligação do KanGest." }));
    };
    Ok(match tipo {
        "documento.emitido" | "documento.anulado" => {
            let Some(d) = sqlx::query(
                "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_descricao, t.grupo, d.data_emissao, d.data_hora,
                        d.entidade_id, d.entidade_nome, d.entidade_nif, d.total_liquido, d.total_iva, d.total, d.valor_pendente,
                        d.estado, d.motivo_anulacao, d.referencia, d.hash_controlo, d.posto_id, d.api_chave_id
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.id = ?",
            )
            .bind(id)
            .fetch_optional(p)
            .await?
            else {
                return Ok(Value::Null);
            };
            let mut v = db::row_json(&d);
            v["linhas"] = db::query_json_bind(
                p,
                "SELECT linha, artigo_id, referencia, descricao, quantidade, unidade, preco_unitario, desconto_pct, taxa_iva_pct,
                        valor_liquido, valor_iva, total, lote FROM documento_linhas WHERE documento_id = ? ORDER BY linha",
                id,
            )
            .await?;
            v["url"] = json!(format!("/api/v1/documentos/{id}"));
            v
        }
        "stock.alterado" => {
            let Some(a) = sqlx::query("SELECT id, referencia, codigo_barras, descricao FROM artigos WHERE id = ?").bind(id).fetch_optional(p).await? else {
                return Ok(Value::Null);
            };
            let armazens = db::query_json_bind(
                p,
                "SELECT s.armazem_id, ar.nome AS armazem, s.quantidade FROM stock s JOIN armazens ar ON ar.id = s.armazem_id WHERE s.artigo_id = ? ORDER BY s.armazem_id",
                id,
            )
            .await?;
            let total: f64 = armazens.as_array().map(|v| v.iter().filter_map(|x| x["quantidade"].as_f64()).sum()).unwrap_or(0.0);
            let mut v = db::row_json(&a);
            v["quantidade"] = json!(total);
            v["armazens"] = armazens;
            v
        }
        "artigo.alterado" => {
            let Some(a) = sqlx::query(
                "SELECT a.id, a.referencia, a.codigo_barras, a.descricao, a.tipo, a.preco1, a.preco2, a.preco3, a.preco4,
                        CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco1_iva, t.percentagem AS taxa_iva,
                        a.promo_desconto, a.promo_inicio, a.promo_fim, a.ativo, a.alterado_em
                 FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.id = ?",
            )
            .bind(id)
            .fetch_optional(p)
            .await?
            else {
                return Ok(Value::Null);
            };
            let mut v = db::row_json(&a);
            v["ativo"] = json!(v["ativo"].as_i64() == Some(1));
            v["url"] = json!(format!("/api/v1/artigos/{id}"));
            v
        }
        _ => Value::Null,
    })
}

/// Envia uma entrega e regista o resultado. Devolve se foi aceite (resposta 2xx).
async fn entregar(p: &SqlitePool, d: &SqliteRow) -> AppResult<bool> {
    let id: i64 = d.get("id");
    let tipo: String = d.get("tipo");
    let webhook: i64 = d.get("webhook_id");
    let dados = dados_evento(p, &tipo, d.get("objecto_id")).await?;
    let corpo = json!({ "id": id, "evento": tipo, "criado_em": d.get::<String, _>("criado_em"), "dados": dados }).to_string();
    let cabecalhos: Vec<(&'static str, String)> = vec![
        ("Content-Type", "application/json; charset=utf-8".into()),
        ("X-KanGest-Evento", tipo.clone()),
        ("X-KanGest-Entrega", id.to_string()),
        ("X-KanGest-Assinatura", assinatura(&crate::cofre::decifrar(p, crate::cofre::WEBHOOK, &d.get::<String, _>("segredo")).await?, agora_s(), &corpo)),
    ];
    let url: String = d.get("url");
    let r = tokio::task::spawn_blocking(move || crate::http::pedido("POST", &url, &cabecalhos, Some(corpo.as_bytes()), Duration::from_secs(15))).await;
    let (aceite, codigo, erro) = match r {
        Ok(Ok(resp)) if (200..300).contains(&resp.estado) => (true, resp.estado as i64, None),
        Ok(Ok(resp)) => (false, resp.estado as i64, Some(format!("HTTP {}: {}", resp.estado, resp.texto().chars().take(200).collect::<String>()))),
        Ok(Err(e)) => (false, 0, Some(e.to_string())),
        Err(e) => (false, 0, Some(e.to_string())),
    };
    let agora = db::agora();
    let tentativas = d.get::<i64, _>("tentativas") + 1;
    if aceite {
        sqlx::query("UPDATE api_entregas SET estado = 'ENTREGUE', tentativas = ?, codigo_http = ?, erro = NULL, proxima_em = NULL, entregue_em = ? WHERE id = ?")
            .bind(tentativas)
            .bind(codigo)
            .bind(&agora)
            .bind(id)
            .execute(p)
            .await?;
        sqlx::query("UPDATE api_webhooks SET ultimo_envio = ?, ultimo_estado = ?, falhas_seguidas = 0 WHERE id = ?").bind(&agora).bind(codigo).bind(webhook).execute(p).await?;
    } else {
        let acabou = tentativas >= ESPERAS_MIN.len() as i64;
        let espera = ESPERAS_MIN[((tentativas - 1).max(0) as usize).min(ESPERAS_MIN.len() - 1)];
        let proxima = (chrono::Local::now() + chrono::Duration::minutes(espera)).format("%Y-%m-%dT%H:%M:%S").to_string();
        sqlx::query("UPDATE api_entregas SET estado = ?, tentativas = ?, codigo_http = ?, erro = ?, proxima_em = ? WHERE id = ?")
            .bind(if acabou { "FALHADA" } else { "PENDENTE" })
            .bind(tentativas)
            .bind(codigo)
            .bind(erro)
            .bind((!acabou).then_some(proxima))
            .bind(id)
            .execute(p)
            .await?;
        sqlx::query("UPDATE api_webhooks SET ultimo_envio = ?, ultimo_estado = ?, falhas_seguidas = falhas_seguidas + 1 WHERE id = ?")
            .bind(&agora)
            .bind(codigo)
            .bind(webhook)
            .execute(p)
            .await?;
    }
    Ok(aceite)
}

// ============================== métodos do ecrã «API e Integrações» ==============================

/// Webhooks, entregas recentes e o catálogo de eventos.
pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.api")?;
    let p = &st.pool;
    let mut webhooks = db::query_json(
        p,
        "SELECT w.id, w.nome, w.url, w.eventos, w.segredo, w.ativo, w.criado_em, w.ultimo_envio, w.ultimo_estado, w.falhas_seguidas,
                (SELECT COUNT(*) FROM api_entregas e WHERE e.webhook_id = w.id AND e.estado = 'PENDENTE') AS pendentes,
                (SELECT COUNT(*) FROM api_entregas e WHERE e.webhook_id = w.id AND e.estado = 'FALHADA') AS falhadas
         FROM api_webhooks w ORDER BY w.id DESC",
    )
    .await?;
    for w in webhooks.as_array_mut().into_iter().flatten() {
        w["eventos"] = serde_json::from_str(w["eventos"].as_str().unwrap_or("[]")).unwrap_or(json!([]));
        w["ativo"] = json!(w["ativo"].as_i64() == Some(1));
        // guardado cifrado; o administrador pode vê-lo para configurar o sistema que recebe os avisos
        w["segredo"] = json!(crate::cofre::decifrar(p, crate::cofre::WEBHOOK, w["segredo"].as_str().unwrap_or_default()).await.unwrap_or_default());
    }
    Ok(json!({
        "webhooks": webhooks,
        "entregas": db::query_json(
            p,
            "SELECT e.id, e.estado, e.tentativas, e.proxima_em, e.codigo_http, e.erro, e.entregue_em, w.nome AS webhook, v.tipo, v.objecto_id, v.criado_em
             FROM api_entregas e JOIN api_webhooks w ON w.id = e.webhook_id JOIN api_eventos v ON v.id = e.evento_id
             ORDER BY e.id DESC LIMIT 200",
        )
        .await?,
        "eventos": EVENTOS.iter().map(|(id, d)| json!({ "id": id, "descricao": d })).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
pub struct WebhookIn {
    id: Option<i64>,
    nome: String,
    url: String,
    #[serde(default)]
    eventos: Vec<String>,
    ativo: Option<bool>,
}

/// Cria ou altera um webhook. Ao criar, devolve o segredo da assinatura.
pub async fn guardar(st: &AppState, w: WebhookIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let nome = w.nome.trim();
    if nome.is_empty() || nome.chars().count() > 80 {
        return Err(AppError::msg("Dê um nome ao aviso (ex.: «Loja online»), até 80 caracteres."));
    }
    let url = w.url.trim();
    let resto = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    if url.len() > 500 || url.chars().any(char::is_whitespace) || resto.map_or(true, |r| r.split('/').next().unwrap_or("").is_empty()) {
        return Err(AppError::msg("Indique o endereço completo que recebe os avisos (https://…)."));
    }
    let mut eventos: Vec<String> = Vec::new();
    for e in &w.eventos {
        if !EVENTOS.iter().any(|x| x.0 == e) {
            return Err(AppError::msg(format!("Evento desconhecido: {e}")));
        }
        if !eventos.contains(e) {
            eventos.push(e.clone());
        }
    }
    if eventos.is_empty() {
        return Err(AppError::msg("Escolha pelo menos um evento."));
    }
    let lista = serde_json::to_string(&eventos)?;
    let ativo = w.ativo.unwrap_or(true) as i64;
    let (id, segredo) = match w.id {
        Some(id) => {
            let r = sqlx::query("UPDATE api_webhooks SET nome = ?, url = ?, eventos = ?, ativo = ? WHERE id = ?").bind(nome).bind(url).bind(&lista).bind(ativo).bind(id).execute(&st.pool).await?;
            if r.rows_affected() == 0 {
                return Err(AppError::NaoEncontrado);
            }
            (id, None)
        }
        None => {
            let segredo = gerar_segredo();
            let id = sqlx::query("INSERT INTO api_webhooks (nome, url, eventos, segredo, ativo) VALUES (?, ?, ?, ?, ?)")
                .bind(nome)
                .bind(url)
                .bind(&lista)
                .bind(crate::cofre::cifrar(&st.pool, crate::cofre::WEBHOOK, &segredo).await?)
                .bind(ativo)
                .execute(&st.pool)
                .await?
                .last_insert_rowid();
            (id, Some(segredo))
        }
    };
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_WEBHOOK', ?)")
        .bind(s.id)
        .bind(format!("Aviso automático {}: {nome} → {url}", if w.id.is_some() { "alterado" } else { "criado" }))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "id": id, "segredo": segredo }))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

pub async fn apagar(st: &AppState, i: IdIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let nome: Option<String> = sqlx::query_scalar("SELECT nome FROM api_webhooks WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    let nome = nome.ok_or(AppError::NaoEncontrado)?;
    sqlx::query("DELETE FROM api_webhooks WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_WEBHOOK', ?)").bind(s.id).bind(format!("Aviso automático apagado: {nome}")).execute(&st.pool).await?;
    Ok(Value::Null)
}

/// Segredo novo (o antigo deixa logo de assinar os avisos).
pub async fn renovar_segredo(st: &AppState, i: IdIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.api")?;
    let segredo = gerar_segredo();
    let cifrado = crate::cofre::cifrar(&st.pool, crate::cofre::WEBHOOK, &segredo).await?;
    let r = sqlx::query("UPDATE api_webhooks SET segredo = ? WHERE id = ?").bind(cifrado).bind(i.id).execute(&st.pool).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NaoEncontrado);
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'API_WEBHOOK', 'Segredo de um aviso automático renovado')").bind(s.id).execute(&st.pool).await?;
    Ok(json!({ "segredo": segredo }))
}

/// Envia já um aviso de teste («ping») e devolve o resultado.
pub async fn testar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("sistema.api")?;
    services::licenca::api_incluida(&services::licenca::calcular(st).await?).map_err(AppError::msg)?;
    let p = &st.pool;
    let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM api_webhooks WHERE id = ?").bind(i.id).fetch_optional(p).await?;
    existe.ok_or(AppError::NaoEncontrado)?;
    let evento = sqlx::query("INSERT INTO api_eventos (tipo, objecto_id, distribuido) VALUES ('ping', NULL, 1)").execute(p).await?.last_insert_rowid();
    let entrega = sqlx::query("INSERT INTO api_entregas (webhook_id, evento_id) VALUES (?, ?)").bind(i.id).bind(evento).execute(p).await?.last_insert_rowid();
    let d = sqlx::query(
        "SELECT e.id, e.tentativas, w.id AS webhook_id, w.url, w.segredo, v.tipo, v.objecto_id, v.criado_em
         FROM api_entregas e JOIN api_webhooks w ON w.id = e.webhook_id JOIN api_eventos v ON v.id = e.evento_id WHERE e.id = ?",
    )
    .bind(entrega)
    .fetch_one(p)
    .await?;
    let aceite = entregar(p, &d).await?;
    // o teste não fica a repetir-se
    sqlx::query("UPDATE api_entregas SET estado = CASE WHEN estado = 'ENTREGUE' THEN estado ELSE 'FALHADA' END, proxima_em = NULL WHERE id = ?").bind(entrega).execute(p).await?;
    sqlx::query("UPDATE api_eventos SET pendente = 0 WHERE id = ?").bind(evento).execute(p).await?;
    let r = sqlx::query("SELECT codigo_http, erro FROM api_entregas WHERE id = ?").bind(entrega).fetch_one(p).await?;
    Ok(json!({ "aceite": aceite, "codigo_http": r.get::<Option<i64>, _>("codigo_http"), "erro": r.get::<Option<String>, _>("erro") }))
}

/// Volta a pôr na fila uma entrega falhada (ou pendente), já.
pub async fn reenviar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("sistema.api")?;
    let evento: Option<i64> = sqlx::query_scalar("SELECT evento_id FROM api_entregas WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    let evento = evento.ok_or(AppError::NaoEncontrado)?;
    sqlx::query("UPDATE api_entregas SET estado = 'PENDENTE', tentativas = 0, proxima_em = NULL WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    sqlx::query("UPDATE api_eventos SET pendente = 1 WHERE id = ?").bind(evento).execute(&st.pool).await?;
    Ok(Value::Null)
}

#[cfg(test)]
mod testes {
    #[test]
    fn assinatura_hmac() {
        // HMAC-SHA256 de "1700000000.{}" com a chave "segredo" (calculado à parte com openssl)
        let a = super::assinatura("segredo", 1_700_000_000, "{}");
        assert_eq!(a, "t=1700000000,v1=28d73844c84580182772a1e98a60aeb8f04184b1bef0d4e147cfa59ebf0bfedc");
        assert_ne!(a, super::assinatura("outro", 1_700_000_000, "{}"));
        assert!(super::gerar_segredo().starts_with("whsec_"));
    }
}

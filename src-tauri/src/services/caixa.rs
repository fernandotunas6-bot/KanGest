use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::catalogo::IdIn;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

async fn sessao_aberta(st: &AppState) -> AppResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT id FROM caixa_sessoes WHERE posto_id = ? AND estado = 'ABERTA' ORDER BY id DESC LIMIT 1")
        .bind(st.posto().id)
        .fetch_optional(&st.pool)
        .await?)
}

async fn resumo(st: &AppState, sid: i64) -> AppResult<Value> {
    let p = &st.pool;
    let s = sqlx::query(
        "SELECT c.*, u.nome AS utilizador, ps.descricao AS posto FROM caixa_sessoes c
         JOIN utilizadores u ON u.id = c.utilizador_id LEFT JOIN postos ps ON ps.id = c.posto_id WHERE c.id = ?",
    )
    .bind(sid)
    .fetch_optional(p)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let por_modo = sqlx::query(
        // parte dos documentos da sessão (índice por sessão); ao contrário, o SQLite indexava todos os pagamentos
        "SELECT m.codigo, m.descricao, COALESCE(x.total, 0) AS total
         FROM modos_pagamento m
         LEFT JOIN (SELECT dp.modo_codigo, SUM(dp.valor) AS total
                    FROM documentos d JOIN documento_pagamentos dp ON dp.documento_id = d.id
                    WHERE d.caixa_sessao_id = ?1 AND d.estado = 'N' GROUP BY dp.modo_codigo) x ON x.modo_codigo = m.codigo
         ORDER BY m.ordem",
    )
    .bind(sid)
    .fetch_all(p)
    .await?;
    let (docs, vendido): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(total), 0) FROM documentos WHERE caixa_sessao_id = ? AND estado = 'N' AND tipo_codigo IN ('FR','RC')",
    )
    .bind(sid)
    .fetch_one(p)
    .await?;
    let (entradas, saidas): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN tipo='ENTRADA' THEN valor END), 0), COALESCE(SUM(CASE WHEN tipo='SAIDA' THEN valor END), 0)
         FROM caixa_movimentos WHERE sessao_id = ? AND COALESCE(modo_codigo, 'NU') = 'NU'",
    )
    .bind(sid)
    .fetch_one(p)
    .await?;
    let movimentos = sqlx::query("SELECT * FROM caixa_movimentos WHERE sessao_id = ? ORDER BY id").bind(sid).fetch_all(p).await?;
    // Artigos vendidos na sessão (o talão de fecho do KIGEST lista-os)
    let artigos = sqlx::query(
        "SELECT l.referencia, l.descricao, SUM(l.quantidade) AS quantidade, SUM(l.total) AS total
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE d.caixa_sessao_id = ? AND d.estado = 'N' AND d.tipo_codigo = 'FR'
         GROUP BY l.referencia, l.descricao ORDER BY total DESC",
    )
    .bind(sid)
    .fetch_all(p)
    .await?;
    // numerário recebido noutras moedas: fica na gaveta em dólares/euros, não em kwanzas; o troco desses
    // pagamentos saiu dos kwanzas (Kz na gaveta = numerário líquido − equivalente bruto do que entrou em moeda)
    let estrangeiras = sqlx::query(
        "SELECT dp.moeda, SUM(dp.valor_moeda) AS valor, CAST(SUM(ROUND(dp.valor_moeda * dp.cambio)) AS INTEGER) AS kz
         FROM documentos d JOIN documento_pagamentos dp ON dp.documento_id = d.id
         WHERE d.caixa_sessao_id = ? AND d.estado = 'N' AND dp.moeda IS NOT NULL GROUP BY dp.moeda ORDER BY dp.moeda",
    )
    .bind(sid)
    .fetch_all(p)
    .await?;
    let em_moeda_kz: i64 = estrangeiras.iter().map(|r| r.get::<i64, _>("kz")).sum();
    let numerario: i64 = por_modo.iter().filter(|r| r.get::<String, _>("codigo") == "NU").map(|r| r.get::<i64, _>("total")).sum::<i64>() - em_moeda_kz;
    let inicial: i64 = s.get("valor_inicial");
    let esperado = inicial + numerario + entradas - saidas;
    let aberta = s.get::<String, _>("estado") == "ABERTA";
    let mut sessao = db::row_json(&s);
    for k in ["contagem", "contado_modos"] {
        if let Some(v) = sessao.get(k).and_then(|v| v.as_str()).and_then(|t| serde_json::from_str::<Value>(t).ok()) {
            sessao[k] = v;
        }
    }
    // conferência do fecho: o esperado de cada forma de pagamento face ao contado
    let contados = sessao["contado_modos"].clone();
    let conferencia: Vec<Value> = if aberta {
        Vec::new()
    } else {
        por_modo
            .iter()
            .filter_map(|r| {
                let codigo: String = r.get("codigo");
                let total: i64 = r.get("total");
                let (esp, cont) = if codigo == "NU" {
                    (esperado, s.get::<Option<i64>, _>("valor_contado"))
                } else {
                    (total, contados.get(&codigo).and_then(|v| v.as_i64()))
                };
                (codigo == "NU" || total != 0 || cont.is_some()).then(|| {
                    json!({ "codigo": codigo, "descricao": r.get::<String, _>("descricao"), "esperado": esp, "contado": cont, "diferenca": cont.map(|c| c - esp) })
                })
            })
            .collect()
    };
    let mut v = json!({
        "sessao": sessao,
        "por_modo": db::rows_json(&por_modo),
        "documentos": docs,
        "total_vendido": vendido,
        "entradas": entradas,
        "saidas": saidas,
        "movimentos": db::rows_json(&movimentos),
        "artigos": db::rows_json(&artigos),
        "numerario_esperado": esperado,
        "moedas_estrangeiras": db::rows_json(&estrangeiras),
        "conferencia": conferencia,
        "cego": false,
    });
    // fecho cego: com o caixa aberto, quem conta não vê o que o sistema espera
    if aberta && fecho_cego(st) && !st.sessao().map(|u| u.tem("caixa.ver_totais")).unwrap_or(false) {
        v["cego"] = json!(true);
        v["numerario_esperado"] = Value::Null;
        v["total_vendido"] = Value::Null;
        v["artigos"] = json!([]);
        v["moedas_estrangeiras"] = json!([]);
        if let Some(modos) = v["por_modo"].as_array_mut() {
            for m in modos {
                m["total"] = Value::Null;
            }
        }
    }
    Ok(v)
}

/// O posto fecha o caixa às cegas (opção do posto «Fecho de caixa cego»).
fn fecho_cego(st: &AppState) -> bool {
    st.posto().opcoes.get("fecho_cego").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Notas e moedas de kwanza aceites na contagem.
pub const DENOMINACOES: [i64; 12] = [5000, 2000, 1000, 500, 200, 100, 50, 20, 10, 5, 2, 1];

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    match sessao_aberta(st).await? {
        Some(id) => resumo(st, id).await,
        None => Ok(Value::Null),
    }
}

/// Resumo de qualquer sessão (reimpressão de fechos anteriores).
pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    resumo(st, i.id).await
}

/// Fechos de caixa (menu "Fechos de Caixa (POS)" do GC).
pub async fn sessoes(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT c.id, c.posto_id, ps.descricao AS posto, u.nome AS utilizador, c.aberta_em, c.fechada_em, c.valor_inicial,
                c.valor_contado, c.estado, c.numero_z,
                (SELECT COALESCE(SUM(total), 0) FROM documentos d WHERE d.caixa_sessao_id = c.id AND d.estado = 'N' AND d.tipo_codigo IN ('FR','RC')) AS total_vendido
         FROM caixa_sessoes c JOIN utilizadores u ON u.id = c.utilizador_id LEFT JOIN postos ps ON ps.id = c.posto_id
         ORDER BY c.aberta_em DESC, c.id DESC LIMIT 200",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct AbrirIn {
    valor_inicial: i64,
}

pub async fn abrir(st: &AppState, i: AbrirIn) -> AppResult<Value> {
    let s = st.sessao()?;
    if sessao_aberta(st).await?.is_some() {
        return Err(AppError::msg("Já existe um caixa aberto neste posto."));
    }
    let id = sqlx::query("INSERT INTO caixa_sessoes (posto_id, utilizador_id, aberta_em, valor_inicial) VALUES (?, ?, ?, ?)")
        .bind(st.posto().id)
        .bind(s.id)
        .bind(db::agora())
        .bind(i.valor_inicial.max(0))
        .execute(&st.pool)
        .await?
        .last_insert_rowid();
    resumo(st, id).await
}

#[derive(Deserialize)]
pub struct MovIn {
    tipo: String,
    valor: i64,
    descricao: String,
    rubrica: Option<String>,
    modo: Option<String>,
}

pub async fn movimento(st: &AppState, m: MovIn) -> AppResult<Value> {
    st.exigir_perm("caixa.movimentos")?;
    let sid = sessao_aberta(st).await?.ok_or_else(|| AppError::msg("Não há caixa aberto."))?;
    if m.tipo != "ENTRADA" && m.tipo != "SAIDA" {
        return Err(AppError::msg("Tipo de movimento inválido."));
    }
    if m.valor <= 0 || m.descricao.trim().is_empty() {
        return Err(AppError::msg("Indique o valor e a descrição."));
    }
    sqlx::query("INSERT INTO caixa_movimentos (sessao_id, data_hora, tipo, valor, descricao, rubrica, modo_codigo) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(sid)
        .bind(db::agora())
        .bind(&m.tipo)
        .bind(m.valor)
        .bind(m.descricao.trim())
        .bind(m.rubrica.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(m.modo.as_deref().unwrap_or("NU"))
        .execute(&st.pool)
        .await?;
    resumo(st, sid).await
}

#[derive(Deserialize)]
pub struct FecharIn {
    /// Total contado em numerário (cêntimos); dispensado quando vem a contagem por notas e moedas.
    #[serde(default)]
    valor_contado: Option<i64>,
    /// Notas e moedas contadas: valor facial em Kz → quantidade.
    #[serde(default)]
    contagem: Option<std::collections::BTreeMap<String, i64>>,
    /// Valores contados das outras formas de pagamento (talão do TPA, transferências…), em cêntimos.
    #[serde(default)]
    contado_modos: Option<std::collections::BTreeMap<String, i64>>,
    #[serde(default)]
    observacoes: Option<String>,
}

pub async fn fechar(st: &AppState, f: FecharIn) -> AppResult<Value> {
    let s = st.exigir_perm("pos.fecho_caixa")?;
    let sid = sessao_aberta(st).await?.ok_or_else(|| AppError::msg("Não há caixa aberto."))?;
    // contagem por notas e moedas (o total sai daí) ou o total indicado
    let mut contagem = std::collections::BTreeMap::new();
    let valor_contado = match &f.contagem {
        Some(c) if !c.is_empty() => {
            let mut total = 0i64;
            for (face, qtd) in c {
                let kz: i64 = face.trim().parse().map_err(|_| AppError::msg(format!("Nota ou moeda inválida: {face}")))?;
                if !DENOMINACOES.contains(&kz) {
                    return Err(AppError::msg(format!("Não há notas nem moedas de {kz} Kz.")));
                }
                if *qtd < 0 {
                    return Err(AppError::msg("As quantidades contadas não podem ser negativas."));
                }
                if *qtd > 0 {
                    contagem.insert(kz.to_string(), *qtd);
                    total += kz * 100 * qtd;
                }
            }
            total
        }
        _ => f.valor_contado.ok_or_else(|| AppError::msg("Indique o numerário contado (por notas e moedas ou o total)."))?,
    };
    if valor_contado < 0 {
        return Err(AppError::msg("O valor contado não pode ser negativo."));
    }
    let modos: std::collections::BTreeMap<String, i64> = f.contado_modos.unwrap_or_default().into_iter().filter(|(k, v)| k != "NU" && *v >= 0).collect();
    let mut tx = crate::db::transacao(&st.pool).await?;
    let numero_z: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(numero_z), 0) + 1 FROM caixa_sessoes WHERE posto_id IS ?")
        .bind(st.posto().id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE caixa_sessoes SET estado = 'FECHADA', fechada_em = ?, valor_contado = ?, numero_z = ?, contagem = ?, contado_modos = ?,
                cego = ?, fechada_por = ?, observacoes = ? WHERE id = ?",
    )
    .bind(db::agora())
    .bind(valor_contado)
    .bind(numero_z)
    .bind((!contagem.is_empty()).then(|| serde_json::to_string(&contagem).unwrap_or_default()))
    .bind((!modos.is_empty()).then(|| serde_json::to_string(&modos).unwrap_or_default()))
    .bind(fecho_cego(st) as i64)
    .bind(s.id)
    .bind(f.observacoes.as_deref().map(str::trim).filter(|x| !x.is_empty()))
    .bind(sid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let r = resumo(st, sid).await?;
    let dif = valor_contado - r["numerario_esperado"].as_i64().unwrap_or(0);
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'FECHO_CAIXA', ?)")
        .bind(s.id)
        .bind(format!("Z n.º {numero_z} (sessão {sid}): contado {}, diferença {}", crate::agt::total_fmt(valor_contado), crate::agt::total_fmt(dif)))
        .execute(&st.pool)
        .await?;
    // o fecho de caixa é um bom momento para uma cópia de segurança
    super::sistema::copia_evento(st, "Cópia no fecho de caixa").await;
    Ok(r)
}

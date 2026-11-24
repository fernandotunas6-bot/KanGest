// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Verificação da integridade dos documentos assinados (cadeia AGT) — o que faria um auditor:
//! série a série, refaz a mensagem de cada documento (`Data;DataHora;Número;Total;HashAnterior`),
//! confirma a assinatura com as chaves públicas conhecidas, a numeração sem saltos, os 4 caracteres
//! de controlo, os totais face às linhas e o estado guardado de cada série. Confirma também que os
//! triggers que protegem os documentos assinados (migração 0018) continuam na base de dados.

use rsa::RsaPublicKey;
use serde_json::{json, Value};
use sqlx::Row;

use crate::agt;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Triggers da migração 0018 (protecção dos documentos assinados).
pub const TRIGGERS: [&str; 9] = [
    "trg_doc_assinado_upd",
    "trg_doc_anulado",
    "trg_doc_assinado_del",
    "trg_linha_assinada_upd",
    "trg_linha_assinada_del",
    "trg_pagamento_assinado_upd",
    "trg_pagamento_assinado_del",
    "trg_serie_recuar",
    "trg_serie_del",
];

/// Erros mostrados por série (os restantes só contam).
const MAX_ERROS: usize = 50;

struct Doc {
    numero: i64,
    numero_doc: String,
    data: String,
    data_hora: String,
    total: i64,
    hash: String,
    controlo: String,
    importado: bool,
    com_linhas: bool,
    liquido: i64,
    iva: i64,
    soma_linhas: i64,
    soma_liquido: i64,
    soma_iva: i64,
}

struct Resultado {
    verificados: usize,
    erros: Vec<Value>,
    total_erros: usize,
}

fn erro(r: &mut Resultado, numero_doc: &str, problema: String) {
    r.total_erros += 1;
    if r.erros.len() < MAX_ERROS {
        r.erros.push(json!({ "documento": numero_doc, "problema": problema }));
    }
}

/// Verifica uma série (trabalho de CPU: corre fora do executor assíncrono).
fn verificar_serie(chaves: &[RsaPublicKey], docs: &[Doc], estado_serie: Option<(i64, String)>) -> Resultado {
    let mut r = Resultado { verificados: 0, erros: Vec::new(), total_erros: 0 };
    let mut anterior: Option<&Doc> = None;
    for d in docs {
        if d.importado {
            // histórico do KIGEST: assinado com a chave do sistema antigo; só conta para a cadeia
            anterior = Some(d);
            continue;
        }
        r.verificados += 1;
        match anterior {
            Some(a) if d.numero != a.numero + 1 => {
                let falta = if d.numero == a.numero + 2 { format!("o n.º {}", a.numero + 1) } else { format!("os n.ºs {} a {}", a.numero + 1, d.numero - 1) };
                erro(&mut r, &d.numero_doc, format!("Numeração com salto: falta {falta} (documentos apagados?)."));
            }
            None if d.numero != 1 => erro(&mut r, &d.numero_doc, format!("A série começa no n.º {} em vez do n.º 1.", d.numero)),
            _ => {}
        }
        let hash_anterior = anterior.map(|a| a.hash.as_str()).unwrap_or("");
        if d.hash.trim().is_empty() {
            erro(&mut r, &d.numero_doc, "Documento sem assinatura.".into());
        } else {
            let msg = agt::mensagem(&d.data, &d.data_hora, &d.numero_doc, d.total, hash_anterior);
            if !agt::confere(chaves, &msg, &d.hash) {
                erro(
                    &mut r,
                    &d.numero_doc,
                    "A assinatura não confere: os dados do documento foram alterados, a cadeia foi quebrada ou foi assinado com uma chave desconhecida.".into(),
                );
            }
            if d.controlo != agt::controlo(&d.hash) {
                erro(&mut r, &d.numero_doc, "Os 4 caracteres de controlo não correspondem à assinatura.".into());
            }
        }
        if d.com_linhas && (d.total != d.liquido + d.iva || d.soma_linhas != d.total || d.soma_liquido != d.liquido || d.soma_iva != d.iva) {
            erro(
                &mut r,
                &d.numero_doc,
                format!("Os totais não correspondem às linhas (documento {}, linhas {}).", agt::valor_pt(d.total), agt::valor_pt(d.soma_linhas)),
            );
        }
        anterior = Some(d);
    }
    if let (Some(u), Some((ultimo_numero, ultimo_hash))) = (docs.last(), estado_serie) {
        if ultimo_numero != u.numero {
            erro(&mut r, &u.numero_doc, format!("A série indica o n.º {ultimo_numero} como último documento, mas o último existente é o n.º {}.", u.numero));
        } else if !u.hash.is_empty() && ultimo_hash != u.hash {
            erro(&mut r, &u.numero_doc, "A assinatura guardada na série não é a do último documento.".into());
        }
    }
    r
}

/// Sistema → Certificação AGT → «Verificar integridade».
pub async fn verificar(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    if !(s.tem("sistema.certificacao") || s.tem("sistema.auditoria")) {
        return Err(AppError::SemPermissao);
    }
    let p = &st.pool;
    let chaves = agt::chaves_publicas(p).await?;
    let series: Vec<(String, String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT x.tipo_codigo, x.serie, s.ultimo_numero, s.ultimo_hash
         FROM (SELECT DISTINCT d.tipo_codigo, d.serie FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
               WHERE t.assinado = 1 AND d.importado = 0) x
         LEFT JOIN series s ON s.tipo_codigo = x.tipo_codigo AND s.serie = x.serie
         ORDER BY x.tipo_codigo, x.serie",
    )
    .fetch_all(p)
    .await?;
    let mut lista = Vec::new();
    let (mut verificados, mut com_erro) = (0usize, 0usize);
    for (tipo, serie, ultimo_numero, ultimo_hash) in series {
        let rows = sqlx::query(
            "SELECT d.numero, d.numero_doc, d.data_emissao, d.data_hora, d.total, d.hash, d.hash_controlo, d.importado,
                    d.total_liquido, d.total_iva, t.classe,
                    COUNT(l.id) AS n_linhas, COALESCE(SUM(l.total), 0) AS soma_total,
                    COALESCE(SUM(l.valor_liquido), 0) AS soma_liquido, COALESCE(SUM(l.valor_iva), 0) AS soma_iva
             FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             LEFT JOIN documento_linhas l ON l.documento_id = d.id
             WHERE d.tipo_codigo = ? AND d.serie = ?
             GROUP BY d.id ORDER BY d.numero",
        )
        .bind(&tipo)
        .bind(&serie)
        .fetch_all(p)
        .await?;
        let docs: Vec<Doc> = rows
            .iter()
            .map(|r| {
                let classe: String = r.get("classe");
                let n_linhas: i64 = r.get("n_linhas");
                Doc {
                    numero: r.get("numero"),
                    numero_doc: r.get("numero_doc"),
                    data: r.get("data_emissao"),
                    data_hora: r.get("data_hora"),
                    total: r.get("total"),
                    hash: r.get("hash"),
                    controlo: r.get("hash_controlo"),
                    importado: r.get::<i64, _>("importado") == 1,
                    // recibos, pagamentos e adiantamentos não têm linhas de artigos
                    com_linhas: n_linhas > 0 && !matches!(classe.as_str(), "RECIBO" | "PAGAMENTO" | "ADIANTAMENTO"),
                    liquido: r.get("total_liquido"),
                    iva: r.get("total_iva"),
                    soma_linhas: r.get("soma_total"),
                    soma_liquido: r.get("soma_liquido"),
                    soma_iva: r.get("soma_iva"),
                }
            })
            .collect();
        let estado = ultimo_numero.map(|n| (n, ultimo_hash.unwrap_or_default()));
        let ch = chaves.clone();
        let (res, docs) = tokio::task::spawn_blocking(move || (verificar_serie(&ch, &docs, estado), docs))
            .await
            .map_err(|e| AppError::msg(e.to_string()))?;
        verificados += res.verificados;
        if res.total_erros > 0 {
            com_erro += 1;
        }
        let proprios: Vec<&Doc> = docs.iter().filter(|d| !d.importado).collect();
        lista.push(json!({
            "tipo": tipo, "serie": serie, "documentos": res.verificados,
            "primeiro": proprios.first().map(|d| d.numero_doc.clone()), "ultimo": proprios.last().map(|d| d.numero_doc.clone()),
            "erros": res.erros, "total_erros": res.total_erros,
        }));
    }
    let existentes: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'trigger'").fetch_all(p).await?;
    let em_falta: Vec<&str> = TRIGGERS.iter().copied().filter(|t| !existentes.iter().any(|e| e == t)).collect();
    let ok = com_erro == 0 && em_falta.is_empty() && !chaves.is_empty();
    let resumo = if ok {
        format!("{verificados} documentos verificados: cadeia íntegra.")
    } else {
        format!("{verificados} documentos verificados: {com_erro} série(s) com problemas; {} protecção(ões) em falta.", em_falta.len())
    };
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'VERIFICAR_INTEGRIDADE', ?)")
        .bind(s.id)
        .bind(&resumo)
        .execute(p)
        .await?;
    Ok(json!({
        "ok": ok, "resumo": resumo, "verificados": verificados, "series": lista,
        "chaves": chaves.len(),
        "protecao": { "ativa": em_falta.is_empty(), "em_falta": em_falta },
        "data": crate::db::agora(),
    }))
}

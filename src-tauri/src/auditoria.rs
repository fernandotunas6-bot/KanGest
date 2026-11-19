//! Auditoria à prova de adulteração (docs/UTILIZADORES.md, «Auditoria»).
//!
//! Cada linha da tabela `auditoria` leva um **selo**: o SHA-256 do selo da linha anterior com o conteúdo desta. Os
//! gatilhos da migração 0037 não deixam alterar nem apagar linhas pelo programa (o selo escreve-se uma vez); quem mexer
//! na base de dados por fora parte a cadeia, e `verificar` diz em que linha. O último selo fica também em `config`
//! (`auditoria.ultimo_selo`), para se dar pela falta de linhas apagadas no fim.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::error::AppResult;

const CHAVE_ULTIMO: &str = "auditoria.ultimo_selo";
/// Acção da linha que denuncia linhas apagadas no fim do registo.
pub const QUEBRA: &str = "AUDITORIA_QUEBRA";
/// Um selador de cada vez (vários pedidos a gravar ao mesmo tempo não podem selar a mesma linha duas vezes).
static SELAR: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Linhas lidas de cada vez ao selar e ao verificar (o registo pode ter centenas de milhares).
const BLOCO: i64 = 2000;

const COLUNAS: &str = "id, data_hora, utilizador_id, acao, detalhe, entidade, registo_id, antes, depois, posto_id, origem, selo";

/// O selo de uma linha: SHA-256 (hex) do selo anterior e de todos os campos, separados por um carácter que não se
/// escreve (0x1F), na ordem da tabela.
fn selo_de(anterior: &str, r: &sqlx::sqlite::SqliteRow) -> String {
    let txt = |c: &str| r.try_get::<Option<String>, _>(c).ok().flatten().unwrap_or_default();
    let num = |c: &str| r.try_get::<Option<i64>, _>(c).ok().flatten().map(|n| n.to_string()).unwrap_or_default();
    let campos = [
        anterior.to_string(),
        num("id"),
        txt("data_hora"),
        num("utilizador_id"),
        txt("acao"),
        txt("detalhe"),
        txt("entidade"),
        txt("registo_id"),
        txt("antes"),
        txt("depois"),
        num("posto_id"),
        txt("origem"),
    ];
    let mut h = Sha256::new();
    h.update(campos.join("\u{1f}").as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Sela as linhas ainda sem selo (as novas e, na primeira vez, as de antes da auditoria em cadeia), pela ordem, numa
/// só transacção e por blocos. As linhas novas têm sempre o id maior, por isso as que faltam selar estão todas no fim.
pub async fn selar(pool: &SqlitePool) -> AppResult<u64> {
    let _vez = SELAR.lock().await;
    let mut tx = crate::db::transacao(pool).await?;
    // O marcador tem de bater com a última linha selada. Se não bate, alguém apagou (ou mudou) linhas no fim do
    // registo por fora: fica uma linha a dizê-lo, selada como as outras, antes de a cadeia continuar.
    let marcador: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE_ULTIMO).fetch_optional(&mut *tx).await?;
    if let Some((id, selo)) = marcador.as_deref().and_then(|m| m.split_once(':')) {
        let ainda: Option<String> = sqlx::query_scalar("SELECT selo FROM auditoria WHERE id = ?").bind(id.parse::<i64>().unwrap_or(0)).fetch_optional(&mut *tx).await?.flatten();
        if ainda.as_deref() != Some(selo) {
            sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (NULL, ?, ?)")
                .bind(QUEBRA)
                .bind(format!("O registo de auditoria foi mexido fora do KanGest: a linha n.º {id}, a última selada, foi apagada ou alterada."))
                .execute(&mut *tx)
                .await?;
        }
    }
    let anterior: Option<(i64, String)> = sqlx::query_as("SELECT id, selo FROM auditoria WHERE selo IS NOT NULL ORDER BY id DESC LIMIT 1").fetch_optional(&mut *tx).await?;
    let (mut desde, mut selo) = anterior.unwrap_or_default();
    let mut n = 0;
    loop {
        let linhas = sqlx::query(&format!("SELECT {COLUNAS} FROM auditoria WHERE id > ? ORDER BY id LIMIT {BLOCO}")).bind(desde).fetch_all(&mut *tx).await?;
        if linhas.is_empty() {
            break;
        }
        for r in &linhas {
            selo = selo_de(&selo, r);
            desde = r.get("id");
            sqlx::query("UPDATE auditoria SET selo = ? WHERE id = ? AND selo IS NULL").bind(&selo).bind(desde).execute(&mut *tx).await?;
            n += 1;
        }
    }
    if n > 0 {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
            .bind(CHAVE_ULTIMO)
            .bind(format!("{desde}:{selo}"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(n)
}

/// Sela só se a linha mais recente ainda não tiver selo (uma consulta rápida; corre no fim de cada pedido).
pub async fn selar_novas(pool: &SqlitePool) -> AppResult<u64> {
    let falta: Option<bool> = sqlx::query_scalar("SELECT selo IS NULL FROM auditoria ORDER BY id DESC LIMIT 1").fetch_optional(pool).await?;
    if falta == Some(true) {
        selar(pool).await
    } else {
        Ok(0)
    }
}

/// Confere a cadeia toda: cada selo tem de bater com o anterior e com o conteúdo da linha, e não pode haver linhas
/// apagadas no fim (`QUEBRA`). Devolve `integra`, os totais e, se não, a linha com problema e porquê.
pub async fn verificar(pool: &SqlitePool) -> AppResult<Value> {
    selar(pool).await?;
    let mut anterior = String::new();
    let mut problema: Option<Value> = None;
    let (mut desde, mut total) = (0i64, 0u64);
    let (mut primeiro, mut ultimo): (Option<String>, Option<String>) = (None, None);
    'blocos: loop {
        let linhas = sqlx::query(&format!("SELECT {COLUNAS} FROM auditoria WHERE id > ? ORDER BY id LIMIT {BLOCO}")).bind(desde).fetch_all(pool).await?;
        if linhas.is_empty() {
            break;
        }
        for r in &linhas {
            desde = r.get("id");
            let data: String = r.get("data_hora");
            let guardado: Option<String> = r.get("selo");
            if guardado.as_deref() != Some(selo_de(&anterior, r).as_str()) {
                problema = Some(json!({
                    "id": desde,
                    "data_hora": data,
                    "motivo": if guardado.is_none() {
                        "Há uma linha sem selo no meio do registo."
                    } else {
                        "Esta linha, ou a que estava antes dela, foi alterada ou apagada fora do KanGest."
                    },
                }));
                break 'blocos;
            }
            anterior = guardado.unwrap_or_default();
            total += 1;
            primeiro.get_or_insert_with(|| data.clone());
            ultimo = Some(data);
        }
    }
    // linhas apagadas no fim (o `selar` acima deixou-o escrito no registo)
    let quebras: Vec<(i64, String, String)> = sqlx::query_as("SELECT id, data_hora, detalhe FROM auditoria WHERE acao = ? ORDER BY id DESC").bind(QUEBRA).fetch_all(pool).await?;
    if let (None, Some((id, data, detalhe))) = (&problema, quebras.first()) {
        problema = Some(json!({ "id": id, "data_hora": data, "motivo": detalhe }));
    }
    let linhas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria").fetch_one(pool).await?;
    Ok(json!({
        "integra": problema.is_none(),
        "total": linhas,
        "conferidas": total,
        "quebras": quebras.len(),
        "primeiro": primeiro,
        "ultimo": ultimo,
        "problema": problema,
        "verificado_em": crate::db::agora(),
    }))
}

/// Uma linha nova de auditoria, já selada. `antes`/`depois`: o que mudou, em JSON.
pub struct Linha<'a> {
    pub utilizador_id: Option<i64>,
    pub acao: &'a str,
    pub detalhe: &'a str,
    pub entidade: Option<&'a str>,
    pub registo_id: Option<&'a str>,
    pub antes: Option<&'a Value>,
    pub depois: Option<&'a Value>,
    pub posto_id: Option<i64>,
    pub origem: &'a str,
}

pub async fn registar(pool: &SqlitePool, l: Linha<'_>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO auditoria (utilizador_id, acao, detalhe, entidade, registo_id, antes, depois, posto_id, origem)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(l.utilizador_id)
    .bind(l.acao)
    .bind(l.detalhe)
    .bind(l.entidade)
    .bind(l.registo_id)
    .bind(l.antes.map(|v| v.to_string()))
    .bind(l.depois.map(|v| v.to_string()))
    .bind(l.posto_id)
    .bind(l.origem)
    .execute(pool)
    .await?;
    selar(pool).await?;
    Ok(())
}

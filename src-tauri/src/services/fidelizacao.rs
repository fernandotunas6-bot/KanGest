// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Fidelização de clientes (cartão de pontos), como nos POS de retalho de referência.
//!
//! - Cada venda a um cliente identificado (FT, FR, ND) dá 1 ponto por cada `compra_por_ponto`
//!   pagos; o que se paga com pontos não dá pontos.
//! - Os pontos pagam vendas e recibos como meio de pagamento «DE» (no SAF-T AO: dinheiro
//!   electrónico, «cartões de fidelidade ou de pontos»), a `valor_ponto` cada, quando o saldo
//!   chega a `minimo_resgate`. Não é desconto: a factura e o IVA ficam iguais.
//! - A nota de crédito estorna os pontos ganhos na proporção do valor creditado e devolve em
//!   pontos (não em dinheiro) a parte da venda que foi paga com pontos.
//! - A anulação de um documento reverte todos os seus movimentos de pontos.
//!
//! O saldo é a soma dos movimentos (`fidelizacao_movimentos`), que ficam como histórico do cliente.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqliteConnection;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub ativa: bool,
    /// Compra (cêntimos, com IVA) que dá um ponto.
    pub compra_por_ponto: i64,
    /// Valor de cada ponto no pagamento (cêntimos).
    pub valor_ponto: i64,
    /// Saldo mínimo para pagar com pontos.
    pub minimo_resgate: i64,
}

impl Default for Cfg {
    fn default() -> Self {
        // 1 ponto por cada 1 000 Kz; cada ponto vale 10 Kz (1 % devolvido em pontos)
        Cfg { ativa: false, compra_por_ponto: 100_000, valor_ponto: 1_000, minimo_resgate: 50 }
    }
}

pub async fn cfg<'e, E: sqlx::SqliteExecutor<'e>>(e: E) -> AppResult<Cfg> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'fidelizacao'").fetch_optional(e).await?;
    Ok(v.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
}

pub async fn saldo_conn(c: &mut SqliteConnection, entidade_id: i64) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COALESCE(SUM(pontos), 0) FROM fidelizacao_movimentos WHERE entidade_id = ?")
        .bind(entidade_id)
        .fetch_one(&mut *c)
        .await?)
}

/// Pontos necessários para pagar `valor` cêntimos (para cima: o ponto não se divide).
pub fn pontos_para(valor: i64, c: &Cfg) -> i64 {
    let v = c.valor_ponto.max(1);
    (valor.max(0) + v - 1) / v
}

/// Valida o pagamento com pontos de uma venda ou recibo e devolve os pontos a gastar.
pub async fn validar_resgate(conn: &mut SqliteConnection, c: &Cfg, entidade_id: i64, valor: i64) -> AppResult<i64> {
    if !c.ativa {
        return Err(AppError::msg("A fidelização de clientes não está activa: não se pode pagar com pontos."));
    }
    let saldo = saldo_conn(conn, entidade_id).await?;
    if saldo < c.minimo_resgate.max(1) {
        return Err(AppError::msg(format!("O cliente tem {saldo} pontos: só pode pagar com pontos a partir de {}.", c.minimo_resgate.max(1))));
    }
    let pontos = pontos_para(valor, c);
    if pontos > saldo {
        return Err(AppError::msg(format!("Pontos insuficientes: este pagamento precisa de {pontos} pontos e o cliente tem {saldo}.")));
    }
    Ok(pontos)
}

#[allow(clippy::too_many_arguments)]
async fn movimento(
    conn: &mut SqliteConnection,
    entidade_id: i64,
    documento_id: Option<i64>,
    tipo: &str,
    pontos: i64,
    valor: i64,
    descricao: &str,
    utilizador_id: i64,
    agora: &str,
) -> AppResult<()> {
    if pontos == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO fidelizacao_movimentos (entidade_id, documento_id, data_hora, tipo, pontos, valor, descricao, utilizador_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entidade_id)
    .bind(documento_id)
    .bind(agora)
    .bind(tipo)
    .bind(pontos)
    .bind(valor)
    .bind(descricao)
    .bind(utilizador_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Venda ou recibo acabado de emitir: pontos gastos no pagamento e pontos ganhos na compra.
pub struct Venda<'a> {
    pub documento_id: i64,
    pub numero: &'a str,
    pub entidade_id: i64,
    /// a venda dá pontos (FT, FR, ND); os recibos só os gastam
    pub ganha: bool,
    /// valor a pagar do documento (cêntimos)
    pub a_pagar: i64,
    /// pago com pontos: valor e pontos gastos
    pub pago_pontos: i64,
    pub pontos_gastos: i64,
    pub utilizador_id: i64,
    pub agora: &'a str,
}

pub async fn registar_venda(conn: &mut SqliteConnection, c: &Cfg, v: Venda<'_>) -> AppResult<()> {
    movimento(conn, v.entidade_id, Some(v.documento_id), "RESGATE", -v.pontos_gastos, v.pago_pontos, &format!("Pagamento de {}", v.numero), v.utilizador_id, v.agora).await?;
    if v.ganha && c.ativa && c.compra_por_ponto > 0 {
        let base = (v.a_pagar - v.pago_pontos).max(0);
        movimento(conn, v.entidade_id, Some(v.documento_id), "GANHO", base / c.compra_por_ponto, base, &format!("Compra {}", v.numero), v.utilizador_id, v.agora).await?;
    }
    Ok(())
}

/// Nota de crédito de uma venda: estorna os pontos ganhos na proporção do valor creditado e
/// devolve em pontos a parte paga com pontos. Devolve o valor (cêntimos) reembolsado em pontos,
/// que deixa de ser devolvido em dinheiro ou crédito.
#[allow(clippy::too_many_arguments)]
pub async fn nota_credito(
    conn: &mut SqliteConnection,
    documento_id: i64,
    numero: &str,
    entidade_id: i64,
    origem_id: i64,
    origem_total: i64,
    total: i64,
    utilizador_id: i64,
    agora: &str,
) -> AppResult<i64> {
    if origem_total <= 0 || entidade_id == 1 {
        return Ok(0);
    }
    // o que a factura de origem movimentou…
    let (ganhos, gastos, pago): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN tipo = 'GANHO' THEN pontos END), 0), COALESCE(SUM(CASE WHEN tipo = 'RESGATE' THEN -pontos END), 0),
                COALESCE(SUM(CASE WHEN tipo = 'RESGATE' THEN valor END), 0)
         FROM fidelizacao_movimentos WHERE documento_id = ?",
    )
    .bind(origem_id)
    .fetch_one(&mut *conn)
    .await?;
    if ganhos == 0 && gastos == 0 {
        return Ok(0);
    }
    // …e o que outras notas de crédito (não anuladas) já acertaram
    let (estornados, devolvidos, devolvido_valor): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN m.tipo = 'ESTORNO' THEN -m.pontos END), 0), COALESCE(SUM(CASE WHEN m.tipo = 'DEVOLUCAO' THEN m.pontos END), 0),
                COALESCE(SUM(CASE WHEN m.tipo = 'DEVOLUCAO' THEN m.valor END), 0)
         FROM fidelizacao_movimentos m JOIN documentos d ON d.id = m.documento_id
         WHERE d.doc_origem_id = ? AND d.estado <> 'A' AND d.id <> ?",
    )
    .bind(origem_id)
    .bind(documento_id)
    .fetch_one(&mut *conn)
    .await?;
    let parte = |x: i64| (x as f64 * total.min(origem_total) as f64 / origem_total as f64).round() as i64;
    let estorno = parte(ganhos).min(ganhos - estornados).max(0);
    let devolve = parte(gastos).min(gastos - devolvidos).max(0);
    let devolve_valor = parte(pago).min(pago - devolvido_valor).min(total).max(0);
    movimento(conn, entidade_id, Some(documento_id), "ESTORNO", -estorno, total, &format!("Nota de crédito {numero}"), utilizador_id, agora).await?;
    movimento(conn, entidade_id, Some(documento_id), "DEVOLUCAO", devolve, devolve_valor, &format!("Devolvidos por {numero}"), utilizador_id, agora).await?;
    Ok(if devolve > 0 { devolve_valor } else { 0 })
}

/// Anulação de um documento: reverte os seus movimentos de pontos.
pub async fn anular(conn: &mut SqliteConnection, documento_id: i64, numero: &str, utilizador_id: i64, agora: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO fidelizacao_movimentos (entidade_id, documento_id, data_hora, tipo, pontos, valor, descricao, utilizador_id)
         SELECT entidade_id, documento_id, ?, 'ANULACAO', -SUM(pontos), 0, ?, ?
         FROM fidelizacao_movimentos WHERE documento_id = ? GROUP BY entidade_id, documento_id HAVING SUM(pontos) <> 0",
    )
    .bind(agora)
    .bind(format!("Anulação de {numero}"))
    .bind(utilizador_id)
    .bind(documento_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Pontos de um documento (para o talão): ganhos, gastos e o saldo do cliente.
pub async fn do_documento(p: &sqlx::SqlitePool, documento_id: i64) -> AppResult<Value> {
    let r: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT entidade_id, COALESCE(SUM(CASE WHEN tipo IN ('GANHO', 'DEVOLUCAO') THEN pontos END), 0),
                COALESCE(SUM(CASE WHEN tipo IN ('RESGATE', 'ESTORNO') THEN -pontos END), 0)
         FROM fidelizacao_movimentos WHERE documento_id = ? AND tipo <> 'ANULACAO' GROUP BY entidade_id",
    )
    .bind(documento_id)
    .fetch_optional(p)
    .await?;
    let Some((entidade, ganhos, gastos)) = r else { return Ok(Value::Null) };
    let mut c = p.acquire().await?;
    Ok(json!({ "ganhos": ganhos, "gastos": gastos, "saldo": saldo_conn(&mut c, entidade).await? }))
}

// ------------------------------------------------------------------ RPC

pub async fn config_obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(json!(cfg(&st.pool).await?))
}

pub async fn config_guardar(st: &AppState, c: Cfg) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    if c.compra_por_ponto <= 0 || c.valor_ponto <= 0 {
        return Err(AppError::msg("Indique quanto se compra por ponto e quanto vale cada ponto."));
    }
    if c.minimo_resgate < 1 {
        return Err(AppError::msg("O saldo mínimo para pagar com pontos é de pelo menos 1 ponto."));
    }
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('fidelizacao', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(serde_json::to_string(&c)?)
        .execute(&st.pool)
        .await?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct EntidadeIn {
    entidade_id: i64,
}

/// Saldo de pontos de um cliente e quanto vale (o POS mostra-o ao escolher o cliente).
pub async fn saldo(st: &AppState, i: EntidadeIn) -> AppResult<Value> {
    st.sessao()?;
    let c = cfg(&st.pool).await?;
    let mut conn = st.pool.acquire().await?;
    let pontos = saldo_conn(&mut conn, i.entidade_id).await?;
    Ok(json!({
        "pontos": pontos,
        "valor": pontos.max(0) * c.valor_ponto,
        "pode_pagar": c.ativa && i.entidade_id != 1 && pontos >= c.minimo_resgate.max(1),
    }))
}

#[derive(Deserialize)]
pub struct ClientesIn {
    q: Option<String>,
}

/// Clientes com pontos (ou movimentos), do maior saldo para o menor.
pub async fn clientes(st: &AppState, i: ClientesIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let q = format!("%{}%", i.q.unwrap_or_default().trim());
    let rows = sqlx::query(
        "SELECT e.id, e.nome, e.nif, e.telefone, SUM(m.pontos) AS saldo,
                SUM(CASE WHEN m.tipo = 'GANHO' THEN m.pontos ELSE 0 END) AS ganhos,
                SUM(CASE WHEN m.tipo = 'RESGATE' THEN -m.pontos ELSE 0 END) AS gastos,
                SUM(CASE WHEN m.tipo = 'GANHO' THEN m.valor ELSE 0 END) AS compras,
                MAX(m.data_hora) AS ultimo
         FROM fidelizacao_movimentos m JOIN entidades e ON e.id = m.entidade_id
         WHERE e.nome LIKE ?1 OR e.nif LIKE ?1
         GROUP BY e.id ORDER BY saldo DESC, e.nome LIMIT 500",
    )
    .bind(&q)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

/// Histórico de pontos de um cliente.
pub async fn movimentos(st: &AppState, i: EntidadeIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let rows = sqlx::query(
        "SELECT m.*, d.numero_doc, u.nome AS utilizador FROM fidelizacao_movimentos m
         LEFT JOIN documentos d ON d.id = m.documento_id LEFT JOIN utilizadores u ON u.id = m.utilizador_id
         WHERE m.entidade_id = ? ORDER BY m.data_hora DESC, m.id DESC LIMIT 1000",
    )
    .bind(i.entidade_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct AjusteIn {
    entidade_id: i64,
    pontos: i64,
    motivo: String,
}

/// Ajuste manual do saldo (campanhas, correcções), com motivo e registo na auditoria.
pub async fn ajustar(st: &AppState, a: AjusteIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    if a.pontos == 0 {
        return Err(AppError::msg("Indique os pontos a somar (positivos) ou a retirar (negativos)."));
    }
    if a.motivo.trim().len() < 3 {
        return Err(AppError::msg("Indique o motivo do ajuste."));
    }
    if a.entidade_id == 1 {
        return Err(AppError::msg("O consumidor final não acumula pontos."));
    }
    let mut tx = db::transacao(&st.pool).await?;
    let nome: Option<String> = sqlx::query_scalar("SELECT nome FROM entidades WHERE id = ? AND tipo IN ('C', 'A')").bind(a.entidade_id).fetch_optional(&mut *tx).await?;
    let nome = nome.ok_or_else(|| AppError::msg("Cliente não encontrado."))?;
    let agora = db::agora();
    movimento(&mut tx, a.entidade_id, None, "AJUSTE", a.pontos, 0, a.motivo.trim(), s.id, &agora).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'PONTOS', ?)")
        .bind(s.id)
        .bind(format!("{nome}: {:+} pontos ({})", a.pontos, a.motivo.trim()))
        .execute(&mut *tx)
        .await?;
    let saldo = saldo_conn(&mut tx, a.entidade_id).await?;
    tx.commit().await?;
    Ok(json!({ "saldo": saldo }))
}

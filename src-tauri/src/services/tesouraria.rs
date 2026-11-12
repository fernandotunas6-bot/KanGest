//! Tesouraria (Fr_FluxoCaixa, Fr_Contas, Fr_CentroCusto, Fr_PagamentoVista, Fr_Deposito do KIGEST GC):
//! contas (caixa/banco), centros de custo, movimentos, transferências e fluxo de caixa.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Saldo de cada conta = saldo inicial + entradas − saídas (movimentos não anulados).
pub async fn contas(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT c.*,
            c.saldo_inicial
              + COALESCE((SELECT SUM(CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END) FROM vw_fluxo f
                          WHERE f.conta_id = c.id AND f.anulado = 0), 0) AS saldo
         FROM contas c ORDER BY c.id",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct ContaIn {
    id: Option<i64>,
    descricao: String,
    tipo: String,
    banco: Option<String>,
    agencia: Option<String>,
    numero: Option<String>,
    iban: Option<String>,
    saldo_inicial: Option<i64>,
    ativo: Option<bool>,
}

pub async fn conta_guardar(st: &AppState, c: ContaIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    if c.descricao.trim().is_empty() || !["CAIXA", "BANCO"].contains(&c.tipo.as_str()) {
        return Err(AppError::msg("Indique a descrição e o tipo da conta."));
    }
    let sql = if c.id.is_some() {
        "UPDATE contas SET descricao=?1, tipo=?2, banco=?3, agencia=?4, numero=?5, iban=?6, saldo_inicial=?7, ativo=?8 WHERE id=?9"
    } else {
        "INSERT INTO contas (descricao, tipo, banco, agencia, numero, iban, saldo_inicial, ativo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    };
    let mut q = sqlx::query(sql)
        .bind(c.descricao.trim())
        .bind(&c.tipo)
        .bind(c.banco)
        .bind(c.agencia)
        .bind(c.numero)
        .bind(c.iban)
        .bind(c.saldo_inicial.unwrap_or(0))
        .bind(c.ativo.unwrap_or(true) as i64);
    if let Some(id) = c.id {
        q = q.bind(id);
    }
    let res = q.execute(&st.pool).await?;
    Ok(json!({ "id": c.id.unwrap_or_else(|| res.last_insert_rowid()) }))
}

pub async fn centros(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    db::query_json(&st.pool, "SELECT * FROM centros_custo ORDER BY tipo, descricao").await
}

#[derive(Deserialize)]
pub struct CentroIn {
    id: Option<i64>,
    descricao: String,
    tipo: String,
    ativo: Option<bool>,
}

pub async fn centro_guardar(st: &AppState, c: CentroIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    if c.descricao.trim().is_empty() || !["DESPESA", "RECEITA"].contains(&c.tipo.as_str()) {
        return Err(AppError::msg("Indique a descrição e o tipo."));
    }
    let id = match c.id {
        Some(id) => {
            sqlx::query("UPDATE centros_custo SET descricao = ?, tipo = ?, ativo = ? WHERE id = ?")
                .bind(c.descricao.trim())
                .bind(&c.tipo)
                .bind(c.ativo.unwrap_or(true) as i64)
                .bind(id)
                .execute(&st.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO centros_custo (descricao, tipo) VALUES (?, ?)")
            .bind(c.descricao.trim())
            .bind(&c.tipo)
            .execute(&st.pool)
            .await?
            .last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct FluxoIn {
    de: String,
    ate: String,
    conta_id: Option<i64>,
}

/// Ecrã "Fluxo de Caixa": movimentos do período, resumo do caixa e resumo de contas.
pub async fn fluxo(st: &AppState, f: FluxoIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let p = &st.pool;
    let movimentos = sqlx::query(
        "SELECT f.*, c.descricao AS conta FROM vw_fluxo f JOIN contas c ON c.id = f.conta_id
         WHERE f.data BETWEEN ?1 AND ?2 AND (?3 IS NULL OR f.conta_id = ?3)
         ORDER BY f.data_hora, f.chave",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .bind(f.conta_id)
    .fetch_all(p)
    .await?;
    let (inicial, anterior): (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COALESCE(SUM(saldo_inicial), 0) FROM contas WHERE ?2 IS NULL OR id = ?2),
            (SELECT COALESCE(SUM(CASE WHEN tipo = 'ENTRADA' THEN valor ELSE -valor END), 0) FROM vw_fluxo
              WHERE anulado = 0 AND data < ?1 AND (?2 IS NULL OR conta_id = ?2))",
    )
    .bind(&f.de)
    .bind(f.conta_id)
    .fetch_one(p)
    .await?;
    let (entradas, saidas): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN tipo = 'ENTRADA' THEN valor END), 0), COALESCE(SUM(CASE WHEN tipo = 'SAIDA' THEN valor END), 0)
         FROM vw_fluxo WHERE anulado = 0 AND data BETWEEN ?1 AND ?2 AND (?3 IS NULL OR conta_id = ?3)",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .bind(f.conta_id)
    .fetch_one(p)
    .await?;
    let (a_receber, a_pagar): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(CASE WHEN t.grupo = 'VENDAS' THEN d.valor_pendente END), 0),
                COALESCE(SUM(CASE WHEN t.grupo = 'COMPRAS' THEN d.valor_pendente END), 0)
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.estado = 'N' AND d.valor_pendente > 0",
    )
    .fetch_one(p)
    .await?;
    let saldo_inicial = inicial + anterior;
    let saldo_final = saldo_inicial + entradas - saidas;
    Ok(json!({
        "movimentos": db::rows_json(&movimentos),
        "saldo_inicial": saldo_inicial,
        "entradas": entradas,
        "saidas": saidas,
        "saldo_final": saldo_final,
        "a_receber": a_receber,
        "a_pagar": a_pagar,
        "previsao": saldo_final + a_receber - a_pagar,
    }))
}

#[derive(Deserialize)]
pub struct AnoIn {
    ano: String,
}

/// "Resumo Mensal": entradas e saídas por mês.
pub async fn resumo_mensal(st: &AppState, a: AnoIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let rows = sqlx::query(
        "SELECT substr(data, 6, 2) AS mes,
                COALESCE(SUM(CASE WHEN tipo = 'ENTRADA' THEN valor END), 0) AS entradas,
                COALESCE(SUM(CASE WHEN tipo = 'SAIDA' THEN valor END), 0) AS saidas
         FROM vw_fluxo WHERE anulado = 0 AND substr(data, 1, 4) = ? GROUP BY mes ORDER BY mes",
    )
    .bind(&a.ano)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct PeriodoIn {
    de: String,
    ate: String,
}

/// "Pagamentos por Centro de Custo" e "Pagamentos por Contas".
pub async fn por_centro(st: &AppState, f: PeriodoIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let centros = sqlx::query(
        "SELECT COALESCE(rubrica, '(sem centro de custo)') AS rubrica, tipo, SUM(valor) AS total, COUNT(*) AS movimentos
         FROM vw_fluxo WHERE anulado = 0 AND origem <> 'DOCUMENTO' AND data BETWEEN ?1 AND ?2
         GROUP BY rubrica, tipo ORDER BY total DESC",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .fetch_all(&st.pool)
    .await?;
    let contas = sqlx::query(
        "SELECT c.descricao AS conta,
                COALESCE(SUM(CASE WHEN f.tipo = 'ENTRADA' THEN f.valor END), 0) AS entradas,
                COALESCE(SUM(CASE WHEN f.tipo = 'SAIDA' THEN f.valor END), 0) AS saidas
         FROM contas c LEFT JOIN vw_fluxo f ON f.conta_id = c.id AND f.anulado = 0 AND f.data BETWEEN ?1 AND ?2
         GROUP BY c.id ORDER BY c.id",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .fetch_all(&st.pool)
    .await?;
    Ok(json!({ "centros": db::rows_json(&centros), "contas": db::rows_json(&contas) }))
}

#[derive(Deserialize)]
pub struct MovimentoIn {
    tipo: String,
    conta_id: i64,
    valor: i64,
    descricao: String,
    centro_custo_id: Option<i64>,
    entidade_id: Option<i64>,
    documento: Option<String>,
    data: Option<String>,
}

/// "Inserir" (entrada) e "Pagamento" (saída com centro de custo).
pub async fn movimento(st: &AppState, m: MovimentoIn) -> AppResult<Value> {
    let s = st.exigir_perm("tesouraria.fluxo")?;
    if m.tipo != "ENTRADA" && m.tipo != "SAIDA" {
        return Err(AppError::msg("Tipo de movimento inválido."));
    }
    if m.valor <= 0 || m.descricao.trim().is_empty() {
        return Err(AppError::msg("Indique o valor e a descrição."));
    }
    let data = m.data.filter(|d| d.len() == 10).unwrap_or_else(db::hoje);
    let id = sqlx::query(
        "INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, centro_custo_id, entidade_id, documento, utilizador_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&data)
    .bind(db::agora())
    .bind(m.conta_id)
    .bind(&m.tipo)
    .bind(m.valor)
    .bind(m.descricao.trim())
    .bind(m.centro_custo_id)
    .bind(m.entidade_id)
    .bind(m.documento.as_deref().map(str::trim).filter(|d| !d.is_empty()))
    .bind(s.id)
    .execute(&st.pool)
    .await?
    .last_insert_rowid();
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct TransferenciaIn {
    de_conta: i64,
    para_conta: i64,
    valor: i64,
    descricao: Option<String>,
}

/// "Depósito" / transferência entre contas: dois movimentos ligados.
pub async fn transferencia(st: &AppState, t: TransferenciaIn) -> AppResult<Value> {
    let s = st.exigir_perm("tesouraria.fluxo")?;
    if t.de_conta == t.para_conta || t.valor <= 0 {
        return Err(AppError::msg("Escolha contas diferentes e um valor positivo."));
    }
    let nomes: Vec<(i64, String)> = sqlx::query_as("SELECT id, descricao FROM contas WHERE id IN (?, ?)")
        .bind(t.de_conta)
        .bind(t.para_conta)
        .fetch_all(&st.pool)
        .await?;
    let nome = |id: i64| nomes.iter().find(|n| n.0 == id).map(|n| n.1.clone()).unwrap_or_default();
    let desc = t.descricao.filter(|d| !d.trim().is_empty()).unwrap_or_else(|| format!("Transferência {} → {}", nome(t.de_conta), nome(t.para_conta)));
    let (hoje, agora) = (db::hoje(), db::agora());
    let mut tx = st.pool.begin().await?;
    let saida = sqlx::query(
        "INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, utilizador_id) VALUES (?, ?, ?, 'SAIDA', ?, ?, ?)",
    )
    .bind(&hoje)
    .bind(&agora)
    .bind(t.de_conta)
    .bind(t.valor)
    .bind(&desc)
    .bind(s.id)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, transferencia, utilizador_id) VALUES (?, ?, ?, 'ENTRADA', ?, ?, ?, ?)",
    )
    .bind(&hoje)
    .bind(&agora)
    .bind(t.para_conta)
    .bind(t.valor)
    .bind(&desc)
    .bind(saida)
    .bind(s.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE movimentos_tesouraria SET transferencia = ? WHERE id = ?").bind(saida).bind(saida).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({ "id": saida }))
}

#[derive(Deserialize)]
pub struct AnularIn {
    id: i64,
    motivo: String,
}

/// "Anular Movimento" (só movimentos de tesouraria; documentos anulam-se nos Documentos).
pub async fn anular(st: &AppState, a: AnularIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    if a.motivo.trim().len() < 3 {
        return Err(AppError::msg("Indique o motivo da anulação."));
    }
    let row = sqlx::query("SELECT transferencia, anulado, documento FROM movimentos_tesouraria WHERE id = ?")
        .bind(a.id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if row.get::<i64, _>("anulado") == 1 {
        return Err(AppError::msg("O movimento já está anulado."));
    }
    let transf: Option<i64> = row.get("transferencia");
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE movimentos_tesouraria SET anulado = 1, motivo_anulacao = ? WHERE id = ? OR (? IS NOT NULL AND transferencia = ?)")
        .bind(a.motivo.trim())
        .bind(a.id)
        .bind(transf)
        .bind(transf)
        .execute(&mut *tx)
        .await?;
    // Pagamento de salários anulado: o processamento volta a ficar por pagar.
    if let Some(rh) = row.get::<Option<String>, _>("documento").and_then(|d| d.strip_prefix("RH-").and_then(|n| n.parse::<i64>().ok())) {
        sqlx::query("UPDATE rh_processamentos SET pago = 0 WHERE id = ?").bind(rh).execute(&mut *tx).await?;
    }
    // Pagamento de honorários anulado.
    if let Some(h) = row.get::<Option<String>, _>("documento").and_then(|d| d.strip_prefix("HON-").and_then(|n| n.parse::<i64>().ok())) {
        sqlx::query("UPDATE rh_honorarios SET pago = 0 WHERE id = ?").bind(h).execute(&mut *tx).await?;
    }
    // Devolução de dinheiro anulada: o crédito volta a ficar disponível.
    for (cid, valor) in sqlx::query_as::<_, (i64, i64)>("SELECT credito_id, valor FROM credito_devolucoes WHERE movimento_id = ?").bind(a.id).fetch_all(&mut *tx).await? {
        sqlx::query("UPDATE documentos SET credito_disponivel = credito_disponivel + ? WHERE id = ? AND estado = 'N'").bind(valor).bind(cid).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM credito_devolucoes WHERE movimento_id = ?").bind(a.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

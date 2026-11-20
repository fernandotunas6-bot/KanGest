// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Avenças: contratos de prestação de serviços que se facturam periodicamente (mensal, trimestral,
//! semestral, anual). Cada período por facturar gera uma factura (FT) com a data do dia em que se
//! factura — o período facturado vai na descrição das linhas e nas observações.
//!
//! Um período só se factura uma vez: antes da factura entra o registo em `avenca_emissoes`
//! (único por avença e período); se a emissão falhar, o registo sai e o período fica por facturar.

use chrono::{Datelike, Months, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const PERMISSAO: &str = "vendas.fatura";

/// `avencas.*` (ver o despachante em services/mod.rs).
pub async fn despachar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    use super::p;
    match metodo {
        "listar" => listar(st).await,
        "guardar" => guardar(st, p(params)?).await,
        "apagar" => apagar(st, p(params)?).await,
        "emitir" => emitir(st, p(params)?).await,
        _ => Err(AppError::msg(format!("Método desconhecido: avencas.{metodo}"))),
    }
}
/// Períodos em atraso facturados de uma vez por avença (o resto fica para a vez seguinte).
const MAX_PERIODOS: usize = 12;

fn meses(periodicidade: &str) -> u32 {
    match periodicidade {
        "TRIMESTRAL" => 3,
        "SEMESTRAL" => 6,
        "ANUAL" => 12,
        _ => 1,
    }
}

fn data(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| AppError::msg(format!("Data inválida: {s}")))
}

const MESES: [&str; 12] = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];

/// «Novembro 2026», «Novembro 2026 a Janeiro 2027», «2026».
pub fn rotulo_periodo(inicio: NaiveDate, periodicidade: &str) -> String {
    let n = meses(periodicidade);
    let m = |d: NaiveDate| format!("{} {}", MESES[d.month0() as usize], d.year());
    if n == 1 {
        return m(inicio);
    }
    if n == 12 && inicio.month() == 1 {
        return inicio.year().to_string();
    }
    let ultimo = inicio.checked_add_months(Months::new(n - 1)).unwrap_or(inicio);
    format!("{} a {}", m(inicio), m(ultimo))
}

fn proximo(d: NaiveDate, periodicidade: &str) -> NaiveDate {
    d.checked_add_months(Months::new(meses(periodicidade))).unwrap_or(d)
}

fn hoje() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Valor estimado de uma avença por período (com IVA), pelos preços actuais.
async fn valor_periodo(p: &sqlx::SqlitePool, avenca_id: i64) -> AppResult<i64> {
    let v: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(l.quantidade * COALESCE(l.preco, a.preco1) * (1 - l.desconto / 100.0) * (1 + t.percentagem / 100.0))
         FROM avenca_linhas l JOIN artigos a ON a.id = l.artigo_id JOIN taxas_iva t ON t.id = a.taxa_iva_id
         WHERE l.avenca_id = ?",
    )
    .bind(avenca_id)
    .fetch_one(p)
    .await?;
    Ok(v.unwrap_or(0.0).round() as i64)
}

pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let rows = sqlx::query(
        "SELECT v.*, e.nome AS cliente, e.nif AS cliente_nif,
                (SELECT COUNT(*) FROM avenca_emissoes x WHERE x.avenca_id = v.id AND x.documento_id IS NOT NULL) AS emitidas,
                (SELECT d.numero_doc FROM avenca_emissoes x JOIN documentos d ON d.id = x.documento_id
                 WHERE x.avenca_id = v.id ORDER BY x.id DESC LIMIT 1) AS ultimo_documento
         FROM avencas v JOIN entidades e ON e.id = v.entidade_id
         ORDER BY v.ativo DESC, v.proxima, e.nome",
    )
    .fetch_all(&st.pool)
    .await?;
    let hoje = hoje().format("%Y-%m-%d").to_string();
    let mut lista = Vec::with_capacity(rows.len());
    for r in &rows {
        let mut v = db::row_json(r);
        let id: i64 = r.get("id");
        let proxima: String = r.get("proxima");
        let fim: Option<String> = r.get("fim");
        let ativo: i64 = r.get("ativo");
        let terminada = fim.as_deref().is_some_and(|f| !f.is_empty() && proxima.as_str() > f);
        v["valor"] = json!(valor_periodo(&st.pool, id).await?);
        v["linhas"] = db::query_json_bind(
            &st.pool,
            "SELECT l.*, a.descricao AS artigo, a.referencia FROM avenca_linhas l JOIN artigos a ON a.id = l.artigo_id WHERE l.avenca_id = ? ORDER BY l.id",
            id,
        )
        .await?;
        v["periodo"] = json!(data(&proxima).map(|d| rotulo_periodo(d, r.get::<String, _>("periodicidade").as_str())).unwrap_or_default());
        v["terminada"] = json!(terminada);
        v["por_facturar"] = json!(ativo == 1 && !terminada && proxima <= hoje);
        lista.push(v);
    }
    Ok(Value::Array(lista))
}

#[derive(Deserialize)]
pub struct LinhaIn {
    artigo_id: i64,
    descricao: Option<String>,
    quantidade: f64,
    preco: Option<i64>,
    #[serde(default)]
    desconto: f64,
}

#[derive(Deserialize)]
pub struct AvencaIn {
    id: Option<i64>,
    entidade_id: i64,
    descricao: String,
    periodicidade: String,
    proxima: String,
    fim: Option<String>,
    prazo: Option<String>,
    observacoes: Option<String>,
    vendedor_id: Option<i64>,
    #[serde(default = "verdadeiro")]
    ativo: bool,
    linhas: Vec<LinhaIn>,
}

fn verdadeiro() -> bool {
    true
}

pub async fn guardar(st: &AppState, a: AvencaIn) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let descricao = a.descricao.trim();
    if descricao.is_empty() {
        return Err(AppError::msg("Indique a descrição da avença (ex.: Contabilidade mensal)."));
    }
    if !["MENSAL", "TRIMESTRAL", "SEMESTRAL", "ANUAL"].contains(&a.periodicidade.as_str()) {
        return Err(AppError::msg("Periodicidade inválida."));
    }
    if a.entidade_id == 1 {
        return Err(AppError::msg("A avença é de um cliente identificado (não do consumidor final)."));
    }
    let proxima = data(&a.proxima)?;
    let fim = match a.fim.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => {
            let f = data(f)?;
            if f < proxima {
                return Err(AppError::msg("A data de fim é anterior à próxima factura."));
            }
            Some(f.format("%Y-%m-%d").to_string())
        }
        None => None,
    };
    if a.linhas.is_empty() {
        return Err(AppError::msg("Indique pelo menos um serviço a facturar."));
    }
    for l in &a.linhas {
        if l.quantidade <= 0.0 {
            return Err(AppError::msg("As quantidades têm de ser positivas."));
        }
        if !(0.0..=100.0).contains(&l.desconto) {
            return Err(AppError::msg("O desconto tem de estar entre 0 e 100%."));
        }
        if l.preco.is_some_and(|p| p < 0) {
            return Err(AppError::msg("Preço inválido."));
        }
    }
    let mut tx = db::transacao(&st.pool).await?;
    let cliente: Option<i64> = sqlx::query_scalar("SELECT id FROM entidades WHERE id = ? AND tipo IN ('C', 'A')").bind(a.entidade_id).fetch_optional(&mut *tx).await?;
    if cliente.is_none() {
        return Err(AppError::msg("Cliente inválido."));
    }
    let campos = (
        a.entidade_id,
        descricao,
        &a.periodicidade,
        proxima.format("%Y-%m-%d").to_string(),
        fim,
        a.prazo.as_deref().map(str::trim).filter(|p| !p.is_empty()),
        a.observacoes.as_deref().map(str::trim).filter(|o| !o.is_empty()),
        a.vendedor_id,
        a.ativo as i64,
    );
    let id = match a.id {
        Some(id) => {
            sqlx::query(
                "UPDATE avencas SET entidade_id = ?, descricao = ?, periodicidade = ?, proxima = ?, fim = ?, prazo = ?, observacoes = ?,
                     vendedor_id = ?, ativo = ?, alterado_em = datetime('now','localtime') WHERE id = ?",
            )
            .bind(campos.0)
            .bind(campos.1)
            .bind(campos.2)
            .bind(&campos.3)
            .bind(&campos.4)
            .bind(campos.5)
            .bind(campos.6)
            .bind(campos.7)
            .bind(campos.8)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM avenca_linhas WHERE avenca_id = ?").bind(id).execute(&mut *tx).await?;
            id
        }
        None => sqlx::query(
            "INSERT INTO avencas (entidade_id, descricao, periodicidade, proxima, fim, prazo, observacoes, vendedor_id, ativo) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(campos.0)
        .bind(campos.1)
        .bind(campos.2)
        .bind(&campos.3)
        .bind(&campos.4)
        .bind(campos.5)
        .bind(campos.6)
        .bind(campos.7)
        .bind(campos.8)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid(),
    };
    for l in &a.linhas {
        let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE id = ? AND ativo = 1").bind(l.artigo_id).fetch_optional(&mut *tx).await?;
        if existe.is_none() {
            return Err(AppError::msg("Um dos artigos da avença não existe ou está inactivo."));
        }
        sqlx::query("INSERT INTO avenca_linhas (avenca_id, artigo_id, descricao, quantidade, preco, desconto) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id)
            .bind(l.artigo_id)
            .bind(l.descricao.as_deref().map(str::trim).filter(|d| !d.is_empty()))
            .bind(l.quantidade)
            .bind(l.preco)
            .bind(l.desconto)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

/// Apaga a avença (se nunca foi facturada) ou desactiva-a (guarda o histórico das facturas).
pub async fn apagar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let emitidas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM avenca_emissoes WHERE avenca_id = ?").bind(i.id).fetch_one(&st.pool).await?;
    if emitidas > 0 {
        sqlx::query("UPDATE avencas SET ativo = 0, alterado_em = datetime('now','localtime') WHERE id = ?").bind(i.id).execute(&st.pool).await?;
        return Ok(json!({ "desactivada": true }));
    }
    sqlx::query("DELETE FROM avencas WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "apagada": true }))
}

#[derive(Deserialize)]
pub struct EmitirIn {
    /// Avenças a facturar (vazio = todas as que estão por facturar).
    #[serde(default)]
    ids: Vec<i64>,
    /// Factura os períodos até esta data (AAAA-MM-DD; vazio = hoje).
    ate: Option<String>,
}

/// Emite as facturas dos períodos por facturar (até `ate`). Devolve o que saiu e o que falhou.
pub async fn emitir(st: &AppState, e: EmitirIn) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let ate = match e.ate.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        Some(x) => data(x)?,
        None => hoje(),
    };
    let rows = sqlx::query("SELECT * FROM avencas WHERE ativo = 1 AND proxima <= ? ORDER BY proxima, id").bind(ate.format("%Y-%m-%d").to_string()).fetch_all(&st.pool).await?;
    let mut emitidos = Vec::new();
    let mut erros = Vec::new();
    for r in rows {
        let id: i64 = r.get("id");
        if !e.ids.is_empty() && !e.ids.contains(&id) {
            continue;
        }
        let descricao: String = r.get("descricao");
        let periodicidade: String = r.get("periodicidade");
        let fim: Option<NaiveDate> = r.get::<Option<String>, _>("fim").and_then(|f| data(&f).ok());
        let mut periodo = data(&r.get::<String, _>("proxima"))?;
        let linhas = sqlx::query("SELECT * FROM avenca_linhas WHERE avenca_id = ? ORDER BY id").bind(id).fetch_all(&st.pool).await?;
        for _ in 0..MAX_PERIODOS {
            if periodo > ate || fim.is_some_and(|f| periodo > f) {
                break;
            }
            let chave = periodo.format("%Y-%m-%d").to_string();
            let rotulo = rotulo_periodo(periodo, &periodicidade);
            // reserva o período: um segundo pedido em paralelo não factura o mesmo período duas vezes
            let reservado = sqlx::query("INSERT OR IGNORE INTO avenca_emissoes (avenca_id, periodo) VALUES (?, ?)").bind(id).bind(&chave).execute(&st.pool).await?;
            if reservado.rows_affected() == 0 {
                periodo = proximo(periodo, &periodicidade);
                continue;
            }
            let substituir = |t: &str| t.replace("{periodo}", &rotulo);
            let doc_linhas: Vec<Value> = linhas
                .iter()
                .map(|l| {
                    let d: Option<String> = l.get("descricao");
                    json!({
                        "artigo_id": l.get::<i64, _>("artigo_id"),
                        "descricao": d.map(|x| substituir(&x)),
                        "quantidade": l.get::<f64, _>("quantidade"),
                        "preco_unitario": l.get::<Option<i64>, _>("preco"),
                        "desconto_pct": l.get::<f64, _>("desconto"),
                    })
                })
                .collect();
            let obs: Option<String> = r.get("observacoes");
            let pedido = json!({
                "tipo": "FT",
                "entidade_id": r.get::<i64, _>("entidade_id"),
                "linhas": doc_linhas,
                "observacoes": obs.map(|o| substituir(&o)).unwrap_or_else(|| format!("Avença: {descricao} — {rotulo}")),
                "prazo": r.get::<Option<String>, _>("prazo"),
                "vendedor_id": r.get::<Option<i64>, _>("vendedor_id"),
                "referencia": format!("Avença {rotulo}"),
            });
            let resultado = match serde_json::from_value::<super::documentos::EmitirIn>(pedido) {
                // em caixa: o futuro da emissão é grande e o despachante não deve crescer por causa dele
                Ok(p) => Box::pin(super::documentos::emitir(st, p)).await,
                Err(err) => Err(AppError::msg(format!("Pedido inválido: {err}"))),
            };
            match resultado {
                Ok(doc) => {
                    let doc_id = doc["documento"]["id"].as_i64();
                    sqlx::query("UPDATE avenca_emissoes SET documento_id = ? WHERE avenca_id = ? AND periodo = ?").bind(doc_id).bind(id).bind(&chave).execute(&st.pool).await?;
                    let seguinte = proximo(periodo, &periodicidade);
                    sqlx::query("UPDATE avencas SET proxima = ? WHERE id = ?").bind(seguinte.format("%Y-%m-%d").to_string()).bind(id).execute(&st.pool).await?;
                    emitidos.push(json!({
                        "avenca_id": id, "avenca": descricao, "periodo": rotulo,
                        "documento_id": doc_id, "numero": doc["documento"]["numero_doc"], "total": doc["documento"]["total"],
                    }));
                    periodo = seguinte;
                }
                Err(err) => {
                    sqlx::query("DELETE FROM avenca_emissoes WHERE avenca_id = ? AND periodo = ? AND documento_id IS NULL").bind(id).bind(&chave).execute(&st.pool).await?;
                    erros.push(json!({ "avenca_id": id, "avenca": descricao, "periodo": rotulo, "erro": err.to_string() }));
                    break;
                }
            }
        }
    }
    Ok(json!({ "emitidos": emitidos, "erros": erros }))
}

/// Quantas avenças estão por facturar hoje (para o aviso no painel).
pub async fn por_facturar(p: &sqlx::SqlitePool) -> AppResult<i64> {
    let hoje = hoje().format("%Y-%m-%d").to_string();
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM avencas WHERE ativo = 1 AND proxima <= ?1 AND (fim IS NULL OR fim = '' OR proxima <= fim)")
        .bind(hoje)
        .fetch_one(p)
        .await?)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn periodos() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(rotulo_periodo(d("2026-11-01"), "MENSAL"), "Novembro 2026");
        assert_eq!(rotulo_periodo(d("2026-11-05"), "TRIMESTRAL"), "Novembro 2026 a Janeiro 2027");
        assert_eq!(rotulo_periodo(d("2026-01-01"), "ANUAL"), "2026");
        assert_eq!(rotulo_periodo(d("2026-07-01"), "ANUAL"), "Julho 2026 a Junho 2027");
        assert_eq!(proximo(d("2026-01-31"), "MENSAL"), d("2026-02-28"), "fim do mês não salta um mês");
        assert_eq!(proximo(d("2026-11-15"), "SEMESTRAL"), d("2027-05-15"));
    }
}

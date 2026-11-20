// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Gestão escolar, só a parte financeira: anos lectivos, turmas (com o valor da propina), matrículas,
//! mapa de propinas e devedores por turma.
//!
//! O aluno é o cliente que paga (a sua conta corrente). A propina mensal de cada matrícula é uma avença
//! (avencas.rs) desse cliente — uma factura por mês, do mês de entrada ao fim do ano lectivo, nunca
//! repetida — e a dívida é a das facturas (`documentos.valor_pendente`), paga com um recibo.

use chrono::{Datelike, Months, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const PERMISSAO: &str = "escola";
/// Referência do artigo da propina mensal (criado na instalação em modo escola).
pub const ARTIGO_PROPINA: &str = "PROP";

/// `escola.*` (ver o despachante em services/mod.rs).
pub async fn despachar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    use super::p;
    match metodo {
        "anos" => anos(st).await,
        "anoGuardar" => ano_guardar(st, p(params)?).await,
        "turmas" => turmas(st, p(params)?).await,
        "turmaGuardar" => turma_guardar(st, p(params)?).await,
        "turmaApagar" => turma_apagar(st, p(params)?).await,
        "matricular" => matricular(st, p(params)?).await,
        "matriculaAnular" => matricula_anular(st, p(params)?).await,
        "mudarTurma" => mudar_turma(st, p(params)?).await,
        "propinas" => Box::pin(propinas(st, p(params)?)).await,
        "devedores" => Box::pin(devedores(st, p(params)?)).await,
        _ => Err(AppError::msg(format!("Método desconhecido: escola.{metodo}"))),
    }
}

// ------------------------------------------------------------------ datas e textos

/// «AAAA-MM» ou «AAAA-MM-DD» → o dia 1 desse mês.
fn mes(s: &str) -> AppResult<NaiveDate> {
    let s = s.trim();
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").or_else(|_| NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d"));
    d.map(|d| d.with_day(1).unwrap_or(d)).map_err(|_| AppError::msg(format!("Data inválida: {s}")))
}

fn txt(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn hoje() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Meses de `inicio` a `fim` (dias 1), inclusive.
fn meses_entre(inicio: NaiveDate, fim: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut m = inicio;
    while m <= fim && out.len() < 36 {
        out.push(m);
        match m.checked_add_months(Months::new(1)) {
            Some(n) => m = n,
            None => break,
        }
    }
    out
}

fn limpo(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

async fn auditar(c: &mut SqliteConnection, utilizador: i64, detalhe: String) -> AppResult<()> {
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'ESCOLA', ?)").bind(utilizador).bind(detalhe).execute(&mut *c).await?;
    Ok(())
}

/// O ano pedido, ou o corrente (ou o mais recente) quando não se diz qual.
async fn ano_ou_corrente(c: &mut SqliteConnection, id: Option<i64>) -> AppResult<Option<i64>> {
    if id.is_some() {
        return Ok(id);
    }
    Ok(sqlx::query_scalar("SELECT id FROM anos_lectivos ORDER BY corrente DESC, inicio DESC LIMIT 1").fetch_optional(&mut *c).await?)
}

async fn artigo_propina(c: &mut SqliteConnection) -> AppResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ? AND ativo = 1").bind(ARTIGO_PROPINA).fetch_optional(&mut *c).await?)
}

// ------------------------------------------------------------------ anos lectivos

pub async fn anos(st: &AppState) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    db::query_json(
        &st.pool,
        "SELECT a.*, (SELECT COUNT(*) FROM matriculas m WHERE m.ano_lectivo_id = a.id AND m.estado = 'ACTIVA') AS alunos
         FROM anos_lectivos a ORDER BY a.inicio DESC",
    )
    .await
}

#[derive(Deserialize)]
pub struct AnoIn {
    id: Option<i64>,
    designacao: String,
    /// mês da primeira propina (AAAA-MM)
    inicio: String,
    /// mês da última propina
    fim: String,
    #[serde(default)]
    corrente: bool,
}

pub async fn ano_guardar(st: &AppState, a: AnoIn) -> AppResult<Value> {
    let s = st.exigir_perm(PERMISSAO)?;
    let designacao = a.designacao.trim();
    if designacao.is_empty() || designacao.chars().count() > 20 {
        return Err(AppError::msg("Indique o ano lectivo (ex.: 2026/2027)."));
    }
    let (inicio, fim) = (mes(&a.inicio)?, mes(&a.fim)?);
    if fim < inicio {
        return Err(AppError::msg("O último mês de propina é anterior ao primeiro."));
    }
    if meses_entre(inicio, fim).len() > 24 {
        return Err(AppError::msg("Um ano lectivo tem no máximo 24 meses de propinas."));
    }
    let mut tx = db::transacao(&st.pool).await?;
    let repetido: Option<i64> = sqlx::query_scalar("SELECT id FROM anos_lectivos WHERE designacao = ? AND id <> ?").bind(designacao).bind(a.id.unwrap_or(0)).fetch_optional(&mut *tx).await?;
    if repetido.is_some() {
        return Err(AppError::msg(format!("O ano lectivo {designacao} já existe.")));
    }
    let id = match a.id {
        Some(id) => {
            let antes = sqlx::query("SELECT inicio, fim FROM anos_lectivos WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NaoEncontrado)?;
            let matriculas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matriculas WHERE ano_lectivo_id = ?").bind(id).fetch_one(&mut *tx).await?;
            if matriculas > 0 && antes.get::<String, _>("inicio") != txt(inicio) {
                return Err(AppError::msg("O ano já tem matrículas: o primeiro mês de propina já não se muda."));
            }
            sqlx::query("UPDATE anos_lectivos SET designacao = ?, inicio = ?, fim = ? WHERE id = ?").bind(designacao).bind(txt(inicio)).bind(txt(fim)).bind(id).execute(&mut *tx).await?;
            // o fim do ano vale para as propinas das matrículas vivas (as facturas emitidas ficam)
            if antes.get::<String, _>("fim") != txt(fim) {
                sqlx::query("UPDATE avencas SET fim = ?, alterado_em = datetime('now','localtime') WHERE id IN (SELECT avenca_id FROM matriculas WHERE ano_lectivo_id = ? AND estado = 'ACTIVA')")
                    .bind(txt(fim))
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            id
        }
        None => sqlx::query("INSERT INTO anos_lectivos (designacao, inicio, fim) VALUES (?, ?, ?)").bind(designacao).bind(txt(inicio)).bind(txt(fim)).execute(&mut *tx).await?.last_insert_rowid(),
    };
    let outro_corrente: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM anos_lectivos WHERE corrente = 1 AND id <> ?").bind(id).fetch_one(&mut *tx).await?;
    if a.corrente || outro_corrente == 0 {
        sqlx::query("UPDATE anos_lectivos SET corrente = (id = ?)").bind(id).execute(&mut *tx).await?;
    }
    auditar(&mut tx, s.id, format!("Ano lectivo {designacao} ({} a {})", &txt(inicio)[..7], &txt(fim)[..7])).await?;
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

// ------------------------------------------------------------------ turmas

#[derive(Deserialize)]
pub struct AnoFiltro {
    #[serde(default)]
    ano_lectivo_id: Option<i64>,
    #[serde(default)]
    turma_id: Option<i64>,
}

pub async fn turmas(st: &AppState, f: AnoFiltro) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let mut c = st.pool.acquire().await?;
    let Some(ano) = ano_ou_corrente(&mut c, f.ano_lectivo_id).await? else { return Ok(json!([])) };
    let rows = sqlx::query(
        "SELECT t.*, (SELECT COUNT(*) FROM matriculas m WHERE m.turma_id = t.id AND m.estado = 'ACTIVA') AS alunos
         FROM turmas t WHERE t.ano_lectivo_id = ? ORDER BY t.ativo DESC, CAST(t.classe AS INTEGER), t.classe, t.turma",
    )
    .bind(ano)
    .fetch_all(&mut *c)
    .await?;
    Ok(Value::Array(rows.iter().map(db::row_json).collect()))
}

#[derive(Deserialize)]
pub struct TurmaIn {
    id: Option<i64>,
    ano_lectivo_id: i64,
    classe: String,
    #[serde(default)]
    turma: Option<String>,
    /// propina mensal em cêntimos
    #[serde(default)]
    propina: i64,
    #[serde(default, deserialize_with = "super::bool_flexivel")]
    ativo: Option<bool>,
}

pub async fn turma_guardar(st: &AppState, t: TurmaIn) -> AppResult<Value> {
    let s = st.exigir_perm(PERMISSAO)?;
    let classe = t.classe.trim();
    if classe.is_empty() || classe.chars().count() > 40 {
        return Err(AppError::msg("Indique a classe (ex.: 7.ª classe)."));
    }
    let turma = limpo(&t.turma).unwrap_or("A");
    if t.propina < 0 {
        return Err(AppError::msg("A propina não pode ser negativa."));
    }
    let mut tx = db::transacao(&st.pool).await?;
    let ano: Option<String> = sqlx::query_scalar("SELECT designacao FROM anos_lectivos WHERE id = ?").bind(t.ano_lectivo_id).fetch_optional(&mut *tx).await?;
    let ano = ano.ok_or_else(|| AppError::msg("Ano lectivo inválido."))?;
    let repetida: Option<i64> = sqlx::query_scalar("SELECT id FROM turmas WHERE ano_lectivo_id = ? AND classe = ? AND turma = ? AND id <> ?")
        .bind(t.ano_lectivo_id)
        .bind(classe)
        .bind(turma)
        .bind(t.id.unwrap_or(0))
        .fetch_optional(&mut *tx)
        .await?;
    if repetida.is_some() {
        return Err(AppError::msg(format!("A turma {classe} {turma} já existe em {ano}.")));
    }
    let mut actualizadas = 0;
    let id = match t.id {
        Some(id) => {
            let antes = sqlx::query("SELECT ano_lectivo_id, propina FROM turmas WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NaoEncontrado)?;
            let com_alunos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matriculas WHERE turma_id = ?").bind(id).fetch_one(&mut *tx).await?;
            if com_alunos > 0 && antes.get::<i64, _>("ano_lectivo_id") != t.ano_lectivo_id {
                return Err(AppError::msg("A turma já tem matrículas: não muda de ano lectivo."));
            }
            sqlx::query("UPDATE turmas SET ano_lectivo_id = ?, classe = ?, turma = ?, propina = ?, ativo = ? WHERE id = ?")
                .bind(t.ano_lectivo_id)
                .bind(classe)
                .bind(turma)
                .bind(t.propina)
                .bind(t.ativo.unwrap_or(true) as i64)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            // propina nova: vale para os próximos meses das matrículas vivas (as facturas emitidas ficam)
            if antes.get::<i64, _>("propina") != t.propina {
                if let Some(art) = artigo_propina(&mut tx).await? {
                    actualizadas = sqlx::query(
                        "UPDATE avenca_linhas SET preco = ? WHERE artigo_id = ? AND avenca_id IN (SELECT avenca_id FROM matriculas WHERE turma_id = ? AND estado = 'ACTIVA')",
                    )
                    .bind((t.propina > 0).then_some(t.propina))
                    .bind(art)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                }
            }
            id
        }
        None => sqlx::query("INSERT INTO turmas (ano_lectivo_id, classe, turma, propina, ativo) VALUES (?, ?, ?, ?, ?)")
            .bind(t.ano_lectivo_id)
            .bind(classe)
            .bind(turma)
            .bind(t.propina)
            .bind(t.ativo.unwrap_or(true) as i64)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid(),
    };
    auditar(&mut tx, s.id, format!("Turma {classe} {turma} de {ano}")).await?;
    tx.commit().await?;
    Ok(json!({ "id": id, "matriculas_actualizadas": actualizadas }))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

pub async fn turma_apagar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let matriculas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matriculas WHERE turma_id = ?").bind(i.id).fetch_one(&st.pool).await?;
    if matriculas > 0 {
        return Err(AppError::msg("A turma tem matrículas: desactive-a em vez de a apagar."));
    }
    sqlx::query("DELETE FROM turmas WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(Value::Null)
}

// ------------------------------------------------------------------ matrículas

#[derive(Deserialize)]
pub struct AlunoNovoIn {
    nome: String,
    #[serde(default)]
    nif: Option<String>,
    #[serde(default)]
    telefone: Option<String>,
}

#[derive(Deserialize)]
pub struct MatricularIn {
    /// o aluno que já é cliente…
    #[serde(default)]
    entidade_id: Option<i64>,
    /// …ou um novo (fica cliente, para receber as facturas)
    #[serde(default)]
    aluno: Option<AlunoNovoIn>,
    turma_id: i64,
    /// bolsa ou desconto na propina, %
    #[serde(default)]
    desconto: f64,
    /// mês da primeira propina (vazio = este mês, ou o início do ano se for antes)
    #[serde(default)]
    primeiro_mes: Option<String>,
}

struct TurmaLida {
    ano_id: i64,
    ano: String,
    inicio: NaiveDate,
    fim: NaiveDate,
    rotulo: String,
    propina: i64,
}

async fn ler_turma(c: &mut SqliteConnection, id: i64) -> AppResult<TurmaLida> {
    let t = sqlx::query("SELECT t.*, y.designacao, y.inicio, y.fim FROM turmas t JOIN anos_lectivos y ON y.id = t.ano_lectivo_id WHERE t.id = ? AND t.ativo = 1")
        .bind(id)
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(|| AppError::msg("A turma não existe ou está desactivada."))?;
    Ok(TurmaLida {
        ano_id: t.get("ano_lectivo_id"),
        ano: t.get("designacao"),
        inicio: mes(&t.get::<String, _>("inicio"))?,
        fim: mes(&t.get::<String, _>("fim"))?,
        rotulo: format!("{} {}", t.get::<String, _>("classe"), t.get::<String, _>("turma")),
        propina: t.get("propina"),
    })
}

/// Textos da avença da propina ({periodo} = o mês facturado): descrição, observações e linha.
fn textos_propina(nome: &str, turma: &str, ano: &str) -> (String, String, String) {
    (format!("Propina — {nome} ({turma})"), format!("Propina de {{periodo}} — {nome}, {turma}, ano lectivo {ano}"), format!("Propina {{periodo}} — {turma}"))
}

pub async fn matricular(st: &AppState, m: MatricularIn) -> AppResult<Value> {
    let s = st.exigir_perm(PERMISSAO)?;
    if !(0.0..=100.0).contains(&m.desconto) {
        return Err(AppError::msg("A bolsa ou desconto tem de estar entre 0 e 100%."));
    }
    let mut tx = db::transacao(&st.pool).await?;
    let t = ler_turma(&mut tx, m.turma_id).await?;
    // o aluno é o cliente das facturas da propina
    let (entidade, nome): (i64, String) = match (&m.aluno, m.entidade_id) {
        (Some(a), _) if !a.nome.trim().is_empty() => {
            let id = sqlx::query("INSERT INTO entidades (tipo, nome, nif, telefone) VALUES ('C', ?, ?, ?)")
                .bind(a.nome.trim())
                .bind(limpo(&a.nif).unwrap_or("999999999"))
                .bind(limpo(&a.telefone))
                .execute(&mut *tx)
                .await?
                .last_insert_rowid();
            (id, a.nome.trim().to_string())
        }
        (_, Some(id)) => {
            let nome: Option<String> = sqlx::query_scalar("SELECT nome FROM entidades WHERE id = ? AND id <> 1 AND tipo IN ('C', 'A')").bind(id).fetch_optional(&mut *tx).await?;
            (id, nome.ok_or_else(|| AppError::msg("O aluno tem de ser um cliente identificado (não o consumidor final)."))?)
        }
        _ => return Err(AppError::msg("Indique o aluno.")),
    };
    let ja: Option<String> = sqlx::query_scalar(
        "SELECT t.classe || ' ' || t.turma FROM matriculas m JOIN turmas t ON t.id = m.turma_id WHERE m.entidade_id = ? AND m.ano_lectivo_id = ? AND m.estado = 'ACTIVA'",
    )
    .bind(entidade)
    .bind(t.ano_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(onde) = ja {
        return Err(AppError::msg(format!("{nome} já está matriculado em {onde} no ano lectivo {}.", t.ano)));
    }
    let primeiro = match limpo(&m.primeiro_mes) {
        Some(x) => mes(x)?,
        None => mes(&txt(hoje()))?.max(t.inicio),
    };
    if primeiro < t.inicio || primeiro > t.fim {
        return Err(AppError::msg(format!("O primeiro mês de propina tem de estar no ano lectivo {} ({} a {}).", t.ano, &txt(t.inicio)[..7], &txt(t.fim)[..7])));
    }
    // bolsa integral: sem propina a facturar
    let avenca_id = if m.desconto < 100.0 {
        let artigo = artigo_propina(&mut tx).await?.ok_or_else(|| AppError::msg(format!("Crie primeiro o artigo da propina mensal (referência {ARTIGO_PROPINA}) em Artigos.")))?;
        let preco_artigo: i64 = sqlx::query_scalar("SELECT preco1 FROM artigos WHERE id = ?").bind(artigo).fetch_one(&mut *tx).await?;
        if t.propina == 0 && preco_artigo == 0 {
            return Err(AppError::msg(format!("Defina a propina da turma {} antes de matricular.", t.rotulo)));
        }
        let (descricao, observacoes, linha) = textos_propina(&nome, &t.rotulo, &t.ano);
        let id = sqlx::query("INSERT INTO avencas (entidade_id, descricao, periodicidade, proxima, fim, observacoes, ativo) VALUES (?, ?, 'MENSAL', ?, ?, ?, 1)")
            .bind(entidade)
            .bind(&descricao)
            .bind(txt(primeiro))
            .bind(txt(t.fim))
            .bind(&observacoes)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        sqlx::query("INSERT INTO avenca_linhas (avenca_id, artigo_id, descricao, quantidade, preco, desconto) VALUES (?, ?, ?, 1, ?, ?)")
            .bind(id)
            .bind(artigo)
            .bind(&linha)
            .bind((t.propina > 0).then_some(t.propina))
            .bind(m.desconto)
            .execute(&mut *tx)
            .await?;
        Some(id)
    } else {
        None
    };
    let id = sqlx::query("INSERT INTO matriculas (entidade_id, turma_id, ano_lectivo_id, data, desconto, avenca_id) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(entidade)
        .bind(m.turma_id)
        .bind(t.ano_id)
        .bind(txt(hoje()))
        .bind(m.desconto)
        .bind(avenca_id)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    auditar(&mut tx, s.id, format!("Matrícula de {nome} em {} ({})", t.rotulo, t.ano)).await?;
    tx.commit().await?;
    Ok(json!({ "id": id, "entidade_id": entidade, "avenca_id": avenca_id }))
}

/// Anula a matrícula: as propinas deixam de se facturar. As facturas já emitidas ficam (anule-as com
/// uma nota de crédito, se for o caso).
pub async fn matricula_anular(st: &AppState, i: IdIn) -> AppResult<Value> {
    let s = st.exigir_perm(PERMISSAO)?;
    let mut tx = db::transacao(&st.pool).await?;
    let m = sqlx::query("SELECT m.avenca_id, m.estado, e.nome FROM matriculas m JOIN entidades e ON e.id = m.entidade_id WHERE m.id = ?")
        .bind(i.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if m.get::<String, _>("estado") != "ACTIVA" {
        return Err(AppError::msg("Esta matrícula já está anulada."));
    }
    sqlx::query("UPDATE matriculas SET estado = 'ANULADA' WHERE id = ?").bind(i.id).execute(&mut *tx).await?;
    if let Some(v) = m.get::<Option<i64>, _>("avenca_id") {
        sqlx::query("UPDATE avencas SET ativo = 0, alterado_em = datetime('now','localtime') WHERE id = ?").bind(v).execute(&mut *tx).await?;
    }
    auditar(&mut tx, s.id, format!("Matrícula de {} anulada", m.get::<String, _>("nome"))).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct MudarTurmaIn {
    id: i64,
    turma_id: i64,
}

/// Muda o aluno de turma no mesmo ano lectivo: as próximas propinas passam a ser as da turma nova.
pub async fn mudar_turma(st: &AppState, mt: MudarTurmaIn) -> AppResult<Value> {
    let s = st.exigir_perm(PERMISSAO)?;
    let mut tx = db::transacao(&st.pool).await?;
    let m = sqlx::query("SELECT m.ano_lectivo_id, m.turma_id, m.avenca_id, m.estado, e.nome FROM matriculas m JOIN entidades e ON e.id = m.entidade_id WHERE m.id = ?")
        .bind(mt.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if m.get::<String, _>("estado") != "ACTIVA" {
        return Err(AppError::msg("Esta matrícula está anulada."));
    }
    if m.get::<i64, _>("turma_id") == mt.turma_id {
        return Ok(Value::Null);
    }
    let t = ler_turma(&mut tx, mt.turma_id).await?;
    if t.ano_id != m.get::<i64, _>("ano_lectivo_id") {
        return Err(AppError::msg("A turma nova tem de ser do mesmo ano lectivo (para outro ano, faça uma matrícula nova)."));
    }
    let nome: String = m.get("nome");
    sqlx::query("UPDATE matriculas SET turma_id = ? WHERE id = ?").bind(mt.turma_id).bind(mt.id).execute(&mut *tx).await?;
    if let Some(v) = m.get::<Option<i64>, _>("avenca_id") {
        let (descricao, observacoes, linha) = textos_propina(&nome, &t.rotulo, &t.ano);
        sqlx::query("UPDATE avencas SET descricao = ?, observacoes = ?, alterado_em = datetime('now','localtime') WHERE id = ?").bind(descricao).bind(observacoes).bind(v).execute(&mut *tx).await?;
        if let Some(art) = artigo_propina(&mut tx).await? {
            sqlx::query("UPDATE avenca_linhas SET descricao = ?, preco = ? WHERE avenca_id = ? AND artigo_id = ?")
                .bind(linha)
                .bind((t.propina > 0).then_some(t.propina))
                .bind(v)
                .bind(art)
                .execute(&mut *tx)
                .await?;
        }
    }
    auditar(&mut tx, s.id, format!("{nome} mudou para a turma {}", t.rotulo)).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

// ------------------------------------------------------------------ propinas e devedores

/// Mapa das propinas: os meses do ano e, por matrícula, o estado de cada mês
/// (PAGA, PARCIAL, DIVIDA, ANULADA, POR_FACTURAR, FUTURO, FORA ou BOLSA).
async fn mapa(st: &AppState, ano: i64, turma: Option<i64>) -> AppResult<(Vec<String>, Vec<Value>)> {
    let y = sqlx::query("SELECT inicio, fim FROM anos_lectivos WHERE id = ?").bind(ano).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let chaves: Vec<String> = meses_entre(mes(&y.get::<String, _>("inicio"))?, mes(&y.get::<String, _>("fim"))?).iter().map(|m| txt(*m)[..7].to_string()).collect();
    let este_mes = txt(hoje())[..7].to_string();
    let rows = sqlx::query(
        "SELECT m.id AS matricula_id, m.entidade_id, m.avenca_id, m.desconto, m.estado, e.nome, e.nif, e.telefone,
                t.id AS turma_id, t.classe || ' ' || t.turma AS turma, v.proxima, v.ativo AS propina_activa,
                (SELECT ROUND(SUM(l.quantidade * COALESCE(l.preco, ar.preco1) * (1 - l.desconto / 100.0) * (1 + tx.percentagem / 100.0)))
                 FROM avenca_linhas l JOIN artigos ar ON ar.id = l.artigo_id JOIN taxas_iva tx ON tx.id = ar.taxa_iva_id WHERE l.avenca_id = m.avenca_id) AS valor_mes
         FROM matriculas m JOIN entidades e ON e.id = m.entidade_id JOIN turmas t ON t.id = m.turma_id LEFT JOIN avencas v ON v.id = m.avenca_id
         WHERE m.ano_lectivo_id = ?1 AND (?2 IS NULL OR m.turma_id = ?2)
           AND (m.estado = 'ACTIVA' OR EXISTS (SELECT 1 FROM avenca_emissoes x WHERE x.avenca_id = m.avenca_id))
         ORDER BY CAST(t.classe AS INTEGER), t.classe, t.turma, e.nome",
    )
    .bind(ano)
    .bind(turma)
    .fetch_all(&st.pool)
    .await?;
    let mut linhas = Vec::with_capacity(rows.len());
    for r in &rows {
        let avenca: Option<i64> = r.get("avenca_id");
        let activa = r.get::<String, _>("estado") == "ACTIVA" && r.get::<Option<i64>, _>("propina_activa") == Some(1);
        let emissoes = match avenca {
            Some(v) => sqlx::query(
                "SELECT substr(x.periodo, 1, 7) AS mes, d.id AS documento_id, d.numero_doc, d.total, d.valor_pendente, d.estado
                 FROM avenca_emissoes x LEFT JOIN documentos d ON d.id = x.documento_id WHERE x.avenca_id = ?",
            )
            .bind(v)
            .fetch_all(&st.pool)
            .await?,
            None => Vec::new(),
        };
        // primeiro mês da propina: o mais antigo emitido, ou o próximo a emitir
        let primeiro = emissoes.iter().map(|e| e.get::<String, _>("mes")).chain(r.get::<Option<String>, _>("proxima").map(|p| p[..7].to_string())).min();
        let valor_mes = r.get::<Option<f64>, _>("valor_mes").unwrap_or(0.0) as i64;
        let (mut em_divida, mut pago, mut por_facturar, mut meses_divida) = (0i64, 0i64, 0i64, 0i64);
        let mut desde: Option<String> = None;
        let mut celulas = Vec::with_capacity(chaves.len());
        for k in &chaves {
            if avenca.is_none() {
                celulas.push(json!({ "mes": k, "estado": "BOLSA" }));
                continue;
            }
            if let Some(e) = emissoes.iter().find(|e| &e.get::<String, _>("mes") == k) {
                let total: i64 = e.get::<Option<i64>, _>("total").unwrap_or(0);
                let pendente: i64 = e.get::<Option<i64>, _>("valor_pendente").unwrap_or(0);
                let estado = match e.get::<Option<String>, _>("estado").as_deref() {
                    Some("N") if pendente == 0 => "PAGA",
                    Some("N") if pendente < total => "PARCIAL",
                    Some("N") => "DIVIDA",
                    Some(_) => "ANULADA",
                    None => "POR_FACTURAR",
                };
                match estado {
                    "PAGA" => pago += total,
                    "PARCIAL" | "DIVIDA" => {
                        em_divida += pendente;
                        pago += total - pendente;
                        meses_divida += 1;
                        desde.get_or_insert_with(|| k.clone());
                    }
                    _ => {}
                }
                celulas.push(json!({ "mes": k, "estado": estado, "total": total, "pendente": pendente,
                                     "documento_id": e.get::<Option<i64>, _>("documento_id"), "numero_doc": e.get::<Option<String>, _>("numero_doc") }));
                continue;
            }
            let estado = if primeiro.as_ref().is_none_or(|p| k < p) || !activa {
                "FORA"
            } else if k <= &este_mes {
                por_facturar += 1;
                "POR_FACTURAR"
            } else {
                "FUTURO"
            };
            celulas.push(json!({ "mes": k, "estado": estado, "total": valor_mes }));
        }
        let mut l = db::row_json(r);
        l["valor_mes"] = json!(valor_mes);
        l["celulas"] = Value::Array(celulas);
        l["em_divida"] = json!(em_divida);
        l["pago"] = json!(pago);
        l["por_facturar"] = json!(por_facturar);
        l["meses_em_divida"] = json!(meses_divida);
        l["divida_desde"] = json!(desde);
        linhas.push(l);
    }
    Ok((chaves, linhas))
}

pub async fn propinas(st: &AppState, f: AnoFiltro) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let mut c = st.pool.acquire().await?;
    let ano = ano_ou_corrente(&mut c, f.ano_lectivo_id).await?;
    drop(c);
    let Some(ano) = ano else { return Ok(json!({ "ano_lectivo_id": null, "meses": [], "linhas": [] })) };
    let (meses, linhas) = mapa(st, ano, f.turma_id).await?;
    let soma = |k: &str| linhas.iter().map(|l| l[k].as_i64().unwrap_or(0)).sum::<i64>();
    // avenças com meses por facturar até hoje (o botão «Facturar propinas» emite só estas)
    let facturar: Vec<i64> = linhas.iter().filter(|l| l["por_facturar"].as_i64().unwrap_or(0) > 0).filter_map(|l| l["avenca_id"].as_i64()).collect();
    Ok(json!({
        "ano_lectivo_id": ano,
        "meses": meses,
        "linhas": linhas,
        "totais": { "em_divida": soma("em_divida"), "pago": soma("pago"), "meses_por_facturar": soma("por_facturar") },
        "avencas_por_facturar": facturar,
    }))
}

/// Devedores por turma: só os alunos com propinas por pagar, do atraso mais antigo para o mais recente.
pub async fn devedores(st: &AppState, f: AnoFiltro) -> AppResult<Value> {
    st.exigir_perm(PERMISSAO)?;
    let mut c = st.pool.acquire().await?;
    let ano = ano_ou_corrente(&mut c, f.ano_lectivo_id).await?;
    drop(c);
    let Some(ano) = ano else { return Ok(json!({ "ano_lectivo_id": null, "turmas": [], "total": 0 })) };
    let (_, linhas) = mapa(st, ano, f.turma_id).await?;
    let mut turmas: Vec<Value> = Vec::new();
    for l in linhas.into_iter().filter(|l| l["em_divida"].as_i64().unwrap_or(0) > 0) {
        let aluno = json!({
            "entidade_id": l["entidade_id"], "nome": l["nome"], "telefone": l["telefone"],
            "meses": l["meses_em_divida"], "desde": l["divida_desde"], "valor": l["em_divida"],
        });
        let valor = l["em_divida"].as_i64().unwrap_or(0);
        match turmas.iter_mut().find(|t| t["turma_id"] == l["turma_id"]) {
            Some(t) => {
                t["total"] = json!(t["total"].as_i64().unwrap_or(0) + valor);
                if let Some(a) = t["alunos"].as_array_mut() {
                    a.push(aluno);
                }
            }
            None => turmas.push(json!({ "turma_id": l["turma_id"], "turma": l["turma"], "total": valor, "alunos": [aluno] })),
        }
    }
    for t in &mut turmas {
        if let Some(a) = t["alunos"].as_array_mut() {
            a.sort_by(|x, y| x["desde"].as_str().cmp(&y["desde"].as_str()).then(y["valor"].as_i64().cmp(&x["valor"].as_i64())));
        }
    }
    let total: i64 = turmas.iter().map(|t| t["total"].as_i64().unwrap_or(0)).sum();
    Ok(json!({ "ano_lectivo_id": ano, "turmas": turmas, "total": total }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn meses_do_ano() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(mes("2026-09").unwrap(), d("2026-09-01"));
        assert_eq!(mes("2026-09-17").unwrap(), d("2026-09-01"));
        assert!(mes("2026-13").is_err());
        let m = meses_entre(d("2026-09-01"), d("2027-06-01"));
        assert_eq!(m.len(), 10);
        assert_eq!((m[0], m[9]), (d("2026-09-01"), d("2027-06-01")));
    }
}

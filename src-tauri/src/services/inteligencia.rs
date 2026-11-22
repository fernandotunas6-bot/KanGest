// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! KanGest inteligente: o resumo de ontem, as previsões e os alertas do painel.
//!
//! Tudo se calcula neste computador (ou no servidor da rede), com regras e estatística simples sobre os dados da
//! empresa: médias por dia da semana, ritmo de venda, comparação com o habitual. Nada sai para fora.
//!
//! - **Resumo de ontem**: vendas, documentos, ticket médio e margem, face ao habitual nesse dia da semana (a média
//!   das 4 semanas anteriores), o artigo mais vendido e o melhor cliente.
//! - **Previsões**: o fecho do mês (o que falta, ao ritmo de cada dia da semana nas últimas 8 semanas), os artigos
//!   que esgotam nos próximos 7 dias ao ritmo de venda e o que entra e sai nos próximos 30 dias (vencimentos).
//! - **Alertas**: vendas abaixo do custo, preços de venda abaixo do custo, diferenças nos fechos de caixa, anulações
//!   acima do habitual, quebra de vendas, margem a cair, devedores em atraso, bons clientes que deixaram de comprar
//!   e stock parado.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};
use sqlx::Row;

use super::relatorios::{SINAL, VENDAS};
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

/// Diferença de caixa que merece aviso: 1 000,00 Kz (cêntimos).
const LIMIAR_CAIXA: i64 = 100_000;
/// Dias de vendas guardados em memória para as contas (resumo, previsão do mês, comparações e margens).
const JANELA: i64 = 120;

pub(crate) fn data(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}
pub(crate) fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}
pub(crate) const DIAS_SEMANA: [&str; 7] = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"];
pub(crate) fn dia_semana(d: NaiveDate) -> &'static str {
    DIAS_SEMANA[d.weekday().num_days_from_sunday() as usize]
}
fn ultimo_dia_do_mes(d: NaiveDate) -> NaiveDate {
    let (a, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(a, m, 1).map(|x| x - Duration::days(1)).unwrap_or(d)
}

/// Vendas líquidas por dia (com IVA, notas de crédito a subtrair) e, nas linhas com custo, a base e o custo.
#[derive(Default, Clone, Copy)]
pub(crate) struct Dia {
    pub total: i64,
    pub docs: i64,
    pub liquido_com_custo: i64,
    pub custo: i64,
}

/// As vendas dos últimos dias, por dia, e o dia da primeira venda (para não contar como «sem vendas» os dias antes
/// de a empresa começar a usar o KanGest).
pub(crate) struct Historico {
    pub dias: HashMap<NaiveDate, Dia>,
    pub primeira: Option<NaiveDate>,
}

impl Historico {
    pub fn em(&self, d: NaiveDate) -> Dia {
        self.dias.get(&d).copied().unwrap_or_default()
    }
    pub fn soma(&self, de: NaiveDate, ate: NaiveDate) -> Dia {
        let mut s = Dia::default();
        let mut d = de;
        while d <= ate {
            let x = self.em(d);
            s.total += x.total;
            s.docs += x.docs;
            s.liquido_com_custo += x.liquido_com_custo;
            s.custo += x.custo;
            d += Duration::days(1);
        }
        s
    }
    /// A média do mesmo dia da semana que `d`, nas `semanas` anteriores a `antes` (exclusive), contando só os dias
    /// depois da primeira venda. `None` sem nenhum desses dias.
    pub fn media_dia_semana(&self, d: NaiveDate, antes: NaiveDate, semanas: i64) -> Option<f64> {
        let primeira = self.primeira?;
        let mut x = antes - Duration::days(1);
        while x.weekday() != d.weekday() {
            x -= Duration::days(1);
        }
        let (mut soma, mut n) = (0i64, 0i64);
        for _ in 0..semanas {
            if x < primeira {
                break;
            }
            soma += self.em(x).total;
            n += 1;
            x -= Duration::days(7);
        }
        (n > 0).then(|| soma as f64 / n as f64)
    }
}

/// Margem bruta em percentagem (só as linhas com custo conhecido), ou `None` sem vendas com custo.
pub(crate) fn margem(d: &Dia) -> Option<f64> {
    (d.liquido_com_custo > 0).then(|| ((d.liquido_com_custo - d.custo) as f64 / d.liquido_com_custo as f64 * 1000.0).round() / 10.0)
}

/// Variação em percentagem (arredondada às unidades) de `actual` face a `habitual`.
pub(crate) fn variacao(actual: i64, habitual: f64) -> Option<i64> {
    (habitual > 0.0).then(|| ((actual as f64 - habitual) / habitual * 100.0).round() as i64)
}

/// O filtro da loja (utilizador de uma só loja): o id vem da sessão, é um inteiro.
pub(crate) fn filtro_loja(st: &AppState) -> String {
    st.loja().map(|a| format!(" AND d.armazem_id = {a}")).unwrap_or_default()
}

pub(crate) async fn historico(st: &AppState, hoje: NaiveDate) -> AppResult<Historico> {
    let p = &st.pool;
    let loja = filtro_loja(st);
    let desde = iso(hoje - Duration::days(JANELA));
    let totais = sqlx::query(&format!(
        "SELECT d.data_emissao AS dia, SUM({SINAL} * d.total) AS total, COUNT(*) AS docs
         FROM documentos d WHERE {VENDAS} AND d.data_emissao >= ?1 AND d.data_emissao <= ?2{loja} GROUP BY d.data_emissao"
    ))
    .bind(&desde)
    .bind(iso(hoje))
    .fetch_all(p)
    .await?;
    let margens = sqlx::query(&format!(
        "SELECT d.data_emissao AS dia, SUM({SINAL} * l.valor_liquido) AS liquido,
                SUM({SINAL} * CAST(ROUND(l.custo * l.quantidade) AS INTEGER)) AS custo
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE {VENDAS} AND l.custo > 0 AND d.data_emissao >= ?1 AND d.data_emissao <= ?2{loja} GROUP BY d.data_emissao"
    ))
    .bind(&desde)
    .bind(iso(hoje))
    .fetch_all(p)
    .await?;
    let primeira: Option<String> = sqlx::query_scalar(&format!("SELECT MIN(d.data_emissao) FROM documentos d WHERE {VENDAS}{loja}"))
        .fetch_one(p)
        .await?;
    let mut dias: HashMap<NaiveDate, Dia> = HashMap::new();
    for r in &totais {
        if let Some(d) = data(&r.get::<String, _>("dia")) {
            let e = dias.entry(d).or_default();
            e.total = r.get::<Option<i64>, _>("total").unwrap_or(0);
            e.docs = r.get("docs");
        }
    }
    for r in &margens {
        if let Some(d) = data(&r.get::<String, _>("dia")) {
            let e = dias.entry(d).or_default();
            e.liquido_com_custo = r.get::<Option<i64>, _>("liquido").unwrap_or(0);
            e.custo = r.get::<Option<i64>, _>("custo").unwrap_or(0);
        }
    }
    Ok(Historico { dias, primeira: primeira.as_deref().and_then(data) })
}

/// O painel inteligente: resumo de ontem, previsões e alertas.
pub async fn painel(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let hoje = data(&db::hoje()).unwrap_or_else(|| chrono::Local::now().date_naive());
    let h = historico(st, hoje).await?;
    Ok(json!({
        "hoje": iso(hoje),
        "resumo": resumo_do_dia(st, &h, hoje - Duration::days(1)).await?,
        "previsoes": {
            "fecho_mes": fecho_do_mes(&h, hoje),
            "ruptura": ruptura(st, hoje, &h).await?,
            "tesouraria": tesouraria(st, hoje).await?,
        },
        "alertas": alertas(st, &h, hoje).await?,
    }))
}

// ------------------------------------------------------------------ resumo do dia

/// O resumo de um dia (normalmente ontem), face ao habitual nesse dia da semana.
pub(crate) async fn resumo_do_dia(st: &AppState, h: &Historico, dia: NaiveDate) -> AppResult<Value> {
    let p = &st.pool;
    let loja = filtro_loja(st);
    let x = h.em(dia);
    let habitual = h.media_dia_semana(dia, dia, 4);
    let top = sqlx::query(&format!(
        "SELECT l.descricao, SUM({SINAL} * l.quantidade) AS quantidade, SUM({SINAL} * l.total) AS total
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE {VENDAS} AND d.data_emissao = ?{loja} GROUP BY l.referencia, l.descricao ORDER BY total DESC LIMIT 1"
    ))
    .bind(iso(dia))
    .fetch_optional(p)
    .await?;
    let cliente = sqlx::query(&format!(
        "SELECT d.entidade_id AS id, MAX(d.entidade_nome) AS nome, SUM({SINAL} * d.total) AS total, COUNT(*) AS docs
         FROM documentos d WHERE {VENDAS} AND d.data_emissao = ? AND d.entidade_id IS NOT NULL AND d.entidade_id <> 1{loja}
         GROUP BY d.entidade_id ORDER BY total DESC LIMIT 1"
    ))
    .bind(iso(dia))
    .fetch_optional(p)
    .await?;
    Ok(json!({
        "dia": iso(dia),
        "dia_semana": dia_semana(dia),
        "vendas": x.total,
        "docs": x.docs,
        "ticket": if x.docs > 0 { x.total / x.docs } else { 0 },
        "habitual": habitual.map(|m| m.round() as i64),
        "variacao": habitual.and_then(|m| variacao(x.total, m)),
        "margem": margem(&x),
        "lucro": (x.liquido_com_custo > 0).then_some(x.liquido_com_custo - x.custo),
        "top_artigo": top.map(|r| json!({ "descricao": r.get::<String, _>("descricao"), "quantidade": r.get::<f64, _>("quantidade"), "total": r.get::<i64, _>("total") })),
        "melhor_cliente": cliente.filter(|r| r.get::<i64, _>("total") > 0).map(|r| json!({
            "id": r.get::<i64, _>("id"), "nome": r.get::<String, _>("nome"), "total": r.get::<i64, _>("total"), "docs": r.get::<i64, _>("docs"),
        })),
    }))
}

// ------------------------------------------------------------------ previsões

/// O fecho do mês: o vendido até hoje mais, para cada dia que falta, a média desse dia da semana nas últimas
/// 8 semanas (e, de hoje, o que ainda falta para a média). Só com 14 dias de histórico ou mais.
pub(crate) fn fecho_do_mes(h: &Historico, hoje: NaiveDate) -> Value {
    let Some(primeira) = h.primeira else { return Value::Null };
    if (hoje - primeira).num_days() < 14 {
        return Value::Null;
    }
    let inicio = hoje.with_day(1).unwrap_or(hoje);
    let fim = ultimo_dia_do_mes(hoje);
    let ate_hoje = h.soma(inicio, hoje).total;
    let mut previsto = ate_hoje as f64;
    previsto += (h.media_dia_semana(hoje, hoje, 8).unwrap_or(0.0) - h.em(hoje).total as f64).max(0.0);
    let mut d = hoje + Duration::days(1);
    while d <= fim {
        previsto += h.media_dia_semana(d, hoje, 8).unwrap_or(0.0);
        d += Duration::days(1);
    }
    let anterior_fim = inicio - Duration::days(1);
    let anterior_inicio = anterior_fim.with_day(1).unwrap_or(anterior_fim);
    let mes_anterior = (primeira <= anterior_inicio).then(|| h.soma(anterior_inicio, anterior_fim).total);
    let previsto = previsto.round() as i64;
    json!({
        "vendas_ate_hoje": ate_hoje,
        "previsto": previsto,
        "dias_restantes": (fim - hoje).num_days(),
        "mes_anterior": mes_anterior,
        "variacao": mes_anterior.and_then(|m| variacao(previsto, m as f64)),
    })
}

/// Os artigos que esgotam nos próximos 7 dias ao ritmo de venda (últimos 30 dias, ou desde a primeira venda),
/// contando o que já está encomendado ao fornecedor.
async fn ruptura(st: &AppState, hoje: NaiveDate, h: &Historico) -> AppResult<Value> {
    let Some(primeira) = h.primeira else { return Ok(json!({ "n": 0, "artigos": [] })) };
    let base = (hoje - primeira).num_days().clamp(7, 30);
    let rows = sqlx::query(
        "WITH vendido AS (
             SELECT l.artigo_id, SUM(CASE WHEN t.credito = 1 THEN -l.quantidade ELSE l.quantidade END) AS q
             FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             WHERE t.grupo = 'VENDAS' AND t.stock <> 0 AND d.estado <> 'A' AND d.data_emissao >= ?1 AND d.data_emissao < ?3
               AND (?2 IS NULL OR d.armazem_id = ?2)
             GROUP BY l.artigo_id),
         encomendado AS (
             SELECT l.artigo_id, SUM(l.quantidade) AS q FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
             WHERE d.tipo_codigo = 'VNE' AND d.estado = 'N' AND (?2 IS NULL OR d.armazem_id = ?2) GROUP BY l.artigo_id),
         existencias AS (
             SELECT artigo_id, SUM(quantidade) AS q FROM stock WHERE (?2 IS NULL OR armazem_id = ?2) GROUP BY artigo_id)
         SELECT a.id, a.referencia, a.descricao, COALESCE(e.q, 0.0) AS stock, v.q AS vendido, COALESCE(o.q, 0.0) AS encomendado
         FROM artigos a JOIN vendido v ON v.artigo_id = a.id
         LEFT JOIN existencias e ON e.artigo_id = a.id
         LEFT JOIN encomendado o ON o.artigo_id = a.id
         WHERE a.ativo = 1 AND a.controla_stock = 1 AND a.composto = 0 AND v.q > 0",
    )
    .bind(iso(hoje - Duration::days(base)))
    .bind(st.loja())
    .bind(iso(hoje))
    .fetch_all(&st.pool)
    .await?;
    let mut artigos: Vec<(f64, Value)> = rows
        .iter()
        .filter_map(|r| {
            let (stock, vendido, encomendado): (f64, f64, f64) = (r.get("stock"), r.get("vendido"), r.get("encomendado"));
            let media = vendido / base as f64;
            let dias = (stock.max(0.0) / media).floor();
            (media > 0.0 && stock + encomendado < media * 7.0 && dias <= 7.0).then(|| {
                (
                    dias,
                    json!({
                        "artigo_id": r.get::<i64, _>("id"), "referencia": r.get::<String, _>("referencia"), "descricao": r.get::<String, _>("descricao"),
                        "stock": stock, "encomendado": encomendado, "media_diaria": (media * 100.0).round() / 100.0, "dias": dias as i64,
                    }),
                )
            })
        })
        .collect();
    artigos.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1["media_diaria"].as_f64().unwrap_or(0.0).total_cmp(&a.1["media_diaria"].as_f64().unwrap_or(0.0))));
    let n = artigos.len();
    Ok(json!({ "n": n, "artigos": artigos.into_iter().take(6).map(|(_, v)| v).collect::<Vec<_>>() }))
}

/// O que entra (clientes) e sai (fornecedores) nos próximos 30 dias pelos vencimentos, e o que já está em atraso.
async fn tesouraria(st: &AppState, hoje: NaiveDate) -> AppResult<Value> {
    let loja = filtro_loja(st);
    let r = sqlx::query(&format!(
        "SELECT COALESCE(SUM(CASE WHEN t.grupo = 'VENDAS' AND v >= ?1 AND v <= date(?1, '+30 day') THEN d.valor_pendente END), 0) AS entra,
                COALESCE(SUM(CASE WHEN t.grupo = 'VENDAS' AND v < ?1 THEN d.valor_pendente END), 0) AS entra_atraso,
                COALESCE(SUM(CASE WHEN t.grupo = 'COMPRAS' AND v >= ?1 AND v <= date(?1, '+30 day') THEN d.valor_pendente END), 0) AS sai,
                COALESCE(SUM(CASE WHEN t.grupo = 'COMPRAS' AND v < ?1 THEN d.valor_pendente END), 0) AS sai_atraso
         FROM (SELECT d.*, COALESCE(d.data_vencimento, d.data_emissao) AS v FROM documentos d) d
         JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.estado = 'N' AND d.valor_pendente > 0{loja}"
    ))
    .bind(iso(hoje))
    .fetch_one(&st.pool)
    .await?;
    let (entra, sai): (i64, i64) = (r.get("entra"), r.get("sai"));
    Ok(json!({
        "entra": entra, "sai": sai, "saldo": entra - sai,
        "entra_atraso": r.get::<i64, _>("entra_atraso"), "sai_atraso": r.get::<i64, _>("sai_atraso"),
    }))
}

// ------------------------------------------------------------------ alertas

fn alerta(tipo: &str, tom: &str, n: i64, valor: i64, ligacao: &str, itens: Vec<Value>, extra: Value) -> Value {
    json!({ "tipo": tipo, "tom": tom, "n": n, "valor": valor, "ligacao": ligacao, "itens": itens, "extra": extra })
}

pub(crate) async fn alertas(st: &AppState, h: &Historico, hoje: NaiveDate) -> AppResult<Vec<Value>> {
    let p = &st.pool;
    let loja = filtro_loja(st);
    let semana = iso(hoje - Duration::days(7));
    let mut v = Vec::new();

    // vendas abaixo do custo nos últimos 7 dias (o preço ou o desconto deixou a linha a perder dinheiro)
    let rows = sqlx::query(&format!(
        "SELECT MAX(l.descricao) AS nome, COUNT(*) AS n,
                SUM(CAST(ROUND(l.custo * l.quantidade) AS INTEGER) - l.valor_liquido) AS perda
         FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
         WHERE d.tipo_codigo IN ('FR','FT') AND d.estado <> 'A' AND d.data_emissao >= ? AND l.custo > 0 AND l.quantidade > 0
           AND l.valor_liquido < CAST(ROUND(l.custo * l.quantidade) AS INTEGER){loja}
         GROUP BY COALESCE(l.artigo_id, l.referencia) ORDER BY perda DESC"
    ))
    .bind(&semana)
    .fetch_all(p)
    .await?;
    if !rows.is_empty() {
        let itens = rows.iter().take(3).map(|r| json!({ "nome": r.get::<String, _>("nome"), "valor": r.get::<i64, _>("perda"), "n": r.get::<i64, _>("n") })).collect();
        v.push(alerta(
            "abaixo_custo",
            "danger",
            rows.iter().map(|r| r.get::<i64, _>("n")).sum(),
            rows.iter().map(|r| r.get::<i64, _>("perda")).sum(),
            "/relatorios?agrupar=artigo",
            itens,
            Value::Null,
        ));
    }

    // diferenças nos fechos de caixa dos últimos 7 dias (o contado face ao esperado, em todas as formas de pagamento)
    let sessoes = sqlx::query(
        "SELECT c.id, COALESCE(ps.descricao, 'Posto') AS posto, u.nome AS utilizador, c.fechada_em
         FROM caixa_sessoes c JOIN utilizadores u ON u.id = c.utilizador_id LEFT JOIN postos ps ON ps.id = c.posto_id
         WHERE c.estado = 'FECHADA' AND c.valor_contado IS NOT NULL AND c.fechada_em >= ?1
           AND (?2 IS NULL OR ps.armazem_id = ?2)
         ORDER BY c.fechada_em DESC LIMIT 60",
    )
    .bind(&semana)
    .bind(st.loja())
    .fetch_all(p)
    .await?;
    let mut caixas = Vec::new();
    for s in &sessoes {
        let r = super::caixa::resumo(st, s.get("id")).await?;
        let dif: i64 = r["conferencia"].as_array().map(|c| c.iter().filter_map(|x| x["diferenca"].as_i64()).sum()).unwrap_or(0);
        if dif.abs() >= LIMIAR_CAIXA {
            caixas.push(json!({
                "nome": format!("{} · {}", s.get::<String, _>("posto"), s.get::<String, _>("utilizador")),
                "valor": dif, "quando": s.get::<Option<String>, _>("fechada_em"),
            }));
        }
    }
    if !caixas.is_empty() {
        let total = caixas.iter().map(|c| c["valor"].as_i64().unwrap_or(0).abs()).sum();
        v.push(alerta("caixa_diferenca", "danger", caixas.len() as i64, total, "/caixa", caixas.into_iter().take(3).collect(), Value::Null));
    }

    // anulações de vendas acima do habitual: as emitidas nos últimos 7 dias face à média semanal das 8 semanas antes
    let (anuladas, antes): (i64, i64) = sqlx::query_as(&format!(
        "SELECT COUNT(CASE WHEN d.data_emissao >= ?1 THEN 1 END), COUNT(CASE WHEN d.data_emissao < ?1 THEN 1 END)
         FROM documentos d WHERE d.tipo_codigo IN ('FR','FT') AND d.estado = 'A' AND d.data_emissao >= date(?1, '-56 day'){loja}"
    ))
    .bind(&semana)
    .fetch_one(p)
    .await?;
    let media_anuladas = antes as f64 / 8.0;
    if anuladas >= 3 && anuladas as f64 > 2.0 * media_anuladas {
        let quem = sqlx::query(&format!(
            "SELECT u.nome, COUNT(*) AS n, SUM(d.total) AS total FROM documentos d JOIN utilizadores u ON u.id = d.utilizador_id
             WHERE d.tipo_codigo IN ('FR','FT') AND d.estado = 'A' AND d.data_emissao >= ?{loja} GROUP BY u.id ORDER BY n DESC LIMIT 3"
        ))
        .bind(&semana)
        .fetch_all(p)
        .await?;
        let itens = quem.iter().map(|r| json!({ "nome": r.get::<String, _>("nome"), "n": r.get::<i64, _>("n"), "valor": r.get::<i64, _>("total") })).collect();
        v.push(alerta("anulacoes", "danger", anuladas, 0, "/documentos", itens, json!({ "habitual": (media_anuladas * 10.0).round() / 10.0 })));
    }

    // quebra de vendas: os últimos 7 dias face à média das 4 semanas antes (com 5 semanas de histórico)
    if h.primeira.is_some_and(|pr| (hoje - pr).num_days() >= 35) {
        let ultimos = h.soma(hoje - Duration::days(7), hoje - Duration::days(1)).total;
        let habitual = h.soma(hoje - Duration::days(35), hoje - Duration::days(8)).total as f64 / 4.0;
        if let Some(pct) = variacao(ultimos, habitual).filter(|p| *p <= -25) {
            v.push(alerta(
                "vendas_quebra",
                if pct <= -50 { "danger" } else { "accent" },
                1,
                ultimos,
                "/relatorios?agrupar=dia",
                Vec::new(),
                json!({ "habitual": habitual.round() as i64, "variacao": pct }),
            ));
        }
    }

    // margem a cair: a dos últimos 30 dias face à dos 90 dias antes (só as linhas com custo)
    let agora = h.soma(hoje - Duration::days(30), hoje - Duration::days(1));
    let antes_m = h.soma(hoje - Duration::days(JANELA), hoje - Duration::days(31));
    if let (Some(m1), Some(m0)) = (margem(&agora), margem(&antes_m)) {
        if agora.docs >= 10 && m0 - m1 >= 5.0 {
            v.push(alerta("margem_queda", "accent", 1, agora.liquido_com_custo - agora.custo, "/relatorios?agrupar=mes", Vec::new(), json!({ "actual": m1, "anterior": m0 })));
        }
    }

    // preços de venda abaixo do custo (depois de uma compra mais cara, por exemplo)
    let precos = sqlx::query(
        "SELECT a.id, a.descricao, a.preco1, CASE WHEN a.custo_medio > 0 THEN a.custo_medio ELSE a.custo END AS custo
         FROM artigos a WHERE a.ativo = 1 AND a.preco1 > 0 AND (CASE WHEN a.custo_medio > 0 THEN a.custo_medio ELSE a.custo END) > a.preco1
         ORDER BY (CASE WHEN a.custo_medio > 0 THEN a.custo_medio ELSE a.custo END) - a.preco1 DESC",
    )
    .fetch_all(p)
    .await?;
    if !precos.is_empty() {
        let itens = precos
            .iter()
            .take(3)
            .map(|r| json!({ "id": r.get::<i64, _>("id"), "nome": r.get::<String, _>("descricao"), "valor": r.get::<i64, _>("preco1"), "custo": r.get::<i64, _>("custo") }))
            .collect();
        v.push(alerta("preco_abaixo_custo", "accent", precos.len() as i64, 0, "/artigos", itens, Value::Null));
    }

    // devedores com dívida vencida há mais de 30 dias
    let devedores = sqlx::query(&format!(
        "SELECT d.entidade_id AS id, MAX(d.entidade_nome) AS nome, SUM(d.valor_pendente) AS valor,
                CAST(MAX(julianday(?1) - julianday(COALESCE(d.data_vencimento, d.data_emissao))) AS INTEGER) AS dias
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = 'VENDAS'
           AND COALESCE(d.data_vencimento, d.data_emissao) < date(?1, '-30 day'){loja}
         GROUP BY d.entidade_id ORDER BY valor DESC"
    ))
    .bind(iso(hoje))
    .fetch_all(p)
    .await?;
    if !devedores.is_empty() {
        let itens = devedores
            .iter()
            .take(3)
            .map(|r| json!({ "id": r.get::<Option<i64>, _>("id"), "nome": r.get::<String, _>("nome"), "valor": r.get::<i64, _>("valor"), "dias": r.get::<i64, _>("dias") }))
            .collect();
        v.push(alerta("devedores", "accent", devedores.len() as i64, devedores.iter().map(|r| r.get::<i64, _>("valor")).sum(), "/avisos", itens, Value::Null));
    }

    // bons clientes que deixaram de comprar: compraram em 3 dias ou mais no último ano e já passou mais do dobro e
    // meio do intervalo habitual entre compras (e pelo menos 30 dias)
    let clientes = sqlx::query(&format!(
        "SELECT d.entidade_id AS id, MAX(d.entidade_nome) AS nome, COUNT(DISTINCT d.data_emissao) AS dias,
                MIN(d.data_emissao) AS primeira, MAX(d.data_emissao) AS ultima, SUM(d.total) AS total
         FROM documentos d WHERE d.tipo_codigo IN ('FR','FT') AND d.estado <> 'A' AND d.entidade_id IS NOT NULL AND d.entidade_id <> 1
           AND d.data_emissao >= date(?, '-365 day'){loja}
         GROUP BY d.entidade_id HAVING COUNT(DISTINCT d.data_emissao) >= 3"
    ))
    .bind(iso(hoje))
    .fetch_all(p)
    .await?;
    let mut parados: Vec<Value> = clientes
        .iter()
        .filter_map(|r| {
            let (primeira, ultima) = (data(&r.get::<String, _>("primeira"))?, data(&r.get::<String, _>("ultima"))?);
            let n: i64 = r.get("dias");
            let intervalo = (ultima - primeira).num_days() as f64 / (n - 1) as f64;
            let sem_comprar = (hoje - ultima).num_days();
            (sem_comprar as f64 > (2.5 * intervalo).max(30.0)).then(|| {
                json!({ "id": r.get::<i64, _>("id"), "nome": r.get::<String, _>("nome"), "valor": r.get::<i64, _>("total"),
                        "dias": sem_comprar, "intervalo": intervalo.round() as i64 })
            })
        })
        .collect();
    if !parados.is_empty() {
        parados.sort_by_key(|c| -c["valor"].as_i64().unwrap_or(0));
        let total = parados.iter().map(|c| c["valor"].as_i64().unwrap_or(0)).sum();
        v.push(alerta("clientes_parados", "neutral", parados.len() as i64, total, "/relatorios?agrupar=cliente", parados.into_iter().take(3).collect(), Value::Null));
    }

    // stock parado: artigos com existências sem saídas nem entradas há 90 dias, pelo valor ao custo
    let parado = sqlx::query(
        "SELECT a.id, a.descricao, SUM(s.quantidade) AS quantidade, CAST(ROUND(SUM(s.quantidade) * a.custo_medio) AS INTEGER) AS valor
         FROM stock s JOIN artigos a ON a.id = s.artigo_id
         WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade > 0 AND (?2 IS NULL OR s.armazem_id = ?2)
         GROUP BY a.id
         HAVING MAX(COALESCE(s.ultima_saida, '')) < date(?1, '-90 day') AND MAX(COALESCE(s.ultima_entrada, '')) < date(?1, '-90 day')
         ORDER BY valor DESC",
    )
    .bind(iso(hoje))
    .bind(st.loja())
    .fetch_all(p)
    .await?;
    if !parado.is_empty() {
        let itens = parado
            .iter()
            .take(3)
            .map(|r| json!({ "id": r.get::<i64, _>("id"), "nome": r.get::<String, _>("descricao"), "valor": r.get::<i64, _>("valor"), "quantidade": r.get::<f64, _>("quantidade") }))
            .collect();
        v.push(alerta("stock_parado", "neutral", parado.len() as i64, parado.iter().map(|r| r.get::<i64, _>("valor")).sum(), "/relatorios/stock?situacao=parado", itens, Value::Null));
    }
    Ok(v)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::db;

    fn h(vendas: &[(&str, i64)], primeira: &str) -> Historico {
        Historico {
            dias: vendas.iter().map(|(d, t)| (data(d).unwrap(), Dia { total: *t, docs: 1, ..Default::default() })).collect(),
            primeira: data(primeira),
        }
    }

    #[test]
    fn media_do_dia_da_semana_conta_so_depois_da_primeira_venda() {
        // 2026-11-20 é sexta-feira; sextas anteriores: 13, 06, 30/10, 23/10
        let x = h(&[("2026-11-13", 1000), ("2026-11-06", 3000), ("2026-10-30", 2000)], "2026-10-30");
        let sexta = data("2026-11-20").unwrap();
        assert_eq!(dia_semana(sexta), "sexta-feira");
        assert_eq!(x.media_dia_semana(sexta, sexta, 4), Some(2000.0), "só 3 sextas depois da primeira venda");
        let y = h(&[("2026-11-13", 1000)], "2026-11-13");
        assert_eq!(y.media_dia_semana(data("2026-11-12").unwrap(), data("2026-11-13").unwrap(), 4), None, "nenhuma quinta depois do início");
        assert_eq!(variacao(1500, 1000.0), Some(50));
        assert_eq!(variacao(1500, 0.0), None);
    }

    #[test]
    fn fecho_do_mes_soma_o_que_falta_ao_ritmo_de_cada_dia() {
        // 8 semanas de 100 por dia, excepto aos domingos (0); hoje, 2026-11-25 (quarta), ainda sem vendas
        let hoje = data("2026-11-25").unwrap();
        let mut vendas = Vec::new();
        let mut d = data("2026-09-01").unwrap();
        while d < hoje {
            if d.weekday() != chrono::Weekday::Sun {
                vendas.push((iso(d), 100));
            }
            d += Duration::days(1);
        }
        let x = Historico {
            dias: vendas.iter().map(|(d, t)| (data(d).unwrap(), Dia { total: *t, docs: 1, ..Default::default() })).collect(),
            primeira: data("2026-09-01"),
        };
        let f = fecho_do_mes(&x, hoje);
        // 1–24 de Novembro: 20 dias com vendas (domingos 1, 8, 15 e 22); hoje e 26–30: 6 dias, 1 domingo (29) → 5 × 100
        assert_eq!(f["vendas_ate_hoje"], 2000);
        assert_eq!(f["previsto"], 2500);
        assert_eq!(f["dias_restantes"], 5);
        assert_eq!(f["mes_anterior"], 2700, "Outubro: 31 dias, 4 domingos");
        assert_eq!(f["variacao"], -7);
        // pouco histórico: sem previsão
        assert!(fecho_do_mes(&h(&[("2026-11-20", 5)], "2026-11-20"), hoje).is_null());
    }

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }

    /// O painel com os dados de uma loja: 30 dias de vendas, um fecho de caixa com falta, uma venda abaixo do custo, um
    /// preço abaixo do custo, um cliente que deixou de comprar (e deve), e stock parado.
    #[tokio::test]
    async fn painel_com_os_dados_de_uma_loja() {
        let pasta = std::env::temp_dir().join(format!("kangest-inteligencia-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
        let mut st = AppState::new(pool);
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.expect("posto 1");
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Exemplo, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let artigo = |d: &str, preco: i64, custo: i64| json!({ "descricao": d, "taxa_iva_id": 1, "preco1": preco, "custo": custo });
        let arroz = rpc(&st, "artigos.guardar", artigo("Arroz 5kg", 100000, 50000)).await["id"].as_i64().unwrap();
        let oleo = rpc(&st, "artigos.guardar", artigo("Óleo 1L", 30000, 40000)).await["id"].as_i64().unwrap();
        let candeeiro = rpc(&st, "artigos.guardar", artigo("Candeeiro", 500000, 200000)).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [
            { "artigo_id": arroz, "quantidade": 35, "preco_unitario": 50000 }, { "artigo_id": candeeiro, "quantidade": 5, "preco_unitario": 200000 },
        ] })).await;
        sqlx::query("UPDATE stock SET ultima_entrada = date('now', 'localtime', '-120 day') WHERE artigo_id = ?").bind(candeeiro).execute(&st.pool).await.unwrap();
        // só nesta base de teste: as datas dos documentos assinados mudam-se para simular o histórico
        sqlx::query("DROP TRIGGER trg_doc_assinado_upd").execute(&st.pool).await.unwrap();
        let mudar_data = |id: i64, dias: i64| {
            let pool = st.pool.clone();
            async move {
                sqlx::query("UPDATE documentos SET data_emissao = date('now', 'localtime', ?1), data_vencimento = date('now', 'localtime', ?1) WHERE id = ?2")
                    .bind(format!("-{dias} day"))
                    .bind(id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        };
        // um saco de arroz por dia nos últimos 30 dias, a pronto
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 1000000 })).await;
        for dia in 1..=30 {
            let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
            mudar_data(fr["documento"]["id"].as_i64().unwrap(), dia).await;
        }
        // hoje, um saco vendido abaixo do custo (400,00 Kz para um custo de 500,00 Kz)
        rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 1, "preco_unitario": 40000 }], "pagamentos": [{ "modo": "NU", "valor": 45600 }] })).await;
        // o fecho de caixa com 2 000,00 Kz a menos
        let esperado = rpc(&st, "caixa.estado", Value::Null).await["numerario_esperado"].as_i64().unwrap();
        rpc(&st, "caixa.fechar", json!({ "valor_contado": esperado - 200000 })).await;
        // um cliente que comprava de 20 em 20 dias e há 60 que não compra; as facturas estão por pagar
        let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Mercearia Boa Vista", "nif": "5417000001" })).await["id"].as_i64().unwrap();
        for dia in [100, 80, 60] {
            let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] })).await;
            mudar_data(ft["documento"]["id"].as_i64().unwrap(), dia).await;
        }

        let v = rpc(&st, "inteligencia.painel", Value::Null).await;
        let alerta = |tipo: &str| v["alertas"].as_array().unwrap().iter().find(|a| a["tipo"] == tipo).cloned().unwrap_or(Value::Null);
        // resumo de ontem: 1 140,00 Kz, como nas 4 semanas anteriores ao mesmo dia
        assert_eq!(v["resumo"]["vendas"], 114000);
        assert_eq!(v["resumo"]["habitual"], 114000);
        assert_eq!(v["resumo"]["variacao"], 0);
        assert_eq!(v["resumo"]["margem"], 50.0);
        assert_eq!(v["resumo"]["top_artigo"]["descricao"], "Arroz 5kg");
        // previsões: o mês tem previsão; o arroz esgota (1 em stock, 1 por dia); as facturas do cliente já venceram
        assert!(v["previsoes"]["fecho_mes"]["previsto"].as_i64().unwrap() >= v["previsoes"]["fecho_mes"]["vendas_ate_hoje"].as_i64().unwrap());
        let ruptura = &v["previsoes"]["ruptura"]["artigos"][0];
        assert_eq!(ruptura["descricao"], "Arroz 5kg");
        assert_eq!(ruptura["stock"], 1.0);
        assert_eq!(v["previsoes"]["tesouraria"]["entra_atraso"], 3 * 114000);
        // alertas
        let abaixo = alerta("abaixo_custo");
        assert_eq!(abaixo["valor"], 10000, "perdeu 100,00 Kz nessa venda");
        assert_eq!(abaixo["itens"][0]["nome"], "Arroz 5kg");
        let caixa = alerta("caixa_diferenca");
        assert_eq!(caixa["n"], 1);
        assert_eq!(caixa["itens"][0]["valor"], -200000);
        assert_eq!(alerta("preco_abaixo_custo")["itens"][0]["nome"], "Óleo 1L");
        let devedores = alerta("devedores");
        assert_eq!(devedores["valor"], 3 * 114000);
        assert_eq!(devedores["itens"][0]["nome"], "Mercearia Boa Vista");
        let parados = alerta("clientes_parados");
        assert_eq!(parados["itens"][0]["nome"], "Mercearia Boa Vista");
        assert_eq!(parados["itens"][0]["dias"], 60);
        assert_eq!(parados["itens"][0]["intervalo"], 20);
        let stock = alerta("stock_parado");
        assert_eq!(stock["itens"][0]["nome"], "Candeeiro");
        assert_eq!(stock["valor"], 5 * 200000);
        assert!(alerta("vendas_quebra").is_null(), "as vendas estão como de costume");
        assert!(alerta("anulacoes").is_null());
        let _ = oleo;
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn margem_so_com_custo() {
        assert_eq!(margem(&Dia { liquido_com_custo: 1000, custo: 750, ..Default::default() }), Some(25.0));
        assert_eq!(margem(&Dia::default()), None);
        assert_eq!(ultimo_dia_do_mes(data("2026-02-10").unwrap()), data("2026-02-28").unwrap());
        assert_eq!(ultimo_dia_do_mes(data("2026-12-31").unwrap()), data("2026-12-31").unwrap());
    }
}

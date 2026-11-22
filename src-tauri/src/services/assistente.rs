// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Assistente de perguntas (Ctrl+K): responde em português a perguntas sobre os dados da empresa, como «quanto vendi
//! ontem?», «quem me deve mais?», «stock do arroz» ou «artigos mais vendidos em outubro».
//!
//! Não usa inteligência artificial de fora: um intérprete de regras encontra o que se pergunta (vendas, lucro, ticket,
//! IVA, compras, mais vendidos, melhores clientes, a receber, a pagar, stock, preço, caixa, previsão, artigos a
//! esgotar, alertas), o período (hoje, ontem, esta semana, o mês passado, «em outubro», «em 2025», «últimos 15 dias»)
//! e de quem ou de quê (o artigo ou o cliente que a pergunta nomeia, sem ligar a acentos). As contas fazem-se na base,
//! neste computador; nada sai para fora.
//!
//! A resposta vem em partes (texto e valores), para o ecrã formatar os kwanzas como no resto do programa e respeitar
//! «Ocultar valores».

use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::inteligencia::{self, data, filtro_loja, iso};
use super::relatorios::{SINAL, VENDAS};
use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct PerguntaIn {
    texto: String,
}

// ------------------------------------------------------------------ interpretar

/// Minúsculas, sem acentos e só letras e algarismos («Quanto é o stock do Óleo?» → «quanto e o stock do oleo»).
pub(crate) fn normalizar(s: &str) -> String {
    let t: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intencao {
    Alertas,
    Previsao,
    Esgotar,
    Caixa,
    TopClientes,
    TopArtigos,
    AReceber,
    APagar,
    Stock,
    Preco,
    Lucro,
    Ticket,
    Iva,
    Compras,
    Vendas,
    /// Só uma pergunta com o nome de um artigo ou cliente («quanto ao arroz?»): o resumo dele.
    Ficha,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Periodo {
    pub de: NaiveDate,
    pub ate: NaiveDate,
    /// «ontem», «esta semana», «em outubro de 2026»…
    pub nome: String,
}

#[derive(Debug)]
pub(crate) struct Pergunta {
    pub intencao: Intencao,
    pub periodo: Option<Periodo>,
    /// As palavras que sobram: o artigo ou o cliente de que se fala.
    pub resto: Vec<String>,
}

const MESES: [&str; 12] = ["janeiro", "fevereiro", "marco", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

/// Palavras que não nomeiam nada (artigos, preposições, verbos da pergunta).
const VAZIAS: &[&str] = &[
    "o", "a", "os", "as", "um", "uma", "uns", "umas", "de", "do", "da", "dos", "das", "d", "em", "no", "na", "nos", "nas", "ao", "aos", "para", "pra",
    "por", "pelo", "pela", "pelos", "pelas", "com", "sem", "que", "qual", "quais", "quanto", "quanta", "quantos", "quantas", "quem", "como", "onde",
    "quando", "e", "ou", "foi", "foram", "e", "sao", "ser", "esta", "estao", "estava", "tenho", "tem", "temos", "tenhamos", "ha", "houve", "havia",
    "meu", "minha", "meus", "minhas", "nosso", "nossa", "eu", "me", "mim", "nos", "este", "esta", "estes", "estas", "esse", "essa", "isso", "isto",
    "aquele", "aquela", "ja", "ainda", "mais", "menos", "muito", "muitos", "total", "totais", "valor", "valores", "kz", "kwanzas", "sobre", "ate",
    "desde", "entre", "dia", "dias", "semana", "semanas", "mes", "meses", "ano", "anos", "hoje", "ontem", "anteontem", "passado", "passada",
    "ultimo", "ultima", "ultimos", "ultimas", "actual", "atual", "corrente", "neste", "nesta", "nestes", "deste", "desta", "agora", "afinal",
    "dizer", "diz", "diga", "mostra", "mostrar", "mostre", "ver", "saber", "quero", "queria", "pode", "podes", "favor", "kangest", "fiz", "fizemos",
    "unidade", "unidades", "un", "artigo", "artigos", "produto", "produtos", "cliente", "clientes", "loja", "empresa", "existe", "existem", "ficou",
    "ficaram", "resta", "restam", "sobrou", "sobra", "temos", "anda", "andam", "vai", "vao", "dinheiro", "numero", "quantidade", "conta", "contas",
    "algum", "alguma", "alguns", "algumas", "tudo", "todos", "todas",
];

/// Raízes que marcam o que se pergunta (também se tiram das palavras que nomeiam o artigo ou o cliente).
const RAIZES: &[&str] = &[
    "alerta", "problema", "errad", "invulga", "atenc", "previs", "prever", "projec", "esgot", "ruptur", "acab", "caixa", "melhor", "maior", "top",
    "client", "compr", "vend", "factur", "fatur", "receit", "deve", "devo", "devem", "devemo", "divid", "receb", "atras", "devedor", "credor",
    "pag", "fornecedor", "stock", "estoq", "existenc", "inventar", "armaz", "preco", "precos", "custa", "pvp", "lucr", "margem", "ganh", "rentab",
    "ticket", "talao", "medio", "media", "iva", "impost", "gast", "despes", "documento", "doc", "fecho", "fechar", "faltam", "falta", "saldo",
];

fn comeca(palavra: &str, raizes: &[&str]) -> bool {
    raizes.iter().any(|r| palavra.starts_with(r))
}

/// O primeiro dia da semana (segunda-feira).
fn segunda(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}
fn fim_do_mes(a: i32, m: u32) -> NaiveDate {
    let (a2, m2) = if m == 12 { (a + 1, 1) } else { (a, m + 1) };
    NaiveDate::from_ymd_opt(a2, m2, 1).map(|x| x - Duration::days(1)).unwrap_or_default()
}

pub(crate) fn periodo(w: &[&str], hoje: NaiveDate) -> Option<Periodo> {
    let tem = |x: &str| w.contains(&x);
    let seguido = |a: &str, b: &str| w.windows(2).any(|p| p[0] == a && p[1] == b);
    let p = |de: NaiveDate, ate: NaiveDate, nome: &str| Some(Periodo { de, ate: ate.min(hoje), nome: nome.to_string() });
    // «últimos 15 dias», «últimas 2 semanas», «últimos 3 meses»
    for j in w.windows(3) {
        if matches!(j[0], "ultimos" | "ultimas") {
            if let Ok(n) = j[1].parse::<i64>() {
                let n = n.clamp(1, 3650);
                let dias = match j[2] {
                    "dias" | "dia" => n,
                    "semanas" | "semana" => n * 7,
                    "meses" | "mes" => n * 30,
                    _ => continue,
                };
                return p(hoje - Duration::days(dias - 1), hoje, &format!("nos últimos {} {}", n, j[2]));
            }
        }
    }
    if tem("anteontem") {
        let d = hoje - Duration::days(2);
        return p(d, d, "anteontem");
    }
    if tem("ontem") {
        let d = hoje - Duration::days(1);
        return p(d, d, "ontem");
    }
    if tem("hoje") {
        return p(hoje, hoje, "hoje");
    }
    if seguido("semana", "passada") || seguido("ultima", "semana") {
        let s = segunda(hoje) - Duration::days(7);
        return p(s, s + Duration::days(6), "na semana passada");
    }
    if tem("semana") {
        return p(segunda(hoje), hoje, "esta semana");
    }
    let ano_dito = w.iter().filter_map(|x| x.parse::<i32>().ok()).find(|a| (2000..=2100).contains(a));
    // «em outubro», «outubro de 2025»
    if let Some(m) = w.iter().find_map(|x| MESES.iter().position(|n| n == x)) {
        let m = m as u32 + 1;
        let a = ano_dito.unwrap_or(if m <= hoje.month() { hoje.year() } else { hoje.year() - 1 });
        let de = NaiveDate::from_ymd_opt(a, m, 1)?;
        return p(de, fim_do_mes(a, m), &format!("em {} de {a}", MESES[m as usize - 1].replace("marco", "março")));
    }
    if seguido("mes", "passado") || seguido("ultimo", "mes") {
        let fim = hoje.with_day(1)? - Duration::days(1);
        return p(fim.with_day(1)?, fim, "no mês passado");
    }
    if tem("mes") {
        return p(hoje.with_day(1)?, hoje, "este mês");
    }
    if seguido("ano", "passado") || seguido("ultimo", "ano") {
        let a = hoje.year() - 1;
        return p(NaiveDate::from_ymd_opt(a, 1, 1)?, NaiveDate::from_ymd_opt(a, 12, 31)?, &format!("em {a}"));
    }
    if let Some(a) = ano_dito {
        return p(NaiveDate::from_ymd_opt(a, 1, 1)?, NaiveDate::from_ymd_opt(a, 12, 31)?, &format!("em {a}"));
    }
    if tem("ano") {
        return p(NaiveDate::from_ymd_opt(hoje.year(), 1, 1)?, hoje, "este ano");
    }
    None
}

/// O que se pergunta. `None` quando a frase não parece uma pergunta sobre os dados (a pesquisa normal trata dela).
pub(crate) fn interpretar(texto: &str, hoje: NaiveDate) -> Option<Pergunta> {
    let t = normalizar(texto);
    let w: Vec<&str> = t.split(' ').filter(|x| !x.is_empty()).collect();
    if w.is_empty() {
        return None;
    }
    let algum = |raizes: &[&str]| w.iter().any(|x| comeca(x, raizes));
    let frase = |s: &str| format!(" {t} ").contains(&format!(" {s} "));
    let periodo = periodo(&w, hoje);
    let resto: Vec<String> = w
        .iter()
        .filter(|x| x.len() >= 2 && !VAZIAS.contains(x) && !comeca(x, RAIZES) && !MESES.contains(x) && x.parse::<i64>().is_err())
        .map(|x| x.to_string())
        .collect();
    let pergunta = algum(&["quant", "qual", "quais", "quem", "como", "onde"]) || texto.contains('?');
    let sobre_clientes = algum(&["client"]) || frase("quem compra") || frase("quem comprou") || frase("quem mais compra");
    let ranking = algum(&["melhor", "maior", "top"]) || frase("mais vendido") || frase("mais vendidos") || frase("vende mais") || frase("vendeu mais")
        || frase("compra mais") || frase("comprou mais") || frase("compram mais");
    let intencao = if algum(&["alerta", "problema", "errad", "invulga"]) || frase("fora do habitual") {
        Intencao::Alertas
    } else if algum(&["previs", "prever", "projec"]) || frase("fecho do mes") || frase("fechar o mes") || frase("vou vender") || frase("vamos vender") {
        Intencao::Previsao
    } else if algum(&["esgot", "ruptur"]) || frase("vai acabar") || frase("vao acabar") || frase("em falta") || frase("abaixo do minimo") {
        Intencao::Esgotar
    } else if algum(&["caixa"]) {
        Intencao::Caixa
    } else if ranking && sobre_clientes {
        Intencao::TopClientes
    } else if ranking {
        Intencao::TopArtigos
    } else if algum(&["devo", "devemo", "credor"]) || (algum(&["pag", "fornecedor"]) && !algum(&["pagou", "pagaram", "recebi"])) {
        Intencao::APagar
    } else if algum(&["deve", "devem", "divid", "receb", "atras", "devedor", "saldo"]) {
        Intencao::AReceber
    } else if algum(&["stock", "estoq", "existenc", "inventar"]) || (algum(&["quant"]) && w.iter().any(|x| matches!(*x, "tenho" | "temos" | "ha" | "restam" | "sobram" | "unidades"))) {
        Intencao::Stock
    } else if algum(&["preco", "custa", "pvp"]) {
        Intencao::Preco
    } else if algum(&["lucr", "margem", "ganh", "rentab"]) {
        Intencao::Lucro
    } else if algum(&["ticket", "talao"]) || frase("venda media") || frase("media por venda") {
        Intencao::Ticket
    } else if algum(&["iva", "impost"]) {
        Intencao::Iva
    } else if (w.iter().any(|x| matches!(*x, "comprei" | "compramos" | "compras")) || algum(&["gast", "despes"])) && !sobre_clientes {
        // «comprei», «compras», «gastei»: o que a empresa comprou («comprou» é um cliente: vendas)
        Intencao::Compras
    } else if algum(&["vend", "factur", "fatur", "receit", "compr"]) || (periodo.is_some() && resto.is_empty() && w.len() <= 4) {
        Intencao::Vendas
    } else if pergunta {
        Intencao::Ficha
    } else {
        return None;
    };
    if intencao == Intencao::Ficha && resto.is_empty() {
        return None;
    }
    Some(Pergunta { intencao, periodo, resto })
}

// ------------------------------------------------------------------ responder

/// Uma parte da resposta: texto ou um valor que o ecrã formata (kwanzas, quantidade, percentagem).
fn kz(c: i64) -> Value {
    json!({ "kz": c })
}
fn qtd(q: f64) -> Value {
    json!({ "qtd": q })
}
fn pct(p: f64) -> Value {
    json!({ "pct": p })
}
fn linha(rotulo: impl Into<String>, valor: Vec<Value>) -> Value {
    json!({ "rotulo": rotulo.into(), "valor": valor })
}
fn dmy(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}
fn intervalo(p: &Periodo) -> String {
    if p.de == p.ate {
        dmy(p.de)
    } else {
        format!("{} a {}", dmy(p.de), dmy(p.ate))
    }
}
fn resposta(entendi: String, partes: Vec<Value>, linhas: Vec<Value>, ligacao: &str, nota: Option<String>) -> Value {
    json!({ "entendi": entendi, "resposta": partes, "linhas": linhas, "ligacao": ligacao, "nota": nota })
}
fn plural(n: i64, um: &str, varios: &str) -> String {
    format!("{n} {}", if n == 1 { um } else { varios })
}

/// O artigo ou o cliente de que a pergunta fala: os que têm mais palavras da pergunta no nome (sem ligar a acentos);
/// em empate, os de nome mais curto (os mais parecidos com o que se escreveu).
#[derive(Default)]
struct Alvo {
    artigos: Vec<(i64, String)>,
    clientes: Vec<(i64, String)>,
    /// quantas palavras da pergunta o melhor artigo e o melhor cliente têm
    pontos_artigos: usize,
    pontos_clientes: usize,
}

/// Quantas palavras da pergunta estão no nome.
pub(crate) fn pontos(resto: &[String], nome: &str) -> usize {
    let n = normalizar(nome);
    resto.iter().filter(|p| p.len() >= 3 && n.contains(p.as_str()) || n.split(' ').any(|x| x == p.as_str())).count()
}

fn melhores(resto: &[String], nomes: Vec<(i64, String, String)>) -> (Vec<(i64, String)>, usize) {
    let mut v: Vec<(usize, i64, String)> = nomes.into_iter().map(|(id, nome, extra)| (pontos(resto, &format!("{nome} {extra}")), id, nome)).filter(|x| x.0 > 0).collect();
    let max = v.iter().map(|x| x.0).max().unwrap_or(0);
    v.retain(|x| x.0 == max);
    v.sort_by_key(|x| x.2.len());
    (v.into_iter().map(|(_, id, nome)| (id, nome)).collect(), max)
}

async fn procurar(st: &AppState, resto: &[String]) -> AppResult<Alvo> {
    if resto.is_empty() {
        return Ok(Alvo::default());
    }
    let artigos = sqlx::query("SELECT id, descricao, referencia FROM artigos WHERE ativo = 1").fetch_all(&st.pool).await?;
    let clientes = sqlx::query("SELECT id, nome FROM entidades WHERE id <> 1 AND tipo IN ('C', 'A')").fetch_all(&st.pool).await?;
    let (artigos, pontos_artigos) = melhores(resto, artigos.iter().map(|r| (r.get("id"), r.get("descricao"), r.get("referencia"))).collect());
    let (clientes, pontos_clientes) = melhores(resto, clientes.iter().map(|r| (r.get("id"), r.get("nome"), String::new())).collect());
    Ok(Alvo { artigos, clientes, pontos_artigos, pontos_clientes })
}

pub async fn perguntar(st: &AppState, i: PerguntaIn) -> AppResult<Value> {
    st.exigir_perm("relatorios.ver")?;
    let hoje = data(&db::hoje()).unwrap_or_else(|| chrono::Local::now().date_naive());
    let Some(q) = interpretar(&i.texto, hoje) else { return Ok(Value::Null) };
    responder(st, &q, hoje).await
}

pub(crate) async fn responder(st: &AppState, q: &Pergunta, hoje: NaiveDate) -> AppResult<Value> {
    let p = &st.pool;
    let loja = filtro_loja(st);
    let precisa = matches!(q.intencao, Intencao::Vendas | Intencao::Ficha | Intencao::AReceber | Intencao::Stock | Intencao::Preco);
    let alvo = if precisa { procurar(st, &q.resto).await? } else { Alvo::default() };
    let nao_encontrei = (!q.resto.is_empty() && alvo.artigos.is_empty() && alvo.clientes.is_empty())
        .then(|| format!("Não encontrei «{}» nos artigos nem nos clientes.", q.resto.join(" ")));
    let por_omissao = |nome: &str, de: NaiveDate| Periodo { de, ate: hoje, nome: nome.to_string() };
    let hoje_p = por_omissao("hoje", hoje);
    let mes_p = por_omissao("este mês", hoje.with_day(1).unwrap_or(hoje));
    let ano_p = por_omissao("este ano", NaiveDate::from_ymd_opt(hoje.year(), 1, 1).unwrap_or(hoje));

    match q.intencao {
        Intencao::Vendas | Intencao::Ficha if !alvo.clientes.is_empty() && alvo.pontos_clientes >= alvo.pontos_artigos => {
            // um cliente: o que comprou no período (por omissão, este ano) e o que deve
            let (id, nome) = alvo.clientes[0].clone();
            let per = q.periodo.clone().unwrap_or(ano_p);
            let (total, docs): (i64, i64) = sqlx::query_as(&format!(
                "SELECT COALESCE(SUM({SINAL} * d.total), 0), COUNT(*) FROM documentos d WHERE {VENDAS} AND d.entidade_id = ?3 AND d.data_emissao BETWEEN ?1 AND ?2{loja}"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .bind(id)
            .fetch_one(p)
            .await?;
            let (deve, vencido): (i64, i64) = sqlx::query_as(&format!(
                "SELECT COALESCE(SUM(d.valor_pendente), 0), COALESCE(SUM(CASE WHEN COALESCE(d.data_vencimento, d.data_emissao) < ?2 THEN d.valor_pendente END), 0)
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = 'VENDAS' AND d.entidade_id = ?1{loja}"
            ))
            .bind(id)
            .bind(iso(hoje))
            .fetch_one(p)
            .await?;
            let ultima: Option<String> = sqlx::query_scalar(&format!("SELECT MAX(d.data_emissao) FROM documentos d WHERE {VENDAS} AND d.entidade_id = ?{loja}"))
                .bind(id)
                .fetch_one(p)
                .await?;
            let mut linhas = vec![linha("Em dívida", if vencido > 0 { vec![kz(deve), " (vencido ".into(), kz(vencido), ")".into()] } else { vec![kz(deve)] })];
            if let Some(u) = ultima.as_deref().and_then(data) {
                linhas.push(linha("Última compra", vec![format!("{} (há {})", dmy(u), plural((hoje - u).num_days(), "dia", "dias")).into()]));
            }
            if alvo.clientes.len() > 1 {
                linhas.push(linha("Outros com esse nome", vec![alvo.clientes[1..].iter().take(3).map(|c| c.1.clone()).collect::<Vec<_>>().join(", ").into()]));
            }
            Ok(resposta(
                format!("Compras de {nome} · {} ({})", per.nome, intervalo(&per)),
                vec![format!("{nome} comprou ").into(), kz(total), format!(" {} em {}.", per.nome, plural(docs, "documento", "documentos")).into()],
                linhas,
                &format!("/clientes?id={id}"),
                None,
            ))
        }
        Intencao::Vendas | Intencao::Ficha if !alvo.artigos.is_empty() => {
            // um artigo (ou vários com o mesmo nome): o vendido no período (por omissão, este mês), o stock e o preço
            let per = q.periodo.clone().unwrap_or(mes_p);
            let ids = alvo.artigos.iter().map(|a| a.0.to_string()).collect::<Vec<_>>().join(",");
            let r = sqlx::query(&format!(
                "SELECT COALESCE(SUM({SINAL} * l.quantidade), 0.0) AS q, COALESCE(SUM({SINAL} * l.total), 0) AS total, COUNT(DISTINCT d.id) AS docs
                 FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
                 WHERE {VENDAS} AND l.artigo_id IN ({ids}) AND d.data_emissao BETWEEN ?1 AND ?2{loja}"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .fetch_one(p)
            .await?;
            let nome = if alvo.artigos.len() == 1 { alvo.artigos[0].1.clone() } else { format!("{} artigos com «{}»", alvo.artigos.len(), q.resto.join(" ")) };
            let mut linhas = fichas_artigos(st, &alvo.artigos).await?;
            linhas.truncate(5);
            Ok(resposta(
                format!("Vendas de {nome} · {} ({})", per.nome, intervalo(&per)),
                vec![
                    "Vendeu ".into(),
                    qtd(r.get("q")),
                    format!(" de {nome} {}: ", per.nome).into(),
                    kz(r.get("total")),
                    format!(" em {}.", plural(r.get("docs"), "documento", "documentos")).into(),
                ],
                linhas,
                "/relatorios?agrupar=artigo",
                None,
            ))
        }
        Intencao::Ficha => Ok(Value::Null),
        Intencao::Vendas | Intencao::Ticket | Intencao::Iva => {
            let per = q.periodo.clone().unwrap_or(hoje_p);
            let soma = |de: NaiveDate, ate: NaiveDate| {
                let sql = format!(
                    "SELECT COALESCE(SUM({SINAL} * d.total), 0) AS total, COUNT(*) AS docs, COALESCE(SUM({SINAL} * d.total_iva), 0) AS iva
                     FROM documentos d WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2{loja}"
                );
                async move { sqlx::query(&sql).bind(iso(de)).bind(iso(ate)).fetch_one(p).await }
            };
            let r = soma(per.de, per.ate).await?;
            let dias = (per.ate - per.de).num_days() + 1;
            let ant = soma(per.de - Duration::days(dias), per.de - Duration::days(1)).await?;
            let (total, docs, iva): (i64, i64, i64) = (r.get("total"), r.get("docs"), r.get("iva"));
            let ant_total: i64 = ant.get("total");
            let ticket = if docs > 0 { total / docs } else { 0 };
            let entendi = |o: &str| format!("{o} · {} ({})", per.nome, intervalo(&per));
            let comparacao = linha(
                format!("Período anterior ({})", intervalo(&Periodo { de: per.de - Duration::days(dias), ate: per.de - Duration::days(1), nome: String::new() })),
                match inteligencia::variacao(total, ant_total as f64) {
                    Some(v) => vec![kz(ant_total), format!(" ({}{v}%)", if v > 0 { "+" } else { "" }).into()],
                    None => vec![kz(ant_total)],
                },
            );
            let nota = nao_encontrei.map(|n| format!("{n} Mostro o total."));
            Ok(match q.intencao {
                Intencao::Ticket => resposta(
                    entendi("Ticket médio"),
                    vec![format!("O ticket médio {} foi ", per.nome).into(), kz(ticket), format!(" ({}).", plural(docs, "documento", "documentos")).into()],
                    vec![linha("Vendas", vec![kz(total)])],
                    "/relatorios?agrupar=dia",
                    nota,
                ),
                Intencao::Iva => resposta(
                    entendi("IVA liquidado"),
                    vec![format!("IVA liquidado {}: ", per.nome).into(), kz(iva), ".".into()],
                    vec![linha("Vendas com IVA", vec![kz(total)])],
                    "/relatorios?agrupar=iva",
                    nota,
                ),
                _ if docs == 0 => resposta(entendi("Vendas"), vec![format!("Não houve vendas {}.", per.nome).into()], vec![comparacao], "/relatorios?agrupar=dia", nota),
                _ => resposta(
                    entendi("Vendas"),
                    vec!["Vendeu ".into(), kz(total), format!(" {} em {}.", per.nome, plural(docs, "documento", "documentos")).into()],
                    vec![linha("Ticket médio", vec![kz(ticket)]), comparacao],
                    "/relatorios?agrupar=dia",
                    nota,
                ),
            })
        }
        Intencao::Lucro => {
            let per = q.periodo.clone().unwrap_or(mes_p);
            let r = sqlx::query(&format!(
                "SELECT COALESCE(SUM({SINAL} * l.valor_liquido), 0) AS liquido, COALESCE(SUM({SINAL} * CAST(ROUND(l.custo * l.quantidade) AS INTEGER)), 0) AS custo,
                        COALESCE(SUM(CASE WHEN l.custo <= 0 THEN {SINAL} * l.valor_liquido END), 0) AS sem_custo
                 FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
                 WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2{loja}"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .fetch_one(p)
            .await?;
            let (liquido, custo, sem_custo): (i64, i64, i64) = (r.get("liquido"), r.get("custo"), r.get("sem_custo"));
            let com_custo = liquido - sem_custo;
            let lucro = com_custo - custo;
            let margem = inteligencia::margem(&inteligencia::Dia { liquido_com_custo: com_custo, custo, ..Default::default() });
            let mut linhas = vec![linha("Vendas sem IVA", vec![kz(liquido)]), linha("Custo do vendido", vec![kz(custo)])];
            if let Some(m) = margem {
                linhas.insert(0, linha("Margem", vec![pct(m)]));
            }
            Ok(resposta(
                format!("Lucro bruto · {} ({})", per.nome, intervalo(&per)),
                vec![format!("O lucro bruto {} foi ", per.nome).into(), kz(lucro), ".".into()],
                linhas,
                "/relatorios?agrupar=artigo",
                (sem_custo > 0).then(|| "As vendas de artigos sem custo registado (serviços, por exemplo) ficam fora da conta.".to_string()),
            ))
        }
        Intencao::TopArtigos => {
            let per = q.periodo.clone().unwrap_or(mes_p);
            let rows = sqlx::query(&format!(
                "SELECT l.descricao, SUM({SINAL} * l.quantidade) AS quantidade, SUM({SINAL} * l.total) AS total
                 FROM documento_linhas l JOIN documentos d ON d.id = l.documento_id
                 WHERE {VENDAS} AND d.data_emissao BETWEEN ?1 AND ?2{loja} GROUP BY l.referencia, l.descricao ORDER BY total DESC LIMIT 5"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .fetch_all(p)
            .await?;
            let linhas: Vec<Value> = rows.iter().map(|r| linha(r.get::<String, _>("descricao"), vec![kz(r.get("total")), " · ".into(), qtd(r.get("quantidade"))])).collect();
            Ok(resposta(
                format!("Artigos mais vendidos · {} ({})", per.nome, intervalo(&per)),
                match rows.first() {
                    Some(r) => vec![format!("O mais vendido {} foi {}, com ", per.nome, r.get::<String, _>("descricao")).into(), kz(r.get("total")), ".".into()],
                    None => vec![format!("Não houve vendas {}.", per.nome).into()],
                },
                linhas,
                "/relatorios?agrupar=artigo",
                None,
            ))
        }
        Intencao::TopClientes => {
            let per = q.periodo.clone().unwrap_or(mes_p);
            let rows = sqlx::query(&format!(
                "SELECT MAX(d.entidade_nome) AS nome, SUM({SINAL} * d.total) AS total, COUNT(*) AS docs FROM documentos d
                 WHERE {VENDAS} AND d.entidade_id IS NOT NULL AND d.entidade_id <> 1 AND d.data_emissao BETWEEN ?1 AND ?2{loja}
                 GROUP BY d.entidade_id ORDER BY total DESC LIMIT 5"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .fetch_all(p)
            .await?;
            let linhas: Vec<Value> = rows.iter().map(|r| linha(r.get::<String, _>("nome"), vec![kz(r.get("total")), format!(" · {}", plural(r.get("docs"), "doc.", "docs.")).into()])).collect();
            Ok(resposta(
                format!("Melhores clientes · {} ({})", per.nome, intervalo(&per)),
                match rows.first() {
                    Some(r) => vec![format!("O melhor cliente {} foi {}, com ", per.nome, r.get::<String, _>("nome")).into(), kz(r.get("total")), ".".into()],
                    None => vec![format!("Não houve vendas a clientes identificados {}.", per.nome).into()],
                },
                linhas,
                "/relatorios?agrupar=cliente",
                None,
            ))
        }
        Intencao::AReceber | Intencao::APagar => {
            let receber = q.intencao == Intencao::AReceber;
            let grupo = if receber { "VENDAS" } else { "COMPRAS" };
            let filtro = if receber { alvo.clientes.first().map(|c| format!(" AND d.entidade_id = {}", c.0)).unwrap_or_default() } else { String::new() };
            let r = sqlx::query(&format!(
                "SELECT COALESCE(SUM(d.valor_pendente), 0) AS total, COUNT(DISTINCT d.entidade_id) AS n,
                        COALESCE(SUM(CASE WHEN COALESCE(d.data_vencimento, d.data_emissao) < ?1 THEN d.valor_pendente END), 0) AS vencido
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ?2{loja}{filtro}"
            ))
            .bind(iso(hoje))
            .bind(grupo)
            .fetch_one(p)
            .await?;
            let (total, n, vencido): (i64, i64, i64) = (r.get("total"), r.get("n"), r.get("vencido"));
            let top = sqlx::query(&format!(
                "SELECT MAX(d.entidade_nome) AS nome, SUM(d.valor_pendente) AS v,
                        CAST(MAX(julianday(?1) - julianday(COALESCE(d.data_vencimento, d.data_emissao))) AS INTEGER) AS dias
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE d.estado = 'N' AND d.valor_pendente > 0 AND t.grupo = ?2{loja}{filtro} GROUP BY d.entidade_id ORDER BY v DESC LIMIT 5"
            ))
            .bind(iso(hoje))
            .bind(grupo)
            .fetch_all(p)
            .await?;
            let mut linhas = vec![linha("Vencido", vec![kz(vencido)])];
            linhas.extend(top.iter().map(|r| {
                let dias: i64 = r.get("dias");
                linha(r.get::<String, _>("nome"), if dias > 0 { vec![kz(r.get("v")), format!(" · vencido há {dias} dias").into()] } else { vec![kz(r.get("v")), " · por vencer".into()] })
            }));
            let partes = match (receber, alvo.clientes.first()) {
                (true, Some((_, nome))) if total == 0 => vec![format!("{nome} não deve nada.").into()],
                (true, Some((_, nome))) => vec![format!("{nome} deve ").into(), kz(total), ".".into()],
                (true, None) if total == 0 => vec!["Nenhum cliente lhe deve nada.".into()],
                (true, None) => vec![format!("{} ", if n == 1 { "1 cliente deve-lhe".to_string() } else { format!("{n} clientes devem-lhe") }).into(), kz(total), ".".into()],
                (false, _) if total == 0 => vec!["Não deve nada a fornecedores.".into()],
                (false, _) => vec!["Deve ".into(), kz(total), format!(" a {}.", plural(n, "fornecedor", "fornecedores")).into()],
            };
            Ok(resposta(
                if receber { "A receber de clientes".to_string() } else { "A pagar a fornecedores".to_string() },
                partes,
                linhas,
                if receber { "/avisos" } else { "/contas/pagar" },
                if receber { nao_encontrei.map(|n| format!("{n} Mostro o total.")) } else { None },
            ))
        }
        Intencao::Stock | Intencao::Preco => {
            if alvo.artigos.is_empty() && q.intencao == Intencao::Stock {
                // sem artigo: os que estão no mínimo ou abaixo
                let rows = sqlx::query(
                    "SELECT a.descricao, COALESCE(s.q, 0.0) AS stock, a.qtd_minima FROM artigos a
                     LEFT JOIN (SELECT artigo_id, SUM(quantidade) AS q FROM stock WHERE (?1 IS NULL OR armazem_id = ?1) GROUP BY artigo_id) s ON s.artigo_id = a.id
                     WHERE a.controla_stock = 1 AND a.ativo = 1 AND COALESCE(s.q, 0) <= a.qtd_minima ORDER BY COALESCE(s.q, 0) - a.qtd_minima LIMIT 100",
                )
                .bind(st.loja())
                .fetch_all(p)
                .await?;
                let n = rows.len() as i64;
                return Ok(resposta(
                    "Artigos no mínimo ou abaixo".to_string(),
                    if n == 0 { vec!["Nenhum artigo está no mínimo ou abaixo.".into()] } else { vec![format!("{} no mínimo ou abaixo.", plural(n, "artigo está", "artigos estão")).into()] },
                    rows.iter().take(5).map(|r| linha(r.get::<String, _>("descricao"), vec![qtd(r.get("stock")), " em stock · mínimo ".into(), qtd(r.get("qtd_minima"))])).collect(),
                    "/stock",
                    nao_encontrei,
                ));
            }
            if alvo.artigos.is_empty() {
                return Ok(resposta("Preço".to_string(), vec![nao_encontrei.unwrap_or_else(|| "Diga de que artigo, por exemplo «preço do arroz».".to_string()).into()], Vec::new(), "/artigos", None));
            }
            let linhas = fichas_artigos(st, &alvo.artigos).await?;
            let (id, nome) = alvo.artigos[0].clone();
            let r = sqlx::query(
                "SELECT COALESCE((SELECT SUM(s.quantidade) FROM stock s WHERE s.artigo_id = a.id AND (?2 IS NULL OR s.armazem_id = ?2)), 0.0) AS stock,
                        a.controla_stock, COALESCE(a.pvp1, CAST(ROUND(a.preco1 * (100 + COALESCE(t.percentagem, 0)) / 100.0) AS INTEGER)) AS pvp
                 FROM artigos a LEFT JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.id = ?1",
            )
            .bind(id)
            .bind(st.loja())
            .fetch_one(p)
            .await?;
            let partes = if q.intencao == Intencao::Preco {
                vec![format!("{nome} custa ").into(), kz(r.get("pvp")), " (com IVA).".into()]
            } else if r.get::<i64, _>("controla_stock") == 0 {
                vec![format!("{nome} não controla stock.").into()]
            } else {
                vec!["Tem ".into(), qtd(r.get("stock")), format!(" de {nome} em stock.").into()]
            };
            Ok(resposta(
                format!("{} · {nome}", if q.intencao == Intencao::Preco { "Preço" } else { "Stock" }),
                partes,
                linhas.into_iter().take(5).collect(),
                &format!("/artigos?id={id}"),
                None,
            ))
        }
        Intencao::Caixa => {
            let c = super::caixa::estado(st).await?;
            if c.is_null() {
                return Ok(resposta("Caixa deste posto".to_string(), vec!["O caixa deste posto está fechado.".into()], Vec::new(), "/caixa", None));
            }
            let desde = c["sessao"]["aberta_em"].as_str().unwrap_or("").replace('T', " ");
            let hora = desde.get(11..16).unwrap_or("");
            let docs = c["documentos"].as_i64().unwrap_or(0);
            if c["cego"].as_bool().unwrap_or(false) {
                return Ok(resposta(
                    "Caixa deste posto".to_string(),
                    vec![format!("O caixa está aberto desde as {hora}, com {}. É um fecho cego: os totais só se vêem ao fechar.", plural(docs, "documento", "documentos")).into()],
                    Vec::new(),
                    "/caixa",
                    None,
                ));
            }
            Ok(resposta(
                "Caixa deste posto".to_string(),
                vec![
                    format!("O caixa está aberto desde as {hora}, com {}: ", plural(docs, "documento", "documentos")).into(),
                    kz(c["total_vendido"].as_i64().unwrap_or(0)),
                    " vendidos a pronto.".into(),
                ],
                vec![
                    linha("Numerário esperado na gaveta", vec![kz(c["numerario_esperado"].as_i64().unwrap_or(0))]),
                    linha("Entradas e saídas", vec![kz(c["entradas"].as_i64().unwrap_or(0)), " / ".into(), kz(c["saidas"].as_i64().unwrap_or(0))]),
                ],
                "/caixa",
                None,
            ))
        }
        Intencao::Compras => {
            let per = q.periodo.clone().unwrap_or(mes_p);
            let (total, docs): (i64, i64) = sqlx::query_as(&format!(
                "SELECT COALESCE(SUM(CASE WHEN t.credito = 1 THEN -d.total ELSE d.total END), 0), COUNT(*)
                 FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE t.grupo = 'COMPRAS' AND t.classe = 'COMPRA' AND d.estado <> 'A' AND d.data_emissao BETWEEN ?1 AND ?2{loja}"
            ))
            .bind(iso(per.de))
            .bind(iso(per.ate))
            .fetch_one(p)
            .await?;
            Ok(resposta(
                format!("Compras · {} ({})", per.nome, intervalo(&per)),
                vec!["Comprou ".into(), kz(total), format!(" {} em {}.", per.nome, plural(docs, "documento", "documentos")).into()],
                Vec::new(),
                "/relatorios/fornecedores",
                None,
            ))
        }
        Intencao::Previsao => {
            let h = inteligencia::historico(st, hoje).await?;
            let f = inteligencia::fecho_do_mes(&h, hoje);
            if f.is_null() {
                return Ok(resposta("Previsão do mês".to_string(), vec!["Ainda há pouco histórico: a previsão aparece com 14 dias de vendas.".into()], Vec::new(), "/", None));
            }
            let mut linhas = vec![linha("Vendido até hoje", vec![kz(f["vendas_ate_hoje"].as_i64().unwrap_or(0))])];
            if let Some(m) = f["mes_anterior"].as_i64() {
                let v = f["variacao"].as_i64().map(|v| format!(" ({}{v}%)", if v > 0 { "+" } else { "" })).unwrap_or_default();
                linhas.push(linha("Mês anterior", vec![kz(m), v.into()]));
            }
            Ok(resposta(
                "Previsão do fecho do mês".to_string(),
                vec!["Ao ritmo das últimas semanas, o mês deve fechar em cerca de ".into(), kz(f["previsto"].as_i64().unwrap_or(0)), ".".into()],
                linhas,
                "/",
                None,
            ))
        }
        Intencao::Esgotar => {
            let h = inteligencia::historico(st, hoje).await?;
            let r = inteligencia::ruptura(st, hoje, &h).await?;
            let n = r["n"].as_i64().unwrap_or(0);
            let linhas = r["artigos"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .take(5)
                        .map(|x| {
                            let dias = x["dias"].as_i64().unwrap_or(0);
                            linha(x["descricao"].as_str().unwrap_or(""), vec![qtd(x["stock"].as_f64().unwrap_or(0.0)), if dias == 0 { " em stock · esgota hoje".into() } else { format!(" em stock · ~{}", plural(dias, "dia", "dias")).into() }])
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(resposta(
                "Artigos que esgotam nos próximos 7 dias".to_string(),
                if n == 0 { vec!["Ao ritmo actual, nenhum artigo esgota nos próximos 7 dias.".into()] } else { vec![format!("{} nos próximos 7 dias, ao ritmo de venda.", plural(n, "artigo esgota", "artigos esgotam")).into()] },
                linhas,
                "/compras/sugestao",
                None,
            ))
        }
        Intencao::Alertas => {
            let h = inteligencia::historico(st, hoje).await?;
            let a = inteligencia::alertas(st, &h, hoje).await?;
            let nomes: Vec<String> = a.iter().map(|x| nome_alerta(x["tipo"].as_str().unwrap_or("")).to_string()).collect();
            Ok(resposta(
                "Alertas".to_string(),
                match nomes.len() {
                    0 => vec!["Nada fora do habitual: vendas, margens, fechos de caixa, preços e clientes dentro do normal.".into()],
                    1 => vec![format!("Há uma coisa fora do habitual: {}.", nomes[0]).into()],
                    n => vec![format!("Há {n} coisas fora do habitual: {} e {}.", nomes[..n - 1].join(", "), nomes[n - 1]).into()],
                },
                Vec::new(),
                "/",
                None,
            ))
        }
    }
}

fn nome_alerta(tipo: &str) -> &'static str {
    match tipo {
        "abaixo_custo" => "vendas abaixo do custo",
        "caixa_diferenca" => "fechos de caixa com diferença",
        "anulacoes" => "anulações acima do habitual",
        "vendas_quebra" => "vendas abaixo do habitual",
        "margem_queda" => "a margem a cair",
        "preco_abaixo_custo" => "preços de venda abaixo do custo",
        "devedores" => "clientes em atraso",
        "clientes_parados" => "bons clientes que deixaram de comprar",
        "stock_parado" => "stock parado",
        _ => "outro alerta",
    }
}

/// Uma linha por artigo: stock e preço com IVA.
async fn fichas_artigos(st: &AppState, artigos: &[(i64, String)]) -> AppResult<Vec<Value>> {
    let ids = artigos.iter().take(5).map(|a| a.0.to_string()).collect::<Vec<_>>().join(",");
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(&format!(
        "SELECT a.id, a.descricao, a.controla_stock,
                COALESCE((SELECT SUM(s.quantidade) FROM stock s WHERE s.artigo_id = a.id AND (?1 IS NULL OR s.armazem_id = ?1)), 0.0) AS stock,
                COALESCE(a.pvp1, CAST(ROUND(a.preco1 * (100 + COALESCE(t.percentagem, 0)) / 100.0) AS INTEGER)) AS pvp
         FROM artigos a LEFT JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.id IN ({ids}) ORDER BY length(a.descricao)"
    ))
    .bind(st.loja())
    .fetch_all(&st.pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let mut v = vec![kz(r.get("pvp"))];
            if r.get::<i64, _>("controla_stock") == 1 {
                v.push(" · ".into());
                v.push(qtd(r.get("stock")));
                v.push(" em stock".into());
            }
            linha(r.get::<String, _>("descricao"), v)
        })
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::db;

    fn d(s: &str) -> NaiveDate {
        data(s).unwrap()
    }

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }

    /// O texto da resposta, com os valores escritos como no ecrã («1140,00 Kz», «3 un.»).
    fn texto(v: &Value) -> String {
        v["resposta"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| match x {
                Value::String(s) => s.clone(),
                o if o.get("kz").is_some() => format!("{} Kz", super::super::impressao::kz(o["kz"].as_i64().unwrap())),
                o if o.get("qtd").is_some() => super::super::impressao::qtd(o["qtd"].as_f64().unwrap()),
                o => super::super::impressao::pct(o["pct"].as_f64().unwrap()),
            })
            .collect()
    }

    #[tokio::test]
    async fn respostas_com_os_dados_de_uma_loja() {
        let pasta = std::env::temp_dir().join(format!("kangest-assistente-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
        let mut st = AppState::new(pool);
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.expect("posto 1");
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Exemplo, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
        rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo Fula 1L", "taxa_iva_id": 1, "preco1": 50000, "custo": 30000 })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": arroz, "quantidade": 10, "preco_unitario": 50000 }] })).await;
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 228000 }] })).await;
        let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Mercearia Boa Vista", "nif": "5417000001" })).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] })).await;
        let perguntar = |t: &str| {
            let st = &st;
            let t = t.to_string();
            async move { rpc(st, "inteligencia.perguntar", json!({ "texto": t })).await }
        };

        let r = perguntar("Quanto vendi hoje?").await;
        assert_eq!(texto(&r), "Vendeu 3.420,00 Kz hoje em 2 documentos.");
        assert_eq!(r["linhas"][0]["rotulo"], "Ticket médio");
        assert!(r["entendi"].as_str().unwrap().starts_with("Vendas · hoje"));
        assert_eq!(texto(&perguntar("e ontem?").await), "Não houve vendas ontem.");
        assert_eq!(texto(&perguntar("lucro de hoje").await), "O lucro bruto hoje foi 1.500,00 Kz.");
        assert_eq!(texto(&perguntar("quanto vendi de arroz este mês").await), "Vendeu 3 de Arroz 5kg este mês: 3.420,00 Kz em 2 documentos.");
        assert_eq!(texto(&perguntar("quantos sacos de arroz tenho?").await), "Tem 7 de Arroz 5kg em stock.");
        assert_eq!(texto(&perguntar("quanto custa o oleo").await), "Óleo Fula 1L custa 570,00 Kz (com IVA).");
        assert_eq!(texto(&perguntar("quanto deve a mercearia boa vista?").await), "Mercearia Boa Vista deve 1.140,00 Kz.");
        assert_eq!(texto(&perguntar("quem me deve?").await), "1 cliente deve-lhe 1.140,00 Kz.");
        let r = perguntar("quanto comprou a mercearia este ano").await;
        assert_eq!(texto(&r), "Mercearia Boa Vista comprou 1.140,00 Kz este ano em 1 documento.");
        assert_eq!(r["ligacao"], format!("/clientes?id={cli}"));
        let r = perguntar("melhores clientes este mês").await;
        assert_eq!(r["linhas"][0]["rotulo"], "Mercearia Boa Vista");
        assert_eq!(perguntar("artigos mais vendidos").await["linhas"][0]["rotulo"], "Arroz 5kg");
        assert!(texto(&perguntar("como está o caixa?").await).starts_with("O caixa está aberto desde as "));
        assert_eq!(texto(&perguntar("quanto devo a fornecedores").await), "Não deve nada a fornecedores.");
        assert!(texto(&perguntar("o que vai esgotar?").await).contains("esgot"));
        assert!(texto(&perguntar("há algum problema?").await).contains("habitual"));
        assert_eq!(texto(&perguntar("previsão do mês").await), "Ainda há pouco histórico: a previsão aparece com 14 dias de vendas.");
        assert_eq!(perguntar("stock do feijão").await["nota"], "Não encontrei «feijao» nos artigos nem nos clientes.");
        assert!(perguntar("FT 2026/1").await.is_null(), "a pesquisa normal trata dos números de documento");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn periodos() {
        let hoje = d("2026-11-22"); // domingo
        let p = |t: &str| {
            let n = normalizar(t);
            let w: Vec<&str> = n.split(' ').collect();
            periodo(&w, hoje).map(|p| (iso(p.de), iso(p.ate), p.nome))
        };
        assert_eq!(p("quanto vendi ontem"), Some(("2026-11-21".into(), "2026-11-21".into(), "ontem".into())));
        assert_eq!(p("vendas de hoje"), Some(("2026-11-22".into(), "2026-11-22".into(), "hoje".into())));
        assert_eq!(p("esta semana").unwrap().0, "2026-11-16", "a semana começa à segunda-feira");
        assert_eq!(p("semana passada"), Some(("2026-11-09".into(), "2026-11-15".into(), "na semana passada".into())));
        assert_eq!(p("este mês"), Some(("2026-11-01".into(), "2026-11-22".into(), "este mês".into())));
        assert_eq!(p("no mês passado"), Some(("2026-10-01".into(), "2026-10-31".into(), "no mês passado".into())));
        assert_eq!(p("em Março"), Some(("2026-03-01".into(), "2026-03-31".into(), "em março de 2026".into())));
        assert_eq!(p("em dezembro"), Some(("2025-12-01".into(), "2025-12-31".into(), "em dezembro de 2025".into())), "um mês que ainda não chegou é do ano passado");
        assert_eq!(p("fevereiro de 2024").unwrap().1, "2024-02-29");
        assert_eq!(p("em 2025"), Some(("2025-01-01".into(), "2025-12-31".into(), "em 2025".into())));
        assert_eq!(p("este ano").unwrap().0, "2026-01-01");
        assert_eq!(p("nos últimos 15 dias"), Some(("2026-11-08".into(), "2026-11-22".into(), "nos últimos 15 dias".into())));
        assert_eq!(p("quanto vendi"), None);
    }

    #[test]
    fn intencoes_e_o_que_sobra() {
        let hoje = d("2026-11-22");
        let i = |t: &str| interpretar(t, hoje).map(|p| (p.intencao, p.resto.join(" ")));
        assert_eq!(i("Quanto vendi ontem?"), Some((Intencao::Vendas, String::new())));
        assert_eq!(i("quanto facturámos este mês"), Some((Intencao::Vendas, String::new())));
        assert_eq!(i("quanto vendi de arroz em outubro"), Some((Intencao::Vendas, "arroz".into())));
        assert_eq!(i("e ontem?"), Some((Intencao::Vendas, String::new())));
        assert_eq!(i("qual foi o lucro do mês passado"), Some((Intencao::Lucro, String::new())));
        assert_eq!(i("margem desta semana"), Some((Intencao::Lucro, String::new())));
        assert_eq!(i("ticket médio de hoje"), Some((Intencao::Ticket, String::new())));
        assert_eq!(i("IVA de outubro"), Some((Intencao::Iva, String::new())));
        assert_eq!(i("artigos mais vendidos este mês"), Some((Intencao::TopArtigos, String::new())));
        assert_eq!(i("o que vende mais"), Some((Intencao::TopArtigos, String::new())));
        assert_eq!(i("melhores clientes de 2025"), Some((Intencao::TopClientes, String::new())));
        assert_eq!(i("quem compra mais?"), Some((Intencao::TopClientes, String::new())));
        assert_eq!(i("quem me deve mais"), Some((Intencao::AReceber, String::new())));
        assert_eq!(i("quanto deve a Mercearia Boa Vista"), Some((Intencao::AReceber, "mercearia boa vista".into())));
        assert_eq!(i("quanto devo aos fornecedores"), Some((Intencao::APagar, String::new())));
        assert_eq!(i("contas a pagar"), Some((Intencao::APagar, String::new())));
        assert_eq!(i("stock do óleo"), Some((Intencao::Stock, "oleo".into())));
        assert_eq!(i("quantos sacos de arroz tenho"), Some((Intencao::Stock, "sacos arroz".into())));
        assert_eq!(i("quanto custa o Hinário"), Some((Intencao::Preco, "hinario".into())));
        assert_eq!(i("como está o caixa"), Some((Intencao::Caixa, String::new())));
        assert_eq!(i("previsão do mês"), Some((Intencao::Previsao, String::new())));
        assert_eq!(i("o que vai acabar"), Some((Intencao::Esgotar, String::new())));
        assert_eq!(i("artigos a esgotar"), Some((Intencao::Esgotar, String::new())));
        assert_eq!(i("há algum problema?"), Some((Intencao::Alertas, String::new())));
        assert_eq!(i("quanto gastei em compras este mês"), Some((Intencao::Compras, String::new())));
        assert_eq!(i("quanto comprou o colégio este ano"), Some((Intencao::Vendas, "colegio".into())));
        // não são perguntas: a pesquisa normal trata delas
        assert!(i("FT 2026/12").is_none());
        assert!(i("Mercearia Boa Vista").is_none());
        assert!(i("").is_none());
    }
}

// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Motor de consultas: o construtor de relatórios (e, depois, o BI e a qualidade dos dados) pede «esta fonte, estes
//! campos, estes filtros, agrupado assim, ordenado assim» e recebe as linhas e os totais.
//!
//! Segurança: o SQL monta-se só com as expressões do registo (`fontes.rs`) e com operadores e funções de listas
//! fechadas; os nomes das colunas são aliases internos (`c0`, `c1`…); os valores dos filtros vão sempre como
//! parâmetros. O motor só lê: nada aqui escreve na base. A permissão é a da fonte.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;

use super::fontes::{fonte, Campo, Fonte, Tipo, FONTES};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::services::relatorios::{SINAL, VENDAS};
use crate::state::AppState;

/// Linhas devolvidas por omissão e no máximo (o total geral conta-as todas).
const LIMITE: i64 = 1000;
const LIMITE_MAX: i64 = 5000;
/// Valores de um filtro «em» (lista).
const MAX_EM: usize = 200;

#[derive(Deserialize, Default)]
pub struct ConsultaIn {
    pub fonte: String,
    /// colunas (sem agrupamento); vazio = todos os campos da fonte
    #[serde(default)]
    pub campos: Vec<String>,
    #[serde(default)]
    pub filtros: Vec<Filtro>,
    /// agrupar por estes campos: cada linha é um grupo, com os agregados
    #[serde(default)]
    pub agrupar: Vec<String>,
    #[serde(default)]
    pub agregados: Vec<Agregado>,
    /// várias colunas, pela ordem: um campo, ou um agregado como «soma:total»
    #[serde(default)]
    pub ordenar: Vec<Ordem>,
    #[serde(default)]
    pub limite: Option<i64>,
}

#[derive(Deserialize, Clone)]
pub struct Filtro {
    pub campo: String,
    /// = <> > >= < <= contem comeca entre em vazio naovazio
    pub op: String,
    #[serde(default)]
    pub valor: Value,
}

#[derive(Deserialize, Clone)]
pub struct Agregado {
    /// soma media min max contar
    pub funcao: String,
    /// vazio só em «contar»
    #[serde(default)]
    pub campo: String,
}

#[derive(Deserialize, Clone)]
pub struct Ordem {
    pub campo: String,
    #[serde(default)]
    pub desc: bool,
}

/// Um valor de filtro, já validado, para ligar à consulta como parâmetro.
#[derive(Debug, Clone)]
enum Valor {
    Texto(String),
    Inteiro(i64),
    Real(f64),
}

fn expr(c: &Campo) -> String {
    c.expr.replace("{SINAL}", SINAL)
}

fn de(f: &Fonte) -> String {
    f.de.replace("{VENDAS}", VENDAS)
}

fn campo<'a>(f: &'a Fonte, id: &str) -> AppResult<&'a Campo> {
    f.campo(id).ok_or_else(|| AppError::msg(format!("O campo «{id}» não existe em «{}».", f.rotulo)))
}

/// O valor de um filtro no tipo do campo (dinheiro em cêntimos, datas AAAA-MM-DD, lógicos 0/1).
fn valor(c: &Campo, v: &Value) -> AppResult<Valor> {
    let erro = || AppError::msg(format!("Valor inválido para «{}».", c.rotulo));
    Ok(match c.tipo {
        Tipo::Texto => Valor::Texto(match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(erro()),
        }),
        Tipo::Data => {
            let s = v.as_str().ok_or_else(erro)?.trim();
            let ok = s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' && s.chars().filter(|x| x.is_ascii_digit()).count() == 8;
            if !ok {
                return Err(AppError::msg(format!("Data inválida em «{}»: use AAAA-MM-DD.", c.rotulo)));
            }
            Valor::Texto(s.to_string())
        }
        Tipo::Dinheiro => Valor::Inteiro(v.as_i64().or_else(|| v.as_f64().map(|x| x.round() as i64)).ok_or_else(erro)?),
        Tipo::Numero => Valor::Real(v.as_f64().ok_or_else(erro)?),
        Tipo::Logico => Valor::Inteiro(match v {
            Value::Bool(b) => *b as i64,
            Value::Number(n) if n.as_i64() == Some(0) || n.as_i64() == Some(1) => n.as_i64().unwrap_or(0),
            _ => return Err(erro()),
        }),
    })
}

/// A condição SQL de um filtro, com os seus parâmetros.
fn condicao(f: &Fonte, fl: &Filtro, params: &mut Vec<Valor>) -> AppResult<String> {
    let c = campo(f, &fl.campo)?;
    let e = expr(c);
    Ok(match fl.op.as_str() {
        op @ ("=" | "<>" | ">" | ">=" | "<" | "<=") => {
            if c.tipo == Tipo::Logico && !matches!(op, "=" | "<>") {
                return Err(AppError::msg(format!("«{}» só se compara com igual ou diferente.", c.rotulo)));
            }
            params.push(valor(c, &fl.valor)?);
            format!("({e}) {op} ?")
        }
        "contem" | "comeca" => {
            if c.tipo != Tipo::Texto {
                return Err(AppError::msg(format!("«{}» não é texto.", c.rotulo)));
            }
            let Valor::Texto(s) = valor(c, &fl.valor)? else { unreachable!() };
            // os caracteres especiais do LIKE valem por si
            let s = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            params.push(Valor::Texto(if fl.op == "contem" { format!("%{s}%") } else { format!("{s}%") }));
            format!("({e}) LIKE ? ESCAPE '\\'")
        }
        "entre" => {
            let par = fl.valor.as_array().filter(|a| a.len() == 2).ok_or_else(|| AppError::msg(format!("«{}»: «entre» leva dois valores.", c.rotulo)))?;
            params.push(valor(c, &par[0])?);
            params.push(valor(c, &par[1])?);
            format!("({e}) BETWEEN ? AND ?")
        }
        "em" => {
            let lista = fl.valor.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_EM).ok_or_else(|| AppError::msg(format!("«{}»: escolha entre 1 e {MAX_EM} valores.", c.rotulo)))?;
            for v in lista {
                params.push(valor(c, v)?);
            }
            format!("({e}) IN ({})", vec!["?"; lista.len()].join(", "))
        }
        "vazio" => format!("(({e}) IS NULL OR ({e}) = '')"),
        "naovazio" => format!("(({e}) IS NOT NULL AND ({e}) <> '')"),
        outro => return Err(AppError::msg(format!("Operador desconhecido: «{outro}»."))),
    })
}

/// SQL de um agregado e o seu tipo de resultado.
fn agregado(f: &Fonte, a: &Agregado) -> AppResult<(String, Tipo, String)> {
    let chave = if a.funcao == "contar" { "contar".to_string() } else { format!("{}:{}", a.funcao, a.campo) };
    if a.funcao == "contar" {
        return Ok(("COUNT(*)".into(), Tipo::Numero, chave));
    }
    let c = campo(f, &a.campo)?;
    let e = expr(c);
    let (sql, tipo) = match a.funcao.as_str() {
        "soma" if !c.somavel => return Err(AppError::msg(format!("«{}» não é um valor que se some.", c.rotulo))),
        "media" if !c.tipo.quantitativo() => return Err(AppError::msg(format!("«{}» não é um número.", c.rotulo))),
        "soma" => (format!("SUM({e})"), c.tipo),
        // a média de dinheiro arredonda ao cêntimo
        "media" if c.tipo == Tipo::Dinheiro => (format!("CAST(ROUND(AVG({e})) AS INTEGER)"), Tipo::Dinheiro),
        "media" => (format!("AVG({e})"), Tipo::Numero),
        "min" => (format!("MIN({e})"), c.tipo),
        "max" => (format!("MAX({e})"), c.tipo),
        outra => return Err(AppError::msg(format!("Função desconhecida: «{outra}»."))),
    };
    Ok((sql, tipo, chave))
}

fn rotulo_agregado(f: &Fonte, a: &Agregado) -> String {
    let nome = f.campo(&a.campo).map(|c| c.rotulo).unwrap_or("");
    match a.funcao.as_str() {
        "contar" => "N.º de registos".into(),
        "soma" => nome.to_string(),
        "media" => format!("Média de {}", nome.to_lowercase()),
        "min" => format!("Mínimo de {}", nome.to_lowercase()),
        "max" => format!("Máximo de {}", nome.to_lowercase()),
        _ => nome.to_string(),
    }
}

fn ligar<'q>(mut q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>, params: &'q [Valor]) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    for p in params {
        q = match p {
            Valor::Texto(s) => q.bind(s.as_str()),
            Valor::Inteiro(i) => q.bind(*i),
            Valor::Real(r) => q.bind(*r),
        };
    }
    q
}

/// As fontes que este utilizador pode consultar, com os campos (para o construtor de relatórios).
pub async fn fontes_listar(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    Ok(Value::Array(
        FONTES
            .iter()
            .filter(|f| s.tem(f.permissao))
            .map(|f| {
                json!({
                    "id": f.id, "rotulo": f.rotulo, "descricao": f.descricao, "data": f.data,
                    "campos": f.campos.iter().map(|c| json!({ "id": c.id, "rotulo": c.rotulo, "tipo": c.tipo.nome() })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    ))
}

/// Consulta uma fonte: linhas (ou grupos) e o total geral de todas as linhas que passam nos filtros.
pub async fn consultar(st: &AppState, q: ConsultaIn) -> AppResult<Value> {
    let f = fonte(&q.fonte).ok_or_else(|| AppError::msg(format!("Fonte de dados desconhecida: «{}».", q.fonte)))?;
    st.exigir_perm(f.permissao)?;

    let mut params = Vec::new();
    let mut onde = String::new();
    // o utilizador de uma loja só vê a dele, peça o que pedir (a loja vem da sessão, não do pedido)
    if let (Some(loja), false) = (st.loja(), f.loja.is_empty()) {
        onde.push_str(&format!(" AND {} = ?", f.loja));
        params.push(Valor::Inteiro(loja));
    }
    for fl in &q.filtros {
        onde.push_str(" AND ");
        onde.push_str(&condicao(f, fl, &mut params)?);
    }
    let base = format!("{}{onde}", de(f));

    // colunas: (sql, alias, id, rótulo, tipo, entra no total)
    let mut colunas: Vec<(String, String, String, String, Tipo, bool)> = Vec::new();
    let agrupado = !q.agrupar.is_empty() || !q.agregados.is_empty();
    if agrupado {
        if q.agregados.is_empty() {
            return Err(AppError::msg("Escolha o que somar ou contar em cada grupo."));
        }
        for id in &q.agrupar {
            let c = campo(f, id)?;
            colunas.push((expr(c), format!("c{}", colunas.len()), c.id.into(), c.rotulo.into(), c.tipo, false));
        }
        for a in &q.agregados {
            let (sql, tipo, chave) = agregado(f, a)?;
            colunas.push((sql, format!("c{}", colunas.len()), chave, rotulo_agregado(f, a), tipo, true));
        }
    } else {
        let ids: Vec<&str> = if q.campos.is_empty() { f.campos.iter().map(|c| c.id).collect() } else { q.campos.iter().map(String::as_str).collect() };
        for id in ids {
            let c = campo(f, id)?;
            colunas.push((expr(c), format!("c{}", colunas.len()), c.id.into(), c.rotulo.into(), c.tipo, c.somavel));
        }
    }
    if colunas.is_empty() {
        return Err(AppError::msg("Escolha pelo menos uma coluna."));
    }

    // ordenação por várias colunas: só colunas pedidas (pelo id do campo ou do agregado)
    let mut ordem = Vec::new();
    for o in &q.ordenar {
        let alias = colunas.iter().find(|c| c.2 == o.campo).map(|c| c.1.clone()).ok_or_else(|| AppError::msg(format!("Só se ordena por colunas do relatório («{}»).", o.campo)))?;
        ordem.push(format!("{alias} {}", if o.desc { "DESC" } else { "ASC" }));
    }

    let limite = q.limite.unwrap_or(LIMITE).clamp(1, LIMITE_MAX);
    let lista = colunas.iter().map(|c| format!("{} AS {}", c.0, c.1)).collect::<Vec<_>>().join(", ");
    let grupos = if q.agrupar.is_empty() { String::new() } else { format!(" GROUP BY {}", (0..q.agrupar.len()).map(|i| format!("c{i}")).collect::<Vec<_>>().join(", ")) };
    let ordenar = if ordem.is_empty() { String::new() } else { format!(" ORDER BY {}", ordem.join(", ")) };
    let sql = format!("SELECT {lista} FROM {base}{grupos}{ordenar} LIMIT {}", limite + 1);

    let linhas_db = ligar(sqlx::query(&sql), &params).fetch_all(&st.pool).await?;
    let truncado = linhas_db.len() as i64 > limite;
    let linhas: Vec<Value> = linhas_db
        .iter()
        .take(limite as usize)
        .map(|r| {
            let o = db::row_json(r);
            Value::Array(colunas.iter().map(|c| o.get(&c.1).cloned().unwrap_or(Value::Null)).collect())
        })
        .collect();

    // total geral (todas as linhas que passam nos filtros, não só as mostradas) e o número de linhas ou grupos
    let mut totais = Map::new();
    let somas: Vec<&(String, String, String, String, Tipo, bool)> = colunas.iter().filter(|c| c.5).collect();
    if !somas.is_empty() {
        let sel = somas
            .iter()
            .map(|c| {
                // num agrupamento o agregado já é a função; sem agrupamento soma-se a coluna
                let s = if agrupado { c.0.clone() } else { format!("SUM({})", c.0) };
                format!("{s} AS {}", c.1)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let r = ligar(sqlx::query(&format!("SELECT {sel} FROM {base}")), &params).fetch_one(&st.pool).await?;
        let o = db::row_json(&r);
        for c in &somas {
            totais.insert(c.2.clone(), o.get(&c.1).cloned().unwrap_or(Value::Null));
        }
    }
    let n: i64 = if q.agrupar.is_empty() && agrupado {
        1
    } else {
        let contar = if q.agrupar.is_empty() {
            format!("SELECT COUNT(*) FROM {base}")
        } else {
            let g = q.agrupar.iter().map(|id| campo(f, id).map(expr)).collect::<AppResult<Vec<_>>>()?.join(", ");
            format!("SELECT COUNT(*) FROM (SELECT 1 FROM {base} GROUP BY {g})")
        };
        ligar(sqlx::query(&contar), &params).fetch_one(&st.pool).await?.try_get(0)?
    };

    Ok(json!({
        "fonte": f.id,
        "colunas": colunas.iter().map(|c| json!({ "id": c.2, "rotulo": c.3, "tipo": c.4.nome() })).collect::<Vec<_>>(),
        "linhas": linhas,
        "totais": totais,
        "total_linhas": n,
        "truncado": truncado,
    }))
}

#[cfg(test)]
mod testes {
    use serde_json::{json, Value};

    use crate::db;
    use crate::state::AppState;

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }
    async fn erro(st: &AppState, m: &str, p: Value) -> String {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => panic!("{m} devia falhar: {v}"),
            Err(e) => e.to_string(),
        }
    }

    /// O motor dá os mesmos totais que os relatórios fixos, recusa o que não está no registo, trata texto com SQL
    /// como texto e respeita a permissão da fonte.
    #[tokio::test]
    async fn consultas_seguras_e_certas() {
        let pasta = std::env::temp_dir().join(format!("kangest-dados-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "taxa_iva_id": 1, "preco1": 100000, "custo": 60000 })).await["id"].as_i64().unwrap();
        let oleo = rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo 1L", "taxa_iva_id": 1, "preco1": 50000, "custo": 30000 })).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": arroz, "quantidade": 20, "preco_unitario": 60000 }, { "artigo_id": oleo, "quantidade": 20, "preco_unitario": 30000 }] })).await;
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
        let fr = |a: i64, q: f64| json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": q }], "pagamentos": [{ "modo": "NU", "valor": 10_000_000 }] });
        let v1 = rpc(&st, "documentos.emitir", fr(arroz, 2.0)).await;
        rpc(&st, "documentos.emitir", fr(oleo, 3.0)).await;
        // nota de crédito de uma unidade de arroz: subtrai
        rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": 1, "doc_origem_id": v1["documento"]["id"], "motivo": "Devolução de 1 unidade", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] })).await;
        let hoje = db::hoje();

        // por mês: igual ao relatório de vendas
        let motor = rpc(&st, "dados.consultar", json!({ "fonte": "vendas", "agrupar": ["mes"], "agregados": [{ "funcao": "contar" }, { "funcao": "soma", "campo": "total" }] })).await;
        let fixo = rpc(&st, "relatorios.vendas", json!({ "de": &hoje, "ate": &hoje, "agrupar": "mes" })).await;
        assert_eq!(motor["linhas"][0][0], fixo[0]["chave"], "{motor}");
        assert_eq!(motor["linhas"][0][2], fixo[0]["total"], "total do mês: motor {motor} × relatório {fixo}");
        assert_eq!(motor["totais"]["soma:total"], fixo[0]["total"]);
        assert_eq!(motor["linhas"][0][1], 3, "3 documentos (2 FR + 1 NC)");

        // por família, das linhas: igual ao relatório de vendas por família
        let motor = rpc(&st, "dados.consultar", json!({ "fonte": "linhas_venda", "agrupar": ["familia"], "agregados": [{ "funcao": "soma", "campo": "total" }, { "funcao": "soma", "campo": "lucro" }] })).await;
        let fixo = rpc(&st, "relatorios.vendas", json!({ "de": &hoje, "ate": &hoje, "agrupar": "familia" })).await;
        let soma_fixo: i64 = fixo.as_array().unwrap().iter().map(|l| l["total"].as_i64().unwrap()).sum();
        assert_eq!(motor["totais"]["soma:total"].as_i64(), Some(soma_fixo), "{motor} × {fixo}");
        // lucro: (2−1)×(1 000−600) + 3×(500−300) = 400 + 600 = 1 000,00 Kz
        assert_eq!(motor["totais"]["soma:lucro"].as_i64(), Some(100_000), "{motor}");

        // linhas soltas, várias ordenações, limite e total geral de todas
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "linhas_venda", "campos": ["artigo", "quantidade", "total"], "ordenar": [{ "campo": "artigo" }, { "campo": "total", "desc": true }], "limite": 2 })).await;
        assert_eq!((r["total_linhas"].as_i64(), r["truncado"].as_bool(), r["linhas"].as_array().unwrap().len()), (Some(3), Some(true), 2), "{r}");
        assert_eq!(r["linhas"][0][0], "Arroz 5kg");
        assert!(r["linhas"][0][2].as_i64().unwrap() > r["linhas"][1][2].as_i64().unwrap(), "dentro do mesmo artigo, o maior total primeiro: {r}");
        assert_eq!(r["totais"]["quantidade"].as_f64(), Some(4.0), "2 − 1 + 3: {r}");

        // filtros: texto, «em», «entre» datas e lógico
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "linhas_venda", "campos": ["artigo", "preco"], "filtros": [{ "campo": "artigo", "op": "contem", "valor": "Óleo" }] })).await;
        assert_eq!(r["total_linhas"], 1, "{r}");
        assert!(r["totais"].get("preco").is_none(), "o preço unitário não se soma: {r}");
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "linhas_venda", "campos": ["artigo"], "filtros": [{ "campo": "artigo", "op": "comeca", "valor": "arroz" }, { "campo": "data", "op": "entre", "valor": [&hoje, &hoje] }] })).await;
        assert_eq!(r["total_linhas"], 2, "{r}");
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["documento"], "filtros": [{ "campo": "tipo", "op": "em", "valor": ["NC"] }, { "campo": "importado", "op": "=", "valor": false }] })).await;
        assert_eq!(r["total_linhas"], 1, "{r}");

        // texto com SQL é só texto
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["documento"], "filtros": [{ "campo": "cliente", "op": "=", "valor": "x' OR '1'='1" }] })).await;
        assert_eq!(r["total_linhas"], 0, "{r}");
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["documento"], "filtros": [{ "campo": "cliente", "op": "contem", "valor": "%" }] })).await;
        assert_eq!(r["total_linhas"], 0, "o % procura-se como carácter: {r}");

        // só o que está no registo
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "documentos" })).await.contains("desconhecida"));
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["d.hash"] })).await.contains("não existe"));
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["total"], "filtros": [{ "campo": "total", "op": "; DROP", "valor": 1 }] })).await.contains("Operador"));
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas", "agrupar": ["mes"], "agregados": [{ "funcao": "soma", "campo": "cliente" }] })).await.contains("some"));
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["total"], "ordenar": [{ "campo": "liquido" }] })).await.contains("Só se ordena"));
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["data"], "filtros": [{ "campo": "data", "op": ">", "valor": "ontem" }] })).await.contains("Data inválida"));

        // compras: a V/Factura menos a V/Nota de Crédito, igual ao mapa de fornecedor
        let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Grossista, SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();
        let fc = rpc(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "FT A/1", "linhas": [{ "artigo_id": arroz, "quantidade": 10, "preco_unitario": 60000 }] })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "VNC", "entidade_id": forn, "doc_externo": "NC 1", "motivo": "Devolução de material danificado", "doc_origem_id": fc["documento"]["id"], "linhas": [{ "artigo_id": arroz, "quantidade": 2, "preco_unitario": 60000 }] })).await;
        let motor = rpc(&st, "dados.consultar", json!({ "fonte": "compras", "agrupar": ["fornecedor"], "agregados": [{ "funcao": "soma", "campo": "total" }, { "funcao": "soma", "campo": "iva" }] })).await;
        let fixo = rpc(&st, "relatorios.fornecedores", json!({ "de": &hoje, "ate": &hoje })).await;
        assert_eq!((&motor["linhas"][0][1], &motor["linhas"][0][2]), (&fixo["resumo"][0]["compras"], &fixo["resumo"][0]["iva"]), "{motor} × {fixo}");
        assert_eq!(motor["totais"]["soma:total"], 547_200, "8 × 600,00 + IVA 14 %: {motor}");

        // stock: a existência da ficha do artigo e o valor ao custo médio
        let ficha = rpc(&st, "artigos.obter", json!({ "id": arroz })).await;
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "stock", "agrupar": ["artigo"], "agregados": [{ "funcao": "soma", "campo": "existencia" }, { "funcao": "soma", "campo": "valor" }], "filtros": [{ "campo": "artigo", "op": "=", "valor": "Arroz 5kg" }] })).await;
        let q = ficha["stock"].as_f64().unwrap();
        assert_eq!(r["linhas"][0][1].as_f64(), Some(q), "{r} × {ficha}");
        assert_eq!(r["linhas"][0][2].as_i64(), Some((q * ficha["custo_medio"].as_f64().unwrap()).round() as i64), "{r} × {ficha}");

        // contas a receber: a factura a crédito vencida há muito fica no último escalão, com o valor em dívida
        let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente a Crédito", "nif": "5417000123" })).await["id"].as_i64().unwrap();
        let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": oleo, "quantidade": 1 }] })).await;
        sqlx::query("UPDATE documentos SET data_vencimento = '2020-01-31' WHERE id = ?").bind(ft["documento"]["id"].as_i64()).execute(&st.pool).await.unwrap();
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "conta_corrente", "agrupar": ["escalao"], "agregados": [{ "funcao": "soma", "campo": "pendente" }, { "funcao": "max", "campo": "dias_atraso" }] })).await;
        assert_eq!((r["linhas"][0][0].as_str(), r["linhas"][0][1].as_i64()), (Some("4. Mais de 90 dias"), Some(57_000)), "{r}");
        assert!(r["linhas"][0][2].as_i64().unwrap() > 90, "{r}");

        // cada fonte corre com todos os campos, e agrupada por cada campo de texto com todas as somas
        for f in super::FONTES {
            let campos: Vec<&str> = f.campos.iter().map(|c| c.id).collect();
            rpc(&st, "dados.consultar", json!({ "fonte": f.id, "campos": campos })).await;
            let mut agregados = vec![json!({ "funcao": "contar" })];
            agregados.extend(f.campos.iter().filter(|c| c.somavel).map(|c| json!({ "funcao": "soma", "campo": c.id })));
            for g in f.campos.iter().filter(|c| c.tipo == super::Tipo::Texto) {
                rpc(&st, "dados.consultar", json!({ "fonte": f.id, "agrupar": [g.id], "agregados": agregados })).await;
            }
        }

        // a tesouraria vê-se com a permissão do fluxo; as fontes listadas são só as permitidas
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "tesouraria", "agrupar": ["conta"], "agregados": [{ "funcao": "soma", "campo": "saldo" }] })).await;
        assert!(r["totais"]["soma:saldo"].as_i64().unwrap_or(0) > 0, "{r}");
        assert_eq!(rpc(&st, "dados.fontes", Value::Null).await.as_array().unwrap().len(), super::FONTES.len());
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await;
        rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
        assert!(erro(&st, "dados.consultar", json!({ "fonte": "vendas" })).await.contains("permissão"));
        assert_eq!(rpc(&st, "dados.fontes", Value::Null).await.as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// Um utilizador limitado a uma loja só vê as linhas dela, no motor e nas verificações, filtre o que filtrar; o
    /// administrador vê todas.
    #[tokio::test]
    async fn so_a_loja_do_utilizador() {
        let pasta = std::env::temp_dir().join(format!("kangest-dados-loja-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Livraria, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let lubango = rpc(&st, "tabela.guardar", json!({ "tabela": "armazens", "descricao": "Livraria do Lubango" })).await["id"].as_i64().unwrap();
        let posto_lub = rpc(&st, "posto.guardar", json!({ "descricao": "Caixa do Lubango", "armazem_id": lubango, "serie_prefixo": "L" })).await["id"].as_i64().unwrap();
        let livro = rpc(&st, "artigos.guardar", json!({ "descricao": "Livro", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": livro, "quantidade": 10, "preco_unitario": 50000 }] })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "armazem_id": lubango, "linhas": [{ "artigo_id": livro, "quantidade": 4, "preco_unitario": 50000 }] })).await;
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": livro, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
        rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "armazem_id": lubango, "linhas": [{ "artigo_id": livro, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 228000 }] })).await;
        let soma = |r: &Value, k: &str| r["totais"][k].as_f64().unwrap_or(-1.0);
        let vendas = json!({ "fonte": "vendas", "campos": ["documento", "loja", "total"] });
        let stock = json!({ "fonte": "stock", "campos": ["loja", "existencia"], "filtros": [{ "campo": "artigo", "op": "=", "valor": "Livro" }] });

        // o administrador vê as duas lojas
        assert_eq!(soma(&rpc(&st, "dados.consultar", vendas.clone()).await, "total"), 342_000.0);
        assert_eq!(soma(&rpc(&st, "dados.consultar", stock.clone()).await, "existencia"), 11.0, "9 na sede + 2 no Lubango");
        sqlx::query("UPDATE stock SET quantidade = -1 WHERE artigo_id = ?").bind(livro).execute(&st.pool).await.unwrap();
        let negativo = |r: &Value| r.as_array().unwrap().iter().find(|v| v["id"] == "stock_negativo").map(|v| v["casos"].as_i64().unwrap());
        assert_eq!(negativo(&rpc(&st, "dados.verificar", Value::Null).await), Some(2));

        // o gerente do Lubango só vê o Lubango, mesmo a pedir a outra loja
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Gerente do Lubango", "login": "gerlub", "perfil": "GESTOR", "senha": "segredo99", "mudar_senha": false, "armazem_id": lubango })).await;
        rpc(&st, "auth.logout", json!({})).await;
        st.carregar_posto(posto_lub).await.unwrap();
        rpc(&st, "auth.login", json!({ "login": "gerlub", "senha": "segredo99" })).await;
        let r = rpc(&st, "dados.consultar", vendas).await;
        assert_eq!((r["total_linhas"].as_i64(), soma(&r, "total")), (Some(1), 228_000.0), "{r}");
        assert_eq!(r["linhas"][0][1], "Livraria do Lubango", "{r}");
        let outra = rpc(&st, "dados.consultar", json!({ "fonte": "vendas", "campos": ["documento"], "filtros": [{ "campo": "loja", "op": "<>", "valor": "Livraria do Lubango" }] })).await;
        assert_eq!(outra["total_linhas"], 0, "{outra}");
        let r = rpc(&st, "dados.consultar", json!({ "fonte": "linhas_venda", "campos": ["artigo", "quantidade"] })).await;
        assert_eq!(soma(&r, "quantidade"), 2.0, "{r}");
        let r = rpc(&st, "dados.consultar", stock).await;
        assert_eq!((r["total_linhas"].as_i64(), soma(&r, "existencia")), (Some(1), -1.0), "{r}");
        assert_eq!(negativo(&rpc(&st, "dados.verificar", Value::Null).await), Some(1));
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

//! Recursos Humanos (KIGEST GC → Recursos Humanos): funcionários, alterações mensais,
//! adiantamentos, processamento salarial com INSS e IRT (Grupo A), recibos, mapas e férias.
//!
//! As taxas e a tabela de IRT estão em tabelas editáveis (rh_irt_escaloes, config rh.*).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ------------------------------------------------------------------ cálculo (função pura)
#[derive(Clone, Debug)]
pub struct Rubrica {
    pub codigo: String,
    pub descricao: String,
    pub tipo: String,
    pub incide_inss: bool,
    pub incide_irt: bool,
    pub isento_irt_ate: i64,
}

#[derive(Clone, Debug)]
pub struct Escalao {
    pub de: i64,
    pub ate: Option<i64>,
    pub parcela_fixa: i64,
    pub taxa: f64,
    pub excesso: i64,
}

#[derive(Clone, Debug)]
pub struct Parametros {
    pub inss_trabalhador: f64,
    pub inss_empregador: f64,
    pub dias_uteis: f64,
    pub horas_dia: f64,
    pub acrescimo_horas_extra: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Funcionario {
    pub salario_base: i64,
    pub fixos: Vec<(String, i64)>, // (rubrica, valor) — subsídios fixos da ficha
    pub desconta_inss: bool,
    pub desconta_irt: bool,
    pub desconto_sindicato: f64,
}

#[derive(Clone, Debug)]
pub struct Alteracao {
    pub rubrica: String,
    pub quantidade: f64,
    pub valor: i64,
    pub descricao: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinhaRecibo {
    pub rubrica: String,
    pub descricao: String,
    pub tipo: String,
    pub quantidade: Option<f64>,
    pub valor: i64,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct Recibo {
    pub linhas: Vec<LinhaRecibo>,
    pub total_remuneracoes: i64,
    pub base_inss: i64,
    pub inss_trabalhador: i64,
    pub inss_empregador: i64,
    pub base_irt: i64,
    pub irt: i64,
    pub total_descontos: i64,
    pub liquido: i64,
}

fn r(x: f64) -> i64 {
    x.round() as i64
}

/// IRT do Grupo A: parcela fixa + (matéria colectável − excesso) × taxa.
pub fn irt(base: i64, tabela: &[Escalao]) -> i64 {
    if base <= 0 {
        return 0;
    }
    tabela
        .iter()
        .find(|e| base >= e.de && e.ate.map(|a| base <= a).unwrap_or(true))
        .map(|e| if e.taxa <= 0.0 { e.parcela_fixa } else { e.parcela_fixa + r((base - e.excesso) as f64 * e.taxa / 100.0) })
        .unwrap_or(0)
}

pub fn calcular(f: &Funcionario, alts: &[Alteracao], rub: &HashMap<String, Rubrica>, p: &Parametros, tabela: &[Escalao]) -> Recibo {
    let base = f.salario_base as f64;
    let valor_dia = if p.dias_uteis > 0.0 { base / p.dias_uteis } else { 0.0 };
    let valor_hora = if p.dias_uteis * p.horas_dia > 0.0 { base / (p.dias_uteis * p.horas_dia) } else { 0.0 };
    let desc = |c: &str| rub.get(c).map(|x| x.descricao.clone()).unwrap_or_else(|| c.to_string());
    let mut linhas: Vec<LinhaRecibo> = Vec::new();

    let mut rem = |codigo: &str, valor: i64, qtd: Option<f64>, d: Option<String>| {
        if valor != 0 {
            linhas.push(LinhaRecibo { rubrica: codigo.into(), descricao: d.unwrap_or_else(|| desc(codigo)), tipo: "REMUNERACAO".into(), quantidade: qtd, valor });
        }
    };
    rem("001", f.salario_base, None, None);
    for (c, v) in &f.fixos {
        rem(c, *v, None, None);
    }
    let mut descontos: Vec<LinhaRecibo> = Vec::new();
    for a in alts {
        let tipo = rub.get(&a.rubrica).map(|x| x.tipo.as_str()).unwrap_or("REMUNERACAO");
        if tipo == "DESCONTO" {
            let valor = if a.valor > 0 {
                a.valor
            } else if a.rubrica == "101" {
                r(valor_dia * a.quantidade)
            } else {
                0
            };
            if valor > 0 {
                descontos.push(LinhaRecibo {
                    rubrica: a.rubrica.clone(),
                    descricao: a.descricao.clone().unwrap_or_else(|| desc(&a.rubrica)),
                    tipo: "DESCONTO".into(),
                    quantidade: (a.quantidade > 0.0).then_some(a.quantidade),
                    valor,
                });
            }
        } else {
            let valor = if a.valor > 0 {
                a.valor
            } else {
                match a.rubrica.as_str() {
                    "006" => r(valor_hora * a.quantidade * (1.0 + p.acrescimo_horas_extra / 100.0)),
                    // subsídio de férias/Natal: % do salário base (por omissão 50%)
                    "004" | "005" => r(base * if a.quantidade > 0.0 { a.quantidade } else { 50.0 } / 100.0),
                    _ => r(valor_dia * a.quantidade),
                }
            };
            rem(&a.rubrica, valor, (a.quantidade > 0.0).then_some(a.quantidade), a.descricao.clone());
        }
    }
    if f.desconto_sindicato > 0.0 {
        descontos.push(LinhaRecibo {
            rubrica: "103".into(),
            descricao: desc("103"),
            tipo: "DESCONTO".into(),
            quantidade: Some(f.desconto_sindicato),
            valor: r(base * f.desconto_sindicato / 100.0),
        });
    }

    let total_remuneracoes: i64 = linhas.iter().map(|l| l.valor).sum();
    let ri = |l: &LinhaRecibo| rub.get(&l.rubrica).cloned();
    let reduz_inss: i64 = descontos.iter().filter(|l| ri(l).map(|x| x.incide_inss).unwrap_or(false)).map(|l| l.valor).sum();
    let reduz_irt: i64 = descontos.iter().filter(|l| ri(l).map(|x| x.incide_irt).unwrap_or(false)).map(|l| l.valor).sum();
    let base_inss = (linhas.iter().filter(|l| ri(l).map(|x| x.incide_inss).unwrap_or(true)).map(|l| l.valor).sum::<i64>() - reduz_inss).max(0);
    let (inss_trabalhador, inss_empregador) = if f.desconta_inss {
        (r(base_inss as f64 * p.inss_trabalhador / 100.0), r(base_inss as f64 * p.inss_empregador / 100.0))
    } else {
        (0, 0)
    };
    // parte tributável de cada remuneração (subsídios de alimentação/transporte isentos até ao limite)
    let tributavel: i64 = linhas
        .iter()
        .filter_map(|l| ri(l).filter(|x| x.incide_irt).map(|x| (l.valor - x.isento_irt_ate).max(0)))
        .sum();
    let base_irt = (tributavel - reduz_irt - inss_trabalhador).max(0);
    let irt_valor = if f.desconta_irt { irt(base_irt, tabela) } else { 0 };
    let total_descontos = inss_trabalhador + irt_valor + descontos.iter().map(|l| l.valor).sum::<i64>();

    if inss_trabalhador > 0 {
        descontos.push(LinhaRecibo { rubrica: "INSS".into(), descricao: format!("Segurança Social ({}%)", p.inss_trabalhador), tipo: "DESCONTO".into(), quantidade: None, valor: inss_trabalhador });
    }
    if irt_valor > 0 {
        descontos.push(LinhaRecibo { rubrica: "IRT".into(), descricao: "IRT".into(), tipo: "DESCONTO".into(), quantidade: None, valor: irt_valor });
    }
    linhas.extend(descontos);
    Recibo {
        linhas,
        total_remuneracoes,
        base_inss,
        inss_trabalhador,
        inss_empregador,
        base_irt,
        irt: irt_valor,
        total_descontos,
        liquido: total_remuneracoes - total_descontos,
    }
}

// ------------------------------------------------------------------ leitura de parâmetros
async fn cfg_f64(p: &SqlitePool, k: &str, padrao: f64) -> AppResult<f64> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(p).await?;
    Ok(v.and_then(|s| s.replace(',', ".").parse().ok()).unwrap_or(padrao))
}

async fn parametros(p: &SqlitePool) -> AppResult<Parametros> {
    Ok(Parametros {
        inss_trabalhador: cfg_f64(p, "rh.inss_trabalhador", 3.0).await?,
        inss_empregador: cfg_f64(p, "rh.inss_empregador", 8.0).await?,
        dias_uteis: cfg_f64(p, "rh.dias_uteis", 22.0).await?,
        horas_dia: cfg_f64(p, "rh.horas_dia", 8.0).await?,
        acrescimo_horas_extra: cfg_f64(p, "rh.acrescimo_horas_extra", 50.0).await?,
    })
}

async fn rubricas(p: &SqlitePool) -> AppResult<HashMap<String, Rubrica>> {
    Ok(sqlx::query("SELECT * FROM rh_rubricas")
        .fetch_all(p)
        .await?
        .iter()
        .map(|r| {
            let x = Rubrica {
                codigo: r.get("codigo"),
                descricao: r.get("descricao"),
                tipo: r.get("tipo"),
                incide_inss: r.get::<i64, _>("incide_inss") == 1,
                incide_irt: r.get::<i64, _>("incide_irt") == 1,
                isento_irt_ate: r.get("isento_irt_ate"),
            };
            (x.codigo.clone(), x)
        })
        .collect())
}

async fn escaloes(p: &SqlitePool) -> AppResult<Vec<Escalao>> {
    Ok(sqlx::query("SELECT * FROM rh_irt_escaloes ORDER BY de")
        .fetch_all(p)
        .await?
        .iter()
        .map(|r| Escalao { de: r.get("de"), ate: r.get("ate"), parcela_fixa: r.get("parcela_fixa"), taxa: r.get("taxa"), excesso: r.get("excesso") })
        .collect())
}

const FIXOS: &[(&str, &str)] = &[
    ("002", "sub_alimentacao"),
    ("003", "sub_transporte"),
    ("008", "sub_chefia"),
    ("009", "sub_turno"),
    ("010", "abono_falhas"),
    ("011", "sub_arrendamento"),
    ("012", "sub_atavio"),
    ("013", "sub_isencao_horario"),
    ("014", "sub_deslocacao"),
];

fn funcionario_de(r: &sqlx::sqlite::SqliteRow) -> Funcionario {
    Funcionario {
        salario_base: r.get("salario_base"),
        fixos: FIXOS.iter().map(|(c, col)| (c.to_string(), r.get::<i64, _>(*col))).filter(|x| x.1 > 0).collect(),
        desconta_inss: r.get::<i64, _>("desconta_inss") == 1,
        desconta_irt: r.get::<i64, _>("desconta_irt") == 1,
        desconto_sindicato: r.get("desconto_sindicato"),
    }
}

// ------------------------------------------------------------------ tabelas e configuração
pub async fn tabelas(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let p = &st.pool;
    let cfg = |k: &'static str| async move { sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(p).await };
    Ok(json!({
        "departamentos": db::query_json(p, "SELECT * FROM rh_departamentos ORDER BY descricao").await?,
        "funcoes": db::query_json(p, "SELECT * FROM rh_funcoes ORDER BY descricao").await?,
        "categorias": db::query_json(p, "SELECT * FROM rh_categorias ORDER BY descricao").await?,
        "rubricas": db::query_json(p, "SELECT * FROM rh_rubricas ORDER BY ordem").await?,
        "escaloes": db::query_json(p, "SELECT * FROM rh_irt_escaloes ORDER BY de").await?,
        "parametros": {
            "inss_trabalhador": cfg("rh.inss_trabalhador").await?,
            "inss_empregador": cfg("rh.inss_empregador").await?,
            "dias_uteis": cfg("rh.dias_uteis").await?,
            "horas_dia": cfg("rh.horas_dia").await?,
            "acrescimo_horas_extra": cfg("rh.acrescimo_horas_extra").await?,
            "irt_independentes": cfg("rh.irt_independentes").await?,
            "tabela_confirmada": cfg("rh.tabela_confirmada").await?.as_deref() == Some("1"),
        },
    }))
}

#[derive(Deserialize)]
pub struct TabelaIn {
    tabela: String,
    id: Option<i64>,
    descricao: String,
}

pub async fn tabela_guardar(st: &AppState, t: TabelaIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let tabela = match t.tabela.as_str() {
        "departamentos" => "rh_departamentos",
        "funcoes" => "rh_funcoes",
        "categorias" => "rh_categorias",
        _ => return Err(AppError::msg("Tabela inválida.")),
    };
    if t.descricao.trim().is_empty() {
        return Err(AppError::msg("Indique a descrição."));
    }
    let id = match t.id {
        Some(id) => {
            sqlx::query(&format!("UPDATE {tabela} SET descricao = ? WHERE id = ?")).bind(t.descricao.trim()).bind(id).execute(&st.pool).await?;
            id
        }
        None => sqlx::query(&format!("INSERT INTO {tabela} (descricao) VALUES (?)")).bind(t.descricao.trim()).execute(&st.pool).await?.last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct EscalaoIn {
    de: i64,
    ate: Option<i64>,
    parcela_fixa: i64,
    taxa: f64,
    excesso: i64,
}

#[derive(Deserialize)]
pub struct RubricaIn {
    codigo: String,
    incide_inss: bool,
    incide_irt: bool,
    isento_irt_ate: i64,
}

#[derive(Deserialize)]
pub struct ConfigIn {
    inss_trabalhador: f64,
    inss_empregador: f64,
    dias_uteis: f64,
    horas_dia: f64,
    acrescimo_horas_extra: f64,
    tabela_confirmada: bool,
    #[serde(default)]
    irt_independentes: Option<f64>,
    escaloes: Vec<EscalaoIn>,
    #[serde(default)]
    rubricas: Vec<RubricaIn>,
}

pub async fn config_guardar(st: &AppState, c: ConfigIn) -> AppResult<Value> {
    st.exigir_perm("rh.configuracao")?;
    if c.escaloes.is_empty() || c.dias_uteis <= 0.0 || c.horas_dia <= 0.0 {
        return Err(AppError::msg("Parâmetros inválidos."));
    }
    let mut tx = st.pool.begin().await?;
    for (k, v) in [
        ("rh.inss_trabalhador", c.inss_trabalhador.to_string()),
        ("rh.inss_empregador", c.inss_empregador.to_string()),
        ("rh.dias_uteis", c.dias_uteis.to_string()),
        ("rh.horas_dia", c.horas_dia.to_string()),
        ("rh.acrescimo_horas_extra", c.acrescimo_horas_extra.to_string()),
        ("rh.tabela_confirmada", (c.tabela_confirmada as i64).to_string()),
    ] {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    if let Some(t) = c.irt_independentes.filter(|t| (0.0..=100.0).contains(t)) {
        let (k, v) = ("rh.irt_independentes", t.to_string());
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM rh_irt_escaloes").execute(&mut *tx).await?;
    for e in &c.escaloes {
        sqlx::query("INSERT INTO rh_irt_escaloes (de, ate, parcela_fixa, taxa, excesso) VALUES (?, ?, ?, ?, ?)")
            .bind(e.de)
            .bind(e.ate)
            .bind(e.parcela_fixa)
            .bind(e.taxa)
            .bind(e.excesso)
            .execute(&mut *tx)
            .await?;
    }
    for r in &c.rubricas {
        sqlx::query("UPDATE rh_rubricas SET incide_inss = ?, incide_irt = ?, isento_irt_ate = ? WHERE codigo = ?")
            .bind(r.incide_inss as i64)
            .bind(r.incide_irt as i64)
            .bind(r.isento_irt_ate.max(0))
            .bind(&r.codigo)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Value::Null)
}

// ------------------------------------------------------------------ funcionários
#[derive(Deserialize)]
pub struct FiltroIn {
    q: Option<String>,
    inativos: Option<bool>,
}

/// Agregado familiar de todos os funcionários (ListaAgregadosFamiliar do KIGEST).
pub async fn agregados(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let rows = sqlx::query(
        "SELECT f.numero, f.nome AS funcionario, f.ativo, a.nome, a.parentesco, a.data_nascimento
         FROM rh_agregado a JOIN funcionarios f ON f.id = a.funcionario_id
         ORDER BY f.nome, a.data_nascimento",
    )
    .fetch_all(&st.pool)
    .await?;
    Ok(crate::db::rows_json(&rows))
}

pub async fn funcionarios(st: &AppState, f: FiltroIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let rows = sqlx::query(
        "SELECT f.*, d.descricao AS departamento, fu.descricao AS funcao, c.descricao AS categoria
         FROM funcionarios f
         LEFT JOIN rh_departamentos d ON d.id = f.departamento_id
         LEFT JOIN rh_funcoes fu ON fu.id = f.funcao_id
         LEFT JOIN rh_categorias c ON c.id = f.categoria_id
         WHERE (f.nome LIKE ?1 OR IFNULL(f.numero,'') LIKE ?1 OR IFNULL(f.nif,'') LIKE ?1 OR IFNULL(f.bi_numero,'') LIKE ?1)
           AND (?2 = 1 OR f.ativo = 1)
         ORDER BY f.nome",
    )
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.inativos.unwrap_or(false) as i64)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

pub async fn funcionario(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let f = sqlx::query("SELECT * FROM funcionarios WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let agregado = sqlx::query("SELECT * FROM rh_agregado WHERE funcionario_id = ? ORDER BY id").bind(i.id).fetch_all(&st.pool).await?;
    let mut v = db::row_json(&f);
    v["agregado"] = db::rows_json(&agregado);
    Ok(v)
}

/// Campos editáveis da ficha (texto, inteiros em cêntimos e reais).
const CAMPOS_TEXTO: &[&str] = &[
    "numero", "nome", "sexo", "data_nascimento", "estado_civil", "nacionalidade", "naturalidade", "bi_numero", "bi_local_emissao", "bi_validade", "nif",
    "inss_numero", "morada", "localidade", "municipio", "provincia", "telefone", "email", "tipo_contrato", "data_admissao", "data_saida", "motivo_saida",
    "banco", "iban", "tipo_honorario", "local_trabalho",
];
const CAMPOS_INT: &[&str] = &[
    "departamento_id", "funcao_id", "categoria_id", "salario_base", "sub_alimentacao", "sub_transporte", "sub_chefia", "sub_turno", "abono_falhas",
    "sub_arrendamento", "sub_atavio", "sub_isencao_horario", "sub_deslocacao", "desconta_inss", "desconta_irt", "dependentes", "conta_empresa_id", "ativo",
    "independente", "honorario",
];

pub async fn funcionario_guardar(st: &AppState, v: Value) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let nome = v.get("nome").and_then(|x| x.as_str()).map(str::trim).unwrap_or("");
    if nome.is_empty() {
        return Err(AppError::msg("O nome do funcionário é obrigatório."));
    }
    let id = v.get("id").and_then(|x| x.as_i64());
    let mut cols: Vec<&str> = Vec::new();
    let mut vals: Vec<Value> = Vec::new();
    for c in CAMPOS_TEXTO {
        if let Some(x) = v.get(*c) {
            cols.push(c);
            vals.push(match x {
                Value::String(s) if s.trim().is_empty() => Value::Null,
                Value::String(s) => Value::String(s.trim().to_string()),
                _ => Value::Null,
            });
        }
    }
    for c in CAMPOS_INT {
        if let Some(x) = v.get(*c) {
            cols.push(c);
            vals.push(match x {
                Value::Bool(b) => json!(*b as i64),
                Value::Number(n) => json!(n.as_f64().map(|f| f.round() as i64)),
                _ => Value::Null,
            });
        }
    }
    if let Some(x) = v.get("desconto_sindicato") {
        cols.push("desconto_sindicato");
        vals.push(json!(x.as_f64().unwrap_or(0.0)));
    }
    let mut tx = st.pool.begin().await?;
    let id = match id {
        Some(id) => {
            let set = cols.iter().map(|c| format!("{c} = ?")).collect::<Vec<_>>().join(", ");
            let sql = format!("UPDATE funcionarios SET {set} WHERE id = ? RETURNING id");
            let mut q = sqlx::query_scalar::<_, i64>(&sql);
            for x in &vals {
                q = bind_json(q, x);
            }
            q.bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NaoEncontrado)?
        }
        None => {
            let marcas = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT INTO funcionarios ({}) VALUES ({marcas}) RETURNING id", cols.join(", "));
            let mut q = sqlx::query_scalar::<_, i64>(&sql);
            for x in &vals {
                q = bind_json(q, x);
            }
            q.fetch_one(&mut *tx).await?
        }
    };
    if let Some(Value::Array(ag)) = v.get("agregado") {
        sqlx::query("DELETE FROM rh_agregado WHERE funcionario_id = ?").bind(id).execute(&mut *tx).await?;
        for a in ag {
            let Some(n) = a.get("nome").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) else { continue };
            sqlx::query("INSERT INTO rh_agregado (funcionario_id, nome, parentesco, data_nascimento) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(n.trim())
                .bind(a.get("parentesco").and_then(|x| x.as_str()))
                .bind(a.get("data_nascimento").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE funcionarios SET dependentes = (SELECT COUNT(*) FROM rh_agregado WHERE funcionario_id = ?1) WHERE id = ?1").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

fn bind_json<'q>(
    q: sqlx::query::QueryScalar<'q, sqlx::Sqlite, i64, sqlx::sqlite::SqliteArguments<'q>>,
    v: &Value,
) -> sqlx::query::QueryScalar<'q, sqlx::Sqlite, i64, sqlx::sqlite::SqliteArguments<'q>> {
    match v {
        Value::Null => q.bind(Option::<String>::None),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64()),
        Value::Number(n) => q.bind(n.as_f64()),
        Value::String(s) => q.bind(s.clone()),
        Value::Bool(b) => q.bind(*b as i64),
        other => q.bind(other.to_string()),
    }
}

// ------------------------------------------------------------------ alterações mensais e adiantamentos
#[derive(Deserialize)]
pub struct MesIn {
    ano: i64,
    mes: i64,
}

pub async fn alteracoes(st: &AppState, m: MesIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let rows = sqlx::query(
        "SELECT a.*, f.nome, r.descricao AS rubrica_desc, r.tipo FROM rh_alteracoes a
         JOIN funcionarios f ON f.id = a.funcionario_id JOIN rh_rubricas r ON r.codigo = a.rubrica
         WHERE a.ano = ? AND a.mes = ? ORDER BY f.nome, r.ordem",
    )
    .bind(m.ano)
    .bind(m.mes)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct AlteracaoIn {
    funcionario_id: i64,
    ano: i64,
    mes: i64,
    rubrica: String,
    quantidade: Option<f64>,
    valor: Option<i64>,
    descricao: Option<String>,
}

pub async fn alteracao_guardar(st: &AppState, a: AlteracaoIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    if !(1..=12).contains(&a.mes) {
        return Err(AppError::msg("Mês inválido."));
    }
    if processado(&st.pool, a.ano, a.mes).await? {
        return Err(AppError::msg("Este mês já foi processado. Anule o processamento para fazer alterações."));
    }
    let existe: Option<String> = sqlx::query_scalar("SELECT codigo FROM rh_rubricas WHERE codigo = ?").bind(&a.rubrica).fetch_optional(&st.pool).await?;
    if existe.is_none() {
        return Err(AppError::msg("Rubrica inválida."));
    }
    let qtd = a.quantidade.unwrap_or(0.0);
    let valor = a.valor.unwrap_or(0);
    if qtd <= 0.0 && valor <= 0 && !matches!(a.rubrica.as_str(), "004" | "005") {
        return Err(AppError::msg("Indique a quantidade ou o valor."));
    }
    let id = sqlx::query("INSERT INTO rh_alteracoes (funcionario_id, ano, mes, rubrica, quantidade, valor, descricao, data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(a.funcionario_id)
        .bind(a.ano)
        .bind(a.mes)
        .bind(&a.rubrica)
        .bind(qtd)
        .bind(valor)
        .bind(a.descricao.as_deref().map(str::trim).filter(|s| !s.is_empty()))
        .bind(db::hoje())
        .execute(&st.pool)
        .await?
        .last_insert_rowid();
    Ok(json!({ "id": id }))
}

pub async fn alteracao_eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let n = sqlx::query("DELETE FROM rh_alteracoes WHERE id = ? AND processado = 0").bind(i.id).execute(&st.pool).await?.rows_affected();
    if n == 0 {
        return Err(AppError::msg("Não é possível eliminar uma alteração já processada."));
    }
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct AdiantamentoIn {
    funcionario_id: i64,
    valor: i64,
    parcelas: i64,
    ano: i64,
    mes: i64,
    descricao: Option<String>,
    /// Conta de tesouraria de onde sai o dinheiro (opcional).
    conta_id: Option<i64>,
}

/// Adiantamento de salário: gera as parcelas a descontar nos meses seguintes.
pub async fn adiantamento(st: &AppState, a: AdiantamentoIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    if a.valor <= 0 || a.parcelas < 1 || a.parcelas > 24 || !(1..=12).contains(&a.mes) {
        return Err(AppError::msg("Indique o valor, o número de parcelas (1 a 24) e o mês inicial."));
    }
    let nome: String = sqlx::query_scalar("SELECT nome FROM funcionarios WHERE id = ?").bind(a.funcionario_id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let mut tx = st.pool.begin().await?;
    let id = sqlx::query("INSERT INTO rh_adiantamentos (funcionario_id, data, valor, parcelas, ano_inicio, mes_inicio, descricao, conta_id, utilizador_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(a.funcionario_id)
        .bind(db::hoje())
        .bind(a.valor)
        .bind(a.parcelas)
        .bind(a.ano)
        .bind(a.mes)
        .bind(&a.descricao)
        .bind(a.conta_id)
        .bind(s.id)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    let parcela = a.valor / a.parcelas;
    for k in 0..a.parcelas {
        let m0 = a.mes - 1 + k;
        let (ano, mes) = (a.ano + m0 / 12, m0 % 12 + 1);
        let valor = if k == a.parcelas - 1 { a.valor - parcela * (a.parcelas - 1) } else { parcela };
        sqlx::query("INSERT INTO rh_alteracoes (funcionario_id, ano, mes, rubrica, quantidade, valor, descricao, data, adiantamento_id) VALUES (?, ?, ?, '102', 0, ?, ?, ?, ?)")
            .bind(a.funcionario_id)
            .bind(ano)
            .bind(mes)
            .bind(valor)
            .bind(format!("Adiantamento — parcela {}/{}", k + 1, a.parcelas))
            .bind(db::hoje())
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(conta) = a.conta_id {
        sqlx::query("INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, utilizador_id) VALUES (?, ?, ?, 'SAIDA', ?, ?, ?)")
            .bind(db::hoje())
            .bind(db::agora())
            .bind(conta)
            .bind(a.valor)
            .bind(format!("Adiantamento salarial — {nome}"))
            .bind(s.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

// ------------------------------------------------------------------ processamento
async fn processado(p: &SqlitePool, ano: i64, mes: i64) -> AppResult<bool> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_processamentos WHERE ano = ? AND mes = ? AND estado = 'PROCESSADO'").bind(ano).bind(mes).fetch_one(p).await?;
    Ok(n > 0)
}

async fn calcular_mes(p: &SqlitePool, ano: i64, mes: i64) -> AppResult<Vec<(sqlx::sqlite::SqliteRow, Recibo)>> {
    let prm = parametros(p).await?;
    let rub = rubricas(p).await?;
    let tab = escaloes(p).await?;
    let ultimo_dia = format!("{ano:04}-{mes:02}-31");
    let primeiro_dia = format!("{ano:04}-{mes:02}-01");
    let funcs = sqlx::query(
        "SELECT f.*, d.descricao AS departamento, fu.descricao AS funcao, c.descricao AS categoria FROM funcionarios f
         LEFT JOIN rh_departamentos d ON d.id = f.departamento_id LEFT JOIN rh_funcoes fu ON fu.id = f.funcao_id
         LEFT JOIN rh_categorias c ON c.id = f.categoria_id
         WHERE f.ativo = 1 AND f.independente = 0 AND (f.data_admissao IS NULL OR f.data_admissao <= ?1) AND (f.data_saida IS NULL OR f.data_saida >= ?2)
           AND NOT EXISTS (SELECT 1 FROM rh_prestacoes pr WHERE pr.funcionario_id = f.id AND pr.suspensao = 1
                           AND pr.data_inicio <= ?2 AND pr.data_fim >= ?1)
         ORDER BY f.nome",
    )
    .bind(&ultimo_dia)
    .bind(&primeiro_dia)
    .fetch_all(p)
    .await?;
    let alts = sqlx::query("SELECT * FROM rh_alteracoes WHERE ano = ? AND mes = ? AND processado = 0 ORDER BY id").bind(ano).bind(mes).fetch_all(p).await?;
    let mut por_func: HashMap<i64, Vec<Alteracao>> = HashMap::new();
    for a in &alts {
        por_func.entry(a.get("funcionario_id")).or_default().push(Alteracao {
            rubrica: a.get("rubrica"),
            quantidade: a.get("quantidade"),
            valor: a.get("valor"),
            descricao: a.get("descricao"),
        });
    }
    Ok(funcs
        .into_iter()
        .map(|f| {
            let id: i64 = f.get("id");
            let rec = calcular(&funcionario_de(&f), por_func.get(&id).map(|v| v.as_slice()).unwrap_or(&[]), &rub, &prm, &tab);
            (f, rec)
        })
        .collect())
}

fn resumo_json(f: &sqlx::sqlite::SqliteRow, r: &Recibo) -> Value {
    json!({
        "funcionario_id": f.get::<i64, _>("id"),
        "nome": f.get::<String, _>("nome"),
        "departamento": f.get::<Option<String>, _>("departamento"),
        "funcao": f.get::<Option<String>, _>("funcao"),
        "salario_base": f.get::<i64, _>("salario_base"),
        "total_remuneracoes": r.total_remuneracoes,
        "base_inss": r.base_inss,
        "inss_trabalhador": r.inss_trabalhador,
        "inss_empregador": r.inss_empregador,
        "base_irt": r.base_irt,
        "irt": r.irt,
        "total_descontos": r.total_descontos,
        "liquido": r.liquido,
        "linhas": r.linhas,
    })
}

/// Pré-visualização do processamento (não grava).
pub async fn simular(st: &AppState, m: MesIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let calc = calcular_mes(&st.pool, m.ano, m.mes).await?;
    Ok(Value::Array(calc.iter().map(|(f, r)| resumo_json(f, r)).collect()))
}

pub async fn processar(st: &AppState, m: MesIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    if !(1..=12).contains(&m.mes) {
        return Err(AppError::msg("Mês inválido."));
    }
    if processado(&st.pool, m.ano, m.mes).await? {
        return Err(AppError::msg("Este mês já foi processado."));
    }
    let calc = calcular_mes(&st.pool, m.ano, m.mes).await?;
    if calc.is_empty() {
        return Err(AppError::msg("Não há funcionários activos para processar."));
    }
    let total: i64 = calc.iter().map(|(_, r)| r.liquido).sum();
    let mut tx = st.pool.begin().await?;
    let pid = sqlx::query("INSERT INTO rh_processamentos (ano, mes, data, utilizador_id, total_liquido) VALUES (?, ?, ?, ?, ?)")
        .bind(m.ano)
        .bind(m.mes)
        .bind(db::agora())
        .bind(s.id)
        .bind(total)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    for (f, r) in &calc {
        let rid = sqlx::query(
            "INSERT INTO rh_recibos (processamento_id, funcionario_id, numero, nome, nif, inss_numero, departamento, funcao, categoria, provincia, municipio,
                banco, iban, salario_base, total_remuneracoes, base_inss, inss_trabalhador, inss_empregador, base_irt, irt, total_descontos, liquido)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(pid)
        .bind(f.get::<i64, _>("id"))
        .bind(f.get::<Option<String>, _>("numero"))
        .bind(f.get::<String, _>("nome"))
        .bind(f.get::<Option<String>, _>("nif"))
        .bind(f.get::<Option<String>, _>("inss_numero"))
        .bind(f.get::<Option<String>, _>("departamento"))
        .bind(f.get::<Option<String>, _>("funcao"))
        .bind(f.get::<Option<String>, _>("categoria"))
        .bind(f.get::<Option<String>, _>("provincia"))
        .bind(f.get::<Option<String>, _>("municipio"))
        .bind(f.get::<Option<String>, _>("banco"))
        .bind(f.get::<Option<String>, _>("iban"))
        .bind(f.get::<i64, _>("salario_base"))
        .bind(r.total_remuneracoes)
        .bind(r.base_inss)
        .bind(r.inss_trabalhador)
        .bind(r.inss_empregador)
        .bind(r.base_irt)
        .bind(r.irt)
        .bind(r.total_descontos)
        .bind(r.liquido)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        for l in &r.linhas {
            sqlx::query("INSERT INTO rh_recibo_linhas (recibo_id, rubrica, descricao, tipo, quantidade, valor) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(rid)
                .bind(&l.rubrica)
                .bind(&l.descricao)
                .bind(&l.tipo)
                .bind(l.quantidade)
                .bind(l.valor)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query("UPDATE rh_alteracoes SET processado = 1 WHERE ano = ? AND mes = ? AND processado = 0").bind(m.ano).bind(m.mes).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'RH_PROCESSAR', ?)").bind(s.id).bind(format!("{:02}/{}", m.mes, m.ano)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({ "id": pid, "funcionarios": calc.len(), "total_liquido": total }))
}

pub async fn processamentos(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    db::query_json(
        &st.pool,
        "SELECT p.*, u.nome AS utilizador, (SELECT COUNT(*) FROM rh_recibos r WHERE r.processamento_id = p.id) AS recibos,
                (SELECT COALESCE(SUM(irt), 0) FROM rh_recibos r WHERE r.processamento_id = p.id) AS total_irt,
                (SELECT COALESCE(SUM(inss_trabalhador + inss_empregador), 0) FROM rh_recibos r WHERE r.processamento_id = p.id) AS total_inss
         FROM rh_processamentos p JOIN utilizadores u ON u.id = p.utilizador_id ORDER BY p.ano DESC, p.mes DESC, p.id DESC",
    )
    .await
}

pub async fn recibos(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let p = sqlx::query("SELECT * FROM rh_processamentos WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    let recs = sqlx::query("SELECT * FROM rh_recibos WHERE processamento_id = ? ORDER BY nome").bind(i.id).fetch_all(&st.pool).await?;
    let linhas = sqlx::query("SELECT l.* FROM rh_recibo_linhas l JOIN rh_recibos r ON r.id = l.recibo_id WHERE r.processamento_id = ? ORDER BY l.recibo_id, l.id")
        .bind(i.id)
        .fetch_all(&st.pool)
        .await?;
    let mut por: HashMap<i64, Vec<Value>> = HashMap::new();
    for l in &linhas {
        por.entry(l.get("recibo_id")).or_default().push(db::row_json(l));
    }
    let lista: Vec<Value> = recs
        .iter()
        .map(|r| {
            let mut v = db::row_json(r);
            v["linhas"] = Value::Array(por.remove(&r.get::<i64, _>("id")).unwrap_or_default());
            v
        })
        .collect();
    Ok(json!({ "processamento": db::row_json(&p), "recibos": lista }))
}

#[derive(Deserialize)]
pub struct AnularIn {
    id: i64,
    motivo: String,
}

pub async fn anular(st: &AppState, a: AnularIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    if a.motivo.trim().len() < 3 {
        return Err(AppError::msg("Indique o motivo."));
    }
    let row = sqlx::query("SELECT ano, mes, estado, pago FROM rh_processamentos WHERE id = ?").bind(a.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    if row.get::<String, _>("estado") != "PROCESSADO" {
        return Err(AppError::msg("O processamento já está anulado."));
    }
    if row.get::<i64, _>("pago") == 1 {
        return Err(AppError::msg("Os salários deste mês já foram pagos. Anule primeiro o movimento de pagamento na tesouraria."));
    }
    let (ano, mes): (i64, i64) = (row.get("ano"), row.get("mes"));
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE rh_processamentos SET estado = 'ANULADO', motivo_anulacao = ? WHERE id = ?").bind(a.motivo.trim()).bind(a.id).execute(&mut *tx).await?;
    sqlx::query("UPDATE rh_alteracoes SET processado = 0 WHERE ano = ? AND mes = ?").bind(ano).bind(mes).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'RH_ANULAR', ?)").bind(s.id).bind(format!("{mes:02}/{ano}: {}", a.motivo.trim())).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub struct PagarIn {
    id: i64,
    conta_id: i64,
}

/// Pagamento dos salários: saída na tesouraria pelo total líquido.
pub async fn pagar(st: &AppState, pg: PagarIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    let row = sqlx::query("SELECT ano, mes, estado, pago, total_liquido FROM rh_processamentos WHERE id = ?").bind(pg.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    if row.get::<String, _>("estado") != "PROCESSADO" || row.get::<i64, _>("pago") == 1 {
        return Err(AppError::msg("Este processamento não pode ser pago (anulado ou já pago)."));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, centro_custo_id, documento, utilizador_id)
                 VALUES (?, ?, ?, 'SAIDA', ?, ?, (SELECT id FROM centros_custo WHERE descricao = 'Salários' LIMIT 1), ?, ?)")
        .bind(db::hoje())
        .bind(db::agora())
        .bind(pg.conta_id)
        .bind(row.get::<i64, _>("total_liquido"))
        .bind(format!("Salários {:02}/{}", row.get::<i64, _>("mes"), row.get::<i64, _>("ano")))
        .bind(format!("RH-{}", pg.id))
        .bind(s.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE rh_processamentos SET pago = 1 WHERE id = ?").bind(pg.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

/// Mapa de Remunerações (declaração mensal de IRT / INSS): uma linha por trabalhador.
pub async fn mapa(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let rub = rubricas(&st.pool).await?;
    let mapa_col: HashMap<String, String> = sqlx::query("SELECT codigo, mapa FROM rh_rubricas WHERE mapa IS NOT NULL")
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| (r.get::<String, _>("codigo"), r.get::<String, _>("mapa")))
        .collect();
    let dados = recibos(st, i).await?;
    let linhas: Vec<Value> = dados["recibos"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|rec| {
            let mut cols: HashMap<String, i64> = HashMap::new();
            let mut excesso = 0;
            for l in rec["linhas"].as_array().cloned().unwrap_or_default() {
                let cod = l["rubrica"].as_str().unwrap_or("").to_string();
                let valor = l["valor"].as_i64().unwrap_or(0);
                if let Some(c) = mapa_col.get(&cod) {
                    *cols.entry(c.clone()).or_default() += valor;
                }
                if let Some(rb) = rub.get(&cod) {
                    if rb.isento_irt_ate > 0 && rb.incide_irt {
                        excesso += (valor - rb.isento_irt_ate).max(0);
                    }
                }
            }
            json!({
                "nome": rec["nome"], "nif": rec["nif"], "inss_numero": rec["inss_numero"], "provincia": rec["provincia"], "municipio": rec["municipio"],
                "banco": rec["banco"], "iban": rec["iban"], "colunas": cols, "excesso_subsidios": excesso,
                "salario_iliquido": rec["total_remuneracoes"], "base_inss": rec["base_inss"], "inss": rec["inss_trabalhador"],
                "inss_empregador": rec["inss_empregador"], "base_irt": rec["base_irt"], "irt": rec["irt"], "liquido": rec["liquido"],
            })
        })
        .collect();
    Ok(json!({ "processamento": dados["processamento"], "linhas": linhas }))
}

// ------------------------------------------------------------------ férias
#[derive(Deserialize)]
pub struct AnoIn {
    ano: i64,
}

pub async fn ferias(st: &AppState, a: AnoIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    let rows = sqlx::query(
        "SELECT fe.*, f.nome FROM rh_ferias fe JOIN funcionarios f ON f.id = fe.funcionario_id WHERE fe.ano = ? ORDER BY fe.data_inicio",
    )
    .bind(a.ano)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct FeriasIn {
    id: Option<i64>,
    funcionario_id: i64,
    data_inicio: String,
    data_fim: String,
    dias: i64,
    estado: Option<String>,
    observacoes: Option<String>,
}

pub async fn ferias_guardar(st: &AppState, f: FeriasIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    if f.data_inicio.len() != 10 || f.data_fim < f.data_inicio || f.dias <= 0 {
        return Err(AppError::msg("Indique as datas e o número de dias de férias."));
    }
    let ano: i64 = f.data_inicio[..4].parse().unwrap_or(0);
    let estado = f.estado.unwrap_or_else(|| "MARCADA".into());
    let id = match f.id {
        Some(id) => {
            sqlx::query("UPDATE rh_ferias SET funcionario_id = ?, ano = ?, data_inicio = ?, data_fim = ?, dias = ?, estado = ?, observacoes = ? WHERE id = ?")
                .bind(f.funcionario_id)
                .bind(ano)
                .bind(&f.data_inicio)
                .bind(&f.data_fim)
                .bind(f.dias)
                .bind(&estado)
                .bind(&f.observacoes)
                .bind(id)
                .execute(&st.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO rh_ferias (funcionario_id, ano, data_inicio, data_fim, dias, estado, observacoes) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(f.funcionario_id)
            .bind(ano)
            .bind(&f.data_inicio)
            .bind(&f.data_fim)
            .bind(f.dias)
            .bind(&estado)
            .bind(&f.observacoes)
            .execute(&st.pool)
            .await?
            .last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tabela() -> Vec<Escalao> {
        let e = |de, ate: Option<i64>, pf, taxa, exc| Escalao { de, ate, parcela_fixa: pf, taxa, excesso: exc };
        vec![
            e(0, Some(15000000), 0, 0.0, 0),
            e(15000001, Some(20000000), 1250000, 16.0, 15000000),
            e(20000001, Some(30000000), 3125000, 18.0, 20000000),
            e(30000001, Some(50000000), 4925000, 19.0, 30000000),
            e(50000001, Some(100000000), 8725000, 20.0, 50000000),
        ]
    }

    fn rubs() -> HashMap<String, Rubrica> {
        let r = |c: &str, t: &str, inss: bool, irt: bool, isento: i64| {
            (c.to_string(), Rubrica { codigo: c.into(), descricao: c.into(), tipo: t.into(), incide_inss: inss, incide_irt: irt, isento_irt_ate: isento })
        };
        [
            r("001", "REMUNERACAO", true, true, 0),
            r("002", "REMUNERACAO", true, true, 3000000),
            r("003", "REMUNERACAO", true, true, 3000000),
            r("004", "REMUNERACAO", true, true, 0),
            r("006", "REMUNERACAO", true, true, 0),
            r("101", "DESCONTO", true, true, 0),
            r("102", "DESCONTO", false, false, 0),
        ]
        .into_iter()
        .collect()
    }

    fn prm() -> Parametros {
        Parametros { inss_trabalhador: 3.0, inss_empregador: 8.0, dias_uteis: 22.0, horas_dia: 8.0, acrescimo_horas_extra: 50.0 }
    }

    #[test]
    fn irt_por_escaloes() {
        let t = tabela();
        assert_eq!(irt(15000000, &t), 0); // 150 000 Kz isento
        // 180 000 Kz → 12 500 + 16% × 30 000 = 17 300 Kz
        assert_eq!(irt(18000000, &t), 1730000);
        // 400 000 Kz → 49 250 + 19% × 100 000 = 68 250 Kz
        assert_eq!(irt(40000000, &t), 6825000);
    }

    #[test]
    fn recibo_completo() {
        // salário 300 000, alimentação 40 000 (10 000 tributáveis), transporte 20 000 (isento)
        let f = Funcionario {
            salario_base: 30000000,
            fixos: vec![("002".into(), 4000000), ("003".into(), 2000000)],
            desconta_inss: true,
            desconta_irt: true,
            desconto_sindicato: 0.0,
        };
        let alts = vec![
            Alteracao { rubrica: "101".into(), quantidade: 2.0, valor: 0, descricao: None },   // 2 faltas
            Alteracao { rubrica: "006".into(), quantidade: 4.0, valor: 0, descricao: None },   // 4 horas extra
            Alteracao { rubrica: "102".into(), quantidade: 0.0, valor: 5000000, descricao: None }, // adiantamento 50 000
        ];
        let r = calcular(&f, &alts, &rubs(), &prm(), &tabela());
        let dia: f64 = 30000000.0 / 22.0; // 13 636,36
        let faltas = (dia * 2.0).round() as i64; // 2 727 273
        let hora: f64 = 30000000.0 / (22.0 * 8.0);
        let he = (hora * 4.0 * 1.5).round() as i64; // 1 022 727
        assert_eq!(r.total_remuneracoes, 30000000 + 4000000 + 2000000 + he);
        assert_eq!(r.base_inss, 36000000 + he - faltas);
        let inss = (r.base_inss as f64 * 0.03).round() as i64;
        assert_eq!(r.inss_trabalhador, inss);
        assert_eq!(r.inss_empregador, (r.base_inss as f64 * 0.08).round() as i64);
        // base IRT = salário + excesso de alimentação (10 000) + horas extra − faltas − INSS
        assert_eq!(r.base_irt, 30000000 + 1000000 + he - faltas - inss);
        assert_eq!(r.irt, irt(r.base_irt, &tabela()));
        assert_eq!(r.total_descontos, faltas + 5000000 + inss + r.irt);
        assert_eq!(r.liquido, r.total_remuneracoes - r.total_descontos);
    }
}

/// Vendedores para os documentos de venda (só exige sessão): funcionários activos.
pub async fn vendedores(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    db::query_json(&st.pool, "SELECT id, nome, numero FROM funcionarios WHERE ativo = 1 AND independente = 0 ORDER BY nome").await
}


// ------------------------------------------------------------------ honorários (trabalhadores por conta própria, IRT Grupo B)
#[derive(Deserialize)]
pub struct HonorarioIn {
    funcionario_id: i64,
    ano: i64,
    mes: i64,
    quantidade: f64,
    #[serde(default)]
    valor_base: Option<i64>,
    #[serde(default)]
    descricao: Option<String>,
}

/// Cálculo do honorário: ilíquido = base × quantidade (MENSAL usa quantidade 1), IRT retido à taxa indicada.
pub fn calcular_honorario(tipo: &str, valor_base: i64, quantidade: f64, irt_pct: f64) -> (i64, i64, i64) {
    let q = if tipo == "MENSAL" && quantidade <= 0.0 { 1.0 } else { quantidade };
    let iliquido = r(valor_base as f64 * q);
    let irt = r(iliquido as f64 * irt_pct / 100.0);
    (iliquido, irt, iliquido - irt)
}

pub async fn honorarios(st: &AppState, m: MesIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let regs = db::rows_json(
        &sqlx::query(
            "SELECT h.*, f.nome, f.nif, f.numero FROM rh_honorarios h JOIN funcionarios f ON f.id = h.funcionario_id
             WHERE h.ano = ? AND h.mes = ? ORDER BY h.id",
        )
        .bind(m.ano)
        .bind(m.mes)
        .fetch_all(&st.pool)
        .await?,
    );
    let independentes = db::query_json(
        &st.pool,
        "SELECT id, nome, numero, nif, honorario, tipo_honorario FROM funcionarios WHERE ativo = 1 AND independente = 1 ORDER BY nome",
    )
    .await?;
    let taxa = cfg_f64(&st.pool, "rh.irt_independentes", 6.5).await?;
    Ok(json!({ "honorarios": regs, "independentes": independentes, "irt_pct": taxa }))
}

pub async fn honorario_processar(st: &AppState, h: HonorarioIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    if !(1..=12).contains(&h.mes) || h.quantidade < 0.0 {
        return Err(AppError::msg("Indique o mês e a quantidade."));
    }
    let f = sqlx::query("SELECT independente, honorario, tipo_honorario, nome FROM funcionarios WHERE id = ?")
        .bind(h.funcionario_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if f.get::<i64, _>("independente") != 1 {
        return Err(AppError::msg("Os honorários só se processam para trabalhadores por conta própria."));
    }
    let tipo: String = f.get("tipo_honorario");
    let base = h.valor_base.unwrap_or_else(|| f.get("honorario"));
    if base <= 0 {
        return Err(AppError::msg("Indique o valor do honorário na ficha do trabalhador ou no lançamento."));
    }
    if tipo != "MENSAL" && h.quantidade <= 0.0 {
        return Err(AppError::msg("Indique o número de dias ou de horas."));
    }
    if tipo == "MENSAL" {
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_honorarios WHERE funcionario_id = ? AND ano = ? AND mes = ? AND tipo = 'MENSAL' AND estado = 'PROCESSADO'")
            .bind(h.funcionario_id)
            .bind(h.ano)
            .bind(h.mes)
            .fetch_one(&st.pool)
            .await?;
        if ja > 0 {
            return Err(AppError::msg("O honorário mensal deste trabalhador já foi processado neste mês."));
        }
    }
    let taxa = cfg_f64(&st.pool, "rh.irt_independentes", 6.5).await?;
    let (iliquido, irt, liquido) = calcular_honorario(&tipo, base, h.quantidade, taxa);
    let q = if tipo == "MENSAL" && h.quantidade <= 0.0 { 1.0 } else { h.quantidade };
    let id = sqlx::query(
        "INSERT INTO rh_honorarios (funcionario_id, data, ano, mes, tipo, valor_base, quantidade, iliquido, irt_pct, irt, liquido, descricao, utilizador_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(h.funcionario_id)
    .bind(db::hoje())
    .bind(h.ano)
    .bind(h.mes)
    .bind(&tipo)
    .bind(base)
    .bind(q)
    .bind(iliquido)
    .bind(taxa)
    .bind(irt)
    .bind(liquido)
    .bind(h.descricao.as_deref().map(str::trim).filter(|x| !x.is_empty()))
    .bind(s.id)
    .execute(&st.pool)
    .await?
    .last_insert_rowid();
    Ok(json!({ "id": id, "iliquido": iliquido, "irt": irt, "liquido": liquido }))
}

pub async fn honorario_anular(st: &AppState, a: AnularIn) -> AppResult<Value> {
    st.exigir_perm("rh.processamento")?;
    let row = sqlx::query("SELECT estado, pago FROM rh_honorarios WHERE id = ?").bind(a.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    if row.get::<String, _>("estado") != "PROCESSADO" {
        return Err(AppError::msg("O honorário já está anulado."));
    }
    if row.get::<i64, _>("pago") == 1 {
        return Err(AppError::msg("O honorário já foi pago: anule primeiro o pagamento no Fluxo de Caixa."));
    }
    if a.motivo.trim().len() < 3 {
        return Err(AppError::msg("Indique o motivo da anulação."));
    }
    sqlx::query("UPDATE rh_honorarios SET estado = 'ANULADO', motivo_anulacao = ? WHERE id = ?").bind(a.motivo.trim()).bind(a.id).execute(&st.pool).await?;
    Ok(Value::Null)
}

pub async fn honorario_pagar(st: &AppState, pg: PagarIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.processamento")?;
    let row = sqlx::query("SELECT h.estado, h.pago, h.liquido, h.mes, h.ano, f.nome FROM rh_honorarios h JOIN funcionarios f ON f.id = h.funcionario_id WHERE h.id = ?")
        .bind(pg.id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    if row.get::<String, _>("estado") != "PROCESSADO" || row.get::<i64, _>("pago") == 1 {
        return Err(AppError::msg("Este honorário não pode ser pago (anulado ou já pago)."));
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, centro_custo_id, documento, utilizador_id)
                 VALUES (?, ?, ?, 'SAIDA', ?, ?, (SELECT id FROM centros_custo WHERE descricao = 'Salários' LIMIT 1), ?, ?)")
        .bind(db::hoje())
        .bind(db::agora())
        .bind(pg.conta_id)
        .bind(row.get::<i64, _>("liquido"))
        .bind(format!("Honorários {:02}/{} — {}", row.get::<i64, _>("mes"), row.get::<i64, _>("ano"), row.get::<String, _>("nome")))
        .bind(format!("HON-{}", pg.id))
        .bind(s.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE rh_honorarios SET pago = 1 WHERE id = ?").bind(pg.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Value::Null)
}

// ------------------------------------------------------------------ prestações sociais
#[derive(Deserialize)]
pub struct PrestacaoIn {
    #[serde(default)]
    id: Option<i64>,
    funcionario_id: i64,
    tipo: String,
    data_inicio: String,
    data_fim: String,
    #[serde(default)]
    liquido_mensal: i64,
    #[serde(default = "um")]
    meses: i64,
    #[serde(default = "verdadeiro")]
    suspensao: bool,
    #[serde(default)]
    observacoes: Option<String>,
}
fn um() -> i64 {
    1
}
fn verdadeiro() -> bool {
    true
}

pub async fn prestacoes(st: &AppState, a: AnoIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    Ok(db::rows_json(
        &sqlx::query(
            "SELECT p.*, f.nome, f.numero FROM rh_prestacoes p JOIN funcionarios f ON f.id = p.funcionario_id
             WHERE substr(p.data_inicio, 1, 4) <= ?1 AND substr(p.data_fim, 1, 4) >= ?1 ORDER BY p.data_inicio DESC",
        )
        .bind(a.ano.to_string())
        .fetch_all(&st.pool)
        .await?,
    ))
}

pub async fn prestacao_guardar(st: &AppState, p: PrestacaoIn) -> AppResult<Value> {
    let s = st.exigir_perm("rh.funcionarios")?;
    if p.tipo.trim().is_empty() || p.data_inicio.len() != 10 || p.data_fim.len() != 10 || p.data_fim < p.data_inicio {
        return Err(AppError::msg("Indique o tipo e um período válido (a data final não pode ser anterior à inicial)."));
    }
    let obs = p.observacoes.as_deref().map(str::trim).filter(|x| !x.is_empty());
    let id = match p.id {
        Some(id) => sqlx::query_scalar::<_, i64>(
            "UPDATE rh_prestacoes SET funcionario_id = ?, tipo = ?, data_inicio = ?, data_fim = ?, liquido_mensal = ?, meses = ?, suspensao = ?, observacoes = ? WHERE id = ? RETURNING id",
        )
        .bind(p.funcionario_id)
        .bind(p.tipo.trim())
        .bind(&p.data_inicio)
        .bind(&p.data_fim)
        .bind(p.liquido_mensal.max(0))
        .bind(p.meses.max(1))
        .bind(p.suspensao as i64)
        .bind(obs)
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?,
        None => sqlx::query(
            "INSERT INTO rh_prestacoes (funcionario_id, tipo, data_inicio, data_fim, liquido_mensal, meses, suspensao, observacoes, data, utilizador_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(p.funcionario_id)
        .bind(p.tipo.trim())
        .bind(&p.data_inicio)
        .bind(&p.data_fim)
        .bind(p.liquido_mensal.max(0))
        .bind(p.meses.max(1))
        .bind(p.suspensao as i64)
        .bind(obs)
        .bind(db::hoje())
        .bind(s.id)
        .execute(&st.pool)
        .await?
        .last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

pub async fn prestacao_eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("rh.funcionarios")?;
    sqlx::query("DELETE FROM rh_prestacoes WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(Value::Null)
}

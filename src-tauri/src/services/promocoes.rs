//! Promoções por regras e cupões de desconto (como os motores de promoções dos POS de referência).
//!
//! - **Leve N pague M** (ex.: leve 3 pague 2) num artigo, numa família ou em todos: as unidades
//!   oferecidas viram desconto % na linha.
//! - **Por quantidade**: desconto % a partir de X unidades do artigo (ou da família).
//! - **Por horário** («happy hour»): desconto % entre horas, nos dias da semana escolhidos.
//! - **Cupões**: código com desconto % ou em Kz na venda toda, com validade, compra mínima e
//!   limite de utilizações.
//!
//! O servidor é quem decide: `documentos.emitir` aplica estas regras como desconto por omissão e,
//! até esse valor, não pede autorização; o POS mostra-as pelo `promocoes.calcular`.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDateTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Uma linha do carrinho, vista pelas regras.
pub struct LinhaPromo {
    pub artigo_id: i64,
    pub familia_id: Option<i64>,
    pub quantidade: f64,
}

/// Melhor desconto das regras para cada linha: (%, descrição da regra).
pub async fn descontos(c: &mut SqliteConnection, linhas: &[LinhaPromo], agora: NaiveDateTime) -> AppResult<Vec<(f64, Option<String>)>> {
    let hoje = agora.format("%Y-%m-%d").to_string();
    let hora = agora.format("%H:%M").to_string();
    let dia = agora.weekday().number_from_monday().to_string();
    let regras = sqlx::query(
        "SELECT * FROM promocao_regras WHERE ativo = 1
           AND (data_inicio IS NULL OR data_inicio = '' OR data_inicio <= ?1)
           AND (data_fim IS NULL OR data_fim = '' OR data_fim >= ?1)",
    )
    .bind(&hoje)
    .fetch_all(&mut *c)
    .await?;
    // quantidades por artigo e por família (somando as linhas)
    let mut por_artigo: HashMap<i64, f64> = HashMap::new();
    let mut por_familia: HashMap<i64, f64> = HashMap::new();
    for l in linhas {
        *por_artigo.entry(l.artigo_id).or_default() += l.quantidade;
        if let Some(f) = l.familia_id {
            *por_familia.entry(f).or_default() += l.quantidade;
        }
    }
    Ok(linhas
        .iter()
        .map(|l| {
            let mut melhor: (f64, Option<String>) = (0.0, None);
            for r in &regras {
                let artigo: Option<i64> = r.get("artigo_id");
                let familia: Option<i64> = r.get("familia_id");
                // a que linhas se aplica, e a quantidade que conta
                let quantidade = match (artigo, familia) {
                    (Some(a), _) if a == l.artigo_id => por_artigo[&l.artigo_id],
                    (Some(_), _) => continue,
                    (None, Some(f)) if Some(f) == l.familia_id => por_familia[&f],
                    (None, Some(_)) => continue,
                    (None, None) => por_artigo[&l.artigo_id],
                };
                let pct = match r.get::<String, _>("tipo").as_str() {
                    "LEVE_PAGUE" => {
                        let (n, m): (f64, f64) = (r.get::<Option<f64>, _>("leve").unwrap_or(0.0), r.get::<Option<f64>, _>("pague").unwrap_or(0.0));
                        // por família, o desconto reparte-se pelas unidades do artigo (aproximação simples)
                        let q = por_artigo[&l.artigo_id];
                        if n <= m || m < 0.0 || n <= 0.0 || q <= 0.0 {
                            0.0
                        } else {
                            let ofertas = (quantidade / n).floor() * (n - m);
                            (ofertas / quantidade * 100.0).min(100.0)
                        }
                    }
                    "QUANTIDADE" => {
                        if quantidade + 1e-9 >= r.get::<Option<f64>, _>("quantidade_min").unwrap_or(f64::MAX) {
                            r.get::<Option<f64>, _>("desconto_pct").unwrap_or(0.0)
                        } else {
                            0.0
                        }
                    }
                    "HORARIO" => {
                        let ini: String = r.get::<Option<String>, _>("hora_inicio").unwrap_or_default();
                        let fim: String = r.get::<Option<String>, _>("hora_fim").unwrap_or_default();
                        let dias: String = r.get::<Option<String>, _>("dias_semana").unwrap_or_default();
                        let na_hora = !ini.is_empty() && !fim.is_empty() && hora.as_str() >= ini.as_str() && hora.as_str() < fim.as_str();
                        if na_hora && (dias.is_empty() || dias.contains(&dia)) {
                            r.get::<Option<f64>, _>("desconto_pct").unwrap_or(0.0)
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                let pct = (pct.clamp(0.0, 100.0) * 10_000.0).round() / 10_000.0;
                if pct > melhor.0 + 1e-9 {
                    melhor = (pct, Some(r.get("descricao")));
                }
            }
            melhor
        })
        .collect())
}

#[derive(Deserialize)]
pub struct CalcularIn {
    linhas: Vec<CalcLinhaIn>,
}

#[derive(Deserialize)]
pub struct CalcLinhaIn {
    artigo_id: i64,
    quantidade: f64,
}

/// POS: o desconto das regras em vigor para as linhas do carrinho.
pub async fn calcular(st: &AppState, i: CalcularIn) -> AppResult<Value> {
    st.sessao()?;
    let mut c = st.pool.acquire().await?;
    let mut linhas = Vec::with_capacity(i.linhas.len());
    for l in &i.linhas {
        let familia: Option<i64> = sqlx::query_scalar("SELECT familia_id FROM artigos WHERE id = ?").bind(l.artigo_id).fetch_optional(&mut *c).await?.flatten();
        linhas.push(LinhaPromo { artigo_id: l.artigo_id, familia_id: familia, quantidade: l.quantidade });
    }
    let r = descontos(&mut c, &linhas, chrono::Local::now().naive_local()).await?;
    Ok(Value::Array(r.into_iter().map(|(pct, regra)| json!({ "desconto": pct, "regra": regra })).collect()))
}

// ------------------------------------------------------------------ cupões

/// Cupão válido para uma compra de `total` (cêntimos, com IVA): (tipo, valor, descrição).
pub async fn cupao_valido(c: &mut SqliteConnection, codigo: &str, total: i64, hoje: &str) -> AppResult<(String, f64, Option<String>)> {
    let r = sqlx::query("SELECT * FROM cupoes WHERE codigo = ? COLLATE NOCASE")
        .bind(codigo.trim())
        .fetch_optional(&mut *c)
        .await?
        .ok_or_else(|| AppError::msg(format!("O cupão {} não existe.", codigo.trim())))?;
    if r.get::<i64, _>("ativo") == 0 {
        return Err(AppError::msg("Este cupão está desactivado."));
    }
    let ini: Option<String> = r.get("data_inicio");
    let fim: Option<String> = r.get("data_fim");
    if ini.as_deref().is_some_and(|d| !d.is_empty() && hoje < d) || fim.as_deref().is_some_and(|d| !d.is_empty() && hoje > d) {
        return Err(AppError::msg("Este cupão não está dentro da validade."));
    }
    if let Some(max) = r.get::<Option<i64>, _>("utilizacoes_max") {
        if r.get::<i64, _>("utilizacoes") >= max {
            return Err(AppError::msg("Este cupão já foi usado o número de vezes permitido."));
        }
    }
    let minimo: i64 = r.get("valor_minimo");
    if total < minimo {
        return Err(AppError::msg(format!("Este cupão só vale para compras a partir de {}.", crate::agt::total_fmt(minimo))));
    }
    Ok((r.get("tipo"), r.get("valor"), r.get("descricao")))
}

#[derive(Deserialize)]
pub struct ValidarIn {
    codigo: String,
    #[serde(default)]
    total: i64,
}

pub async fn validar(st: &AppState, v: ValidarIn) -> AppResult<Value> {
    st.sessao()?;
    let mut c = st.pool.acquire().await?;
    let (tipo, valor, descricao) = cupao_valido(&mut c, &v.codigo, v.total, &db::hoje()).await?;
    Ok(json!({ "codigo": v.codigo.trim().to_uppercase(), "tipo": tipo, "valor": valor, "descricao": descricao }))
}

// ------------------------------------------------------------------ gestão

pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(json!({
        "regras": db::query_json(&st.pool,
            "SELECT r.*, a.descricao AS artigo, f.descricao AS familia FROM promocao_regras r
             LEFT JOIN artigos a ON a.id = r.artigo_id LEFT JOIN familias f ON f.id = r.familia_id ORDER BY r.ativo DESC, r.id DESC").await?,
        "cupoes": db::query_json(&st.pool, "SELECT * FROM cupoes ORDER BY ativo DESC, id DESC").await?,
    }))
}

#[derive(Deserialize)]
pub struct RegraIn {
    id: Option<i64>,
    descricao: String,
    tipo: String,
    artigo_id: Option<i64>,
    familia_id: Option<i64>,
    leve: Option<f64>,
    pague: Option<f64>,
    quantidade_min: Option<f64>,
    desconto_pct: Option<f64>,
    hora_inicio: Option<String>,
    hora_fim: Option<String>,
    dias_semana: Option<String>,
    data_inicio: Option<String>,
    data_fim: Option<String>,
    #[serde(default = "verdade")]
    ativo: bool,
}
fn verdade() -> bool {
    true
}

fn hora_ok(h: &Option<String>) -> bool {
    h.as_deref().is_none_or(|h| h.is_empty() || chrono::NaiveTime::parse_from_str(h, "%H:%M").is_ok())
}

pub async fn regra_guardar(st: &AppState, r: RegraIn) -> AppResult<Value> {
    let s = st.exigir_perm("artigos.editar")?;
    if r.descricao.trim().is_empty() {
        return Err(AppError::msg("Dê um nome à promoção (aparece no POS)."));
    }
    let pct_ok = |p: Option<f64>| p.is_some_and(|p| p > 0.0 && p <= 100.0);
    match r.tipo.as_str() {
        "LEVE_PAGUE" => {
            let (n, m) = (r.leve.unwrap_or(0.0), r.pague.unwrap_or(-1.0));
            if !(n > m && m >= 0.0) {
                return Err(AppError::msg("Leve N pague M: N tem de ser maior do que M (ex.: leve 3 pague 2)."));
            }
        }
        "QUANTIDADE" if r.quantidade_min.unwrap_or(0.0) <= 0.0 || !pct_ok(r.desconto_pct) => {
            return Err(AppError::msg("Indique a quantidade mínima e o desconto (%)."));
        }
        "HORARIO" if !pct_ok(r.desconto_pct) || r.hora_inicio.as_deref().unwrap_or("").is_empty() || r.hora_fim.as_deref().unwrap_or("").is_empty() => {
            return Err(AppError::msg("Indique as horas (início e fim) e o desconto (%)."));
        }
        "QUANTIDADE" | "HORARIO" => {}
        _ => return Err(AppError::msg("Tipo de promoção inválido.")),
    }
    if !hora_ok(&r.hora_inicio) || !hora_ok(&r.hora_fim) {
        return Err(AppError::msg("As horas escrevem-se HH:MM (ex.: 17:00)."));
    }
    let dias: String = r.dias_semana.unwrap_or_default().chars().filter(|c| ('1'..='7').contains(c)).collect();
    let q = if r.id.is_some() {
        "UPDATE promocao_regras SET descricao=?1, tipo=?2, artigo_id=?3, familia_id=?4, leve=?5, pague=?6, quantidade_min=?7, desconto_pct=?8,
            hora_inicio=?9, hora_fim=?10, dias_semana=?11, data_inicio=?12, data_fim=?13, ativo=?14 WHERE id=?15"
    } else {
        "INSERT INTO promocao_regras (descricao, tipo, artigo_id, familia_id, leve, pague, quantidade_min, desconto_pct, hora_inicio, hora_fim,
            dias_semana, data_inicio, data_fim, ativo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
    };
    let vazio = |x: Option<String>| x.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let mut query = sqlx::query(q)
        .bind(r.descricao.trim())
        .bind(&r.tipo)
        .bind(r.artigo_id)
        .bind(if r.artigo_id.is_some() { None } else { r.familia_id })
        .bind(r.leve)
        .bind(r.pague)
        .bind(r.quantidade_min)
        .bind(r.desconto_pct)
        .bind(vazio(r.hora_inicio))
        .bind(vazio(r.hora_fim))
        .bind(dias)
        .bind(vazio(r.data_inicio))
        .bind(vazio(r.data_fim))
        .bind(r.ativo as i64);
    if let Some(id) = r.id {
        query = query.bind(id);
    }
    let res = query.execute(&st.pool).await?;
    let id = r.id.unwrap_or_else(|| res.last_insert_rowid());
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'PROMOCAO', ?)").bind(s.id).bind(format!("Regra {id}: {}", r.descricao.trim())).execute(&st.pool).await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct CupaoIn {
    id: Option<i64>,
    codigo: String,
    descricao: Option<String>,
    tipo: String,
    valor: f64,
    #[serde(default)]
    valor_minimo: i64,
    data_inicio: Option<String>,
    data_fim: Option<String>,
    utilizacoes_max: Option<i64>,
    #[serde(default = "verdade")]
    ativo: bool,
}

pub async fn cupao_guardar(st: &AppState, c: CupaoIn) -> AppResult<Value> {
    let s = st.exigir_perm("artigos.editar")?;
    let codigo = c.codigo.trim().to_uppercase();
    if codigo.len() < 3 || !codigo.chars().all(|x| x.is_ascii_alphanumeric() || x == '-' || x == '_') {
        return Err(AppError::msg("O código do cupão tem pelo menos 3 letras ou algarismos (sem espaços)."));
    }
    match c.tipo.as_str() {
        "PCT" if c.valor > 0.0 && c.valor <= 100.0 => {}
        "VALOR" if c.valor > 0.0 => {}
        _ => return Err(AppError::msg("Indique o desconto do cupão: % (até 100) ou valor em Kz.")),
    }
    let vazio = |x: Option<String>| x.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let q = if c.id.is_some() {
        "UPDATE cupoes SET codigo=?1, descricao=?2, tipo=?3, valor=?4, valor_minimo=?5, data_inicio=?6, data_fim=?7, utilizacoes_max=?8, ativo=?9 WHERE id=?10"
    } else {
        "INSERT INTO cupoes (codigo, descricao, tipo, valor, valor_minimo, data_inicio, data_fim, utilizacoes_max, ativo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    };
    let mut query = sqlx::query(q)
        .bind(&codigo)
        .bind(vazio(c.descricao))
        .bind(&c.tipo)
        .bind(c.valor)
        .bind(c.valor_minimo.max(0))
        .bind(vazio(c.data_inicio))
        .bind(vazio(c.data_fim))
        .bind(c.utilizacoes_max.filter(|x| *x > 0))
        .bind(c.ativo as i64);
    if let Some(id) = c.id {
        query = query.bind(id);
    }
    let res = query.execute(&st.pool).await.map_err(|e| match e {
        sqlx::Error::Database(d) if d.message().contains("UNIQUE") => AppError::msg(format!("Já existe o cupão {codigo}.")),
        e => e.into(),
    })?;
    let id = c.id.unwrap_or_else(|| res.last_insert_rowid());
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'CUPAO', ?)").bind(s.id).bind(format!("Cupão {codigo}")).execute(&st.pool).await?;
    Ok(json!({ "id": id }))
}

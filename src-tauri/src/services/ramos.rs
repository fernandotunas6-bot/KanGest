// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Ramos de actividade: o perfil do negócio e os dados próprios de cada ramo nos artigos.
//!
//! O ecrã decide que campos mostra em cada ramo (src/lib/ramos.ts); aqui guarda-se o ramo escolhido,
//! validam-se os atributos dos artigos (um objecto JSON pequeno, pesquisável) e geram-se as variantes
//! (tamanho × cor) de um artigo de moda ou calçado.

use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Ramos conhecidos (o identificador vai para config('empresa.ramo')).
pub const RAMOS: &[&str] = &[
    "comercio",
    "supermercado",
    "livraria",
    "farmacia",
    "moda",
    "electronica",
    "ferragens",
    "autopecas",
    "restauracao",
    "servicos",
    "beleza",
    "oficina",
    "grossista",
];

pub async fn ramo<'e, E: sqlx::SqliteExecutor<'e>>(e: E) -> AppResult<String> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'empresa.ramo'").fetch_optional(e).await?;
    Ok(v.filter(|r| RAMOS.contains(&r.as_str())).unwrap_or_else(|| "comercio".into()))
}

pub(crate) async fn gravar_ramo<'e, E: sqlx::SqliteExecutor<'e>>(e: E, ramo: &str) -> AppResult<()> {
    if !RAMOS.contains(&ramo) {
        return Err(AppError::msg("Ramo de actividade desconhecido."));
    }
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('empresa.ramo', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(ramo)
        .execute(e)
        .await?;
    Ok(())
}

pub async fn obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(json!({ "ramo": ramo(&st.pool).await? }))
}

#[derive(Deserialize)]
pub struct DefinirIn {
    ramo: String,
}

pub async fn definir(st: &AppState, d: DefinirIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    gravar_ramo(&st.pool, d.ramo.trim()).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'CONFIG', ?)")
        .bind(s.id)
        .bind(format!("Ramo de actividade: {}", d.ramo.trim()))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "ramo": d.ramo.trim() }))
}

const MAX_ATRIBUTOS: usize = 40;
const MAX_TEXTO: usize = 300;

/// Atributos de um artigo → texto JSON a gravar (None quando não sobra nenhum). Só aceita um objecto
/// plano com nomes simples e valores de texto, número ou sim/não.
pub fn validar_atributos(v: &Value) -> AppResult<Option<String>> {
    let Some(obj) = v.as_object() else {
        if v.is_null() {
            return Ok(None);
        }
        return Err(AppError::msg("Os dados do ramo do artigo são inválidos."));
    };
    if obj.len() > MAX_ATRIBUTOS {
        return Err(AppError::msg(format!("No máximo {MAX_ATRIBUTOS} dados de ramo por artigo.")));
    }
    let mut limpo = Map::new();
    for (k, val) in obj {
        if k.is_empty() || k.len() > 40 || !k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(AppError::msg(format!("Nome de dado de ramo inválido: «{k}».")));
        }
        match val {
            Value::Null => {}
            Value::String(s) => {
                let s = s.trim();
                if s.chars().count() > MAX_TEXTO {
                    return Err(AppError::msg(format!("O dado «{k}» é demasiado longo (máximo {MAX_TEXTO} caracteres).")));
                }
                if !s.is_empty() {
                    limpo.insert(k.clone(), Value::String(s.to_string()));
                }
            }
            Value::Number(_) => {
                limpo.insert(k.clone(), val.clone());
            }
            Value::Bool(b) => {
                // só os "sim" ficam: "não" é o mesmo que não ter o dado
                if *b {
                    limpo.insert(k.clone(), Value::Bool(true));
                }
            }
            _ => return Err(AppError::msg(format!("O dado «{k}» tem de ser texto, número ou sim/não."))),
        }
    }
    Ok((!limpo.is_empty()).then(|| Value::Object(limpo).to_string()))
}

/// Código com forma de ISBN: 10 caracteres (ISBN-10, pode acabar em X) ou 13 algarismos começados por 978/979.
pub fn parece_isbn(texto: &str) -> bool {
    let s: String = texto.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    (s.len() == 10 && s[..9].chars().all(|c| c.is_ascii_digit()) && s[9..].chars().all(|c| c.is_ascii_digit() || c == 'X' || c == 'x'))
        || (s.len() == 13 && s.chars().all(|c| c.is_ascii_digit()) && (s.starts_with("978") || s.starts_with("979")))
}

/// ISBN-10 ou ISBN-13 (com ou sem hífenes) → ISBN-13 só com algarismos, se o algarismo de controlo bater.
pub fn isbn13(texto: &str) -> Option<String> {
    let s: String = texto.chars().filter(|c| !matches!(c, '-' | ' ')).collect::<String>().to_ascii_uppercase();
    let d: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if s.len() == 13 && d.len() == 13 && (s.starts_with("978") || s.starts_with("979")) {
        let soma: u32 = d[..12].iter().enumerate().map(|(i, x)| if i % 2 == 0 { *x } else { x * 3 }).sum();
        return ((10 - soma % 10) % 10 == d[12]).then_some(s);
    }
    if s.len() == 10 && d.len() >= 9 && s[..9].chars().all(|c| c.is_ascii_digit()) {
        let controlo = match s.chars().last()? {
            'X' => 10,
            c => c.to_digit(10)?,
        };
        let soma: u32 = d[..9].iter().enumerate().map(|(i, x)| x * (10 - i as u32)).sum::<u32>() + controlo;
        if soma % 11 != 0 {
            return None;
        }
        let base = format!("978{}", &s[..9]);
        let b: Vec<u32> = base.chars().filter_map(|c| c.to_digit(10)).collect();
        let soma13: u32 = b.iter().enumerate().map(|(i, x)| if i % 2 == 0 { *x } else { x * 3 }).sum();
        return Some(format!("{base}{}", (10 - soma13 % 10) % 10));
    }
    None
}

#[derive(Deserialize)]
pub struct Eixo {
    /// nome do dado (ex.: "tamanho", "cor")
    nome: String,
    valores: Vec<String>,
}

#[derive(Deserialize)]
pub struct VariantesIn {
    artigo_id: i64,
    eixos: Vec<Eixo>,
}

/// Sufixo curto e seguro para a referência (ex.: "Azul marinho" → "AZULM").
fn sufixo(v: &str) -> String {
    let s: String = v
        .chars()
        .filter_map(|c| {
            let c = match c {
                'á' | 'à' | 'â' | 'ã' | 'Á' | 'À' | 'Â' | 'Ã' => 'A',
                'é' | 'ê' | 'É' | 'Ê' => 'E',
                'í' | 'Í' => 'I',
                'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
                'ú' | 'Ú' => 'U',
                'ç' | 'Ç' => 'C',
                c => c,
            };
            c.is_ascii_alphanumeric().then(|| c.to_ascii_uppercase())
        })
        .collect();
    s.chars().take(6).collect()
}

/// Gera um artigo por combinação (ex.: 5 tamanhos × 3 cores = 15 artigos), a partir de um artigo modelo:
/// mesma família, IVA, preços e custo; referência "MODELO-M-AZUL"; descrição "Camisa Polo M / Azul";
/// os atributos levam o modelo e o valor de cada eixo. As combinações que já existem não se repetem.
pub async fn variantes(st: &AppState, v: VariantesIn) -> AppResult<Value> {
    st.exigir_perm("artigos.criar")?;
    let eixos: Vec<(String, Vec<String>)> = v
        .eixos
        .into_iter()
        .map(|e| {
            let vals: Vec<String> = e.valores.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
            (e.nome.trim().to_lowercase(), vals)
        })
        .filter(|(_, vals)| !vals.is_empty())
        .collect();
    if eixos.is_empty() {
        return Err(AppError::msg("Indique pelo menos um tamanho, cor ou outro valor."));
    }
    for (n, _) in &eixos {
        if n.is_empty() || !n.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Err(AppError::msg("Nome de variante inválido."));
        }
    }
    let total: usize = eixos.iter().map(|(_, v)| v.len()).product();
    if total > 200 {
        return Err(AppError::msg(format!("São {total} combinações: no máximo 200 de cada vez.")));
    }
    let mut tx = crate::db::transacao(&st.pool).await?;
    let base = sqlx::query("SELECT referencia, descricao, atributos FROM artigos WHERE id = ?")
        .bind(v.artigo_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    let referencia: String = base.get("referencia");
    let descricao: String = base.get("descricao");
    let mut atributos: Map<String, Value> =
        base.get::<Option<String>, _>("atributos").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    atributos.entry("modelo").or_insert_with(|| Value::String(referencia.clone()));

    // produto cartesiano dos eixos
    let mut combinacoes: Vec<Vec<(String, String)>> = vec![vec![]];
    for (nome, vals) in &eixos {
        combinacoes = combinacoes
            .into_iter()
            .flat_map(|c| vals.iter().map(move |x| [c.clone(), vec![(nome.clone(), x.clone())]].concat()))
            .collect();
    }
    let (mut criados, mut existentes) = (Vec::new(), 0);
    for c in combinacoes {
        let r = format!("{referencia}-{}", c.iter().map(|(_, x)| sufixo(x)).collect::<Vec<_>>().join("-"));
        let ja: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ?").bind(&r).fetch_optional(&mut *tx).await?;
        if ja.is_some() {
            existentes += 1;
            continue;
        }
        let d = format!("{descricao} {}", c.iter().map(|(_, x)| x.as_str()).collect::<Vec<_>>().join(" / "));
        let mut at = atributos.clone();
        for (n, x) in &c {
            at.insert(n.clone(), Value::String(x.clone()));
        }
        let id = sqlx::query(
            "INSERT INTO artigos (referencia, descricao, descricao_curta, tipo, familia_id, subfamilia_id, unidade_id, taxa_iva_id,
                 motivo_isencao, custo, custo_medio, preco1, preco2, preco3, preco4, pvp1, pvp2, pvp3, pvp4, qtd_minima, qtd_maxima,
                 controla_stock, permite_negativo, ativo, atributos, alterado_em)
             SELECT ?, ?, descricao_curta, tipo, familia_id, subfamilia_id, unidade_id, taxa_iva_id,
                 motivo_isencao, custo, custo, preco1, preco2, preco3, preco4, pvp1, pvp2, pvp3, pvp4, qtd_minima, qtd_maxima,
                 controla_stock, permite_negativo, 1, ?, datetime('now','localtime')
             FROM artigos WHERE id = ?",
        )
        .bind(&r)
        .bind(&d)
        .bind(Value::Object(at).to_string())
        .bind(v.artigo_id)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        criados.push(json!({ "id": id, "referencia": r, "descricao": d }));
    }
    tx.commit().await?;
    Ok(json!({ "criados": criados, "existentes": existentes }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn isbn() {
        assert_eq!(isbn13("978-989-52-7385-0").as_deref(), Some("9789895273850"));
        assert_eq!(isbn13("972-20-1290-8").as_deref(), Some("9789722012904"));
        assert_eq!(isbn13("043942089X").as_deref(), Some("9780439420891"));
        assert!(isbn13("978-989-52-7385-9").is_none(), "algarismo de controlo errado");
        assert!(isbn13("972-20-1290-1").is_none());
        assert!(isbn13("5601234567890").is_none(), "um EAN que não é de livro");
        assert!(parece_isbn("972-20-1290-8") && parece_isbn("9789895273850") && !parece_isbn("5601234567890") && !parece_isbn("A00012"));
    }

    #[test]
    fn atributos() {
        assert_eq!(validar_atributos(&json!({ "autor": " Pepetela ", "ano": 2021, "receita": false, "vazio": "" })).unwrap().unwrap(), r#"{"ano":2021,"autor":"Pepetela"}"#);
        assert!(validar_atributos(&json!({})).unwrap().is_none());
        assert!(validar_atributos(&json!({ "Autor": "x" })).is_err());
        assert!(validar_atributos(&json!({ "lista": [1, 2] })).is_err());
        assert!(validar_atributos(&json!("texto")).is_err());
        assert_eq!(sufixo("Azul marinho"), "AZULMA");
        assert_eq!(sufixo("Média"), "MEDIA");
    }
}

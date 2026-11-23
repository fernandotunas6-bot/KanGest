// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Todas as tabelas auxiliares de uma vez (formulários e POS).
pub async fn tabelas(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let p = &st.pool;
    Ok(json!({
        "familias": db::query_json(p, "SELECT * FROM familias ORDER BY descricao").await?,
        "subfamilias": db::query_json(p, "SELECT * FROM subfamilias ORDER BY descricao").await?,
        "unidades": db::query_json(p, "SELECT * FROM unidades ORDER BY descricao").await?,
        "taxas": db::query_json(p, "SELECT * FROM taxas_iva WHERE ativo = 1 ORDER BY percentagem DESC").await?,
        "motivos": db::query_json(p, "SELECT * FROM motivos_isencao ORDER BY codigo").await?,
        "armazens": db::query_json(p, "SELECT * FROM armazens WHERE ativo = 1 ORDER BY id").await?,
        // os pontos de fidelização («DE») pagam-se pelo botão próprio do POS, não por um campo livre
        "modos_pagamento": db::query_json(p, "SELECT * FROM modos_pagamento WHERE ativo = 1 AND codigo <> 'DE' ORDER BY ordem").await?,
        "tipos_documento": db::query_json(p, "SELECT * FROM tipos_documento WHERE ativo = 1 ORDER BY ordem").await?,
        "postos": db::query_json(p, "SELECT id, descricao, armazem_id, serie_prefixo FROM postos ORDER BY id").await?,
        "posto_id": st.posto().id,
        "armazem_id": st.posto().armazem_id,
        "posto": st.posto(),
        // vendas calculadas a partir do PVP com IVA (Parametrização → Opções de venda)
        "precos_com_iva": super::documentos::precos_com_iva(p).await?,
        "sem_stock": super::documentos::politica_sem_stock(p).await?,
        "fidelizacao": super::fidelizacao::cfg(p).await?,
        // ramo de actividade: decide os dados de ramo dos artigos e alguns comportamentos do POS
        "ramo": super::ramos::ramo(p).await?,
        // tipo de gestão (loja ou escola): decide os menus
        "modo": super::modos::modo(p).await?,
    }))
}

#[derive(Deserialize)]
pub struct TabelaIn {
    tabela: String,
    id: Option<i64>,
    descricao: String,
    abreviatura: Option<String>,
    familia_id: Option<i64>,
    percentagem: Option<f64>,
    codigo: Option<String>,
}

/// Inserção/edição simples de tabelas auxiliares.
pub async fn tabela_guardar(st: &AppState, t: TabelaIn) -> AppResult<Value> {
    st.exigir_perm("sistema.tabelas")?;
    let desc = t.descricao.trim();
    if desc.is_empty() {
        return Err(AppError::msg("A descrição é obrigatória."));
    }
    let p = &st.pool;
    let id = match (t.tabela.as_str(), t.id) {
        ("familias", None) => sqlx::query("INSERT INTO familias (descricao) VALUES (?)").bind(desc).execute(p).await?.last_insert_rowid(),
        ("familias", Some(id)) => { sqlx::query("UPDATE familias SET descricao = ? WHERE id = ?").bind(desc).bind(id).execute(p).await?; id }
        ("subfamilias", None) => sqlx::query("INSERT INTO subfamilias (familia_id, descricao) VALUES (?, ?)").bind(t.familia_id.unwrap_or(1)).bind(desc).execute(p).await?.last_insert_rowid(),
        ("subfamilias", Some(id)) => { sqlx::query("UPDATE subfamilias SET familia_id = ?, descricao = ? WHERE id = ?").bind(t.familia_id.unwrap_or(1)).bind(desc).bind(id).execute(p).await?; id }
        ("unidades", None) => sqlx::query("INSERT INTO unidades (descricao, abreviatura) VALUES (?, ?)").bind(desc).bind(t.abreviatura.unwrap_or_default()).execute(p).await?.last_insert_rowid(),
        ("unidades", Some(id)) => { sqlx::query("UPDATE unidades SET descricao = ?, abreviatura = ? WHERE id = ?").bind(desc).bind(t.abreviatura.unwrap_or_default()).bind(id).execute(p).await?; id }
        ("armazens", None) => sqlx::query("INSERT INTO armazens (nome) VALUES (?)").bind(desc).execute(p).await?.last_insert_rowid(),
        ("armazens", Some(id)) => { sqlx::query("UPDATE armazens SET nome = ? WHERE id = ?").bind(desc).bind(id).execute(p).await?; id }
        ("taxas", None) => sqlx::query("INSERT INTO taxas_iva (codigo, descricao, percentagem) VALUES (?, ?, ?)").bind(t.codigo.unwrap_or_else(|| "NOR".into())).bind(desc).bind(t.percentagem.unwrap_or(0.0)).execute(p).await?.last_insert_rowid(),
        _ => return Err(AppError::msg("Tabela não suportada.")),
    };
    Ok(json!({ "id": id }))
}

const ARTIGO_SELECT: &str = "
    SELECT a.*, t.percentagem AS taxa_pct, t.descricao AS taxa_desc, u.abreviatura AS unidade,
           (SELECT fo.alterado_em FROM artigo_fotos fo WHERE fo.artigo_id = a.id) AS foto,
           f.descricao AS familia, sf.descricao AS subfamilia,
           COALESCE(s.quantidade, 0) AS stock,
           CASE WHEN a.pvp1 IS NOT NULL AND CAST(ROUND(a.pvp1 * 100.0 / (100 + t.percentagem)) AS INTEGER) = a.preco1 THEN a.pvp1
                ELSE CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) END AS preco1_iva,
           CASE WHEN a.promo_desconto > 0 AND COALESCE(a.promo_inicio, '') <= date('now', 'localtime')
                     AND (a.promo_fim IS NULL OR a.promo_fim = '' OR a.promo_fim >= date('now', 'localtime'))
                THEN a.promo_desconto ELSE 0 END AS promo_atual
    FROM artigos a
    JOIN taxas_iva t ON t.id = a.taxa_iva_id
    LEFT JOIN unidades u ON u.id = a.unidade_id
    LEFT JOIN familias f ON f.id = a.familia_id
    LEFT JOIN subfamilias sf ON sf.id = a.subfamilia_id
    LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?";

#[derive(Deserialize)]
pub struct ArtigosFiltro {
    q: Option<String>,
    familia_id: Option<i64>,
    armazem_id: Option<i64>,
    inativos: Option<bool>,
    limite: Option<i64>,
}

pub async fn artigos_listar(st: &AppState, f: ArtigosFiltro) -> AppResult<Value> {
    st.sessao()?;
    let q = format!("%{}%", f.q.unwrap_or_default().trim());
    let sql = format!(
        "{ARTIGO_SELECT}
         WHERE (a.descricao LIKE ?1 OR a.referencia LIKE ?1 OR IFNULL(a.codigo_barras,'') LIKE ?1
                OR (a.atributos IS NOT NULL AND EXISTS (SELECT 1 FROM json_each(a.atributos) j WHERE j.type = 'text' AND j.value LIKE ?1)))
           AND (?2 IS NULL OR a.familia_id = ?2)
           AND (?3 = 1 OR a.ativo = 1)
         ORDER BY a.descricao LIMIT ?4"
    )
    // o primeiro "?" (armazém) é o parâmetro 1 sem número — renumeramos:
    .replacen("s.armazem_id = ?", "s.armazem_id = ?5", 1);
    let rows = sqlx::query(&sql)
        .bind(q)
        .bind(f.familia_id)
        .bind(f.inativos.unwrap_or(false) as i64)
        .bind(f.limite.unwrap_or(500))
        .bind(f.armazem_id.unwrap_or(st.posto().armazem_id))
        .fetch_all(&st.pool)
        .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct IdIn {
    pub id: i64,
}

pub async fn artigos_obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let row = sqlx::query(&format!("{ARTIGO_SELECT} WHERE a.id = ?"))
        .bind(st.posto().armazem_id)
        .bind(i.id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    let mut v = db::row_json(&row);
    v["codigos"] = db::query_json_bind(&st.pool, "SELECT codigo, descricao, quantidade FROM artigo_codigos WHERE artigo_id = ? ORDER BY id", i.id).await?;
    Ok(v)
}

#[derive(Deserialize)]
pub struct IdsIn {
    ids: Vec<i64>,
}

/// Vários artigos de uma vez (retomar vendas suspensas / proformas no POS).
pub async fn artigos_por_ids(st: &AppState, i: IdsIn) -> AppResult<Value> {
    st.sessao()?;
    if i.ids.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let marcas = vec!["?"; i.ids.len()].join(",");
    let sql = format!("{ARTIGO_SELECT} WHERE a.id IN ({marcas})");
    let mut q = sqlx::query(&sql).bind(st.posto().armazem_id);
    for id in &i.ids {
        q = q.bind(id);
    }
    Ok(db::rows_json(&q.fetch_all(&st.pool).await?))
}

#[derive(Deserialize)]
pub struct CodigoIn {
    codigo: String,
}

/// Regra de uma etiqueta de balança: EAN-13 que começa por `prefixo`, seguido do código do artigo
/// (`digitos_artigo` algarismos) e do peso (PESO, em gramas com `decimais` = 3) ou do preço com IVA
/// (PRECO, em cêntimos com `decimais` = 2); o último algarismo é o de controlo.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct RegraBalanca {
    pub prefixo: String,
    pub tipo: String,
    #[serde(default = "cinco")]
    pub digitos_artigo: usize,
    #[serde(default = "tres")]
    pub decimais: u32,
}
fn cinco() -> usize {
    5
}
fn tres() -> u32 {
    3
}

/// Regras de balança configuradas (Parametrização); por omissão: 21 = peso em gramas, 22 = preço em Kz.
pub async fn regras_balanca(p: &sqlx::SqlitePool) -> AppResult<Vec<RegraBalanca>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'balanca.regras'").fetch_optional(p).await?;
    Ok(v.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| {
        vec![
            RegraBalanca { prefixo: "21".into(), tipo: "PESO".into(), digitos_artigo: 5, decimais: 3 },
            RegraBalanca { prefixo: "22".into(), tipo: "PRECO".into(), digitos_artigo: 5, decimais: 0 },
        ]
    }))
}

/// Algarismo de controlo de um EAN-13 (sobre os 12 primeiros).
pub fn ean13_controlo(doze: &str) -> Option<u32> {
    if doze.len() != 12 || !doze.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let soma: u32 = doze.chars().enumerate().map(|(i, c)| c.to_digit(10).unwrap() * if i % 2 == 0 { 1 } else { 3 }).sum();
    Some((10 - soma % 10) % 10)
}

/// Etiqueta de balança: (código do artigo, tipo, valor) — peso em kg ou preço em cêntimos.
pub fn ler_etiqueta(codigo: &str, regras: &[RegraBalanca]) -> Option<(String, String, f64)> {
    let c = codigo.trim();
    if c.len() != 13 || ean13_controlo(&c[..12])? != c[12..].parse::<u32>().ok()? {
        return None;
    }
    let r = regras.iter().find(|r| c.starts_with(&r.prefixo) && r.prefixo.len() + r.digitos_artigo < 12)?;
    let ini = r.prefixo.len();
    let artigo = c[ini..ini + r.digitos_artigo].to_string();
    let valor: f64 = c[ini + r.digitos_artigo..12].parse().ok()?;
    match r.tipo.as_str() {
        "PESO" => Some((artigo, "PESO".into(), valor / 10f64.powi(r.decimais as i32))),
        "PRECO" => Some((artigo, "PRECO".into(), valor * 10f64.powi(2 - r.decimais as i32))),
        _ => None,
    }
}

/// Leitura por código de barras ou referência exacta (leitor no POS): código principal, códigos
/// adicionais (com a quantidade da embalagem) e etiquetas de balança (com o peso ou o preço).
/// Devolve o artigo com `quantidade_lida` quando o código traz a quantidade.
pub async fn artigos_codigo(st: &AppState, c: CodigoIn) -> AppResult<Value> {
    st.sessao()?;
    let leitura = interpretar_codigo(&c.codigo);
    let Some((id, lido)) = ler_interpretado(&st.pool, &leitura).await? else { return Ok(Value::Null) };
    let armazem = st.posto().armazem_id;
    let Some(r) = sqlx::query(&format!("{ARTIGO_SELECT} WHERE a.ativo = 1 AND a.id = ?")).bind(armazem).bind(id).fetch_optional(&st.pool).await? else {
        return Ok(Value::Null);
    };
    let mut v = db::row_json(&r);
    if let Some(l) = lido {
        let q = l["quantidade"].as_f64().unwrap_or(1.0);
        if l["origem"] == "balanca" {
            v["quantidade_lida"] = json!(q);
            v["etiqueta"] = json!({ "tipo": l["tipo"], "valor": l["valor"] });
        } else if (q - 1.0).abs() > 1e-9 {
            // código adicional (ex.: embalagem de 12)
            v["quantidade_lida"] = json!(q);
        }
    }
    if let Some(g) = leitura.gs1 {
        v["gs1"] = g;
    }
    Ok(v)
}

/// Um código lido, já limpo: as formas a procurar (o próprio e as equivalentes) e, num código GS1 (o DataMatrix dos
/// medicamentos, o GS1-128 das caixas), o GTIN, o lote, a validade e o n.º de série.
pub struct Leitura {
    pub limpo: String,
    pub candidatos: Vec<String>,
    pub gs1: Option<Value>,
}

/// Dígito de controlo de um GTIN (EAN-8, UPC-A, EAN-13, GTIN-14): o último algarismo bate com os outros.
pub fn gtin_valido(c: &str) -> bool {
    if !matches!(c.len(), 8 | 12 | 13 | 14) || !c.chars().all(|x| x.is_ascii_digit()) {
        return false;
    }
    let d: Vec<u32> = c.chars().filter_map(|x| x.to_digit(10)).collect();
    let (corpo, controlo) = d.split_at(d.len() - 1);
    // da direita para a esquerda, pesos 3, 1, 3, 1…
    let soma: u32 = corpo.iter().rev().enumerate().map(|(i, x)| x * if i % 2 == 0 { 3 } else { 1 }).sum();
    (10 - soma % 10) % 10 == controlo[0]
}

/// GS1 (AI 01 + GTIN-14 e, a seguir, 17 validade, 10 lote, 21 série, 11/13/15 datas). Os campos de comprimento
/// variável acabam no separador de grupos (FNC1, \x1d) ou no fim; também aceita a forma com parênteses do texto.
fn ler_gs1(c: &str) -> Option<Value> {
    let texto = if c.starts_with("(01)") { c.replace('(', "\u{1d}").replace(')', "") } else { c.to_string() };
    let mut s = texto.trim_start_matches('\u{1d}');
    if !s.is_ascii() || !s.starts_with("01") || s.len() < 16 {
        return None;
    }
    let mut campos = Map::new();
    let data = |v: &str| {
        let (a, m, d) = (&v[0..2], &v[2..4], &v[4..6]);
        if d == "00" {
            format!("20{a}-{m}")
        } else {
            format!("20{a}-{m}-{d}")
        }
    };
    while s.len() >= 2 {
        let ai = &s[..2];
        let fixo = match ai {
            "01" | "02" => Some(14),
            "11" | "13" | "15" | "17" => Some(6),
            "10" | "21" => None,
            _ => break,
        };
        let resto = &s[2..];
        let (valor, depois) = match fixo {
            Some(n) if resto.len() >= n => (&resto[..n], &resto[n..]),
            Some(_) => break,
            None => match resto.find('\u{1d}') {
                Some(k) => (&resto[..k], &resto[k..]),
                None => (resto, ""),
            },
        };
        if fixo.is_some() && !valor.chars().all(|x| x.is_ascii_digit()) {
            break;
        }
        let nome = match ai {
            "01" => "gtin",
            "02" => "gtin_conteudo",
            "17" => "validade",
            "11" => "producao",
            "13" => "embalagem",
            "15" => "consumir_antes",
            "10" => "lote",
            _ => "serie",
        };
        let v = if matches!(ai, "11" | "13" | "15" | "17") { data(valor) } else { valor.to_string() };
        campos.insert(nome.into(), json!(v));
        s = depois.trim_start_matches('\u{1d}');
    }
    let gtin = campos.get("gtin")?.as_str()?.to_string();
    gtin_valido(&gtin).then_some(Value::Object(campos))
}

/// Limpa o que o leitor enviou e diz por que formas se procura. Aceita:
/// - o prefixo AIM que alguns leitores acrescentam («]E0» EAN, «]C1» GS1-128, «]d2» DataMatrix, «]Q3» QR);
/// - UPC-A (12) e EAN-13 com o zero à frente, e GTIN-14 (caixas) com zeros à esquerda;
/// - ISBN com hífenes ou ISBN-10 (os livros guardam-se em ISBN-13);
/// - GS1: procura-se pelo GTIN (o lote e a validade vêm à parte).
pub fn interpretar_codigo(bruto: &str) -> Leitura {
    let mut limpo: String = bruto.chars().filter(|x| *x == '\u{1d}' || !x.is_control()).collect::<String>().trim().to_string();
    if limpo.starts_with(']') && limpo.chars().count() > 3 {
        limpo = limpo.chars().skip(3).collect::<String>().trim().to_string();
    }
    let gs1 = ler_gs1(&limpo);
    let mut candidatos: Vec<String> = Vec::new();
    let mut juntar = |c: String| {
        if !c.is_empty() && !candidatos.contains(&c) {
            candidatos.push(c);
        }
    };
    let base = match gs1.as_ref().and_then(|g| g["gtin"].as_str()) {
        Some(g) => g.to_string(),
        None => {
            let sem_gs = limpo.replace('\u{1d}', "");
            juntar(sem_gs.clone());
            sem_gs
        }
    };
    // só algarismos (ISBN e códigos com hífenes ou espaços)
    let digitos: String = base.chars().filter(|x| !matches!(x, '-' | ' ')).collect();
    if !digitos.is_empty() && digitos.chars().all(|x| x.is_ascii_digit()) {
        juntar(digitos.clone());
        match digitos.len() {
            12 => {
                juntar(format!("0{digitos}"));
                juntar(format!("00{digitos}"));
            }
            13 | 14 => {
                let mut d = digitos.as_str();
                while d.len() > 12 && d.starts_with('0') {
                    d = &d[1..];
                    juntar(d.to_string());
                }
                if digitos.len() == 14 && digitos.starts_with("000000") {
                    juntar(digitos[6..].to_string()); // EAN-8 numa caixa
                }
                if digitos.len() == 13 {
                    juntar(format!("0{digitos}"));
                }
            }
            _ => {}
        }
    }
    if super::ramos::parece_isbn(&base) {
        if let Some(i) = super::ramos::isbn13(&base) {
            juntar(i);
        }
    }
    let limpo = limpo.replace('\u{1d}', "");
    Leitura { limpo, candidatos, gs1 }
}

/// Leitura de um código para a API de integração: (artigo, leitura). A leitura só vem quando o código vale
/// outra quantidade que não 1: embalagem (`{"origem": "embalagem", "quantidade", "descricao"}`) ou etiqueta
/// de balança (`{"origem": "balanca", "tipo", "valor", "quantidade"}`). Mesma ordem que o leitor do POS.
pub async fn ler_codigo(p: &sqlx::SqlitePool, codigo: &str) -> AppResult<Option<(i64, Option<Value>)>> {
    ler_interpretado(p, &interpretar_codigo(codigo)).await
}

async fn ler_interpretado(p: &sqlx::SqlitePool, l: &Leitura) -> AppResult<Option<(i64, Option<Value>)>> {
    if l.candidatos.is_empty() {
        return Ok(None);
    }
    // 1. código de barras ou referência do artigo (em todas as formas do código)
    for codigo in &l.candidatos {
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE ativo = 1 AND (codigo_barras = ?1 OR referencia = ?1) ORDER BY codigo_barras = ?1 DESC, id LIMIT 1")
            .bind(codigo)
            .fetch_optional(p)
            .await?;
        if let Some(id) = id {
            return Ok(Some((id, None)));
        }
    }
    // 2. código adicional (outra embalagem)
    for codigo in &l.candidatos {
        let extra: Option<(i64, f64, Option<String>)> = sqlx::query_as(
            "SELECT c.artigo_id, c.quantidade, c.descricao FROM artigo_codigos c JOIN artigos a ON a.id = c.artigo_id WHERE c.codigo = ? AND a.ativo = 1",
        )
        .bind(codigo)
        .fetch_optional(p)
        .await?;
        if let Some((id, q, d)) = extra {
            return Ok(Some((id, ((q - 1.0).abs() > 1e-9 || d.is_some()).then(|| json!({ "origem": "embalagem", "quantidade": q, "descricao": d })))));
        }
    }
    // 3. etiqueta de balança (EAN-13 com o código do artigo e o peso ou o preço com IVA)
    let Some((plu, tipo, valor)) = ler_etiqueta(&l.limpo, &regras_balanca(p).await?) else { return Ok(None) };
    let artigo: Option<(i64, i64, String)> = sqlx::query_as(
        "SELECT a.id,
                CASE WHEN a.pvp1 IS NOT NULL AND CAST(ROUND(a.pvp1 * 100.0 / (100 + t.percentagem)) AS INTEGER) = a.preco1 THEN a.pvp1
                     ELSE CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) END,
                a.descricao
         FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id
         WHERE a.ativo = 1 AND a.codigo_balanca IS NOT NULL AND CAST(a.codigo_balanca AS INTEGER) = CAST(? AS INTEGER) LIMIT 1",
    )
    .bind(&plu)
    .fetch_optional(p)
    .await?;
    let Some((id, pvp, descricao)) = artigo else { return Ok(None) };
    let quantidade = if tipo == "PESO" {
        valor
    } else {
        // etiqueta com o preço (com IVA): a quantidade é o preço a dividir pelo PVP ao kg
        if pvp <= 0 {
            return Err(AppError::msg(format!("O artigo da etiqueta ({descricao}) não tem preço de venda.")));
        }
        (valor / pvp as f64 * 1_000_000.0).round() / 1_000_000.0
    };
    Ok(Some((id, Some(json!({ "origem": "balanca", "tipo": tipo, "valor": valor, "quantidade": quantidade })))))
}

#[derive(Deserialize)]
pub struct AssociarCodigoIn {
    artigo_id: i64,
    codigo: String,
    #[serde(default)]
    quantidade: Option<f64>,
    #[serde(default)]
    descricao: Option<String>,
}

/// Junta a um artigo um código que o leitor não conhecia (outra embalagem, o código do fornecedor…) sem abrir a
/// ficha; num código GS1 guarda-se o GTIN. Devolve o artigo, pronto a vender.
pub async fn associar_codigo(st: &AppState, a: AssociarCodigoIn) -> AppResult<Value> {
    st.exigir_perm("artigos.editar")?;
    let l = interpretar_codigo(&a.codigo);
    let codigo = match l.gs1.as_ref().and_then(|g| g["gtin"].as_str()) {
        Some(g) if g.starts_with('0') => g[1..].to_string(),
        Some(g) => g.to_string(),
        None => l.limpo.clone(),
    };
    if codigo.is_empty() || codigo.chars().count() > 64 {
        return Err(AppError::msg("Leia ou escreva o código (até 64 caracteres)."));
    }
    let quantidade = a.quantidade.unwrap_or(1.0);
    if !(quantidade > 0.0 && quantidade <= 100_000.0) {
        return Err(AppError::msg("A quantidade que o código vende tem de ser maior que zero."));
    }
    if let Some((id, _)) = ler_interpretado(&st.pool, &l).await? {
        let nome: String = sqlx::query_scalar("SELECT referencia || ' «' || descricao || '»' FROM artigos WHERE id = ?").bind(id).fetch_one(&st.pool).await?;
        return Err(AppError::msg(format!("O código {codigo} já está no artigo {nome}.")));
    }
    let descricao: String = sqlx::query_scalar("SELECT descricao FROM artigos WHERE id = ? AND ativo = 1")
        .bind(a.artigo_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::msg("Escolha um artigo activo."))?;
    let desc = a.descricao.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    sqlx::query("INSERT INTO artigo_codigos (artigo_id, codigo, descricao, quantidade) VALUES (?, ?, ?, ?)")
        .bind(a.artigo_id)
        .bind(&codigo)
        .bind(&desc)
        .bind(quantidade)
        .execute(&st.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.message().contains("UNIQUE") => AppError::msg(format!("O código {codigo} já está noutro artigo (pode estar desactivado).")),
            e => e.into(),
        })?;
    super::auditoria::registar_nota(
        st,
        "artigos",
        a.artigo_id,
        &format!("Artigo «{descricao}» (n.º {}): código de barras {codigo} associado pelo leitor", a.artigo_id),
        json!({ "outro_codigo": codigo, "quantidade": quantidade, "descricao_codigo": desc }),
    )
    .await?;
    artigos_codigo(st, CodigoIn { codigo }).await
}

#[derive(Deserialize)]
pub struct ArtigoIn {
    id: Option<i64>,
    referencia: Option<String>,
    codigo_barras: Option<String>,
    descricao: String,
    descricao_curta: Option<String>,
    tipo: Option<String>,
    familia_id: Option<i64>,
    subfamilia_id: Option<i64>,
    unidade_id: Option<i64>,
    taxa_iva_id: i64,
    motivo_isencao: Option<String>,
    custo: Option<i64>,
    preco1: i64,
    preco2: Option<i64>,
    preco3: Option<i64>,
    preco4: Option<i64>,
    /// PVP com IVA (cêntimos). Quando vem, é o valor mestre: o preço sem IVA calcula-se a partir dele.
    #[serde(default)]
    pvp1: Option<i64>,
    #[serde(default)]
    pvp2: Option<i64>,
    #[serde(default)]
    pvp3: Option<i64>,
    #[serde(default)]
    pvp4: Option<i64>,
    qtd_minima: Option<f64>,
    qtd_maxima: Option<f64>,
    controla_stock: Option<bool>,
    permite_negativo: Option<bool>,
    ativo: Option<bool>,
    controla_lote: Option<bool>,
    aviso_validade_dias: Option<i64>,
    composto: Option<bool>,
    /// Promoção: desconto % entre promo_inicio e promo_fim (AAAA-MM-DD; vazios = sem limite)
    promo_desconto: Option<f64>,
    promo_inicio: Option<String>,
    promo_fim: Option<String>,
    /// Composição do kit: [{componente_id, quantidade}]
    componentes: Option<Vec<ComponenteIn>>,
    /// Dados do ramo (ISBN/autor, tamanho/cor, marca/modelo…): objecto JSON; `None` mantém os que existem.
    #[serde(default)]
    atributos: Option<Value>,
    /// tipo de inventário do SAF-T (M, P, A, S, T); `None` mantém o que estava (M num artigo novo)
    #[serde(default)]
    tipo_inventario: Option<String>,
    /// Outros códigos de barras (ex.: embalagens); `None` mantém os que existem.
    #[serde(default)]
    codigos: Option<Vec<CodigoExtraIn>>,
    /// Código do artigo na balança (PLU), lido das etiquetas EAN-13.
    #[serde(default)]
    codigo_balanca: Option<String>,
}

#[derive(Deserialize)]
pub struct CodigoExtraIn {
    codigo: String,
    #[serde(default)]
    descricao: Option<String>,
    #[serde(default = "um_f")]
    quantidade: f64,
}
fn um_f() -> f64 {
    1.0
}

#[derive(Deserialize)]
pub struct ComponenteIn {
    componente_id: i64,
    quantidade: f64,
}

pub async fn artigos_guardar(st: &AppState, a: ArtigoIn) -> AppResult<Value> {
    st.exigir_perm(if a.id.is_some() { "artigos.editar" } else { "artigos.criar" })?;
    if a.descricao.trim().is_empty() {
        return Err(AppError::msg("A descrição do artigo é obrigatória."));
    }
    let pct: f64 = sqlx::query_scalar("SELECT percentagem FROM taxas_iva WHERE id = ?")
        .bind(a.taxa_iva_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::msg("Taxa de IVA inválida."))?;
    let motivo = a.motivo_isencao.filter(|m| !m.is_empty());
    if pct == 0.0 && motivo.is_none() {
        return Err(AppError::msg("Artigos isentos de IVA exigem motivo de isenção."));
    }
    // preços: com o PVP (com IVA) indicado, o preço sem IVA sai dele; sem PVP, o PVP sai do preço
    let niveis: Vec<(i64, i64)> = [(Some(a.preco1), a.pvp1), (a.preco2, a.pvp2), (a.preco3, a.pvp3), (a.preco4, a.pvp4)]
        .into_iter()
        .map(|(preco, pvp)| match pvp {
            Some(g) => (super::documentos::sem_iva(g.max(0), pct), g.max(0)),
            None => {
                let p = preco.unwrap_or(0).max(0);
                (p, super::documentos::com_iva(p, pct))
            }
        })
        .collect();
    let tipo = a.tipo.unwrap_or_else(|| "P".into());
    let controla = a.controla_stock.unwrap_or(tipo == "P") && tipo == "P";
    let cobar = a.codigo_barras.filter(|c| !c.trim().is_empty());
    let atributos = match &a.atributos {
        Some(v) => Some(super::ramos::validar_atributos(v)?),
        None => None,
    };
    // livraria: o ISBN é o código de barras do livro — confere-se e guarda-se em ISBN-13 (o ISBN-10 antigo converte-se)
    let cobar = match cobar {
        Some(c) if super::ramos::parece_isbn(&c) && super::ramos::ramo(&st.pool).await? == "livraria" => {
            Some(super::ramos::isbn13(&c).ok_or_else(|| AppError::msg(format!("O ISBN {} não é válido (confira os algarismos).", c.trim())))?)
        }
        outro => outro,
    };

    let mut tx = crate::db::transacao(&st.pool).await?;
    if let Some(c) = &cobar {
        let outro: Option<String> = sqlx::query_scalar("SELECT a.descricao FROM artigo_codigos c JOIN artigos a ON a.id = c.artigo_id WHERE c.codigo = ? AND c.artigo_id <> ?")
            .bind(c.trim())
            .bind(a.id.unwrap_or(0))
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(d) = outro {
            return Err(AppError::msg(format!("O código de barras {} já é um código de \"{d}\".", c.trim())));
        }
        // o mesmo código de barras (ou ISBN) em dois artigos: o leitor só encontraria um deles
        let dono: Option<String> = sqlx::query_scalar("SELECT descricao FROM artigos WHERE codigo_barras = ? AND id <> ? LIMIT 1")
            .bind(c.trim())
            .bind(a.id.unwrap_or(0))
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(d) = dono {
            return Err(AppError::msg(format!("O código de barras {} já é do artigo \"{d}\".", c.trim())));
        }
    }
    let referencia = match (a.referencia.filter(|r| !r.trim().is_empty()), a.id) {
        (Some(r), _) => r.trim().to_string(),
        // ao editar, uma referência vazia mantém a que o artigo já tem (antes gerava-lhe outra)
        (None, Some(id)) => sqlx::query_scalar("SELECT referencia FROM artigos WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NaoEncontrado)?,
        (None, None) => {
            let n: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM artigos").fetch_one(&mut *tx).await?;
            format!("A{n:05}")
        }
    };
    let dup: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ? AND id <> ?")
        .bind(&referencia)
        .bind(a.id.unwrap_or(0))
        .fetch_optional(&mut *tx)
        .await?;
    if dup.is_some() {
        return Err(AppError::msg(format!("Já existe um artigo com a referência {referencia}.")));
    }
    let tipo_inv = match a.tipo_inventario.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => Some(super::inventario::tipo_inventario(t).ok_or_else(|| AppError::msg("Tipo de inventário inválido (M, P, A, S ou T)."))?),
        None => None,
    };
    let q = if a.id.is_some() {
        "UPDATE artigos SET referencia=?1, codigo_barras=?2, descricao=?3, descricao_curta=?4, tipo=?5, familia_id=?6,
            subfamilia_id=?7, unidade_id=?8, taxa_iva_id=?9, motivo_isencao=?10, custo=?11, preco1=?12, preco2=?13,
            preco3=?14, preco4=?15, qtd_minima=?16, qtd_maxima=?17, controla_stock=?18, permite_negativo=?19, ativo=?20
         WHERE id = ?21"
    } else {
        "INSERT INTO artigos (referencia, codigo_barras, descricao, descricao_curta, tipo, familia_id, subfamilia_id,
            unidade_id, taxa_iva_id, motivo_isencao, custo, preco1, preco2, preco3, preco4, qtd_minima, qtd_maxima,
            controla_stock, permite_negativo, ativo, custo_medio)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?11)"
    };
    let mut query = sqlx::query(q)
        .bind(&referencia)
        .bind(cobar)
        .bind(a.descricao.trim())
        .bind(a.descricao_curta)
        .bind(&tipo)
        .bind(a.familia_id)
        .bind(a.subfamilia_id)
        .bind(a.unidade_id)
        .bind(a.taxa_iva_id)
        .bind(motivo)
        .bind(a.custo.unwrap_or(0))
        .bind(niveis[0].0)
        .bind(niveis[1].0)
        .bind(niveis[2].0)
        .bind(niveis[3].0)
        .bind(a.qtd_minima.unwrap_or(0.0))
        .bind(a.qtd_maxima.unwrap_or(0.0))
        .bind(controla as i64)
        .bind(a.permite_negativo.unwrap_or(false) as i64)
        .bind(a.ativo.unwrap_or(true) as i64);
    if let Some(id) = a.id {
        query = query.bind(id);
    }
    let res = query.execute(&mut *tx).await?;
    let id = a.id.unwrap_or_else(|| res.last_insert_rowid());
    if let Some(t) = tipo_inv {
        sqlx::query("UPDATE artigos SET tipo_inventario = ? WHERE id = ?").bind(t).bind(id).execute(&mut *tx).await?;
    }
    // código na balança (PLU) e outros códigos de barras
    if let Some(plu) = &a.codigo_balanca {
        let plu = plu.trim();
        if !plu.is_empty() && (!plu.chars().all(|c| c.is_ascii_digit()) || plu.len() > 6) {
            return Err(AppError::msg("O código da balança tem até 6 algarismos."));
        }
        if !plu.is_empty() {
            let dup: Option<String> = sqlx::query_scalar("SELECT descricao FROM artigos WHERE id <> ? AND CAST(codigo_balanca AS INTEGER) = CAST(? AS INTEGER)")
                .bind(id)
                .bind(plu)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(d) = dup {
                return Err(AppError::msg(format!("O código de balança {plu} já é do artigo \"{d}\".")));
            }
        }
        sqlx::query("UPDATE artigos SET codigo_balanca = ? WHERE id = ?").bind(Some(plu).filter(|x| !x.is_empty())).bind(id).execute(&mut *tx).await?;
    }
    if let Some(codigos) = &a.codigos {
        sqlx::query("DELETE FROM artigo_codigos WHERE artigo_id = ?").bind(id).execute(&mut *tx).await?;
        for c in codigos {
            let codigo = c.codigo.trim();
            if codigo.is_empty() {
                continue;
            }
            if c.quantidade <= 0.0 {
                return Err(AppError::msg(format!("A quantidade do código {codigo} tem de ser positiva.")));
            }
            let outro: Option<String> = sqlx::query_scalar(
                "SELECT descricao FROM artigos WHERE (codigo_barras = ?1 OR referencia = ?1)
                 UNION ALL SELECT a.descricao FROM artigo_codigos c JOIN artigos a ON a.id = c.artigo_id WHERE c.codigo = ?1 LIMIT 1",
            )
            .bind(codigo)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(d) = outro {
                return Err(AppError::msg(format!("O código {codigo} já está em uso (\"{d}\").")));
            }
            sqlx::query("INSERT INTO artigo_codigos (artigo_id, codigo, descricao, quantidade) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(codigo)
                .bind(c.descricao.as_deref().map(str::trim).filter(|x| !x.is_empty()))
                .bind(c.quantidade)
                .execute(&mut *tx)
                .await?;
        }
    }
    if let Some(at) = &atributos {
        sqlx::query("UPDATE artigos SET atributos = ? WHERE id = ?").bind(at).bind(id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE artigos SET pvp1 = ?, pvp2 = ?, pvp3 = ?, pvp4 = ? WHERE id = ?")
        .bind(niveis[0].1)
        .bind(niveis[1].1)
        .bind(niveis[2].1)
        .bind(niveis[3].1)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // lotes e composição (kit): o kit não tem stock próprio — movimenta os componentes
    let composto = a.composto.unwrap_or(false);
    sqlx::query("UPDATE artigos SET controla_lote = ?, aviso_validade_dias = ?, composto = ?, controla_stock = CASE WHEN ? = 1 THEN 0 ELSE controla_stock END WHERE id = ?")
        .bind((a.controla_lote.unwrap_or(false) && !composto) as i64)
        .bind(a.aviso_validade_dias.unwrap_or(30).clamp(0, 3650))
        .bind(composto as i64)
        .bind(composto as i64)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(pct) = a.promo_desconto {
        let data_ok = |d: &Option<String>| -> AppResult<Option<String>> {
            match d.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
                None => Ok(None),
                Some(x) if chrono::NaiveDate::parse_from_str(x, "%Y-%m-%d").is_ok() => Ok(Some(x.to_string())),
                Some(_) => Err(AppError::msg("Data da promoção inválida.")),
            }
        };
        let (ini, fim) = (data_ok(&a.promo_inicio)?, data_ok(&a.promo_fim)?);
        if !(0.0..=100.0).contains(&pct) {
            return Err(AppError::msg("O desconto da promoção tem de estar entre 0 e 100%."));
        }
        if let (Some(i), Some(f)) = (&ini, &fim) {
            if f < i {
                return Err(AppError::msg("A promoção termina antes de começar."));
            }
        }
        sqlx::query("UPDATE artigos SET promo_desconto = ?, promo_inicio = ?, promo_fim = ? WHERE id = ?")
            .bind(pct)
            .bind(if pct > 0.0 { ini } else { None })
            .bind(if pct > 0.0 { fim } else { None })
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(comps) = &a.componentes {
        sqlx::query("DELETE FROM artigo_componentes WHERE artigo_id = ?").bind(id).execute(&mut *tx).await?;
        if composto {
            if comps.is_empty() {
                return Err(AppError::msg("Indique os componentes do artigo composto."));
            }
            for c in comps {
                if c.componente_id == id || c.quantidade <= 0.0 {
                    return Err(AppError::msg("Componente inválido: um kit não se pode conter a si próprio e as quantidades têm de ser positivas."));
                }
                let e_kit: Option<i64> = sqlx::query_scalar("SELECT composto FROM artigos WHERE id = ?").bind(c.componente_id).fetch_optional(&mut *tx).await?;
                match e_kit {
                    None => return Err(AppError::msg("Componente não encontrado.")),
                    Some(1) => return Err(AppError::msg("Um componente não pode ser, ele próprio, um artigo composto.")),
                    _ => {}
                }
                sqlx::query("INSERT INTO artigo_componentes (artigo_id, componente_id, quantidade) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(c.componente_id)
                    .bind(c.quantidade)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(json!({ "id": id, "referencia": referencia }))
}

pub async fn artigos_componentes(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT c.componente_id, c.quantidade, a.referencia, a.descricao, a.custo_medio, u.abreviatura AS unidade
         FROM artigo_componentes c JOIN artigos a ON a.id = c.componente_id LEFT JOIN unidades u ON u.id = a.unidade_id
         WHERE c.artigo_id = ? ORDER BY a.descricao",
    )
    .bind(i.id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct LotesFiltro {
    artigo_id: Option<i64>,
    armazem_id: Option<i64>,
    /// Só lotes que expiram nos próximos N dias (aviso de validade).
    dias: Option<i64>,
    com_stock: Option<bool>,
}

/// Lotes por artigo / produtos por data de validade.
pub async fn lotes_listar(st: &AppState, f: LotesFiltro) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT l.*, a.referencia, a.descricao, ar.nome AS armazem, a.aviso_validade_dias,
                CAST(julianday(l.validade) - julianday(date('now','localtime')) AS INTEGER) AS dias_validade
         FROM lotes l JOIN artigos a ON a.id = l.artigo_id JOIN armazens ar ON ar.id = l.armazem_id
         WHERE (?1 IS NULL OR l.artigo_id = ?1) AND (?2 IS NULL OR l.armazem_id = ?2)
           AND (?3 = 0 OR l.quantidade > 0)
           AND (?4 IS NULL OR (l.validade IS NOT NULL AND julianday(l.validade) - julianday(date('now','localtime')) <= ?4))
         ORDER BY l.validade IS NULL, l.validade, a.descricao",
    )
    .bind(f.artigo_id)
    .bind(f.armazem_id)
    .bind(f.com_stock.unwrap_or(true) as i64)
    .bind(f.dias)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

pub async fn moedas(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    db::query_json(&st.pool, "SELECT * FROM moedas ORDER BY codigo = 'AOA' DESC, codigo").await
}

#[derive(Deserialize)]
pub struct MoedaIn {
    codigo: String,
    descricao: String,
    simbolo: String,
    cambio: f64,
    ativo: Option<bool>,
    /// aceite em numerário no pagamento do POS (`None` mantém o que estava)
    #[serde(default)]
    pos: Option<bool>,
}

/// Moedas e câmbios (frmMoeda / frmAlterarCambio do KIGEST).
pub async fn moeda_guardar(st: &AppState, m: MoedaIn) -> AppResult<Value> {
    st.exigir_perm("compras.fatura")?;
    let codigo = m.codigo.trim().to_uppercase();
    if codigo.len() != 3 || !codigo.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::msg("O código da moeda deve ter 3 letras (ex.: USD)."));
    }
    if codigo == "AOA" && (m.cambio - 1.0).abs() > 1e-9 {
        return Err(AppError::msg("O câmbio do Kwanza é sempre 1."));
    }
    if m.cambio < 0.0 {
        return Err(AppError::msg("Câmbio inválido."));
    }
    let ativo = m.ativo.unwrap_or(true);
    if m.pos == Some(true) {
        if codigo == "AOA" {
            return Err(AppError::msg("O Kwanza é a moeda do POS; as outras aceitam-se além dele."));
        }
        if !ativo || m.cambio <= 0.0 {
            return Err(AppError::msg("Para aceitar esta moeda no POS, active-a e indique o câmbio do dia."));
        }
    }
    sqlx::query(
        "INSERT INTO moedas (codigo, descricao, simbolo, cambio, ativo, pos) VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, 0))
         ON CONFLICT(codigo) DO UPDATE SET descricao = ?2, simbolo = ?3, cambio = ?4, ativo = ?5, pos = COALESCE(?6, pos)",
    )
    .bind(&codigo)
    .bind(m.descricao.trim())
    .bind(m.simbolo.trim())
    .bind(m.cambio)
    .bind(ativo as i64)
    .bind(m.pos.map(|p| p && ativo && m.cambio > 0.0 && codigo != "AOA").map(|p| p as i64))
    .execute(&st.pool)
    .await?;
    Ok(json!({ "codigo": codigo }))
}

#[derive(Deserialize)]
pub struct StockFiltro {
    armazem_id: Option<i64>,
    q: Option<String>,
    abaixo_minimo: Option<bool>,
}

pub async fn stock_listar(st: &AppState, f: StockFiltro) -> AppResult<Value> {
    st.sessao()?;
    // reservado: notas de encomenda de clientes por facturar; encomendado: encomendas a fornecedores
    // por receber (QtdPedidoCliente / QtdPedidoFornecedor do KIGEST)
    let rows = sqlx::query(
        "WITH pend AS (
            SELECT l.artigo_id,
                   SUM(CASE WHEN d.tipo_codigo = 'NE' THEN l.quantidade ELSE 0 END) AS reservado,
                   SUM(CASE WHEN d.tipo_codigo = 'VNE' THEN l.quantidade ELSE 0 END) AS encomendado
            FROM documentos d JOIN documento_linhas l ON l.documento_id = d.id
            WHERE d.tipo_codigo IN ('NE', 'VNE') AND d.estado = 'N' AND COALESCE(d.armazem_id, ?1) = ?1
            GROUP BY l.artigo_id)
         SELECT a.id, a.referencia, a.descricao, u.abreviatura AS unidade, a.qtd_minima, a.qtd_maxima,
                a.custo_medio, a.preco1, COALESCE(s.quantidade, 0) AS quantidade,
                CAST(ROUND(COALESCE(s.quantidade, 0) * a.custo_medio) AS INTEGER) AS valor_custo,
                s.ultima_entrada, s.ultima_saida,
                COALESCE(p.reservado, 0) AS reservado, COALESCE(p.encomendado, 0) AS encomendado,
                COALESCE(s.quantidade, 0) - COALESCE(p.reservado, 0) AS disponivel
         FROM artigos a
         LEFT JOIN unidades u ON u.id = a.unidade_id
         LEFT JOIN stock s ON s.artigo_id = a.id AND s.armazem_id = ?1
         LEFT JOIN pend p ON p.artigo_id = a.id
         WHERE a.controla_stock = 1 AND a.ativo = 1
           AND (a.descricao LIKE ?2 OR a.referencia LIKE ?2)
           AND (?3 = 0 OR COALESCE(s.quantidade, 0) <= a.qtd_minima)
         ORDER BY a.descricao",
    )
    .bind(f.armazem_id.unwrap_or(st.posto().armazem_id))
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.abaixo_minimo.unwrap_or(false) as i64)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[derive(Deserialize)]
pub struct MovFiltro {
    artigo_id: i64,
    armazem_id: Option<i64>,
}

pub async fn stock_movimentos(st: &AppState, f: MovFiltro) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT m.*, d.numero_doc, ar.nome AS armazem
         FROM movimentos_stock m
         LEFT JOIN documentos d ON d.id = m.documento_id
         LEFT JOIN armazens ar ON ar.id = m.armazem_id
         WHERE m.artigo_id = ?1 AND (?2 IS NULL OR m.armazem_id = ?2)
         ORDER BY m.data_hora DESC, m.id DESC LIMIT 500",
    )
    .bind(f.artigo_id)
    .bind(f.armazem_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

// ------------------------------------------------------------------ recálculo de stock (Fr_RecalculoStock)
#[derive(Deserialize)]
pub struct RecalcIn {
    #[serde(default)]
    aplicar: bool,
}

/// Compara as existências gravadas com a soma dos movimentos (stock e lotes) e, com `aplicar`, corrige-as
/// e refaz o saldo acumulado de cada movimento.
pub async fn stock_recalcular(st: &AppState, r: RecalcIn) -> AppResult<Value> {
    let s = st.exigir_perm("stock.inventario")?;
    let diferencas = db::rows_json(
        &sqlx::query(
            "WITH mov AS (SELECT artigo_id, armazem_id, SUM(quantidade) AS q FROM movimentos_stock GROUP BY artigo_id, armazem_id),
                  chaves AS (SELECT artigo_id, armazem_id FROM mov UNION SELECT artigo_id, armazem_id FROM stock)
             SELECT c.artigo_id, c.armazem_id, a.referencia, a.descricao, ar.nome AS armazem,
                    COALESCE(s.quantidade, 0) AS gravado, ROUND(COALESCE(m.q, 0), 6) AS calculado
             FROM chaves c
             JOIN artigos a ON a.id = c.artigo_id
             LEFT JOIN armazens ar ON ar.id = c.armazem_id
             LEFT JOIN stock s ON s.artigo_id = c.artigo_id AND s.armazem_id = c.armazem_id
             LEFT JOIN mov m ON m.artigo_id = c.artigo_id AND m.armazem_id = c.armazem_id
             WHERE ABS(COALESCE(s.quantidade, 0) - COALESCE(m.q, 0)) > 0.000001
             ORDER BY a.descricao",
        )
        .fetch_all(&st.pool)
        .await?,
    );
    let lotes = db::rows_json(
        &sqlx::query(
            "SELECT l.id, l.lote, a.referencia, a.descricao, l.quantidade AS gravado, ROUND(l.saldo_inicial + COALESCE(SUM(lm.quantidade), 0), 6) AS calculado
             FROM lotes l JOIN artigos a ON a.id = l.artigo_id
             LEFT JOIN lote_movimentos lm ON lm.lote_id = l.id
             GROUP BY l.id HAVING ABS(l.quantidade - l.saldo_inicial - COALESCE(SUM(lm.quantidade), 0)) > 0.000001
             ORDER BY a.descricao, l.lote",
        )
        .fetch_all(&st.pool)
        .await?,
    );
    let mut saldos = 0u64;
    if r.aplicar {
        let mut tx = crate::db::transacao(&st.pool).await?;
        sqlx::query(
            "INSERT INTO stock (artigo_id, armazem_id, quantidade)
             SELECT artigo_id, armazem_id, ROUND(SUM(quantidade), 6) FROM movimentos_stock GROUP BY artigo_id, armazem_id
             ON CONFLICT(artigo_id, armazem_id) DO UPDATE SET quantidade = excluded.quantidade",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE stock SET quantidade = 0 WHERE NOT EXISTS
               (SELECT 1 FROM movimentos_stock m WHERE m.artigo_id = stock.artigo_id AND m.armazem_id = stock.armazem_id)",
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE lotes SET quantidade = ROUND(saldo_inicial + COALESCE((SELECT SUM(quantidade) FROM lote_movimentos WHERE lote_id = lotes.id), 0), 6)")
            .execute(&mut *tx)
            .await?;
        saldos = sqlx::query(
            "UPDATE movimentos_stock SET saldo = s.acc
             FROM (SELECT id, ROUND(SUM(quantidade) OVER (PARTITION BY artigo_id, armazem_id ORDER BY data_hora, id), 6) AS acc FROM movimentos_stock) s
             WHERE s.id = movimentos_stock.id AND ABS(movimentos_stock.saldo - s.acc) > 0.000001",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'RECALCULO_STOCK', ?)")
            .bind(s.id)
            .bind(format!("{} existência(s), {} lote(s), {} saldo(s) de movimentos", diferencas.as_array().map_or(0, |v| v.len()), lotes.as_array().map_or(0, |v| v.len()), saldos))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(json!({ "diferencas": diferencas, "lotes": lotes, "aplicado": r.aplicar, "saldos_corrigidos": saldos }))
}

// ============================== eliminar e acções em lote ==============================

/// Elimina um artigo que nunca foi usado. Um artigo com documentos, movimentos de stock, lotes, avenças, ordens da
/// oficina, promoções, ou que entra num kit, fica só **desactivado**: deixa de aparecer nas vendas e na pesquisa, os
/// documentos antigos continuam a mostrá-lo, e pode reactivar-se na ficha.
pub async fn artigos_eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("artigos.editar")?;
    let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    existe.ok_or(AppError::NaoEncontrado)?;
    let documentos: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT documento_id) FROM documento_linhas WHERE artigo_id = ?").bind(i.id).fetch_one(&st.pool).await?;
    let outros: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM movimentos_stock WHERE artigo_id = ?1) + (SELECT COUNT(*) FROM lotes WHERE artigo_id = ?1)
              + (SELECT COUNT(*) FROM artigo_componentes WHERE componente_id = ?1) + (SELECT COUNT(*) FROM avenca_linhas WHERE artigo_id = ?1)
              + (SELECT COUNT(*) FROM of_ordem_linhas WHERE artigo_id = ?1) + (SELECT COUNT(*) FROM promocao_regras WHERE artigo_id = ?1)",
    )
    .bind(i.id)
    .fetch_one(&st.pool)
    .await?;
    if documentos == 0 && outros == 0 {
        let mut tx = db::transacao(&st.pool).await?;
        sqlx::query("DELETE FROM stock WHERE artigo_id = ? AND quantidade = 0").bind(i.id).execute(&mut *tx).await?;
        // as chaves estrangeiras ainda podem recusar (uma ligação que não está acima): então desactiva-se
        if sqlx::query("DELETE FROM artigos WHERE id = ?").bind(i.id).execute(&mut *tx).await.is_ok() {
            tx.commit().await?;
            return Ok(json!({ "eliminado": true }));
        }
        tx.rollback().await?;
    }
    sqlx::query("UPDATE artigos SET ativo = 0 WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "eliminado": false, "desactivado": true, "documentos": documentos }))
}

#[derive(Deserialize)]
pub struct ArtigosLoteIn {
    ids: Vec<i64>,
    /// activar | desactivar | familia | preco
    accao: String,
    familia_id: Option<i64>,
    /// percentagem a somar ao PVP 1 (negativa para baixar)
    percentagem: Option<f64>,
    /// arredondar o PVP novo a múltiplos deste valor, em kwanzas (0 = ao cêntimo)
    arredondar: Option<i64>,
    /// tipo de inventário do SAF-T (M, P, A, S, T)
    #[serde(default)]
    tipo_inventario: Option<String>,
}

/// O mesmo a vários artigos de uma vez: activar, desactivar, mudar de família ou ajustar o PVP 1 numa percentagem.
/// Cada artigo alterado fica na auditoria com o antes e o depois.
pub async fn artigos_lote(st: &AppState, l: ArtigosLoteIn) -> AppResult<Value> {
    st.exigir_perm("artigos.editar")?;
    if l.ids.is_empty() {
        return Err(AppError::msg("Escolha pelo menos um artigo."));
    }
    if l.ids.len() > 5000 {
        return Err(AppError::msg("De uma vez, no máximo 5 000 artigos."));
    }
    let pct = l.percentagem.unwrap_or(0.0);
    match l.accao.as_str() {
        "activar" | "desactivar" => {}
        "familia" => {
            let f = l.familia_id.ok_or_else(|| AppError::msg("Escolha a família."))?;
            let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE id = ?").bind(f).fetch_optional(&st.pool).await?;
            existe.ok_or_else(|| AppError::msg("A família escolhida já não existe."))?;
        }
        "tipo_inventario" => {
            l.tipo_inventario.as_deref().and_then(super::inventario::tipo_inventario).ok_or_else(|| AppError::msg("Escolha o tipo de inventário (M, P, A, S ou T)."))?;
        }
        "preco" => {
            if !pct.is_finite() || pct == 0.0 || pct <= -100.0 || pct > 1000.0 {
                return Err(AppError::msg("Indique uma percentagem entre -99,99% e 1 000% (diferente de zero)."));
            }
        }
        _ => return Err(AppError::msg("Acção desconhecida.")),
    }
    let passo = l.arredondar.unwrap_or(0).clamp(0, 100_000) * 100;
    let mut alterados = 0;
    for &id in &l.ids {
        let Some(antes) = super::auditoria::ficha(st, "artigos", id).await else { continue };
        match l.accao.as_str() {
            "activar" => sqlx::query("UPDATE artigos SET ativo = 1 WHERE id = ?").bind(id).execute(&st.pool).await?,
            "desactivar" => sqlx::query("UPDATE artigos SET ativo = 0 WHERE id = ?").bind(id).execute(&st.pool).await?,
            "tipo_inventario" => sqlx::query("UPDATE artigos SET tipo_inventario = ? WHERE id = ?")
                .bind(l.tipo_inventario.as_deref().and_then(super::inventario::tipo_inventario))
                .bind(id)
                .execute(&st.pool)
                .await?,
            "familia" => sqlx::query("UPDATE artigos SET familia_id = ?, subfamilia_id = NULL WHERE id = ? AND familia_id IS NOT ?")
                .bind(l.familia_id)
                .bind(id)
                .bind(l.familia_id)
                .execute(&st.pool)
                .await?,
            _ => {
                let (preco1, pvp1, taxa): (i64, Option<i64>, f64) =
                    sqlx::query_as("SELECT a.preco1, a.pvp1, t.percentagem FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.id = ?").bind(id).fetch_one(&st.pool).await?;
                let sem_iva = |pvp: i64| (pvp as f64 * 100.0 / (100.0 + taxa)).round() as i64;
                let actual = match pvp1 {
                    Some(p) if sem_iva(p) == preco1 => p,
                    _ => (preco1 as f64 * (1.0 + taxa / 100.0)).round() as i64,
                };
                let mut novo = (actual as f64 * (1.0 + pct / 100.0)).round() as i64;
                if passo > 0 {
                    novo = ((novo + passo / 2) / passo) * passo;
                }
                let novo = novo.max(0);
                sqlx::query("UPDATE artigos SET preco1 = ?, pvp1 = ? WHERE id = ?").bind(sem_iva(novo)).bind(novo).bind(id).execute(&st.pool).await?
            }
        };
        let agora = super::auditoria::ficha(st, "artigos", id).await;
        if agora.as_ref() != Some(&antes) {
            alterados += 1;
        }
        super::auditoria::registar_ficha(st, "artigos", id, Some(antes), agora, None).await?;
    }
    Ok(json!({ "alterados": alterados }))
}

#[derive(Deserialize)]
pub struct CodigoUsadoIn {
    codigo: String,
    #[serde(default)]
    id: Option<i64>,
}

/// O artigo que já usa esta referência ou código de barras (para avisar enquanto se escreve na ficha), ou `null`.
pub async fn codigo_usado(st: &AppState, c: CodigoUsadoIn) -> AppResult<Value> {
    st.sessao()?;
    let codigo = c.codigo.trim();
    if codigo.is_empty() {
        return Ok(Value::Null);
    }
    let r: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT a.id, a.referencia, a.descricao, a.ativo FROM artigos a
          WHERE a.id <> ?1 AND (a.referencia = ?2 OR a.codigo_barras = ?2 OR EXISTS (SELECT 1 FROM artigo_codigos c WHERE c.artigo_id = a.id AND c.codigo = ?2))
          LIMIT 1",
    )
    .bind(c.id.unwrap_or(0))
    .bind(codigo)
    .fetch_optional(&st.pool)
    .await?;
    Ok(r.map(|(id, referencia, descricao, ativo)| json!({ "id": id, "referencia": referencia, "descricao": descricao, "ativo": ativo == 1 })).unwrap_or(Value::Null))
}

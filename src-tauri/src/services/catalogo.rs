use serde::Deserialize;
use serde_json::{json, Value};

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
        "modos_pagamento": db::query_json(p, "SELECT * FROM modos_pagamento WHERE ativo = 1 ORDER BY ordem").await?,
        "tipos_documento": db::query_json(p, "SELECT * FROM tipos_documento WHERE ativo = 1 ORDER BY ordem").await?,
        "postos": db::query_json(p, "SELECT id, descricao, armazem_id, serie_prefixo FROM postos ORDER BY id").await?,
        "posto_id": st.posto().id,
        "armazem_id": st.posto().armazem_id,
        "posto": st.posto(),
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
           f.descricao AS familia, sf.descricao AS subfamilia,
           COALESCE(s.quantidade, 0) AS stock,
           CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) AS preco1_iva,
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
                OR EXISTS (SELECT 1 FROM artigo_codigos c WHERE c.artigo_id = a.id AND c.codigo LIKE ?1))
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
    v["codigos"] = db::query_json_bind(&st.pool, "SELECT codigo, quantidade, descricao FROM artigo_codigos WHERE artigo_id = ? ORDER BY id", i.id).await?;
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

/// Leitura por código de barras ou referência exacta (leitor no POS), código extra (embalagem) ou etiqueta da
/// balança. O artigo vem com `leitura` quando uma leitura vale outra quantidade: `{origem, quantidade, …}`.
pub async fn artigos_codigo(st: &AppState, c: CodigoIn) -> AppResult<Value> {
    st.sessao()?;
    let Some((id, leitura)) = ler_codigo(&st.pool, &c.codigo).await? else { return Ok(Value::Null) };
    let row = sqlx::query(&format!("{ARTIGO_SELECT} WHERE a.id = ?")).bind(st.posto().armazem_id).bind(id).fetch_one(&st.pool).await?;
    let mut v = db::row_json(&row);
    if let Some(l) = leitura {
        v["leitura"] = l;
    }
    Ok(v)
}

/// Artigo activo de um código lido (leitor do POS ou API): (id, leitura). A leitura diz a quantidade que a
/// leitura vale quando não é 1: embalagem (unidades da caixa) ou etiqueta da balança (peso, ou o preço convertido).
pub async fn ler_codigo(p: &sqlx::SqlitePool, codigo: &str) -> AppResult<Option<(i64, Option<Value>)>> {
    let codigo = codigo.trim();
    if codigo.is_empty() {
        return Ok(None);
    }
    // 1. código de barras ou referência do artigo
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE ativo = 1 AND (codigo_barras = ?1 OR referencia = ?1) ORDER BY codigo_barras = ?1 DESC, id LIMIT 1")
        .bind(codigo)
        .fetch_optional(p)
        .await?;
    if let Some(id) = id {
        return Ok(Some((id, None)));
    }
    // 2. código extra (outra embalagem)
    let extra: Option<(i64, f64, Option<String>)> = sqlx::query_as(
        "SELECT c.artigo_id, c.quantidade, c.descricao FROM artigo_codigos c JOIN artigos a ON a.id = c.artigo_id WHERE c.codigo = ? AND a.ativo = 1",
    )
    .bind(codigo)
    .fetch_optional(p)
    .await?;
    if let Some((id, q, d)) = extra {
        return Ok(Some((id, (q != 1.0 || d.is_some()).then(|| json!({ "origem": "embalagem", "quantidade": q, "descricao": d })))));
    }
    // 3. etiqueta da balança (EAN-13 com o código do artigo e o peso ou o preço)
    let cfg = balanca_ler(p).await?;
    let Some((plu, valor)) = cfg.partir(codigo) else { return Ok(None) };
    let artigo: Option<(i64, i64)> = sqlx::query_as(
        "SELECT a.id, CAST(ROUND(a.preco1 * (1 + t.percentagem / 100.0)) AS INTEGER) FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id
         WHERE a.ativo = 1 AND a.codigo_balanca IS NOT NULL AND (a.codigo_balanca = ?1 OR ltrim(a.codigo_balanca, '0') = ltrim(?1, '0')) LIMIT 1",
    )
    .bind(&plu)
    .fetch_optional(p)
    .await?;
    let Some((id, preco_iva)) = artigo else { return Ok(None) };
    let escala = 10f64.powi(cfg.decimais as i32);
    let leitura = if cfg.tipo == "PRECO" {
        // preço a pagar na etiqueta: a quantidade é a que dá esse preço ao preço do artigo (com IVA)
        let preco = (valor as f64 / escala * 100.0).round() as i64;
        if preco_iva <= 0 {
            return Err(AppError::msg("O artigo da etiqueta não tem preço: não se pode calcular a quantidade."));
        }
        let q = ((preco as f64 / preco_iva as f64) * 1000.0).round() / 1000.0;
        json!({ "origem": "balanca", "quantidade": q, "preco_etiqueta": preco })
    } else {
        let q = valor as f64 / escala;
        json!({ "origem": "balanca", "quantidade": q, "peso": q })
    };
    if leitura["quantidade"].as_f64().unwrap_or(0.0) <= 0.0 {
        return Err(AppError::msg("A etiqueta da balança tem quantidade zero."));
    }
    Ok(Some((id, Some(leitura))))
}

// ------------------------------------------------------------------ balança (etiquetas com peso ou preço)

/// Formato das etiquetas EAN-13 da balança: prefixo (2 algarismos) + código do artigo + valor + controlo.
pub struct Balanca {
    pub ativo: bool,
    pub prefixos: Vec<String>,
    pub digitos_codigo: usize,
    /// PESO | PRECO
    pub tipo: String,
    pub decimais: u32,
}

impl Balanca {
    /// (código do artigo, valor) de uma etiqueta da balança, ou None se o código não for uma.
    pub fn partir(&self, codigo: &str) -> Option<(String, i64)> {
        if !self.ativo || codigo.len() != 13 || !codigo.bytes().all(|b| b.is_ascii_digit()) || !ean13_valido(codigo) {
            return None;
        }
        if !self.prefixos.iter().any(|p| codigo.starts_with(p.as_str())) {
            return None;
        }
        let n = self.digitos_codigo;
        let plu = codigo.get(2..2 + n)?.to_string();
        let valor = codigo.get(2 + n..12)?.parse().ok()?;
        Some((plu, valor))
    }
}

/// Dígito de controlo EAN-13 (pesos 1 e 3 alternados da esquerda para a direita).
pub fn ean13_valido(codigo: &str) -> bool {
    let d: Vec<u32> = codigo.bytes().map(|b| (b - b'0') as u32).collect();
    if d.len() != 13 {
        return false;
    }
    let soma: u32 = d[..12].iter().enumerate().map(|(i, x)| if i % 2 == 0 { *x } else { x * 3 }).sum();
    (10 - soma % 10) % 10 == d[12]
}

pub async fn balanca_ler(p: &sqlx::SqlitePool) -> AppResult<Balanca> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT chave, valor FROM config WHERE chave LIKE 'balanca.%'").fetch_all(p).await?;
    let v = |k: &str| rows.iter().find(|r| r.0 == k).map(|r| r.1.trim().to_string()).unwrap_or_default();
    Ok(Balanca {
        ativo: v("balanca.ativo") == "1",
        prefixos: v("balanca.prefixos").split(|c: char| c == ',' || c.is_whitespace()).filter(|x| x.len() == 2).map(String::from).collect(),
        digitos_codigo: v("balanca.digitos_codigo").parse().unwrap_or(5).clamp(4, 6),
        tipo: if v("balanca.tipo") == "PRECO" { "PRECO".into() } else { "PESO".into() },
        decimais: v("balanca.decimais").parse().unwrap_or(3).min(3),
    })
}

pub async fn balanca_obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let b = balanca_ler(&st.pool).await?;
    Ok(json!({ "ativo": b.ativo, "prefixos": b.prefixos.join(","), "digitos_codigo": b.digitos_codigo, "tipo": b.tipo, "decimais": b.decimais }))
}

#[derive(Deserialize)]
pub struct BalancaIn {
    ativo: bool,
    prefixos: String,
    digitos_codigo: usize,
    tipo: String,
    decimais: u32,
}

pub async fn balanca_guardar(st: &AppState, b: BalancaIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.parametrizacao")?;
    let prefixos: Vec<&str> = b.prefixos.split(|c: char| c == ',' || c.is_whitespace()).filter(|x| !x.is_empty()).collect();
    if prefixos.is_empty() || prefixos.iter().any(|x| x.len() != 2 || !x.starts_with('2') || !x.bytes().all(|c| c.is_ascii_digit())) {
        return Err(AppError::msg("Os prefixos das etiquetas da balança têm 2 algarismos, de 20 a 29 (ex.: 20,21,28)."));
    }
    if !(4..=6).contains(&b.digitos_codigo) || b.decimais > 3 || !matches!(b.tipo.as_str(), "PESO" | "PRECO") {
        return Err(AppError::msg("Formato da balança inválido: o código do artigo tem 4 a 6 algarismos e o valor até 3 casas decimais."));
    }
    let mut tx = st.pool.begin().await?;
    for (k, v) in [
        ("balanca.ativo", if b.ativo { "1".to_string() } else { "0".to_string() }),
        ("balanca.prefixos", prefixos.join(",")),
        ("balanca.digitos_codigo", b.digitos_codigo.to_string()),
        ("balanca.tipo", b.tipo.clone()),
        ("balanca.decimais", b.decimais.to_string()),
    ] {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'CONFIGURACAO', ?)")
        .bind(s.id)
        .bind(format!("Etiquetas da balança: {}", if b.ativo { "ligadas" } else { "desligadas" }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    balanca_obter(st).await
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
    /// Código do artigo na balança (PLU). Vazio = sem balança; omitido = não muda.
    codigo_balanca: Option<String>,
    /// Outros códigos de barras e embalagens: [{codigo, quantidade, descricao}]. Omitido = não mudam.
    codigos: Option<Vec<CodigoExtraIn>>,
}

#[derive(Deserialize)]
pub struct CodigoExtraIn {
    codigo: String,
    quantidade: Option<f64>,
    descricao: Option<String>,
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
    let tipo = a.tipo.unwrap_or_else(|| "P".into());
    let controla = a.controla_stock.unwrap_or(tipo == "P") && tipo == "P";
    let cobar = a.codigo_barras.filter(|c| !c.trim().is_empty());
    let cobar_txt: Option<String> = cobar.as_ref().map(|c| c.trim().to_string());

    let mut tx = st.pool.begin().await?;
    let referencia = match (a.referencia.filter(|r| !r.trim().is_empty()), a.id) {
        (Some(r), _) => r.trim().to_string(),
        // ao alterar sem referência, fica a que tinha (antes gerava-se outra)
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
        .bind(a.preco1)
        .bind(a.preco2.unwrap_or(0))
        .bind(a.preco3.unwrap_or(0))
        .bind(a.preco4.unwrap_or(0))
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
    if let Some(plu) = &a.codigo_balanca {
        let plu: String = plu.chars().filter(|c| !c.is_whitespace()).collect();
        if !plu.is_empty() && (!plu.bytes().all(|c| c.is_ascii_digit()) || plu.len() > 6) {
            return Err(AppError::msg("O código da balança tem só algarismos (até 6)."));
        }
        if !plu.is_empty() {
            let outro: Option<String> = sqlx::query_scalar("SELECT descricao FROM artigos WHERE id <> ? AND ativo = 1 AND ltrim(codigo_balanca, '0') = ltrim(?, '0')")
                .bind(id)
                .bind(&plu)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(o) = outro {
                return Err(AppError::msg(format!("O código da balança {plu} já é do artigo «{o}».")));
            }
        }
        sqlx::query("UPDATE artigos SET codigo_balanca = ? WHERE id = ?").bind((!plu.is_empty()).then_some(&plu)).bind(id).execute(&mut *tx).await?;
    }
    if let Some(codigos) = &a.codigos {
        sqlx::query("DELETE FROM artigo_codigos WHERE artigo_id = ?").bind(id).execute(&mut *tx).await?;
        let mut vistos: Vec<String> = Vec::new();
        for c in codigos {
            let codigo = c.codigo.trim().to_string();
            if codigo.is_empty() {
                continue;
            }
            let q = c.quantidade.unwrap_or(1.0);
            if !q.is_finite() || q <= 0.0 || q > 1_000_000.0 {
                return Err(AppError::msg(format!("A quantidade da embalagem {codigo} tem de ser positiva.")));
            }
            if vistos.contains(&codigo) || cobar_txt.as_deref() == Some(codigo.as_str()) {
                return Err(AppError::msg(format!("O código {codigo} está repetido neste artigo.")));
            }
            let dono: Option<String> = sqlx::query_scalar(
                "SELECT descricao FROM artigos WHERE id <> ?1 AND (codigo_barras = ?2 OR referencia = ?2)
                 UNION ALL SELECT a.descricao FROM artigo_codigos c JOIN artigos a ON a.id = c.artigo_id WHERE c.codigo = ?2 AND c.artigo_id <> ?1 LIMIT 1",
            )
            .bind(id)
            .bind(&codigo)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(d) = dono {
                return Err(AppError::msg(format!("O código {codigo} já é do artigo «{d}».")));
            }
            sqlx::query("INSERT INTO artigo_codigos (artigo_id, codigo, quantidade, descricao) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(&codigo)
                .bind(q)
                .bind(c.descricao.as_deref().map(str::trim).filter(|d| !d.is_empty()))
                .execute(&mut *tx)
                .await?;
            vistos.push(codigo);
        }
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
    sqlx::query(
        "INSERT INTO moedas (codigo, descricao, simbolo, cambio, ativo) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(codigo) DO UPDATE SET descricao = ?2, simbolo = ?3, cambio = ?4, ativo = ?5",
    )
    .bind(&codigo)
    .bind(m.descricao.trim())
    .bind(m.simbolo.trim())
    .bind(m.cambio)
    .bind(m.ativo.unwrap_or(true) as i64)
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
        let mut tx = st.pool.begin().await?;
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

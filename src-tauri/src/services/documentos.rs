//! Emissão de documentos comerciais — o coração do sistema.
//! Um único motor para todos os tipos do KIGEST (vendas, compras, stock, recibos e pagamentos):
//! o comportamento vem dos atributos do tipo (`tipos_documento`). Numeração por série, assinatura
//! AGT encadeada, stock, conta corrente, retenções e caixa — tudo numa única transacção.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, Sqlite, Transaction};

use super::catalogo::IdIn;
use crate::agt;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LinhaIn {
    artigo_id: Option<i64>,
    descricao: Option<String>,
    /// Quantidade (no inventário: quantidade contada).
    quantidade: f64,
    /// Preço unitário SEM IVA em cêntimos (na moeda do documento). Se omitido, usa o preço do artigo.
    preco_unitario: Option<i64>,
    desconto_pct: Option<f64>,
    /// Lote (obrigatório nas entradas de artigos com controlo de lotes; nas saídas, se omitido, FEFO).
    lote: Option<String>,
    validade: Option<String>,
}

/// Desconto da promoção do artigo se `data` (AAAA-MM-DD) estiver dentro do período; 0 fora dele.
fn promocao_vigente(a: &sqlx::sqlite::SqliteRow, data: &str) -> f64 {
    let pct: f64 = a.try_get("promo_desconto").unwrap_or(0.0);
    if pct <= 0.0 {
        return 0.0;
    }
    let ini: Option<String> = a.try_get("promo_inicio").unwrap_or(None);
    let fim: Option<String> = a.try_get("promo_fim").unwrap_or(None);
    let depois_inicio = ini.as_deref().filter(|x| !x.is_empty()).is_none_or(|i| data >= i);
    let antes_fim = fim.as_deref().filter(|x| !x.is_empty()).is_none_or(|f| data <= f);
    if depois_inicio && antes_fim {
        pct.min(100.0)
    } else {
        0.0
    }
}

#[derive(Deserialize)]
pub struct CustoIn {
    tipo: String,
    descricao: Option<String>,
    /// Valor em Kz (cêntimos).
    valor: i64,
}

#[derive(Deserialize)]
pub struct PagamentoIn {
    modo: String,
    valor: i64,
}

#[derive(Deserialize)]
pub struct LiquidarIn {
    documento_id: i64,
    valor: i64,
}

#[derive(Deserialize)]
pub struct RetencaoIn {
    codigo: String,
    /// Se omitida, usa a taxa configurada.
    percentagem: Option<f64>,
}

#[derive(Deserialize)]
pub struct EmitirIn {
    tipo: String,
    entidade_id: Option<i64>,
    data_vencimento: Option<String>,
    armazem_id: Option<i64>,
    armazem_destino_id: Option<i64>,
    #[serde(default)]
    linhas: Vec<LinhaIn>,
    #[serde(default)]
    pagamentos: Vec<PagamentoIn>,
    #[serde(default)]
    liquidar: Vec<LiquidarIn>,
    #[serde(default)]
    retencoes: Vec<RetencaoIn>,
    desconto_global_pct: Option<f64>,
    /// Desconto global em valor (cêntimos), distribuído proporcionalmente pelas linhas.
    desconto_global_valor: Option<i64>,
    doc_origem_id: Option<i64>,
    observacoes: Option<String>,
    /// PIN de um supervisor quando o operador não tem permissão para preço/desconto.
    autorizacao_pin: Option<String>,
    /// N.º do documento do fornecedor (compras).
    doc_externo: Option<String>,
    /// 4 caracteres de controlo (assinatura) impressos no documento do fornecedor.
    doc_externo_hash: Option<String>,
    /// Data do documento do fornecedor (AAAA-MM-DD).
    doc_externo_data: Option<String>,
    /// Tipo de dedução do IVA para o SAF-T de aquisição (MFI, INV, OBC, SERV, IMPT, SCE).
    deducao_tipo: Option<String>,
    /// Motivo de emissão (notas de crédito / débito).
    motivo: Option<String>,
    /// V/ Referência (requisição, obra…).
    referencia: Option<String>,
    /// Prazo de pagamento (código de `prazos_pagamento`).
    prazo: Option<String>,
    /// Guias: { carga_local, carga_data, descarga_local, descarga_data, matricula, motorista }.
    transporte: Option<Value>,
    /// Moeda dos preços indicados (AOA por omissão) e câmbio (Kz por unidade).
    moeda: Option<String>,
    cambio: Option<f64>,
    /// Custos adicionais de compra (frete, desalfandegamento…), repartidos pelo custo dos artigos.
    #[serde(default)]
    custos: Vec<CustoIn>,
    /// Série escolhida (vazio = série por defeito do tipo ou série automática do posto).
    serie: Option<String>,
    /// Recuperação de um documento emitido à mão: { numero, data } (exige série de documentos manuais).
    manual: Option<ManualIn>,
    /// Valor do recibo de adiantamento.
    valor: Option<i64>,
    /// Vendedor (funcionário) e local de entrega.
    vendedor_id: Option<i64>,
    endereco_entrega: Option<String>,
}

#[derive(Deserialize)]
pub struct ManualIn {
    numero: String,
    data: String,
}

/// Escolhe a série do documento: a indicada, a série por defeito do tipo (em vigor) ou a automática do posto.
#[allow(clippy::too_many_arguments)]
async fn escolher_serie(
    tx: &mut Transaction<'_, Sqlite>,
    tipo: &str,
    posto_id: i64,
    prefixo: &str,
    data: &str,
    pedida: Option<&str>,
    manual: bool,
) -> AppResult<String> {
    // Facturação electrónica: os documentos comunicados à AGT usam séries atribuídas pela AGT.
    let fe = !manual
        && super::fe::TIPOS_FE.contains(&tipo)
        && sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = 'fe.ativo'").fetch_optional(&mut **tx).await?.as_deref() == Some("1");
    if let Some(sr) = pedida.map(str::trim).filter(|x| !x.is_empty()) {
        let row = sqlx::query("SELECT ativa, manual, data_inicio, data_fim, posto_id, fe_agt FROM series WHERE tipo_codigo = ? AND serie = ?")
            .bind(tipo)
            .bind(sr)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::msg(format!("A série {sr} não existe para {tipo}.")))?;
        if row.get::<i64, _>("ativa") == 0 {
            return Err(AppError::msg(format!("A série {sr} está bloqueada.")));
        }
        let e_manual = row.get::<i64, _>("manual") == 1;
        if manual && !e_manual {
            return Err(AppError::msg("Os documentos manuais só podem ser recuperados numa série de documentos manuais."));
        }
        if !manual && e_manual {
            return Err(AppError::msg(format!("A série {sr} é de documentos manuais.")));
        }
        if fe && row.get::<i64, _>("fe_agt") == 0 {
            return Err(AppError::msg(format!("A série {sr} não foi atribuída pela AGT. Com a facturação electrónica, use uma série pedida à AGT.")));
        }
        if row.get::<Option<i64>, _>("posto_id").is_some_and(|p| p != posto_id) {
            return Err(AppError::msg(format!("A série {sr} pertence a outro posto.")));
        }
        let ini: Option<String> = row.get("data_inicio");
        let fim: Option<String> = row.get("data_fim");
        if ini.as_deref().is_some_and(|d| data < d) || fim.as_deref().is_some_and(|d| data > d) {
            return Err(AppError::msg(format!("A série {sr} não está em vigor em {data}.")));
        }
        return Ok(sr.to_string());
    }
    if manual {
        return Err(AppError::msg("Seleccione a série de documentos manuais."));
    }
    let padrao: Option<String> = sqlx::query_scalar(
        "SELECT serie FROM series WHERE tipo_codigo = ?1 AND padrao = 1 AND ativa = 1 AND manual = 0
           AND (posto_id IS NULL OR posto_id = ?2) AND (data_inicio IS NULL OR data_inicio <= ?3) AND (data_fim IS NULL OR data_fim >= ?3)
           AND (?4 = 0 OR fe_agt = 1)
         ORDER BY posto_id IS NULL, id DESC LIMIT 1",
    )
    .bind(tipo)
    .bind(posto_id)
    .bind(data)
    .bind(fe as i64)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(sr) = padrao {
        return Ok(sr);
    }
    if fe {
        return Err(AppError::msg(format!(
            "Não há série da AGT em vigor para {tipo}. Peça-a em Sistema → Facturação Electrónica antes de emitir."
        )));
    }
    // Série automática = prefixo do posto + ano (ex.: "2026" ou "P22026"), como as séries por posto do KIGEST.
    let ano = &data[..4];
    let sr = format!("{prefixo}{ano}");
    sqlx::query(
        "INSERT OR IGNORE INTO series (tipo_codigo, serie, descricao, data_inicio, data_fim) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(tipo)
    .bind(&sr)
    .bind(format!("Automática {ano}"))
    .bind(format!("{ano}-01-01"))
    .bind(format!("{ano}-12-31"))
    .execute(&mut **tx)
    .await?;
    let ativa: i64 = sqlx::query_scalar("SELECT ativa FROM series WHERE tipo_codigo = ? AND serie = ?").bind(tipo).bind(&sr).fetch_one(&mut **tx).await?;
    if ativa == 0 {
        return Err(AppError::msg(format!("A série {sr} está bloqueada. Defina uma série por defeito em Séries de Documentos.")));
    }
    Ok(sr)
}

struct Tipo {
    codigo: String,
    descricao: String,
    classe: String,
    grupo: String,
    entidade: Option<String>,
    stock: i64,
    conta_corrente: i64,
    pago_no_acto: bool,
    assinado: bool,
    credito: bool,
    externo: bool,
    validade_dias: i64,
}

struct Linha {
    artigo_id: Option<i64>,
    referencia: String,
    descricao: String,
    quantidade: f64,
    unidade: Option<String>,
    preco: i64,
    desconto_pct: f64,
    valor_desconto: i64,
    taxa_id: Option<i64>,
    taxa_pct: f64,
    motivo: Option<String>,
    liquido: i64,
    iva: i64,
    total: i64,
    custo: i64,
    controla_stock: bool,
    permite_negativo: bool,
    controla_lote: bool,
    lote: Option<String>,
    validade: Option<String>,
    /// Kit: (componente, quantidade por unidade, permite stock negativo, controla stock)
    componentes: Vec<(i64, f64, bool, bool)>,
    /// Parte dos custos adicionais da compra imputada a esta linha (Kz, cêntimos).
    custo_extra: i64,
}

fn r(x: f64) -> i64 {
    x.round() as i64
}

fn txt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

async fn carregar_tipo(tx: &mut Transaction<'_, Sqlite>, codigo: &str) -> AppResult<Tipo> {
    let row = sqlx::query("SELECT * FROM tipos_documento WHERE codigo = ? AND ativo = 1")
        .bind(codigo)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::msg(format!("Tipo de documento inválido: {codigo}")))?;
    Ok(Tipo {
        codigo: row.get("codigo"),
        descricao: row.get("descricao"),
        classe: row.get("classe"),
        grupo: row.get("grupo"),
        entidade: row.get("entidade"),
        stock: row.get("stock"),
        conta_corrente: row.get("conta_corrente"),
        pago_no_acto: row.get::<i64, _>("pago_no_acto") == 1,
        assinado: row.get::<i64, _>("assinado") == 1,
        credito: row.get::<i64, _>("credito") == 1,
        externo: row.get::<i64, _>("externo") == 1,
        validade_dias: row.get("validade_dias"),
    })
}

/// Aplica um movimento de stock e devolve a existência resultante.
#[allow(clippy::too_many_arguments)]
async fn mover_stock(
    tx: &mut Transaction<'_, Sqlite>,
    artigo_id: i64,
    armazem_id: i64,
    delta: f64,
    custo_entrada: Option<i64>,
    documento_id: i64,
    descricao: &str,
    permite_negativo: bool,
    agora: &str,
) -> AppResult<f64> {
    let atual = existencia(tx, artigo_id, armazem_id).await?;
    let novo = atual + delta;
    if delta < 0.0 && novo < -1e-9 && !permite_negativo {
        return Err(AppError::msg(format!(
            "Stock insuficiente para \"{descricao}\": existência {atual}, pedido {}.",
            -delta
        )));
    }
    // Custo médio ponderado (global, como no KIGEST) nas entradas com custo.
    if let Some(custo) = custo_entrada.filter(|_| delta > 0.0) {
        let (total_qtd, cm): (f64, i64) = sqlx::query_as(
            "SELECT COALESCE((SELECT SUM(quantidade) FROM stock WHERE artigo_id = ?1), 0.0), custo_medio FROM artigos WHERE id = ?1",
        )
        .bind(artigo_id)
        .fetch_one(&mut **tx)
        .await?;
        let base = total_qtd.max(0.0);
        let novo_cm = if base + delta > 0.0 { r((base * cm as f64 + delta * custo as f64) / (base + delta)) } else { custo };
        sqlx::query("UPDATE artigos SET custo = ?, custo_medio = ? WHERE id = ?")
            .bind(custo)
            .bind(novo_cm)
            .bind(artigo_id)
            .execute(&mut **tx)
            .await?;
    }
    let (col_e, col_s) = if delta >= 0.0 { (Some(agora), None) } else { (None, Some(agora)) };
    sqlx::query(
        "INSERT INTO stock (artigo_id, armazem_id, quantidade, ultima_entrada, ultima_saida) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(artigo_id, armazem_id) DO UPDATE SET quantidade = ?3,
           ultima_entrada = COALESCE(?4, ultima_entrada), ultima_saida = COALESCE(?5, ultima_saida)",
    )
    .bind(artigo_id)
    .bind(armazem_id)
    .bind(novo)
    .bind(col_e)
    .bind(col_s)
    .execute(&mut **tx)
    .await?;
    let cm: i64 = sqlx::query_scalar("SELECT custo_medio FROM artigos WHERE id = ?").bind(artigo_id).fetch_one(&mut **tx).await?;
    sqlx::query(
        "INSERT INTO movimentos_stock (data_hora, artigo_id, armazem_id, quantidade, saldo, custo, documento_id, descricao)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(agora)
    .bind(artigo_id)
    .bind(armazem_id)
    .bind(delta)
    .bind(novo)
    .bind(cm)
    .bind(documento_id)
    .bind(descricao)
    .execute(&mut **tx)
    .await?;
    Ok(novo)
}

async fn existencia(tx: &mut Transaction<'_, Sqlite>, artigo_id: i64, armazem_id: i64) -> AppResult<f64> {
    Ok(sqlx::query_scalar("SELECT quantidade FROM stock WHERE artigo_id = ? AND armazem_id = ?")
        .bind(artigo_id)
        .bind(armazem_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(0.0))
}

fn qtd_txt(q: f64) -> String {
    let s = format!("{q:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Entrada num lote (cria-o se não existir) e regista o movimento.
#[allow(clippy::too_many_arguments)]
async fn lote_entrada(
    tx: &mut Transaction<'_, Sqlite>,
    artigo: i64,
    armazem: i64,
    qtd: f64,
    lote: &str,
    validade: Option<&str>,
    custo: i64,
    documento_id: i64,
    agora: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO lotes (artigo_id, armazem_id, lote, validade, quantidade, custo, ultima_entrada) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(artigo_id, armazem_id, lote) DO UPDATE SET quantidade = quantidade + ?5, validade = COALESCE(?4, validade),
           custo = CASE WHEN ?6 > 0 THEN ?6 ELSE custo END, ultima_entrada = ?7, ativo = 1",
    )
    .bind(artigo)
    .bind(armazem)
    .bind(lote)
    .bind(validade)
    .bind(qtd)
    .bind(custo)
    .bind(agora)
    .execute(&mut **tx)
    .await?;
    let id: i64 = sqlx::query_scalar("SELECT id FROM lotes WHERE artigo_id = ? AND armazem_id = ? AND lote = ?")
        .bind(artigo)
        .bind(armazem)
        .bind(lote)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO lote_movimentos (lote_id, documento_id, quantidade) VALUES (?, ?, ?)")
        .bind(id)
        .bind(documento_id)
        .bind(qtd)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Saída de lotes: o lote indicado, ou FEFO (primeiro o que expira mais cedo).
/// Devolve os lotes consumidos: (lote, validade, quantidade, custo).
async fn lote_saida(
    tx: &mut Transaction<'_, Sqlite>,
    artigo: i64,
    armazem: i64,
    qtd: f64,
    lote: Option<&str>,
    documento_id: i64,
    agora: &str,
) -> AppResult<Vec<(String, Option<String>, f64, i64)>> {
    let candidatos: Vec<(i64, String, Option<String>, f64, i64)> = match lote {
        Some(l) => {
            let v: Vec<(i64, String, Option<String>, f64, i64)> =
                sqlx::query_as("SELECT id, lote, validade, quantidade, custo FROM lotes WHERE artigo_id = ? AND armazem_id = ? AND lote = ?")
                    .bind(artigo)
                    .bind(armazem)
                    .bind(l)
                    .fetch_all(&mut **tx)
                    .await?;
            if v.is_empty() {
                return Err(AppError::msg(format!("O lote {l} não existe neste armazém.")));
            }
            v
        }
        None => {
            sqlx::query_as(
                "SELECT id, lote, validade, quantidade, custo FROM lotes WHERE artigo_id = ? AND armazem_id = ? AND quantidade > 0 AND ativo = 1
                 ORDER BY validade IS NULL, validade, id",
            )
            .bind(artigo)
            .bind(armazem)
            .fetch_all(&mut **tx)
            .await?
        }
    };
    let mut resto = qtd;
    let mut usados = Vec::new();
    for (id, nome, validade, disp, custo) in candidatos {
        if resto <= 1e-9 {
            break;
        }
        if lote.is_some() && disp + 1e-9 < resto {
            return Err(AppError::msg(format!("O lote {nome} só tem {} em stock.", qtd_txt(disp))));
        }
        let tira = resto.min(disp);
        if tira <= 0.0 {
            continue;
        }
        sqlx::query("UPDATE lotes SET quantidade = quantidade - ?, ultima_saida = ? WHERE id = ?").bind(tira).bind(agora).bind(id).execute(&mut **tx).await?;
        sqlx::query("INSERT INTO lote_movimentos (lote_id, documento_id, quantidade) VALUES (?, ?, ?)").bind(id).bind(documento_id).bind(-tira).execute(&mut **tx).await?;
        usados.push((nome, validade, tira, custo));
        resto -= tira;
    }
    Ok(usados)
}

fn lotes_txt(usados: &[(String, Option<String>, f64, i64)]) -> Option<String> {
    (!usados.is_empty()).then(|| usados.iter().map(|u| if usados.len() == 1 { u.0.clone() } else { format!("{} ({})", u.0, qtd_txt(u.2)) }).collect::<Vec<_>>().join(", "))
}

fn permissao_tipo(st: &AppState, tipo: &str) -> AppResult<i64> {
    let perm = match tipo {
        "FR" => "vendas.fatura_recibo",
        "FT" | "GT" => "vendas.fatura",
        "PP" | "OR" | "NE" => "vendas.proforma",
        "NC" => "vendas.nota_credito",
        "ND" => "vendas.nota_debito",
        "RC" => "tesouraria.recibo",
        "FC" | "VFR" | "VNC" | "VND" | "VNE" | "VOR" | "VGT" | "AF" => "compras.fatura",
        "VRC" => "tesouraria.pagamento",
        "ES" => "stock.entrada",
        "SS" | "CI" => "stock.saida",
        "TS" => "stock.transferencia",
        "IN" => "stock.inventario",
        _ => "sistema.utilizadores",
    };
    Ok(st.exigir_perm(perm)?.id)
}

pub async fn emitir(st: &AppState, i: EmitirIn) -> AppResult<Value> {
    let utilizador_id = permissao_tipo(st, &i.tipo)?;
    // Chave carregada fora da transacção (a primeira geração pode demorar).
    let chave = agt::chave(&st.pool).await?;
    let _guard = st.emissao.lock().await;

    let posto = st.posto();
    let mut tx = st.pool.begin().await?;
    let tipo = carregar_tipo(&mut tx, &i.tipo).await?;
    let agora = db::agora();
    let hoje = db::hoje();
    let armazem_id = i.armazem_id.unwrap_or(posto.armazem_id);
    let liquida = matches!(tipo.classe.as_str(), "RECIBO" | "PAGAMENTO");
    let adiantamento = tipo.classe == "ADIANTAMENTO";

    // ---------- documento manual (recuperação de documentos emitidos à mão) ----------
    let manual: Option<(String, String)> = match &i.manual {
        Some(m) => {
            st.exigir_perm("vendas.documento_manual")?;
            if !(tipo.assinado && tipo.grupo == "VENDAS" && matches!(tipo.classe.as_str(), "VENDA" | "TRANSPORTE")) {
                return Err(AppError::msg("Só facturas, notas de crédito e de débito e guias emitidas à mão podem ser recuperadas."));
            }
            let numero = m.numero.trim();
            if numero.is_empty() {
                return Err(AppError::msg("Indique o número do documento manual."));
            }
            let d = m.data.trim();
            if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() || d > hoje.as_str() {
                return Err(AppError::msg("Data do documento manual inválida."));
            }
            let dup: Option<String> = sqlx::query_scalar("SELECT numero_doc FROM documentos WHERE tipo_codigo = ? AND doc_manual = ? AND estado <> 'A'")
                .bind(&tipo.codigo)
                .bind(numero)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(n) = dup {
                return Err(AppError::msg(format!("O documento manual {numero} já foi recuperado em {n}.")));
            }
            Some((numero.to_string(), d.to_string()))
        }
        None => None,
    };
    // Data do documento: a do original manual, ou hoje.
    let data_doc = manual.as_ref().map(|m| m.1.clone()).unwrap_or_else(|| hoje.clone());
    let vendedor_id = match i.vendedor_id {
        Some(v) => {
            let ok: Option<i64> = sqlx::query_scalar("SELECT id FROM funcionarios WHERE id = ? AND ativo = 1").bind(v).fetch_optional(&mut *tx).await?;
            Some(ok.ok_or_else(|| AppError::msg("Vendedor inválido."))?)
        }
        None => None,
    };

    // ---------- moeda (preços indicados em moeda estrangeira são convertidos ao câmbio) ----------
    let moeda = i.moeda.as_deref().map(str::trim).filter(|m| !m.is_empty()).unwrap_or("AOA").to_uppercase();
    let cambio = if moeda == "AOA" {
        1.0
    } else {
        let c = match i.cambio.filter(|c| *c > 0.0) {
            Some(c) => c,
            None => sqlx::query_scalar::<_, f64>("SELECT cambio FROM moedas WHERE codigo = ?").bind(&moeda).fetch_optional(&mut *tx).await?.unwrap_or(0.0),
        };
        if c <= 0.0 {
            return Err(AppError::msg(format!("Indique o câmbio de {moeda} (Kz por unidade).")));
        }
        c
    };

    // ---------- entidade ----------
    let entidade_id = match tipo.entidade.as_deref() {
        None => None,
        Some("F") => Some(i.entidade_id.filter(|id| *id != 1).ok_or_else(|| AppError::msg("Seleccione o fornecedor."))?),
        _ => Some(i.entidade_id.unwrap_or(1)),
    };
    let (ent_nome, ent_nif, ent_end, ent_loc, nivel_preco, ent_desconto) = match entidade_id {
        Some(id) => {
            let row = sqlx::query("SELECT tipo, nome, nif, endereco, localidade, nivel_preco, desconto FROM entidades WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| AppError::msg("Entidade não encontrada."))?;
            let et: String = row.get("tipo");
            match tipo.entidade.as_deref() {
                Some("F") if et == "C" => return Err(AppError::msg("A entidade seleccionada não é um fornecedor.")),
                Some("C") if et == "F" => return Err(AppError::msg("A entidade seleccionada não é um cliente.")),
                _ => {}
            }
            (
                row.get::<String, _>("nome"),
                row.get::<String, _>("nif"),
                row.get::<Option<String>, _>("endereco"),
                row.get::<Option<String>, _>("localidade"),
                row.get::<i64, _>("nivel_preco"),
                row.get::<f64, _>("desconto"),
            )
        }
        None => {
            let (n, nif): (String, String) = sqlx::query_as("SELECT nome, nif FROM empresa WHERE id = 1").fetch_one(&mut *tx).await?;
            (n, nif, None, None, 1, 0.0)
        }
    };
    if tipo.conta_corrente == 1 && entidade_id == Some(1) {
        return Err(AppError::msg(format!("{} a crédito exige um cliente identificado (não Consumidor Final).", tipo.descricao)));
    }
    if adiantamento && entidade_id == Some(1) {
        return Err(AppError::msg("Identifique o cliente do adiantamento (não Consumidor Final)."));
    }
    if tipo.stock == 2 {
        match i.armazem_destino_id {
            Some(d) if d != armazem_id => {}
            _ => return Err(AppError::msg("Seleccione um armazém de destino diferente do de origem.")),
        }
    }
    let motivo = txt(&i.motivo);
    if (tipo.credito || matches!(tipo.codigo.as_str(), "ND" | "VND")) && motivo.is_none() {
        return Err(AppError::msg("Indique o motivo de emissão do documento."));
    }
    let doc_externo = txt(&i.doc_externo);
    if tipo.externo && tipo.classe == "COMPRA" && doc_externo.is_none() {
        return Err(AppError::msg("Indique o número do documento do fornecedor."));
    }
    let doc_externo_hash = txt(&i.doc_externo_hash);
    if let Some(h) = &doc_externo_hash {
        if h != "0" && h.chars().count() != 4 {
            return Err(AppError::msg("A assinatura do documento do fornecedor tem 4 caracteres (ou 0 quando não se aplica)."));
        }
    }
    let doc_externo_data = txt(&i.doc_externo_data);
    if let Some(d) = &doc_externo_data {
        if chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() || d.as_str() > db::hoje().as_str() {
            return Err(AppError::msg("Data do documento do fornecedor inválida."));
        }
    }
    let deducao_tipo = txt(&i.deducao_tipo).map(|t| t.to_uppercase());
    if let Some(t) = &deducao_tipo {
        if !matches!(t.as_str(), "MFI" | "INV" | "OBC" | "SERV" | "IMPT" | "SCE") {
            return Err(AppError::msg("Tipo de dedução do IVA inválido."));
        }
    }

    // ---------- documento de origem (notas de crédito / conversões) ----------
    struct Origem {
        id: i64,
        tipo: String,
        classe: String,
        grupo: String,
        credito: bool,
        entidade: i64,
        pendente: i64,
        pago: bool,
    }
    let mut origem: Option<Origem> = None;
    if let Some(oid) = i.doc_origem_id {
        let row = sqlx::query(
            "SELECT d.id, d.tipo_codigo, d.entidade_id, d.valor_pendente, d.estado, t.pago_no_acto, t.classe, t.grupo, t.credito
             FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo WHERE d.id = ?",
        )
        .bind(oid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::msg("Documento de origem não encontrado."))?;
        if row.get::<String, _>("estado") == "A" {
            return Err(AppError::msg("O documento de origem está anulado."));
        }
        origem = Some(Origem {
            id: row.get("id"),
            tipo: row.get("tipo_codigo"),
            classe: row.get("classe"),
            grupo: row.get("grupo"),
            credito: row.get::<i64, _>("credito") == 1,
            entidade: row.get::<Option<i64>, _>("entidade_id").unwrap_or(1),
            pendente: row.get("valor_pendente"),
            pago: row.get::<i64, _>("pago_no_acto") == 1,
        });
    }
    if tipo.credito {
        match &origem {
            Some(o) if !o.credito && o.grupo == tipo.grupo && matches!(o.classe.as_str(), "VENDA" | "COMPRA") && Some(o.entidade) == entidade_id => {}
            Some(_) => return Err(AppError::msg("A nota de crédito deve referir uma factura da mesma entidade.")),
            None => return Err(AppError::msg("A nota de crédito exige o documento de origem.")),
        }
    }

    // ---------- linhas ----------
    if adiantamento && !i.linhas.is_empty() {
        return Err(AppError::msg("O recibo de adiantamento não tem linhas: indique só o valor recebido."));
    }
    if !liquida && !adiantamento && i.linhas.is_empty() {
        return Err(AppError::msg("O documento não tem linhas."));
    }
    let desc_global = i.desconto_global_pct.unwrap_or(0.0).clamp(0.0, 100.0);
    let desc_global_valor = i.desconto_global_valor.unwrap_or(0).max(0);
    // Preço/desconto alterados em vendas exigem permissão (ou PIN de supervisor).
    // Documentos com origem (NC, conversão de proforma) herdam os valores já autorizados.
    let verificar = tipo.grupo == "VENDAS" && matches!(tipo.classe.as_str(), "VENDA" | "PROFORMA" | "TRANSPORTE") && i.doc_origem_id.is_none();
    let inventario = tipo.stock == 3;
    let mut preco_alterado = false;
    let mut desconto_extra = desc_global > 0.0 || desc_global_valor > 0;
    let mut linhas: Vec<Linha> = Vec::with_capacity(i.linhas.len());
    for l in &i.linhas {
        if (inventario && l.quantidade < 0.0) || (!inventario && l.quantidade <= 0.0) {
            return Err(AppError::msg("As quantidades têm de ser positivas."));
        }
        let aid = l.artigo_id.ok_or_else(|| AppError::msg("Linha sem artigo."))?;
        let a = sqlx::query(
            "SELECT a.*, t.percentagem, u.abreviatura FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id
             LEFT JOIN unidades u ON u.id = a.unidade_id WHERE a.id = ?",
        )
        .bind(aid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::msg(format!("Artigo {aid} não encontrado.")))?;
        let preco_artigo = match tipo.grupo.as_str() {
            "COMPRAS" => a.get::<i64, _>("custo"),
            "STOCK" => a.get::<i64, _>("custo_medio"),
            _ => a.get::<i64, _>(match nivel_preco {
                2 => "preco2",
                3 => "preco3",
                4 => "preco4",
                _ => "preco1",
            }),
        };
        let preco = match l.preco_unitario {
            Some(p) if moeda != "AOA" => r(p as f64 * cambio),
            Some(p) => p,
            None => preco_artigo,
        }
        .max(0);
        if preco != preco_artigo {
            preco_alterado = true;
        }
        // kit: o stock movimentado é o dos componentes
        let componentes: Vec<(i64, f64, bool, bool)> = if a.get::<i64, _>("composto") == 1 {
            sqlx::query_as::<_, (i64, f64, i64, i64)>(
                "SELECT c.componente_id, c.quantidade, ar.permite_negativo, ar.controla_stock
                 FROM artigo_componentes c JOIN artigos ar ON ar.id = c.componente_id WHERE c.artigo_id = ?",
            )
            .bind(aid)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|(id, q, neg, ctl)| (id, q, neg == 1, ctl == 1))
            .collect()
        } else {
            Vec::new()
        };
        let (taxa_id, taxa_pct, motivo_isencao) = if tipo.classe == "STOCK" {
            (None, 0.0, None)
        } else {
            (Some(a.get::<i64, _>("taxa_iva_id")), a.get::<f64, _>("percentagem"), a.get::<Option<String>, _>("motivo_isencao"))
        };
        // promoção do artigo em vigor na data do documento (só vendas): é o desconto por omissão
        // e, até esse valor, não precisa de autorização
        let promo = if tipo.grupo == "VENDAS" { promocao_vigente(&a, &data_doc) } else { 0.0 };
        // desconto de linha combinado com o desconto global
        let dl = l.desconto_pct.unwrap_or(promo).clamp(0.0, 100.0);
        if dl > ent_desconto.max(promo) + 1e-9 {
            desconto_extra = true;
        }
        let desconto_pct = 100.0 - (100.0 - dl) * (100.0 - desc_global) / 100.0;
        let bruto = r(l.quantidade * preco as f64);
        let valor_desconto = r(bruto as f64 * desconto_pct / 100.0);
        let liquido = bruto - valor_desconto;
        let iva = r(liquido as f64 * taxa_pct / 100.0);
        linhas.push(Linha {
            artigo_id: Some(aid),
            referencia: a.get("referencia"),
            descricao: l.descricao.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| a.get("descricao")),
            quantidade: l.quantidade,
            unidade: a.get("abreviatura"),
            preco,
            desconto_pct,
            valor_desconto,
            taxa_id,
            taxa_pct,
            motivo: if taxa_pct == 0.0 && tipo.classe != "STOCK" { motivo_isencao.or(Some("M00".into())) } else { None },
            liquido,
            iva,
            total: liquido + iva,
            custo: a.get("custo_medio"),
            controla_stock: a.get::<i64, _>("controla_stock") == 1 && a.get::<String, _>("tipo") == "P" && componentes.is_empty(),
            permite_negativo: a.get::<i64, _>("permite_negativo") == 1,
            controla_lote: a.get::<i64, _>("controla_lote") == 1,
            lote: l.lote.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from),
            validade: l.validade.as_deref().map(str::trim).filter(|s| s.len() == 10).map(String::from),
            componentes,
            custo_extra: 0,
        });
    }

    // ---------- desconto global em valor (distribuído pelas linhas) ----------
    if desc_global_valor > 0 && !linhas.is_empty() {
        let base: i64 = linhas.iter().map(|l| l.liquido).sum();
        if desc_global_valor > base {
            return Err(AppError::msg("O desconto global não pode ser superior ao total."));
        }
        let n = linhas.len();
        let mut restante = desc_global_valor;
        for (k, l) in linhas.iter_mut().enumerate() {
            let quota = if k + 1 == n { restante } else { r(desc_global_valor as f64 * l.liquido as f64 / base as f64) };
            let d = quota.min(l.liquido).min(restante);
            restante -= d;
            l.liquido -= d;
            l.valor_desconto += d;
            l.iva = r(l.liquido as f64 * l.taxa_pct / 100.0);
            l.total = l.liquido + l.iva;
            let bruto = l.liquido + l.valor_desconto;
            l.desconto_pct = if bruto > 0 { (l.valor_desconto as f64 * 1_000_000.0 / bruto as f64).round() / 10_000.0 } else { 0.0 };
        }
    }

    // ---------- custos adicionais de compra (repartidos pelo valor das linhas) ----------
    let custos: Vec<(String, Option<String>, i64)> = i.custos.iter().filter(|c| c.valor > 0).map(|c| (c.tipo.clone(), txt(&c.descricao), c.valor)).collect();
    if !custos.is_empty() {
        if tipo.grupo != "COMPRAS" {
            return Err(AppError::msg("Os custos adicionais só se aplicam a documentos de compra."));
        }
        let total_custos: i64 = custos.iter().map(|c| c.2).sum();
        let base: i64 = linhas.iter().map(|l| l.liquido).sum();
        if base > 0 {
            let n = linhas.len();
            let mut resto = total_custos;
            for (k, l) in linhas.iter_mut().enumerate() {
                let quota = if k + 1 == n { resto } else { r(total_custos as f64 * l.liquido as f64 / base as f64) };
                l.custo_extra = quota.min(resto);
                resto -= l.custo_extra;
            }
        }
    }

    // ---------- autorização (preço / desconto) ----------
    let mut autorizado_por: Option<i64> = None;
    if verificar && (preco_alterado || desconto_extra) {
        let mut precisa: Vec<&str> = Vec::new();
        if preco_alterado {
            precisa.push("pos.alterar_preco");
        }
        if desconto_extra {
            precisa.push("pos.desconto");
        }
        let max_desc = if desconto_extra { linhas.iter().map(|l| l.desconto_pct).fold(0.0, f64::max) } else { 0.0 };
        let s = st.sessao()?;
        let perms_ok = precisa.iter().all(|p| s.tem(p));
        let limite_ok = !desconto_extra || s.limite_desconto <= 0.0 || max_desc <= s.limite_desconto + 1e-9;
        if !(perms_ok && limite_ok) {
            let pin = i.autorizacao_pin.as_deref().unwrap_or("");
            if pin.trim().is_empty() {
                return Err(AppError::msg(if perms_ok {
                    format!("O desconto ({max_desc:.2}%) excede o seu limite. É necessária a autorização de um supervisor (PIN).")
                } else {
                    "Alteração de preço ou desconto: é necessária a autorização de um supervisor (PIN).".to_string()
                }));
            }
            match super::auth::supervisor_por_pin(st, pin, &precisa, max_desc).await? {
                Some(sup) => autorizado_por = Some(sup.id),
                None => return Err(AppError::msg("PIN de autorização inválido.")),
            }
        }
    }

    // ---------- liquidações (recibo de cliente / pagamento a fornecedor) ----------
    let mut liquidacoes: Vec<(i64, i64)> = Vec::new();
    if liquida {
        if i.liquidar.is_empty() {
            return Err(AppError::msg("Seleccione os documentos a liquidar."));
        }
        for lq in &i.liquidar {
            let (ent, pend, estado): (Option<i64>, i64, String) =
                sqlx::query_as("SELECT entidade_id, valor_pendente, estado FROM documentos WHERE id = ?")
                    .bind(lq.documento_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or_else(|| AppError::msg("Documento a liquidar não encontrado."))?;
            if ent != entidade_id || estado != "N" {
                return Err(AppError::msg("Documento a liquidar inválido para esta entidade."));
            }
            if lq.valor <= 0 || lq.valor > pend {
                return Err(AppError::msg("Valor a liquidar superior ao valor em dívida."));
            }
            liquidacoes.push((lq.documento_id, lq.valor));
        }
    }

    // ---------- totais ----------
    let (total_bruto, total_desconto, total_liquido, total_iva) = linhas.iter().fold((0, 0, 0, 0), |acc, l| {
        (acc.0 + l.liquido + l.valor_desconto, acc.1 + l.valor_desconto, acc.2 + l.liquido, acc.3 + l.iva)
    });
    let total: i64 = if liquida {
        liquidacoes.iter().map(|x| x.1).sum()
    } else if adiantamento {
        match i.valor {
            Some(v) if v > 0 => v,
            _ => return Err(AppError::msg("Indique o valor do adiantamento.")),
        }
    } else {
        total_liquido + total_iva
    };

    // ---------- retenção na fonte ----------
    let mut retencoes: Vec<(String, String, f64, i64, i64)> = Vec::new(); // codigo, descricao, %, base, valor
    for rt in &i.retencoes {
        let row = sqlx::query("SELECT * FROM retencoes WHERE codigo = ? AND ativo = 1")
            .bind(rt.codigo.trim())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::msg(format!("Retenção desconhecida: {}", rt.codigo)))?;
        let pct = rt.percentagem.unwrap_or_else(|| row.get("percentagem")).clamp(0.0, 100.0);
        let base = if row.get::<String, _>("base") == "IVA" { total_iva } else { total_liquido };
        retencoes.push((row.get("codigo"), row.get("descricao"), pct, base, r(base as f64 * pct / 100.0)));
    }
    let total_retencao: i64 = retencoes.iter().map(|x| x.4).sum();
    if total_retencao > total {
        return Err(AppError::msg("A retenção não pode exceder o total do documento."));
    }
    let a_pagar = total - total_retencao;

    // ---------- pagamentos ----------
    let mut pagamentos: Vec<(String, i64)> = i.pagamentos.iter().filter(|p| p.valor > 0).map(|p| (p.modo.clone(), p.valor)).collect();
    // Compensação de saldo (CS): usa adiantamentos e notas de crédito ainda por utilizar.
    let compensacao: i64 = pagamentos.iter().filter(|p| p.0 == "CS").map(|p| p.1).sum();
    let mut creditos_usar: Vec<(i64, i64)> = Vec::new();
    if compensacao > 0 {
        if !liquida {
            return Err(AppError::msg("A compensação de saldo só se usa em recibos de cliente e pagamentos a fornecedor."));
        }
        let disponiveis: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT d.id, d.credito_disponivel FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
             WHERE d.entidade_id = ? AND d.estado = 'N' AND d.credito_disponivel > 0 AND t.grupo = ? ORDER BY d.data_emissao, d.id",
        )
        .bind(entidade_id)
        .bind(&tipo.grupo)
        .fetch_all(&mut *tx)
        .await?;
        let disponivel: i64 = disponiveis.iter().map(|x| x.1).sum();
        if compensacao > disponivel {
            return Err(AppError::msg(format!("Crédito disponível insuficiente para compensar: {}.", agt::total_fmt(disponivel))));
        }
        let mut resto = compensacao;
        for (id, disp) in disponiveis {
            if resto == 0 {
                break;
            }
            let usa = resto.min(disp);
            creditos_usar.push((id, usa));
            resto -= usa;
        }
    }
    let entregue: i64 = pagamentos.iter().map(|p| p.1).sum();
    let mut troco = 0;
    if tipo.pago_no_acto {
        if entregue < a_pagar {
            return Err(AppError::msg(format!("Valor pago insuficiente. Falta {}.", agt::total_fmt(a_pagar - entregue))));
        }
        troco = entregue - a_pagar;
        if troco > 0 {
            match pagamentos.iter_mut().find(|p| p.0 == "NU") {
                Some(p) if p.1 >= troco => p.1 -= troco,
                _ => return Err(AppError::msg("O troco só pode ser dado em numerário.")),
            }
        }
    } else {
        pagamentos.clear();
    }
    let valor_pendente = if tipo.conta_corrente == 1 { a_pagar } else { 0 };

    // ---------- limite de crédito do cliente ----------
    if tipo.conta_corrente == 1 && tipo.grupo == "VENDAS" && manual.is_none() {
        if let Some(eid) = entidade_id.filter(|e| *e != 1) {
            let (limite, saldo): (i64, i64) = sqlx::query_as(
                "SELECT e.limite_credito, COALESCE((SELECT SUM(d.valor_pendente) - SUM(d.credito_disponivel) FROM documentos d
                                                    WHERE d.entidade_id = e.id AND d.estado = 'N'), 0)
                 FROM entidades e WHERE e.id = ?",
            )
            .bind(eid)
            .fetch_one(&mut *tx)
            .await?;
            if limite > 0 && saldo + a_pagar > limite {
                let s = st.sessao()?;
                if !s.tem("vendas.limite_credito") {
                    let pin = i.autorizacao_pin.as_deref().unwrap_or("");
                    let msg = format!(
                        "Limite de crédito excedido: saldo {} + documento {} > limite {}.",
                        agt::total_fmt(saldo),
                        agt::total_fmt(a_pagar),
                        agt::total_fmt(limite)
                    );
                    if pin.trim().is_empty() {
                        return Err(AppError::msg(format!("{msg} É necessária a autorização de um supervisor (PIN).")));
                    }
                    match super::auth::supervisor_por_pin(st, pin, &["vendas.limite_credito"], 0.0).await? {
                        Some(sup) => autorizado_por = autorizado_por.or(Some(sup.id)),
                        None => return Err(AppError::msg("PIN de autorização inválido.")),
                    }
                }
            }
        }
    }

    // ---------- numeração + assinatura ----------
    let serie = escolher_serie(&mut tx, &tipo.codigo, posto.id, &posto.serie_prefixo, &data_doc, i.serie.as_deref(), manual.is_some()).await?;
    let (ultimo, hash_ant, data_ultimo): (i64, String, Option<String>) =
        sqlx::query_as("SELECT ultimo_numero, ultimo_hash, data_ultimo FROM series WHERE tipo_codigo = ? AND serie = ?")
            .bind(&tipo.codigo)
            .bind(&serie)
            .fetch_one(&mut *tx)
            .await?;
    // Ordem cronológica dentro da série (requisito da AGT).
    if let Some(u) = data_ultimo.as_deref().filter(|u| data_doc.as_str() < *u) {
        return Err(AppError::msg(format!("A data do documento ({data_doc}) é anterior à do último documento da série {serie} ({u}).")));
    }
    let numero = ultimo + 1;
    let numero_doc = format!("{} {}/{}", tipo.codigo, serie, numero);
    let (hash, controlo) = if tipo.assinado {
        let h = agt::assinar(&chave, &agt::mensagem(&data_doc, &agora, &numero_doc, total, &hash_ant));
        let c = agt::controlo(&h);
        (h, c)
    } else {
        (String::new(), String::new())
    };
    sqlx::query("UPDATE series SET ultimo_numero = ?, ultimo_hash = ?, data_ultimo = ? WHERE tipo_codigo = ? AND serie = ?")
        .bind(numero)
        .bind(if tipo.assinado { &hash } else { &hash_ant })
        .bind(&data_doc)
        .bind(&tipo.codigo)
        .bind(&serie)
        .execute(&mut *tx)
        .await?;

    // Dinheiro recebido no POS fica associado ao caixa aberto do posto (não nos documentos manuais recuperados).
    let sessao_caixa: Option<i64> = if manual.is_none() && tipo.grupo == "VENDAS" && (tipo.pago_no_acto || tipo.credito) {
        sqlx::query_scalar("SELECT id FROM caixa_sessoes WHERE posto_id = ? AND estado = 'ABERTA' ORDER BY id DESC LIMIT 1")
            .bind(posto.id)
            .fetch_optional(&mut *tx)
            .await?
    } else {
        None
    };

    let transporte = i.transporte.as_ref().filter(|v| v.is_object()).map(|v| v.to_string());
    let doc_id = sqlx::query(
        "INSERT INTO documentos (tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, data_vencimento,
            entidade_id, entidade_nome, entidade_nif, entidade_endereco, entidade_localidade, utilizador_id, posto_id, armazem_id,
            armazem_destino_id, doc_origem_id, total_bruto, total_desconto, total_liquido, total_iva, total_retencao, total,
            valor_entregue, troco, valor_pendente, hash, hash_controlo, caixa_sessao_id, observacoes, autorizado_por,
            doc_externo, motivo, referencia, prazo, transporte, moeda, cambio, total_moeda)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&tipo.codigo)
    .bind(&serie)
    .bind(numero)
    .bind(&numero_doc)
    .bind(&data_doc)
    .bind(&agora)
    .bind(i.data_vencimento.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| somar_dias(&data_doc, tipo.validade_dias)))
    .bind(entidade_id)
    .bind(&ent_nome)
    .bind(&ent_nif)
    .bind(&ent_end)
    .bind(&ent_loc)
    .bind(utilizador_id)
    .bind(posto.id)
    .bind(armazem_id)
    .bind(i.armazem_destino_id)
    .bind(i.doc_origem_id)
    .bind(total_bruto)
    .bind(total_desconto)
    .bind(total_liquido)
    .bind(total_iva)
    .bind(total_retencao)
    .bind(total)
    .bind(entregue)
    .bind(troco)
    .bind(valor_pendente)
    .bind(&hash)
    .bind(&controlo)
    .bind(sessao_caixa)
    .bind(txt(&i.observacoes))
    .bind(autorizado_por)
    .bind(&doc_externo)
    .bind(&motivo)
    .bind(txt(&i.referencia))
    .bind(txt(&i.prazo))
    .bind(&transporte)
    .bind(&moeda)
    .bind(cambio)
    .bind(if moeda == "AOA" { None } else { Some(r(total as f64 / cambio)) })
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("UPDATE documentos SET origem = ?, doc_manual = ?, credito_disponivel = ?, vendedor_id = ?, endereco_entrega = ? WHERE id = ?")
        .bind(if manual.is_some() { "M" } else { "P" })
        .bind(manual.as_ref().map(|m| m.0.clone()))
        .bind(if adiantamento { total } else { 0 })
        .bind(vendedor_id)
        .bind(txt(&i.endereco_entrega))
        .bind(doc_id)
        .execute(&mut *tx)
        .await?;
    for (cid, valor) in &creditos_usar {
        sqlx::query("INSERT INTO credito_utilizacoes (documento_id, credito_id, valor) VALUES (?, ?, ?)").bind(doc_id).bind(cid).bind(valor).execute(&mut *tx).await?;
        sqlx::query("UPDATE documentos SET credito_disponivel = credito_disponivel - ? WHERE id = ?").bind(valor).bind(cid).execute(&mut *tx).await?;
    }
    if tipo.grupo == "COMPRAS" && (doc_externo_hash.is_some() || doc_externo_data.is_some() || deducao_tipo.is_some()) {
        sqlx::query("UPDATE documentos SET doc_externo_hash = ?, doc_externo_data = ?, deducao_tipo = ? WHERE id = ?")
            .bind(&doc_externo_hash)
            .bind(&doc_externo_data)
            .bind(&deducao_tipo)
            .bind(doc_id)
            .execute(&mut *tx)
            .await?;
    }

    for (n, l) in linhas.iter().enumerate() {
        let linha_id = sqlx::query(
            "INSERT INTO documento_linhas (documento_id, linha, artigo_id, referencia, descricao, quantidade, unidade,
                preco_unitario, desconto_pct, valor_desconto, taxa_iva_id, taxa_iva_pct, motivo_isencao, valor_liquido, valor_iva, total, custo)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(doc_id)
        .bind(n as i64 + 1)
        .bind(l.artigo_id)
        .bind(&l.referencia)
        .bind(&l.descricao)
        .bind(l.quantidade)
        .bind(&l.unidade)
        .bind(l.preco)
        .bind(l.desconto_pct)
        .bind(l.valor_desconto)
        .bind(l.taxa_id)
        .bind(l.taxa_pct)
        .bind(&l.motivo)
        .bind(l.liquido)
        .bind(l.iva)
        .bind(l.total)
        .bind(l.custo)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();

        // ---------- stock (artigo, componentes de kits e lotes) ----------
        let mut usados_lote: Vec<(String, Option<String>, f64, i64)> = Vec::new();
        if tipo.stock != 0 {
            let txt_mov = format!("{} — {}", numero_doc, l.descricao);
            // (artigo, quantidade, permite stock negativo, é o artigo da própria linha)
            let alvos: Vec<(i64, f64, bool, bool)> = if !l.componentes.is_empty() {
                if tipo.stock == 3 {
                    Vec::new()
                } else {
                    l.componentes.iter().filter(|c| c.3).map(|c| (c.0, l.quantidade * c.1, c.2, false)).collect()
                }
            } else if l.controla_stock {
                vec![(l.artigo_id.unwrap_or_default(), l.quantidade, l.permite_negativo, true)]
            } else {
                Vec::new()
            };
            for (aid, qtd, neg, principal) in alvos {
                let com_lote = principal && l.controla_lote;
                match tipo.stock {
                    1 => {
                        // entradas com custo (compras e documento de stock) actualizam o custo médio
                        let custo = if principal && (tipo.classe == "COMPRA" || tipo.codigo == "ES") {
                            Some(if qtd > 0.0 { r((l.liquido + l.custo_extra) as f64 / qtd) } else { l.preco })
                        } else {
                            None
                        };
                        mover_stock(&mut tx, aid, armazem_id, qtd, custo, doc_id, &txt_mov, true, &agora).await?;
                        if com_lote {
                            // Devolução sem lote indicado (ex.: POS): volta ao lote da factura de origem.
                            let do_origem: Option<(String, Option<String>)> = match (&l.lote, &origem) {
                                (None, Some(o)) => sqlx::query_as::<_, (String, Option<String>)>(
                                    "SELECT lote, validade FROM documento_linhas WHERE documento_id = ? AND artigo_id = ? AND lote IS NOT NULL ORDER BY linha LIMIT 1",
                                )
                                .bind(o.id)
                                .bind(aid)
                                .fetch_optional(&mut *tx)
                                .await?
                                .map(|(t, v)| (t.split(" (").next().unwrap_or(&t).split(", ").next().unwrap_or("").trim().to_string(), v))
                                .filter(|(t, _)| !t.is_empty()),
                                _ => None,
                            };
                            let (lote, validade) = match (&l.lote, do_origem) {
                                (Some(x), _) => (x.clone(), l.validade.clone()),
                                (None, Some((x, v))) => (x, l.validade.clone().or(v)),
                                (None, None) => return Err(AppError::msg(format!("Indique o lote de \"{}\" (artigo com controlo de lotes).", l.descricao))),
                            };
                            lote_entrada(&mut tx, aid, armazem_id, qtd, &lote, validade.as_deref(), custo.unwrap_or(l.custo), doc_id, &agora).await?;
                            usados_lote.push((lote, validade, qtd, 0));
                        }
                    }
                    -1 => {
                        mover_stock(&mut tx, aid, armazem_id, -qtd, None, doc_id, &l.descricao, neg, &agora).await?;
                        if com_lote {
                            usados_lote = lote_saida(&mut tx, aid, armazem_id, qtd, l.lote.as_deref(), doc_id, &agora).await?;
                        }
                    }
                    2 => {
                        let dest = i.armazem_destino_id.unwrap_or_default();
                        mover_stock(&mut tx, aid, armazem_id, -qtd, None, doc_id, &l.descricao, neg, &agora).await?;
                        mover_stock(&mut tx, aid, dest, qtd, None, doc_id, &txt_mov, true, &agora).await?;
                        if com_lote {
                            usados_lote = lote_saida(&mut tx, aid, armazem_id, qtd, l.lote.as_deref(), doc_id, &agora).await?;
                            for (nome, validade, q, custo) in &usados_lote {
                                lote_entrada(&mut tx, aid, dest, *q, nome, validade.as_deref(), *custo, doc_id, &agora).await?;
                            }
                        }
                    }
                    3 => {
                        // inventário: acerta a existência para a quantidade contada.
                        // Com lote indicado, a contagem é desse lote e o stock do artigo acerta pela diferença do lote.
                        if let (true, Some(lote)) = (com_lote, l.lote.as_deref()) {
                            let no_lote: f64 = sqlx::query_scalar("SELECT quantidade FROM lotes WHERE artigo_id = ? AND armazem_id = ? AND lote = ?")
                                .bind(aid)
                                .bind(armazem_id)
                                .bind(lote)
                                .fetch_optional(&mut *tx)
                                .await?
                                .unwrap_or(0.0);
                            let delta = qtd - no_lote;
                            if delta.abs() > 1e-9 {
                                mover_stock(&mut tx, aid, armazem_id, delta, None, doc_id, &txt_mov, true, &agora).await?;
                                lote_entrada(&mut tx, aid, armazem_id, delta, lote, l.validade.as_deref(), 0, doc_id, &agora).await?;
                            }
                            usados_lote.push((lote.to_string(), l.validade.clone(), qtd, 0));
                        } else {
                            let atual = existencia(&mut tx, aid, armazem_id).await?;
                            let delta = qtd - atual;
                            if delta.abs() > 1e-9 {
                                mover_stock(&mut tx, aid, armazem_id, delta, None, doc_id, &txt_mov, true, &agora).await?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(t_lotes) = lotes_txt(&usados_lote) {
            let validade = if usados_lote.len() == 1 { usados_lote[0].1.clone() } else { None };
            sqlx::query("UPDATE documento_linhas SET lote = ?, validade = ? WHERE id = ?")
                .bind(t_lotes)
                .bind(validade)
                .bind(linha_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for (modo, valor) in &pagamentos {
        sqlx::query("INSERT INTO documento_pagamentos (documento_id, modo_codigo, valor) VALUES (?, ?, ?)")
            .bind(doc_id)
            .bind(modo)
            .bind(valor)
            .execute(&mut *tx)
            .await?;
    }
    for (codigo, descricao, pct, base, valor) in &retencoes {
        sqlx::query("INSERT INTO documento_retencoes (documento_id, codigo, descricao, percentagem, base, valor) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(doc_id)
            .bind(codigo)
            .bind(descricao)
            .bind(pct)
            .bind(base)
            .bind(valor)
            .execute(&mut *tx)
            .await?;
    }

    for (tipo_custo, descricao, valor) in &custos {
        sqlx::query("INSERT INTO documento_custos (documento_id, tipo, descricao, valor) VALUES (?, ?, ?, ?)")
            .bind(doc_id)
            .bind(tipo_custo)
            .bind(descricao)
            .bind(valor)
            .execute(&mut *tx)
            .await?;
    }

    // ---------- conta corrente ----------
    for (did, valor) in &liquidacoes {
        sqlx::query("INSERT INTO recibo_documentos (recibo_id, documento_id, valor) VALUES (?, ?, ?)")
            .bind(doc_id)
            .bind(did)
            .bind(valor)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE documentos SET valor_pendente = valor_pendente - ? WHERE id = ?")
            .bind(valor)
            .bind(did)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(o) = &origem {
        if tipo.credito {
            let abate = o.pendente.min(total);
            if abate > 0 {
                sqlx::query("INSERT INTO recibo_documentos (recibo_id, documento_id, valor) VALUES (?, ?, ?)")
                    .bind(doc_id)
                    .bind(o.id)
                    .bind(abate)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE documentos SET valor_pendente = valor_pendente - ? WHERE id = ?")
                    .bind(abate)
                    .bind(o.id)
                    .execute(&mut *tx)
                    .await?;
            }
            // Devolução em dinheiro de uma venda a pronto (POS com caixa aberto).
            let mut devolvido = 0;
            if o.pago && tipo.grupo == "VENDAS" {
                if let Some(sid) = sessao_caixa {
                    let cm = sqlx::query("INSERT INTO caixa_movimentos (sessao_id, data_hora, tipo, valor, descricao) VALUES (?, ?, 'SAIDA', ?, ?)")
                        .bind(sid)
                        .bind(&agora)
                        .bind(total - abate)
                        .bind(format!("Devolução {numero_doc}"))
                        .execute(&mut *tx)
                        .await?
                        .last_insert_rowid();
                    devolvido = total - abate;
                    sqlx::query("INSERT INTO credito_devolucoes (credito_id, caixa_movimento_id, valor, data) VALUES (?, ?, ?, ?)")
                        .bind(doc_id)
                        .bind(cm)
                        .bind(devolvido)
                        .bind(&data_doc)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            // O que não abateu à dívida nem foi devolvido fica como crédito a favor da entidade.
            let credito = (total - abate - devolvido).max(0);
            if credito > 0 {
                sqlx::query("UPDATE documentos SET credito_disponivel = ? WHERE id = ?").bind(credito).bind(doc_id).execute(&mut *tx).await?;
            }
        } else if matches!(o.classe.as_str(), "PROFORMA" | "COMPRA_DOC" | "TRANSPORTE") {
            // proforma / orçamento / encomenda / guia convertidos em factura
            sqlx::query("UPDATE documentos SET estado = 'F' WHERE id = ?").bind(o.id).execute(&mut *tx).await?;
        }
        let _ = &o.tipo;
    }

    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'EMITIR', ?)")
        .bind(utilizador_id)
        .bind(&numero_doc)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    drop(_guard);

    obter(st, IdIn { id: doc_id }).await
}

#[derive(Deserialize)]
pub struct Filtro {
    tipo: Option<String>,
    classe: Option<String>,
    grupo: Option<String>,
    de: Option<String>,
    ate: Option<String>,
    q: Option<String>,
    entidade_id: Option<i64>,
    estado: Option<String>,
    limite: Option<i64>,
}

pub async fn listar(st: &AppState, f: Filtro) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_desc, t.classe, t.grupo, d.data_emissao, d.data_hora,
                d.data_vencimento, d.entidade_id, d.entidade_nome, d.entidade_nif, d.total_liquido, d.total_iva, d.total,
                d.total_retencao, d.valor_pendente, d.estado, d.hash_controlo, d.doc_externo, d.importado, u.nome AS utilizador
         FROM documentos d
         JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         JOIN utilizadores u ON u.id = d.utilizador_id
         WHERE (?1 IS NULL OR d.tipo_codigo = ?1)
           AND (?2 IS NULL OR t.classe = ?2)
           AND (?3 IS NULL OR d.data_emissao >= ?3)
           AND (?4 IS NULL OR d.data_emissao <= ?4)
           AND (d.numero_doc LIKE ?5 OR d.entidade_nome LIKE ?5 OR d.entidade_nif LIKE ?5 OR IFNULL(d.doc_externo, '') LIKE ?5)
           AND (?6 IS NULL OR d.entidade_id = ?6)
           AND (?8 IS NULL OR d.estado = ?8)
           AND (?9 IS NULL OR t.grupo = ?9)
         ORDER BY d.data_hora DESC, d.id DESC LIMIT ?7",
    )
    .bind(f.tipo.filter(|s| !s.is_empty()))
    .bind(f.classe.filter(|s| !s.is_empty()))
    .bind(f.de.filter(|s| !s.is_empty()))
    .bind(f.ate.filter(|s| !s.is_empty()))
    .bind(format!("%{}%", f.q.unwrap_or_default().trim()))
    .bind(f.entidade_id)
    .bind(f.limite.unwrap_or(1000))
    .bind(f.estado.filter(|s| !s.is_empty()))
    .bind(f.grupo.filter(|s| !s.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let p = &st.pool;
    let doc = sqlx::query(
        "SELECT d.*, t.descricao AS tipo_desc, t.classe, t.grupo, t.credito, t.externo, t.vias, u.nome AS utilizador,
                t.validade_dias, t.prefixo_extenso, t.texto_rodape, t.formato,
                o.numero_doc AS doc_origem, a.nome AS armazem, ad.nome AS armazem_destino, ua.nome AS autorizado_por_nome,
                pz.descricao AS prazo_desc
         FROM documentos d
         JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         JOIN utilizadores u ON u.id = d.utilizador_id
         LEFT JOIN utilizadores ua ON ua.id = d.autorizado_por
         LEFT JOIN documentos o ON o.id = d.doc_origem_id
         LEFT JOIN armazens a ON a.id = d.armazem_id
         LEFT JOIN armazens ad ON ad.id = d.armazem_destino_id
         LEFT JOIN prazos_pagamento pz ON pz.codigo = d.prazo
         WHERE d.id = ?",
    )
    .bind(i.id)
    .fetch_optional(p)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let linhas = sqlx::query("SELECT * FROM documento_linhas WHERE documento_id = ? ORDER BY linha").bind(i.id).fetch_all(p).await?;
    let pags = sqlx::query(
        "SELECT p.modo_codigo, m.descricao, p.valor FROM documento_pagamentos p
         JOIN modos_pagamento m ON m.codigo = p.modo_codigo WHERE p.documento_id = ?",
    )
    .bind(i.id)
    .fetch_all(p)
    .await?;
    let liq = sqlx::query(
        "SELECT r.valor, d.id, d.numero_doc, d.data_emissao, d.total FROM recibo_documentos r
         JOIN documentos d ON d.id = r.documento_id WHERE r.recibo_id = ?",
    )
    .bind(i.id)
    .fetch_all(p)
    .await?;
    let ret = sqlx::query("SELECT codigo, descricao, percentagem, base, valor FROM documento_retencoes WHERE documento_id = ? ORDER BY id")
        .bind(i.id)
        .fetch_all(p)
        .await?;
    let custos = sqlx::query("SELECT c.*, t.descricao AS tipo_desc FROM documento_custos c JOIN tipos_custo t ON t.codigo = c.tipo WHERE c.documento_id = ? ORDER BY c.id")
        .bind(i.id)
        .fetch_all(p)
        .await?;
    // resumo de IVA por taxa (quadro obrigatório na factura)
    let iva = sqlx::query(
        "SELECT taxa_iva_pct AS taxa, motivo_isencao, SUM(valor_liquido) AS base, SUM(valor_iva) AS iva
         FROM documento_linhas WHERE documento_id = ? GROUP BY taxa_iva_pct, motivo_isencao ORDER BY taxa_iva_pct DESC",
    )
    .bind(i.id)
    .fetch_all(p)
    .await?;
    let empresa = super::setup::empresa_obter(st).await?;
    let mut documento = db::row_json(&doc);
    if let Some(t) = documento.get("transporte").and_then(|t| t.as_str()).and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        documento["transporte"] = t;
    }
    // Facturação electrónica: estado da comunicação à AGT (o impresso mostra o QR code da AGT).
    let tipo_doc = documento.get("tipo_codigo").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let fe_info = if super::fe::ativo(p).await? && super::fe::TIPOS_FE.contains(&tipo_doc.as_str()) && documento.get("importado").and_then(|x| x.as_i64()) == Some(0) {
        let est: Option<String> = sqlx::query_scalar("SELECT estado FROM fe_envios WHERE documento_id = ?").bind(i.id).fetch_optional(p).await?;
        let nif: Option<String> = sqlx::query_scalar("SELECT nif FROM empresa WHERE id = 1").fetch_optional(p).await?;
        json!({ "estado": est.unwrap_or_else(|| "PENDENTE".into()), "nif": nif.unwrap_or_default() })
    } else {
        Value::Null
    };
    Ok(json!({
        "documento": documento,
        "linhas": db::rows_json(&linhas),
        "pagamentos": db::rows_json(&pags),
        "liquidacoes": db::rows_json(&liq),
        "retencoes": db::rows_json(&ret),
        "custos": db::rows_json(&custos),
        "resumo_iva": db::rows_json(&iva),
        "empresa": empresa,
        "opcoes": super::parametros::opcoes(p).await?,
        "fe": fe_info,
    }))
}

/// Data ISO + n dias (validade dos documentos por defeito).
fn somar_dias(data: &str, dias: i64) -> String {
    if dias <= 0 {
        return data.to_string();
    }
    chrono::NaiveDate::parse_from_str(data, "%Y-%m-%d")
        .map(|d| (d + chrono::Duration::days(dias)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| data.to_string())
}

#[derive(Deserialize)]
pub struct AnularIn {
    id: i64,
    motivo: String,
}

pub async fn anular(st: &AppState, a: AnularIn) -> AppResult<Value> {
    let s = st.exigir_perm("documentos.anular")?;
    if a.motivo.trim().len() < 5 {
        return Err(AppError::msg("Indique o motivo da anulação."));
    }
    let _guard = st.emissao.lock().await;
    let mut tx = st.pool.begin().await?;
    let row = sqlx::query("SELECT numero_doc, estado, importado, tipo_codigo, data_emissao FROM documentos WHERE id = ?")
        .bind(a.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NaoEncontrado)?;
    let numero: String = row.get("numero_doc");
    if row.get::<String, _>("estado") == "A" {
        return Err(AppError::msg("O documento já está anulado."));
    }
    if row.get::<i64, _>("importado") == 1 {
        return Err(AppError::msg("Os documentos importados do KIGEST não podem ser anulados no sistema novo. Emita uma nota de crédito."));
    }
    // Facturação electrónica: o documento comunicado (ou em fila) à AGT não se anula só no KanGest,
    // senão o KanGest e a AGT ficariam diferentes. Os rejeitados pela AGT podem ser anulados.
    let tipo_doc: String = row.get("tipo_codigo");
    if super::fe::TIPOS_FE.contains(&tipo_doc.as_str()) {
        let fe = sqlx::query_as::<_, (String, String)>(
            "SELECT (SELECT valor FROM config WHERE chave = 'fe.ativo'), COALESCE((SELECT valor FROM config WHERE chave = 'fe.desde'), '')",
        )
        .fetch_one(&mut *tx)
        .await?;
        let data_doc: String = row.get("data_emissao");
        let estado_fe: Option<String> = sqlx::query_scalar("SELECT estado FROM fe_envios WHERE documento_id = ?").bind(a.id).fetch_optional(&mut *tx).await?;
        let comunicado = match estado_fe.as_deref() {
            Some("INVALIDO") => false,
            Some(_) => true,
            None => fe.0 == "1" && !fe.1.is_empty() && data_doc >= fe.1,
        };
        if comunicado {
            return Err(AppError::msg(if tipo_doc == "RC" {
                "Este recibo foi comunicado à AGT (facturação electrónica) e não pode ser anulado só no KanGest. Regularize com a AGT ou emita os documentos de correcção."
            } else {
                "Este documento foi comunicado à AGT (facturação electrónica) e não pode ser anulado só no KanGest. Corrija-o com uma nota de crédito, que também é comunicada."
            }));
        }
    }
    let liquidado: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recibo_documentos r JOIN documentos d ON d.id = r.recibo_id WHERE r.documento_id = ? AND d.estado = 'N'")
        .bind(a.id)
        .fetch_one(&mut *tx)
        .await?;
    if liquidado > 0 {
        return Err(AppError::msg("O documento tem recibos ou notas de crédito associados. Anule-os primeiro."));
    }
    let usado: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM credito_utilizacoes u JOIN documentos r ON r.id = u.documento_id WHERE u.credito_id = ?1 AND r.estado = 'N')
              + (SELECT COUNT(*) FROM credito_devolucoes v LEFT JOIN movimentos_tesouraria m ON m.id = v.movimento_id
                  WHERE v.credito_id = ?1 AND (v.movimento_id IS NULL OR m.anulado = 0))
              + (SELECT COUNT(*) FROM cc_regularizacao_docs x JOIN cc_regularizacoes g ON g.id = x.regularizacao_id WHERE x.documento_id = ?1 AND g.anulado = 0)",
    )
    .bind(a.id)
    .fetch_one(&mut *tx)
    .await?;
    if usado > 0 {
        return Err(AppError::msg("O crédito ou a dívida deste documento já foi movimentado (compensação, devolução ou regularização). Anule primeiro esse movimento."));
    }
    // recibo pago com compensação: devolve o crédito utilizado
    let usados: Vec<(i64, i64)> = sqlx::query_as("SELECT credito_id, valor FROM credito_utilizacoes WHERE documento_id = ?").bind(a.id).fetch_all(&mut *tx).await?;
    for (cid, valor) in usados {
        sqlx::query("UPDATE documentos SET credito_disponivel = credito_disponivel + ? WHERE id = ?").bind(valor).bind(cid).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM credito_utilizacoes WHERE documento_id = ?").bind(a.id).execute(&mut *tx).await?;
    let agora = db::agora();
    // repõe stock com movimentos inversos
    let movs: Vec<(i64, i64, f64)> = sqlx::query_as("SELECT artigo_id, armazem_id, quantidade FROM movimentos_stock WHERE documento_id = ?")
        .bind(a.id)
        .fetch_all(&mut *tx)
        .await?;
    for (artigo, armazem, qtd) in movs {
        mover_stock(&mut tx, artigo, armazem, -qtd, None, a.id, &format!("Anulação {numero}"), true, &agora).await?;
    }
    // repõe os lotes
    let lms: Vec<(i64, f64)> = sqlx::query_as("SELECT lote_id, quantidade FROM lote_movimentos WHERE documento_id = ?").bind(a.id).fetch_all(&mut *tx).await?;
    for (lote_id, q) in lms {
        sqlx::query("UPDATE lotes SET quantidade = quantidade - ? WHERE id = ?").bind(q).bind(lote_id).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM lote_movimentos WHERE documento_id = ?").bind(a.id).execute(&mut *tx).await?;
    // repõe pendentes liquidados por este documento (recibo / pagamento / nota de crédito)
    let liqs: Vec<(i64, i64)> = sqlx::query_as("SELECT documento_id, valor FROM recibo_documentos WHERE recibo_id = ?")
        .bind(a.id)
        .fetch_all(&mut *tx)
        .await?;
    for (did, valor) in liqs {
        sqlx::query("UPDATE documentos SET valor_pendente = valor_pendente + ? WHERE id = ?").bind(valor).bind(did).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE documentos SET estado = 'A', motivo_anulacao = ?, valor_pendente = 0, credito_disponivel = 0 WHERE id = ?")
        .bind(a.motivo.trim())
        .bind(a.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'ANULAR', ?)")
        .bind(s.id)
        .bind(format!("{numero}: {}", a.motivo.trim()))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Value::Null)
}

/// Tabelas do formulário de documento: prazos de pagamento e retenções.
pub async fn auxiliares(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(json!({
        "prazos": db::query_json(&st.pool, "SELECT * FROM prazos_pagamento ORDER BY dias").await?,
        "retencoes": db::query_json(&st.pool, "SELECT * FROM retencoes WHERE ativo = 1 ORDER BY codigo").await?,
        "moedas": db::query_json(&st.pool, "SELECT * FROM moedas WHERE ativo = 1 ORDER BY codigo = 'AOA' DESC, codigo").await?,
        "tipos_custo": db::query_json(&st.pool, "SELECT * FROM tipos_custo ORDER BY descricao").await?,
    }))
}

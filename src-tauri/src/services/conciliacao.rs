// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Conciliação bancária (decisão do dono, 2026-11-24: importar o extracto). Lê-se o extracto do banco (Excel, ODS ou
//! CSV, pela leitura das folhas da importação), e cada linha casa com movimentos do fluxo de caixa da conta (`vw_fluxo`):
//!
//! 1. um movimento com o mesmo valor e uma data próxima (o banco lança até uns dias depois);
//! 2. senão, os movimentos por conciliar de um dia que somam o valor da linha: os de um modo de pagamento (o banco
//!    liquida numa só linha as vendas com cartão do dia) ou todos os desse dia.
//!
//! O que sobra dos dois lados fica à vista. Casa-se à mão, escolhendo os movimentos (a soma tem de bater, ou lança-se
//! a diferença, por exemplo a comissão do banco). Uma linha sem nada no KanGest (juros, comissões) lança-se como
//! movimento de tesouraria. Os lançamentos passam pelo serviço da tesouraria, com a permissão, o fecho de períodos e
//! a auditoria dele. A conciliação só aponta: não altera os movimentos que concilia.

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::services::importar_folha::{self as folha, Folha};
use crate::state::AppState;

/// Linhas de um extracto (um mês de um banco movimentado anda pelas centenas).
const MAX_LINHAS: usize = 5000;
/// Tamanho do ficheiro enviado pelo ecrã.
const MAX_BYTES: usize = 5 * 1024 * 1024;
/// O banco lança o movimento até uns dias depois (data-valor, cheques, transferências): casa-se com esta folga.
const FOLGA_DIAS: i64 = 4;

/// As colunas do extracto (índice da coluna na folha). Sem elas, o KanGest procura-as pelos títulos.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Debug)]
pub struct Colunas {
    #[serde(default)]
    data: Option<usize>,
    #[serde(default)]
    descricao: Option<usize>,
    /// um só valor (negativo = saída), ou
    #[serde(default)]
    valor: Option<usize>,
    /// débito e crédito em colunas separadas
    #[serde(default)]
    debito: Option<usize>,
    #[serde(default)]
    credito: Option<usize>,
    /// «D» / «C» ao lado de um valor sem sinal
    #[serde(default)]
    natureza: Option<usize>,
    #[serde(default)]
    saldo: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct FicheiroIn {
    /// o ficheiro neste computador (diálogo nativo), ou…
    #[serde(default)]
    caminho: Option<String>,
    /// …o ficheiro enviado pelo ecrã (base64), para funcionar em qualquer posto
    #[serde(default)]
    nome: Option<String>,
    #[serde(default)]
    conteudo: Option<String>,
    #[serde(default)]
    folha: Option<String>,
    #[serde(default)]
    colunas: Option<Colunas>,
}

#[derive(Debug, Clone, PartialEq)]
struct LinhaExtracto {
    linha: usize,
    data: String,
    descricao: String,
    valor: i64,
    saldo: Option<i64>,
}

struct Extracto {
    nome: String,
    folhas: Vec<String>,
    folha: String,
    cabecalho: Vec<String>,
    linha_cabecalho: usize,
    colunas: Colunas,
    linhas: Vec<LinhaExtracto>,
    ignoradas: usize,
}

/// Uma data de extracto: «31/10/2026», «31-10-2026», «31.10.26», «2026-10-31», com ou sem hora, ou o número de série
/// do Excel (dias desde 30/12/1899).
fn data_extracto(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let so_data = t.split([' ', 'T']).next().unwrap_or(t);
    for f in ["%d/%m/%Y", "%d-%m-%Y", "%d.%m.%Y", "%Y-%m-%d", "%Y/%m/%d", "%d/%m/%y", "%d-%m-%y", "%d.%m.%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(so_data, f) {
            if (1990..=2100).contains(&d.year()) {
                return Some(d.format("%Y-%m-%d").to_string());
            }
        }
    }
    // número de série do Excel (as células de data chegam assim da leitura das folhas)
    let n = folha::numero(t)?;
    if (20_000.0..80_000.0).contains(&n) && !t.contains('/') {
        let base = NaiveDate::from_ymd_opt(1899, 12, 30)?;
        return Some((base + chrono::Duration::days(n.floor() as i64)).format("%Y-%m-%d").to_string());
    }
    None
}

fn centimos(s: &str) -> Option<i64> {
    folha::numero(s).map(|v| (v * 100.0).round() as i64)
}

/// O texto de uma célula (vazio quando a linha é mais curta ou a coluna não foi escolhida).
fn cel(l: &[String], c: Option<usize>) -> &str {
    c.and_then(|j| l.get(j)).map(String::as_str).unwrap_or("")
}

/// Os títulos conhecidos de cada coluna (já normalizados: minúsculas, sem acentos nem pontuação).
fn papel(titulo: &str) -> Option<&'static str> {
    let t = folha::normalizar(titulo);
    let t = t.as_str();
    let comeca = |p: &[&str]| p.iter().any(|x| t == *x || t.starts_with(&format!("{x} ")));
    if comeca(&["data", "dt", "date"]) {
        return Some("data");
    }
    if comeca(&["saldo", "balance"]) {
        return Some("saldo");
    }
    // «Débito/Crédito» numa só coluna: é o valor com sinal
    if t.contains("debito") && t.contains("credito") {
        return Some("valor");
    }
    if comeca(&["debito", "debitos", "saida", "saidas", "levantamento", "levantamentos", "debit"]) {
        return Some("debito");
    }
    if comeca(&["credito", "creditos", "entrada", "entradas", "deposito", "depositos", "credit"]) {
        return Some("credito");
    }
    if comeca(&["valor", "montante", "importancia", "quantia", "amount"]) {
        return Some("valor");
    }
    if matches!(t, "dc" | "d c" | "c d" | "natureza" | "sinal" | "deb cred" | "tipo") {
        return Some("natureza");
    }
    if comeca(&["descricao", "descritivo", "historico", "movimento", "movimentos", "detalhe", "detalhes", "designacao", "operacao", "description"]) {
        return Some("descricao");
    }
    None
}

/// A linha dos títulos (a que tem mais títulos conhecidos, com a data e um valor) e as colunas que eles dão.
fn detectar(f: &Folha) -> Option<(usize, Colunas)> {
    let mut melhor: Option<(usize, usize, Colunas)> = None;
    for (i, l) in f.linhas.iter().enumerate().take(40) {
        let mut c = Colunas::default();
        let mut n = 0;
        for (j, texto) in l.iter().enumerate() {
            let slot = match papel(texto) {
                Some("data") => &mut c.data,
                Some("descricao") => &mut c.descricao,
                Some("valor") => &mut c.valor,
                Some("debito") => &mut c.debito,
                Some("credito") => &mut c.credito,
                Some("natureza") => &mut c.natureza,
                Some("saldo") => &mut c.saldo,
                _ => continue,
            };
            // a primeira coluna de cada papel (a «Data» antes da «Data-valor»)
            if slot.is_none() {
                *slot = Some(j);
                n += 1;
            }
        }
        let valido = c.data.is_some() && (c.valor.is_some() || c.debito.is_some() || c.credito.is_some());
        if valido && melhor.as_ref().is_none_or(|m| n > m.1) {
            melhor = Some((i, n, c));
        }
    }
    melhor.map(|(i, _, c)| (i, c))
}

fn ler_extracto(e: &FicheiroIn) -> AppResult<Extracto> {
    // o ficheiro: o caminho (diálogo nativo) ou o conteúdo enviado pelo ecrã, gravado num temporário
    let (caminho, temporario, nome) = match (&e.caminho, &e.conteudo) {
        (Some(c), _) if !c.trim().is_empty() => {
            let p = PathBuf::from(c.trim());
            let nome = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            (p, false, nome)
        }
        (_, Some(b64)) => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| AppError::msg("O ficheiro chegou estragado. Escolha-o outra vez."))?;
            if bytes.len() > MAX_BYTES {
                return Err(AppError::msg("O extracto tem mais de 5 MB: exporte só o período a conciliar."));
            }
            let nome = e.nome.clone().unwrap_or_else(|| "extracto.xlsx".into());
            let ext = std::path::Path::new(&nome).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_else(|| "xlsx".into());
            let unico = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
            let p = std::env::temp_dir().join(format!("kangest-extracto-{}-{unico}.{ext}", std::process::id()));
            std::fs::write(&p, bytes).map_err(|e| AppError::msg(format!("Não consegui guardar o extracto num ficheiro temporário: {e}")))?;
            (p, true, nome)
        }
        _ => return Err(AppError::msg("Escolha o ficheiro do extracto.")),
    };
    let lidas = folha::ler(&caminho);
    if temporario {
        let _ = std::fs::remove_file(&caminho);
    }
    let folhas = lidas?;
    let nomes: Vec<String> = folhas.iter().map(|f| f.nome.clone()).collect();
    let f = match &e.folha {
        Some(n) => folhas.iter().find(|f| &f.nome == n).ok_or_else(|| AppError::msg(format!("Não há a folha «{n}» no ficheiro.")))?,
        // sem escolha: a primeira folha onde se reconhecem os títulos
        None => folhas.iter().find(|f| detectar(f).is_some()).unwrap_or(&folhas[0]),
    };
    let detectado = detectar(f);
    let (linha_cab, colunas) = match (e.colunas, detectado) {
        (Some(c), Some((i, _))) => (i, c),
        (Some(c), None) => (0, c),
        (None, Some(d)) => d,
        (None, None) => {
            return Err(AppError::msg(
                "Não encontrei as colunas da data e do valor. Confirme que o ficheiro é o extracto do banco, ou escolha as colunas no ecrã.",
            ))
        }
    };
    if colunas.data.is_none() || (colunas.valor.is_none() && colunas.debito.is_none() && colunas.credito.is_none()) {
        return Err(AppError::msg("Escolha a coluna da data e a do valor (ou as do débito e do crédito)."));
    }
    let mut linhas = Vec::new();
    let mut ignoradas = 0;
    for (i, l) in f.linhas.iter().enumerate().skip(linha_cab + 1) {
        if l.iter().all(|c| c.is_empty()) {
            continue;
        }
        // linhas sem data (saldo anterior, totais, notas do banco) ficam de fora
        let Some(data) = data_extracto(cel(l, colunas.data)) else {
            ignoradas += 1;
            continue;
        };
        let valor = if colunas.valor.is_some() {
            let v = centimos(cel(l, colunas.valor));
            let nat = folha::normalizar(cel(l, colunas.natureza));
            match (v, nat.as_str()) {
                (Some(v), "d" | "deb" | "debito" | "db" | "dr") => Some(-v.abs()),
                (Some(v), "c" | "cred" | "credito" | "cr") => Some(v.abs()),
                (v, _) => v,
            }
        } else {
            let d = centimos(cel(l, colunas.debito)).map(i64::abs).unwrap_or(0);
            let c = centimos(cel(l, colunas.credito)).map(i64::abs).unwrap_or(0);
            Some(c - d)
        };
        let Some(valor) = valor.filter(|v| *v != 0) else {
            ignoradas += 1;
            continue;
        };
        let descricao = cel(l, colunas.descricao).trim();
        linhas.push(LinhaExtracto {
            linha: i + 1,
            data,
            descricao: if descricao.is_empty() { "(sem descrição)".into() } else { descricao.chars().take(200).collect() },
            valor,
            saldo: centimos(cel(l, colunas.saldo)),
        });
        if linhas.len() > MAX_LINHAS {
            return Err(AppError::msg("O extracto tem mais de 5 000 movimentos: exporte só o período a conciliar."));
        }
    }
    if linhas.is_empty() {
        return Err(AppError::msg("Não encontrei movimentos com data e valor no extracto. Confirme as colunas."));
    }
    Ok(Extracto {
        nome,
        folhas: nomes,
        folha: f.nome.clone(),
        cabecalho: f.linhas.get(linha_cab).cloned().unwrap_or_default(),
        linha_cabecalho: linha_cab + 1,
        colunas,
        linhas,
        ignoradas,
    })
}

fn periodo(linhas: &[LinhaExtracto]) -> (String, String) {
    let de = linhas.iter().map(|l| l.data.clone()).min().unwrap_or_default();
    let ate = linhas.iter().map(|l| l.data.clone()).max().unwrap_or_default();
    (de, ate)
}

/// O saldo final segundo o banco: o da linha mais recente com saldo (os extractos vêm do mais antigo para o mais
/// recente, ou ao contrário; no último dia fica a linha que o banco lançou por último).
fn saldo_final(linhas: &[LinhaExtracto]) -> Option<i64> {
    let com: Vec<&LinhaExtracto> = linhas.iter().filter(|l| l.saldo.is_some()).collect();
    let ultima_data = com.iter().map(|l| l.data.as_str()).max()?;
    let do_dia: Vec<&&LinhaExtracto> = com.iter().filter(|l| l.data == ultima_data).collect();
    let crescente = com.first().map(|l| l.data.as_str()) <= com.last().map(|l| l.data.as_str());
    let l = if crescente { do_dia.last() } else { do_dia.first() };
    l.and_then(|l| l.saldo)
}

fn colunas_json(c: &Colunas) -> Value {
    json!({ "data": c.data, "descricao": c.descricao, "valor": c.valor, "debito": c.debito, "credito": c.credito, "natureza": c.natureza, "saldo": c.saldo })
}

/// Lê o extracto e mostra o que se vai importar (nada se grava).
pub async fn ler(st: &AppState, e: FicheiroIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let x = tokio::task::spawn_blocking(move || ler_extracto(&e)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let (de, ate) = periodo(&x.linhas);
    let entradas: i64 = x.linhas.iter().filter(|l| l.valor > 0).map(|l| l.valor).sum();
    let saidas: i64 = x.linhas.iter().filter(|l| l.valor < 0).map(|l| -l.valor).sum();
    Ok(json!({
        "ficheiro": x.nome,
        "folhas": x.folhas,
        "folha": x.folha,
        "linha_cabecalho": x.linha_cabecalho,
        "cabecalho": x.cabecalho,
        "colunas": colunas_json(&x.colunas),
        "de": de,
        "ate": ate,
        "movimentos": x.linhas.len(),
        "ignoradas": x.ignoradas,
        "entradas": entradas,
        "saidas": saidas,
        "saldo_banco": saldo_final(&x.linhas),
        "amostra": x.linhas.iter().take(8).map(|l| json!({ "linha": l.linha, "data": l.data, "descricao": l.descricao, "valor": l.valor })).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
pub struct CriarIn {
    conta_id: i64,
    #[serde(flatten)]
    ficheiro: FicheiroIn,
}

/// Importa o extracto para uma conta e casa o que puder sozinho.
pub async fn criar(st: &AppState, c: CriarIn) -> AppResult<Value> {
    let s = st.exigir_perm("tesouraria.fluxo")?;
    let conta: Option<i64> = sqlx::query_scalar("SELECT id FROM contas WHERE id = ? AND ativo = 1").bind(c.conta_id).fetch_optional(&st.pool).await?;
    if conta.is_none() {
        return Err(AppError::msg("Escolha a conta do banco."));
    }
    let f = c.ficheiro;
    let x = tokio::task::spawn_blocking(move || ler_extracto(&f)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let (de, ate) = periodo(&x.linhas);
    let soma: i64 = x.linhas.iter().map(|l| l.valor).sum();
    // o mesmo extracto importado duas vezes: mesma conta, mesmo período, mesmas linhas e mesmo total
    let repetida: Option<i64> = sqlx::query_scalar(
        "SELECT c.id FROM conciliacoes c WHERE c.conta_id = ? AND c.de = ? AND c.ate = ?
           AND (SELECT COUNT(*) FROM conciliacao_linhas l WHERE l.conciliacao_id = c.id) = ?
           AND (SELECT COALESCE(SUM(l.valor), 0) FROM conciliacao_linhas l WHERE l.conciliacao_id = c.id) = ?",
    )
    .bind(c.conta_id)
    .bind(&de)
    .bind(&ate)
    .bind(x.linhas.len() as i64)
    .bind(soma)
    .fetch_optional(&st.pool)
    .await?;
    if let Some(id) = repetida {
        return Err(AppError::msg(format!("Este extracto já foi importado (conciliação n.º {id}).")));
    }
    let mut tx = db::transacao(&st.pool).await?;
    let id = sqlx::query("INSERT INTO conciliacoes (conta_id, ficheiro, de, ate, saldo_banco, criado_em, utilizador_id) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(c.conta_id)
        .bind(&x.nome)
        .bind(&de)
        .bind(&ate)
        .bind(saldo_final(&x.linhas))
        .bind(db::agora())
        .bind(s.id)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    for l in &x.linhas {
        sqlx::query("INSERT INTO conciliacao_linhas (conciliacao_id, linha, data, descricao, valor) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(l.linha as i64)
            .bind(&l.data)
            .bind(&l.descricao)
            .bind(l.valor)
            .execute(&mut *tx)
            .await?;
    }
    let casadas = casar_auto(&mut tx, id, c.conta_id).await?;
    tx.commit().await?;
    Ok(json!({ "id": id, "linhas": x.linhas.len(), "casadas": casadas }))
}

fn dias(a: &str, b: &str) -> i64 {
    match (NaiveDate::parse_from_str(a, "%Y-%m-%d"), NaiveDate::parse_from_str(b, "%Y-%m-%d")) {
        (Ok(x), Ok(y)) => (x - y).num_days().abs(),
        _ => i64::MAX,
    }
}

/// Casa sozinho: primeiro um movimento com o mesmo valor e a data mais próxima (até `FOLGA_DIAS`); depois, para as
/// linhas que sobram, os movimentos por conciliar de um dia que somam o valor da linha: os de um modo de pagamento (a
/// liquidação das vendas com cartão do dia) ou todos os desse dia.
async fn casar_auto(tx: &mut sqlx::SqliteConnection, conciliacao_id: i64, conta_id: i64) -> AppResult<usize> {
    let linhas: Vec<(i64, String, i64)> = sqlx::query_as("SELECT id, data, valor FROM conciliacao_linhas WHERE conciliacao_id = ? AND casado IS NULL ORDER BY data, id")
        .bind(conciliacao_id)
        .fetch_all(&mut *tx)
        .await?;
    let Some((de, ate)) = linhas.first().map(|l| l.1.clone()).zip(linhas.last().map(|l| l.1.clone())) else { return Ok(0) };
    // (chave, data, valor com sinal, rubrica: o modo de pagamento de um documento, o centro de custo de um movimento)
    let movimentos: Vec<(String, String, i64, String)> = sqlx::query_as(&format!(
        "SELECT f.chave, f.data, CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END, COALESCE(f.rubrica, '')
         FROM vw_fluxo f
         WHERE f.conta_id = ? AND f.anulado = 0 AND f.data BETWEEN date(?, '-{FOLGA_DIAS} day') AND date(?, '+{FOLGA_DIAS} day')
           AND NOT EXISTS (SELECT 1 FROM conciliacao_pares p WHERE p.fluxo = f.chave)
         ORDER BY f.data, f.chave"
    ))
    .bind(conta_id)
    .bind(&de)
    .bind(&ate)
    .fetch_all(&mut *tx)
    .await?;
    let mut usados: HashSet<String> = HashSet::new();
    let mut pares: Vec<(i64, Vec<String>)> = Vec::new();
    let mut sobram = Vec::new();
    // 1. um para um: mesmo valor, data mais próxima
    for (id, data, valor) in &linhas {
        let melhor = movimentos
            .iter()
            .filter(|m| m.2 == *valor && !usados.contains(&m.0))
            .map(|m| (dias(&m.1, data), m))
            .filter(|(d, _)| *d <= FOLGA_DIAS)
            .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
        match melhor {
            Some((_, m)) => {
                usados.insert(m.0.clone());
                pares.push((*id, vec![m.0.clone()]));
            }
            None => sobram.push((*id, data.clone(), *valor)),
        }
    }
    // 2. vários de um dia, com o sinal da linha: os de um modo de pagamento (o banco liquida os cartões do dia numa só
    //    linha, sem as transferências do mesmo dia) ou todos os do dia, a somar o valor dela
    for (id, data, valor) in &sobram {
        let mut melhor: Option<(i64, Vec<String>)> = None;
        let mut dias_vistos: Vec<&str> = movimentos.iter().filter(|m| !usados.contains(&m.0) && dias(&m.1, data) <= FOLGA_DIAS).map(|m| m.1.as_str()).collect();
        dias_vistos.dedup();
        for dia in dias_vistos {
            let do_dia: Vec<&(String, String, i64, String)> = movimentos.iter().filter(|m| m.1 == dia && !usados.contains(&m.0) && m.2.signum() == valor.signum()).collect();
            let mut rubricas: Vec<Option<&str>> = do_dia.iter().map(|m| Some(m.3.as_str())).collect();
            rubricas.sort();
            rubricas.dedup();
            rubricas.push(None);
            for r in rubricas {
                let grupo: Vec<&&(String, String, i64, String)> = do_dia.iter().filter(|m| r.is_none_or(|r| m.3 == r)).collect();
                if grupo.len() < 2 || grupo.iter().map(|m| m.2).sum::<i64>() != *valor {
                    continue;
                }
                let d = dias(dia, data);
                if melhor.as_ref().is_none_or(|m| d < m.0) {
                    melhor = Some((d, grupo.iter().map(|m| m.0.clone()).collect()));
                }
            }
        }
        if let Some((_, chaves)) = melhor {
            usados.extend(chaves.iter().cloned());
            pares.push((*id, chaves));
        }
    }
    for (id, chaves) in &pares {
        for c in chaves {
            sqlx::query("INSERT INTO conciliacao_pares (linha_id, fluxo) VALUES (?, ?)").bind(id).bind(c).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE conciliacao_linhas SET casado = 'auto' WHERE id = ?").bind(id).execute(&mut *tx).await?;
    }
    Ok(pares.len())
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

#[derive(Deserialize, Default)]
pub struct ContaIn {
    #[serde(default)]
    conta_id: Option<i64>,
}

/// As conciliações (de uma conta ou de todas), das mais recentes para as mais antigas.
pub async fn listar(st: &AppState, f: ContaIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let linhas = sqlx::query(
        "SELECT c.id, c.conta_id, ct.descricao AS conta, c.ficheiro, c.de, c.ate, c.estado, c.criado_em, c.saldo_banco,
                COUNT(l.id) AS linhas, COUNT(l.casado) AS casadas, COALESCE(SUM(l.valor), 0) AS total
         FROM conciliacoes c JOIN contas ct ON ct.id = c.conta_id LEFT JOIN conciliacao_linhas l ON l.conciliacao_id = c.id
         WHERE (?1 IS NULL OR c.conta_id = ?1)
         GROUP BY c.id ORDER BY c.ate DESC, c.id DESC",
    )
    .bind(f.conta_id)
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&linhas))
}

async fn estado_de(st: &AppState, id: i64) -> AppResult<String> {
    sqlx::query_scalar("SELECT estado FROM conciliacoes WHERE id = ?").bind(id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)
}

/// Uma conciliação: as linhas do extracto com os movimentos casados, os movimentos da conta por conciliar e o resumo.
pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let c = sqlx::query(
        "SELECT c.*, ct.descricao AS conta, u.nome AS criado_por FROM conciliacoes c
         JOIN contas ct ON ct.id = c.conta_id LEFT JOIN utilizadores u ON u.id = c.utilizador_id WHERE c.id = ?",
    )
    .bind(i.id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    let conta_id: i64 = c.get("conta_id");
    let (de, ate): (String, String) = (c.get("de"), c.get("ate"));
    let linhas = sqlx::query("SELECT id, linha, data, descricao, valor, casado FROM conciliacao_linhas WHERE conciliacao_id = ? ORDER BY data, linha")
        .bind(i.id)
        .fetch_all(&st.pool)
        .await?;
    let pares = sqlx::query(
        "SELECT p.linha_id, f.chave, f.data, f.documento, COALESCE(f.descricao, f.rubrica) AS descricao, f.origem,
                CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END AS valor
         FROM conciliacao_pares p JOIN conciliacao_linhas l ON l.id = p.linha_id JOIN vw_fluxo f ON f.chave = p.fluxo
         WHERE l.conciliacao_id = ? ORDER BY f.data, f.chave",
    )
    .bind(i.id)
    .fetch_all(&st.pool)
    .await?;
    let pares = db::rows_json(&pares);
    let linhas: Vec<Value> = db::rows_json(&linhas)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut l| {
            let id = l["id"].as_i64();
            l["movimentos"] = Value::Array(pares.as_array().map(|p| p.iter().filter(|x| x["linha_id"].as_i64() == id).cloned().collect()).unwrap_or_default());
            l
        })
        .collect();
    // do lado do KanGest: os movimentos da conta no período (com a folga) que nenhuma conciliação apontou
    let pendentes = sqlx::query(&format!(
        "SELECT f.chave, f.data, f.documento, COALESCE(f.descricao, f.rubrica) AS descricao, f.origem,
                CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END AS valor
         FROM vw_fluxo f
         WHERE f.conta_id = ? AND f.anulado = 0 AND f.data BETWEEN date(?, '-{FOLGA_DIAS} day') AND date(?, '+{FOLGA_DIAS} day')
           AND NOT EXISTS (SELECT 1 FROM conciliacao_pares p WHERE p.fluxo = f.chave)
         ORDER BY f.data, f.chave"
    ))
    .bind(conta_id)
    .bind(&de)
    .bind(&ate)
    .fetch_all(&st.pool)
    .await?;
    // o saldo da conta no KanGest no último dia do extracto (saldo inicial da conta e todo o fluxo até lá)
    let saldo_kangest: i64 = sqlx::query_scalar(
        "SELECT ct.saldo_inicial + COALESCE((SELECT SUM(CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END)
                FROM vw_fluxo f WHERE f.conta_id = ct.id AND f.anulado = 0 AND f.data <= ?), 0)
         FROM contas ct WHERE ct.id = ?",
    )
    .bind(&ate)
    .bind(conta_id)
    .fetch_one(&st.pool)
    .await?;
    let total: i64 = linhas.iter().map(|l| l["valor"].as_i64().unwrap_or(0)).sum();
    let por_casar: i64 = linhas.iter().filter(|l| l["casado"].is_null()).map(|l| l["valor"].as_i64().unwrap_or(0)).sum();
    let pend = db::rows_json(&pendentes);
    let kangest_por_casar: i64 = pend.as_array().map(|p| p.iter().map(|x| x["valor"].as_i64().unwrap_or(0)).sum()).unwrap_or(0);
    Ok(json!({
        "conciliacao": db::row_json(&c),
        "linhas": linhas,
        "pendentes": pend,
        "resumo": {
            "total_extracto": total,
            "extracto_por_casar": por_casar,
            "linhas_por_casar": linhas.iter().filter(|l| l["casado"].is_null()).count(),
            "kangest_por_casar": kangest_por_casar,
            "saldo_banco": c.get::<Option<i64>, _>("saldo_banco"),
            "saldo_kangest": saldo_kangest,
        },
    }))
}

/// A linha, se a conciliação dela está aberta: (valor, conta, data, já casada).
async fn linha_aberta(st: &AppState, linha_id: i64) -> AppResult<(i64, i64, String, bool)> {
    let r = sqlx::query(
        "SELECT l.valor, l.data, l.casado IS NOT NULL AS casada, c.conta_id, c.estado
         FROM conciliacao_linhas l JOIN conciliacoes c ON c.id = l.conciliacao_id WHERE l.id = ?",
    )
    .bind(linha_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(AppError::NaoEncontrado)?;
    if r.get::<String, _>("estado") != "ABERTA" {
        return Err(AppError::msg("Esta conciliação está fechada."));
    }
    Ok((r.get("valor"), r.get("conta_id"), r.get("data"), r.get::<i64, _>("casada") == 1))
}

#[derive(Deserialize)]
pub struct CasarIn {
    linha_id: i64,
    /// as chaves dos movimentos (vw_fluxo) que fazem a linha
    #[serde(default)]
    chaves: Vec<String>,
    /// quando a soma não bate: lança-se a diferença (comissão do banco, por exemplo) e casa-se com ela
    #[serde(default)]
    diferenca: Option<DiferencaIn>,
    /// PIN de quem aprova a saída lançada, se a regra de aprovação «saidas» se aplicar (services/aprovacoes.rs)
    #[serde(default)]
    autorizacao_pin: Option<String>,
}

#[derive(Deserialize)]
pub struct DiferencaIn {
    descricao: String,
    #[serde(default)]
    centro_custo_id: Option<i64>,
}

/// Casa uma linha do extracto com movimentos escolhidos à mão (a soma tem de bater, ou lança-se a diferença).
pub async fn casar(st: &AppState, c: CasarIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let (valor, conta_id, data, casada) = linha_aberta(st, c.linha_id).await?;
    if casada {
        return Err(AppError::msg("Esta linha já está casada. Desfaça primeiro."));
    }
    let mut chaves: Vec<String> = c.chaves.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    chaves.sort();
    chaves.dedup();
    if chaves.is_empty() {
        return Err(AppError::msg("Escolha os movimentos que fazem esta linha."));
    }
    let mut soma = 0i64;
    for k in &chaves {
        let r = sqlx::query(
            "SELECT f.conta_id, f.anulado, CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END AS valor,
                    EXISTS (SELECT 1 FROM conciliacao_pares p WHERE p.fluxo = f.chave) AS usado
             FROM vw_fluxo f WHERE f.chave = ?",
        )
        .bind(k)
        .fetch_optional(&st.pool)
        .await?
        .ok_or_else(|| AppError::msg(format!("O movimento {k} não existe.")))?;
        if r.get::<i64, _>("conta_id") != conta_id || r.get::<i64, _>("anulado") == 1 {
            return Err(AppError::msg("Só se casam movimentos válidos desta conta."));
        }
        if r.get::<i64, _>("usado") == 1 {
            return Err(AppError::msg("Um dos movimentos já está conciliado noutra linha."));
        }
        soma += r.get::<i64, _>("valor");
    }
    let falta = valor - soma;
    if falta != 0 {
        let Some(d) = c.diferenca else {
            return Err(AppError::msg(format!(
                "Os valores não batem: o extracto tem {} e os movimentos somam {}. Escolha outros, ou lance a diferença.",
                crate::agt::total_fmt(valor),
                crate::agt::total_fmt(soma)
            )));
        };
        if d.descricao.trim().is_empty() {
            return Err(AppError::msg("Indique a descrição da diferença (por exemplo, «Comissão do banco»)."));
        }
        // pela tesouraria de sempre: permissão, fecho de períodos e auditoria dela
        let m = Box::pin(crate::services::dispatch(
            st,
            "tesouraria.movimento",
            json!({ "tipo": if falta > 0 { "ENTRADA" } else { "SAIDA" }, "conta_id": conta_id, "valor": falta.abs(), "descricao": d.descricao.trim(),
                    "centro_custo_id": d.centro_custo_id, "data": data, "autorizacao_pin": c.autorizacao_pin }),
        ))
        .await?;
        chaves.push(format!("T{}", m["id"].as_i64().unwrap_or(0)));
    }
    guardar_pares(st, c.linha_id, &chaves, "manual").await?;
    Ok(json!({ "linha_id": c.linha_id, "movimentos": chaves.len() }))
}

async fn guardar_pares(st: &AppState, linha_id: i64, chaves: &[String], como: &str) -> AppResult<()> {
    let mut tx = db::transacao(&st.pool).await?;
    for k in chaves {
        sqlx::query("INSERT INTO conciliacao_pares (linha_id, fluxo) VALUES (?, ?)")
            .bind(linha_id)
            .bind(k)
            .execute(&mut *tx)
            .await
            .map_err(|_| AppError::msg("Um dos movimentos já está conciliado noutra linha."))?;
    }
    sqlx::query("UPDATE conciliacao_linhas SET casado = ? WHERE id = ?").bind(como).bind(linha_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct LinhaIn {
    linha_id: i64,
    #[serde(default)]
    descricao: Option<String>,
    #[serde(default)]
    centro_custo_id: Option<i64>,
    /// PIN de quem aprova a saída lançada, se a regra de aprovação «saidas» se aplicar (services/aprovacoes.rs)
    #[serde(default)]
    autorizacao_pin: Option<String>,
}

/// Uma linha do extracto sem nada no KanGest (juros, comissões, um depósito esquecido): lança-se como movimento de
/// tesouraria, com a data e o valor da linha, e fica casada com ele.
pub async fn lancar(st: &AppState, l: LinhaIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    let (valor, conta_id, data, casada) = linha_aberta(st, l.linha_id).await?;
    if casada {
        return Err(AppError::msg("Esta linha já está casada."));
    }
    let desc: String = match l.descricao.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => d.to_string(),
        None => sqlx::query_scalar("SELECT descricao FROM conciliacao_linhas WHERE id = ?").bind(l.linha_id).fetch_one(&st.pool).await?,
    };
    let m = Box::pin(crate::services::dispatch(
        st,
        "tesouraria.movimento",
        json!({ "tipo": if valor > 0 { "ENTRADA" } else { "SAIDA" }, "conta_id": conta_id, "valor": valor.abs(), "descricao": desc,
                "centro_custo_id": l.centro_custo_id, "data": data, "documento": "Extracto bancário", "autorizacao_pin": l.autorizacao_pin }),
    ))
    .await?;
    let chave = format!("T{}", m["id"].as_i64().unwrap_or(0));
    guardar_pares(st, l.linha_id, std::slice::from_ref(&chave), "lancado").await?;
    Ok(json!({ "linha_id": l.linha_id, "movimento": chave }))
}

/// Desfaz o casamento de uma linha (os movimentos lançados a partir dela continuam na tesouraria: anulam-se lá).
pub async fn descasar(st: &AppState, l: LinhaIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    linha_aberta(st, l.linha_id).await?;
    let mut tx = db::transacao(&st.pool).await?;
    sqlx::query("DELETE FROM conciliacao_pares WHERE linha_id = ?").bind(l.linha_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE conciliacao_linhas SET casado = NULL WHERE id = ?").bind(l.linha_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({ "linha_id": l.linha_id }))
}

/// Fecha a conciliação: fica como está, para consulta.
pub async fn fechar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    if estado_de(st, i.id).await? != "ABERTA" {
        return Err(AppError::msg("Esta conciliação já está fechada."));
    }
    sqlx::query("UPDATE conciliacoes SET estado = 'FECHADA', fechado_em = ? WHERE id = ?").bind(db::agora()).bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "id": i.id }))
}

/// Apaga uma conciliação aberta (por exemplo, o extracto errado). Os movimentos lançados a partir dela ficam.
pub async fn eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("tesouraria.fluxo")?;
    if estado_de(st, i.id).await? != "ABERTA" {
        return Err(AppError::msg("Uma conciliação fechada não se apaga."));
    }
    sqlx::query("DELETE FROM conciliacoes WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "id": i.id }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_e_titulos() {
        assert_eq!(data_extracto("31/10/2026").as_deref(), Some("2026-10-31"));
        assert_eq!(data_extracto("31-10-2026 14:05").as_deref(), Some("2026-10-31"));
        assert_eq!(data_extracto("2026-10-31T00:00:00").as_deref(), Some("2026-10-31"));
        assert_eq!(data_extracto("31.10.26").as_deref(), Some("2026-10-31"));
        assert_eq!(data_extracto("46326").as_deref(), Some("2026-10-31"), "número de série do Excel");
        assert_eq!(data_extracto("Saldo anterior"), None);
        assert_eq!(data_extracto("1 500,00"), None);
        assert_eq!(papel("Data Mov."), Some("data"));
        assert_eq!(papel("Descritivo"), Some("descricao"));
        assert_eq!(papel("Montante (Kz)"), Some("valor"));
        assert_eq!(papel("Débito"), Some("debito"));
        assert_eq!(papel("Créditos"), Some("credito"));
        assert_eq!(papel("Saldo Contabilístico"), Some("saldo"));
        assert_eq!(papel("D/C"), Some("natureza"));
        assert_eq!(papel("Nº Documento"), None);
        assert_eq!(papel("Débito/Crédito"), Some("valor"));
    }

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

    /// Um extracto em CSV (título, saldo anterior, débito e crédito em colunas, saldo): casa sozinho o depósito, a renda e
    /// as vendas com cartão de um dia (numa só linha); a comissão lança-se a partir da linha; o TPA com a comissão
    /// descontada casa à mão, lançando a diferença; o mesmo extracto não entra duas vezes; o operador de caixa não vê.
    #[tokio::test]
    async fn conciliacao_por_extracto() {
        let pasta = std::env::temp_dir().join(format!("kangest-conciliacao-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let hoje = NaiveDate::parse_from_str(&db::hoje(), "%Y-%m-%d").unwrap();
        let d = |n: i64| (hoje - chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
        let br = |n: i64| (hoje - chrono::Duration::days(n)).format("%d/%m/%Y").to_string();
        let banco: i64 = sqlx::query_scalar("SELECT id FROM contas WHERE tipo = 'BANCO' ORDER BY id LIMIT 1").fetch_one(&st.pool).await.unwrap();
        let mov = |tipo: &str, valor: i64, desc: &str, n: i64| json!({ "tipo": tipo, "conta_id": banco, "valor": valor, "descricao": desc, "data": d(n) });
        // o que o KanGest tem na conta do banco
        rpc(&st, "tesouraria.movimento", mov("ENTRADA", 15_000_000, "Depósito das vendas", 10)).await;
        rpc(&st, "tesouraria.movimento", mov("SAIDA", 2_000_000, "Renda do armazém", 8)).await;
        rpc(&st, "tesouraria.movimento", mov("ENTRADA", 300_000, "TPA venda 1", 6)).await;
        rpc(&st, "tesouraria.movimento", mov("ENTRADA", 200_000, "TPA venda 2", 6)).await;
        // no mesmo dia, uma entrada de outra rubrica: o banco liquida só as vendas com cartão
        let centro = rpc(&st, "tesouraria.centroGuardar", json!({ "descricao": "Rendas recebidas", "tipo": "RECEITA" })).await["id"].as_i64().unwrap();
        let mut renda = mov("ENTRADA", 40_000, "Renda do anexo", 6);
        renda["centro_custo_id"] = json!(centro);
        rpc(&st, "tesouraria.movimento", renda).await;
        let tpa3 = rpc(&st, "tesouraria.movimento", mov("ENTRADA", 100_000, "TPA venda 3", 2)).await["id"].as_i64().unwrap();
        let tpa4 = rpc(&st, "tesouraria.movimento", mov("ENTRADA", 150_000, "TPA venda 4", 2)).await["id"].as_i64().unwrap();
        rpc(&st, "tesouraria.movimento", mov("ENTRADA", 700_000, "Depósito por confirmar", 4)).await;

        // o extracto do banco: título sem separadores, saldo anterior sem data, depósito um dia depois
        let csv = format!(
            "Extracto da conta 123456789\nData Mov.;Descritivo;Débito;Crédito;Saldo\n;Saldo anterior;;;0,00\n{};DEP. NUMERARIO;;150.000,00;150.000,00\n{};TRF RENDA;20.000,00;;130.000,00\n{};TPA VENDAS;;5.000,00;135.000,00\n{};COMISSAO MANUTENCAO;1.500,00;;133.500,00\n{};TPA VENDAS;;2.475,00;135.975,00\n",
            br(9),
            br(8),
            br(5),
            br(3),
            br(1)
        );
        use base64::Engine;
        let ficheiro = json!({ "nome": "extracto.csv", "conteudo": base64::engine::general_purpose::STANDARD.encode(csv.as_bytes()) });
        let v = rpc(&st, "conciliacao.ler", ficheiro.clone()).await;
        assert_eq!((v["movimentos"].as_i64(), v["ignoradas"].as_i64(), v["linha_cabecalho"].as_i64()), (Some(5), Some(1), Some(2)), "{v}");
        assert_eq!((v["entradas"].as_i64(), v["saidas"].as_i64(), v["saldo_banco"].as_i64()), (Some(15_747_500), Some(2_150_000), Some(13_597_500)), "{v}");
        assert_eq!((v["de"].as_str(), v["ate"].as_str()), (Some(d(9).as_str()), Some(d(1).as_str())));

        let mut pedido = ficheiro.clone();
        pedido["conta_id"] = json!(banco);
        let c = rpc(&st, "conciliacao.criar", pedido.clone()).await;
        assert_eq!((c["linhas"].as_i64(), c["casadas"].as_i64()), (Some(5), Some(3)), "depósito, renda e o TPA de um dia: {c}");
        let id = c["id"].as_i64().unwrap();
        assert!(erro(&st, "conciliacao.criar", pedido).await.contains("já foi importado"));

        let o = rpc(&st, "conciliacao.obter", json!({ "id": id })).await;
        let linha = |o: &Value, desc: &str| o["linhas"].as_array().unwrap().iter().find(|l| l["descricao"] == desc).cloned().unwrap();
        let tpa = o["linhas"].as_array().unwrap().iter().find(|l| l["descricao"] == "TPA VENDAS" && l["valor"] == 500_000).cloned().unwrap();
        assert_eq!(tpa["movimentos"].as_array().unwrap().len(), 2, "{tpa}");
        assert_eq!(o["resumo"]["linhas_por_casar"], 2, "{o}");
        assert_eq!(o["resumo"]["extracto_por_casar"], 247_500 - 150_000);
        assert_eq!(o["resumo"]["kangest_por_casar"], 700_000 + 100_000 + 150_000 + 40_000, "o depósito por confirmar, as duas vendas do TPA com comissão e a renda");
        assert_eq!(o["resumo"]["saldo_banco"], 13_597_500);

        // a comissão: lança-se a partir da linha (saída na tesouraria, com a data do extracto)
        let comissao = linha(&o, "COMISSAO MANUTENCAO")["id"].as_i64().unwrap();
        let l = rpc(&st, "conciliacao.lancar", json!({ "linha_id": comissao, "descricao": "Comissão de manutenção de conta" })).await;
        let t: (String, i64, String) = sqlx::query_as("SELECT tipo, valor, data FROM movimentos_tesouraria WHERE id = ?")
            .bind(l["movimento"].as_str().unwrap()[1..].parse::<i64>().unwrap())
            .fetch_one(&st.pool)
            .await
            .unwrap();
        assert_eq!(t, ("SAIDA".to_string(), 150_000, d(3)));

        // o TPA com a comissão descontada: as duas vendas (2 500,00) não batem com 2 475,00 sem lançar a diferença
        let tpa_c = o["linhas"].as_array().unwrap().iter().find(|l| l["valor"] == 247_500).unwrap()["id"].as_i64().unwrap();
        let chaves = json!([format!("T{tpa3}"), format!("T{tpa4}")]);
        assert!(erro(&st, "conciliacao.casar", json!({ "linha_id": tpa_c, "chaves": chaves })).await.contains("não batem"));
        rpc(&st, "conciliacao.casar", json!({ "linha_id": tpa_c, "chaves": chaves, "diferenca": { "descricao": "Comissão do TPA" } })).await;
        let dif: (String, i64) = sqlx::query_as("SELECT tipo, valor FROM movimentos_tesouraria WHERE descricao = 'Comissão do TPA'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(dif, ("SAIDA".to_string(), 2_500));
        // um movimento só se concilia uma vez
        let dep = linha(&o, "DEP. NUMERARIO")["id"].as_i64().unwrap();
        rpc(&st, "conciliacao.descasar", json!({ "linha_id": dep })).await;
        assert!(erro(&st, "conciliacao.casar", json!({ "linha_id": dep, "chaves": [format!("T{tpa3}")] })).await.contains("já está conciliado"));
        let o = rpc(&st, "conciliacao.obter", json!({ "id": id })).await;
        let deposito = o["pendentes"].as_array().unwrap().iter().find(|p| p["valor"] == 15_000_000).unwrap()["chave"].clone();
        rpc(&st, "conciliacao.casar", json!({ "linha_id": dep, "chaves": [deposito] })).await;
        let o = rpc(&st, "conciliacao.obter", json!({ "id": id })).await;
        assert_eq!(o["resumo"]["linhas_por_casar"], 0, "{o}");
        assert_eq!(o["resumo"]["kangest_por_casar"], 740_000, "o depósito e a renda que o banco ainda não mostra");
        // o saldo do KanGest no último dia do extracto difere do do banco por esses dois
        assert_eq!(o["resumo"]["saldo_kangest"].as_i64().unwrap() - o["resumo"]["saldo_banco"].as_i64().unwrap(), 740_000, "{o}");

        // fechada, já não muda nem se apaga; o operador de caixa não vê a tesouraria
        rpc(&st, "conciliacao.fechar", json!({ "id": id })).await;
        assert!(erro(&st, "conciliacao.descasar", json!({ "linha_id": dep })).await.contains("fechada"));
        assert!(erro(&st, "conciliacao.eliminar", json!({ "id": id })).await.contains("fechada"));
        let lista = rpc(&st, "conciliacao.listar", json!({ "conta_id": banco })).await;
        assert_eq!((lista[0]["linhas"].as_i64(), lista[0]["casadas"].as_i64(), lista[0]["estado"].as_str()), (Some(5), Some(5), Some("FECHADA")), "{lista}");
        let aud: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE entidade IN ('conciliacoes', 'conciliacao_linhas')").fetch_one(&st.pool).await.unwrap();
        assert_eq!(aud, 6, "criar, lançar, casar, descasar, casar e fechar");
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await;
        rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
        assert!(erro(&st, "conciliacao.listar", Value::Null).await.contains("permissão"));
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

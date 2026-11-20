// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Tipo de gestão, escolhido na instalação: lojas e comércio, escola ou (em breve) hospital.
//!
//! É um nível acima dos ramos de actividade: decide os menus e o que se prepara para começar. A escola usa
//! o mesmo motor de facturação, recibos e contas correntes (as propinas são avenças mensais), com os
//! artigos do ensino isentos de IVA (art. 12.º, n.º 1, al. l) do CIVA — código M21 da AGT).
//!
//! Fica em config('empresa.modo'); sem escolha (bases anteriores) é «loja».

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqliteConnection;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Tipos de gestão conhecidos.
pub const MODOS: &[&str] = &["loja", "escola", "hospital"];
/// Os que já se podem escolher (o hospital vem numa próxima versão).
pub const DISPONIVEIS: &[&str] = &["loja", "escola"];

const MAX_FAMILIAS: usize = 30;
const MAX_ARTIGOS: usize = 60;
/// 100 milhões de kwanzas, em cêntimos: um preço acima disto é engano de escrita.
const MAX_PRECO: i64 = 100_000_000 * 100;

/// `modo.*` (ver o despachante em services/mod.rs).
pub async fn despachar(st: &AppState, metodo: &str, params: Value) -> AppResult<Value> {
    use super::p;
    match metodo {
        "obter" => obter(st).await,
        "definir" => definir(st, p(params)?).await,
        _ => Err(AppError::msg(format!("Método desconhecido: modo.{metodo}"))),
    }
}

pub async fn modo<'e, E: sqlx::SqliteExecutor<'e>>(e: E) -> AppResult<String> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'empresa.modo'").fetch_optional(e).await?;
    Ok(v.filter(|m| DISPONIVEIS.contains(&m.as_str())).unwrap_or_else(|| "loja".into()))
}

/// Confere o tipo de gestão escolhido (o hospital ainda não se pode escolher).
pub fn validar_modo(modo: &str) -> AppResult<()> {
    if !MODOS.contains(&modo) {
        return Err(AppError::msg("Tipo de gestão desconhecido."));
    }
    if !DISPONIVEIS.contains(&modo) {
        return Err(AppError::msg("A gestão hospitalar ainda não está disponível: chega numa próxima versão do KanGest."));
    }
    Ok(())
}

pub(crate) async fn gravar_modo<'e, E: sqlx::SqliteExecutor<'e>>(e: E, modo: &str) -> AppResult<()> {
    validar_modo(modo)?;
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('empresa.modo', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(modo)
        .execute(e)
        .await?;
    Ok(())
}

/// Famílias e artigos para começar, revistos pelo utilizador antes de os criar (na instalação ou ao mudar
/// o tipo de gestão). O que já existe com o mesmo nome ou referência fica como está.
#[derive(Deserialize, Default)]
pub struct BaseIn {
    #[serde(default)]
    pub familias: Vec<String>,
    #[serde(default)]
    pub artigos: Vec<ArtigoBaseIn>,
}

#[derive(Deserialize)]
pub struct ArtigoBaseIn {
    referencia: String,
    descricao: String,
    /// nome da família (das indicadas acima ou de uma que já exista)
    #[serde(default)]
    familia: Option<String>,
    /// preço de venda em cêntimos (sem IVA; nos isentos é o mesmo que o PVP)
    #[serde(default)]
    preco: i64,
    /// P produto | S serviço (por omissão, serviço)
    #[serde(default)]
    tipo: Option<String>,
    /// abreviatura da unidade (UN, SV, MES…); por omissão SV no serviço e UN no produto
    #[serde(default)]
    unidade: Option<String>,
    /// motivo de isenção de IVA (ex.: M21, ensino); sem ele, IVA à taxa normal
    #[serde(default)]
    motivo_isencao: Option<String>,
}

fn referencia_valida(r: &str) -> bool {
    !r.is_empty() && r.chars().count() <= 20 && r.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
}

/// Confere a base antes de gravar o que quer que seja (a instalação não pode ficar a meio).
pub async fn validar_base(c: &mut SqliteConnection, b: &BaseIn) -> AppResult<()> {
    if b.familias.len() > MAX_FAMILIAS {
        return Err(AppError::msg(format!("No máximo {MAX_FAMILIAS} famílias de uma vez.")));
    }
    if b.artigos.len() > MAX_ARTIGOS {
        return Err(AppError::msg(format!("No máximo {MAX_ARTIGOS} artigos de uma vez.")));
    }
    for f in &b.familias {
        let f = f.trim();
        if f.is_empty() || f.chars().count() > 60 {
            return Err(AppError::msg("Cada família precisa de um nome (até 60 caracteres)."));
        }
    }
    let mut vistas = std::collections::HashSet::new();
    for a in &b.artigos {
        let r = a.referencia.trim().to_uppercase();
        let d = a.descricao.trim();
        if d.is_empty() || d.chars().count() > 200 {
            return Err(AppError::msg("Cada artigo precisa de uma descrição (até 200 caracteres)."));
        }
        if !referencia_valida(&r) {
            return Err(AppError::msg(format!("A referência de «{d}» só pode ter letras, algarismos e - _ . / (até 20).")));
        }
        if !vistas.insert(r.clone()) {
            return Err(AppError::msg(format!("A referência {r} aparece duas vezes.")));
        }
        if !(0..=MAX_PRECO).contains(&a.preco) {
            return Err(AppError::msg(format!("O preço de «{d}» não é válido.")));
        }
        if let Some(t) = a.tipo.as_deref() {
            if !matches!(t, "P" | "S") {
                return Err(AppError::msg(format!("O tipo de «{d}» tem de ser produto ou serviço.")));
            }
        }
        if let Some(fam) = a.familia.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            let nas_novas = b.familias.iter().any(|f| f.trim().eq_ignore_ascii_case(fam));
            let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE lower(descricao) = lower(?)").bind(fam).fetch_optional(&mut *c).await?;
            if !nas_novas && existe.is_none() {
                return Err(AppError::msg(format!("A família «{fam}» de «{d}» não existe.")));
            }
        }
        if let Some(u) = a.unidade.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM unidades WHERE upper(abreviatura) = upper(?)").bind(u).fetch_optional(&mut *c).await?;
            if existe.is_none() {
                return Err(AppError::msg(format!("A unidade {u} de «{d}» não existe.")));
            }
        }
        if let Some(m) = a.motivo_isencao.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            let existe: Option<String> = sqlx::query_scalar("SELECT codigo FROM motivos_isencao WHERE codigo = ?").bind(m).fetch_optional(&mut *c).await?;
            if existe.is_none() {
                return Err(AppError::msg(format!("O motivo de isenção {m} de «{d}» não existe.")));
            }
        }
    }
    Ok(())
}

async fn familia_id(c: &mut SqliteConnection, nome: &str) -> AppResult<i64> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE lower(descricao) = lower(?)").bind(nome).fetch_optional(&mut *c).await?;
    Ok(match id {
        Some(id) => id,
        None => sqlx::query("INSERT INTO familias (descricao) VALUES (?)").bind(nome).execute(&mut *c).await?.last_insert_rowid(),
    })
}

/// Cria as famílias e os artigos da base (já validada). Devolve quantos criou de cada.
pub async fn criar_base(c: &mut SqliteConnection, b: &BaseIn) -> AppResult<(usize, usize)> {
    let mut familias = 0;
    for f in &b.familias {
        let f = f.trim();
        let ja: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE lower(descricao) = lower(?)").bind(f).fetch_optional(&mut *c).await?;
        if ja.is_none() {
            sqlx::query("INSERT INTO familias (descricao) VALUES (?)").bind(f).execute(&mut *c).await?;
            familias += 1;
        }
    }
    let isenta: Option<(i64, f64)> = sqlx::query_as("SELECT id, percentagem FROM taxas_iva WHERE codigo = 'ISE' ORDER BY id LIMIT 1").fetch_optional(&mut *c).await?;
    let normal: Option<(i64, f64)> = sqlx::query_as("SELECT id, percentagem FROM taxas_iva WHERE codigo = 'NOR' ORDER BY percentagem DESC, id LIMIT 1").fetch_optional(&mut *c).await?;
    let mut artigos = 0;
    for a in &b.artigos {
        let referencia = a.referencia.trim().to_uppercase();
        let ja: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ?").bind(&referencia).fetch_optional(&mut *c).await?;
        if ja.is_some() {
            continue;
        }
        let tipo = a.tipo.clone().unwrap_or_else(|| "S".into());
        let motivo = a.motivo_isencao.as_deref().map(str::trim).filter(|m| !m.is_empty());
        let (taxa_id, pct) = match motivo {
            Some(_) => isenta.ok_or_else(|| AppError::msg("Falta a taxa de IVA «Isento» na tabela de taxas."))?,
            None => normal.ok_or_else(|| AppError::msg("Falta a taxa normal de IVA na tabela de taxas."))?,
        };
        let unidade = a.unidade.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(if tipo == "S" { "SV" } else { "UN" });
        let unidade_id: Option<i64> = sqlx::query_scalar("SELECT id FROM unidades WHERE upper(abreviatura) = upper(?) ORDER BY id LIMIT 1").bind(unidade).fetch_optional(&mut *c).await?;
        let familia = match a.familia.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            Some(f) => Some(familia_id(c, f).await?),
            None => None,
        };
        sqlx::query(
            "INSERT INTO artigos (referencia, descricao, tipo, familia_id, unidade_id, taxa_iva_id, motivo_isencao, preco1, pvp1, controla_stock, ativo)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(&referencia)
        .bind(a.descricao.trim())
        .bind(&tipo)
        .bind(familia)
        .bind(unidade_id)
        .bind(taxa_id)
        .bind(motivo)
        .bind(a.preco)
        .bind(super::documentos::com_iva(a.preco, pct))
        .bind((tipo == "P") as i64)
        .execute(&mut *c)
        .await?;
        artigos += 1;
    }
    Ok((familias, artigos))
}

pub async fn obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    Ok(json!({ "modo": modo(&st.pool).await? }))
}

#[derive(Deserialize)]
pub struct DefinirIn {
    modo: String,
    /// famílias e artigos a criar com a mudança (ex.: os do ensino ao passar a escola)
    #[serde(default)]
    base: Option<BaseIn>,
}

/// Muda o tipo de gestão depois da instalação (só muda os menus e o que se prepara; os dados ficam).
pub async fn definir(st: &AppState, d: DefinirIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    let modo = d.modo.trim();
    validar_modo(modo)?;
    let mut tx = crate::db::transacao(&st.pool).await?;
    let (mut familias, mut artigos) = (0, 0);
    if let Some(b) = &d.base {
        validar_base(&mut tx, b).await?;
        (familias, artigos) = criar_base(&mut tx, b).await?;
    }
    gravar_modo(&mut *tx, modo).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'CONFIG', ?)")
        .bind(s.id)
        .bind(format!("Tipo de gestão: {modo} ({familias} famílias e {artigos} artigos criados)"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "modo": modo, "familias": familias, "artigos": artigos }))
}

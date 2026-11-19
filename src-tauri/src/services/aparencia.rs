//! Aparência da empresa: o jogo de cores do sistema (escolhido na instalação e em Configurações → Aparência) e a
//! imagem do ecrã de entrada. Ficam na tabela `config` e valem para todos os postos e browsers; o ecrã de entrada lê-os
//! antes de haver sessão (`setup.estado`, `aparencia.imagemEntrada`). O tema claro ou escuro continua a ser de cada um.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const CHAVE_CORES: &str = "aparencia.cores";
const CHAVE_IMAGEM: &str = "aparencia.imagem_entrada";
/// Os jogos de cores prontos (as cores de cada um estão em src/lib/cores.ts). «petroleo» é o do KanGest.
pub const PALETAS: [&str; 7] = ["petroleo", "azul", "oceano", "vinho", "floresta", "grafite", "violeta"];
/// A imagem chega em data URL, já reduzida no browser; o limite deixa folga para 1600 px em JPEG ou WebP.
const IMAGEM_MAX: usize = 2_000_000;

fn hex(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

/// Confere e normaliza as cores: `{"paleta": "<um dos PALETAS>"}` ou
/// `{"paleta": "personalizada", "marca": "#rrggbb", "destaque": "#rrggbb"}`.
pub fn validar_cores(v: &Value) -> AppResult<Value> {
    let paleta = v.get("paleta").and_then(Value::as_str).unwrap_or("").trim();
    if PALETAS.contains(&paleta) {
        return Ok(json!({ "paleta": paleta }));
    }
    if paleta == "personalizada" {
        let cor = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
        let (marca, destaque) = (cor("marca"), cor("destaque"));
        if hex(&marca) && hex(&destaque) {
            return Ok(json!({ "paleta": "personalizada", "marca": marca, "destaque": destaque }));
        }
        return Err(AppError::msg("Escolha as duas cores do jogo personalizado (a principal e a de destaque)."));
    }
    Err(AppError::msg("Jogo de cores desconhecido."))
}

/// As cores guardadas (`None`: as do KanGest).
pub async fn cores(pool: &SqlitePool) -> AppResult<Option<Value>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE_CORES).fetch_optional(pool).await?;
    Ok(v.and_then(|s| serde_json::from_str(&s).ok()))
}

pub async fn gravar_cores(pool: &SqlitePool, v: &Value) -> AppResult<Value> {
    let c = validar_cores(v)?;
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(CHAVE_CORES)
        .bind(c.to_string())
        .execute(pool)
        .await?;
    Ok(c)
}

/// Há imagem própria para o ecrã de entrada?
pub async fn tem_imagem(pool: &SqlitePool) -> AppResult<bool> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM config WHERE chave = ?").bind(CHAVE_IMAGEM).fetch_one(pool).await? > 0)
}

/// A imagem do ecrã de entrada da empresa (pública: mostra-se antes de entrar). `null` sem imagem própria.
pub async fn imagem_entrada(st: &AppState) -> AppResult<Value> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE_IMAGEM).fetch_optional(&st.pool).await?;
    Ok(json!({ "imagem": v }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    /// O jogo de cores (ver `validar_cores`); sem ele, fica como está.
    #[serde(default)]
    cores: Option<Value>,
    /// Imagem do ecrã de entrada em data URL (JPEG, PNG ou WebP); `""` tira-a (volta a do KanGest); sem ela, fica.
    #[serde(default)]
    imagem_entrada: Option<String>,
}

/// Configurações → Aparência (permissão de configurar o sistema).
pub async fn guardar(st: &AppState, i: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    if let Some(img) = i.imagem_entrada.as_deref() {
        if !img.is_empty() {
            let tipo_ok = ["data:image/jpeg;base64,", "data:image/png;base64,", "data:image/webp;base64,"].iter().any(|p| img.starts_with(p));
            if !tipo_ok || img.len() > IMAGEM_MAX {
                return Err(AppError::msg("A imagem do ecrã de entrada deve ser JPEG, PNG ou WebP com menos de 1,5 MB."));
            }
        }
    }
    let cores = match i.cores.as_ref() {
        Some(c) => Some(gravar_cores(&st.pool, c).await?),
        None => cores(&st.pool).await?,
    };
    match i.imagem_entrada.as_deref() {
        Some("") => {
            sqlx::query("DELETE FROM config WHERE chave = ?").bind(CHAVE_IMAGEM).execute(&st.pool).await?;
        }
        Some(img) => {
            sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
                .bind(CHAVE_IMAGEM)
                .bind(img)
                .execute(&st.pool)
                .await?;
        }
        None => {}
    }
    Ok(json!({ "cores": cores, "imagem_entrada": tem_imagem(&st.pool).await? }))
}

//! Aparência da empresa: o jogo de cores do sistema (escolhido na instalação e em Configurações → Aparência). Fica na
//! tabela `config` e vale para todos os postos e browsers; o ecrã de entrada lê-o antes de haver sessão
//! (`setup.estado`). O tema claro ou escuro continua a ser de cada um. A imagem do ecrã de entrada é a do KanGest e não
//! se troca.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const CHAVE_CORES: &str = "aparencia.cores";
/// Os jogos de cores prontos (as cores de cada um estão em src/lib/cores.ts). «petroleo» é o do KanGest.
pub const PALETAS: [&str; 7] = ["petroleo", "azul", "oceano", "vinho", "floresta", "grafite", "violeta"];

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

#[derive(Deserialize)]
pub struct GuardarIn {
    /// O jogo de cores (ver `validar_cores`).
    cores: Value,
}

/// Configurações → Aparência (permissão de configurar o sistema).
pub async fn guardar(st: &AppState, i: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    Ok(json!({ "cores": gravar_cores(&st.pool, &i.cores).await? }))
}

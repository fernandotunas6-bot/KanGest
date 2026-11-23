// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Funções nativas do sistema (Windows e macOS) pelos plugins do Tauri. O `AppHandle` guarda-se no arranque
//! (`definir`), para os serviços e as tarefas de fundo lhe chegarem; nos testes não há, e as funções dizem-no.
//!
//! - **Arranque automático** (`tauri-plugin-autostart`): o KanGest abre ao ligar o computador. É o que se quer no
//!   computador servidor da loja, de que dependem os postos em rede, o acesso pela Internet e as cópias.
//!   Windows: a chave «Run» do utilizador; macOS: um LaunchAgent.
//! - **Uma só cópia aberta** (`tauri-plugin-single-instance`, em `lib.rs`): abrir o KanGest outra vez traz a janela
//!   aberta para a frente, em vez de uma segunda cópia a escrever na mesma base (e a repetir as cópias de
//!   segurança e os envios à AGT).

use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri_plugin_autostart::ManagerExt;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Guarda o `AppHandle` (uma vez, no arranque).
pub fn definir(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

fn app() -> AppResult<&'static tauri::AppHandle> {
    APP.get().ok_or_else(|| AppError::msg("Esta função só existe na aplicação instalada."))
}

/// O arranque automático deste computador (método local: cada computador tem o seu).
pub async fn arranque(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let ativo = app()?.autolaunch().is_enabled().map_err(|e| AppError::msg(format!("Arranque automático: {e}")))?;
    Ok(json!({ "ativo": ativo, "servidor": crate::rede::config().modo == "servidor" }))
}

#[derive(Deserialize)]
pub struct ArranqueIn {
    ativo: bool,
}

pub async fn arranque_guardar(st: &AppState, i: ArranqueIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let gestor = app()?.autolaunch();
    let r = if i.ativo { gestor.enable() } else { gestor.disable() };
    r.map_err(|e| AppError::msg(format!("Não foi possível mudar o arranque automático: {e}")))?;
    arranque(st).await
}

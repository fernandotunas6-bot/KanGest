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
//! - **Notificações do sistema** (`tauri-plugin-notification`): o que falha em segundo plano, quando ninguém está a
//!   olhar para o KanGest — cópias de segurança, facturas rejeitadas pela AGT, integrações que deixaram de responder.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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

/// O mesmo aviso (a mesma `chave`) só volta a aparecer depois deste tempo.
const INTERVALO_AVISOS: Duration = Duration::from_secs(60 * 60);

/// Pode mostrar-se o aviso `chave` agora? Regista-o, se sim.
fn pode_avisar(ultimos: &mut HashMap<String, Instant>, chave: &str, agora: Instant) -> bool {
    match ultimos.get(chave) {
        Some(antes) if agora.duration_since(*antes) < INTERVALO_AVISOS => false,
        _ => {
            ultimos.insert(chave.to_string(), agora);
            true
        }
    }
}

/// Notificação do sistema. `chave` identifica o aviso (ex.: «copia-externa»): o mesmo aviso aparece no máximo uma vez
/// por hora. Sem a aplicação (testes) ou se o sistema a recusar, não faz nada (o erro fica no registo).
pub fn notificar(chave: &str, titulo: &str, texto: &str) {
    use tauri_plugin_notification::NotificationExt;
    let Some(app) = APP.get() else { return };
    static ULTIMOS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let mut ultimos = ULTIMOS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if !pode_avisar(&mut ultimos, chave, Instant::now()) {
        return;
    }
    drop(ultimos);
    if let Err(e) = app.notification().builder().title(titulo).body(texto).show() {
        eprintln!("Notificação «{titulo}» não mostrada: {e}");
    }
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

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_mesmo_aviso_so_uma_vez_por_hora() {
        let mut u = HashMap::new();
        let t0 = Instant::now();
        assert!(pode_avisar(&mut u, "copia", t0));
        assert!(!pode_avisar(&mut u, "copia", t0 + Duration::from_secs(60)), "repetido logo a seguir: não");
        assert!(pode_avisar(&mut u, "agt", t0 + Duration::from_secs(60)), "outro aviso: sim");
        assert!(pode_avisar(&mut u, "copia", t0 + INTERVALO_AVISOS), "uma hora depois: sim");
        // sem a aplicação (testes) não faz nada nem falha
        notificar("teste", "Título", "Texto");
    }
}


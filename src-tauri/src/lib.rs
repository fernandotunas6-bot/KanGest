mod agt;
mod db;
mod error;
pub mod http;
mod rede;
mod services;
mod state;

use std::sync::Arc;

use serde_json::Value;
use tauri::Manager;

use error::AppError;
use state::AppState;

/// Ponto único de entrada do frontend: `invoke("rpc", { method, params })`.
/// Em modo cliente o mesmo pedido segue para o servidor da rede (ver `rede.rs`).
#[tauri::command]
async fn rpc(state: tauri::State<'_, Arc<AppState>>, method: String, params: Option<Value>) -> Result<Value, AppError> {
    rede::chamar(&state, &method, params.unwrap_or(Value::Null)).await
}

/// Diálogo de impressão nativo. No macOS o `window.print()` do WKWebView não funciona,
/// por isso o frontend usa este comando; no Windows (WebView2) usa `window.print()`.
#[tauri::command]
fn imprimir(janela: tauri::WebviewWindow) -> Result<(), String> {
    janela.print().map_err(|e| e.to_string())
}

/// N.º do posto desta máquina (ficheiro local; a base de dados pode vir a ser partilhada).
fn posto_local(dir: &std::path::Path) -> i64 {
    std::fs::read_to_string(dir.join("posto.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["posto_id"].as_i64())
        .unwrap_or(1)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let caminho = dir.join("kigest.db");
            let pool = tauri::async_runtime::block_on(db::abrir(&caminho))?;
            let mut estado = AppState::new(pool);
            estado.pasta_dados = dir.clone();
            estado.pasta_exportacao = app.path().download_dir().unwrap_or_else(|_| dir.clone());
            tauri::async_runtime::block_on(async {
                estado.carregar_posto(posto_local(&dir)).await?;
                // Cópia de segurança automática diária (não impede o arranque se falhar).
                if let Err(e) = services::sistema::backup_automatico(&estado.pool, &dir).await {
                    eprintln!("Cópia de segurança automática falhou: {e}");
                }
                Ok::<_, AppError>(())
            })?;
            // Postos em rede: abre o servidor ou liga-se a ele conforme o `rede.json` desta máquina.
            let estado = Arc::new(estado);
            rede::registar_estado(estado.clone());
            let config_rede = rede::carregar(&dir);
            if config_rede.modo == "servidor" {
                if let Err(e) = rede::iniciar_servidor(estado.clone(), &config_rede) {
                    eprintln!("Servidor de rede não arrancou: {e}");
                }
            } else if config_rede.cliente() {
                let st = estado.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = rede::sincronizar_posto(&st).await {
                        eprintln!("Sem ligação ao servidor: {e}");
                    }
                });
            }
            // Facturação electrónica da AGT: envia e consulta os documentos em segundo plano.
            services::fe::iniciar(estado.clone());
            app.manage(estado);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc, imprimir])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o KanGest");
}

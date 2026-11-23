// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

mod agt;
pub mod api;
mod auditoria;
mod db;
mod enderecos;
mod error;
mod nativo;
pub mod http;
pub mod internet;
mod produtor;
mod rede;
mod segredos;
pub mod selo;
mod services;
mod smtp;
mod state;
mod web;
pub mod webhooks;

use std::sync::Arc;

/// Pilha das threads que atendem pedidos (aplicação, postos em rede, browser, API). A emissão de documentos é uma
/// função assíncrona muito grande e, nas compilações de desenvolvimento, chega perto dos 2 MB de pilha por omissão
/// das threads; com 8 MB há margem larga (nas compilações oficiais usa muito menos).
pub(crate) const PILHA_PEDIDOS: usize = 8 * 1024 * 1024;

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

/// Reinicia a aplicação (depois de marcar um restauro de cópia de segurança).
#[tauri::command]
fn reiniciar(app: tauri::AppHandle) {
    app.restart();
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
    // o runtime assíncrono da aplicação com threads de pilha maior (ver PILHA_PEDIDOS)
    match tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(PILHA_PEDIDOS).thread_name("kangest-rt").build() {
        Ok(rt) => {
            tauri::async_runtime::set(rt.handle().clone());
            // o runtime vive enquanto o programa viver
            std::mem::forget(rt);
        }
        Err(e) => eprintln!("Runtime assíncrono por omissão (não foi possível criar o próprio: {e})"),
    }
    tauri::Builder::default()
        // tem de ser o primeiro plugin: abrir o KanGest outra vez traz a janela aberta para a frente, em vez de uma
        // segunda cópia a escrever na mesma base (src/nativo.rs)
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(janela) = app.get_webview_window("main") {
                let _ = janela.unminimize();
                let _ = janela.show();
                let _ = janela.set_focus();
            }
        }))
        // abrir ao ligar o computador (Configurações → Rede): chave «Run» no Windows, LaunchAgent no macOS
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        // notificações do sistema: o que falha em segundo plano (src/nativo.rs, notificar)
        .plugin(tauri_plugin_notification::init())
        // janelas nativas de guardar e abrir (o ecrã usa-as em src/lib/dialogos.ts; permissões em capabilities/nativo.json)
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            nativo::definir(app.handle().clone());
            // selo de integridade do executável (só nas compilações oficiais; corre numa thread à parte)
            if let Ok(recursos) = app.path().resource_dir() {
                selo::iniciar(recursos);
            } else {
                selo::iniciar(std::path::PathBuf::new());
            }
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let caminho = dir.join("kigest.db");
            // chave mestra dos segredos da configuração (fora da base de dados)
            segredos::iniciar(&dir)?;
            // proxy desta máquina para a Internet (internet.json; sem ele, o do sistema)
            internet::carregar(&dir);
            // restauro de uma cópia de segurança marcado antes de reiniciar
            match services::sistema::aplicar_restauro_pendente(&dir) {
                Ok(Some(origem)) => eprintln!("Cópia de segurança restaurada: {origem}"),
                Ok(None) => {}
                Err(e) => eprintln!("O restauro da cópia de segurança falhou: {e}"),
            }
            let pool = tauri::async_runtime::block_on(db::abrir(&caminho))?;
            if let Err(e) = tauri::async_runtime::block_on(segredos::cifrar_existentes(&pool)) {
                eprintln!("Não foi possível cifrar os segredos guardados: {e}");
            }
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
            // Modo web: o servidor de rede entrega ao browser os mesmos ficheiros da aplicação embutidos aqui.
            let activos = app.asset_resolver();
            web::registar(Box::new(move |c: &str| activos.get(c.to_string()).map(|a| (a.bytes, a.mime_type))));
            // Postos em rede: abre o servidor ou liga-se a ele conforme o `rede.json` desta máquina.
            let estado = Arc::new(estado);
            rede::registar_estado(estado.clone());
            api::registar_estado(estado.clone());
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
            // API de integração para outros sistemas: só na máquina que guarda a base de dados.
            if !config_rede.cliente() {
                api::arrancar(estado.clone());
            }
            // acesso pela Internet (subdomínio do KanGest): volta a ligar o túnel, se estiver activado
            services::acesso_web::arrancar(estado.clone());
            // avisos automáticos da API (webhooks) às lojas online e outros sistemas
            webhooks::iniciar(estado.clone());
            // Facturação electrónica da AGT: envia e consulta os documentos em segundo plano.
            services::fe::iniciar(estado.clone());
            // Cópias de segurança periódicas enquanto o programa está aberto.
            services::sistema::iniciar_agendamento(estado.clone());
            // Resumo diário ao dono por email (se estiver ligado no painel).
            services::resumo_email::iniciar(estado.clone());
            app.manage(estado);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc, imprimir, reiniciar])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o KanGest");
}

// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Verificação de humano (Cloudflare Turnstile) na entrada pela Internet, como no Claude.
//!
//! Só para os KanGest com endereço do produtor (<nome>.<domínio do KanGest>): o widget do produtor só serve esse
//! domínio. Na rede da loja e num domínio próprio não se pede. A chave pública e a confirmação vêm do servidor de
//! licenças (o segredo não sai de lá); sem resposta dele, deixa entrar e regista — uma loja que não entra porque um
//! serviço de fora caiu é pior do que um robô a mais durante esse tempo.
//!
//! Cada token do Turnstile só se usa uma vez: confirmado, o aparelho fica confirmado 10 minutos, e as voltas seguintes
//! da mesma entrada (o código de dois passos, «Sair e trocar de conta») não pedem outro.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const VALIDADE: Duration = Duration::from_secs(600);
static CONFIRMADOS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// A frase para quem entra quando falta a verificação ou ela não vale.
pub const MENSAGEM: &str = "Confirme que não é um robô (a caixa da Cloudflare, por cima do botão Entrar) e tente outra vez.";

/// Nos testes: por empresa, (chave pública, resposta da verificação). Os testes nunca vão à rede.
#[cfg(test)]
pub(crate) static TESTE: Mutex<Option<HashMap<String, (Option<String>, bool)>>> = Mutex::new(None);

/// A entrada deste pedido pede a verificação: veio pela Internet e o endereço é do produtor (não um domínio próprio).
async fn exigida(st: &AppState) -> bool {
    crate::state::pela_internet() && !super::acesso_web::proprio(st).await.unwrap_or(true)
}

/// A chave pública do widget para o ecrã de entrada deste pedido (None = não se pede).
pub async fn chave_site(st: &AppState) -> Option<String> {
    if !exigida(st).await {
        return None;
    }
    #[cfg(test)]
    {
        teste(st).await.and_then(|t| t.0)
    }
    #[cfg(not(test))]
    {
        chave_do_servidor().await
    }
}

/// Confere a verificação de humano, antes da senha. Sem ela (ou com ela recusada), a entrada não segue.
pub async fn confirmar(st: &AppState, token: Option<&str>) -> AppResult<()> {
    if chave_site(st).await.is_none() {
        return Ok(());
    }
    let aparelho = crate::state::contexto().and_then(|c| c.ligacao).unwrap_or_default();
    {
        let mut g = CONFIRMADOS.lock().unwrap();
        let m = g.get_or_insert_with(HashMap::new);
        m.retain(|_, t| t.elapsed() < VALIDADE);
        if m.contains_key(&aparelho) {
            return Ok(());
        }
    }
    let token = token.map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| AppError::msg(MENSAGEM))?;
    if !verificar(st, token).await {
        return Err(AppError::msg(MENSAGEM));
    }
    CONFIRMADOS.lock().unwrap().get_or_insert_with(HashMap::new).insert(aparelho, Instant::now());
    Ok(())
}

#[cfg(not(test))]
async fn chave_do_servidor() -> Option<String> {
    // uma hora com a chave; sem resposta do servidor, volta a perguntar daí a 5 minutos (e até lá não se pede)
    static CHAVE: Mutex<Option<(Instant, Duration, Option<String>)>> = Mutex::new(None);
    if let Some((quando, dura, c)) = CHAVE.lock().unwrap().clone() {
        if quando.elapsed() < dura {
            return c;
        }
    }
    let (c, dura) = match super::licenca::obter_servidor("/v1/web/humano").await {
        Ok(v) => (v["site_key"].as_str().map(str::to_string).filter(|s| !s.is_empty()), Duration::from_secs(3600)),
        Err(_) => (None, Duration::from_secs(300)),
    };
    *CHAVE.lock().unwrap() = Some((Instant::now(), dura, c.clone()));
    c
}

async fn verificar(st: &AppState, token: &str) -> bool {
    #[cfg(test)]
    {
        let _ = token;
        teste(st).await.is_some_and(|t| t.1)
    }
    #[cfg(not(test))]
    {
        let _ = st;
        match super::licenca::chamar_servidor("/v1/web/humano", serde_json::json!({ "token": token })).await {
            Ok(v) => v["humano"].as_bool().unwrap_or(true),
            Err(e) => {
                eprintln!("Verificação de humano sem resposta do servidor de licenças (deixa entrar): {e}");
                true
            }
        }
    }
}

#[cfg(test)]
async fn teste(st: &AppState) -> Option<(Option<String>, bool)> {
    let empresa: String = sqlx::query_scalar("SELECT nome FROM empresa WHERE id = 1").fetch_one(&st.pool).await.ok()?;
    TESTE.lock().unwrap().as_ref()?.get(&empresa).cloned()
}

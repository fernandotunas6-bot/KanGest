//! Ecrã «Ligação à Internet»: proxy desta máquina (`crate::internet`) e diagnóstico de cada serviço externo.

use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::internet::{self, Config};
use crate::state::AppState;

/// Site público do KanGest (para saber se há Internet, sem depender de nenhum serviço do programa).
const SITE: &str = "https://kangest.pages.dev/";
/// Origem das actualizações publicadas pela GitHub Actions.
pub const ORIGEM_GITHUB: &str = "https://github.com/fernandotunas6-bot/KanGest/releases/latest/download";

fn descrever(p: &Option<internet::Proxy>) -> Value {
    match p {
        Some(p) => json!({ "endereco": format!("{}:{}", p.host, p.porta), "origem": p.origem, "autenticacao": p.autorizacao.is_some() }),
        None => Value::Null,
    }
}

pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let c = internet::config();
    Ok(json!({
        "proxy": c.proxy, "utilizador": c.utilizador, "tem_senha": !c.senha.is_empty(), "excepcoes": c.excepcoes, "do_sistema": c.do_sistema,
        // o que vale agora para a Internet (pode vir das variáveis ou do Windows)
        "efectivo": descrever(&internet::proxy_para("kangest.pages.dev", true)),
        "origem_actualizacoes": ORIGEM_GITHUB,
    }))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    #[serde(default)]
    proxy: String,
    #[serde(default)]
    utilizador: String,
    /// Sem valor: mantém a senha guardada; vazio: apaga-a.
    #[serde(default)]
    senha: Option<String>,
    #[serde(default)]
    excepcoes: String,
    #[serde(default = "sim")]
    do_sistema: bool,
}

fn sim() -> bool {
    true
}

pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    let proxy = g.proxy.trim().to_string();
    if !proxy.is_empty() {
        internet::partir_proxy(&proxy).map_err(AppError::msg)?;
    }
    let actual = internet::config();
    let senha = match g.senha {
        None => actual.senha,
        Some(x) if x.is_empty() => String::new(),
        Some(x) => crate::segredos::cifrar(&x),
    };
    let c = Config { proxy, utilizador: g.utilizador.trim().to_string(), senha, excepcoes: g.excepcoes.trim().to_string(), do_sistema: g.do_sistema };
    internet::gravar(&st.pasta_dados, &c)?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'INTERNET', ?)")
        .bind(s.id)
        .bind(if c.proxy.is_empty() { "Ligação à Internet: sem proxy próprio".to_string() } else { format!("Ligação à Internet pelo proxy {}", c.proxy) })
        .execute(&st.pool)
        .await?;
    estado(st).await
}

/// Um teste. Para os serviços, qualquer resposta HTTP conta (chegou-se lá); para um ficheiro que tem de existir
/// (o manifesto das actualizações), só a resposta 200. `alcancado` diz se houve resposta (há Internet até lá).
fn testar_url(servico: &str, url: &str, ficheiro: bool) -> Value {
    let inicio = Instant::now();
    let host = url.split("://").nth(1).and_then(|r| r.split(['/', ':']).next()).unwrap_or("").to_string();
    let via = descrever(&internet::proxy_para(&host, url.starts_with("https://")));
    let r = crate::http::pedido("GET", url, &[], None, Duration::from_secs(15));
    let ms = inicio.elapsed().as_millis() as u64;
    match r {
        Ok(resp) if ficheiro && resp.estado != 200 => json!({
            "servico": servico, "endereco": url, "ok": false, "alcancado": true, "ms": ms, "via": via,
            "erro": format!("O endereço responde, mas o ficheiro não está lá (HTTP {}): confirme a origem ou publique uma versão com o manifesto.", resp.estado),
        }),
        Ok(resp) => json!({ "servico": servico, "endereco": url, "ok": true, "alcancado": true, "ms": ms, "detalhe": format!("Responde (HTTP {})", resp.estado), "via": via }),
        Err(e) => json!({ "servico": servico, "endereco": url, "ok": false, "alcancado": false, "ms": ms, "erro": e.to_string(), "via": via }),
    }
}

/// O servidor de email só se testa até abrir a ligação (sem enviar nada).
fn testar_tcp(servico: &str, host: &str, porta: u16) -> Value {
    let inicio = Instant::now();
    // um nome com IPv6 e IPv4 tenta os dois, pela ordem do sistema
    let alvo = crate::enderecos::alvo(host, porta);
    let r = crate::enderecos::ligar(&alvo, Duration::from_secs(10)).map_err(|e| format!("Sem ligação a {alvo}: {e}"));
    let ms = inicio.elapsed().as_millis() as u64;
    let endereco = format!("{host}:{porta}");
    match r {
        Ok(_) => json!({ "servico": servico, "endereco": endereco, "ok": true, "alcancado": true, "ms": ms, "detalhe": "Ligação aberta", "via": Value::Null }),
        Err(e) => json!({ "servico": servico, "endereco": endereco, "ok": false, "alcancado": false, "ms": ms, "erro": e, "via": Value::Null }),
    }
}

/// Testa, em paralelo, a Internet e cada serviço que o KanGest usa a partir desta máquina.
pub async fn diagnostico(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let mut urls: Vec<(String, String, bool)> = vec![
        ("Internet (site do KanGest)".into(), SITE.into(), false),
        ("Servidor de licenças".into(), format!("{}/", super::licenca::servidor()), false),
    ];
    let fonte = super::atualizacao::ler_cfg(&st.pasta_dados).fonte;
    let fonte = if fonte.trim().is_empty() { ORIGEM_GITHUB.to_string() } else { fonte.trim().trim_end_matches('/').to_string() };
    let pasta_actualizacoes = (!fonte.starts_with("http")).then(|| fonte.clone());
    if pasta_actualizacoes.is_none() {
        let manifesto = if fonte.ends_with(".json") { fonte.clone() } else { format!("{fonte}/{}", super::atualizacao::MANIFESTO) };
        urls.push(("Actualizações (manifesto)".into(), manifesto, true));
    }
    let fe = super::fe::cfg(&st.pool).await?;
    if fe.ativo {
        urls.push((format!("Facturação Electrónica da AGT ({})", if fe.ambiente == "PROD" { "produção" } else { "homologação" }), format!("{}/", super::fe::endereco_servicos(&fe)), false));
    }
    let email = super::partilha::cfg(&st.pool).await?;
    let resultados = tokio::task::spawn_blocking(move || {
        std::thread::scope(|s| {
            let mut tarefas: Vec<_> = urls.iter().map(|(n, u, f)| s.spawn(move || testar_url(n, u, *f))).collect();
            if let Some(p) = &pasta_actualizacoes {
                let p = p.clone();
                tarefas.push(s.spawn(move || match std::fs::read_dir(&p) {
                    Ok(_) => json!({ "servico": "Actualizações (pasta partilhada)", "endereco": p, "ok": true, "alcancado": true, "ms": 0, "detalhe": "Pasta acessível", "via": Value::Null }),
                    Err(e) => json!({ "servico": "Actualizações (pasta partilhada)", "endereco": p, "ok": false, "alcancado": false, "ms": 0, "erro": format!("Pasta inacessível: {e}"), "via": Value::Null }),
                }));
            }
            if !email.servidor.trim().is_empty() {
                let (h, p) = (email.servidor.trim().to_string(), email.porta);
                tarefas.push(s.spawn(move || testar_tcp("Email (servidor de envio)", &h, p)));
            }
            tarefas.into_iter().filter_map(|t| t.join().ok()).collect::<Vec<Value>>()
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?;
    // há Internet se algum endereço da Internet respondeu (um serviço bloqueado não quer dizer que não haja rede)
    let internet_ok = resultados.iter().any(|r| r["alcancado"] == true && r["endereco"].as_str().is_some_and(|e| e.starts_with("http")));
    let todos = resultados.iter().all(|r| r["ok"] == true);
    Ok(json!({ "resultados": resultados, "internet": internet_ok, "todos": todos, "proxy": descrever(&internet::proxy_para("kangest.pages.dev", true)) }))
}

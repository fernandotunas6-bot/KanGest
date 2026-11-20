// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Acesso pela Internet com endereço próprio do KanGest (ex.: `https://loja.kangest.ao`), como os programas na nuvem.
//!
//! O servidor de licenças cria, em nome do produtor, um túnel da Cloudflare até este computador e o registo DNS
//! (`servidor-licencas`, rotas `/v1/web/…`). Aqui guarda-se o token do túnel (cifrado, `segredos`) e corre-se o
//! `cloudflared`, que liga o túnel de dentro para fora: não se abre nenhuma porta no router e o browser recebe HTTPS.
//! Os dados ficam no computador da loja; a loja continua a vender sem Internet. Só no computador servidor e com o
//! modo web ligado (docs/WEB-PWA.md).
//!
//! O `cloudflared` (versão fixa abaixo) descarrega-se da release oficial no GitHub só quando o acesso é activado e
//! confere-se pelo SHA-256 que o GitHub publica para esse ficheiro; fica na pasta de dados.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Versão do cloudflared que o KanGest usa (muda-se com uma versão nova do KanGest).
pub const VERSAO_CLOUDFLARED: &str = "2026.10.7";
const LINHAS_GUARDADAS: usize = 40;

// ============================== estado do túnel ==============================

#[derive(Default)]
struct Tunel {
    /// Cada arranque tem um número: um supervisor antigo que acorde depois de parado não volta a lançar nada.
    geracao: u64,
    a_correr: bool,
    ligado: bool,
    erro: Option<String>,
    linhas: VecDeque<String>,
    filho: Option<Child>,
}

fn tunel() -> &'static Mutex<Tunel> {
    static T: OnceLock<Mutex<Tunel>> = OnceLock::new();
    T.get_or_init(Default::default)
}

fn anotar(geracao: u64, linha: &str) {
    let mut t = tunel().lock().unwrap();
    if t.geracao == geracao {
        aplicar_linha(&mut t, linha);
    }
}

/// O que uma linha do cloudflared diz do túnel: ligado («Registered tunnel connection») ou o último erro.
fn aplicar_linha(t: &mut Tunel, linha: &str) {
    if linha.contains("Registered tunnel connection") {
        t.ligado = true;
        t.erro = None;
    } else if linha.contains(" ERR ") {
        t.erro = Some(linha.split(" ERR ").nth(1).unwrap_or(linha).trim().to_string());
    }
    t.linhas.push_back(linha.chars().take(300).collect());
    while t.linhas.len() > LINHAS_GUARDADAS {
        t.linhas.pop_front();
    }
}

/// Liga o túnel numa thread própria e mantém-no ligado (volta a lançar o cloudflared se parar), até `parar`.
pub fn iniciar(pasta_dados: PathBuf, token: String) {
    let geracao = {
        let mut t = tunel().lock().unwrap();
        if let Some(mut f) = t.filho.take() {
            let _ = f.kill();
        }
        t.geracao += 1;
        t.a_correr = true;
        t.ligado = false;
        t.erro = None;
        t.geracao
    };
    std::thread::spawn(move || {
        let mut espera = 5;
        loop {
            if tunel().lock().unwrap().geracao != geracao {
                return;
            }
            match obter_cloudflared(&pasta_dados).and_then(|exe| lancar(&exe, &token, geracao)) {
                Ok(()) => espera = 5,
                Err(e) => {
                    let mut t = tunel().lock().unwrap();
                    if t.geracao == geracao {
                        t.erro = Some(e.to_string());
                    }
                }
            }
            {
                let mut t = tunel().lock().unwrap();
                if t.geracao != geracao {
                    return;
                }
                t.ligado = false;
            }
            std::thread::sleep(Duration::from_secs(espera));
            espera = (espera * 2).min(60);
        }
    });
}

/// Corre o cloudflared até ele terminar. O token vai pelo ambiente (`TUNNEL_TOKEN`), não na linha de comandos.
fn lancar(exe: &Path, token: &str, geracao: u64) -> AppResult<()> {
    let mut c = Command::new(exe);
    c.args(["tunnel", "--no-autoupdate", "run"]).env("TUNNEL_TOKEN", token).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    super::impressao::sem_consola(&mut c);
    let mut filho = c.spawn().map_err(|e| AppError::msg(format!("Não foi possível abrir o cloudflared: {e}")))?;
    let saida = filho.stderr.take();
    {
        let mut t = tunel().lock().unwrap();
        if t.geracao != geracao {
            let _ = filho.kill();
            return Ok(());
        }
        t.filho = Some(filho);
    }
    if let Some(s) = saida {
        for linha in BufReader::new(s).lines().map_while(Result::ok) {
            anotar(geracao, &linha);
        }
    }
    let mut t = tunel().lock().unwrap();
    if t.geracao == geracao {
        if let Some(mut f) = t.filho.take() {
            let _ = f.wait();
        }
    }
    Ok(())
}

/// Desliga o túnel (o supervisor sai na volta seguinte).
pub fn parar() {
    let mut t = tunel().lock().unwrap();
    t.geracao += 1;
    t.a_correr = false;
    t.ligado = false;
    if let Some(mut f) = t.filho.take() {
        let _ = f.kill();
        let _ = f.wait();
    }
}

fn estado_tunel() -> Value {
    resumo_tunel(&tunel().lock().unwrap())
}

fn resumo_tunel(t: &Tunel) -> Value {
    json!({ "a_correr": t.a_correr, "ligado": t.ligado, "erro": t.erro, "linhas": t.linhas.iter().rev().take(12).collect::<Vec<_>>() })
}

// ============================== cloudflared ==============================

/// Nome do ficheiro na release do cloudflared para este sistema e se vem num .tgz (macOS).
fn ficheiro_release() -> Option<(&'static str, bool)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") | ("windows", "aarch64") => Some(("cloudflared-windows-amd64.exe", false)),
        ("macos", "aarch64") => Some(("cloudflared-darwin-arm64.tgz", true)),
        ("macos", "x86_64") => Some(("cloudflared-darwin-amd64.tgz", true)),
        ("linux", "x86_64") => Some(("cloudflared-linux-amd64", false)),
        ("linux", "aarch64") => Some(("cloudflared-linux-arm64", false)),
        _ => None,
    }
}

fn caminho_cloudflared(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("cloudflared").join(format!("cloudflared-{VERSAO_CLOUDFLARED}{}", std::env::consts::EXE_SUFFIX))
}

fn sha256_hex(dados: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, dados).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// O resumo que a release publica para o ficheiro (campo `digest` da API do GitHub: «sha256:…»).
fn resumo_publicado(release: &Value, nome: &str) -> Option<(String, String)> {
    let a = release["assets"].as_array()?.iter().find(|a| a["name"] == nome)?;
    let resumo = a["digest"].as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
    let url = a["browser_download_url"].as_str()?.to_string();
    (resumo.len() == 64 && url.starts_with("https://github.com/cloudflare/cloudflared/releases/download/")).then_some((resumo, url))
}

/// Devolve o cloudflared desta versão, descarregando-o e conferindo-o na primeira vez.
fn obter_cloudflared(pasta_dados: &Path) -> AppResult<PathBuf> {
    let destino = caminho_cloudflared(pasta_dados);
    if destino.is_file() {
        return Ok(destino);
    }
    let (nome, tgz) = ficheiro_release().ok_or_else(|| AppError::msg("O acesso pela Internet não está disponível neste sistema operativo."))?;
    let cab = [("Accept", "application/vnd.github+json".to_string()), ("User-Agent", format!("KanGest/{}", env!("CARGO_PKG_VERSION")))];
    let r = crate::http::pedido(
        "GET",
        &format!("https://api.github.com/repos/cloudflare/cloudflared/releases/tags/{VERSAO_CLOUDFLARED}"),
        &cab,
        None,
        Duration::from_secs(30),
    )
    .map_err(|e| AppError::msg(format!("Não foi possível obter o cloudflared (sem ligação ao GitHub?): {e}")))?;
    let release: Value = serde_json::from_slice(&r.corpo).map_err(|_| AppError::msg("Resposta inesperada do GitHub ao procurar o cloudflared."))?;
    let (resumo, url) = resumo_publicado(&release, nome).ok_or_else(|| AppError::msg(format!("A release {VERSAO_CLOUDFLARED} do cloudflared não tem o ficheiro {nome} com resumo.")))?;
    let d = crate::http::pedido("GET", &url, &cab[1..], None, Duration::from_secs(600)).map_err(|e| AppError::msg(format!("A transferência do cloudflared falhou: {e}")))?;
    if d.estado != 200 || sha256_hex(&d.corpo) != resumo {
        return Err(AppError::msg("O cloudflared descarregado não confere com o resumo publicado: não foi instalado."));
    }
    let pasta = destino.parent().unwrap();
    std::fs::create_dir_all(pasta).map_err(|e| AppError::msg(e.to_string()))?;
    let temp = pasta.join(format!("{nome}.parcial"));
    std::fs::write(&temp, &d.corpo).map_err(|e| AppError::msg(e.to_string()))?;
    let exe = if tgz {
        let ok = Command::new("tar").arg("-xzf").arg(&temp).arg("-C").arg(pasta).status().map(|s| s.success()).unwrap_or(false);
        let _ = std::fs::remove_file(&temp);
        if !ok {
            return Err(AppError::msg("Não foi possível abrir o pacote do cloudflared."));
        }
        pasta.join("cloudflared")
    } else {
        temp
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).map_err(|e| AppError::msg(e.to_string()))?;
    }
    std::fs::rename(&exe, &destino).map_err(|e| AppError::msg(e.to_string()))?;
    Ok(destino)
}

// ============================== métodos ==============================

async fn cfg(st: &AppState, chave: &str) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(&st.pool).await?.filter(|v: &String| !v.is_empty()))
}

async fn cfg_gravar(st: &AppState, chave: &str, valor: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(chave).bind(valor).execute(&st.pool).await?;
    Ok(())
}

/// Nos testes: este computador conta como servidor nesta porta, sem mexer na configuração global da rede (que os
/// testes de rede usam ao mesmo tempo).
#[cfg(test)]
pub(crate) static SERVIDOR_TESTE: Mutex<Option<u16>> = Mutex::new(None);

/// Este computador pode ter o acesso pela Internet: é o servidor da rede e entrega a aplicação ao browser.
fn pode_servir() -> Result<u16, &'static str> {
    #[cfg(test)]
    if let Some(porta) = *SERVIDOR_TESTE.lock().unwrap() {
        return Ok(porta);
    }
    let r = crate::rede::config();
    if r.modo != "servidor" {
        return Err("Ligue primeiro este computador como servidor em Configurações → Rede: é ele que fica acessível pela Internet.");
    }
    if !r.web {
        return Err("Ligue «Permitir postos pelo browser» em Configurações → Rede: o acesso pela Internet abre o KanGest no browser.");
    }
    Ok(r.porta)
}

#[derive(Deserialize, Default)]
pub struct EstadoIn {
    /// Pergunta também ao servidor de licenças (disponível, domínio): quando se abre o ecrã.
    #[serde(default)]
    verificar: bool,
}

pub async fn estado(st: &AppState, i: EstadoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    let mut v = json!({
        "servidor": pode_servir().err(),
        "nome": cfg(st, "web.nome").await?,
        "endereco": cfg(st, "web.endereco").await?,
        "activo": crate::segredos::ler(&st.pool, "web.token").await.ok().flatten().is_some_and(|t| !t.is_empty()),
        "proprio": proprio(st).await?,
        "porta": crate::rede::config().porta,
        "tunel": estado_tunel(),
        "cloudflared": VERSAO_CLOUDFLARED,
    });
    if i.verificar {
        v["remoto"] = match super::licenca::credenciais(st).await {
            Err(e) => json!({ "erro": e.to_string() }),
            Ok(c) => super::licenca::chamar_servidor("/v1/web/estado", c).await.unwrap_or_else(|e| json!({ "erro": e.to_string() })),
        };
    }
    Ok(v)
}

/// O acesso usa um túnel da própria empresa (domínio dela), e não um endereço do serviço do KanGest.
async fn proprio(st: &AppState) -> AppResult<bool> {
    Ok(cfg(st, "web.proprio").await?.as_deref() == Some("1"))
}

/// Token de um túnel da Cloudflare, sozinho ou dentro do comando de instalação que o painel mostra
/// (`cloudflared service install eyJ…`): base64 de `{"a": conta, "t": túnel, "s": segredo}`.
fn token_do_tunel(texto: &str) -> Option<String> {
    use base64::Engine;
    texto.split_whitespace().rev().find_map(|p| {
        let p = p.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
        if !p.starts_with("eyJ") {
            return None;
        }
        let sem = p.trim_end_matches('=');
        let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(sem)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(sem))
            .ok()?;
        let v: Value = serde_json::from_slice(&bytes).ok()?;
        ["a", "t", "s"].iter().all(|k| v[*k].as_str().is_some_and(|x| !x.is_empty())).then(|| p.to_string())
    })
}

/// `gestao.empresa.ao`, `https://gestao.empresa.ao/` ou com caminho → `https://gestao.empresa.ao`.
fn endereco_proprio(texto: &str) -> Option<String> {
    let t = texto.trim().to_lowercase();
    let t = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")).unwrap_or(&t);
    let host = t.split(['/', '?', '#']).next()?.trim_end_matches('.');
    let partes: Vec<&str> = host.split('.').collect();
    let valida = |l: &&str| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (host.len() <= 253 && partes.len() >= 2 && partes.iter().all(valida) && partes.last()?.chars().all(|c| c.is_ascii_alphabetic())).then(|| format!("https://{host}"))
}

#[derive(Deserialize)]
pub struct ProprioIn {
    endereco: String,
    token: String,
}

/// Domínio da empresa: o túnel criado por ela no painel da Cloudflare (Zero Trust → Networks → Tunnels), com o
/// endereço a apontar para `http://localhost:<porta>`. O KanGest só o corre e mantém ligado.
pub async fn activar_proprio(st: &AppState, i: ProprioIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    pode_servir().map_err(AppError::msg)?;
    let endereco = endereco_proprio(&i.endereco).ok_or_else(|| AppError::msg("Escreva o endereço completo, por exemplo gestao.empresa.ao."))?;
    let token = token_do_tunel(&i.token).ok_or_else(|| {
        AppError::msg("O token do túnel não é válido. No painel da Cloudflare (Zero Trust → Networks → Tunnels), copie o comando de instalação ou só o token (começa por «eyJ»).")
    })?;
    if !proprio(st).await? && crate::segredos::ler(&st.pool, "web.token").await.ok().flatten().is_some_and(|t| !t.is_empty()) {
        return Err(AppError::msg("Esta empresa já tem um endereço do KanGest activo. Desligue-o primeiro."));
    }
    crate::segredos::gravar(&st.pool, "web.token", &token).await?;
    cfg_gravar(st, "web.nome", endereco.trim_start_matches("https://")).await?;
    cfg_gravar(st, "web.endereco", &endereco).await?;
    cfg_gravar(st, "web.proprio", "1").await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'ACESSO_WEB', ?)")
        .bind(s.id)
        .bind(format!("Acesso pela Internet activado com o domínio da empresa: {endereco}"))
        .execute(&st.pool)
        .await?;
    iniciar(st.pasta_dados.clone(), token);
    estado(st, EstadoIn::default()).await
}

#[derive(Deserialize)]
pub struct ActivarIn {
    nome: String,
}

/// Cria (ou muda) o endereço e liga o túnel.
pub async fn activar(st: &AppState, i: ActivarIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    let porta = pode_servir().map_err(AppError::msg)?;
    if proprio(st).await? {
        return Err(AppError::msg("Esta empresa usa o seu próprio domínio. Desligue-o primeiro para usar um endereço do KanGest."));
    }
    let mut corpo = super::licenca::credenciais(st).await?;
    corpo["nome"] = json!(i.nome.trim().to_lowercase());
    corpo["porta"] = json!(porta);
    let r = super::licenca::chamar_servidor("/v1/web/activar", corpo).await?;
    let (Some(nome), Some(endereco), Some(token)) = (r["nome"].as_str(), r["endereco"].as_str(), r["token"].as_str()) else {
        return Err(AppError::msg("O servidor de licenças deu uma resposta incompleta. Tente de novo."));
    };
    crate::segredos::gravar(&st.pool, "web.token", token).await?;
    cfg_gravar(st, "web.nome", nome).await?;
    cfg_gravar(st, "web.endereco", endereco).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'ACESSO_WEB', ?)")
        .bind(s.id)
        .bind(format!("Acesso pela Internet activado: {endereco}"))
        .execute(&st.pool)
        .await?;
    iniciar(st.pasta_dados.clone(), token.to_string());
    estado(st, EstadoIn::default()).await
}

/// Desliga o túnel e apaga o endereço.
pub async fn desactivar(st: &AppState) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.configuracoes")?;
    parar();
    let endereco = cfg(st, "web.endereco").await?.unwrap_or_default();
    // endereço do KanGest: deixa de existir na Cloudflare (sem Internet, desliga-se aqui e avisa); domínio da
    // empresa: o túnel é dela, só se esquece aqui
    let aviso = if proprio(st).await? {
        None
    } else {
        match super::licenca::credenciais(st).await {
            Ok(c) => super::licenca::chamar_servidor("/v1/web/desactivar", c).await.err().map(|e| e.to_string()),
            Err(e) => Some(e.to_string()),
        }
    };
    if aviso.is_none() {
        for k in ["web.token", "web.nome", "web.endereco", "web.proprio"] {
            sqlx::query("DELETE FROM config WHERE chave = ?").bind(k).execute(&st.pool).await?;
        }
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'ACESSO_WEB', ?)")
        .bind(s.id)
        .bind(format!("Acesso pela Internet desligado: {endereco}"))
        .execute(&st.pool)
        .await?;
    let mut v = estado(st, EstadoIn::default()).await?;
    if let Some(a) = aviso {
        v["aviso"] = json!(format!("O túnel foi desligado neste computador, mas o endereço não foi apagado no servidor ({a}). Tente de novo com Internet."));
    }
    Ok(v)
}

/// Ao arrancar: volta a ligar o túnel, se o acesso estiver activado e este computador for o servidor.
pub fn arrancar(st: Arc<AppState>) {
    if pode_servir().is_err() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Ok(Some(token)) = crate::segredos::ler(&st.pool, "web.token").await {
            if !token.is_empty() {
                iniciar(st.pasta_dados.clone(), token);
            }
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn resumo_da_release() {
        let r = json!({ "assets": [
            { "name": "cloudflared-linux-amd64", "digest": "sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
              "browser_download_url": "https://github.com/cloudflare/cloudflared/releases/download/2026.10.7/cloudflared-linux-amd64" },
            { "name": "cloudflared-darwin-arm64.tgz", "digest": "sha256:curto", "browser_download_url": "https://github.com/cloudflare/cloudflared/releases/download/x" },
            { "name": "cloudflared-windows-amd64.exe", "digest": "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
              "browser_download_url": "https://outro-sitio.example/cloudflared.exe" }
        ]});
        let (h, url) = resumo_publicado(&r, "cloudflared-linux-amd64").unwrap();
        assert_eq!(h, "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789");
        assert!(url.ends_with("/cloudflared-linux-amd64"));
        assert!(resumo_publicado(&r, "cloudflared-darwin-arm64.tgz").is_none(), "resumo inválido");
        assert!(resumo_publicado(&r, "cloudflared-windows-amd64.exe").is_none(), "só da release oficial");
        assert!(resumo_publicado(&r, "nao-existe").is_none());
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn token_e_endereco_do_dominio_da_empresa() {
        use base64::Engine;
        let t = base64::engine::general_purpose::STANDARD.encode(r#"{"a":"conta","t":"6ff42ae2-765d-4adf-8112-31c55c1551ef","s":"c2VncmVkbw=="}"#);
        assert_eq!(token_do_tunel(&t).as_deref(), Some(t.as_str()));
        assert_eq!(token_do_tunel(&format!("sudo cloudflared service install {t}")).as_deref(), Some(t.as_str()), "do comando de instalação");
        assert_eq!(token_do_tunel(&format!("cloudflared.exe service install \"{t}\"\n")).as_deref(), Some(t.as_str()), "Windows, com aspas");
        let sem_segredo = base64::engine::general_purpose::STANDARD.encode(r#"{"a":"conta","t":"x"}"#);
        assert!(token_do_tunel(&sem_segredo).is_none());
        assert!(token_do_tunel("eyJ-isto-nao-e-base64").is_none());
        assert!(token_do_tunel("").is_none());

        assert_eq!(endereco_proprio(" Gestao.LivrariaAdventista.ao ").as_deref(), Some("https://gestao.livrariaadventista.ao"));
        assert_eq!(endereco_proprio("https://gestao.empresa.ao/painel?x=1").as_deref(), Some("https://gestao.empresa.ao"));
        for mau in ["gestao", "-a.empresa.ao", "a b.ao", "gestao.empresa.123", "http://", ""] {
            assert!(endereco_proprio(mau).is_none(), "{mau}");
        }
    }

    #[test]
    fn estado_do_tunel_pelas_linhas_do_cloudflared() {
        let mut t = Tunel::default();
        aplicar_linha(&mut t, "2026-11-17T10:00:00Z ERR Failed to dial a quic connection error=\"timeout\"");
        assert_eq!(resumo_tunel(&t)["ligado"], false);
        assert!(resumo_tunel(&t)["erro"].as_str().unwrap().starts_with("Failed to dial"));
        aplicar_linha(&mut t, "2026-11-17T10:00:05Z INF Registered tunnel connection connIndex=0 location=lis01");
        assert_eq!((resumo_tunel(&t)["ligado"].as_bool(), resumo_tunel(&t)["erro"].is_null()), (Some(true), true));
        for i in 0..100 {
            aplicar_linha(&mut t, &format!("linha {i}"));
        }
        assert_eq!(t.linhas.len(), LINHAS_GUARDADAS, "só as últimas linhas");
        assert_eq!(resumo_tunel(&t)["linhas"][0], "linha 99", "a mais recente primeiro");
    }
}

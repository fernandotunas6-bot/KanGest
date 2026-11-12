//! Utilizadores, perfis de acesso e autenticação.
//!
//! - Senhas e PIN guardados com argon2.
//! - Bloqueio temporário da conta após N tentativas falhadas (política em `config`, chaves `seguranca.*`).
//! - Senha provisória (utilizadores importados ou senha redefinida): obrigatório mudá-la no primeiro acesso;
//!   até lá a sessão só serve para mudar a senha ou sair (ver `services::dispatch`).
//! - Sessão bloqueada (inactividade ou pedido do operador): retoma-se com a senha ou o PIN do próprio.
//! - Registo de acessos na tabela `acessos`; as operações continuam na `auditoria`.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::{permissoes_efectivas, permissoes_perfil, AppState, Sessao, PERMISSOES};

pub fn hash_senha(senha: &str) -> AppResult<String> {
    let sal = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(senha.as_bytes(), &sal)
        .map(|h| h.to_string())
        .map_err(|e| AppError::msg(e.to_string()))
}

fn verificar(senha: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(senha.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

// ============================== política de segurança ==============================

#[derive(Debug, Clone, serde::Serialize)]
pub struct Politica {
    /// comprimento mínimo das senhas novas
    pub senha_min: usize,
    /// tentativas falhadas seguidas até bloquear a conta (0 = nunca bloqueia)
    pub tentativas: i64,
    /// minutos de bloqueio da conta
    pub bloqueio_min: i64,
    /// minutos sem actividade até bloquear a sessão (0 = nunca)
    pub inatividade_min: i64,
    /// permitir entrar só com o PIN (terminais POS)
    pub login_pin: bool,
}

pub async fn politica(st: &AppState) -> AppResult<Politica> {
    let rows = sqlx::query("SELECT chave, valor FROM config WHERE chave LIKE 'seguranca.%'").fetch_all(&st.pool).await?;
    let v = |k: &str| rows.iter().find(|r| r.get::<String, _>("chave") == k).map(|r| r.get::<String, _>("valor")).unwrap_or_default();
    let n = |k: &str, d: i64| v(k).trim().parse::<i64>().unwrap_or(d);
    Ok(Politica {
        senha_min: n("seguranca.senha_min", 6).clamp(4, 32) as usize,
        tentativas: n("seguranca.tentativas", 5).clamp(0, 20),
        bloqueio_min: n("seguranca.bloqueio_min", 15).clamp(1, 1440),
        inatividade_min: n("seguranca.inatividade_min", 0).clamp(0, 480),
        login_pin: v("seguranca.login_pin") == "1",
    })
}

/// Política visível antes de entrar (o ecrã de entrada precisa de saber se há entrada por PIN).
pub async fn politica_publica(st: &AppState) -> AppResult<Value> {
    let p = politica(st).await?;
    Ok(json!({ "login_pin": p.login_pin, "senha_min": p.senha_min, "inatividade_min": p.inatividade_min }))
}

pub async fn seguranca_obter(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    Ok(serde_json::to_value(politica(st).await?)?)
}

#[derive(Deserialize)]
pub struct PoliticaIn {
    senha_min: i64,
    tentativas: i64,
    bloqueio_min: i64,
    inatividade_min: i64,
    login_pin: bool,
}

pub async fn seguranca_guardar(st: &AppState, p: PoliticaIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.utilizadores")?;
    let valores = [
        ("seguranca.senha_min", p.senha_min.clamp(4, 32).to_string()),
        ("seguranca.tentativas", p.tentativas.clamp(0, 20).to_string()),
        ("seguranca.bloqueio_min", p.bloqueio_min.clamp(1, 1440).to_string()),
        ("seguranca.inatividade_min", p.inatividade_min.clamp(0, 480).to_string()),
        ("seguranca.login_pin", if p.login_pin { "1" } else { "0" }.to_string()),
    ];
    let mut tx = st.pool.begin().await?;
    for (k, v) in valores {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(k).bind(v).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'SEGURANCA', 'Política de segurança alterada')").bind(s.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(serde_json::to_value(politica(st).await?)?)
}

// ============================== registo de acessos ==============================

async fn registar(st: &AppState, utilizador_id: Option<i64>, login: &str, evento: &str, detalhe: Option<String>) {
    let _ = sqlx::query("INSERT INTO acessos (utilizador_id, login, evento, posto_id, detalhe) VALUES (?, ?, ?, ?, ?)")
        .bind(utilizador_id)
        .bind(login)
        .bind(evento)
        .bind(st.posto().id)
        .bind(detalhe)
        .execute(&st.pool)
        .await;
}

#[derive(Deserialize)]
pub struct RegistoIn {
    de: String,
    ate: String,
    #[serde(default)]
    utilizador_id: Option<i64>,
    #[serde(default)]
    evento: Option<String>,
}

pub async fn acessos_listar(st: &AppState, f: RegistoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    let rows = sqlx::query(
        "SELECT a.id, a.data_hora, a.evento, a.detalhe, COALESCE(u.nome, a.login) AS utilizador, a.login, p.descricao AS posto
         FROM acessos a LEFT JOIN utilizadores u ON u.id = a.utilizador_id LEFT JOIN postos p ON p.id = a.posto_id
         WHERE substr(a.data_hora, 1, 10) BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR a.utilizador_id = ?3) AND (?4 IS NULL OR a.evento = ?4)
         ORDER BY a.id DESC LIMIT 2000",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .bind(f.utilizador_id)
    .bind(f.evento.filter(|e| !e.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

/// Operações registadas (emissões, anulações, importações, alterações de configuração…).
pub async fn auditoria_listar(st: &AppState, f: RegistoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    let rows = sqlx::query(
        "SELECT a.id, a.data_hora, a.acao, a.detalhe, u.nome AS utilizador
         FROM auditoria a LEFT JOIN utilizadores u ON u.id = a.utilizador_id
         WHERE substr(replace(a.data_hora, 'T', ' '), 1, 10) BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR a.utilizador_id = ?3) AND (?4 IS NULL OR a.acao = ?4)
         ORDER BY a.id DESC LIMIT 2000",
    )
    .bind(&f.de)
    .bind(&f.ate)
    .bind(f.utilizador_id)
    .bind(f.evento.filter(|e| !e.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

// ============================== autenticação ==============================

async fn sessao_de(st: &AppState, row: &sqlx::sqlite::SqliteRow) -> Sessao {
    let perfil: String = row.get("perfil");
    let proprias: Option<String> = row.get("permissoes");
    let (nivel, permissoes) = permissoes_efectivas(&st.pool, &perfil, proprias.as_deref()).await;
    let perfil_descricao = sqlx::query_scalar("SELECT descricao FROM perfis WHERE codigo = ?").bind(&perfil).fetch_optional(&st.pool).await.ok().flatten();
    Sessao {
        id: row.get("id"),
        nome: row.get("nome"),
        login: row.get("login"),
        permissoes,
        limite_desconto: row.get("limite_desconto"),
        perfil,
        nivel,
        mudar_senha: row.get::<i64, _>("mudar_senha") == 1,
        bloqueada: false,
        perfil_descricao,
    }
}

/// "2026-11-12T10:15:00" → "10:15"
fn hora(s: &str) -> String {
    s.get(11..16).unwrap_or(s).to_string()
}

/// Mais um falhanço de um utilizador conhecido: conta e, no limite, bloqueia a conta.
/// Devolve a mensagem de erro para o ecrã.
async fn falhou(st: &AppState, pol: &Politica, id: i64, login: &str, tentativas: i64, motivo: &str) -> AppResult<AppError> {
    let n = tentativas + 1;
    if pol.tentativas > 0 && n >= pol.tentativas {
        let ate = (chrono::Local::now() + chrono::Duration::minutes(pol.bloqueio_min)).format("%Y-%m-%dT%H:%M:%S").to_string();
        sqlx::query("UPDATE utilizadores SET tentativas = 0, bloqueado_ate = ? WHERE id = ?").bind(&ate).bind(id).execute(&st.pool).await?;
        registar(st, Some(id), login, "FALHA", Some(motivo.into())).await;
        registar(st, Some(id), login, "BLOQUEIO", Some(format!("{n} tentativas falhadas; bloqueada até às {}", hora(&ate)))).await;
        return Ok(AppError::msg(format!(
            "Conta bloqueada por {n} tentativas falhadas. Tente de novo às {} ou peça ao administrador para a desbloquear.",
            hora(&ate)
        )));
    }
    sqlx::query("UPDATE utilizadores SET tentativas = ? WHERE id = ?").bind(n).bind(id).execute(&st.pool).await?;
    registar(st, Some(id), login, "FALHA", Some(motivo.into())).await;
    let restam = pol.tentativas - n;
    Ok(AppError::msg(if pol.tentativas > 0 && restam <= 2 {
        format!("Utilizador ou senha incorrectos. Resta(m) {restam} tentativa(s) antes de a conta ser bloqueada.")
    } else {
        "Utilizador ou senha incorrectos.".into()
    }))
}

/// Conta bloqueada por tentativas falhadas? (o bloqueio termina sozinho)
fn bloqueada_ate(row: &sqlx::sqlite::SqliteRow) -> Option<String> {
    row.get::<Option<String>, _>("bloqueado_ate").filter(|ate| *ate > db::agora())
}

async fn entrar(st: &AppState, row: &sqlx::sqlite::SqliteRow, como: &str) -> AppResult<Value> {
    let id: i64 = row.get("id");
    let login: String = row.get("login");
    sqlx::query("UPDATE utilizadores SET tentativas = 0, bloqueado_ate = NULL, ultimo_acesso = ? WHERE id = ?")
        .bind(db::agora())
        .bind(id)
        .execute(&st.pool)
        .await?;
    let s = sessao_de(st, row).await;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao) VALUES (?, 'LOGIN')").bind(id).execute(&st.pool).await?;
    registar(st, Some(id), &login, "LOGIN", Some(como.into())).await;
    st.definir_sessao(Some(s.clone()));
    Ok(serde_json::to_value(s)?)
}

#[derive(Deserialize)]
pub struct LoginIn {
    login: String,
    senha: String,
}

pub async fn login(st: &AppState, i: LoginIn) -> AppResult<Value> {
    let pol = politica(st).await?;
    let login = i.login.trim();
    let Some(row) = sqlx::query("SELECT * FROM utilizadores WHERE login = ?").bind(login).fetch_optional(&st.pool).await? else {
        registar(st, None, login, "FALHA", Some("utilizador inexistente".into())).await;
        return Err(AppError::msg("Utilizador ou senha incorrectos."));
    };
    let id: i64 = row.get("id");
    if let Some(ate) = bloqueada_ate(&row) {
        registar(st, Some(id), login, "FALHA", Some("conta bloqueada".into())).await;
        return Err(AppError::msg(format!("Conta bloqueada por tentativas falhadas até às {}. Peça ao administrador para a desbloquear.", hora(&ate))));
    }
    if !verificar(&i.senha, &row.get::<String, _>("senha_hash")) {
        return Err(falhou(st, &pol, id, login, row.get("tentativas"), "senha errada").await?);
    }
    if row.get::<i64, _>("ativo") != 1 {
        registar(st, Some(id), login, "FALHA", Some("conta desactivada".into())).await;
        return Err(AppError::msg("Esta conta está desactivada. Fale com o administrador."));
    }
    entrar(st, &row, "senha").await
}

/// Falhas seguidas de entrada por PIN em cada posto (um PIN de 4 dígitos não pode ser adivinhado à força).
static FALHAS_PIN: std::sync::Mutex<Option<std::collections::HashMap<i64, (u32, chrono::DateTime<chrono::Local>)>>> = std::sync::Mutex::new(None);

#[derive(Deserialize)]
pub struct PinIn {
    pin: String,
}

/// Entrada só com o PIN (terminais POS), se a política o permitir.
pub async fn login_pin(st: &AppState, i: PinIn) -> AppResult<Value> {
    let pol = politica(st).await?;
    if !pol.login_pin {
        return Err(AppError::msg("A entrada por PIN não está activada."));
    }
    let posto = st.posto().id;
    {
        let mut g = FALHAS_PIN.lock().unwrap();
        let mapa = g.get_or_insert_with(Default::default);
        if let Some((n, quando)) = mapa.get(&posto) {
            if *n >= 5 && chrono::Local::now() - *quando < chrono::Duration::minutes(5) {
                return Err(AppError::msg("Demasiadas tentativas com PIN neste posto. Aguarde 5 minutos ou entre com a senha."));
            }
        }
    }
    let pin = i.pin.trim();
    let rows = sqlx::query("SELECT * FROM utilizadores WHERE ativo = 1 AND pin_hash IS NOT NULL").fetch_all(&st.pool).await?;
    let encontrado = rows.into_iter().find(|r| verificar(pin, &r.get::<String, _>("pin_hash")));
    let Some(row) = encontrado else {
        {
            let mut g = FALHAS_PIN.lock().unwrap();
            let e = g.get_or_insert_with(Default::default).entry(posto).or_insert((0, chrono::Local::now()));
            e.0 += 1;
            e.1 = chrono::Local::now();
        }
        registar(st, None, "", "FALHA", Some("PIN errado".into())).await;
        return Err(AppError::msg("PIN incorrecto."));
    };
    if let Some(ate) = bloqueada_ate(&row) {
        return Err(AppError::msg(format!("Conta bloqueada por tentativas falhadas até às {}.", hora(&ate))));
    }
    if let Some(m) = FALHAS_PIN.lock().unwrap().as_mut() {
        m.remove(&posto);
    }
    entrar(st, &row, "PIN").await
}

pub async fn logout(st: &AppState) -> AppResult<Value> {
    if let Some(s) = st.sessao_opcional() {
        registar(st, Some(s.id), &s.login, "LOGOUT", None).await;
    }
    st.definir_sessao(None);
    Ok(Value::Null)
}

/// Bloqueia a sessão (inactividade ou o operador sai por momentos): o ecrã fica tapado e o servidor
/// recusa os pedidos até se retomar.
pub async fn bloquear(st: &AppState) -> AppResult<Value> {
    let mut s = st.sessao()?;
    if !s.bloqueada {
        s.bloqueada = true;
        registar(st, Some(s.id), &s.login, "SESSAO_BLOQUEADA", None).await;
        st.definir_sessao(Some(s.clone()));
    }
    Ok(serde_json::to_value(s)?)
}

#[derive(Deserialize)]
pub struct RetomarIn {
    #[serde(default)]
    senha: Option<String>,
    #[serde(default)]
    pin: Option<String>,
}

/// Retoma a sessão bloqueada com a senha ou o PIN do próprio utilizador.
pub async fn desbloquear(st: &AppState, i: RetomarIn) -> AppResult<Value> {
    let mut s = st.sessao()?;
    let pol = politica(st).await?;
    let row = sqlx::query("SELECT * FROM utilizadores WHERE id = ?").bind(s.id).fetch_one(&st.pool).await?;
    let ok = match (i.senha.as_deref().filter(|x| !x.is_empty()), i.pin.as_deref().map(str::trim).filter(|x| !x.is_empty())) {
        (Some(senha), _) => verificar(senha, &row.get::<String, _>("senha_hash")),
        (None, Some(pin)) => row.get::<Option<String>, _>("pin_hash").is_some_and(|h| verificar(pin, &h)),
        _ => false,
    };
    if !ok {
        let erro = falhou(st, &pol, s.id, &s.login, row.get("tentativas"), "retomar sessão").await?;
        // conta bloqueada: a sessão também termina
        if sqlx::query_scalar::<_, Option<String>>("SELECT bloqueado_ate FROM utilizadores WHERE id = ?").bind(s.id).fetch_one(&st.pool).await?.is_some_and(|a| a > db::agora()) {
            st.definir_sessao(None);
        }
        return Err(erro);
    }
    sqlx::query("UPDATE utilizadores SET tentativas = 0 WHERE id = ?").bind(s.id).execute(&st.pool).await?;
    s.bloqueada = false;
    registar(st, Some(s.id), &s.login, "SESSAO_RETOMADA", None).await;
    st.definir_sessao(Some(s.clone()));
    Ok(serde_json::to_value(s)?)
}

#[derive(Deserialize)]
pub struct SenhaIn {
    atual: String,
    nova: String,
}

/// Valida uma senha nova contra a política.
fn validar_senha(pol: &Politica, senha: &str) -> AppResult<()> {
    if senha.chars().count() < pol.senha_min {
        return Err(AppError::msg(format!("A senha deve ter pelo menos {} caracteres.", pol.senha_min)));
    }
    if senha.trim().is_empty() || senha.chars().all(|c| c == senha.chars().next().unwrap_or(' ')) {
        return Err(AppError::msg("Escolha uma senha menos previsível."));
    }
    Ok(())
}

pub async fn alterar_senha(st: &AppState, i: SenhaIn) -> AppResult<Value> {
    let mut s = st.sessao()?;
    let pol = politica(st).await?;
    let hash: String = sqlx::query_scalar("SELECT senha_hash FROM utilizadores WHERE id = ?").bind(s.id).fetch_one(&st.pool).await?;
    if !verificar(&i.atual, &hash) {
        return Err(AppError::msg("A senha actual está incorrecta."));
    }
    validar_senha(&pol, &i.nova)?;
    if i.nova == i.atual {
        return Err(AppError::msg("A nova senha tem de ser diferente da actual."));
    }
    sqlx::query("UPDATE utilizadores SET senha_hash = ?, mudar_senha = 0, senha_alterada_em = ? WHERE id = ?")
        .bind(hash_senha(&i.nova)?)
        .bind(db::agora())
        .bind(s.id)
        .execute(&st.pool)
        .await?;
    registar(st, Some(s.id), &s.login, "SENHA", Some("alterada pelo próprio".into())).await;
    s.mudar_senha = false;
    st.definir_sessao(Some(s.clone()));
    Ok(serde_json::to_value(s)?)
}

// ============================== utilizadores ==============================

pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    let rows = sqlx::query("SELECT u.*, p.descricao AS perfil_descricao FROM utilizadores u LEFT JOIN perfis p ON p.codigo = u.perfil ORDER BY u.nome")
        .fetch_all(&st.pool)
        .await?;
    let agora = db::agora();
    let mut lista = Vec::with_capacity(rows.len());
    for r in &rows {
        let perfil: String = r.get("perfil");
        let proprias: Option<String> = r.get("permissoes");
        let (_, permissoes) = permissoes_efectivas(&st.pool, &perfil, proprias.as_deref()).await;
        let bloqueado: Option<String> = r.get::<Option<String>, _>("bloqueado_ate").filter(|a| *a > agora);
        lista.push(json!({
            "id": r.get::<i64, _>("id"),
            "nome": r.get::<String, _>("nome"),
            "login": r.get::<String, _>("login"),
            "ativo": r.get::<i64, _>("ativo"),
            "criado_em": r.get::<String, _>("criado_em"),
            "permissoes_proprias": proprias.is_some(),
            "permissoes": permissoes,
            "limite_desconto": r.get::<f64, _>("limite_desconto"),
            "tem_pin": r.get::<Option<String>, _>("pin_hash").is_some(),
            "perfil": perfil,
            "perfil_descricao": r.get::<Option<String>, _>("perfil_descricao"),
            "mudar_senha": r.get::<i64, _>("mudar_senha") == 1,
            "ultimo_acesso": r.get::<Option<String>, _>("ultimo_acesso"),
            "bloqueado_ate": bloqueado,
            "tentativas": r.get::<i64, _>("tentativas"),
            "importado": r.get::<Option<String>, _>("id_origem").is_some(),
        }));
    }
    Ok(Value::Array(lista))
}

#[derive(Deserialize)]
pub struct UtilizadorIn {
    id: Option<i64>,
    nome: String,
    login: String,
    perfil: String,
    ativo: Option<bool>,
    senha: Option<String>,
    /// PIN de autorização (4–8 dígitos). Vazio = manter; "-" = remover.
    pin: Option<String>,
    /// Lista de permissões; `null` = usar as do perfil.
    permissoes: Option<Vec<String>>,
    limite_desconto: Option<f64>,
    /// Obrigar a mudar a senha no próximo acesso (por omissão nos utilizadores novos).
    mudar_senha: Option<bool>,
}

/// O PIN identifica a pessoa (entrada por PIN e autorizações de supervisor): não pode repetir-se.
async fn pin_em_uso(st: &AppState, pin: &str, excepto: Option<i64>) -> AppResult<bool> {
    let rows = sqlx::query("SELECT id, pin_hash FROM utilizadores WHERE pin_hash IS NOT NULL AND ativo = 1").fetch_all(&st.pool).await?;
    Ok(rows.iter().any(|r| Some(r.get::<i64, _>("id")) != excepto && verificar(pin, &r.get::<String, _>("pin_hash"))))
}

pub async fn guardar(st: &AppState, u: UtilizadorIn) -> AppResult<Value> {
    let quem = st.exigir_perm("sistema.utilizadores")?;
    let pol = politica(st).await?;
    let existe: Option<i64> = sqlx::query_scalar("SELECT nivel FROM perfis WHERE codigo = ?").bind(&u.perfil).fetch_optional(&st.pool).await?;
    if existe.is_none() {
        return Err(AppError::msg("Perfil inválido."));
    }
    if u.nome.trim().is_empty() || u.login.trim().is_empty() {
        return Err(AppError::msg("Nome e utilizador são obrigatórios."));
    }
    // o último administrador activo não pode perder o acesso
    if let Some(id) = u.id {
        let era_admin: bool = sqlx::query_scalar::<_, String>("SELECT perfil FROM utilizadores WHERE id = ?").bind(id).fetch_optional(&st.pool).await?.as_deref() == Some("ADMIN");
        let fica_admin = u.perfil == "ADMIN" && u.ativo.unwrap_or(true);
        if era_admin && !fica_admin {
            let outros: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilizadores WHERE perfil = 'ADMIN' AND ativo = 1 AND id <> ?").bind(id).fetch_one(&st.pool).await?;
            if outros == 0 {
                return Err(AppError::msg("Este é o único administrador activo: crie ou active outro antes de lhe retirar o acesso."));
            }
        }
    }
    let perms = match &u.permissoes {
        Some(p) => {
            if let Some(x) = p.iter().find(|x| !PERMISSOES.contains(&x.as_str())) {
                return Err(AppError::msg(format!("Permissão desconhecida: {x}")));
            }
            Some(serde_json::to_string(p)?)
        }
        None => None,
    };
    let pin = u.pin.as_deref().map(str::trim).unwrap_or("");
    if !pin.is_empty() && pin != "-" {
        if pin.len() < 4 || pin.len() > 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::msg("O PIN deve ter entre 4 e 8 dígitos."));
        }
        if pin_em_uso(st, pin, u.id).await? {
            return Err(AppError::msg("Este PIN já é usado por outro utilizador. Escolha outro."));
        }
    }
    let ativo = u.ativo.unwrap_or(true) as i64;
    let limite = u.limite_desconto.unwrap_or(0.0).clamp(0.0, 100.0);
    let mut tx = st.pool.begin().await?;
    let id = match u.id {
        Some(id) => {
            sqlx::query("UPDATE utilizadores SET nome = ?, login = ?, perfil = ?, ativo = ?, permissoes = ?, limite_desconto = ? WHERE id = ?")
                .bind(u.nome.trim())
                .bind(u.login.trim())
                .bind(&u.perfil)
                .bind(ativo)
                .bind(&perms)
                .bind(limite)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| if e.to_string().contains("UNIQUE") { AppError::msg("Já existe um utilizador com este nome de acesso.") } else { e.into() })?;
            if let Some(s) = u.senha.as_deref().filter(|s| !s.is_empty()) {
                validar_senha(&pol, s)?;
                sqlx::query("UPDATE utilizadores SET senha_hash = ?, senha_alterada_em = ?, mudar_senha = ? WHERE id = ?")
                    .bind(hash_senha(s)?)
                    .bind(db::agora())
                    .bind(u.mudar_senha.unwrap_or(true) as i64)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            } else if let Some(m) = u.mudar_senha {
                sqlx::query("UPDATE utilizadores SET mudar_senha = ? WHERE id = ?").bind(m as i64).bind(id).execute(&mut *tx).await?;
            }
            id
        }
        None => {
            let senha = u.senha.as_deref().unwrap_or("");
            validar_senha(&pol, senha)?;
            sqlx::query(
                "INSERT INTO utilizadores (nome, login, senha_hash, perfil, ativo, permissoes, limite_desconto, mudar_senha, senha_alterada_em)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(u.nome.trim())
            .bind(u.login.trim())
            .bind(hash_senha(senha)?)
            .bind(&u.perfil)
            .bind(ativo)
            .bind(&perms)
            .bind(limite)
            .bind(u.mudar_senha.unwrap_or(true) as i64)
            .bind(db::agora())
            .execute(&mut *tx)
            .await
            .map_err(|e| if e.to_string().contains("UNIQUE") { AppError::msg("Já existe um utilizador com este nome de acesso.") } else { e.into() })?
            .last_insert_rowid()
        }
    };
    match pin {
        "-" => {
            sqlx::query("UPDATE utilizadores SET pin_hash = NULL WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
        p if !p.is_empty() => {
            sqlx::query("UPDATE utilizadores SET pin_hash = ? WHERE id = ?").bind(hash_senha(p)?).bind(id).execute(&mut *tx).await?;
        }
        _ => {}
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'UTILIZADOR', ?)")
        .bind(quem.id)
        .bind(format!("{} {} ({})", if u.id.is_some() { "Alterado" } else { "Criado" }, u.login.trim(), u.perfil))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdUtilizadorIn {
    id: i64,
    #[serde(default)]
    senha: Option<String>,
}

/// O administrador põe uma senha provisória (ou gera uma): o utilizador tem de a mudar ao entrar.
pub async fn redefinir_senha(st: &AppState, i: IdUtilizadorIn) -> AppResult<Value> {
    let quem = st.exigir_perm("sistema.utilizadores")?;
    let pol = politica(st).await?;
    let senha = match i.senha.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            validar_senha(&pol, s)?;
            s.to_string()
        }
        None => {
            const ALFABETO: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
            let mut r = rand::thread_rng();
            (0..pol.senha_min.max(8)).map(|_| ALFABETO[r.gen_range(0..ALFABETO.len())] as char).collect()
        }
    };
    let login: Option<String> = sqlx::query_scalar("SELECT login FROM utilizadores WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    let login = login.ok_or_else(|| AppError::msg("Utilizador não encontrado."))?;
    sqlx::query("UPDATE utilizadores SET senha_hash = ?, mudar_senha = 1, tentativas = 0, bloqueado_ate = NULL, senha_alterada_em = ? WHERE id = ?")
        .bind(hash_senha(&senha)?)
        .bind(db::agora())
        .bind(i.id)
        .execute(&st.pool)
        .await?;
    registar(st, Some(i.id), &login, "SENHA_REDEFINIDA", Some(format!("por {}", quem.nome))).await;
    Ok(json!({ "senha": senha, "login": login }))
}

/// Levanta o bloqueio por tentativas falhadas.
pub async fn desbloquear_conta(st: &AppState, i: IdUtilizadorIn) -> AppResult<Value> {
    let quem = st.exigir_perm("sistema.utilizadores")?;
    let login: Option<String> = sqlx::query_scalar("SELECT login FROM utilizadores WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?;
    let login = login.ok_or_else(|| AppError::msg("Utilizador não encontrado."))?;
    sqlx::query("UPDATE utilizadores SET tentativas = 0, bloqueado_ate = NULL WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    registar(st, Some(i.id), &login, "DESBLOQUEIO", Some(format!("por {}", quem.nome))).await;
    Ok(Value::Null)
}

// ============================== perfis de acesso ==============================

pub async fn perfis_listar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    let rows = sqlx::query(
        "SELECT p.*, (SELECT COUNT(*) FROM utilizadores u WHERE u.perfil = p.codigo) AS utilizadores
         FROM perfis p ORDER BY p.sistema DESC, p.nivel DESC, p.descricao",
    )
    .fetch_all(&st.pool)
    .await?;
    let mut lista = Vec::with_capacity(rows.len());
    for r in &rows {
        let codigo: String = r.get("codigo");
        let (_, permissoes) = permissoes_efectivas(&st.pool, &codigo, None).await;
        lista.push(json!({
            "codigo": codigo,
            "descricao": r.get::<String, _>("descricao"),
            "nivel": r.get::<i64, _>("nivel"),
            "sistema": r.get::<i64, _>("sistema") == 1,
            "personalizado": r.get::<Option<String>, _>("permissoes").is_some(),
            "permissoes": permissoes,
            "utilizadores": r.get::<i64, _>("utilizadores"),
        }));
    }
    Ok(Value::Array(lista))
}

#[derive(Deserialize)]
pub struct PerfilIn {
    /// vazio = perfil novo (o código nasce da descrição)
    #[serde(default)]
    codigo: Option<String>,
    descricao: String,
    nivel: i64,
    /// `null` num perfil de origem = repor as permissões de origem
    permissoes: Option<Vec<String>>,
}

pub async fn perfis_guardar(st: &AppState, p: PerfilIn) -> AppResult<Value> {
    let quem = st.exigir_perm("sistema.utilizadores")?;
    let descricao = p.descricao.trim();
    if descricao.is_empty() {
        return Err(AppError::msg("Indique o nome do perfil."));
    }
    let nivel = p.nivel.clamp(1, 2); // o escalão de administrador é só do perfil Administrador
    if let Some(x) = p.permissoes.iter().flatten().find(|x| !PERMISSOES.contains(&x.as_str())) {
        return Err(AppError::msg(format!("Permissão desconhecida: {x}")));
    }
    let lista = p.permissoes.as_ref().map(serde_json::to_string).transpose()?;
    let codigo = match p.codigo.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some("ADMIN") => return Err(AppError::msg("O perfil Administrador tem sempre todas as permissões.")),
        Some(c) => {
            let sistema: Option<i64> = sqlx::query_scalar("SELECT sistema FROM perfis WHERE codigo = ?").bind(c).fetch_optional(&st.pool).await?;
            let sistema = sistema.ok_or_else(|| AppError::msg("Perfil não encontrado."))?;
            // perfis de origem: muda-se a lista, não o nome nem o escalão
            if sistema == 1 {
                sqlx::query("UPDATE perfis SET permissoes = ? WHERE codigo = ?").bind(&lista).bind(c).execute(&st.pool).await?;
            } else {
                sqlx::query("UPDATE perfis SET descricao = ?, nivel = ?, permissoes = ? WHERE codigo = ?")
                    .bind(descricao)
                    .bind(nivel)
                    .bind(lista.clone().unwrap_or_else(|| "[]".into()))
                    .bind(c)
                    .execute(&st.pool)
                    .await?;
            }
            c.to_string()
        }
        None => {
            let base: String = descricao
                .to_uppercase()
                .chars()
                .map(|c| match c {
                    'Á' | 'À' | 'Â' | 'Ã' => 'A',
                    'É' | 'Ê' => 'E',
                    'Í' => 'I',
                    'Ó' | 'Ô' | 'Õ' => 'O',
                    'Ú' => 'U',
                    'Ç' => 'C',
                    c if c.is_ascii_alphanumeric() => c,
                    _ => '_',
                })
                .collect::<String>()
                .trim_matches('_')
                .chars()
                .take(20)
                .collect();
            let base = if base.is_empty() { "PERFIL".to_string() } else { base };
            let mut codigo = base.clone();
            let mut n = 2;
            while sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM perfis WHERE codigo = ?").bind(&codigo).fetch_one(&st.pool).await? > 0 {
                codigo = format!("{base}_{n}");
                n += 1;
            }
            sqlx::query("INSERT INTO perfis (codigo, descricao, nivel, permissoes, sistema) VALUES (?, ?, ?, ?, 0)")
                .bind(&codigo)
                .bind(descricao)
                .bind(nivel)
                .bind(lista.clone().unwrap_or_else(|| serde_json::to_string(&permissoes_perfil("CAIXA")).unwrap_or_default()))
                .execute(&st.pool)
                .await?;
            codigo
        }
    };
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'PERFIL', ?)")
        .bind(quem.id)
        .bind(format!("Perfil {codigo} guardado"))
        .execute(&st.pool)
        .await?;
    Ok(json!({ "codigo": codigo }))
}

#[derive(Deserialize)]
pub struct CodigoIn {
    codigo: String,
}

pub async fn perfis_eliminar(st: &AppState, c: CodigoIn) -> AppResult<Value> {
    let quem = st.exigir_perm("sistema.utilizadores")?;
    let sistema: Option<i64> = sqlx::query_scalar("SELECT sistema FROM perfis WHERE codigo = ?").bind(&c.codigo).fetch_optional(&st.pool).await?;
    match sistema {
        None => return Err(AppError::msg("Perfil não encontrado.")),
        Some(1) => return Err(AppError::msg("Os perfis de origem não se eliminam.")),
        _ => {}
    }
    let em_uso: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilizadores WHERE perfil = ?").bind(&c.codigo).fetch_one(&st.pool).await?;
    if em_uso > 0 {
        return Err(AppError::msg(format!("{em_uso} utilizador(es) usam este perfil: mude-lhes o perfil antes de o eliminar.")));
    }
    sqlx::query("DELETE FROM perfis WHERE codigo = ?").bind(&c.codigo).execute(&st.pool).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'PERFIL', ?)")
        .bind(quem.id)
        .bind(format!("Perfil {} eliminado", c.codigo))
        .execute(&st.pool)
        .await?;
    Ok(Value::Null)
}

// ============================== autorização por PIN (supervisor) ==============================

/// Supervisor identificado por PIN que tem todas as permissões pedidas e limite de desconto suficiente.
pub struct Supervisor {
    pub id: i64,
    pub nome: String,
    pub limite_desconto: f64,
}

pub async fn supervisor_por_pin(st: &AppState, pin: &str, perms: &[&str], desconto: f64) -> AppResult<Option<Supervisor>> {
    let pin = pin.trim();
    if pin.is_empty() {
        return Ok(None);
    }
    let rows = sqlx::query("SELECT id, nome, perfil, permissoes, limite_desconto, pin_hash FROM utilizadores WHERE ativo = 1 AND pin_hash IS NOT NULL")
        .fetch_all(&st.pool)
        .await?;
    for r in rows {
        let hash: String = r.get("pin_hash");
        if !verificar(pin, &hash) {
            continue;
        }
        let perfil: String = r.get("perfil");
        let json: Option<String> = r.get("permissoes");
        let (_, lista) = permissoes_efectivas(&st.pool, &perfil, json.as_deref()).await;
        let limite: f64 = r.get("limite_desconto");
        let tem_todas = perms.iter().all(|p| lista.iter().any(|x| x == p));
        let limite_ok = limite <= 0.0 || desconto <= limite + 1e-9;
        if tem_todas && limite_ok {
            return Ok(Some(Supervisor { id: r.get("id"), nome: r.get("nome"), limite_desconto: limite }));
        }
        return Err(AppError::msg("O utilizador deste PIN não tem permissão (ou limite de desconto) para esta operação."));
    }
    Ok(None)
}

#[derive(Deserialize)]
pub struct AutorizarIn {
    pin: String,
    permissao: String,
    #[serde(default)]
    desconto: f64,
}

/// Validação imediata do PIN no POS (ecrã "Autenticação" do KIGEST).
pub async fn autorizar(st: &AppState, a: AutorizarIn) -> AppResult<Value> {
    st.sessao()?;
    match supervisor_por_pin(st, &a.pin, &[a.permissao.as_str()], a.desconto).await? {
        Some(s) => Ok(json!({ "id": s.id, "nome": s.nome, "limite_desconto": s.limite_desconto })),
        None => Err(AppError::msg("PIN inválido.")),
    }
}

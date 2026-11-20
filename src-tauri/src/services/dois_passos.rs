// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Verificação em dois passos (TOTP, RFC 6238), como nos programas na nuvem: depois da senha, um código de 6
//! algarismos que muda de 30 em 30 segundos numa aplicação de autenticação (Google Authenticator, Microsoft
//! Authenticator, Authy…), ou um dos códigos de recuperação (cada um serve uma vez).
//!
//! - Cada utilizador liga-a para si (com a senha e um código da aplicação); o administrador pode repô-la a quem
//!   perdeu o telemóvel.
//! - Política «exigir a quem entra pela Internet»: pelo acesso por subdomínio, quem não a tem não entra.
//! - O segredo fica cifrado (`segredos`); um código não serve duas vezes; um código errado conta como tentativa
//!   falhada da senha (bloqueio da conta).

use ring::hmac;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use super::auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const PERIODO: i64 = 30;
const CODIGOS_RECUPERACAO: usize = 8;
const ALFABETO_B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

// ============================== TOTP ==============================

pub fn base32(dados: &[u8]) -> String {
    let mut s = String::new();
    for bloco in dados.chunks(5) {
        let mut b = [0u8; 5];
        b[..bloco.len()].copy_from_slice(bloco);
        let n = u64::from_be_bytes([0, 0, 0, b[0], b[1], b[2], b[3], b[4]]);
        let letras = (bloco.len() * 8).div_ceil(5);
        for i in 0..letras {
            s.push(ALFABETO_B32[((n >> (35 - 5 * i)) & 31) as usize] as char);
        }
    }
    s
}

pub fn de_base32(s: &str) -> Option<Vec<u8>> {
    let mut bits: u64 = 0;
    let mut n = 0;
    let mut out = Vec::new();
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=' && *c != '-') {
        let v = ALFABETO_B32.iter().position(|&x| x as char == c.to_ascii_uppercase())? as u64;
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
            bits &= (1 << n) - 1;
        }
    }
    Some(out)
}

/// Código de 6 algarismos de um período (RFC 6238 com HMAC-SHA1, como as aplicações de autenticação).
pub fn codigo(segredo: &[u8], passo: i64) -> String {
    let chave = hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, segredo);
    let m = hmac::sign(&chave, &(passo as u64).to_be_bytes());
    let h = m.as_ref();
    let o = (h[19] & 0x0f) as usize;
    let n = u32::from_be_bytes([h[o] & 0x7f, h[o + 1], h[o + 2], h[o + 3]]);
    format!("{:06}", n % 1_000_000)
}

fn passo_agora() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(PERIODO)
}

/// Período aceite para o código (o actual, o anterior ou o seguinte: relógios ligeiramente desacertados), nunca um já
/// usado (`ultimo`).
pub fn conferir(segredo: &[u8], codigo_dado: &str, ultimo: Option<i64>, agora: i64) -> Option<i64> {
    let c: String = codigo_dado.chars().filter(char::is_ascii_digit).collect();
    if c.len() != 6 {
        return None;
    }
    (agora - 1..=agora + 1).find(|&p| ultimo.is_none_or(|u| p > u) && codigo(segredo, p) == c)
}

fn resumo_recuperacao(c: &str) -> String {
    let limpo: String = c.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
    ring::digest::digest(&ring::digest::SHA256, format!("kangest-recuperacao:{limpo}").as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn novos_codigos() -> Vec<String> {
    use rand::Rng;
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut r = rand::thread_rng();
    (0..CODIGOS_RECUPERACAO)
        .map(|_| {
            let mut s: String = (0..8).map(|_| A[r.gen_range(0..A.len())] as char).collect();
            s.insert(4, '-');
            s
        })
        .collect()
}

fn segredo_do_utilizador(row: &sqlx::sqlite::SqliteRow) -> AppResult<Option<Vec<u8>>> {
    let Some(cifrado) = row.try_get::<Option<String>, _>("totp_segredo").ok().flatten().filter(|s| !s.is_empty()) else { return Ok(None) };
    let texto = crate::segredos::decifrar(&cifrado).map_err(|e| {
        AppError::msg(format!("A verificação em dois passos desta conta não abre neste computador ({e}). Peça ao administrador para a repor."))
    })?;
    Ok(Some(de_base32(&texto).ok_or_else(|| AppError::msg("Segredo de dois passos inválido."))?))
}

// ============================== no login ==============================

pub enum NoLogin {
    Seguir,
    /// A senha está certa e falta o código: o ecrã pede-o.
    PedirCodigo,
    Errado,
}

/// O que a entrada precisa depois da senha certa. `codigo` é o código da aplicação ou um de recuperação.
pub async fn no_login(st: &AppState, row: &sqlx::sqlite::SqliteRow, codigo_dado: Option<&str>) -> AppResult<NoLogin> {
    if segredo_do_utilizador(row)?.is_none() {
        if crate::state::pela_internet() && auth::politica(st).await?.dois_passos_internet {
            return Err(AppError::msg(
                "Para entrar pela Internet, esta conta precisa da verificação em dois passos. Ligue-a no KanGest da loja (menu do utilizador → Verificação em dois passos).",
            ));
        }
        return Ok(NoLogin::Seguir);
    }
    let Some(dado) = codigo_dado.map(str::trim).filter(|c| !c.is_empty()) else { return Ok(NoLogin::PedirCodigo) };
    Ok(match conferir_conta(st, row, dado, "Entrada").await? {
        Some(true) => NoLogin::Seguir,
        _ => NoLogin::Errado,
    })
}

/// Confere o código da aplicação de autenticação ou um código de recuperação (que se gasta) da conta `row`.
/// `None`: a conta não tem a verificação em dois passos. `para`: o que fica na auditoria quando se usa um código de
/// recuperação («Entrada», «Recuperação da senha»).
pub async fn conferir_conta(st: &AppState, row: &sqlx::sqlite::SqliteRow, dado: &str, para: &str) -> AppResult<Option<bool>> {
    let id: i64 = row.get("id");
    let Some(segredo) = segredo_do_utilizador(row)? else { return Ok(None) };
    let dado = dado.trim();
    if dado.is_empty() {
        return Ok(Some(false));
    }
    let ultimo: Option<i64> = row.try_get("totp_ultimo").ok().flatten();
    if let Some(p) = conferir(&segredo, dado, ultimo, passo_agora()) {
        sqlx::query("UPDATE utilizadores SET totp_ultimo = ? WHERE id = ?").bind(p).bind(id).execute(&st.pool).await?;
        return Ok(Some(true));
    }
    // código de recuperação: serve uma vez
    let mut lista: Vec<String> = row
        .try_get::<Option<String>, _>("totp_recuperacao")
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    let r = resumo_recuperacao(dado);
    if let Some(i) = lista.iter().position(|h| *h == r) {
        lista.remove(i);
        sqlx::query("UPDATE utilizadores SET totp_recuperacao = ? WHERE id = ?").bind(serde_json::to_string(&lista)?).bind(id).execute(&st.pool).await?;
        sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'DOIS_PASSOS', ?)")
            .bind(id)
            .bind(format!("{para} com um código de recuperação (restam {})", lista.len()))
            .execute(&st.pool)
            .await?;
        return Ok(Some(true));
    }
    Ok(Some(false))
}

// ============================== métodos ==============================

async fn utilizador(st: &AppState, id: i64) -> AppResult<sqlx::sqlite::SqliteRow> {
    Ok(sqlx::query("SELECT * FROM utilizadores WHERE id = ?").bind(id).fetch_one(&st.pool).await?)
}

async fn auditar(st: &AppState, quem: i64, detalhe: String) -> AppResult<()> {
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'DOIS_PASSOS', ?)").bind(quem).bind(detalhe).execute(&st.pool).await?;
    Ok(())
}

/// Estado para o utilizador com sessão.
pub async fn estado(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    let row = utilizador(st, s.id).await?;
    let activo = row.try_get::<Option<String>, _>("totp_segredo").ok().flatten().is_some_and(|x| !x.is_empty());
    let restantes = row
        .try_get::<Option<String>, _>("totp_recuperacao")
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .map_or(0, |v| v.len());
    Ok(json!({ "activo": activo, "codigos_restantes": restantes, "exigido_internet": auth::politica(st).await?.dois_passos_internet }))
}

/// Segredo novo para mostrar no código QR (só se guarda ao confirmar com um código da aplicação).
pub async fn novo(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    let bytes: [u8; 20] = rand::random();
    let segredo = base32(&bytes);
    let rotulo: String = s.login.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c.to_string() } else { format!("%{:02X}", c as u32 & 0xff) }).collect();
    let uri = format!("otpauth://totp/KanGest:{rotulo}?secret={segredo}&issuer=KanGest&algorithm=SHA1&digits=6&period=30");
    Ok(json!({ "segredo": segredo, "uri": uri }))
}

#[derive(Deserialize)]
pub struct LigarIn {
    segredo: String,
    codigo: String,
    senha: String,
}

/// Liga os dois passos: a senha da conta e um código da aplicação com o segredo novo. Devolve os códigos de
/// recuperação (só agora; na base ficam os resumos).
pub async fn ligar(st: &AppState, i: LigarIn) -> AppResult<Value> {
    let s = st.sessao()?;
    let row = utilizador(st, s.id).await?;
    if !auth::senha_confere(&i.senha, &row.get::<String, _>("senha_hash")) {
        return Err(AppError::msg("A senha não está certa."));
    }
    let bytes = de_base32(&i.segredo).filter(|b| b.len() >= 16).ok_or_else(|| AppError::msg("Segredo inválido: comece de novo."))?;
    let Some(p) = conferir(&bytes, &i.codigo, None, passo_agora()) else {
        return Err(AppError::msg("O código não confere. Confirme que a hora do telemóvel está certa e escreva o código que a aplicação mostra agora."));
    };
    let codigos = novos_codigos();
    let resumos: Vec<String> = codigos.iter().map(|c| resumo_recuperacao(c)).collect();
    sqlx::query("UPDATE utilizadores SET totp_segredo = ?, totp_ultimo = ?, totp_recuperacao = ? WHERE id = ?")
        .bind(crate::segredos::cifrar(&base32(&bytes)))
        .bind(p)
        .bind(serde_json::to_string(&resumos)?)
        .bind(s.id)
        .execute(&st.pool)
        .await?;
    auditar(st, s.id, format!("Verificação em dois passos ligada para {}", s.login)).await?;
    Ok(json!({ "codigos": codigos }))
}

#[derive(Deserialize)]
pub struct DesligarIn {
    senha: String,
    codigo: String,
}

/// Desliga os dois passos da própria conta: a senha e um código (da aplicação ou de recuperação).
pub async fn desligar(st: &AppState, i: DesligarIn) -> AppResult<Value> {
    let s = st.sessao()?;
    let row = utilizador(st, s.id).await?;
    if !auth::senha_confere(&i.senha, &row.get::<String, _>("senha_hash")) {
        return Err(AppError::msg("A senha não está certa."));
    }
    if !matches!(no_login(st, &row, Some(&i.codigo)).await?, NoLogin::Seguir) {
        return Err(AppError::msg("O código não confere."));
    }
    sqlx::query("UPDATE utilizadores SET totp_segredo = NULL, totp_ultimo = NULL, totp_recuperacao = NULL WHERE id = ?").bind(s.id).execute(&st.pool).await?;
    auditar(st, s.id, format!("Verificação em dois passos desligada por {}", s.login)).await?;
    estado(st).await
}

#[derive(Deserialize)]
pub struct ReporIn {
    id: i64,
}

/// O administrador desliga os dois passos de outro utilizador (perdeu o telemóvel e os códigos de recuperação).
pub async fn repor(st: &AppState, i: ReporIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.utilizadores")?;
    let login: String = sqlx::query_scalar("SELECT login FROM utilizadores WHERE id = ?").bind(i.id).fetch_optional(&st.pool).await?.ok_or(AppError::NaoEncontrado)?;
    sqlx::query("UPDATE utilizadores SET totp_segredo = NULL, totp_ultimo = NULL, totp_recuperacao = NULL WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    auditar(st, s.id, format!("Verificação em dois passos de {login} reposta pelo administrador")).await?;
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn totp_da_rfc_6238() {
        // vectores da RFC 6238 (SHA-1, segredo «12345678901234567890»), nos 6 últimos algarismos
        let s = b"12345678901234567890";
        for (t, esperado) in [(59, "287082"), (1111111109, "081804"), (1111111111, "050471"), (1234567890, "005924"), (2000000000, "279037")] {
            assert_eq!(codigo(s, t / 30), esperado, "t = {t}");
        }
        assert_eq!(base32(s), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(de_base32("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").unwrap(), s.to_vec());
        assert!(de_base32("1!").is_none());
    }

    #[test]
    fn janela_e_sem_repeticao() {
        let s = b"segredo-de-teste-123";
        let agora = 1_000_000;
        assert_eq!(conferir(s, &codigo(s, agora), None, agora), Some(agora));
        assert_eq!(conferir(s, &codigo(s, agora - 1), None, agora), Some(agora - 1), "relógio um pouco atrás");
        assert_eq!(conferir(s, &codigo(s, agora + 1), None, agora), Some(agora + 1));
        assert_eq!(conferir(s, &codigo(s, agora - 2), None, agora), None, "código velho");
        assert_eq!(conferir(s, &codigo(s, agora), Some(agora), agora), None, "o mesmo código não serve duas vezes");
        assert_eq!(conferir(s, " 123 ", None, agora), None);
        assert_eq!(resumo_recuperacao("ABCD-efgh"), resumo_recuperacao("abcdefgh"));
    }
}

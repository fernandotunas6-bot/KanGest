//! Segredos guardados na configuração (tabela `config`): chave do contribuinte da Facturação
//! Electrónica, senhas e, nas compilações de desenvolvimento, a chave de assinatura.
//!
//! Ficam cifrados com AES-256-GCM. A chave mestra está num ficheiro à parte (`kangest.chave`, na
//! pasta de dados da aplicação), fora da base de dados: uma cópia da base, sozinha, não revela os
//! segredos. Restaurar a base noutra máquina obriga a introduzir de novo esses segredos.

use std::path::Path;
use std::sync::OnceLock;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

const PREFIXO: &str = "kgs1:";
const AAD: &[u8] = b"kangest-config";
pub const FICHEIRO: &str = "kangest.chave";

/// Chaves da tabela `config` que guardam segredos.
pub const CHAVES: [&str; 5] = ["agt.chave_privada", "fe.senha", "fe.chave_software", "fe.chave_contribuinte", "email.senha"];

static CHAVE: OnceLock<[u8; 32]> = OnceLock::new();

fn aleatorio<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    SystemRandom::new().fill(&mut b).expect("gerador aleatório do sistema");
    b
}

/// Lê (ou cria, na primeira vez) a chave mestra da pasta de dados.
pub fn iniciar(pasta: &Path) -> AppResult<()> {
    let f = pasta.join(FICHEIRO);
    let chave: [u8; 32] = match std::fs::read(&f) {
        Ok(b) => b.try_into().map_err(|_| AppError::msg(format!("O ficheiro {} está danificado.", f.display())))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let k = aleatorio::<32>();
            std::fs::write(&f, k).map_err(|e| AppError::msg(format!("Não foi possível criar {}: {e}", f.display())))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
            }
            k
        }
        Err(e) => return Err(AppError::msg(format!("Não foi possível ler {}: {e}", f.display()))),
    };
    let _ = CHAVE.set(chave);
    Ok(())
}

/// Sem `iniciar` (testes), usa uma chave só desta execução.
fn chave() -> LessSafeKey {
    let k = CHAVE.get_or_init(aleatorio::<32>);
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, k).expect("chave AES-256"))
}

pub fn cifrar(texto: &str) -> String {
    let nonce = aleatorio::<NONCE_LEN>();
    let mut dados = texto.as_bytes().to_vec();
    chave()
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(AAD), &mut dados)
        .expect("cifrar");
    let mut tudo = nonce.to_vec();
    tudo.extend_from_slice(&dados);
    format!("{PREFIXO}{}", B64.encode(tudo))
}

pub fn cifrado(valor: &str) -> bool {
    valor.starts_with(PREFIXO)
}

/// Decifra um valor guardado; um valor antigo (em texto) passa tal como está.
pub fn decifrar(valor: &str) -> Result<String, String> {
    let Some(b64) = valor.strip_prefix(PREFIXO) else { return Ok(valor.to_string()) };
    let tudo = B64.decode(b64.trim()).map_err(|_| "valor cifrado ilegível".to_string())?;
    if tudo.len() < NONCE_LEN + 16 {
        return Err("valor cifrado incompleto".into());
    }
    let (n, c) = tudo.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(n).map_err(|_| "valor cifrado ilegível".to_string())?;
    let mut dados = c.to_vec();
    let texto = chave()
        .open_in_place(nonce, Aad::from(AAD), &mut dados)
        .map_err(|_| "foi cifrado noutra instalação (outra chave mestra)".to_string())?;
    String::from_utf8(texto.to_vec()).map_err(|_| "valor cifrado ilegível".to_string())
}

/// Lê um segredo da configuração.
pub async fn ler<'e, E: sqlx::SqliteExecutor<'e>>(e: E, chave: &str) -> AppResult<Option<String>> {
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(e).await?;
    match v {
        None => Ok(None),
        Some(v) => decifrar(&v).map(Some).map_err(|motivo| {
            AppError::msg(format!("O segredo «{chave}» não pode ser lido nesta máquina ({motivo}). Volte a introduzi-lo nas configurações."))
        }),
    }
}

/// Grava um segredo (cifrado) na configuração.
pub async fn gravar<'e, E: sqlx::SqliteExecutor<'e>>(e: E, chave: &str, valor: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(chave)
        .bind(cifrar(valor))
        .execute(e)
        .await?;
    Ok(())
}

/// Cifra os segredos que versões anteriores gravaram em texto.
pub async fn cifrar_existentes(pool: &SqlitePool) -> AppResult<usize> {
    let mut n = 0;
    for k in CHAVES {
        let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(pool).await?;
        if let Some(v) = v.filter(|v| !v.is_empty() && !cifrado(v)) {
            gravar(pool, k, &v).await?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod testes {
    #[test]
    fn ida_e_volta() {
        let c = super::cifrar("-----BEGIN PRIVATE KEY----- segredo");
        assert!(super::cifrado(&c) && !c.contains("segredo"));
        assert_eq!(super::decifrar(&c).unwrap(), "-----BEGIN PRIVATE KEY----- segredo");
        assert_ne!(super::cifrar("x"), super::cifrar("x"), "nonce diferente em cada vez");
        assert_eq!(super::decifrar("texto antigo").unwrap(), "texto antigo");
        let mut estragado = c.clone();
        estragado.replace_range(c.len() - 4.., "AAAA");
        assert!(super::decifrar(&estragado).is_err());
    }
}

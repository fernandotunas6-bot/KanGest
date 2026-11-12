//! Assinatura de documentos (AGT — Decreto Presidencial 312/18, SAF-T AO).
//!
//! Cadeia de hash: RSA-SHA1 (PKCS#1 v1.5) em Base64 sobre
//! `DataEmissao;DataHoraSistema;NumeroDocumento;TotalBruto;HashAnterior`
//! — o mesmo formato que o KIGEST usa (ver Dados.Txt: "2019-07-29;2019-07-29T13:05:47;VD 1/4;81.23;...").
//!
//! IMPORTANTE: para uso legal, a chave privada pertence ao produtor do software e a chave
//! pública tem de ser entregue à AGT no processo de certificação. A chave gerada aqui
//! serve para desenvolvimento até haver certificado.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use sha1::Sha1;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

pub const CHAVE_CONFIG: &str = "agt.chave_privada";
pub const CERTIFICADO_CONFIG: &str = "agt.numero_certificado";

pub async fn chave(pool: &SqlitePool) -> AppResult<RsaPrivateKey> {
    let pem: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?")
        .bind(CHAVE_CONFIG)
        .fetch_optional(pool)
        .await?;
    if let Some(pem) = pem {
        return RsaPrivateKey::from_pkcs8_pem(&pem).map_err(|e| AppError::msg(format!("Chave AGT inválida: {e}")));
    }
    let chave = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut rand::thread_rng(), 1024))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
        .map_err(|e| AppError::msg(format!("Falha ao gerar chave RSA: {e}")))?;
    let pem = chave
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AppError::msg(e.to_string()))?
        .to_string();
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?)")
        .bind(CHAVE_CONFIG)
        .bind(pem)
        .execute(pool)
        .await?;
    Ok(chave)
}

pub fn chave_publica_pem(chave: &RsaPrivateKey) -> AppResult<String> {
    chave
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Total com 2 casas decimais e ponto (ex.: 81.23), a partir de cêntimos.
pub fn total_fmt(centimos: i64) -> String {
    let sinal = if centimos < 0 { "-" } else { "" };
    let c = centimos.abs();
    format!("{sinal}{}.{:02}", c / 100, c % 100)
}

pub fn mensagem(data: &str, data_hora: &str, numero_doc: &str, total: i64, hash_anterior: &str) -> String {
    format!("{data};{data_hora};{numero_doc};{};{hash_anterior}", total_fmt(total))
}

pub fn assinar(chave: &RsaPrivateKey, msg: &str) -> String {
    let sk = SigningKey::<Sha1>::new(chave.clone());
    B64.encode(sk.sign(msg.as_bytes()).to_bytes())
}

/// Os 4 caracteres impressos no documento: posições 1, 11, 21 e 31 do hash.
pub fn controlo(hash: &str) -> String {
    let c: Vec<char> = hash.chars().collect();
    [0usize, 10, 20, 30].iter().filter_map(|&i| c.get(i)).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formato_mensagem() {
        assert_eq!(
            mensagem("2019-07-29", "2019-07-29T13:05:47", "VD 1/4", 8123, ""),
            "2019-07-29;2019-07-29T13:05:47;VD 1/4;81.23;"
        );
        assert_eq!(total_fmt(5), "0.05");
        assert_eq!(total_fmt(-150), "-1.50");
    }

    #[test]
    fn assinatura_verificavel() {
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::signature::Verifier;
        let k = RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
        let msg = mensagem("2026-11-09", "2026-11-09T10:00:00", "FR 2026/1", 11400, "");
        let h = assinar(&k, &msg);
        assert_eq!(h.len(), 172); // 128 bytes em base64
        let vk = VerifyingKey::<Sha1>::new(k.to_public_key());
        let sig = Signature::try_from(B64.decode(&h).unwrap().as_slice()).unwrap();
        assert!(vk.verify(msg.as_bytes(), &sig).is_ok());
        assert_eq!(controlo(&h).len(), 4);
    }
}

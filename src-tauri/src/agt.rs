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
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha1::Sha1;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::{AppError, AppResult};

pub const CHAVE_CONFIG: &str = "agt.chave_privada";
pub const CERTIFICADO_CONFIG: &str = "agt.numero_certificado";

/// Chave que assina os documentos.
///
/// - Compilação oficial: a chave do produtor vem embutida no programa (`produtor::chave_agt`) e nunca
///   é gravada na base de dados. Se a base ainda tiver uma chave (de desenvolvimento ou carregada à
///   mão em versões anteriores), fica só a pública arquivada em `agt_chaves` e a privada é apagada.
/// - Compilação de desenvolvimento: usa a chave da base de dados ou gera uma de desenvolvimento.
pub async fn chave(pool: &SqlitePool) -> AppResult<RsaPrivateKey> {
    if let Some(k) = crate::produtor::chave_agt().map_err(AppError::msg)? {
        retirar_chave_da_base(pool, k).await?;
        // a pública fica registada: se uma versão futura trouxer outra chave, estes documentos continuam verificáveis
        sqlx::query("INSERT OR IGNORE INTO agt_chaves (chave_publica, origem) VALUES (?, 'PRODUTOR')")
            .bind(chave_publica_pem(k)?)
            .execute(pool)
            .await?;
        return Ok(k.clone());
    }
    // compilação de desenvolvimento: chave guardada (cifrada) na configuração
    match crate::segredos::ler(pool, CHAVE_CONFIG).await {
        Ok(Some(pem)) => return RsaPrivateKey::from_pkcs8_pem(&pem).map_err(|e| AppError::msg(format!("Chave AGT inválida: {e}"))),
        Ok(None) => {}
        Err(_) => {
            // base restaurada noutro computador: a chave cifrada não se lê aqui. A pública já está em
            // agt_chaves (os documentos antigos continuam verificáveis); gera-se uma chave nova.
            sqlx::query("DELETE FROM config WHERE chave = ?").bind(CHAVE_CONFIG).execute(pool).await?;
            sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (NULL, 'AGT_CHAVE', 'Chave de desenvolvimento ilegível nesta máquina (cópia restaurada): gerada uma nova')")
                .execute(pool)
                .await?;
        }
    }
    let chave = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut rand::thread_rng(), 1024))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
        .map_err(|e| AppError::msg(format!("Falha ao gerar chave RSA: {e}")))?;
    let pem = chave
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| AppError::msg(e.to_string()))?
        .to_string();
    crate::segredos::gravar(pool, CHAVE_CONFIG, &pem).await?;
    sqlx::query("INSERT OR IGNORE INTO agt_chaves (chave_publica, origem) VALUES (?, 'DESENVOLVIMENTO')")
        .bind(chave_publica_pem(&chave)?)
        .execute(pool)
        .await?;
    Ok(chave)
}

/// Com a chave do produtor embutida, a privada que esteja na base de dados sai de lá (fica a pública).
pub(crate) async fn retirar_chave_da_base(pool: &SqlitePool, produtor: &RsaPrivateKey) -> AppResult<()> {
    let bruto: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CHAVE_CONFIG).fetch_optional(pool).await?;
    let Some(bruto) = bruto else { return Ok(()) };
    let pem = crate::segredos::decifrar(&bruto).unwrap_or_default();
    let mut tx = pool.begin().await?;
    if RsaPrivateKey::from_pkcs8_pem(&pem).map(|k| k.to_public_key() != produtor.to_public_key()).unwrap_or(false) {
        let importada: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'agt.chave_importada_em'").fetch_optional(&mut *tx).await?;
        arquivar_chave(&mut tx, if importada.is_some() { "IMPORTADA" } else { "DESENVOLVIMENTO" }).await?;
    }
    sqlx::query("DELETE FROM config WHERE chave = ?").bind(CHAVE_CONFIG).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (NULL, 'AGT_CHAVE', 'Chave privada retirada da base de dados: passa a ser usada a chave do produtor incluída no programa')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// De onde vem a chave que assina: PRODUTOR (embutida), IMPORTADA (carregada à mão) ou DESENVOLVIMENTO.
pub async fn origem_chave(pool: &SqlitePool) -> AppResult<&'static str> {
    if crate::produtor::chave_agt().map_err(AppError::msg)?.is_some() {
        return Ok("PRODUTOR");
    }
    let importada: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'agt.chave_importada_em'").fetch_optional(pool).await?;
    Ok(if importada.is_some() { "IMPORTADA" } else { "DESENVOLVIMENTO" })
}

/// N.º do certificado AGT: o da compilação oficial ou o configurado no programa.
pub async fn certificado(pool: &SqlitePool) -> AppResult<String> {
    if let Some(c) = crate::produtor::certificado() {
        return Ok(c);
    }
    let c: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(CERTIFICADO_CONFIG).fetch_optional(pool).await?;
    Ok(c.unwrap_or_default().trim().to_string())
}

/// Versão da chave com que se assina agora (HashControl do SAF-T): "0" enquanto o programa não tem
/// certificado; depois, a versão da chave do produtor.
pub async fn versao_assinatura(pool: &SqlitePool) -> AppResult<String> {
    let cert = certificado(pool).await?;
    if cert.is_empty() || cert == "0" {
        return Ok("0".into());
    }
    if crate::produtor::chave_agt().map_err(AppError::msg)?.is_some() {
        return Ok(crate::produtor::versao_chave());
    }
    let v: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'agt.versao_chave'").fetch_optional(pool).await?;
    Ok(v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| "1".into()))
}

/// Compilação que exige a certificação: sem chave do produtor e n.º de certificado não se assina.
pub async fn exigir_certificacao(pool: &SqlitePool) -> AppResult<()> {
    if !crate::produtor::exigir_certificacao() {
        return Ok(());
    }
    if crate::produtor::chave_agt().map_err(AppError::msg)?.is_none() || certificado(pool).await?.is_empty() {
        return Err(AppError::msg("Esta versão do KanGest só emite documentos assinados com a chave e o certificado do produtor, que não vieram na instalação. Instale a versão oficial."));
    }
    Ok(())
}

pub fn chave_publica_pem(chave: &RsaPrivateKey) -> AppResult<String> {
    chave
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Chaves públicas com que se verificam os documentos: a da chave actual e as usadas antes
/// (tabela `agt_chaves`). Não gera chave nenhuma.
pub async fn chaves_publicas(pool: &SqlitePool) -> AppResult<Vec<RsaPublicKey>> {
    let mut v: Vec<RsaPublicKey> = Vec::new();
    if let Ok(Some(k)) = crate::produtor::chave_agt() {
        v.push(k.to_public_key());
    }
    let pem = crate::segredos::ler(pool, CHAVE_CONFIG).await.ok().flatten();
    if let Some(k) = pem.and_then(|p| RsaPrivateKey::from_pkcs8_pem(&p).ok()) {
        if !v.contains(&k.to_public_key()) {
            v.push(k.to_public_key());
        }
    }
    let antigas: Vec<String> = sqlx::query_scalar("SELECT chave_publica FROM agt_chaves ORDER BY id DESC").fetch_all(pool).await?;
    for k in antigas.iter().filter_map(|p| RsaPublicKey::from_public_key_pem(p).ok()) {
        if !v.contains(&k) {
            v.push(k);
        }
    }
    Ok(v)
}

/// A assinatura (`hash`, Base64) da mensagem confere com alguma das chaves?
pub fn confere(chaves: &[RsaPublicKey], msg: &str, hash: &str) -> bool {
    let Ok(bytes) = B64.decode(hash.trim()) else { return false };
    let Ok(sig) = Signature::try_from(bytes.as_slice()) else { return false };
    chaves.iter().any(|k| VerifyingKey::<Sha1>::new(k.clone()).verify(msg.as_bytes(), &sig).is_ok())
}

/// Guarda a chave pública da chave em uso antes de a substituir: os documentos que ela assinou
/// continuam verificáveis na verificação de integridade.
pub async fn arquivar_chave(tx: &mut Transaction<'_, Sqlite>, origem: &str) -> AppResult<()> {
    let pem = crate::segredos::ler(&mut **tx, CHAVE_CONFIG).await.ok().flatten();
    if let Some(k) = pem.and_then(|p| RsaPrivateKey::from_pkcs8_pem(&p).ok()) {
        sqlx::query("INSERT OR IGNORE INTO agt_chaves (chave_publica, origem) VALUES (?, ?)")
            .bind(chave_publica_pem(&k)?)
            .bind(origem)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
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

// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Segredos do produtor do software, embutidos na compilação oficial (ver `build.rs` e
//! docs/CERTIFICACAO-AGT.md): a chave que assina os documentos, o n.º do certificado AGT, as
//! credenciais e a chave de software da Facturação Electrónica e a chave pública das actualizações.
//! Nunca são gravados na base de dados dos clientes nem nas cópias de segurança.

use std::sync::OnceLock;

use rsa::RsaPrivateKey;

mod gerado {
    include!(concat!(env!("OUT_DIR"), "/segredos_produtor.rs"));
}

/// Tem de ser igual a `fluxo` no `build.rs`.
fn fluxo(i: usize) -> u8 {
    gerado::MASCARA[i % gerado::MASCARA.len()] ^ (i.wrapping_mul(167) as u8).rotate_left((i % 7) as u32)
}

fn revelar(dados: &[u8]) -> Option<String> {
    if dados.is_empty() {
        return None;
    }
    let v: Vec<u8> = dados.iter().enumerate().map(|(i, b)| b ^ fluxo(i)).collect();
    String::from_utf8(v).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Lê uma chave RSA em PEM (PKCS#8 ou PKCS#1).
pub fn ler_chave(pem: &str) -> Result<RsaPrivateKey, String> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    RsaPrivateKey::from_pkcs8_pem(pem.trim())
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem.trim()))
        .map_err(|e| format!("chave RSA inválida: {e}"))
}

/// Chave do produtor que assina os documentos (AGT), se esta compilação a trouxer.
/// `Err` se veio mas não é uma chave RSA válida (compilação mal configurada).
pub fn chave_agt() -> Result<Option<&'static RsaPrivateKey>, String> {
    static CHAVE: OnceLock<Result<Option<RsaPrivateKey>, String>> = OnceLock::new();
    CHAVE
        .get_or_init(|| revelar(gerado::AGT_CHAVE).map(|p| ler_chave(&p)).transpose())
        .as_ref()
        .map(|o| o.as_ref())
        .map_err(|e| format!("A chave do produtor incluída nesta versão é inválida ({e}). Contacte o produtor do KanGest."))
}

/// N.º do certificado AGT do programa, se veio na compilação.
pub fn certificado() -> Option<String> {
    revelar(gerado::AGT_CERTIFICADO)
}

/// Versão da chave do produtor (HashControl do SAF-T).
pub fn versao_chave() -> String {
    revelar(gerado::AGT_VERSAO_CHAVE).unwrap_or_else(|| "1".into())
}

pub fn fe_utilizador() -> Option<String> {
    revelar(gerado::FE_UTILIZADOR)
}

pub fn fe_senha() -> Option<String> {
    revelar(gerado::FE_SENHA)
}

pub fn fe_chave_software() -> Option<String> {
    revelar(gerado::FE_CHAVE_SOFTWARE)
}

pub fn fe_numero_validacao() -> Option<String> {
    revelar(gerado::FE_NUMERO_VALIDACAO)
}

/// Chave pública minisign com que se confirmam as actualizações.
pub fn chave_actualizacoes() -> Option<String> {
    revelar(gerado::CHAVE_ACTUALIZACOES)
}

/// Esta compilação só assina documentos com a chave e o certificado do produtor.
pub fn exigir_certificacao() -> bool {
    gerado::EXIGIR_CERTIFICACAO
}

#[cfg(test)]
mod testes {
    #[test]
    fn mascara_desfaz_o_que_o_build_fez() {
        // o build.rs aplica o mesmo fluxo: aplicado duas vezes, volta ao texto
        let texto = b"-----BEGIN PRIVATE KEY-----";
        let ida: Vec<u8> = texto.iter().enumerate().map(|(i, b)| b ^ super::fluxo(i)).collect();
        assert_ne!(ida.as_slice(), texto.as_slice());
        assert_eq!(super::revelar(&ida).as_deref(), Some("-----BEGIN PRIVATE KEY-----"));
        assert_eq!(super::revelar(&[]), None);
    }
}

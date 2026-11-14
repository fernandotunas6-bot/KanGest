//! Compilação do KanGest.
//!
//! Além do `tauri_build`, embute no programa os segredos do produtor do software, lidos de variáveis
//! de ambiente (segredos do GitHub Actions na compilação oficial). Nunca ficam na base de dados dos
//! clientes nem nas cópias de segurança. Ver docs/CERTIFICACAO-AGT.md.
//!
//! | Variável | Conteúdo |
//! |---|---|
//! | `KANGEST_AGT_CHAVE` (ou `KANGEST_AGT_CHAVE_FICHEIRO`) | chave privada RSA do produtor, PEM (assinatura dos documentos) |
//! | `KANGEST_AGT_CERTIFICADO` | n.º do certificado AGT do programa |
//! | `KANGEST_AGT_VERSAO_CHAVE` | versão da chave (HashControl do SAF-T), por omissão 1 |
//! | `KANGEST_FE_UTILIZADOR`, `KANGEST_FE_SENHA` | credenciais dos serviços da Facturação Electrónica |
//! | `KANGEST_FE_CHAVE_SOFTWARE` (ou `_FICHEIRO`) | chave RSA do produtor para a Facturação Electrónica, PEM |
//! | `KANGEST_FE_NUMERO_VALIDACAO` | n.º de validação do software na Facturação Electrónica |
//! | `KANGEST_CHAVE_ACTUALIZACOES` | chave pública minisign das actualizações |
//! | `KANGEST_EXIGIR_CERTIFICACAO=1` | recusa assinar documentos sem a chave e o certificado do produtor |
//!
//! Sem estas variáveis (compilação de desenvolvimento) o programa funciona como antes: gera uma chave
//! de desenvolvimento e aceita a configuração feita no próprio programa.
//!
//! Os valores ficam ofuscados (XOR com uma máscara gerada em cada compilação), para não aparecerem
//! numa simples procura de texto no executável. Não é cifra: quem desmontar o programa pode chegar
//! lá. É a mesma protecção que os programas certificados usam para a chave do produtor.

use std::collections::hash_map::RandomState;
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher};

const VARIAVEIS: [(&str, &str); 8] = [
    ("AGT_CHAVE", "KANGEST_AGT_CHAVE"),
    ("AGT_CERTIFICADO", "KANGEST_AGT_CERTIFICADO"),
    ("AGT_VERSAO_CHAVE", "KANGEST_AGT_VERSAO_CHAVE"),
    ("FE_UTILIZADOR", "KANGEST_FE_UTILIZADOR"),
    ("FE_SENHA", "KANGEST_FE_SENHA"),
    ("FE_CHAVE_SOFTWARE", "KANGEST_FE_CHAVE_SOFTWARE"),
    ("FE_NUMERO_VALIDACAO", "KANGEST_FE_NUMERO_VALIDACAO"),
    ("CHAVE_ACTUALIZACOES", "KANGEST_CHAVE_ACTUALIZACOES"),
];

/// Tem de ser igual a `produtor::fluxo`.
fn fluxo(mascara: &[u8], i: usize) -> u8 {
    mascara[i % mascara.len()] ^ (i.wrapping_mul(167) as u8).rotate_left((i % 7) as u32)
}

fn valor(var: &str) -> String {
    println!("cargo:rerun-if-env-changed={var}");
    let ficheiro = format!("{var}_FICHEIRO");
    println!("cargo:rerun-if-env-changed={ficheiro}");
    if let Some(caminho) = std::env::var(&ficheiro).ok().filter(|c| !c.trim().is_empty()) {
        println!("cargo:rerun-if-changed={caminho}");
        return std::fs::read_to_string(caminho.trim()).unwrap_or_else(|e| panic!("{ficheiro}: não foi possível ler {caminho}: {e}"));
    }
    std::env::var(var).unwrap_or_default()
}

fn main() {
    // máscara aleatória desta compilação (as chaves do RandomState vêm do sistema operativo)
    let estado = RandomState::new();
    let mascara: Vec<u8> = (0..64u64)
        .map(|i| {
            let mut h = estado.build_hasher();
            h.write_u64(i);
            h.finish() as u8
        })
        .collect();
    let mut src = String::from("// Gerado pelo build.rs: segredos do produtor (ofuscados).\n");
    let _ = writeln!(src, "pub const MASCARA: &[u8] = &{mascara:?};");
    for (nome, var) in VARIAVEIS {
        let v = valor(var);
        let bytes: Vec<u8> = v.trim().bytes().enumerate().map(|(i, b)| b ^ fluxo(&mascara, i)).collect();
        let _ = writeln!(src, "pub const {nome}: &[u8] = &{bytes:?};");
    }
    println!("cargo:rerun-if-env-changed=KANGEST_EXIGIR_CERTIFICACAO");
    let exigir = matches!(std::env::var("KANGEST_EXIGIR_CERTIFICACAO").unwrap_or_default().trim(), "1" | "true" | "sim");
    let _ = writeln!(src, "pub const EXIGIR_CERTIFICACAO: bool = {exigir};");
    let saida = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("segredos_produtor.rs");
    std::fs::write(&saida, src).expect("gravar segredos_produtor.rs");

    tauri_build::build()
}

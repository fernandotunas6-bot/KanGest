//! Selo de integridade do executável do KanGest (ver `src-tauri/src/selo/mod.rs` e `docs/SEGURANCA.md`).
//!
//! ```text
//! kangest-selo chaves                                            par de chaves novo
//! kangest-selo assinar <executável> <selo.json> <versão> <alvo>  sela (KANGEST_SELO_PRIVADA e KANGEST_SELO_PUBLICA)
//! kangest-selo verificar <executável> <selo.json>                confirma (KANGEST_SELO_PUBLICA)
//! kangest-selo resumo <executável>
//! ```
//!
//! O resumo é calculado pelo mesmo código que o KanGest usa ao arrancar (`formato.rs`, partilhado).

#[path = "../../../src-tauri/src/selo/formato.rs"]
mod formato;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["chaves"] => chaves(),
        ["assinar", exe, selo, versao, alvo] => assinar(exe, selo, versao, alvo),
        ["verificar", exe, selo] => verificar(exe, selo),
        ["resumo", exe] => ler(exe).and_then(|b| formato::resumo(&b)).map(|r| println!("{}", formato::hex(&r))),
        _ => Err("Uso: kangest-selo chaves | assinar <executável> <selo.json> <versão> <alvo> | verificar <executável> <selo.json> | resumo <executável>".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Selo: {e}");
            ExitCode::FAILURE
        }
    }
}

fn ler(caminho: &str) -> Result<Vec<u8>, String> {
    std::fs::read(caminho).map_err(|e| format!("não foi possível ler {caminho}: {e}"))
}

fn variavel(nome: &str) -> Result<String, String> {
    std::env::var(nome).ok().map(|v| v.split_whitespace().collect::<String>()).filter(|v| !v.is_empty()).ok_or_else(|| format!("falta a variável {nome}"))
}

/// Base64 normal ou para URLs, com ou sem `=`.
fn base64(texto: &str) -> Result<Vec<u8>, String> {
    let t = texto.trim_end_matches('=').replace('-', "+").replace('_', "/");
    let t = format!("{t}{}", "=".repeat((4 - t.len() % 4) % 4));
    STANDARD.decode(t).map_err(|_| "base64 inválido".to_string())
}

fn chaves() -> Result<(), String> {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "sem números aleatórios do sistema")?;
    let par = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "chave gerada inválida")?;
    println!("Guarde os dois valores como segredos do repositório (GitHub → Settings → Secrets and variables → Actions).");
    println!("A chave privada não se mostra a ninguém nem se guarda no código.\n");
    println!("KANGEST_SELO_PRIVADA={}", STANDARD.encode(pkcs8.as_ref()));
    println!("KANGEST_SELO_PUBLICA={}", URL_SAFE_NO_PAD.encode(par.public_key().as_ref()));
    Ok(())
}

fn assinar(exe: &str, saida: &str, versao: &str, alvo: &str) -> Result<(), String> {
    let pkcs8 = base64(&variavel("KANGEST_SELO_PRIVADA")?)?;
    let par = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&pkcs8).map_err(|_| "KANGEST_SELO_PRIVADA não é uma chave Ed25519 em PKCS#8")?;
    // a chave pública vai compilada no executável: se não for a desta chave privada, nenhuma instalação abriria o selo
    let publica = base64(&variavel("KANGEST_SELO_PUBLICA")?)?;
    if publica != par.public_key().as_ref() {
        return Err("KANGEST_SELO_PUBLICA não corresponde a KANGEST_SELO_PRIVADA".into());
    }
    let resumo = formato::hex(&formato::resumo(&ler(exe)?)?);
    let assinatura = par.sign(formato::mensagem(versao, alvo, &resumo).as_bytes());
    let selo = serde_json::json!({
        "v": 1, "versao": versao, "alvo": alvo, "resumo": resumo,
        "assinatura": URL_SAFE_NO_PAD.encode(assinatura.as_ref()),
    });
    std::fs::write(saida, serde_json::to_string_pretty(&selo).unwrap_or_default() + "\n").map_err(|e| format!("não foi possível gravar {saida}: {e}"))?;
    println!("Selo: {exe} ({versao}, {alvo}) → {resumo}");
    Ok(())
}

fn verificar(exe: &str, selo: &str) -> Result<(), String> {
    let publica = base64(&variavel("KANGEST_SELO_PUBLICA")?)?;
    let texto = std::fs::read_to_string(selo).map_err(|e| format!("não foi possível ler {selo}: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&texto).map_err(|_| format!("{selo} ilegível"))?;
    let (Some(versao), Some(alvo), Some(resumo), Some(assinatura)) = (v["versao"].as_str(), v["alvo"].as_str(), v["resumo"].as_str(), v["assinatura"].as_str()) else {
        return Err(format!("{selo} incompleto (compilado sem KANGEST_SELO_PRIVADA?)"));
    };
    UnparsedPublicKey::new(&ED25519, &publica)
        .verify(formato::mensagem(versao, alvo, resumo).as_bytes(), &base64(assinatura)?)
        .map_err(|_| "a assinatura do selo não confere com KANGEST_SELO_PUBLICA")?;
    let actual = formato::hex(&formato::resumo(&ler(exe)?)?);
    if actual != resumo {
        return Err(format!("{exe} não corresponde ao selo: {actual} ≠ {resumo}"));
    }
    println!("Selo confirmado: {exe} ({versao}, {alvo})");
    Ok(())
}

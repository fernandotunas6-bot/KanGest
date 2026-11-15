//! Ficheiros exportados que não são texto (livros Excel .xlsx gerados no ecrã): chegam em base64 e gravam-se
//! na pasta de exportação deste computador (num posto cliente, no próprio posto, como os CSV).

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct BinarioIn {
    nome: String,
    base64: String,
}

/// Nome limpo de separadores e caracteres especiais, para não sair da pasta de exportação.
fn limpar_nome(nome: &str) -> AppResult<String> {
    let n = nome
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if n.is_empty() || !n.contains('.') {
        return Err(AppError::msg("Nome de ficheiro inválido."));
    }
    Ok(n)
}

pub async fn exportar_binario(st: &AppState, e: BinarioIn) -> AppResult<Value> {
    st.sessao()?;
    let nome = limpar_nome(&e.nome)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(e.base64.trim())
        .map_err(|_| AppError::msg("O conteúdo do ficheiro não é válido."))?;
    if bytes.is_empty() {
        return Err(AppError::msg("O ficheiro está vazio."));
    }
    let caminho = st.pasta_exportacao.join(&nome);
    std::fs::write(&caminho, bytes).map_err(|err| AppError::msg(format!("Não foi possível gravar {}: {err}", caminho.display())))?;
    Ok(json!({ "ficheiro": caminho.display().to_string() }))
}

#[cfg(test)]
mod testes {
    use super::limpar_nome;

    #[test]
    fn nomes_seguros() {
        assert_eq!(limpar_nome("Clientes 2026.xlsx").unwrap(), "Clientes 2026.xlsx");
        assert_eq!(limpar_nome("../../etc/passwd.xlsx").unwrap(), "_.._etc_passwd.xlsx");
        assert!(limpar_nome("semextensao").is_err());
        assert!(limpar_nome("..").is_err());
    }
}

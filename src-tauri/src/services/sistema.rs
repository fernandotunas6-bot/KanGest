//! Cópias de segurança (Fr_CopiaSeguranca do KIGEST): manual e automática diária.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MANTER: usize = 14;

/// Cópia consistente da base (mesmo com a aplicação em uso) via `VACUUM INTO`.
pub async fn copiar(pool: &SqlitePool, destino: &Path) -> AppResult<()> {
    if destino.exists() {
        std::fs::remove_file(destino).map_err(|e| AppError::msg(format!("Não foi possível substituir {}: {e}", destino.display())))?;
    }
    sqlx::query("VACUUM INTO ?").bind(destino.to_string_lossy().to_string()).execute(pool).await?;
    Ok(())
}

pub async fn backup(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let nome = format!("kangest-copia-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let destino = st.pasta_exportacao.join(nome);
    copiar(&st.pool, &destino).await?;
    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);
    Ok(json!({ "ficheiro": destino.display().to_string(), "bytes": bytes }))
}

fn pasta_copias(pasta_dados: &Path) -> PathBuf {
    pasta_dados.join("copias")
}

/// Uma cópia por dia na pasta de dados; mantém as últimas 14.
pub async fn backup_automatico(pool: &SqlitePool, pasta_dados: &Path) -> AppResult<Option<PathBuf>> {
    let dir = pasta_copias(pasta_dados);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    let destino = dir.join(format!("kangest-{}.db", chrono::Local::now().format("%Y%m%d")));
    if destino.exists() {
        return Ok(None);
    }
    copiar(pool, &destino).await?;
    // as mais antigas saem primeiro, pela data do ficheiro (os nomes mudaram de «kigest-» para «kangest-»)
    let mut ficheiros: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| AppError::msg(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
        .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    ficheiros.sort();
    while ficheiros.len() > MANTER {
        let _ = std::fs::remove_file(ficheiros.remove(0).1);
    }
    Ok(Some(destino))
}

pub async fn copias(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.backup")?;
    let dir = pasta_copias(&st.pasta_dados);
    let mut lista: Vec<Value> = std::fs::read_dir(&dir)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
                .map(|e| json!({ "ficheiro": e.path().display().to_string(), "bytes": e.metadata().map(|m| m.len()).unwrap_or(0) }))
                .collect()
        })
        .unwrap_or_default();
    lista.sort_by(|a, b| b["ficheiro"].as_str().cmp(&a["ficheiro"].as_str()));
    Ok(json!({ "pasta": dir.display().to_string(), "copias": lista }))
}

#[derive(serde::Deserialize)]
pub struct ExportarIn {
    nome: String,
    conteudo: String,
}

/// Grava um ficheiro de texto (CSV, mapas) na pasta de exportação e devolve o caminho.
/// O nome é limpo de separadores para não sair da pasta.
pub async fn exportar(st: &AppState, e: ExportarIn) -> AppResult<Value> {
    st.sessao()?;
    let nome: String = e
        .nome
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();
    if nome.is_empty() || !nome.contains('.') {
        return Err(AppError::msg("Nome de ficheiro inválido."));
    }
    let caminho = st.pasta_exportacao.join(&nome);
    // BOM UTF-8: o Excel abre os acentos correctamente
    let mut bytes = Vec::with_capacity(e.conteudo.len() + 3);
    if nome.to_lowercase().ends_with(".csv") {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(e.conteudo.as_bytes());
    std::fs::write(&caminho, bytes).map_err(|err| AppError::msg(format!("Não foi possível gravar {}: {err}", caminho.display())))?;
    Ok(json!({ "ficheiro": caminho.display().to_string() }))
}

/// Ecrã «Sobre»: versão, pasta de dados, tamanho da base e contagens principais.
pub async fn info(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let base = st.pasta_dados.join("kigest.db");
    let bytes = std::fs::metadata(&base).map(|m| m.len()).unwrap_or(0);
    let conta = |sql: &'static str| async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&st.pool).await };
    let sqlite: String = sqlx::query_scalar("SELECT sqlite_version()").fetch_one(&st.pool).await?;
    let migracoes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&st.pool).await.unwrap_or(0);
    let ultima_copia = std::fs::read_dir(pasta_copias(&st.pasta_dados))
        .ok()
        .and_then(|it| it.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".db")).max());
    Ok(json!({
        "versao": env!("CARGO_PKG_VERSION"),
        "plataforma": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "sqlite": sqlite,
        "migracoes": migracoes,
        "pasta_dados": st.pasta_dados.display().to_string(),
        "pasta_exportacao": st.pasta_exportacao.display().to_string(),
        "base_bytes": bytes,
        "ultima_copia": ultima_copia,
        "artigos": conta("SELECT COUNT(*) FROM artigos").await?,
        "entidades": conta("SELECT COUNT(*) FROM entidades").await?,
        "documentos": conta("SELECT COUNT(*) FROM documentos").await?,
        "importados": conta("SELECT COUNT(*) FROM documentos WHERE importado = 1").await?,
        "utilizadores": conta("SELECT COUNT(*) FROM utilizadores").await?,
    }))
}

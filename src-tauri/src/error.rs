use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),
    #[error("Sessão inválida. Inicie sessão novamente.")]
    NaoAutenticado,
    #[error("Sem permissão para esta operação.")]
    SemPermissao,
    #[error("Registo não encontrado.")]
    NaoEncontrado,
    #[error("Erro de base de dados: {0}")]
    Db(#[from] sqlx::Error),
    #[error("Erro de migração: {0}")]
    Migracao(#[from] sqlx::migrate::MigrateError),
    #[error("Parâmetros inválidos: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        AppError::Msg(s.into())
    }
}

// O frontend recebe sempre uma string legível em português.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

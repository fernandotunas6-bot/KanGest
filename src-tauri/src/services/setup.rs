use serde::Deserialize;
use serde_json::{json, Value};

use super::auth::hash_senha;
use crate::agt;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn estado(st: &AppState) -> AppResult<Value> {
    let empresa: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM empresa").fetch_one(&st.pool).await?;
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilizadores").fetch_one(&st.pool).await?;
    let nome: Option<String> = sqlx::query_scalar("SELECT nome FROM empresa WHERE id = 1").fetch_optional(&st.pool).await?;
    Ok(json!({ "inicializado": empresa > 0 && users > 0, "empresa": nome }))
}

#[derive(Deserialize)]
pub struct EmpresaIn {
    nome: String,
    designacao_comercial: Option<String>,
    nif: String,
    endereco: Option<String>,
    localidade: Option<String>,
    provincia: Option<String>,
    telefone: Option<String>,
    email: Option<String>,
    regime_iva: Option<String>,
    rodape: Option<String>,
    bancos: Option<Value>,
    /// Logótipo em data URL (image/png ou image/jpeg), até ~1 MB.
    logo: Option<String>,
}

#[derive(Deserialize)]
pub struct AdminIn {
    nome: String,
    login: String,
    senha: String,
}

#[derive(Deserialize)]
pub struct InicializarIn {
    empresa: EmpresaIn,
    admin: AdminIn,
}

async fn gravar_empresa(st: &AppState, e: &EmpresaIn) -> AppResult<()> {
    if e.nome.trim().is_empty() || e.nif.trim().is_empty() {
        return Err(AppError::msg("Nome e NIF da empresa são obrigatórios."));
    }
    if let Some(l) = e.logo.as_deref().filter(|l| !l.is_empty()) {
        if !(l.starts_with("data:image/png;base64,") || l.starts_with("data:image/jpeg;base64,")) || l.len() > 1_400_000 {
            return Err(AppError::msg("O logótipo deve ser PNG ou JPEG com menos de 1 MB."));
        }
    }
    sqlx::query(
        "INSERT INTO empresa (id, nome, designacao_comercial, nif, endereco, localidade, provincia, telefone, email, regime_iva, rodape, bancos, logo)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET nome=excluded.nome, designacao_comercial=excluded.designacao_comercial, nif=excluded.nif,
           endereco=excluded.endereco, localidade=excluded.localidade, provincia=excluded.provincia, telefone=excluded.telefone,
           email=excluded.email, regime_iva=excluded.regime_iva, rodape=excluded.rodape, bancos=excluded.bancos, logo=excluded.logo",
    )
    .bind(e.nome.trim())
    .bind(&e.designacao_comercial)
    .bind(e.nif.trim())
    .bind(&e.endereco)
    .bind(&e.localidade)
    .bind(&e.provincia)
    .bind(&e.telefone)
    .bind(&e.email)
    .bind(e.regime_iva.clone().unwrap_or_else(|| "GERAL".into()))
    .bind(&e.rodape)
    .bind(e.bancos.as_ref().map(|b| b.to_string()))
    .bind(e.logo.as_deref().filter(|l| !l.is_empty()))
    .execute(&st.pool)
    .await?;
    Ok(())
}

pub async fn inicializar(st: &AppState, i: InicializarIn) -> AppResult<Value> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilizadores").fetch_one(&st.pool).await?;
    if users > 0 {
        return Err(AppError::msg("O sistema já foi inicializado."));
    }
    if i.admin.senha.len() < 4 {
        return Err(AppError::msg("A senha deve ter pelo menos 4 caracteres."));
    }
    gravar_empresa(st, &i.empresa).await?;
    sqlx::query("INSERT INTO utilizadores (nome, login, senha_hash, perfil) VALUES (?, ?, ?, 'ADMIN')")
        .bind(i.admin.nome.trim())
        .bind(i.admin.login.trim())
        .bind(hash_senha(&i.admin.senha)?)
        .execute(&st.pool)
        .await?;
    agt::chave(&st.pool).await?; // gera par de chaves de desenvolvimento
    Ok(Value::Null)
}

pub async fn empresa_obter(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let rows = sqlx::query("SELECT * FROM empresa WHERE id = 1").fetch_all(&st.pool).await?;
    let mut v = rows.first().map(db::row_json).unwrap_or(Value::Null);
    // bancos é guardado como JSON em texto
    if let Some(b) = v.get("bancos").and_then(|b| b.as_str()).and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        v["bancos"] = b;
    }
    let cert: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?")
        .bind(agt::CERTIFICADO_CONFIG)
        .fetch_optional(&st.pool)
        .await?;
    let anterior: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'agt.certificado_anterior'")
        .fetch_optional(&st.pool)
        .await?;
    if v.is_object() {
        v["certificado"] = json!(cert.unwrap_or_default());
        // 2.ª via de documentos importados: foram processados pelo programa anterior (KIGEST)
        v["certificado_anterior"] = json!(anterior);
    }
    Ok(v)
}

pub async fn empresa_guardar(st: &AppState, e: EmpresaIn) -> AppResult<Value> {
    st.exigir_perm("sistema.configuracoes")?;
    gravar_empresa(st, &e).await?;
    Ok(Value::Null)
}

pub async fn agt_info(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.certificacao")?;
    let chave = agt::chave(&st.pool).await?;
    let cfg = |k: &'static str| async move {
        sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_optional(&st.pool).await
    };
    Ok(json!({
        "chave_publica": agt::chave_publica_pem(&chave)?,
        "certificado": cfg(agt::CERTIFICADO_CONFIG).await?.unwrap_or_default(),
        "certificado_anterior": cfg("agt.certificado_anterior").await?,
        "chave_importada": cfg("agt.chave_importada_em").await?,
    }))
}

#[derive(Deserialize)]
pub struct ChaveIn {
    pem: String,
}

/// Carrega a chave privada do produtor (PEM PKCS#8 ou PKCS#1). Só deve ser usada pelo titular
/// do certificado AGT do programa; a chave pública correspondente é a entregue à AGT.
pub async fn agt_importar_chave(st: &AppState, c: ChaveIn) -> AppResult<Value> {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
    use rsa::traits::PublicKeyParts;
    let s = st.exigir_perm("sistema.certificacao")?;
    let pem = c.pem.trim();
    let chave = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|_| AppError::msg("Chave inválida. Use um ficheiro PEM com a chave privada RSA (BEGIN PRIVATE KEY ou BEGIN RSA PRIVATE KEY)."))?;
    if chave.size() * 8 < 1024 {
        return Err(AppError::msg("A chave tem de ter pelo menos 1024 bits."));
    }
    let pkcs8 = chave.to_pkcs8_pem(LineEnding::LF).map_err(|e| AppError::msg(e.to_string()))?.to_string();
    let agora = crate::db::agora();
    let mut tx = st.pool.begin().await?;
    for (k, v) in [(agt::CHAVE_CONFIG, pkcs8.as_str()), ("agt.chave_importada_em", agora.as_str())] {
        sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
            .bind(k)
            .bind(v)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'AGT_CHAVE', 'Chave privada do produtor carregada')")
        .bind(s.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "chave_publica": agt::chave_publica_pem(&chave)? }))
}

#[derive(Deserialize)]
pub struct CertIn {
    certificado: String,
}

pub async fn agt_guardar(st: &AppState, c: CertIn) -> AppResult<Value> {
    st.exigir_perm("sistema.certificacao")?;
    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(agt::CERTIFICADO_CONFIG)
        .bind(c.certificado.trim())
        .execute(&st.pool)
        .await?;
    Ok(Value::Null)
}

//! Fotografias dos artigos. O ecrã reduz a imagem antes de a enviar (miniatura ~160 px para a grelha do POS e imagem
//! até 800 px para a ficha e a API); aqui só se confere o formato e o tamanho e se guarda.

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_MINIATURA: usize = 60 * 1024;
const MAX_IMAGEM: usize = 600 * 1024;
const TIPOS: [&str; 3] = ["image/webp", "image/jpeg", "image/png"];

/// Confere uma data URL de imagem (tipo aceite, base64 válido, tamanho) e devolve o tipo e os bytes.
pub fn abrir_data_url(d: &str, max: usize) -> AppResult<(String, Vec<u8>)> {
    let (cab, dados) = d.split_once(',').ok_or_else(|| AppError::msg("Imagem inválida."))?;
    let tipo = cab.strip_prefix("data:").and_then(|c| c.strip_suffix(";base64")).ok_or_else(|| AppError::msg("Imagem inválida."))?;
    if !TIPOS.contains(&tipo) {
        return Err(AppError::msg("A fotografia tem de ser WebP, JPEG ou PNG."));
    }
    if dados.len() > max * 4 / 3 + 4 {
        return Err(AppError::msg(format!("A fotografia é grande demais (máximo {} KB).", max / 1024)));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(dados.trim()).map_err(|_| AppError::msg("Imagem inválida."))?;
    // assinatura do formato: o conteúdo tem de ser mesmo uma imagem do tipo declarado
    let ok = match tipo {
        "image/png" => bytes.starts_with(b"\x89PNG"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        _ => bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
    };
    if !ok {
        return Err(AppError::msg("O conteúdo não é uma imagem válida."));
    }
    Ok((tipo.to_string(), bytes))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    artigo_id: i64,
    miniatura: String,
    imagem: String,
}

pub async fn guardar(st: &AppState, g: GuardarIn) -> AppResult<Value> {
    st.exigir_perm("artigos.editar")?;
    abrir_data_url(&g.miniatura, MAX_MINIATURA)?;
    abrir_data_url(&g.imagem, MAX_IMAGEM)?;
    let existe: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE id = ?").bind(g.artigo_id).fetch_optional(&st.pool).await?;
    if existe.is_none() {
        return Err(AppError::NaoEncontrado);
    }
    sqlx::query(
        "INSERT INTO artigo_fotos (artigo_id, miniatura, imagem) VALUES (?, ?, ?)
         ON CONFLICT(artigo_id) DO UPDATE SET miniatura = excluded.miniatura, imagem = excluded.imagem,
             alterado_em = strftime('%Y-%m-%dT%H:%M:%f', 'now', 'localtime')",
    )
    .bind(g.artigo_id)
    .bind(&g.miniatura)
    .bind(&g.imagem)
    .execute(&st.pool)
    .await?;
    // a loja online (API) vê a mudança na sincronização por «alterado_desde»
    sqlx::query("UPDATE artigos SET alterado_em = datetime('now','localtime') WHERE id = ?").bind(g.artigo_id).execute(&st.pool).await?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct IdIn {
    artigo_id: i64,
}

pub async fn apagar(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.exigir_perm("artigos.editar")?;
    sqlx::query("DELETE FROM artigo_fotos WHERE artigo_id = ?").bind(i.artigo_id).execute(&st.pool).await?;
    sqlx::query("UPDATE artigos SET alterado_em = datetime('now','localtime') WHERE id = ?").bind(i.artigo_id).execute(&st.pool).await?;
    Ok(json!({ "ok": true }))
}

/// Imagem grande de um artigo (ficha).
pub async fn obter(st: &AppState, i: IdIn) -> AppResult<Value> {
    st.sessao()?;
    let v: Option<String> = sqlx::query_scalar("SELECT imagem FROM artigo_fotos WHERE artigo_id = ?").bind(i.artigo_id).fetch_optional(&st.pool).await?;
    Ok(json!({ "imagem": v }))
}

#[derive(Deserialize)]
pub struct IdsIn {
    ids: Vec<i64>,
}

/// Miniaturas de vários artigos de uma vez (grelha do POS): { "id": dataURL }.
pub async fn miniaturas(st: &AppState, i: IdsIn) -> AppResult<Value> {
    st.sessao()?;
    let ids: Vec<i64> = i.ids.into_iter().take(300).collect();
    if ids.is_empty() {
        return Ok(json!({}));
    }
    let marcas = vec!["?"; ids.len()].join(",");
    let sql = format!("SELECT artigo_id, miniatura FROM artigo_fotos WHERE artigo_id IN ({marcas})");
    let mut q = sqlx::query(&sql);
    for id in &ids {
        q = q.bind(id);
    }
    let mut m = Map::new();
    for r in q.fetch_all(&st.pool).await? {
        m.insert(r.get::<i64, _>("artigo_id").to_string(), Value::String(r.get("miniatura")));
    }
    Ok(Value::Object(m))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn imagens_conferidas() {
        let png = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\n0000"));
        assert_eq!(abrir_data_url(&png, 1024).unwrap().0, "image/png");
        let falso = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"<script>"));
        assert!(abrir_data_url(&falso, 1024).is_err(), "o conteúdo tem de bater com o tipo");
        assert!(abrir_data_url("data:image/svg+xml;base64,PHN2Zz4=", 1024).is_err(), "SVG não (pode levar código)");
        assert!(abrir_data_url("texto", 1024).is_err());
    }
}

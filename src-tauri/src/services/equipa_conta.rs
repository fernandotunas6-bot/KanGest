//! Equipa gerida no site (a conta da empresa em kangest.pages.dev/conta): o dono cria, desactiva e muda o perfil e a loja
//! dos funcionários, e as lojas e os postos; o KanGest recebe-a na verificação da licença (`/v1/renovar`) quando está
//! atrás e aplica-a aqui.
//!
//! - Cada coisa liga-se pelo identificador do site (`conta_id`); se ainda não estiver ligada, adopta-se a que já existe
//!   com o mesmo login (funcionários) ou nome (lojas, postos). O que só existe no programa não se toca.
//! - Nunca se apaga nada: desactivar é que tira o acesso. O último administrador activo nunca perde o acesso.
//! - A senha provisória chega só como resumo PBKDF2 e obriga a mudar a senha no primeiro acesso.
//! - Os postos novos respeitam o limite de postos da licença.

use serde_json::{json, Value};
use sqlx::Row;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const CHAVE_VERSAO: &str = "conta.equipa_versao";

/// A versão da equipa gerida que este KanGest já aplicou (0 = nenhuma).
pub async fn versao(st: &AppState) -> i64 {
    sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?")
        .bind(CHAVE_VERSAO)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn texto(v: &Value, max: usize) -> String {
    v.as_str().unwrap_or("").trim().chars().take(max).collect()
}

/// Aplica a equipa que o site mandou. Devolve a versão aplicada e os avisos do que ficou por fazer.
pub async fn aplicar(st: &AppState, e: &Value) -> AppResult<Value> {
    let versao = e["versao"].as_i64().filter(|v| *v > 0).ok_or_else(|| AppError::msg("Equipa do site sem versão."))?;
    if versao <= self::versao(st).await {
        return Ok(json!({ "aplicada": versao, "avisos": [] }));
    }
    let vazio = Vec::new();
    let mut avisos: Vec<String> = Vec::new();
    let limite_postos = super::licenca::limite_postos(st).await?;
    let mut tx = st.pool.begin().await?;

    // lojas (armazéns)
    for l in e["lojas"].as_array().unwrap_or(&vazio) {
        let (id, nome, ativo) = (texto(&l["id"], 40), texto(&l["nome"], 60), l["ativo"].as_bool().unwrap_or(true));
        if id.is_empty() || nome.is_empty() {
            continue;
        }
        let existente: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM armazens WHERE conta_id = ?1 UNION ALL SELECT id FROM armazens WHERE conta_id IS NULL AND nome = ?2 COLLATE NOCASE LIMIT 1",
        )
        .bind(&id)
        .bind(&nome)
        .fetch_optional(&mut *tx)
        .await?;
        match existente {
            Some(a) => {
                sqlx::query("UPDATE armazens SET nome = ?, ativo = ?, conta_id = ? WHERE id = ?").bind(&nome).bind(ativo).bind(&id).bind(a).execute(&mut *tx).await?;
            }
            None => {
                sqlx::query("INSERT INTO armazens (nome, ativo, conta_id) VALUES (?, ?, ?)").bind(&nome).bind(ativo).bind(&id).execute(&mut *tx).await?;
            }
        }
    }
    let armazem_de = |nome: String| {
        sqlx::query_scalar::<_, i64>("SELECT id FROM armazens WHERE nome = ? COLLATE NOCASE ORDER BY ativo DESC, id LIMIT 1").bind(nome)
    };

    // postos
    for p in e["postos"].as_array().unwrap_or(&vazio) {
        let (id, descricao, loja) = (texto(&p["id"], 40), texto(&p["descricao"], 60), texto(&p["loja"], 60));
        if id.is_empty() || descricao.is_empty() {
            continue;
        }
        let Some(armazem) = armazem_de(loja.clone()).fetch_optional(&mut *tx).await? else {
            avisos.push(format!("Posto «{descricao}»: a loja «{loja}» não existe."));
            continue;
        };
        let existente: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM postos WHERE conta_id = ?1 UNION ALL SELECT id FROM postos WHERE conta_id IS NULL AND descricao = ?2 COLLATE NOCASE LIMIT 1",
        )
        .bind(&id)
        .bind(&descricao)
        .fetch_optional(&mut *tx)
        .await?;
        match existente {
            Some(pid) => {
                sqlx::query("UPDATE postos SET descricao = ?, armazem_id = ?, conta_id = ? WHERE id = ?").bind(&descricao).bind(armazem).bind(&id).bind(pid).execute(&mut *tx).await?;
            }
            None => {
                let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM postos").fetch_one(&mut *tx).await?;
                if limite_postos.is_some_and(|l| n >= l) {
                    avisos.push(format!("Posto «{descricao}»: a licença não permite mais postos."));
                    continue;
                }
                sqlx::query("INSERT INTO postos (descricao, armazem_id, serie_prefixo, opcoes, conta_id) VALUES (?, ?, '', (SELECT opcoes FROM postos WHERE id = 1), ?)")
                    .bind(&descricao)
                    .bind(armazem)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    // funcionários
    let perfis: Vec<String> = sqlx::query_scalar("SELECT codigo FROM perfis").fetch_all(&mut *tx).await?;
    for f in e["funcionarios"].as_array().unwrap_or(&vazio) {
        let (id, login, nome) = (texto(&f["id"], 40), texto(&f["login"], 40).to_lowercase(), texto(&f["nome"], 80));
        let (perfil, loja) = (texto(&f["perfil"], 20).to_uppercase(), texto(&f["loja"], 60));
        let mut ativo = f["ativo"].as_bool().unwrap_or(true);
        let provisoria = texto(&f["senha_provisoria"], 300);
        if id.is_empty() || login.is_empty() || nome.is_empty() {
            continue;
        }
        if !perfis.contains(&perfil) {
            avisos.push(format!("«{login}»: o perfil {perfil} não existe neste KanGest."));
            continue;
        }
        if !provisoria.is_empty() && !provisoria.starts_with("pbkdf2-sha256$") {
            avisos.push(format!("«{login}»: senha provisória em formato desconhecido."));
            continue;
        }
        let armazem = if loja.is_empty() { None } else { armazem_de(loja.clone()).fetch_optional(&mut *tx).await? };
        let existente = sqlx::query(
            "SELECT id, perfil, ativo FROM utilizadores WHERE conta_id = ?1 UNION ALL SELECT id, perfil, ativo FROM utilizadores WHERE conta_id IS NULL AND login = ?2 COLLATE NOCASE LIMIT 1",
        )
        .bind(&id)
        .bind(&login)
        .fetch_optional(&mut *tx)
        .await?;
        let mut perfil_final = perfil.clone();
        if let Some(u) = &existente {
            // o último administrador activo nunca perde o acesso
            let era_admin_activo = u.get::<String, _>("perfil") == "ADMIN" && u.get::<i64, _>("ativo") == 1;
            if era_admin_activo && (!ativo || perfil != "ADMIN") {
                let outros: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilizadores WHERE perfil = 'ADMIN' AND ativo = 1 AND id <> ?")
                    .bind(u.get::<i64, _>("id"))
                    .fetch_one(&mut *tx)
                    .await?;
                if outros == 0 {
                    avisos.push(format!("«{login}» é o único administrador activo: continua activo e administrador."));
                    ativo = true;
                    perfil_final = "ADMIN".into();
                }
            }
        }
        match existente {
            Some(u) => {
                let uid: i64 = u.get("id");
                sqlx::query("UPDATE utilizadores SET login = ?, nome = ?, perfil = ?, ativo = ?, armazem_id = ?, conta_id = ? WHERE id = ?")
                    .bind(&login)
                    .bind(&nome)
                    .bind(&perfil_final)
                    .bind(ativo)
                    .bind(armazem)
                    .bind(&id)
                    .bind(uid)
                    .execute(&mut *tx)
                    .await?;
                if !provisoria.is_empty() {
                    sqlx::query("UPDATE utilizadores SET senha_hash = ?, mudar_senha = 1, tentativas = 0, bloqueado_ate = NULL WHERE id = ?")
                        .bind(&provisoria)
                        .bind(uid)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            None => {
                if provisoria.is_empty() {
                    avisos.push(format!("«{login}»: falta a senha provisória (reponha-a no site)."));
                    continue;
                }
                sqlx::query("INSERT INTO utilizadores (nome, login, senha_hash, perfil, ativo, armazem_id, conta_id, mudar_senha) VALUES (?, ?, ?, ?, ?, ?, ?, 1)")
                    .bind(&nome)
                    .bind(&login)
                    .bind(&provisoria)
                    .bind(&perfil_final)
                    .bind(ativo)
                    .bind(armazem)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
        .bind(CHAVE_VERSAO)
        .bind(versao.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (NULL, 'EQUIPA_DO_SITE', ?)")
        .bind(format!("versão {versao}{}", if avisos.is_empty() { String::new() } else { format!("; avisos: {}", avisos.join(" · ")) }))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({ "aplicada": versao, "avisos": avisos }))
}

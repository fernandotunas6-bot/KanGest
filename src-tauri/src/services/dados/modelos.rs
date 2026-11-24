// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Modelos do construtor de relatórios: guardar, listar e apagar a definição de um relatório. Um modelo é de quem o
//! criou (só ele, ou um administrador, o altera ou apaga); «partilhado» mostra-o à equipa. Cada um só vê os modelos
//! das fontes que a sua permissão deixa consultar. As gravações ficam na auditoria (antes e depois), pelo registo das
//! fichas (`services/auditoria.rs`).

use serde::Deserialize;
use serde_json::{json, Value};

use super::fontes::fonte;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// A definição de um modelo é pequena (fonte, campos, filtros…): um limite generoso evita guardar lixo.
const MAX_DEFINICAO: usize = 20_000;

/// Os modelos que este utilizador pode abrir: os seus e os partilhados, das fontes que pode consultar.
pub async fn listar(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    let linhas = db::query_json_bind(
        &st.pool,
        "SELECT m.id, m.nome, m.fonte, m.definicao, m.partilhado, m.utilizador_id, u.nome AS dono, m.criado_em, m.alterado_em
         FROM relatorio_modelos m LEFT JOIN utilizadores u ON u.id = m.utilizador_id
         WHERE m.partilhado = 1 OR m.utilizador_id = ? ORDER BY m.nome COLLATE NOCASE",
        s.id,
    )
    .await?;
    Ok(Value::Array(
        linhas
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m["fonte"].as_str().and_then(fonte).is_some_and(|f| s.tem(f.permissao)))
            .map(|mut m| {
                // a definição vai como objecto (na base está em texto)
                let d = m["definicao"].as_str().and_then(|t| serde_json::from_str::<Value>(t).ok()).unwrap_or(Value::Null);
                m["definicao"] = d;
                m["meu"] = json!(m["utilizador_id"].as_i64() == Some(s.id));
                m
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct GuardarIn {
    id: Option<i64>,
    nome: String,
    fonte: String,
    definicao: Value,
    #[serde(default)]
    partilhado: bool,
}

/// Só quem criou o modelo, ou um administrador, o altera ou apaga.
async fn exigir_dono(st: &AppState, id: i64) -> AppResult<()> {
    let s = st.sessao()?;
    let dono: Option<Option<i64>> = sqlx::query_scalar("SELECT utilizador_id FROM relatorio_modelos WHERE id = ?").bind(id).fetch_optional(&st.pool).await?;
    let dono = dono.ok_or(AppError::NaoEncontrado)?;
    if dono != Some(s.id) && s.perfil != "ADMIN" {
        return Err(AppError::msg("Este modelo é de outro utilizador: guarde-o com outro nome para o mudar."));
    }
    Ok(())
}

pub async fn guardar(st: &AppState, i: GuardarIn) -> AppResult<Value> {
    let s = st.sessao()?;
    let f = fonte(&i.fonte).ok_or_else(|| AppError::msg(format!("Fonte de dados desconhecida: «{}».", i.fonte)))?;
    st.exigir_perm(f.permissao)?;
    let nome = i.nome.trim();
    if nome.is_empty() || nome.chars().count() > 80 {
        return Err(AppError::msg("Dê um nome ao modelo (até 80 caracteres)."));
    }
    if !i.definicao.is_object() {
        return Err(AppError::msg("Definição do modelo inválida."));
    }
    let definicao = i.definicao.to_string();
    if definicao.len() > MAX_DEFINICAO {
        return Err(AppError::msg("O modelo é grande de mais."));
    }
    let id = match i.id.filter(|id| *id > 0) {
        Some(id) => {
            exigir_dono(st, id).await?;
            sqlx::query("UPDATE relatorio_modelos SET nome = ?, fonte = ?, definicao = ?, partilhado = ?, alterado_em = datetime('now','localtime') WHERE id = ?")
                .bind(nome)
                .bind(f.id)
                .bind(&definicao)
                .bind(i.partilhado as i64)
                .bind(id)
                .execute(&st.pool)
                .await?;
            id
        }
        None => sqlx::query("INSERT INTO relatorio_modelos (nome, fonte, definicao, utilizador_id, partilhado) VALUES (?, ?, ?, ?, ?)")
            .bind(nome)
            .bind(f.id)
            .bind(&definicao)
            .bind(s.id)
            .bind(i.partilhado as i64)
            .execute(&st.pool)
            .await?
            .last_insert_rowid(),
    };
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct IdIn {
    id: i64,
}

pub async fn eliminar(st: &AppState, i: IdIn) -> AppResult<Value> {
    exigir_dono(st, i.id).await?;
    sqlx::query("DELETE FROM relatorio_modelos WHERE id = ?").bind(i.id).execute(&st.pool).await?;
    Ok(json!({ "eliminado": true }))
}

#[cfg(test)]
mod testes {
    use serde_json::{json, Value};

    use crate::db;
    use crate::state::AppState;

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }
    async fn erro(st: &AppState, m: &str, p: Value) -> String {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => panic!("{m} devia falhar: {v}"),
            Err(e) => e.to_string(),
        }
    }

    /// Modelos: de quem os cria, partilhados com a equipa, protegidos contra alterações de outros, e auditados.
    #[tokio::test]
    async fn modelos_de_relatorio() {
        let pasta = std::env::temp_dir().join(format!("kangest-modelos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Gestora", "login": "gestora", "perfil": "GESTOR", "senha": "gestora123", "mudar_senha": false })).await;

        let def = json!({ "modo": "resumo", "agrupar": ["mes"], "agregados": [{ "funcao": "soma", "campo": "total" }], "periodo": "ano" });
        let privado = rpc(&st, "dados.modeloGuardar", json!({ "nome": "Vendas por mês", "fonte": "vendas", "definicao": def })).await["id"].as_i64().unwrap();
        let partilhado = rpc(&st, "dados.modeloGuardar", json!({ "nome": "Caixa por conta", "fonte": "tesouraria", "definicao": { "modo": "resumo" }, "partilhado": true })).await["id"].as_i64().unwrap();
        let lista = rpc(&st, "dados.modelos", Value::Null).await;
        assert_eq!(lista.as_array().unwrap().len(), 2);
        assert_eq!(lista[1]["definicao"]["agrupar"][0], "mes", "a definição volta como objecto: {lista}");
        assert_eq!(lista[1]["meu"], true);

        assert!(erro(&st, "dados.modeloGuardar", json!({ "nome": " ", "fonte": "vendas", "definicao": {} })).await.contains("nome"));
        assert!(erro(&st, "dados.modeloGuardar", json!({ "nome": "X", "fonte": "documentos", "definicao": {} })).await.contains("desconhecida"));
        assert!(erro(&st, "dados.modeloGuardar", json!({ "nome": "X", "fonte": "vendas", "definicao": "texto" })).await.contains("inválida"));

        // outra pessoa: vê só o partilhado, e não o pode mudar nem apagar
        rpc(&st, "auth.login", json!({ "login": "gestora", "senha": "gestora123" })).await;
        let lista = rpc(&st, "dados.modelos", Value::Null).await;
        assert_eq!(lista.as_array().unwrap().len(), 1, "{lista}");
        assert_eq!((lista[0]["nome"].as_str(), lista[0]["meu"].as_bool(), lista[0]["dono"].as_str()), (Some("Caixa por conta"), Some(false), Some("Admin")));
        assert!(erro(&st, "dados.modeloGuardar", json!({ "id": partilhado, "nome": "Mudado", "fonte": "tesouraria", "definicao": {} })).await.contains("outro utilizador"));
        assert!(erro(&st, "dados.modeloEliminar", json!({ "id": partilhado })).await.contains("outro utilizador"));

        // o dono muda e apaga; tudo fica na auditoria, com o antes e o depois
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        rpc(&st, "dados.modeloGuardar", json!({ "id": privado, "nome": "Vendas mensais", "fonte": "vendas", "definicao": def })).await;
        rpc(&st, "dados.modeloEliminar", json!({ "id": partilhado })).await;
        assert_eq!(rpc(&st, "dados.modelos", Value::Null).await.as_array().unwrap().len(), 1);
        let acoes: Vec<(String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT acao, antes, depois FROM auditoria WHERE entidade = 'relatorio_modelos' ORDER BY id").fetch_all(&st.pool).await.unwrap();
        assert_eq!(acoes.len(), 4, "dois criados, um alterado, um apagado: {acoes:?}");
        assert!(acoes[2].1.as_deref().unwrap_or("").contains("Vendas por mês") && acoes[2].2.as_deref().unwrap_or("").contains("Vendas mensais"), "{acoes:?}");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

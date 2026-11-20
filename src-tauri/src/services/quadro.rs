// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Quadro de propostas e encomendas (como o kanban de cotações do Odoo): proformas, orçamentos e notas de
//! encomenda de clientes, ou encomendas e orçamentos de fornecedores, com o documento em que se converteram.

use serde::Deserialize;
use serde_json::Value;

use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct QuadroIn {
    /// VENDAS (PP, OR, NE) ou COMPRAS (VNE, VOR)
    grupo: String,
    /// os documentos fechados (convertidos ou anulados) só a partir desta data; os em aberto aparecem todos
    desde: Option<String>,
}

pub async fn quadro(st: &AppState, q: QuadroIn) -> AppResult<Value> {
    st.sessao()?;
    let tipos = if q.grupo == "COMPRAS" { "'VNE','VOR'" } else { "'PP','OR','NE'" };
    let rows = sqlx::query(&format!(
        "SELECT d.id, d.numero_doc, d.tipo_codigo, t.descricao AS tipo_desc, d.data_emissao, d.data_hora, d.data_vencimento,
                d.entidade_id, d.entidade_nome, d.total, d.estado, d.importado,
                (SELECT x.id FROM documentos x WHERE x.doc_origem_id = d.id AND x.estado <> 'A' ORDER BY x.id LIMIT 1) AS convertido_id,
                (SELECT x.numero_doc FROM documentos x WHERE x.doc_origem_id = d.id AND x.estado <> 'A' ORDER BY x.id LIMIT 1) AS convertido_em
         FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
         WHERE d.tipo_codigo IN ({tipos}) AND (d.estado = 'N' OR ?1 IS NULL OR d.data_emissao >= ?1)
         ORDER BY d.data_hora DESC, d.id DESC LIMIT 1000"
    ))
    .bind(q.desde.filter(|s| !s.is_empty()))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

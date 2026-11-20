// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! SAF-T de inventários (AGT): as existências de cada artigo no fim do exercício (31 de Dezembro), com a quantidade,
//! a unidade e o valor ao custo médio ponderado, o critério valorimétrico do KanGest.
//!
//! A obrigação vem do Regime Jurídico das Facturas (Decreto Presidencial n.º 71/25) e a estrutura do ficheiro da
//! Circular n.º 05/GACA/GJ/DSF/AGT/2026: cabeçalho com o NIF, o período, a data de geração e o software certificado;
//! por produto, o método de custeio, o tipo, o identificador, a descrição, a quantidade, a unidade e o valor. Todos os
//! contribuintes entregam, mesmo sem existências (ficheiro «sem operações»). Este serviço devolve esses dados; o ecrã
//! gera o livro Excel que se carrega no Portal do Contribuinte.

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct InventarioIn {
    ano: i32,
}

pub const METODO_CUSTEIO: &str = "Custo médio ponderado";

/// Tipos de inventário do SAF-T (código na ficha do artigo → nome no ficheiro).
pub const TIPOS_INVENTARIO: &[(&str, &str)] = &[
    ("M", "Mercadorias"),
    ("P", "Matérias-primas, subsidiárias e de consumo"),
    ("A", "Produtos acabados e intermédios"),
    ("S", "Subprodutos, desperdícios e refugos"),
    ("T", "Produtos e trabalhos em curso"),
];

/// O código de um tipo de inventário, validado (`None` se não for um dos cinco).
pub fn tipo_inventario(codigo: &str) -> Option<&'static str> {
    let c = codigo.trim().to_uppercase();
    TIPOS_INVENTARIO.iter().find(|t| t.0 == c).map(|t| t.0)
}

fn nome_tipo(codigo: &str) -> &'static str {
    TIPOS_INVENTARIO.iter().find(|t| t.0 == codigo).map(|t| t.1).unwrap_or(TIPOS_INVENTARIO[0].1)
}

pub async fn inventario(st: &AppState, i: InventarioIn) -> AppResult<Value> {
    st.exigir_perm("sistema.saft")?;
    let hoje = db::hoje();
    let ano_atual: i32 = hoje[..4].parse().unwrap_or(0);
    if i.ano < 2000 || i.ano > ano_atual {
        return Err(AppError::msg("Indique um exercício já começado (o ano do inventário)."));
    }
    let referencia = format!("{}-12-31", i.ano);
    // Exercício ainda a decorrer: as existências de hoje (inventário provisório). Exercício fechado: reconstituídas
    // pelos movimentos até 31/12 — por armazém, o saldo do último movimento até essa data; o custo médio (que é
    // global ao artigo) é o do último movimento do artigo até essa data.
    let provisorio = referencia >= hoje;
    let rows = if provisorio {
        sqlx::query(
            "SELECT a.id, a.referencia, a.descricao, a.codigo_barras, u.abreviatura AS unidade, u.descricao AS unidade_desc,
                    f.descricao AS familia, a.ativo, a.tipo_inventario,
                    CAST(COALESCE((SELECT SUM(s.quantidade) FROM stock s WHERE s.artigo_id = a.id), 0) AS REAL) AS quantidade,
                    a.custo_medio AS custo
             FROM artigos a
             LEFT JOIN unidades u ON u.id = a.unidade_id
             LEFT JOIN familias f ON f.id = a.familia_id
             WHERE a.tipo = 'P' AND a.controla_stock = 1
             ORDER BY a.referencia",
        )
        .fetch_all(&st.pool)
        .await?
    } else {
        sqlx::query(
            "WITH mov AS (
                 SELECT artigo_id, armazem_id, saldo, custo,
                        ROW_NUMBER() OVER (PARTITION BY artigo_id, armazem_id ORDER BY data_hora DESC, id DESC) AS n_arm,
                        ROW_NUMBER() OVER (PARTITION BY artigo_id ORDER BY data_hora DESC, id DESC) AS n_art
                 FROM movimentos_stock WHERE substr(data_hora, 1, 10) <= ?1),
              qtd AS (SELECT artigo_id, SUM(saldo) AS quantidade FROM mov WHERE n_arm = 1 GROUP BY artigo_id),
              cm AS (SELECT artigo_id, custo FROM mov WHERE n_art = 1)
             SELECT a.id, a.referencia, a.descricao, a.codigo_barras, u.abreviatura AS unidade, u.descricao AS unidade_desc,
                    f.descricao AS familia, a.ativo, a.tipo_inventario,
                    CAST(COALESCE(q.quantidade, 0) AS REAL) AS quantidade, COALESCE(c.custo, a.custo_medio) AS custo
             FROM artigos a
             LEFT JOIN qtd q ON q.artigo_id = a.id
             LEFT JOIN cm c ON c.artigo_id = a.id
             LEFT JOIN unidades u ON u.id = a.unidade_id
             LEFT JOIN familias f ON f.id = a.familia_id
             WHERE a.tipo = 'P' AND a.controla_stock = 1
             ORDER BY a.referencia",
        )
        .bind(&referencia)
        .fetch_all(&st.pool)
        .await?
    };

    let mut linhas = Vec::new();
    let mut negativos = Vec::new();
    let mut total = 0i64;
    for r in &rows {
        let quantidade: f64 = r.get("quantidade");
        let custo: i64 = r.get("custo");
        let referencia_art: String = r.get("referencia");
        let descricao: String = r.get("descricao");
        // arredondamento à unidade de quantidade usada no resto do KanGest (3 casas)
        let quantidade = (quantidade * 1000.0).round() / 1000.0;
        if quantidade < 0.0 {
            negativos.push(json!({ "referencia": referencia_art, "descricao": descricao, "quantidade": quantidade }));
            continue;
        }
        if quantidade == 0.0 {
            continue;
        }
        let valor = (quantidade * custo as f64).round() as i64;
        total += valor;
        linhas.push(json!({
            "referencia": referencia_art,
            "descricao": descricao,
            "codigo_barras": r.get::<Option<String>, _>("codigo_barras"),
            "tipo": nome_tipo(&r.get::<String, _>("tipo_inventario")),
            "tipo_codigo": r.get::<String, _>("tipo_inventario"),
            "unidade": r.get::<Option<String>, _>("unidade").unwrap_or_else(|| "UN".into()),
            "unidade_desc": r.get::<Option<String>, _>("unidade_desc"),
            "familia": r.get::<Option<String>, _>("familia"),
            "quantidade": quantidade,
            "custo_medio": custo,
            "valor": valor,
            "ativo": r.get::<i64, _>("ativo") == 1,
        }));
    }
    let empresa = super::setup::empresa_obter(st).await?;
    Ok(json!({
        "ano": i.ano,
        "data_referencia": referencia,
        "data_geracao": db::agora(),
        "provisorio": provisorio,
        "empresa": { "nif": empresa["nif"], "nome": empresa["nome"] },
        "software": { "nome": "KanGest", "versao": env!("CARGO_PKG_VERSION"), "certificado": empresa["certificado"] },
        "metodo_custeio": METODO_CUSTEIO,
        "sem_operacoes": linhas.is_empty(),
        "total_artigos": linhas.len(),
        "total_valor": total,
        "por_tipo": TIPOS_INVENTARIO
            .iter()
            .map(|(c, nome)| {
                let ls: Vec<&Value> = linhas.iter().filter(|l| l["tipo_codigo"] == *c).collect();
                json!({ "codigo": c, "tipo": nome, "artigos": ls.len(), "valor": ls.iter().map(|l| l["valor"].as_i64().unwrap_or(0)).sum::<i64>() })
            })
            .filter(|t| t["artigos"].as_u64() != Some(0))
            .collect::<Vec<_>>(),
        "linhas": linhas,
        "negativos": negativos,
    }))
}

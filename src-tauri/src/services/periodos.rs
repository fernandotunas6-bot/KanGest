// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Fecho de períodos (decisão do dono, 2026-11-24). O administrador fecha um mês que já terminou e, a partir daí, nada
//! com data desse mês se lança nem se anula:
//! - documentos desse mês já não se anulam (corrigem-se com nota de crédito);
//! - movimentos de tesouraria com data desse mês não se lançam nem se anulam;
//! - compras com a data do documento do fornecedor nesse mês não se registam;
//! - regularizações de conta corrente desse mês não se anulam.
//!
//! Os vales e os acertos de caixa, as vendas, os recibos e as transferências têm sempre a data de hoje, e o mês de hoje
//! nunca está fechado. Reabrir pede um motivo. Fechar e reabrir ficam na auditoria (`FICHAS`: periodos_fechados). Os
//! gatilhos da migração 0049 repetem a regra na base, para qualquer caminho que escape a `exigir_aberto`.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MESES: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

/// «outubro de 2026» (AAAA-MM), para as mensagens.
pub fn nome_mes(mes: &str) -> String {
    let n: usize = mes.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(0);
    match MESES.get(n.wrapping_sub(1)) {
        Some(nome) => format!("{nome} de {}", &mes[..4]),
        None => mes.to_string(),
    }
}

/// Recusa uma operação com data num mês fechado, a explicar o que fazer. `porque` diz o que não se pode fazer
/// («Os documentos desse mês já não se anulam: corrija com uma nota de crédito.»).
pub async fn exigir_aberto<'e, E>(ex: E, data: &str, porque: &str) -> AppResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let Some(mes) = data.get(..7) else { return Ok(()) };
    let fechado = sqlx::query_scalar::<_, i64>("SELECT 1 FROM periodos_fechados WHERE mes = ?").bind(mes).fetch_optional(ex).await?.is_some();
    if fechado {
        return Err(AppError::msg(format!(
            "O mês de {} está fechado. {porque} Só o administrador o pode reabrir, em Gestão de Finanças › Fecho de Períodos.",
            nome_mes(mes)
        )));
    }
    Ok(())
}

/// O mês do pedido, validado (AAAA-MM).
fn mes_valido(mes: &str) -> AppResult<String> {
    let m = mes.trim();
    if m.len() != 7 || chrono::NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").is_err() {
        return Err(AppError::msg("Mês inválido: use AAAA-MM."));
    }
    Ok(m.to_string())
}

/// Os últimos 24 meses (e os fechados mais antigos), com o estado e quantos documentos e movimentos têm.
pub async fn listar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.periodos")?;
    let hoje = db::hoje();
    let atual = &hoje[..7];
    let linhas = sqlx::query(
        "WITH RECURSIVE m(mes, n) AS (
             SELECT substr(?1, 1, 7), 0
             UNION ALL SELECT substr(date(mes || '-01', '-1 month'), 1, 7), n + 1 FROM m WHERE n < 23
         ),
         todos(mes) AS (SELECT mes FROM m UNION SELECT mes FROM periodos_fechados)
         SELECT t.mes, p.fechado_em, u.nome AS fechado_por,
                (SELECT COUNT(*) FROM documentos d WHERE d.data_emissao >= t.mes || '-01' AND d.data_emissao < date(t.mes || '-01', '+1 month')) AS documentos,
                (SELECT COUNT(*) FROM movimentos_tesouraria x WHERE x.data >= t.mes || '-01' AND x.data < date(t.mes || '-01', '+1 month')) AS movimentos
         FROM todos t
         LEFT JOIN periodos_fechados p ON p.mes = t.mes
         LEFT JOIN utilizadores u ON u.id = p.utilizador_id
         ORDER BY t.mes DESC",
    )
    .bind(&hoje)
    .fetch_all(&st.pool)
    .await?;
    let mut out = Vec::new();
    for r in db::rows_json(&linhas).as_array().cloned().unwrap_or_default() {
        let mes = r["mes"].as_str().unwrap_or_default().to_string();
        let fechado = !r["fechado_em"].is_null();
        out.push(json!({
            "mes": mes,
            "nome": nome_mes(&mes),
            "estado": if fechado { "fechado" } else if mes.as_str() == atual { "em_curso" } else { "aberto" },
            "fechado_em": r["fechado_em"],
            "fechado_por": r["fechado_por"],
            "documentos": r["documentos"],
            "movimentos": r["movimentos"],
        }));
    }
    Ok(Value::Array(out))
}

#[derive(Deserialize)]
pub struct MesIn {
    mes: String,
    /// só na reabertura (fica na auditoria)
    #[serde(default)]
    motivo: Option<String>,
}

/// Fecha um mês que já terminou.
pub async fn fechar(st: &AppState, m: MesIn) -> AppResult<Value> {
    let s = st.exigir_perm("sistema.periodos")?;
    let mes = mes_valido(&m.mes)?;
    if mes.as_str() >= &db::hoje()[..7] {
        return Err(AppError::msg("Só se fecha um mês que já terminou."));
    }
    let nome = nome_mes(&mes);
    let feito = sqlx::query("INSERT OR IGNORE INTO periodos_fechados (mes, descricao, fechado_em, utilizador_id) VALUES (?, ?, ?, ?)")
        .bind(&mes)
        .bind(primeira_maiuscula(&nome))
        .bind(db::agora())
        .bind(s.id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if feito == 0 {
        return Err(AppError::msg(format!("O mês de {nome} já está fechado.")));
    }
    Ok(json!({ "mes": mes }))
}

/// Reabre um mês fechado, com o motivo (fica na auditoria).
pub async fn reabrir(st: &AppState, m: MesIn) -> AppResult<Value> {
    st.exigir_perm("sistema.periodos")?;
    let mes = mes_valido(&m.mes)?;
    if m.motivo.as_deref().map(str::trim).unwrap_or_default().chars().count() < 5 {
        return Err(AppError::msg("Indique o motivo da reabertura (fica na auditoria)."));
    }
    let feito = sqlx::query("DELETE FROM periodos_fechados WHERE mes = ?").bind(&mes).execute(&st.pool).await?.rows_affected();
    if feito == 0 {
        return Err(AppError::msg(format!("O mês de {} não está fechado.", nome_mes(&mes))));
    }
    Ok(json!({ "mes": mes }))
}

fn primeira_maiuscula(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
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

    #[test]
    fn nomes_dos_meses() {
        assert_eq!(super::nome_mes("2026-10"), "outubro de 2026");
        assert_eq!(super::nome_mes("2026-03"), "março de 2026");
        assert_eq!(super::nome_mes("2026-13"), "2026-13");
        assert_eq!(super::primeira_maiuscula("outubro de 2026"), "Outubro de 2026");
    }

    /// Fechar e reabrir: só o administrador, só meses terminados, reabrir com motivo, tudo na auditoria; com o mês
    /// fechado não se anulam documentos, movimentos nem regularizações, nem se lançam movimentos ou compras com data
    /// desse mês; depois de reaberto, volta tudo a poder fazer-se.
    #[tokio::test]
    async fn fecho_de_periodos() {
        let pasta = std::env::temp_dir().join(format!("kangest-periodos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let hoje = db::hoje();
        let atual = hoje[..7].to_string();
        let anterior = {
            let (a, m): (i32, u32) = (hoje[..4].parse().unwrap(), hoje[5..7].parse().unwrap());
            if m == 1 { format!("{}-12", a - 1) } else { format!("{a}-{:02}", m - 1) }
        };
        let dia_anterior = format!("{anterior}-15");

        // o que se vai tentar com o mês fechado: um movimento do mês anterior, uma venda e uma regularização de hoje
        let conta: i64 = sqlx::query_scalar("SELECT id FROM contas ORDER BY id LIMIT 1").fetch_one(&st.pool).await.unwrap();
        let mov = rpc(&st, "tesouraria.movimento", json!({ "tipo": "ENTRADA", "conta_id": conta, "valor": 10_000, "descricao": "Venda de sucata", "data": &dia_anterior })).await["id"].as_i64().unwrap();
        let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Livro", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
        let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente a Crédito", "nif": "5417000123" })).await["id"].as_i64().unwrap();
        let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await["documento"]["id"].as_i64().unwrap();
        let reg = rpc(&st, "contas.regularizar", json!({ "entidade_id": cli, "tipo": "CREDITO", "valor": 1000, "justificacao": "Acerto de arredondamento" })).await["id"].as_i64().unwrap();
        let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Grossista, SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();

        // regras do fecho
        assert!(erro(&st, "periodos.fechar", json!({ "mes": &atual })).await.contains("já terminou"));
        assert!(erro(&st, "periodos.fechar", json!({ "mes": "2026-13" })).await.contains("inválido"));
        rpc(&st, "periodos.fechar", json!({ "mes": &anterior })).await;
        assert!(erro(&st, "periodos.fechar", json!({ "mes": &anterior })).await.contains("já está fechado"));
        let lista = rpc(&st, "periodos.listar", Value::Null).await;
        let linha = |m: &str| lista.as_array().unwrap().iter().find(|l| l["mes"] == m).cloned().unwrap();
        assert_eq!((linha(&anterior)["estado"].as_str(), linha(&atual)["estado"].as_str()), (Some("fechado"), Some("em_curso")), "{lista}");
        assert_eq!(linha(&anterior)["fechado_por"], "Admin");
        assert_eq!(linha(&anterior)["movimentos"], 1);
        assert_eq!(lista.as_array().unwrap().len(), 24);

        // com o mês anterior fechado: nem lançar nem anular movimentos com data dele, nem compras com a data dele
        let e = erro(&st, "tesouraria.movimento", json!({ "tipo": "SAIDA", "conta_id": conta, "valor": 500, "descricao": "Esquecido", "data": &dia_anterior })).await;
        assert!(e.contains("está fechado") && e.contains("Fecho de Períodos"), "{e}");
        assert!(erro(&st, "tesouraria.anular", json!({ "id": mov, "motivo": "Lançado em duplicado" })).await.contains("está fechado"));
        let fc = |data: &str, ext: &str| json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": ext, "doc_externo_data": data, "linhas": [{ "artigo_id": art, "quantidade": 1, "preco_unitario": 50000 }] });
        assert!(erro(&st, "documentos.emitir", fc(&dia_anterior, "FT A/1")).await.contains("está fechado"));
        rpc(&st, "documentos.emitir", fc(&hoje, "FT A/2")).await;
        rpc(&st, "tesouraria.movimento", json!({ "tipo": "SAIDA", "conta_id": conta, "valor": 500, "descricao": "Hoje", "data": &hoje })).await;

        // o mês de hoje (fechado à mão, como se o mês já tivesse terminado): os documentos e as regularizações
        // desse mês não se anulam; a nota de crédito continua a poder emitir-se
        sqlx::query("INSERT INTO periodos_fechados (mes, descricao, fechado_em) VALUES (?, 'Teste', ?)").bind(&atual).bind(db::agora()).execute(&st.pool).await.unwrap();
        let e = erro(&st, "documentos.anular", json!({ "id": ft, "motivo": "Cliente desistiu da compra" })).await;
        assert!(e.contains("nota de crédito"), "{e}");
        assert!(erro(&st, "contas.regularizacaoAnular", json!({ "id": reg, "motivo": "Engano" })).await.contains("está fechado"));
        // a base recusa também por fora dos serviços
        let direto = sqlx::query("UPDATE documentos SET estado = 'A' WHERE id = ?").bind(ft).execute(&st.pool).await;
        assert!(direto.unwrap_err().to_string().contains("fechado"));
        let direto = sqlx::query("UPDATE movimentos_tesouraria SET anulado = 1 WHERE id = ?").bind(mov).execute(&st.pool).await;
        assert!(direto.unwrap_err().to_string().contains("fechado"));
        let direto = sqlx::query("UPDATE cc_regularizacoes SET anulado = 1 WHERE id = ?").bind(reg).execute(&st.pool).await;
        assert!(direto.unwrap_err().to_string().contains("fechado"));
        rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": ft, "motivo": "Devolução do livro", "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
        sqlx::query("DELETE FROM periodos_fechados WHERE mes = ?").bind(&atual).execute(&st.pool).await.unwrap();

        // só o administrador fecha e reabre
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Gestor", "login": "gestor", "perfil": "GESTOR", "senha": "gestor123", "mudar_senha": false })).await;
        rpc(&st, "auth.login", json!({ "login": "gestor", "senha": "gestor123" })).await;
        assert!(erro(&st, "periodos.reabrir", json!({ "mes": &anterior, "motivo": "Quero lançar" })).await.contains("permissão"));
        assert!(erro(&st, "periodos.listar", Value::Null).await.contains("permissão"));
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

        // reabrir pede motivo; depois, volta tudo a poder fazer-se
        assert!(erro(&st, "periodos.reabrir", json!({ "mes": &anterior })).await.contains("motivo"));
        rpc(&st, "periodos.reabrir", json!({ "mes": &anterior, "motivo": "Falta lançar a renda de outubro" })).await;
        assert!(erro(&st, "periodos.reabrir", json!({ "mes": &anterior, "motivo": "De novo, por engano" })).await.contains("não está fechado"));
        rpc(&st, "tesouraria.anular", json!({ "id": mov, "motivo": "Lançado em duplicado" })).await;
        rpc(&st, "documentos.emitir", fc(&dia_anterior, "FT A/1")).await;

        // fechar e reabrir ficam na auditoria, com o motivo
        let aud: Vec<(String, String)> = sqlx::query_as("SELECT acao, detalhe FROM auditoria WHERE entidade = 'periodos_fechados' ORDER BY id").fetch_all(&st.pool).await.unwrap();
        assert_eq!(aud.len(), 2, "{aud:?}");
        assert!(aud[1].1.contains("Falta lançar a renda"), "{aud:?}");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

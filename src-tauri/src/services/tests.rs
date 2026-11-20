//! Teste de integração do núcleo comercial: corre o fluxo completo numa base SQLite temporária.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::signature::Verifier;
use serde_json::{json, Value};
use sha1::Sha1;

use super::dispatch;
use crate::state::AppState;
use crate::{agt, db};

async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
    match dispatch(st, m, p).await {
        Ok(v) => v,
        Err(e) => panic!("{m} falhou: {e}"),
    }
}

async fn rpc_err(st: &AppState, m: &str, p: Value) -> String {
    match dispatch(st, m, p).await {
        Ok(v) => panic!("{m} devia falhar, devolveu {v}"),
        Err(e) => e.to_string(),
    }
}

/// Valida um ficheiro SAF-T contra o XSD oficial da AGT quando `KIGEST_SAFT_XSD` aponta para ele (usa o xmllint).
fn validar_xsd(ficheiro: &str) {
    let Ok(xsd) = std::env::var("KIGEST_SAFT_XSD") else { return };
    let out = std::process::Command::new("xmllint").args(["--noout", "--schema", &xsd, ficheiro]).output().expect("xmllint");
    assert!(out.status.success(), "{ficheiro} não respeita o XSD:\n{}", String::from_utf8_lossy(&out.stderr));
}

async fn stock(st: &AppState, artigo: i64) -> f64 {
    let a = rpc(st, "artigos.obter", json!({ "id": artigo })).await;
    a["stock"].as_f64().unwrap()
}

#[tokio::test]
async fn fluxo_completo() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let caminho = pasta.join("kigest.db");
    let pool = db::abrir(&caminho).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.expect("posto 1");

    // ---- configuração inicial e login ----
    assert_eq!(rpc(&st, "setup.estado", Value::Null).await["inicializado"], false);
    rpc(
        &st,
        "setup.inicializar",
        json!({ "empresa": { "nome": "Loja Teste, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } }),
    )
    .await;
    assert!(rpc_err(&st, "artigos.listar", json!({})).await.contains("Sessão"));
    assert!(rpc_err(&st, "auth.login", json!({ "login": "admin", "senha": "errada" })).await.contains("incorrectos"));
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // ---- artigo: 1 000,00 Kz s/ IVA, IVA 14% ----
    let art = rpc(
        &st,
        "artigos.guardar",
        json!({ "descricao": "Arroz 5kg", "codigo_barras": "560123", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 }),
    )
    .await;
    let artigo = art["id"].as_i64().unwrap();
    assert_eq!(rpc(&st, "artigos.codigo", json!({ "codigo": "560123" })).await["id"], artigo);
    // isento sem motivo é recusado
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "X", "taxa_iva_id": 2, "preco1": 1 })).await.contains("motivo"));

    // ---- entrada de stock: 10 un. a 500,00 ----
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": artigo, "quantidade": 10, "preco_unitario": 50000 }] })).await;
    assert_eq!(stock(&st, artigo).await, 10.0);

    // ---- caixa + venda a pronto (FR) com troco ----
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 1000000 })).await;
    let fr1 = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 300000 }] }),
    )
    .await;
    let d1 = &fr1["documento"];
    let ano = &db::hoje()[..4];
    assert_eq!(d1["numero_doc"], format!("FR {ano}/1"));
    assert_eq!(d1["total_liquido"], 200000);
    assert_eq!(d1["total_iva"], 28000);
    assert_eq!(d1["total"], 228000);
    assert_eq!(d1["troco"], 72000);
    assert_eq!(fr1["pagamentos"][0]["valor"], 228000); // numerário líquido do troco
    assert_eq!(d1["hash"].as_str().unwrap().len(), 172);
    assert_eq!(d1["hash_controlo"].as_str().unwrap().len(), 4);
    assert_eq!(stock(&st, artigo).await, 8.0);

    // ---- segunda FR: verifica a cadeia de assinaturas com a chave pública ----
    let fr2 = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "MB", "valor": 114000 }] }),
    )
    .await;
    let d2 = &fr2["documento"];
    let chave = agt::chave(&st.pool).await.unwrap();
    let vk = VerifyingKey::<Sha1>::new(chave.to_public_key());
    let msg = agt::mensagem(
        d2["data_emissao"].as_str().unwrap(),
        d2["data_hora"].as_str().unwrap(),
        d2["numero_doc"].as_str().unwrap(),
        d2["total"].as_i64().unwrap(),
        d1["hash"].as_str().unwrap(),
    );
    let sig = Signature::try_from(B64.decode(d2["hash"].as_str().unwrap()).unwrap().as_slice()).unwrap();
    assert!(vk.verify(msg.as_bytes(), &sig).is_ok(), "a FR 2 deve assinar com o hash da FR 1");

    // troco só em numerário; pagamento insuficiente; stock insuficiente
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "MB", "valor": 200000 }] }))
        .await
        .contains("troco"));
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 100 }] }))
        .await
        .contains("insuficiente"));
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 100 }], "pagamentos": [{ "modo": "NU", "valor": 99999999 }] }))
        .await
        .contains("Stock insuficiente"));
    // a numeração não salta quando a emissão falha
    let fr3 = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] }),
    )
    .await;
    assert_eq!(fr3["documento"]["numero_doc"], format!("FR {ano}/3"));
    assert_eq!(stock(&st, artigo).await, 6.0);

    // ---- factura a crédito: exige cliente identificado ----
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await.contains("cliente identificado"));
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Crédito", "nif": "5417000000" })).await["id"].as_i64().unwrap();
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    let ft_id = ft["documento"]["id"].as_i64().unwrap();
    assert_eq!(ft["documento"]["valor_pendente"], 114000);
    assert_eq!(rpc(&st, "entidades.pendentes", json!({ "id": cli })).await.as_array().unwrap().len(), 1);

    // ---- recibo parcial + total ----
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": cli, "liquidar": [{ "documento_id": ft_id, "valor": 999999 }], "pagamentos": [{ "modo": "NU", "valor": 999999 }] }))
        .await
        .contains("superior"));
    let rc = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "RC", "entidade_id": cli, "liquidar": [{ "documento_id": ft_id, "valor": 114000 }], "pagamentos": [{ "modo": "TB", "valor": 114000 }] }),
    )
    .await;
    assert_eq!(rc["documento"]["total"], 114000);
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ft_id })).await["documento"]["valor_pendente"], 0);
    // a ficha da FT mostra o recibo que a liquidou (documentos ligados)
    let ligados = rpc(&st, "documentos.obter", json!({ "id": ft_id })).await;
    assert_eq!(ligados["recibos"].as_array().unwrap().len(), 1);
    assert_eq!(ligados["recibos"][0]["id"], rc["documento"]["id"]);
    assert_eq!(ligados["recibos"][0]["valor"], 114000);

    // FT com recibo não pode ser anulada; anula-se o recibo primeiro
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": ft_id, "motivo": "erro de digitação" })).await.contains("recibos"));
    rpc(&st, "documentos.anular", json!({ "id": rc["documento"]["id"], "motivo": "recibo errado" })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ft_id })).await["documento"]["valor_pendente"], 114000);
    rpc(&st, "documentos.anular", json!({ "id": ft_id, "motivo": "erro de digitação" })).await;
    assert_eq!(stock(&st, artigo).await, 6.0); // a FT tinha tirado 1 → reposto

    // ---- nota de crédito de uma FR (devolução) ----
    let fr1_id = d1["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "NC", "motivo": "Devolução", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await.contains("origem"));
    let nc = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "NC", "entidade_id": 1, "doc_origem_id": fr1_id, "motivo": "Devolução de 1 unidade", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] }),
    )
    .await;
    assert_eq!(nc["documento"]["total"], 114000);
    assert_eq!(nc["documento"]["doc_origem"], format!("FR {ano}/1"));
    assert_eq!(stock(&st, artigo).await, 7.0);

    // ---- proforma convertida em factura ----
    let pp = rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": cli, "linhas": [{ "artigo_id": artigo, "quantidade": 3 }] })).await;
    assert_eq!(stock(&st, artigo).await, 7.0); // proforma não mexe em stock
    let pp_id = pp["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "doc_origem_id": pp_id, "linhas": [{ "artigo_id": artigo, "quantidade": 3 }] })).await;
    let quadro = rpc(&st, "documentos.quadro", json!({ "grupo": "VENDAS" })).await;
    let no_quadro = quadro.as_array().unwrap().iter().find(|d| d["id"] == pp_id).cloned().unwrap();
    assert_eq!((no_quadro["estado"].as_str(), no_quadro["convertido_em"].as_str().map(|s| s.starts_with("FT "))), (Some("F"), Some(true)), "{no_quadro}");
    assert!(rpc(&st, "documentos.quadro", json!({ "grupo": "COMPRAS" })).await.as_array().unwrap().is_empty());
    let pp_ligados = rpc(&st, "documentos.obter", json!({ "id": pp_id })).await;
    assert_eq!(pp_ligados["documento"]["estado"], "F");
    assert_eq!(pp_ligados["derivados"][0]["tipo_codigo"], "FT", "a proforma mostra a factura emitida a partir dela");
    let fr1_ligados = rpc(&st, "documentos.obter", json!({ "id": fr1_id })).await;
    assert_eq!(fr1_ligados["derivados"][0]["tipo_codigo"], "NC", "a FR mostra a nota de crédito");
    assert_eq!(stock(&st, artigo).await, 4.0);

    // ---- transferência entre armazéns ----
    let arm2 = rpc(&st, "tabela.guardar", json!({ "tabela": "armazens", "descricao": "Loja 2" })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "TS", "armazem_destino_id": arm2, "linhas": [{ "artigo_id": artigo, "quantidade": 2 }] })).await;
    assert_eq!(stock(&st, artigo).await, 2.0);
    let s2 = rpc(&st, "stock.listar", json!({ "armazem_id": arm2 })).await;
    assert_eq!(s2[0]["quantidade"].as_f64().unwrap(), 2.0);

    // ---- compra a fornecedor actualiza custo médio ----
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Fornecedor SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "FT A/123", "linhas": [{ "artigo_id": artigo, "quantidade": 4, "preco_unitario": 80000 }] })).await;
    // a mesma V/Factura do fornecedor não se regista duas vezes (nem com espaços ou minúsculas diferentes)
    let rep = rpc_err(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "ft a/ 123", "linhas": [{ "artigo_id": artigo, "quantidade": 4, "preco_unitario": 80000 }] })).await;
    assert!(rep.contains("já foi registado"), "{rep}");
    // o vencimento não pode ser anterior à data do documento
    let venc = rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "data_vencimento": "2000-01-01", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    assert!(venc.contains("vencimento"), "{venc}");
    let a = rpc(&st, "artigos.obter", json!({ "id": artigo })).await;
    // existências globais antes: 2 (principal) + 2 (loja 2) = 4 a 500,00; entram 4 a 800,00 → 650,00
    assert_eq!(a["custo_medio"], 65000);
    assert_eq!(a["stock"].as_f64().unwrap(), 6.0);

    // ---- caixa: esperado = inicial + numerário ----
    let cx = rpc(&st, "caixa.estado", Value::Null).await;
    let nu = cx["por_modo"].as_array().unwrap().iter().find(|m| m["codigo"] == "NU").unwrap()["total"].as_i64().unwrap();
    assert_eq!(nu, 228000 + 114000);
    // NC da FR gerou saída de caixa (devolução)
    assert_eq!(cx["saidas"], 114000);
    assert_eq!(cx["numerario_esperado"], 1000000 + 342000 - 114000);
    let fecho = rpc(&st, "caixa.fechar", json!({ "valor_contado": 1228000 })).await;
    assert_eq!(fecho["sessao"]["estado"], "FECHADA");
    assert!(rpc(&st, "caixa.estado", Value::Null).await.is_null());

    // ---- painel e todos os relatórios executam ----
    let dash = rpc(&st, "dashboard", Value::Null).await;
    // FR1 228000 + FR2 114000 + FR3 114000 + FT(proforma) 342000 − NC 114000 (FT anulada não conta)
    assert_eq!(dash["vendas_hoje"], 228000 + 114000 + 114000 + 342000 - 114000);
    assert_eq!(dash["vendas_mes_anterior"], 0);
    let ant = &dash["antiguidade"];
    let soma_faixas: i64 = ["por_vencer", "d1_30", "d31_60", "d61_90", "mais_90"].iter().map(|k| ant[*k].as_i64().unwrap()).sum();
    assert_eq!(soma_faixas, dash["a_receber"].as_i64().unwrap(), "as faixas de antiguidade somam o «A receber»");
    assert!(dash["alertas"]["proformas_a_expirar"].is_i64() && dash["alertas"]["fe_rejeitadas"].is_i64());
    assert!(!dash["por_modo"].as_array().unwrap().is_empty(), "recebimentos do mês por meio de pagamento");
    for g in ["dia", "artigo", "familia", "operador", "cliente", "pagamento", "iva"] {
        let r = rpc(&st, "relatorios.vendas", json!({ "de": "2000-01-01", "ate": "2100-01-01", "agrupar": g })).await;
        assert!(r.as_array().map(|x| !x.is_empty()).unwrap_or(false), "relatório {g} vazio");
    }
    let docs = rpc(&st, "documentos.listar", json!({})).await;
    assert!(docs.as_array().unwrap().len() >= 10);

    // ---- SAF-T (AO) ----
    let saft = rpc(&st, "saft.exportar", json!({ "de": "2000-01-01", "ate": "2100-12-31" })).await;
    validar_xsd(saft["ficheiro"].as_str().unwrap());
    let xml = std::fs::read_to_string(saft["ficheiro"].as_str().unwrap()).unwrap();
    assert!(xml.contains(&format!("<InvoiceNo>FR {ano}/1</InvoiceNo>")));
    assert!(xml.contains("<InvoiceStatus>A</InvoiceStatus>"), "a FT anulada aparece com estado A");
    assert!(xml.contains("<WorkType>PP</WorkType>"));
    assert!(xml.contains("<PaymentType>RC</PaymentType>"));
    assert!(xml.contains("<CustomerTaxID>999999999</CustomerTaxID>"));
    assert!(xml.contains(&format!("<Reference>FR {ano}/1</Reference>")), "a NC referencia a FR de origem");
    // TotalCredit = soma NetTotal das facturas válidas (FR1 2000 + FR2 1000 + FR3 1000 + FT 3000)
    assert!(xml.contains("<TotalCredit>7000.00</TotalCredit>"));
    assert!(xml.contains("<TotalDebit>1000.00</TotalDebit>")); // NC
    let _ = std::fs::remove_file(saft["ficheiro"].as_str().unwrap());

    // ---- histórico de fechos de caixa ----
    let sessoes = rpc(&st, "caixa.sessoes", Value::Null).await;
    let sid = sessoes[0]["id"].as_i64().unwrap();
    let r = rpc(&st, "caixa.obter", json!({ "id": sid })).await;
    assert!(!r["artigos"].as_array().unwrap().is_empty(), "o fecho lista os artigos vendidos");

    // ---- cópias de segurança ----
    let bk = rpc(&st, "sistema.backup", Value::Null).await;
    assert!(std::path::Path::new(bk["ficheiro"].as_str().unwrap()).exists());
    // o fecho de caixa já fez uma cópia (verificada); ao abrir no mesmo dia não se repete
    let copias = rpc(&st, "sistema.copias", Value::Null).await;
    assert_eq!(copias["copias"].as_array().unwrap().len(), 1, "{copias}");
    assert_eq!(copias["copias"][0]["verificada"], true);
    assert!(super::sistema::backup_automatico(&st.pool, &pasta).await.unwrap().is_none(), "uma cópia automática por dia");

    // ---- posto 2 com série própria ----
    let p2 = rpc(&st, "posto.guardar", json!({ "descricao": "Posto#2", "armazem_id": 1, "serie_prefixo": "p2" })).await["id"].as_i64().unwrap();
    rpc(&st, "posto.selecionar", json!({ "id": p2 })).await;
    assert_eq!(st.posto().id, p2);
    assert!(pasta.join("posto.json").exists());
    let frp2 = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] }),
    )
    .await;
    assert_eq!(frp2["documento"]["numero_doc"], format!("FR P2{ano}/1"));
    assert!(rpc_err(&st, "posto.guardar", json!({ "descricao": "X", "armazem_id": 1, "serie_prefixo": "A/B" })).await.contains("prefixo"));
    rpc(&st, "posto.selecionar", json!({ "id": 1 })).await;

    // ---- utilizadores: PIN de supervisor e permissões ----
    rpc(&st, "utilizadores.guardar", json!({ "id": 1, "nome": "Admin", "login": "admin", "perfil": "ADMIN", "pin": "4321" })).await;
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "id": 1, "nome": "Admin", "login": "admin", "perfil": "ADMIN", "pin": "12a" })).await.contains("PIN"));
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await;
    rpc(
        &st,
        "utilizadores.guardar",
        json!({ "nome": "Operador 2", "login": "caixa2", "perfil": "CAIXA", "senha": "caixa234", "mudar_senha": false, "limite_desconto": 5,
                "permissoes": ["vendas.fatura_recibo", "pos.desconto", "pos.suspenso", "clientes"] }),
    )
    .await;
    let users = rpc(&st, "utilizadores.listar", Value::Null).await;
    let admin = users.as_array().unwrap().iter().find(|u| u["login"] == "admin").unwrap();
    assert_eq!(admin["tem_pin"], true);

    // ---- perfil CAIXA: não pode criar artigos, anular, nem alterar preço sem PIN ----
    let sessao = rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
    assert!(sessao["permissoes"].as_array().unwrap().iter().any(|p| p == "pos.suspenso"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Y", "taxa_iva_id": 1, "preco1": 1 })).await.contains("permissão"));
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": fr1_id, "motivo": "teste teste" })).await.contains("permissão"));
    let venda_preco = |pin: Option<&str>| {
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1, "preco_unitario": 90000 }],
                "pagamentos": [{ "modo": "NU", "valor": 200000 }], "autorizacao_pin": pin })
    };
    assert!(rpc_err(&st, "documentos.emitir", venda_preco(None)).await.contains("supervisor"));
    assert!(rpc_err(&st, "documentos.emitir", venda_preco(Some("0000"))).await.contains("inválido"));
    let autorizada = rpc(&st, "documentos.emitir", venda_preco(Some("4321"))).await;
    assert_eq!(autorizada["documento"]["autorizado_por_nome"], "Admin");
    assert_eq!(autorizada["documento"]["total"], 102600); // 900,00 + 14%
    assert_eq!(rpc(&st, "auth.autorizar", json!({ "pin": "4321", "permissao": "pos.desconto" })).await["nome"], "Admin");

    // ---- limite de desconto (5%) e desconto global em valor ----
    rpc(&st, "auth.login", json!({ "login": "caixa2", "senha": "caixa234" })).await;
    let com_desc = |pct: f64| {
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1, "desconto_pct": pct }], "pagamentos": [{ "modo": "NU", "valor": 200000 }] })
    };
    assert!(rpc_err(&st, "documentos.emitir", com_desc(10.0)).await.contains("excede"));
    let ok5 = rpc(&st, "documentos.emitir", com_desc(5.0)).await;
    assert_eq!(ok5["documento"]["total_liquido"], 95000);
    let global = |pin: Option<&str>| {
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "desconto_global_valor": 10000,
                "pagamentos": [{ "modo": "NU", "valor": 200000 }], "autorizacao_pin": pin })
    };
    assert!(rpc_err(&st, "documentos.emitir", global(None)).await.contains("excede"));
    let g = rpc(&st, "documentos.emitir", global(Some("4321"))).await;
    assert_eq!(g["documento"]["total_desconto"], 10000);
    assert_eq!(g["documento"]["total_liquido"], 90000);
    assert_eq!(g["documento"]["total"], 102600);

    // ---- documentos suspensos ----
    let susp = rpc(
        &st,
        "suspensos.guardar",
        json!({ "entidade_id": 1, "entidade_nome": "Consumidor Final", "descricao": "Mesa 4", "total": 114000,
                "dados": { "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] } }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert_eq!(rpc(&st, "suspensos.listar", Value::Null).await.as_array().unwrap().len(), 1);
    let aberto = rpc(&st, "suspensos.abrir", json!({ "id": susp })).await;
    assert_eq!(aberto["dados"]["linhas"][0]["artigo_id"], artigo);
    assert_eq!(rpc(&st, "suspensos.listar", Value::Null).await.as_array().unwrap().len(), 0);
    assert!(rpc_err(&st, "suspensos.abrir", json!({ "id": susp })).await.contains("não encontrado"));
    let ids = rpc(&st, "artigos.porIds", json!({ "ids": [artigo] })).await;
    assert_eq!(ids.as_array().unwrap().len(), 1);

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

// =====================================================================================
// Paridade com o KIGEST GC (compras, retenções, inventário, tesouraria) e importação
// =====================================================================================

use super::importar::{analisar_fonte, importar_dados, Memoria, Opcoes, Registo, Val};

fn reg(campos: &[(&str, Val)]) -> Registo {
    campos.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect()
}
fn s(x: &str) -> Val {
    Val::S(x.into())
}

#[tokio::test]
async fn gc_paridade_e_importacao() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-gc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Teste", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let ano = &db::hoje()[..4];

    let artigo = rpc(&st, "artigos.guardar", json!({ "descricao": "Cimento 50kg", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Fornecedor SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Construtora Lda", "nif": "5417000000" })).await["id"].as_i64().unwrap();

    // ---- compras: V/Factura exige o n.º do documento do fornecedor ----
    let vft = |ext: Option<&str>| json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": ext, "linhas": [{ "artigo_id": artigo, "quantidade": 10, "preco_unitario": 60000 }] });
    assert!(rpc_err(&st, "documentos.emitir", vft(None)).await.contains("fornecedor"));
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": cli, "doc_externo": "X", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await.contains("não é um fornecedor"));
    let fc = rpc(&st, "documentos.emitir", vft(Some("FT 2026/88"))).await;
    let fc_id = fc["documento"]["id"].as_i64().unwrap();
    assert_eq!(fc["documento"]["valor_pendente"], 684000);
    assert_eq!(fc["documento"]["doc_externo"], "FT 2026/88");
    assert_eq!(stock(&st, artigo).await, 10.0);
    let pend = rpc(&st, "contas.pendentes", json!({ "tipo": "F" })).await;
    assert_eq!(pend["documentos"].as_array().unwrap().len(), 1);

    // ---- pagamento a fornecedor (parcial, por transferência → conta Banco) ----
    let vrc = rpc(&st, "documentos.emitir", json!({ "tipo": "VRC", "entidade_id": forn, "liquidar": [{ "documento_id": fc_id, "valor": 300000 }], "pagamentos": [{ "modo": "TB", "valor": 300000 }] })).await;
    assert_eq!(vrc["documento"]["total"], 300000);
    // ---- V/Nota de crédito: devolve 2 unidades ao fornecedor ----
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "VNC", "entidade_id": forn, "doc_externo": "NC 1", "doc_origem_id": fc_id, "linhas": [{ "artigo_id": artigo, "quantidade": 2, "preco_unitario": 60000 }] })).await.contains("motivo"));
    rpc(&st, "documentos.emitir", json!({ "tipo": "VNC", "entidade_id": forn, "doc_externo": "NC 1", "motivo": "Devolução de material danificado", "doc_origem_id": fc_id, "linhas": [{ "artigo_id": artigo, "quantidade": 2, "preco_unitario": 60000 }] })).await;
    assert_eq!(stock(&st, artigo).await, 8.0);
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": fc_id })).await["documento"]["valor_pendente"], 684000 - 300000 - 136800);

    // ---- venda com retenção na fonte (Imposto Industrial 6,5%) ----
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "prazo": "30", "referencia": "Obra 12", "retencoes": [{ "codigo": "II" }], "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    assert_eq!(ft["documento"]["total"], 114000);
    assert_eq!(ft["documento"]["total_retencao"], 6500);
    assert_eq!(ft["documento"]["valor_pendente"], 107500);
    assert_eq!(ft["documento"]["prazo_desc"], "30 dias");
    assert_eq!(ft["retencoes"][0]["codigo"], "II");
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": ft["documento"]["id"], "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await.contains("motivo"));
    assert_eq!(stock(&st, artigo).await, 7.0);

    // ---- inventário (acerta para a quantidade contada) e consumo interno ----
    rpc(&st, "documentos.emitir", json!({ "tipo": "IN", "linhas": [{ "artigo_id": artigo, "quantidade": 5 }] })).await;
    assert_eq!(stock(&st, artigo).await, 5.0);
    rpc(&st, "documentos.emitir", json!({ "tipo": "CI", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    assert_eq!(stock(&st, artigo).await, 4.0);

    // ---- nota de encomenda facturada; guia de transporte ----
    let ne = rpc(&st, "documentos.emitir", json!({ "tipo": "NE", "entidade_id": cli, "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    let ne_id = ne["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "doc_origem_id": ne_id, "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ne_id })).await["documento"]["estado"], "F");
    let gt = rpc(&st, "documentos.emitir", json!({ "tipo": "GT", "entidade_id": cli, "transporte": { "carga_local": "Armazém Viana", "descarga_local": "Obra Talatona", "matricula": "LD-12-34-AB", "motorista": "João" }, "linhas": [{ "artigo_id": artigo, "quantidade": 1 }] })).await;
    assert_eq!(gt["documento"]["transporte"]["matricula"], "LD-12-34-AB");

    // ---- tesouraria ----
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let centros = rpc(&st, "tesouraria.centros", Value::Null).await;
    let combustivel = centros.as_array().unwrap().iter().find(|c| c["descricao"] == "Combustível").unwrap()["id"].as_i64().unwrap();
    rpc(&st, "tesouraria.movimento", json!({ "tipo": "SAIDA", "conta_id": 1, "valor": 20000, "descricao": "Gasóleo gerador", "centro_custo_id": combustivel })).await;
    let tr = rpc(&st, "tesouraria.transferencia", json!({ "de_conta": 1, "para_conta": 2, "valor": 50000 })).await["id"].as_i64().unwrap();
    let saldo = |v: &Value, id: i64| v.as_array().unwrap().iter().find(|c| c["id"] == id).unwrap()["saldo"].as_i64().unwrap();
    let contas = rpc(&st, "tesouraria.contas", Value::Null).await;
    assert_eq!(saldo(&contas, 1), 114000 - 20000 - 50000);
    assert_eq!(saldo(&contas, 2), -300000 + 50000);
    rpc(&st, "tesouraria.anular", json!({ "id": tr, "motivo": "lançado por engano" })).await;
    let contas = rpc(&st, "tesouraria.contas", Value::Null).await;
    assert_eq!(saldo(&contas, 1), 114000 - 20000);
    assert_eq!(saldo(&contas, 2), -300000);
    let fluxo = rpc(&st, "tesouraria.fluxo", json!({ "de": "2000-01-01", "ate": "2100-01-01" })).await;
    assert_eq!(fluxo["entradas"], 114000);
    assert_eq!(fluxo["saidas"], 300000 + 20000);
    assert_eq!(fluxo["a_receber"], 107500 + 114000);
    let pc = rpc(&st, "tesouraria.porCentro", json!({ "de": "2000-01-01", "ate": "2100-01-01" })).await;
    assert!(pc["centros"].as_array().unwrap().iter().any(|c| c["rubrica"] == "Combustível"));
    let ext = rpc(&st, "contas.extrato", json!({ "entidade_id": cli })).await;
    assert_eq!(ext["saldo"], 107500 + 114000);

    // ---- SAF-T com os novos tipos ----
    let saft = rpc(&st, "saft.exportar", json!({ "de": "2000-01-01", "ate": "2100-12-31" })).await;
    validar_xsd(saft["ficheiro"].as_str().unwrap());
    let xml = std::fs::read_to_string(saft["ficheiro"].as_str().unwrap()).unwrap();
    assert!(xml.contains("<WorkType>NE</WorkType>"));
    let guias = &xml[xml.find("<MovementOfGoods>").unwrap()..xml.find("</MovementOfGoods>").unwrap()];
    assert!(!guias.contains("TaxPointDate"), "as linhas das guias não levam TaxPointDate");
    assert!(xml.contains("<MovementType>GT</MovementType>"));
    assert!(xml.contains("<WithholdingTaxType>II</WithholdingTaxType>"));
    assert!(!xml.contains("FT 2026/88"), "documentos de compra não entram no SAF-T de facturação");

    // ---- importação de um KIGEST (tabelas e colunas reais) ----
    let mut t: std::collections::HashMap<String, Vec<Registo>> = std::collections::HashMap::new();
    t.insert("tbzconfigCompany".into(), vec![reg(&[("NomeCompany", s("Loja KIGEST")), ("NIF", s("5001112223")), ("Endereco", s("Rua 1")), ("NumCertificado", s("123/AGT/2019"))])]);
    t.insert("tbzconfigTaxasIva".into(), vec![reg(&[("Id", Val::I(1)), ("Taxa", Val::F(14.0)), ("Descricao", s("IVA 14%"))]), reg(&[("Id", Val::I(2)), ("Taxa", Val::F(0.0)), ("Descricao", s("Isento"))])]);
    t.insert("tbArtigosFamilia".into(), vec![reg(&[("IdFamilia", Val::I(1)), ("Descricao", s("Mercadorias"))]), reg(&[("IdFamilia", Val::I(2)), ("Descricao", s("Serviços"))])]);
    t.insert("tbArtigosSubFamilia".into(), vec![reg(&[("Id", Val::I(1)), ("IdFamilia", Val::I(1)), ("Descricao", s("Bebidas"))])]);
    t.insert("tbArtigosUnidade".into(), vec![reg(&[("IdUnidade", Val::I(1)), ("Descricao", s("Unidade")), ("Abreviatura", s("UN"))])]);
    t.insert("tbzconfigArmazem".into(), vec![reg(&[("IdArmazem", Val::I(1)), ("NomeArmazem", s("Principal"))]), reg(&[("IdArmazem", Val::I(2)), ("NomeArmazem", s("Loja 2"))])]);
    t.insert(
        "tbArtigos".into(),
        vec![
            reg(&[("IdArtigo", Val::I(10)), ("Referencia", s("COCA")), ("Descricao", s("Coca-Cola 33cl")), ("CoBar", s("5449000000996")), ("IdFamilia", Val::I(1)), ("IdSubFamilia", Val::I(1)), ("IdUnidade", Val::I(1)), ("IdTaxa", Val::I(1)), ("Custo", Val::F(150.0)), ("Preco1", Val::F(250.0))]),
            reg(&[("IdArtigo", Val::I(11)), ("Referencia", s("SERV1")), ("Descricao", s("Montagem")), ("IdFamilia", Val::I(2)), ("IdTaxa", Val::I(2)), ("CodigoIsencao", s("M10")), ("Preco1", Val::F(1000.0))]),
        ],
    );
    t.insert(
        "tbArtigosStock".into(),
        vec![
            reg(&[("IdArtigo", Val::I(10)), ("IdArmazem", Val::I(1)), ("QtdExistente", Val::F(24.0)), ("CustoMedio", Val::F(150.0))]),
            reg(&[("IdArtigo", Val::I(10)), ("IdArmazem", Val::I(2)), ("QtdExistente", Val::F(6.0))]),
        ],
    );
    t.insert("tbClientes".into(), vec![reg(&[("IdCliente", Val::I(1)), ("Nome", s("Consumidor Final")), ("NIF", s("999999999"))]), reg(&[("IdCliente", Val::I(2)), ("Nome", s("Empresa X")), ("NIF", s("5000000001")), ("Endereco", s("Luanda"))])]);
    t.insert("tbFornecedores".into(), vec![reg(&[("IdFornecedor", Val::I(1)), ("Nome", s("Distribuidora Y")), ("NIF", s("5000000002"))])]);
    t.insert("tbUsuario".into(), vec![reg(&[("IdUsuario", Val::I(1)), ("Nome", s("Operador K")), ("Usuario", s("opk")), ("CodPin", s("1234"))])]);
    t.insert("tbzconfigTiposDocumentos".into(), vec![reg(&[("Id", Val::I(1)), ("Descricao", s("Fatura")), ("Abreviatura", s("FT"))]), reg(&[("Id", Val::I(2)), ("Descricao", s("Venda a Dinheiro")), ("Abreviatura", s("VD"))]), reg(&[("Id", Val::I(12)), ("Descricao", s("V/Fatura")), ("Abreviatura", s("FT"))])]);
    t.insert(
        "tbDocumentosCabessalho".into(),
        vec![
            reg(&[("RefDocumento", s("FT 2025/7")), ("IdTipoDocumento", Val::I(1)), ("DataEmissao", s("2025-12-01 00:00:00")), ("DataHoraEmissao", s("2025-12-01 10:00:00")), ("IdEntidade", Val::I(2)), ("Nome", s("Empresa X")), ("NIF", s("5000000001")), ("ValTotal", Val::F(570.0)), ("ValTaxa", Val::F(70.0)), ("ValLiquido", Val::F(500.0)), ("Hash", s("HASHORIGINAL")), ("AssinaturaCurta", s("HSIO"))]),
            reg(&[("RefDocumento", s("VD 1/4")), ("IdTipoDocumento", Val::I(2)), ("DataEmissao", s("2025-12-02 00:00:00")), ("IdEntidade", Val::I(1)), ("Nome", s("Consumidor Final")), ("ValTotal", Val::F(285.0)), ("ValTaxa", Val::F(35.0))]),
            reg(&[("RefDocumento", s("VFT 2025/3")), ("IdTipoDocumento", Val::I(12)), ("DataEmissao", s("2025-11-10 00:00:00")), ("IdEntidade", Val::I(1)), ("Nome", s("Distribuidora Y")), ("ValTotal", Val::F(1140.0)), ("ValTaxa", Val::F(140.0)), ("ValLiquido", Val::F(1000.0))]),
        ],
    );
    t.insert("tbDocumentosDetalhe".into(), vec![reg(&[("RefDocumento", s("FT 2025/7")), ("IdArtigo", Val::I(10)), ("Referencia", s("COCA")), ("ArtigoDescricao", s("Coca-Cola 33cl")), ("Qtd", Val::F(2.0)), ("ValorUnit", Val::F(250.0)), ("Taxaporcento", Val::F(14.0)), ("ValorLiquido", Val::F(500.0)), ("ValorTaxa", Val::F(70.0)), ("ValorTotal", Val::F(570.0))])]);
    t.insert("tbContasCorrente".into(), vec![reg(&[("Documento", s("FT 2025/7")), ("ValorFalta", Val::F(570.0))])]);
    t.insert("tbDocsModoPagamentos".into(), vec![reg(&[("RefDocumento", s("VD 1/4")), ("IdModopagamento", Val::I(1)), ("Valor", Val::F(285.0))])]);
    // RH, lotes e oficina
    t.insert("tbZconfigRhDepartamento".into(), vec![reg(&[("Id", Val::I(1)), ("Descritivo", s("Vendas"))])]);
    t.insert("tbZconfigRhFuncao".into(), vec![reg(&[("Id", Val::I(3)), ("Descritivo", s("Caixa"))])]);
    t.insert("tbZconfigRhCategoria".into(), vec![reg(&[("Id", Val::I(2)), ("Descritivo", s("Técnico Médio"))])]);
    t.insert(
        "tbRhFuncionarios".into(),
        vec![
            reg(&[("Id", s("F001")), ("Nome", s("Maria Kiala")), ("Id_Departamento", Val::I(1)), ("Id_Funcao", Val::I(3)), ("Id_categoria", Val::I(2)), ("Salario_Base", Val::F(150000.0)),
                  ("Subsidio_Alimentacao", Val::F(20000.0)), ("Desconto_INSS", s("S")), ("Desconto_IRT", s("S")), ("Contrato", s("Tempo indeterminado")),
                  ("Inicio_Funcoes", s("2022-03-01 00:00:00")), ("Func_pagto_IBAN", s("AO06000600000100037131174")), ("NumeroINSS", s("123456"))]),
            reg(&[("Id", s("F002")), ("Nome", s("João Diarista")), ("TipoProcessamento", s("Trab. por conta própria")), ("Termino_Funcoes", s("2024-01-31 00:00:00"))]),
        ],
    );
    t.insert("tbRhAgregadoFamiliar".into(), vec![reg(&[("IdFuncionario", s("F001")), ("NomeAgregado", s("Pedro Kiala")), ("DataNascimento", s("2015-05-01 00:00:00"))])]);
    t.insert(
        "tbRhMapaFerias".into(),
        vec![
            reg(&[("IdFuncionario", s("F001")), ("Ano", Val::I(2025)), ("Mes", s("Agosto")), ("DataInicial", s("2025-08-01 00:00:00")), ("DataFinal", s("2025-08-22 00:00:00")), ("Dias", Val::I(22))]),
            reg(&[("IdFuncionario", s("F001")), ("Ano", Val::I(2025)), ("Mes", s("Setembro")), ("DataInicial", s("2025-08-01 00:00:00")), ("DataFinal", s("2025-08-22 00:00:00")), ("Dias", Val::I(22))]),
        ],
    );
    t.insert(
        "tbArtigosLotes".into(),
        vec![
            reg(&[("IdArtigo", Val::I(10)), ("IdArmazem", Val::I(1)), ("Lote", s("L1")), ("Qtd", Val::F(20.0)), ("DataValidade", s("2027-01-31 00:00:00")), ("PrecoCusto", Val::F(150.0)), ("DiasAviso", Val::I(45))]),
            reg(&[("IdArtigo", Val::I(10)), ("IdArmazem", Val::I(1)), ("Lote", s("L2")), ("Qtd", Val::F(4.0)), ("DataValidade", s("2026-12-31 00:00:00"))]),
            reg(&[("IdArtigo", Val::I(10)), ("IdArmazem", Val::I(2)), ("Lote", s("L3")), ("Qtd", Val::F(6.0))]),
            reg(&[("IdArtigo", Val::I(99)), ("Lote", s("SEM-ARTIGO")), ("Qtd", Val::F(1.0))]),
        ],
    );
    t.insert(
        "vsOficinaFichaAutomovel".into(),
        vec![reg(&[("IdViatura", Val::I(7)), ("IdCliente", Val::I(2)), ("Marca", s("Toyota")), ("Modelo", s("Hilux")), ("Matricula", s("LD-12-34-AB")), ("NumeroChassi", s("JT123")),
                   ("DescritivoCombustivel", s("Gasóleo")), ("KilometragenIncial", Val::I(120000))])],
    );
    t.insert(
        "vsOrdemServicoCabessalho".into(),
        vec![
            reg(&[("IdOrdemServico", Val::I(1)), ("RefDocumento", s("OS 2025/1")), ("DataEmissao", s("2025-11-03 00:00:00")), ("IdViatura", Val::I(7)), ("IdCliente", Val::I(2)),
                  ("Diagnostico", s("Travões gastos")), ("Estado", s("Concluída")), ("DocVenvaAssociado", s("FT 2025/7")), ("KilometragenIncial", Val::I(120500)), ("NomeUsuario", s("Mecânico A"))]),
            reg(&[("IdOrdemServico", Val::I(2)), ("RefDocumento", s("OS 2025/2")), ("IdViatura", Val::I(99))]),
        ],
    );
    t.insert("vsOrdemServicoDetalhes".into(), vec![reg(&[("RefDocumento", s("OS 2025/1")), ("Descricao", s("Pastilhas de travão")), ("Qtd", Val::F(2.0))])]);
    // recibos, retenções, proformas, tesouraria, caixa, moedas, prazos e histórico salarial
    t.insert("vsSAFT_REC_Cabecalho".into(), vec![reg(&[("NumeroRecibo", s("RC 2025/3")), ("Data", s("2025-12-05 00:00:00")), ("Data_Hora_Emissao", s("2025-12-05 09:30:00")), ("EntidadeId", Val::I(2)), ("TipoCliente", s("Cliente")), ("Total", Val::F(200.0)), ("CanceladoStatus", Val::I(0))])]);
    t.insert("vsSAFT_ReciboLinha".into(), vec![reg(&[("DocumentoReferencia", s("RC 2025/3")), ("DocumentoOrigem", s("FT 2025/7")), ("DataDocsOrigem", s("2025-12-01 00:00:00")), ("ValorLiquido", Val::F(200.0))])]);
    t.insert("vsTotalporPagtoRecibos".into(), vec![reg(&[("RefDocumento", s("RC 2025/3")), ("ModoPagto", s("Multicaixa")), ("Valor", Val::F(200.0))])]);
    t.insert("tbDocsRetencaoFonte".into(), vec![reg(&[("RefDocumento", s("FT 2025/7")), ("Codigo", s("II")), ("Percentagem", Val::F(6.5)), ("Valor", Val::F(32.5)), ("Motivo", s("Imposto Industrial"))])]);
    t.insert("tbzconfigTiposCustosMercadorias".into(), vec![reg(&[("Codigo", s("01")), ("Designacao", s("Frete marítimo"))]), reg(&[("Codigo", s("02")), ("Designacao", s("Despachante"))])]);
    t.insert(
        "tbDocsDetalhesCustoMecadorias".into(),
        vec![
            reg(&[("RefDocumento", s("VFT 2025/3")), ("Cod_TipoCusto", s("01")), ("Valor", Val::F(100.0)), ("Id_Moeda", Val::I(2)), ("Contravalor", Val::F(95050.0))]),
            reg(&[("RefDocumento", s("VFT 2025/3")), ("Cod_TipoCusto", s("02")), ("Valor", Val::F(15000.0)), ("Id_Moeda", Val::I(1))]),
            reg(&[("RefDocumento", s("VFT 1999/1")), ("Cod_TipoCusto", s("01")), ("Valor", Val::F(10.0))]),
        ],
    );
    t.insert("vsDocsProformaCabessalho".into(), vec![reg(&[("RefDocumento", s("PP 2025/2")), ("DataEmissao", s("2025-11-20 00:00:00")), ("IdCliente", Val::I(2)), ("Validade", s("2025-12-20 00:00:00"))])]);
    t.insert("vsDocsProformaDetalhes".into(), vec![reg(&[("RefDocumento", s("PP 2025/2")), ("IdArtigo", Val::I(10)), ("Referencia", s("COCA")), ("ArtigoDescricao", s("Coca-Cola 33cl")), ("Qtd", Val::F(10.0)), ("ValorUnit", Val::F(250.0)), ("Taxaporcento", Val::F(14.0)), ("ValorLiquido", Val::F(2500.0)), ("ValorTaxa", Val::F(350.0)), ("ValorTotal", Val::F(2850.0))])]);
    t.insert("tbzconfigCentroCusto".into(), vec![reg(&[("Id", Val::I(4)), ("Descritivo", s("Renda"))]), reg(&[("Id", Val::I(9)), ("Descritivo", s("Segurança"))])]);
    t.insert("tbFluxoCaixaContas".into(), vec![reg(&[("ID", Val::I(1)), ("Descricao", s("Caixa Principal")), ("Tipo", s("Caixa"))]), reg(&[("ID", Val::I(2)), ("Descricao", s("BAI Empresa")), ("Tipo", s("Banco")), ("Conta_Iban", s("AO06004000001234567890123"))])]);
    t.insert(
        "tbFlulxoaixaCabessalho".into(),
        vec![
            reg(&[("Id", Val::I(1)), ("IdConta", Val::I(2)), ("Data", s("2024-06-01 00:00:00")), ("ValorCredito", Val::F(100000.0)), ("ValorDebito", Val::F(0.0)), ("Desc_Geral", s("Depósito inicial"))]),
            reg(&[("Id", Val::I(2)), ("IdConta", Val::I(2)), ("Data", s("2024-06-05 00:00:00")), ("ValorCredito", Val::F(0.0)), ("ValorDebito", Val::F(30000.0)), ("Desc_Geral", s("Renda de Junho")), ("Id_CentroCusto", Val::I(4))]),
            reg(&[("Id", Val::I(3)), ("IdConta", Val::I(77)), ("Data", s("2024-06-06 00:00:00")), ("ValorCredito", Val::F(5.0))]),
        ],
    );
    t.insert("tbCaixaDiaria".into(), vec![reg(&[("Referencia", s("CX-2025-1")), ("DataAbertura", s("2025-12-02 08:00:00")), ("DataFecho", s("2025-12-02 18:00:00")), ("Saldo_Inicial", Val::F(5000.0)), ("SaldoFinal", Val::F(5285.0)), ("IdUsusario", Val::I(1))])]);
    t.insert("tbzconfigMoeda".into(), vec![reg(&[("Id", Val::I(1)), ("Descricao", s("Kwanza")), ("Simbolo", s("Kz")), ("Cambio", Val::F(1.0))]), reg(&[("Id", Val::I(2)), ("Descricao", s("Dólar")), ("Simbolo", s("USD")), ("Cambio", Val::F(950.5))])]);
    t.insert("tbzconfigTiposPagamento".into(), vec![reg(&[("IdPagto", Val::I(1)), ("Descricao", s("30 dias")), ("Dias", Val::I(30))]), reg(&[("IdPagto", Val::I(2)), ("Descricao", s("120 dias")), ("Dias", Val::I(120))])]);
    t.insert("tbRhProcessamentoCabessalho".into(), vec![reg(&[("IdFuncionario", s("F001")), ("Ano", Val::I(2025)), ("Mes", Val::I(12)), ("MesExtenso", s("Dezembro")), ("Nome", s("Maria Kiala")), ("SalarioBase", Val::F(150000.0)), ("Iliquido", Val::F(170000.0)), ("BaseTributavelINSS", Val::F(170000.0)), ("ContribuicaoINSS", Val::F(5100.0)), ("BaseTributavelIRT", Val::F(154900.0)), ("IRT", Val::F(4900.0)), ("Liquido", Val::F(160000.0))])]);
    t.insert(
        "tbRhProcessamentoRecibo".into(),
        vec![
            reg(&[("IdFuncionario", s("F001")), ("Ano", Val::I(2025)), ("Mes", Val::I(12)), ("CodDescritivo", s("001")), ("Descritivo", s("Salário base")), ("Valor", Val::F(150000.0))]),
            reg(&[("IdFuncionario", s("F001")), ("Ano", Val::I(2025)), ("Mes", Val::I(12)), ("CodDescritivo", s("002")), ("Descritivo", s("Subsídio de alimentação")), ("Valor", Val::F(20000.0))]),
        ],
    );
    t.insert("vsRhProcesamentoHonorarios".into(), vec![reg(&[("IdDiarista", s("F002")), ("Ano", Val::I(2025)), ("Mes", Val::I(11)), ("ValorIliquido", Val::F(50000.0)), ("IRTPorcento", Val::F(6.5)), ("IRT", Val::F(3250.0)), ("TotalLiquido", Val::F(46750.0)), ("TipoProcessamento", s("Mensal"))])]);
    t.insert(
        "tbRhAlteracoesMensais".into(),
        vec![
            reg(&[("IdFuncionario", s("F001")), ("AnoProcessamento", Val::I(2026)), ("MesProcessamento", Val::I(1)), ("CodDescritivo", s("006")), ("Quantidade", Val::F(4.0)), ("StatusProcessamento", s("Não processado"))]),
            reg(&[("IdFuncionario", s("F001")), ("AnoProcessamento", Val::I(2025)), ("MesProcessamento", Val::I(12)), ("CodDescritivo", s("006")), ("Quantidade", Val::F(2.0)), ("StatusProcessamento", s("Processado"))]),
            reg(&[("IdFuncionario", s("F001")), ("AnoProcessamento", Val::I(2026)), ("MesProcessamento", Val::I(1)), ("CodDescritivo", s("999")), ("Valor", Val::F(1.0)), ("StatusProcessamento", s("Pendente"))]),
        ],
    );
    t.insert("tbRhAdiantamentoSalario".into(), vec![reg(&[("IdFuncionario", s("F001")), ("AnoProcessamento", Val::I(2026)), ("MesProcessamento", Val::I(1)), ("Parcela", Val::I(1)), ("ValorParcela", Val::F(10000.0)), ("StatusProcessamento", s("Pendente"))])]);
    t.insert("tbTabelaSemUso".into(), vec![reg(&[("Campo", s("x"))])]);
    let opcoes = Opcoes { empresa: true, artigos: true, stock: true, clientes: true, fornecedores: true, utilizadores: true, documentos: true, rh: true, lotes: true, oficina: true, tesouraria: true, simular: false };
    let mut fonte = Memoria(t);
    let analise = analisar_fonte(&mut fonte).await.unwrap();
    assert!(analise["nao_importadas"].as_array().unwrap().iter().any(|x| x["tabela"] == "tbTabelaSemUso" && x["colunas"][0] == "campo"), "{analise}");
    assert!(analise["reconhecidas"].as_array().unwrap().iter().any(|x| x["conjunto"] == "recibos" && x["tabela"] == "vsSAFT_REC_Cabecalho"));
    // simulação: mostra o que seria importado e não grava nada
    let ensaio = importar_dados(&st, &mut fonte, &Opcoes { simular: true, ..opcoes.clone() }, "teste/KigestGC").await.expect("simulação");
    assert_eq!(ensaio["simulacao"], true);
    assert!(ensaio["contagens"].as_array().unwrap().iter().any(|c| c["conjunto"] == "Artigos" && c["registos"] == 2));
    assert!(ensaio["senhas"].as_array().unwrap().is_empty());
    assert_eq!(rpc(&st, "artigos.listar", json!({ "q": "Coca" })).await.as_array().unwrap().len(), 0, "a simulação não grava");
    assert!(rpc(&st, "rh.funcionarios", json!({ "q": "", "inativos": true })).await.as_array().unwrap().is_empty());
    let n_imp: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM importacoes").fetch_one(&st.pool).await.unwrap();
    assert_eq!(n_imp, 0);
    let r = importar_dados(&st, &mut fonte, &opcoes, "teste/KigestGC").await.expect("importação");
    assert!(r["senhas"].as_array().unwrap().iter().any(|x| x["login"] == "opk"));
    // conferência KIGEST × KanGest
    let conf = |nome: &str| r["conferencia"].as_array().unwrap().iter().find(|c| c["item"] == nome).cloned().unwrap_or_else(|| panic!("{nome}: {}", r["conferencia"]));
    assert_eq!((conf("Documentos")["kigest"].as_i64(), conf("Documentos")["kigest_novo"].as_i64()), (Some(3), Some(3)));
    assert_eq!((conf("Valores em dívida (conta corrente)")["kigest"].as_i64(), conf("Valores em dívida (conta corrente)")["kigest_novo"].as_i64()), (Some(57000), Some(57000)));
    let saldo = conf("Saldo das contas de tesouraria");
    assert_eq!((saldo["kigest"].as_i64(), saldo["kigest_novo"].as_i64()), (Some(7000500), Some(7000000)), "o movimento sem conta fica de fora e a diferença aparece");
    assert_eq!(conf("Funcionários")["kigest"], conf("Funcionários")["kigest_novo"]);
    let emp = rpc(&st, "empresa.obter", Value::Null).await;
    assert_eq!(emp["nome"], "Loja KIGEST");
    let agt_info = rpc(&st, "agt.info", Value::Null).await;
    assert_eq!(agt_info["certificado_anterior"], "123/AGT/2019");
    // o certificado do KIGEST não é activado: fica o desta versão (vazio numa compilação de desenvolvimento)
    assert_eq!(agt_info["certificado"], crate::produtor::certificado().unwrap_or_default(), "o certificado do KIGEST não é activado automaticamente");
    let coca = rpc(&st, "artigos.codigo", json!({ "codigo": "5449000000996" })).await;
    assert_eq!(coca["preco1"], 25000);
    assert_eq!(coca["stock"].as_f64().unwrap(), 24.0);
    let loja2 = rpc(&st, "tabelas", Value::Null).await["armazens"].as_array().unwrap().iter().find(|a| a["nome"] == "Loja 2").unwrap()["id"].as_i64().unwrap();
    let s2 = rpc(&st, "stock.listar", json!({ "armazem_id": loja2, "q": "Coca" })).await;
    assert_eq!(s2[0]["quantidade"].as_f64().unwrap(), 6.0);
    let serv = rpc(&st, "artigos.codigo", json!({ "codigo": "SERV1" })).await;
    assert_eq!(serv["tipo"], "S");
    assert_eq!(serv["motivo_isencao"], "M10");
    let x = rpc(&st, "entidades.listar", json!({ "tipo": "C", "q": "Empresa X" })).await[0].clone();
    assert_eq!(x["saldo"], 57000, "o valor em dívida do KIGEST passa para a conta corrente");
    let hist = rpc(&st, "documentos.listar", json!({ "q": "FT 2025/7" })).await[0].clone();
    assert_eq!(hist["hash_controlo"], "HSIO");
    // recibo no sistema novo sobre a factura antiga
    let rc = rpc(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": x["id"], "liquidar": [{ "documento_id": hist["id"], "valor": 57000 }], "pagamentos": [{ "modo": "NU", "valor": 57000 }] })).await;
    assert!(rc["documento"]["numero_doc"].as_str().unwrap().starts_with(&format!("RC N{ano}/")), "séries novas com prefixo N");
    // os pagamentos históricos não mexem no fluxo de caixa
    let fluxo2 = rpc(&st, "tesouraria.fluxo", json!({ "de": "2025-01-01", "ate": "2025-12-31" })).await;
    assert_eq!(fluxo2["entradas"], 0);
    // documentos importados não se anulam no sistema novo, mas aceitam nota de crédito
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": hist["id"], "motivo": "teste de anulação" })).await.contains("importados"));
    let nc_hist = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "NC", "entidade_id": x["id"], "doc_origem_id": hist["id"], "motivo": "Devolução de mercadoria vendida no KIGEST",
                "linhas": [{ "artigo_id": coca["id"], "quantidade": 1, "preco_unitario": 25000, "lote": "L1" }] }),
    )
    .await;
    assert_eq!(nc_hist["documento"]["doc_origem"], "FT 2025/7");
    // ---- RH, lotes e oficina importados ----
    let funcs = rpc(&st, "rh.funcionarios", json!({ "q": "", "inativos": true })).await;
    let maria_id = funcs.as_array().unwrap().iter().find(|f| f["nome"] == "Maria Kiala").unwrap()["id"].as_i64().unwrap();
    let maria = rpc(&st, "rh.funcionario", json!({ "id": maria_id })).await;
    assert_eq!(maria["salario_base"], 15000000);
    assert_eq!(maria["sub_alimentacao"], 2000000);
    assert_eq!(maria["tipo_contrato"], "Indeterminado");
    assert_eq!(maria["data_admissao"], "2022-03-01");
    assert_eq!(maria["dependentes"], 1);
    assert_eq!(maria["ativo"], 1);
    let (dep, fun, cat): (String, String, String) = sqlx::query_as(
        "SELECT d.descricao, fu.descricao, c.descricao FROM funcionarios f JOIN rh_departamentos d ON d.id = f.departamento_id
         JOIN rh_funcoes fu ON fu.id = f.funcao_id JOIN rh_categorias c ON c.id = f.categoria_id WHERE f.id = ?",
    )
    .bind(maria_id)
    .fetch_one(&st.pool)
    .await
    .unwrap();
    assert_eq!((dep.as_str(), fun.as_str(), cat.as_str()), ("Vendas", "Caixa", "Técnico Médio"));
    assert_eq!(maria["agregado"][0]["nome"], "Pedro Kiala");
    let joao = funcs.as_array().unwrap().iter().find(|f| f["nome"] == "João Diarista").unwrap();
    let joao = rpc(&st, "rh.funcionario", json!({ "id": joao["id"] })).await;
    assert_eq!((joao["independente"].as_i64(), joao["ativo"].as_i64()), (Some(1), Some(0)), "conta própria e já saiu");
    let ferias = rpc(&st, "rh.ferias", json!({ "ano": 2025 })).await;
    assert_eq!(ferias.as_array().unwrap().len(), 1, "o mapa mensal do KIGEST dá um só período");
    assert_eq!(ferias[0]["dias"], 22);
    assert_eq!(ferias[0]["estado"], "GOZADA");
    let lotes = rpc(&st, "lotes.listar", json!({ "artigo_id": coca["id"], "com_stock": false })).await;
    assert_eq!(lotes.as_array().unwrap().len(), 3);
    assert_eq!(lotes[0]["lote"], "L2", "ordenados pela validade");
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": coca["id"] })).await["controla_lote"], 1, "lotes = stock em todos os armazéns");
    let rec = rpc(&st, "stock.recalcular", json!({})).await;
    assert_eq!(rec["lotes"].as_array().unwrap().len(), 0, "o saldo inicial dos lotes conta no recálculo");
    let viats = rpc(&st, "oficina.viaturas", json!({})).await;
    assert_eq!(viats.as_array().unwrap().len(), 1);
    assert_eq!(viats[0]["cliente"], "Empresa X");
    assert_eq!(viats[0]["km"], 120000);
    let ordens = rpc(&st, "oficina.ordens", json!({})).await;
    assert_eq!(ordens.as_array().unwrap().len(), 1, "a OS sem viatura não entra");
    let os = rpc(&st, "oficina.ordem", json!({ "id": ordens[0]["id"] })).await;
    assert_eq!(os["estado"], "FACTURADA");
    assert_eq!(os["documento_id"], hist["id"], "ligada à factura histórica");
    assert_eq!(os["linhas"][0]["descricao"], "Pastilhas de travão");
    assert!(r["avisos"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("ordem(ns) de serviço sem viatura")));

    // ---- recibos, retenções, proformas ----
    let rc_h = rpc(&st, "documentos.listar", json!({ "q": "RC 2025/3" })).await[0].clone();
    let rc_d = rpc(&st, "documentos.obter", json!({ "id": rc_h["id"] })).await;
    assert_eq!(rc_d["documento"]["total"], 20000);
    assert_eq!(rc_d["documento"]["importado"], 1);
    assert_eq!(rc_d["liquidacoes"][0]["numero_doc"], "FT 2025/7");
    assert_eq!(rc_d["pagamentos"][0]["modo_codigo"], "MB");
    let ft_d = rpc(&st, "documentos.obter", json!({ "id": hist["id"] })).await;
    assert_eq!(ft_d["retencoes"][0]["codigo"], "II");
    assert_eq!(ft_d["documento"]["total_retencao"], 3250);
    // custos de importação da compra (em Kz)
    let vft = rpc(&st, "documentos.listar", json!({ "q": "VFT 2025/3" })).await[0].clone();
    assert_eq!(vft["tipo_codigo"], "FC");
    let vft_d = rpc(&st, "documentos.obter", json!({ "id": vft["id"] })).await;
    let custos: Vec<(String, i64)> = vft_d["custos"].as_array().unwrap().iter().map(|c| (c["tipo"].as_str().unwrap().to_string(), c["valor"].as_i64().unwrap())).collect();
    assert_eq!(custos, vec![("FRETE".to_string(), 9505000), ("DESPACHO".to_string(), 1500000)]);
    assert!(r["avisos"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("custo(s) adicional(is)")));
    let pp = rpc(&st, "documentos.listar", json!({ "q": "PP 2025/2" })).await[0].clone();
    let pp_d = rpc(&st, "documentos.obter", json!({ "id": pp["id"] })).await;
    assert_eq!((pp_d["documento"]["tipo_codigo"].as_str(), pp_d["documento"]["total"].as_i64()), (Some("PP"), Some(285000)));
    assert_eq!(pp_d["linhas"].as_array().unwrap().len(), 1);
    assert_eq!(pp_d["documento"]["data_vencimento"], "2025-12-20");
    // ---- tesouraria: o saldo da conta fica igual ao do KIGEST ----
    let contas = rpc(&st, "tesouraria.contas", Value::Null).await;
    let bai = contas.as_array().unwrap().iter().find(|c| c["descricao"] == "BAI Empresa").unwrap().clone();
    assert_eq!(bai["tipo"], "BANCO");
    assert_eq!(bai["saldo"], 7000000, "100 000 − 30 000");
    assert_eq!(contas.as_array().unwrap().iter().filter(|c| c["descricao"] == "Caixa Principal").count(), 1, "aproveita a conta existente");
    let renda: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movimentos_tesouraria m JOIN centros_custo c ON c.id = m.centro_custo_id WHERE c.descricao = 'Renda' AND m.id_origem IS NOT NULL").fetch_one(&st.pool).await.unwrap();
    assert_eq!(renda, 1);
    assert!(r["avisos"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("sem conta conhecida")));
    let fechos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM caixa_sessoes WHERE id_origem = 'CX-2025-1' AND estado = 'FECHADA' AND valor_contado = 528500").fetch_one(&st.pool).await.unwrap();
    assert_eq!(fechos, 1);
    assert_eq!(rpc(&st, "moedas.listar", Value::Null).await.as_array().unwrap().iter().find(|m| m["codigo"] == "USD").unwrap()["cambio"], 950.5);
    let prazos120: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prazos_pagamento WHERE dias = 120").fetch_one(&st.pool).await.unwrap();
    assert_eq!(prazos120, 1);
    // ---- histórico salarial ----
    let procs = rpc(&st, "rh.processamentos", Value::Null).await;
    let dez = procs.as_array().unwrap().iter().find(|p| p["ano"] == 2025 && p["mes"] == 12).unwrap().clone();
    assert_eq!(dez["total_liquido"], 16000000);
    let recs = rpc(&st, "rh.recibos", json!({ "id": dez["id"] })).await;
    assert_eq!(recs["recibos"][0]["irt"], 490000);
    assert_eq!(recs["recibos"][0]["linhas"].as_array().unwrap().len(), 2);
    let hon: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_honorarios WHERE importado = 1 AND liquido = 4675000 AND pago = 1").fetch_one(&st.pool).await.unwrap();
    assert_eq!(hon, 1);
    let alts = rpc(&st, "rh.alteracoes", json!({ "ano": 2026, "mes": 1 })).await;
    let alts = alts.as_array().unwrap();
    assert_eq!(alts.len(), 2, "horas extra por processar + parcela do adiantamento: {alts:?}");
    assert!(alts.iter().any(|a| a["rubrica"] == "102" && a["valor"] == 1000000));
    assert!(r["avisos"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("rubricas que não existem")));

    // reimportar não duplica
    let r2 = importar_dados(&st, &mut fonte, &opcoes, "teste/KigestGC").await.expect("reimportação");
    assert!(r2["senhas"].as_array().unwrap().is_empty());
    assert_eq!(rpc(&st, "documentos.listar", json!({ "q": "FT 2025/7" })).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "artigos.listar", json!({ "q": "Coca" })).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "rh.funcionarios", json!({ "q": "", "inativos": true })).await.as_array().unwrap().len(), 2);
    assert_eq!(rpc(&st, "rh.funcionario", json!({ "id": maria_id })).await["agregado"].as_array().unwrap().len(), 1);
    let agregados = rpc(&st, "rh.agregados", Value::Null).await;
    assert_eq!(agregados.as_array().unwrap().len(), 1);
    assert_eq!(agregados[0]["funcionario"], "Maria Kiala");
    assert_eq!(rpc(&st, "rh.ferias", json!({ "ano": 2025 })).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "oficina.viaturas", json!({})).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "oficina.ordens", json!({})).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "lotes.listar", json!({ "artigo_id": coca["id"], "com_stock": false })).await.as_array().unwrap().len(), 3);
    assert_eq!(rpc(&st, "documentos.listar", json!({ "q": "RC 2025/3" })).await.as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": hist["id"] })).await["retencoes"].as_array().unwrap().len(), 1);
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": vft["id"] })).await["custos"].as_array().unwrap().len(), 2);
    let n_mov: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movimentos_tesouraria WHERE id_origem IS NOT NULL").fetch_one(&st.pool).await.unwrap();
    assert_eq!(n_mov, 2);
    let n_fx: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM caixa_sessoes WHERE id_origem IS NOT NULL").fetch_one(&st.pool).await.unwrap();
    assert_eq!(n_fx, 1);
    assert_eq!(rpc(&st, "rh.alteracoes", json!({ "ano": 2026, "mes": 1 })).await.as_array().unwrap().len(), 2);
    let n_rec: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_recibos").fetch_one(&st.pool).await.unwrap();
    let n_hon: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_honorarios WHERE importado = 1").fetch_one(&st.pool).await.unwrap();
    assert_eq!((n_rec, n_hon), (1, 1));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn rh_oficina_lotes_moedas() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-rh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Oficina Teste", "nif": "5000000000", "endereco": "Rua 2", "localidade": "Luanda" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let hoje = db::hoje();
    let (ano, mes): (i64, i64) = (hoje[..4].parse().unwrap(), hoje[5..7].parse().unwrap());
    let dias = |n: i64| (chrono::Local::now().date_naive() + chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Transportes Lda", "nif": "5417000001" })).await["id"].as_i64().unwrap();
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Import Export SA", "nif": "5410000009", "pais": "PT" })).await["id"].as_i64().unwrap();

    // =========================== RH ===========================
    let tab = rpc(&st, "rh.tabelas", Value::Null).await;
    assert!(tab["rubricas"].as_array().unwrap().iter().any(|r| r["codigo"] == "002"));
    assert!(rpc_err(&st, "rh.funcionarioGuardar", json!({ "nome": " " })).await.contains("nome"));
    let func = rpc(
        &st,
        "rh.funcionarioGuardar",
        json!({ "nome": "Ana Domingos", "nif": "001234567LA041", "salario_base": 30000000, "sub_alimentacao": 4000000, "sub_transporte": 2000000,
                "desconta_inss": true, "desconta_irt": true, "data_admissao": "2020-01-06", "banco": "BAI", "iban": "AO06004000000000000000000",
                "agregado": [{ "nome": "Filho 1", "parentesco": "Filho" }] }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert_eq!(rpc(&st, "rh.funcionario", json!({ "id": func })).await["dependentes"], 1);
    rpc(&st, "rh.alteracaoGuardar", json!({ "funcionario_id": func, "ano": ano, "mes": mes, "rubrica": "101", "quantidade": 2 })).await;
    rpc(&st, "rh.adiantamento", json!({ "funcionario_id": func, "valor": 5000000, "parcelas": 2, "ano": ano, "mes": mes, "conta_id": 1 })).await;
    let sim = rpc(&st, "rh.simular", json!({ "ano": ano, "mes": mes })).await;
    let r = &sim[0];
    let liquido = r["liquido"].as_i64().unwrap();
    assert_eq!(r["total_remuneracoes"], 36000000);
    assert_eq!(liquido, r["total_remuneracoes"].as_i64().unwrap() - r["total_descontos"].as_i64().unwrap());
    assert!(r["irt"].as_i64().unwrap() > 0 && r["inss_trabalhador"].as_i64().unwrap() > 0);
    assert!(r["linhas"].as_array().unwrap().iter().any(|l| l["rubrica"] == "102" && l["valor"] == 2500000), "1.ª parcela do adiantamento");
    let proc = rpc(&st, "rh.processar", json!({ "ano": ano, "mes": mes })).await;
    let pid = proc["id"].as_i64().unwrap();
    assert_eq!(proc["total_liquido"], liquido);
    assert!(rpc_err(&st, "rh.processar", json!({ "ano": ano, "mes": mes })).await.contains("processado"));
    let recs = rpc(&st, "rh.recibos", json!({ "id": pid })).await;
    assert_eq!(recs["recibos"][0]["nome"], "Ana Domingos");
    let mapa = rpc(&st, "rh.mapa", json!({ "id": pid })).await;
    assert_eq!(mapa["linhas"][0]["excesso_subsidios"], 1000000, "alimentação acima de 30 000 Kz é tributável");
    let saldo1 = |c: &Value| c.as_array().unwrap().iter().find(|c| c["id"] == 1).unwrap()["saldo"].as_i64().unwrap();
    let antes = saldo1(&rpc(&st, "tesouraria.contas", Value::Null).await);
    assert_eq!(antes, -5000000, "o adiantamento sai da tesouraria");
    rpc(&st, "rh.pagar", json!({ "id": pid, "conta_id": 1 })).await;
    assert_eq!(saldo1(&rpc(&st, "tesouraria.contas", Value::Null).await), antes - liquido);
    assert!(rpc_err(&st, "rh.anular", json!({ "id": pid, "motivo": "erro nas faltas" })).await.contains("pagos"));
    let mov: i64 = sqlx::query_scalar("SELECT id FROM movimentos_tesouraria WHERE documento = ?").bind(format!("RH-{pid}")).fetch_one(&st.pool).await.unwrap();
    rpc(&st, "tesouraria.anular", json!({ "id": mov, "motivo": "pagamento lançado na conta errada" })).await;
    rpc(&st, "rh.anular", json!({ "id": pid, "motivo": "erro nas faltas" })).await;
    let proc2 = rpc(&st, "rh.processar", json!({ "ano": ano, "mes": mes })).await;
    assert_eq!(proc2["total_liquido"], liquido, "as alterações voltam a entrar no reprocessamento");
    rpc(&st, "rh.feriasGuardar", json!({ "funcionario_id": func, "data_inicio": format!("{ano}-12-01"), "data_fim": format!("{ano}-12-22"), "dias": 22 })).await;
    assert_eq!(rpc(&st, "rh.ferias", json!({ "ano": ano })).await.as_array().unwrap().len(), 1);

    // =========================== lotes (FEFO) ===========================
    let med = rpc(&st, "artigos.guardar", json!({ "descricao": "Paracetamol 500mg", "taxa_iva_id": 1, "preco1": 50000, "controla_lote": true })).await["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": med, "quantidade": 5, "preco_unitario": 20000 }] })).await.contains("lote"));
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": med, "quantidade": 5, "preco_unitario": 20000, "lote": "L2", "validade": dias(200) }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": med, "quantidade": 5, "preco_unitario": 20000, "lote": "L1", "validade": dias(10) }] })).await;
    assert_eq!(stock(&st, med).await, 10.0);
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": med, "quantidade": 6, "lote": "L1" }] })).await.contains("só tem"));
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": med, "quantidade": 7 }] })).await;
    let lote_txt = ft["linhas"][0]["lote"].as_str().unwrap().to_string();
    assert!(lote_txt.contains("L1") && lote_txt.contains("L2"), "FEFO: primeiro o lote que expira mais cedo ({lote_txt})");
    let lotes = rpc(&st, "lotes.listar", json!({ "artigo_id": med })).await;
    assert_eq!(lotes.as_array().unwrap().len(), 1);
    assert_eq!(lotes[0]["lote"], "L2");
    assert_eq!(lotes[0]["quantidade"].as_f64().unwrap(), 3.0);
    let aviso = rpc(&st, "lotes.listar", json!({ "dias": 30, "com_stock": false })).await;
    assert_eq!(aviso.as_array().unwrap().len(), 1, "aviso de validade: só o L1 expira em 30 dias");
    // devolução sem lote (como no POS): volta ao lote da factura de origem
    let nc = rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": ft["documento"]["id"], "motivo": "Devolução de 1 caixa",
        "linhas": [{ "artigo_id": med, "quantidade": 1, "preco_unitario": 50000 }] })).await;
    let l1 = |v: &Value| v.as_array().unwrap().iter().find(|l| l["lote"] == "L1").unwrap()["quantidade"].as_f64().unwrap();
    assert_eq!(l1(&rpc(&st, "lotes.listar", json!({ "artigo_id": med, "com_stock": false })).await), 1.0);
    assert_eq!(nc["linhas"][0]["lote"], "L1");
    rpc(&st, "documentos.anular", json!({ "id": nc["documento"]["id"], "motivo": "Nota de crédito de teste" })).await;
    assert_eq!(l1(&rpc(&st, "lotes.listar", json!({ "artigo_id": med, "com_stock": false })).await), 0.0);
    rpc(&st, "documentos.anular", json!({ "id": ft["documento"]["id"], "motivo": "Factura emitida por engano" })).await;
    let lotes = rpc(&st, "lotes.listar", json!({ "artigo_id": med })).await;
    assert_eq!(lotes.as_array().unwrap().iter().map(|l| l["quantidade"].as_f64().unwrap()).sum::<f64>(), 10.0, "a anulação repõe os lotes");
    // inventário de um só lote: acerta esse lote e o stock do artigo pela diferença
    rpc(&st, "documentos.emitir", json!({ "tipo": "IN", "linhas": [{ "artigo_id": med, "quantidade": 4, "lote": "L2" }] })).await;
    assert_eq!(stock(&st, med).await, 9.0);
    let lotes = rpc(&st, "lotes.listar", json!({ "artigo_id": med })).await;
    let q_lote = |n: &str| lotes.as_array().unwrap().iter().find(|l| l["lote"] == n).unwrap()["quantidade"].as_f64().unwrap();
    assert_eq!((q_lote("L1"), q_lote("L2")), (5.0, 4.0));

    // =========================== artigos compostos ===========================
    let pneu = rpc(&st, "artigos.guardar", json!({ "descricao": "Pneu 195/65", "taxa_iva_id": 1, "preco1": 4000000 })).await["id"].as_i64().unwrap();
    let valvula = rpc(&st, "artigos.guardar", json!({ "descricao": "Válvula", "taxa_iva_id": 1, "preco1": 50000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": pneu, "quantidade": 10, "preco_unitario": 2500000 }, { "artigo_id": valvula, "quantidade": 10, "preco_unitario": 10000 }] })).await;
    let kit = rpc(&st, "artigos.guardar", json!({ "descricao": "Kit 4 pneus montados", "taxa_iva_id": 1, "preco1": 17000000, "composto": true,
        "componentes": [{ "componente_id": pneu, "quantidade": 4 }, { "componente_id": valvula, "quantidade": 4 }] })).await["id"].as_i64().unwrap();
    let e = rpc_err(&st, "artigos.guardar", json!({ "id": kit, "descricao": "Kit", "taxa_iva_id": 1, "preco1": 1, "composto": true, "componentes": [{ "componente_id": kit, "quantidade": 1 }] })).await;
    assert!(e.contains("kit"), "{e}");
    // logo a seguir a uma gravação recusada (a transacção ainda pode estar a desfazer-se noutra ligação)
    let e = rpc_err(&st, "artigos.guardar", json!({ "descricao": "Kit de kits", "taxa_iva_id": 1, "preco1": 1, "composto": true, "componentes": [{ "componente_id": kit, "quantidade": 1 }] })).await;
    assert!(e.contains("composto"), "{e}");
    assert_eq!(rpc(&st, "artigos.componentes", json!({ "id": kit })).await.as_array().unwrap().len(), 2);
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": kit, "quantidade": 2 }] })).await;
    assert_eq!(stock(&st, pneu).await, 2.0, "a venda do kit baixa os componentes");
    assert_eq!(stock(&st, valvula).await, 2.0);

    // =========================== oficina ===========================
    let mao = rpc(&st, "artigos.guardar", json!({ "descricao": "Mão de obra (hora)", "tipo": "S", "taxa_iva_id": 1, "preco1": 1500000 })).await["id"].as_i64().unwrap();
    let viat = rpc(&st, "oficina.viaturaGuardar", json!({ "entidade_id": cli, "matricula": "ld-11-22-aa", "marca": "Toyota", "modelo": "Hilux", "km": 1000 })).await["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "oficina.viaturaGuardar", json!({ "matricula": "LD-11-22-AA", "marca": "Nissan" })).await.contains("matrícula"));
    assert!(rpc_err(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat, "km_entrada": 1500, "km_saida": 1400 })).await.contains("quilometragem"));
    let os = rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat, "km_entrada": 1500, "avaria": "Ruído nos travões", "tecnico": "Mário",
        "linhas": [{ "artigo_id": mao, "descricao": "Mão de obra (hora)", "quantidade": 2 }, { "artigo_id": pneu, "descricao": "Pneu 195/65", "quantidade": 1 }] })).await["id"].as_i64().unwrap();
    let o = rpc(&st, "oficina.ordem", json!({ "id": os })).await;
    assert_eq!(o["numero"], format!("OS {ano}/1"));
    assert_eq!(o["cliente"], "Transportes Lda");
    assert_eq!(o["linhas"].as_array().unwrap().len(), 2);
    assert_eq!(rpc(&st, "oficina.viaturas", json!({ "q": "hilux" })).await[0]["km"], 1500);
    let pp = rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": cli, "linhas": [{ "artigo_id": mao, "quantidade": 2 }] })).await;
    assert!(rpc_err(&st, "oficina.associar", json!({ "ordem_id": os, "documento_id": pp["documento"]["id"] })).await.contains("factura"));
    let fos = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "referencia": "OS", "linhas": [{ "artigo_id": mao, "quantidade": 2 }, { "artigo_id": pneu, "quantidade": 1 }] })).await;
    rpc(&st, "oficina.associar", json!({ "ordem_id": os, "documento_id": fos["documento"]["id"] })).await;
    assert_eq!(rpc(&st, "oficina.ordem", json!({ "id": os })).await["estado"], "FACTURADA");
    assert!(rpc_err(&st, "oficina.associar", json!({ "ordem_id": os, "documento_id": fos["documento"]["id"] })).await.contains("facturada"));
    assert!(rpc_err(&st, "oficina.ordemGuardar", json!({ "id": os, "viatura_id": viat })).await.contains("facturada"));
    let os2 = rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat })).await["id"].as_i64().unwrap();
    assert_eq!(rpc(&st, "oficina.ordem", json!({ "id": os2 })).await["numero"], format!("OS {ano}/2"));
    // quadro kanban: muda só o estado; concluir regista a data; facturada só pela factura e depois não muda
    rpc(&st, "oficina.ordemEstado", json!({ "id": os2, "estado": "EM_CURSO" })).await;
    let o2 = rpc(&st, "oficina.ordemEstado", json!({ "id": os2, "estado": "CONCLUIDA" })).await;
    assert_eq!(o2["estado"], "CONCLUIDA");
    assert!(rpc(&st, "oficina.ordem", json!({ "id": os2 })).await["data_conclusao"].is_string());
    assert!(rpc_err(&st, "oficina.ordemEstado", json!({ "id": os2, "estado": "FACTURADA" })).await.contains("inválido"));
    assert!(rpc_err(&st, "oficina.ordemEstado", json!({ "id": os, "estado": "ABERTA" })).await.contains("facturada"));

    // =========================== moedas e custos de importação ===========================
    assert!(rpc_err(&st, "moedas.guardar", json!({ "codigo": "AOA", "descricao": "Kwanza", "simbolo": "Kz", "cambio": 2.0 })).await.contains("Kwanza"));
    rpc(&st, "moedas.guardar", json!({ "codigo": "usd", "descricao": "Dólar americano", "simbolo": "US$", "cambio": 900.0 })).await;
    let oleo = rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo 5W30 (lata)", "taxa_iva_id": 1, "preco1": 2000000 })).await["id"].as_i64().unwrap();
    let compra = |moeda: &str, hash: &str| json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "INV-77", "doc_externo_hash": hash, "doc_externo_data": dias(-3),
        "moeda": moeda, "linhas": [{ "artigo_id": oleo, "quantidade": 10, "preco_unitario": 1000 }], "custos": [{ "tipo": "FRETE", "valor": 900000 }] });
    assert!(rpc_err(&st, "documentos.emitir", compra("EUR", "AbCd")).await.contains("âmbio"));
    assert!(rpc_err(&st, "documentos.emitir", compra("USD", "ABC")).await.contains("4 caracteres"));
    let fc = rpc(&st, "documentos.emitir", compra("USD", "AbCd")).await;
    // 10 × 10,00 USD × 900 = 90 000 Kz + IVA 14%
    assert_eq!(fc["documento"]["total_liquido"], 9000000);
    assert_eq!(fc["documento"]["total"], 10260000);
    assert_eq!(fc["documento"]["total_moeda"], 11400);
    assert_eq!(fc["custos"].as_array().unwrap().len(), 1);
    let a_oleo = rpc(&st, "artigos.obter", json!({ "id": oleo })).await;
    assert_eq!(a_oleo["custo"], 990000, "o frete entra no custo de aquisição (9 000 + 900 Kz)");
    let fc_id = fc["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "VNC", "entidade_id": forn, "doc_externo": "NC-5", "doc_externo_hash": "WxYz", "doc_origem_id": fc_id,
        "motivo": "Latas amolgadas", "linhas": [{ "artigo_id": oleo, "quantidade": 1, "preco_unitario": 900000 }] })).await;

    // =========================== SAF-T de aquisição ===========================
    let sc = rpc(&st, "saft.exportarCompras", json!({ "de": format!("{ano}-01-01"), "ate": format!("{ano}-12-31") })).await;
    assert_eq!(sc["documentos"], 2);
    validar_xsd(sc["ficheiro"].as_str().unwrap());
    let xml = std::fs::read_to_string(sc["ficheiro"].as_str().unwrap()).unwrap();
    for frag in [
        "<TaxAccountingBasis>A</TaxAccountingBasis>",
        "<InvoiceNo>INV-77</InvoiceNo>",
        "<Hash>AbCd</Hash>",
        "<PurchaseType>FT</PurchaseType>",
        "<PurchaseType>NC</PurchaseType>",
        "<Reference>INV-77</Reference>",
        "<DeductibleTaxType>IMPT</DeductibleTaxType>",
        "<CurrencyCode>USD</CurrencyCode>",
        "<ExchangeRate>900.000000</ExchangeRate>",
        "<Country>PT</Country>",
        &format!("<InvoiceDate>{}</InvoiceDate>", dias(-3)),
    ] {
        assert!(xml.contains(frag), "falta {frag} no SAF-T de aquisição");
    }
    // e o SAF-T de facturação do mesmo período também é válido
    let sv = rpc(&st, "saft.exportar", json!({ "de": format!("{ano}-01-01"), "ate": format!("{ano}-12-31") })).await;
    validar_xsd(sv["ficheiro"].as_str().unwrap());

    // =========================== relatórios novos ===========================
    let periodo = json!({ "de": format!("{ano}-01-01"), "ate": format!("{ano}-12-31") });
    let vendas = |agrupar: &str, ent: Value| {
        let mut v = periodo.clone();
        v["agrupar"] = json!(agrupar);
        v["entidade_id"] = ent;
        v
    };
    // por posto: tudo foi vendido no posto desta máquina
    let por_posto = rpc(&st, "relatorios.vendas", vendas("posto", Value::Null)).await;
    assert_eq!(por_posto.as_array().unwrap().len(), 1);
    assert_eq!(por_posto[0]["chave"], "Posto#1");
    // detalhe de documentos: uma linha por documento de venda
    let por_doc = rpc(&st, "relatorios.vendas", vendas("documento", Value::Null)).await;
    assert!(por_doc.as_array().unwrap().len() >= 2, "um registo por documento: {por_doc}");
    assert!(por_doc[0]["chave"].as_str().unwrap().contains("FT "));
    // artigos por entidade, com e sem filtro de cliente
    let ae = rpc(&st, "relatorios.vendas", vendas("artigo_entidade", Value::Null)).await;
    assert!(ae.as_array().unwrap().iter().any(|l| l["chave"].as_str().unwrap().contains("Transportes Lda · Pneu 195/65")), "{ae}");
    let ae_cli = rpc(&st, "relatorios.vendas", vendas("artigo_entidade", json!(cli))).await;
    assert!(ae_cli.as_array().unwrap().iter().all(|l| l["chave"].as_str().unwrap().starts_with("Transportes Lda")));
    assert!(ae_cli.as_array().unwrap().len() <= ae.as_array().unwrap().len());

    // relatórios de stock: ruptura, mínimo, excesso, validade e parados
    let rel_stock = |situacao: &str, dias: i64| json!({ "situacao": situacao, "armazem_id": 1, "dias": dias });
    let nomes = |v: &Value| v.as_array().unwrap().iter().map(|l| l["descricao"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    rpc(&st, "artigos.guardar", json!({ "descricao": "Artigo esgotado", "taxa_iva_id": 1, "preco1": 1000, "qtd_minima": 5 })).await;
    let r = rpc(&st, "relatorios.stock", rel_stock("ruptura", 0)).await;
    assert!(nomes(&r).contains(&"Artigo esgotado".to_string()), "{r}");
    let excessivo = rpc(&st, "artigos.guardar", json!({ "descricao": "Artigo em excesso", "taxa_iva_id": 1, "preco1": 1000, "qtd_maxima": 3 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": excessivo, "quantidade": 10, "preco_unitario": 100 }] })).await;
    assert!(nomes(&rpc(&st, "relatorios.stock", rel_stock("excesso", 0)).await).contains(&"Artigo em excesso".to_string()));
    assert!(!nomes(&rpc(&st, "relatorios.stock", rel_stock("ruptura", 0)).await).contains(&"Artigo em excesso".to_string()));
    // o lote L1 expira em 10 dias → aparece a 30 dias e não a 5
    let val30 = rpc(&st, "relatorios.stock", rel_stock("validade", 30)).await;
    assert!(val30.as_array().unwrap().iter().any(|l| l["lote"] == "L1"), "{val30}");
    assert!(!rpc(&st, "relatorios.stock", rel_stock("validade", 5)).await.as_array().unwrap().iter().any(|l| l["lote"] == "L1"));
    // sem saída há 0 dias: entrou agora e nunca saiu
    assert!(nomes(&rpc(&st, "relatorios.stock", rel_stock("parado", 0)).await).contains(&"Artigo em excesso".to_string()));
    assert!(rpc_err(&st, "relatorios.stock", rel_stock("inventado", 0)).await.contains("inválida"));

    // rastreio do lote L1: entrada pelo documento de stock e saída pela factura
    // (uma venda nova, já que a anterior foi anulada e a anulação repõe o lote)
    assert!(rpc_err(&st, "relatorios.rastreio", json!({})).await.contains("lote ou o artigo"));
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": med, "quantidade": 2, "lote": "L1" }] })).await;
    let rast = rpc(&st, "relatorios.rastreio", json!({ "lote": "L1" })).await;
    assert_eq!(rast.as_array().unwrap().len(), 1, "{rast}");
    assert_eq!(rast[0]["descricao"], "Paracetamol 500mg");
    let movs = rast[0]["movimentos"].as_array().unwrap();
    assert!(movs.iter().any(|m| m["quantidade"].as_f64().unwrap() > 0.0 && m["tipo"].as_str().unwrap().contains("Entrada")), "{movs:?}");
    assert!(
        movs.iter().any(|m| m["quantidade"].as_f64().unwrap() < 0.0 && m["entidade_nome"] == "Transportes Lda"),
        "falta a saída para o cliente: {movs:?}"
    );
    // pelo artigo saem os dois lotes
    assert_eq!(rpc(&st, "relatorios.rastreio", json!({ "artigo_id": med })).await.as_array().unwrap().len(), 2);

    // mapa de fornecedor: compra em USD menos a nota de crédito, e a dívida
    let mapa = rpc(&st, "relatorios.fornecedores", periodo.clone()).await;
    let f = &mapa["resumo"][0];
    assert_eq!(f["nome"], "Import Export SA");
    assert_eq!(f["compras"], 10260000 - 1026000, "compras líquidas da V/Nota de crédito");
    assert_eq!(f["docs"], 2);
    assert_eq!(mapa["documentos"].as_array().unwrap().len(), 0, "sem fornecedor escolhido não vêm documentos");
    let mut so_forn = periodo.clone();
    so_forn["entidade_id"] = json!(forn);
    let mapa1 = rpc(&st, "relatorios.fornecedores", so_forn).await;
    assert_eq!(mapa1["resumo"].as_array().unwrap().len(), 1);
    assert_eq!(mapa1["documentos"].as_array().unwrap().len(), 2);
    assert_eq!(mapa1["documentos"][0]["doc_externo"], "INV-77");
    assert_eq!(mapa1["pendentes"][0]["pendente"], 10260000 - 1026000, "a dívida é a V/Factura menos a V/Nota de crédito");

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn series_manuais_creditos_estado() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-series-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Séries", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let hoje = db::hoje();
    let ano = &hoje[..4];
    let dias = |n: i64| (chrono::Local::now().date_naive() + chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço técnico", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Limite", "nif": "5417000002", "limite_credito": 150000 })).await["id"].as_i64().unwrap();
    let estado = rpc(&st, "entidades.guardar", json!({ "nome": "Ministério X", "nif": "7401000001", "estado": true })).await["id"].as_i64().unwrap();
    let ft = |ent: i64, extra: Value| {
        let mut v = json!({ "tipo": "FT", "entidade_id": ent, "linhas": [{ "artigo_id": art, "quantidade": 1 }] });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };

    // ---------------- séries ----------------
    assert!(rpc_err(&st, "series.guardar", json!({ "tipo_codigo": "FT", "serie": "M 1", "manual": true })).await.contains("letras"));
    assert!(rpc_err(&st, "series.guardar", json!({ "tipo_codigo": "FT", "serie": "M1", "manual": true, "padrao": true })).await.contains("manuais"));
    rpc(&st, "series.guardar", json!({ "tipo_codigo": "FT", "serie": "a26", "descricao": "Facturas loja", "padrao": true, "data_inicio": format!("{ano}-01-01"), "data_fim": format!("{ano}-12-31") })).await;
    let manual_id = rpc(&st, "series.guardar", json!({ "tipo_codigo": "FT", "serie": "M26", "descricao": "Livro de facturas manuais", "manual": true })).await["id"].as_i64().unwrap();
    let f1 = rpc(&st, "documentos.emitir", ft(estado, json!({}))).await;
    assert_eq!(f1["documento"]["numero_doc"], "FT A26/1", "usa a série por defeito");
    assert!(rpc_err(&st, "documentos.emitir", ft(estado, json!({ "serie": "M26" }))).await.contains("manuais"));
    rpc(&st, "series.estado", json!({ "id": manual_id, "ativa": false })).await;
    assert!(rpc_err(&st, "documentos.emitir", ft(estado, json!({ "serie": "M26", "manual": { "numero": "FT 0001", "data": dias(-5) } }))).await.contains("bloqueada"));
    rpc(&st, "series.estado", json!({ "id": manual_id, "ativa": true })).await;
    assert!(rpc_err(&st, "series.eliminar", json!({ "id": rpc(&st, "series.listar", json!({ "tipo": "FT" })).await.as_array().unwrap().iter().find(|s| s["serie"] == "A26").unwrap()["id"] })).await.contains("documentos"));

    // ---------------- documento manual (recuperação) ----------------
    let m1 = rpc(&st, "documentos.emitir", ft(cli, json!({ "serie": "M26", "manual": { "numero": "FT 0001", "data": dias(-5) } }))).await;
    assert_eq!(m1["documento"]["numero_doc"], "FT M26/1");
    assert_eq!(m1["documento"]["data_emissao"], dias(-5));
    assert_eq!(m1["documento"]["origem"], "M");
    assert_eq!(m1["documento"]["doc_manual"], "FT 0001");
    assert!(rpc_err(&st, "documentos.emitir", ft(cli, json!({ "serie": "M26", "manual": { "numero": "FT 0001", "data": dias(-4) } }))).await.contains("já foi recuperado"));
    assert!(rpc_err(&st, "documentos.emitir", ft(cli, json!({ "serie": "M26", "manual": { "numero": "FT 0002", "data": dias(-6) } }))).await.contains("anterior"));
    assert!(rpc_err(&st, "documentos.emitir", ft(cli, json!({ "serie": "M26", "manual": { "numero": "FT 0003", "data": dias(1) } }))).await.contains("inválida"));

    // ---------------- limite de crédito ----------------
    // o cliente já deve 1 140 Kz (manual); limite 1 500 Kz → nova FT de 1 140 Kz excede
    rpc(&st, "utilizadores.guardar", json!({ "id": 1, "nome": "Admin", "login": "admin", "perfil": "ADMIN", "pin": "4321" })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await;
    rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
    assert!(rpc_err(&st, "documentos.emitir", ft(cli, json!({}))).await.contains("Limite de crédito"));
    assert!(rpc_err(&st, "documentos.emitir", ft(cli, json!({ "autorizacao_pin": "0000" }))).await.contains("PIN"));
    let f2 = rpc(&st, "documentos.emitir", ft(cli, json!({ "autorizacao_pin": "4321" }))).await;
    assert_eq!(f2["documento"]["autorizado_por"], 1);
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // ---------------- adiantamento + compensação ----------------
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "RA", "entidade_id": 1, "valor": 50000, "pagamentos": [{ "modo": "NU", "valor": 50000 }] })).await.contains("cliente"));
    let ra = rpc(&st, "documentos.emitir", json!({ "tipo": "RA", "entidade_id": cli, "valor": 50000, "pagamentos": [{ "modo": "TB", "valor": 50000 }] })).await;
    let ra_id = ra["documento"]["id"].as_i64().unwrap();
    assert_eq!(ra["documento"]["credito_disponivel"], 50000);
    let saldo_cli = |v: &Value| v.as_array().unwrap().iter().find(|e| e["id"] == cli).unwrap()["saldo"].as_i64().unwrap();
    assert_eq!(saldo_cli(&rpc(&st, "entidades.listar", json!({ "tipo": "C" })).await), 228000 - 50000);
    let m1_id = m1["documento"]["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": cli, "liquidar": [{ "documento_id": m1_id, "valor": 114000 }], "pagamentos": [{ "modo": "CS", "valor": 60000 }, { "modo": "NU", "valor": 54000 }] })).await.contains("insuficiente"));
    let rc = rpc(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": cli, "liquidar": [{ "documento_id": m1_id, "valor": 114000 }], "pagamentos": [{ "modo": "CS", "valor": 50000 }, { "modo": "NU", "valor": 64000 }] })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ra_id })).await["documento"]["credito_disponivel"], 0);
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": ra_id, "motivo": "teste de anulação" })).await.contains("movimentado"));
    let fluxo = rpc(&st, "tesouraria.fluxo", json!({ "de": "2000-01-01", "ate": "2100-01-01" })).await;
    assert_eq!(fluxo["entradas"], 50000 + 64000, "a compensação não é dinheiro");
    rpc(&st, "documentos.anular", json!({ "id": rc["documento"]["id"], "motivo": "recibo emitido por engano" })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ra_id })).await["documento"]["credito_disponivel"], 50000, "anular o recibo devolve o crédito");

    // ---------------- Factura/Recibo paga com o crédito do cliente (como o POS oferece) ----------------
    let fr_cs = |cs: i64, nu: i64, ent: i64| json!({ "tipo": "FR", "entidade_id": ent, "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "CS", "valor": cs }, { "modo": "NU", "valor": nu }] });
    assert!(rpc_err(&st, "documentos.emitir", fr_cs(50000, 64000, 1)).await.contains("Identifique o cliente"));
    assert!(rpc_err(&st, "documentos.emitir", fr_cs(60000, 54000, cli)).await.contains("insuficiente"));
    assert!(rpc_err(&st, "documentos.emitir", fr_cs(50000, 0, cli)).await.contains("insuficiente"), "o crédito não chega: falta pagar o resto");
    let fr_pago = rpc(&st, "documentos.emitir", fr_cs(50000, 70000, cli)).await;
    assert_eq!((fr_pago["documento"]["troco"].as_i64(), fr_pago["documento"]["valor_pendente"].as_i64()), (Some(6000), Some(0)), "troco só do numerário");
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ra_id })).await["documento"]["credito_disponivel"], 0, "o adiantamento foi usado");
    rpc(&st, "documentos.anular", json!({ "id": fr_pago["documento"]["id"], "motivo": "venda registada por engano" })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ra_id })).await["documento"]["credito_disponivel"], 50000, "anular a FR devolve o crédito");
    assert_eq!(rpc(&st, "entidades.obter", json!({ "id": cli })).await["credito"], 50000, "o POS vê o crédito do cliente");
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "CS", "valor": 10000 }] })).await.contains("compensação"));

    // ---------------- nota de crédito sobre factura paga → crédito → devolução ----------------
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let nc = rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": fr["documento"]["id"], "motivo": "Serviço não prestado",
        "linhas": [{ "artigo_id": art, "quantidade": 1, "preco_unitario": 100000 }] })).await;
    let nc_id = nc["documento"]["id"].as_i64().unwrap();
    assert_eq!(nc["documento"]["credito_disponivel"], 114000);
    let creditos = rpc(&st, "contas.creditos", json!({ "entidade_id": cli })).await;
    assert_eq!(creditos.as_array().unwrap().len(), 2);
    assert!(rpc_err(&st, "contas.devolver", json!({ "credito_id": nc_id, "valor": 200000, "conta_id": 1 })).await.contains("disponível"));
    let dv = rpc(&st, "contas.devolver", json!({ "credito_id": nc_id, "valor": 100000, "conta_id": 1 })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": nc_id })).await["documento"]["credito_disponivel"], 14000);
    rpc(&st, "tesouraria.anular", json!({ "id": dv["movimento_id"], "motivo": "devolução lançada na conta errada" })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": nc_id })).await["documento"]["credito_disponivel"], 114000);
    rpc(&st, "contas.devolver", json!({ "credito_id": nc_id, "valor": 114000, "conta_id": 1 })).await;

    // ---------------- regularização ----------------
    let f2_id = f2["documento"]["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "contas.regularizar", json!({ "entidade_id": cli, "tipo": "CREDITO", "valor": 1000, "justificacao": "x" })).await.contains("justificação"));
    let reg = rpc(&st, "contas.regularizar", json!({ "entidade_id": cli, "tipo": "CREDITO", "valor": 4000, "justificacao": "Arredondamento acordado com o cliente" })).await;
    let pend = |v: &Value| v["documento"]["valor_pendente"].as_i64().unwrap();
    assert_eq!(pend(&rpc(&st, "documentos.obter", json!({ "id": m1_id })).await), 110000, "abate o documento mais antigo");
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": m1_id, "motivo": "teste de anulação" })).await.contains("movimentado"));
    rpc(&st, "contas.regularizacaoAnular", json!({ "id": reg["id"], "motivo": "lançada por engano" })).await;
    assert_eq!(pend(&rpc(&st, "documentos.obter", json!({ "id": m1_id })).await), 114000);
    rpc(&st, "contas.regularizar", json!({ "entidade_id": cli, "tipo": "DEBITO", "valor": 10000, "justificacao": "Crédito do adiantamento perdido" })).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ra_id })).await["documento"]["credito_disponivel"], 40000);

    // ---------------- extracto = saldo da ficha ----------------
    let ext = rpc(&st, "contas.extrato", json!({ "entidade_id": cli })).await;
    let saldo = saldo_cli(&rpc(&st, "entidades.listar", json!({ "tipo": "C" })).await);
    assert_eq!(ext["saldo"].as_i64().unwrap(), saldo, "o extracto bate com o saldo da ficha");
    assert_eq!(saldo, 114000 + 114000 - 40000);
    assert!(ext["movimentos"].as_array().unwrap().iter().any(|m| m["origem"] == "DEVOLUCAO"));
    let _ = f2_id;

    // ---------------- aviso de vencimento ----------------
    let av = rpc(&st, "contas.vencidos", json!({ "tipo": "C" })).await;
    let e = av["entidades"].as_array().unwrap().iter().find(|e| e["entidade_id"] == cli).expect("cliente com documentos vencidos").clone();
    assert_eq!(e["documentos"][0]["numero_doc"], "FT M26/1");
    assert_eq!(e["documentos"][0]["dias_atraso"], 5);

    // ---------------- SAF-T: manual (M), HashControl 0 e SAF-T do Estado ----------------
    let sv = rpc(&st, "saft.exportar", json!({ "de": format!("{ano}-01-01"), "ate": format!("{ano}-12-31") })).await;
    validar_xsd(sv["ficheiro"].as_str().unwrap());
    let xml = std::fs::read_to_string(sv["ficheiro"].as_str().unwrap()).unwrap();
    assert!(xml.contains("<SourceBilling>M</SourceBilling>"));
    // sem certificado: HashControl 0; numa compilação com o certificado do produtor, a versão da chave
    let hc = if crate::produtor::certificado().is_some() { crate::produtor::versao_chave() } else { "0".to_string() };
    assert!(xml.contains(&format!("<HashControl>{hc}</HashControl>")), "HashControl {hc}");
    if hc == "0" {
        assert!(!xml.contains("<HashControl>1</HashControl>"), "sem certificado: HashControl 0");
    }
    assert!(!xml.contains("RA "), "o recibo de adiantamento não é documento fiscal");
    let se = rpc(&st, "saft.exportarEstado", json!({ "de": format!("{ano}-01-01"), "ate": format!("{ano}-12-31") })).await;
    validar_xsd(se["ficheiro"].as_str().unwrap());
    let xe = std::fs::read_to_string(se["ficheiro"].as_str().unwrap()).unwrap();
    assert!(xe.contains("<TaxAccountingBasis>E</TaxAccountingBasis>"));
    assert!(xe.contains("FT A26/1") && !xe.contains("FT M26/1"), "só documentos de entidades públicas");
    assert_eq!(se["facturas"], 1);

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Impressão térmica directa: bytes ESC/POS do talão, corte, acentos e regra da gaveta.
/// Usa a ligação "ficheiro", que escreve os mesmos bytes que iriam para a impressora.
#[tokio::test]
async fn talao_escpos() {
    let pasta = std::env::temp_dir().join(format!("kigest-escpos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.expect("posto 1");

    rpc(
        &st,
        "setup.inicializar",
        json!({ "empresa": { "nome": "Loja Térmica, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } }),
    )
    .await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // impressora "em ficheiro" no posto desta máquina
    let saida = pasta.join("talao.prn");
    rpc(
        &st,
        "posto.guardar",
        json!({
            "id": 1, "descricao": "Posto#1", "armazem_id": 1, "serie_prefixo": "",
            "opcoes": { "formato": "ticket", "vias": 1, "escpos": {
                "ativo": true, "ligacao": "ficheiro", "destino": saida.to_str().unwrap(),
                "colunas": 42, "cortar": true, "acentos": false, "gaveta": "numerario", "pino": 2
            }}
        }),
    )
    .await;

    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Café São Brás 500g", "taxa_iva_id": 1, "preco1": 100000 })).await;
    let artigo = art["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": artigo, "quantidade": 10, "preco_unitario": 50000 }] })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 1000000 })).await;

    // ---- venda em numerário: talão + gaveta ----
    let fr = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 300000 }] }),
    )
    .await;
    let id = fr["documento"]["id"].as_i64().unwrap();
    let r = rpc(&st, "impressao.talao", json!({ "id": id, "vias": 2 })).await;
    assert_eq!(r["vias"], 2);
    assert_eq!(r["gaveta"], true);

    let bytes = std::fs::read(&saida).unwrap();
    let texto = String::from_utf8_lossy(&bytes).to_string();
    if std::env::var("VER_TALAO").is_ok() {
        eprintln!("{texto}");
    }
    assert!(bytes.starts_with(&[0x1B, b'@']), "falta a inicialização ESC @");
    assert!(texto.contains("Loja Termica, Lda"), "sem cabeçalho da empresa (sem acentos): {texto}");
    assert!(texto.contains("Cafe Sao Bras 500g"), "sem a linha do artigo");
    assert!(texto.contains("2.280,00"), "sem o total formatado: {texto}");
    assert!(texto.contains("Troco"), "sem o troco");
    assert!(texto.contains("Processado por programa"), "sem o rodapé da AGT");
    assert_eq!(texto.matches("Total (AOA)").count(), 2, "deviam sair 2 vias: {texto}");
    assert!(texto.contains("Qtd  Un"), "sem o cabeçalho das colunas: {texto}");
    assert!(texto.contains("Modo de Pagamento") && texto.contains("Resumo de Imposto"), "sem as secções do talão do KIGEST");
    assert!(texto.contains("* OBRIGADO VOLTE SEMPRE *"), "sem o fecho do talão");
    assert_eq!(bytes.windows(3).filter(|w| *w == [0x1D, b'V', 1]).count(), 2, "cada via tem de cortar o papel");
    assert_eq!(bytes.windows(5).filter(|w| *w == [0x1B, b'p', 0, 25, 250]).count(), 1, "a gaveta abre uma vez");
    // nenhuma linha passa da largura do papel
    for l in texto.lines() {
        let visivel: String = l.chars().filter(|c| !c.is_control() && *c != '\u{1b}' && *c != '\u{1d}').collect();
        assert!(visivel.chars().count() <= 48, "linha larga demais ({}): {l}", visivel.chars().count());
    }

    // ---- venda em Multicaixa: talão sem gaveta ----
    std::fs::remove_file(&saida).unwrap();
    let mb = rpc(
        &st,
        "documentos.emitir",
        json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "MB", "valor": 114000 }] }),
    )
    .await;
    let r = rpc(&st, "impressao.talao", json!({ "id": mb["documento"]["id"], "segunda_via": true })).await;
    assert_eq!(r["gaveta"], false);
    let bytes = std::fs::read(&saida).unwrap();
    assert!(!bytes.windows(5).any(|w| w == [0x1B, b'p', 0, 25, 250]), "a gaveta não devia abrir sem numerário");
    assert!(String::from_utf8_lossy(&bytes).contains("2.a via"), "reimpressão tem de sair marcada como 2.ª via");

    // ---- botão da gaveta e teste de impressão ----
    std::fs::remove_file(&saida).unwrap();
    rpc(&st, "impressao.gaveta", Value::Null).await;
    assert_eq!(std::fs::read(&saida).unwrap().windows(5).filter(|w| *w == [0x1B, b'p', 0, 25, 250]).count(), 1);
    std::fs::remove_file(&saida).unwrap();
    rpc(&st, "impressao.teste", Value::Null).await;
    assert!(String::from_utf8_lossy(&std::fs::read(&saida).unwrap()).contains("TESTE DE IMPRESSAO"));

    // ---- sem configuração não se imprime às escondidas ----
    rpc(&st, "posto.guardar", json!({ "id": 1, "descricao": "Posto#1", "armazem_id": 1, "serie_prefixo": "", "opcoes": { "escpos": { "ativo": false } } })).await;
    assert!(rpc_err(&st, "impressao.talao", json!({ "id": id })).await.contains("não está activada"));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Postos em rede: o servidor serve o mesmo despachante por HTTP, cada posto tem a sua sessão
/// e os documentos saem com a série do posto que os emitiu.
/// Precisa de runtime multi-thread (o cliente faz I/O bloqueante).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rede_multiposto() {
    use std::io::{Read, Write};

    let pasta = std::env::temp_dir().join(format!("kigest-rede-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();

    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Rede, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let artigo = rpc(&st, "artigos.guardar", json!({ "descricao": "Água 1,5L", "taxa_iva_id": 1, "preco1": 50000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": artigo, "quantidade": 100, "preco_unitario": 20000 }] })).await;
    // posto 2 (o que vai estar na outra máquina), com a sua própria série
    let posto2 = rpc(&st, "posto.guardar", json!({ "descricao": "Caixa 2", "armazem_id": 1, "serie_prefixo": "P2" })).await["id"].as_i64().unwrap();
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Joana", "login": "joana", "senha": "432100", "perfil": "CAIXA", "mudar_senha": false })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    assert!(st.sessao_opcional().is_none());

    // ---- abre o servidor numa porta livre ----
    let porta = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arc = std::sync::Arc::new(st);
    let cfg = crate::rede::Config { modo: "servidor".into(), porta, servidor: String::new(), chave: "segredo123".into(), sal: String::new(), web: true };
    crate::rede::registar_estado(arc.clone());
    assert!(crate::rede::iniciar_servidor(arc.clone(), &crate::rede::Config { chave: "curta".into(), ..cfg.clone() }).is_err(), "a chave tem de ter 6 caracteres");
    assert_eq!(crate::rede::iniciar_servidor(arc.clone(), &cfg).unwrap(), porta);
    let gravado: crate::rede::Config = serde_json::from_str(&std::fs::read_to_string(pasta.join("rede.json")).unwrap()).unwrap();
    assert!(!gravado.sal.is_empty(), "o servidor guarda o sal da derivação da chave");

    // posto cliente: fala com o servidor pelo protocolo cifrado
    let cliente = crate::rede::Config { modo: "cliente".into(), porta, servidor: format!("127.0.0.1:{porta}"), chave: "segredo123".into(), sal: String::new(), web: true };
    let pedir_v2 = |token: &str, metodo: &str, params: Value| -> Result<Value, String> {
        crate::rede::pedir_com_token(&cliente, token, metodo, params, posto2, "Caixa 2").map_err(|e| e.to_string())
    };
    let bruto = |corpo: &str| -> String {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).expect("ligar ao servidor");
        s.write_all(format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len()).as_bytes()).unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        r
    };

    // ---- a chave de rede é obrigatória (e nunca vai na rede) ----
    let errado = crate::rede::Config { chave: "outra-chave-qualquer".into(), ..cliente.clone() };
    let mau = crate::rede::pedir_com_token(&errado, "t1", "setup.estado", Value::Null, posto2, "Caixa 2").unwrap_err().to_string();
    assert!(mau.contains("Chave de rede"), "{mau}");
    // um posto com o protocolo antigo (chave em claro) não é atendido e é avisado
    let antigo = bruto(&json!({ "chave": "segredo123", "token": "t9", "posto": posto2, "metodo": "artigos.listar", "params": {} }).to_string());
    assert!(antigo.contains("vers") && antigo.contains("antiga") && !antigo.contains("\"dados\""), "{antigo}");

    // ---- sem sessão não se lê nada; cada posto tem a sua ----
    let sem = pedir_v2("t1", "artigos.listar", json!({})).unwrap_err();
    assert!(sem.contains("Sessão"), "{sem}");
    let entrada = pedir_v2("t1", "auth.login", json!({ "login": "joana", "senha": "432100" })).unwrap();
    assert_eq!(entrada["nome"], "Joana");
    assert_eq!(pedir_v2("t1", "artigos.listar", json!({})).unwrap().as_array().unwrap().len(), 1);
    assert!(pedir_v2("t2", "artigos.listar", json!({})).is_err(), "a sessão de um posto não serve outro posto");
    // e a máquina do servidor continua sem sessão
    assert!(arc.sessao_opcional().is_none(), "o login de um posto em rede não entra na máquina do servidor");

    // ---- o posto do pedido é o que manda: série e caixa são do posto 2 ----
    assert_eq!(pedir_v2("t1", "posto.obter", Value::Null).unwrap()["atual"]["descricao"], "Caixa 2");
    pedir_v2("t1", "caixa.abrir", json!({ "valor_inicial": 500000 })).unwrap();
    let venda = pedir_v2("t1", "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).unwrap();
    let numero = venda["documento"]["numero_doc"].as_str().unwrap().to_string();
    assert!(numero.contains("P2"), "o documento tem de sair na série do posto 2: {numero}");
    assert_eq!(venda["documento"]["posto_id"], posto2);
    assert_eq!(arc.posto().id, 1, "o posto desta máquina não muda por servir outro");

    // ---- o GET serve para confirmar que há um KanGest à escuta ----
    let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    assert!(r.contains("\"app\":\"kigest\""), "{r}");

    // ---- os postos ligados aparecem no estado da rede ----
    rpc(&arc, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let estado = rpc(&arc, "rede.estado", Value::Null).await;
    assert_eq!(estado["modo"], "servidor");
    assert_eq!(estado["servidor_activo"], porta);
    let ligados = estado["ligados"].as_array().unwrap();
    assert!(ligados.iter().any(|l| l["utilizador"] == "Joana" && l["posto"] == "Caixa 2"), "{ligados:?}");
    assert!(rpc_err(&arc, "rede.testar", Value::Null).await.contains("posto cliente"));
    // a chave de rede só a vê quem pode mudar a configuração
    assert_eq!(estado["chave"], "segredo123");

    // ---- um pedido capturado na rede não pode ser repetido ----
    let (corpo, k) = crate::rede::teste_pedido_cifrado(&cliente, "t1", "artigos.listar", json!({}), posto2);
    assert!(!corpo.contains("artigos.listar") && !corpo.contains("segredo123"), "o pedido vai cifrado");
    assert_eq!(crate::rede::teste_abrir_resposta(&k, &bruto(&corpo))["ok"], true);
    let repetido = crate::rede::teste_abrir_resposta(&k, &bruto(&corpo));
    assert!(repetido["erro"].as_str().unwrap().contains("repetido"), "{repetido}");

    // ---- modo web: o mesmo servidor entrega a aplicação ao browser (telemóvel, tablet, terminal) ----
    let js = format!("console.log('{}');", "kangest ".repeat(400));
    let js_c = js.clone();
    crate::web::registar(Box::new(move |c: &str| match c {
        "index.html" => Some((b"<!doctype html><div id=\"root\"></div>".to_vec(), "text/html".into())),
        "assets/app-Ab12.js" => Some((js_c.clone().into_bytes(), "text/javascript".into())),
        // como o Tauri: um caminho desconhecido devolve a página inicial
        _ => Some((b"<!doctype html><div id=\"root\"></div>".to_vec(), "text/html".into())),
    }));
    let get = |caminho: &str, cabecalhos: &str| -> (String, Vec<u8>) {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
        s.write_all(format!("GET {caminho} HTTP/1.1\r\nHost: localhost\r\n{cabecalhos}Connection: close\r\n\r\n").as_bytes()).unwrap();
        let mut r = Vec::new();
        s.read_to_end(&mut r).unwrap();
        let corte = r.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        (String::from_utf8_lossy(&r[..corte]).to_string(), r[corte..].to_vec())
    };
    let (cab, corpo) = get("/", "Accept: text/html,application/xhtml+xml\r\n");
    assert!(cab.starts_with("HTTP/1.1 200") && String::from_utf8_lossy(&corpo).contains("id=\"root\""), "{cab}");
    assert!(cab.contains("Content-Security-Policy: default-src 'self'") && cab.contains("Cache-Control: no-cache") && cab.contains("nosniff"), "{cab}");
    let (cab, corpo) = get("/assets/app-Ab12.js", "Accept-Encoding: gzip, deflate\r\n");
    assert!(cab.contains("Content-Encoding: gzip") && cab.contains("immutable") && cab.contains("text/javascript"), "{cab}");
    let mut aberto = String::new();
    flate2::read::GzDecoder::new(&corpo[..]).read_to_string(&mut aberto).unwrap();
    assert_eq!(aberto, js, "o ficheiro comprimido abre igual ao original");
    assert!(get("/assets/nao-existe.js", "").0.starts_with("HTTP/1.1 404"), "um .js que falta não pode vir como HTML");
    assert!(get("/../rede.json", "Accept: text/html\r\n").0.starts_with("HTTP/1.1 400"));
    let (cab, corpo) = get("/rede/info", "Accept: */*\r\n");
    let info: Value = serde_json::from_slice(&corpo).unwrap();
    assert!(cab.contains("application/json") && info["web"] == true && info["sal"].as_str().is_some_and(|s| !s.is_empty()), "{info}");
    // o que mexe na máquina onde corre não se faz a partir de outro posto ou do browser
    for m in ["rede.estado", "atualizacao.verificar", "sistema.exportar", "internet.estado", "partilha.abrir"] {
        let e = pedir_v2("t1", m, json!({})).unwrap_err();
        assert!(e.contains("só se usa no próprio computador"), "{m}: {e}");
    }
    // o browser leva o ficheiro que gerou no servidor (e só esse posto o pode levar)
    pedir_v2("t5", "auth.login", json!({ "login": "admin", "senha": "1234" })).unwrap();
    let copia = pedir_v2("t5", "sistema.backup", Value::Null).unwrap();
    let ficheiro = copia["ficheiro"].as_str().unwrap().to_string();
    let levado = pedir_v2("t5", "ficheiros.descarregar", json!({ "ficheiro": ficheiro })).unwrap();
    let bytes = B64.decode(levado["base64"].as_str().unwrap()).unwrap();
    assert_eq!(bytes, std::fs::read(&ficheiro).unwrap());
    assert!(pedir_v2("t1", "ficheiros.descarregar", json!({ "ficheiro": ficheiro })).unwrap_err().contains("indisponível"), "outro posto não leva o ficheiro");
    assert!(pedir_v2("t5", "ficheiros.descarregar", json!({ "ficheiro": "/etc/passwd" })).unwrap_err().contains("indisponível"));
    // escolher o posto a partir de outro aparelho não muda o posto da máquina do servidor
    let antes = std::fs::read_to_string(pasta.join("posto.json")).ok();
    assert_eq!(pedir_v2("t5", "posto.selecionar", json!({ "id": posto2 })).unwrap()["id"], posto2);
    assert_eq!(std::fs::read_to_string(pasta.join("posto.json")).ok(), antes, "o posto escolhido é do aparelho, não do servidor");
    assert_eq!(arc.posto().id, 1);

    // ---- quem erra a chave muitas vezes fica bloqueado uns minutos ----
    for _ in 0..6 {
        let _ = crate::rede::pedir_com_token(&errado, "t3", "setup.estado", Value::Null, posto2, "Caixa 3");
    }
    let bloqueado = pedir_v2("t1", "artigos.listar", json!({})).unwrap_err();
    assert!(bloqueado.contains("Demasiadas tentativas"), "{bloqueado}");

    arc.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn rh_honorarios_prestacoes() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-hon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Consultoria", "nif": "5000000001", "endereco": "Rua 3", "localidade": "Luanda" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let (ano, mes) = (2026i64, 3i64);

    // Assalariado com prestação social que suspende o mês inteiro → fora do processamento.
    let ana = rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Ana", "salario_base": 20000000, "desconta_inss": true, "desconta_irt": true })).await["id"].as_i64().unwrap();
    let rui = rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Rui", "salario_base": 15000000, "desconta_inss": true, "desconta_irt": true })).await["id"].as_i64().unwrap();
    let ind = rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Carla (consultora)", "independente": true, "honorario": 10000000, "tipo_honorario": "MENSAL" })).await["id"].as_i64().unwrap();
    let hora = rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Pedro (formador)", "independente": 1, "honorario": 500000, "tipo_honorario": "HORA" })).await["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "rh.prestacaoGuardar", json!({ "funcionario_id": ana, "tipo": "Maternidade", "data_inicio": "2026-03-31", "data_fim": "2026-03-01" })).await.contains("período"));
    let pr = rpc(&st, "rh.prestacaoGuardar", json!({ "funcionario_id": ana, "tipo": "Maternidade", "data_inicio": "2026-02-15", "data_fim": "2026-05-15", "meses": 3 })).await["id"].as_i64().unwrap();
    let sim = rpc(&st, "rh.simular", json!({ "ano": ano, "mes": mes })).await;
    let nomes: Vec<_> = sim.as_array().unwrap().iter().map(|r| r["nome"].as_str().unwrap().to_string()).collect();
    assert_eq!(nomes, vec!["Rui"], "independentes e funcionários suspensos não entram no salário");
    assert_eq!(rpc(&st, "rh.prestacoes", json!({ "ano": ano })).await.as_array().unwrap().len(), 1);
    rpc(&st, "rh.prestacaoGuardar", json!({ "id": pr, "funcionario_id": ana, "tipo": "Maternidade", "data_inicio": "2026-02-15", "data_fim": "2026-05-15", "suspensao": false })).await;
    assert_eq!(rpc(&st, "rh.simular", json!({ "ano": ano, "mes": mes })).await.as_array().unwrap().len(), 2);
    rpc(&st, "rh.prestacaoEliminar", json!({ "id": pr })).await;
    assert_eq!(rpc(&st, "rh.prestacoes", json!({ "ano": ano })).await.as_array().unwrap().len(), 0);
    let _ = rui;

    // Honorários: IRT Grupo B à taxa configurada (6,5%).
    assert!(rpc_err(&st, "rh.honorarioProcessar", json!({ "funcionario_id": ana, "ano": ano, "mes": mes, "quantidade": 1 })).await.contains("conta própria"));
    let h = rpc(&st, "rh.honorarioProcessar", json!({ "funcionario_id": ind, "ano": ano, "mes": mes, "quantidade": 0 })).await;
    assert_eq!(h["iliquido"], 10000000);
    assert_eq!(h["irt"], 650000);
    assert_eq!(h["liquido"], 9350000);
    assert!(rpc_err(&st, "rh.honorarioProcessar", json!({ "funcionario_id": ind, "ano": ano, "mes": mes, "quantidade": 1 })).await.contains("já foi processado"));
    assert!(rpc_err(&st, "rh.honorarioProcessar", json!({ "funcionario_id": hora, "ano": ano, "mes": mes, "quantidade": 0 })).await.contains("horas"));
    let h2 = rpc(&st, "rh.honorarioProcessar", json!({ "funcionario_id": hora, "ano": ano, "mes": mes, "quantidade": 12.5, "descricao": "Formação POS" })).await;
    assert_eq!(h2["iliquido"], 6250000);
    assert_eq!(h2["irt"], 406250);
    let lista = rpc(&st, "rh.honorarios", json!({ "ano": ano, "mes": mes })).await;
    assert_eq!(lista["honorarios"].as_array().unwrap().len(), 2);
    assert_eq!(lista["independentes"].as_array().unwrap().len(), 2);
    assert_eq!(lista["irt_pct"], 6.5);

    // Pagamento pela tesouraria, anulação do pagamento e anulação do honorário.
    let hid = h["id"].as_i64().unwrap();
    rpc(&st, "rh.honorarioPagar", json!({ "id": hid, "conta_id": 1 })).await;
    assert!(rpc_err(&st, "rh.honorarioPagar", json!({ "id": hid, "conta_id": 1 })).await.contains("já pago"));
    assert!(rpc_err(&st, "rh.honorarioAnular", json!({ "id": hid, "motivo": "erro" })).await.contains("Fluxo de Caixa"));
    let mov: i64 = sqlx::query_scalar("SELECT id FROM movimentos_tesouraria WHERE documento = ?").bind(format!("HON-{hid}")).fetch_one(&st.pool).await.unwrap();
    let valor: i64 = sqlx::query_scalar("SELECT valor FROM movimentos_tesouraria WHERE id = ?").bind(mov).fetch_one(&st.pool).await.unwrap();
    assert_eq!(valor, 9350000);
    rpc(&st, "tesouraria.anular", json!({ "id": mov, "motivo": "conta errada" })).await;
    rpc(&st, "rh.honorarioAnular", json!({ "id": hid, "motivo": "valor errado" })).await;
    rpc(&st, "rh.honorarioProcessar", json!({ "funcionario_id": ind, "ano": ano, "mes": mes, "quantidade": 1, "valor_base": 12000000 })).await;
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn recalculo_stock() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-recalc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja", "nif": "5000000002" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "taxa_iva_id": 1, "preco1": 500000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": art, "quantidade": 10, "preco_unitario": 300000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "SS", "linhas": [{ "artigo_id": art, "quantidade": 3, "preco_unitario": 300000 }] })).await;
    assert_eq!(stock(&st, art).await, 7.0);
    let r = rpc(&st, "stock.recalcular", json!({})).await;
    assert_eq!(r["diferencas"].as_array().unwrap().len(), 0, "sem diferenças depois de movimentos normais");

    // Estraga o stock gravado e o saldo de um movimento.
    sqlx::query("UPDATE stock SET quantidade = 99 WHERE artigo_id = ?").bind(art).execute(&st.pool).await.unwrap();
    sqlx::query("UPDATE movimentos_stock SET saldo = 0 WHERE artigo_id = ?").bind(art).execute(&st.pool).await.unwrap();
    let r = rpc(&st, "stock.recalcular", json!({ "aplicar": false })).await;
    assert_eq!(r["diferencas"][0]["gravado"], 99.0);
    assert_eq!(r["diferencas"][0]["calculado"], 7.0);
    assert_eq!(stock(&st, art).await, 99.0, "a análise não altera nada");
    let r = rpc(&st, "stock.recalcular", json!({ "aplicar": true })).await;
    assert_eq!(r["saldos_corrigidos"], 2);
    assert_eq!(stock(&st, art).await, 7.0);
    let saldos: Vec<f64> = sqlx::query_scalar("SELECT saldo FROM movimentos_stock WHERE artigo_id = ? ORDER BY id").bind(art).fetch_all(&st.pool).await.unwrap();
    assert_eq!(saldos, vec![10.0, 7.0]);
    let aud: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE acao = 'RECALCULO_STOCK'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(aud, 1);

    // Mapas de imposto por dia e por família.
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 1140000 }] })).await;
    let hoje = db::hoje();
    let dia = rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "iva_dia" })).await;
    assert_eq!(dia.as_array().unwrap().len(), 1);
    assert!(dia[0]["chave"].as_str().unwrap().starts_with(&hoje));
    assert_eq!(dia[0]["liquido"], 1000000);
    assert_eq!(dia[0]["iva"], 140000);
    let fam = rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "iva_familia" })).await;
    assert_eq!(fam[0]["iva"], 140000);
    assert!(fam[0]["chave"].as_str().unwrap().contains("IVA 14"));
    let vend = rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "vendedor" })).await;
    assert_eq!(vend[0]["chave"], "(sem vendedor)");
    assert_eq!(vend[0]["total"], 1140000);
    // IRT Grupo C: 6,5% do volume de vendas sem IVA (1 000 000,00 → 65 000,00), taxa editável
    let c = rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "irt_c" })).await;
    assert_eq!(c[0]["chave"], &hoje[..7]);
    assert_eq!(c[0]["liquido"], 1000000);
    assert_eq!(c[0]["irt"], 65000);
    assert_eq!(rpc(&st, "relatorios.irtC", Value::Null).await["taxa"], 6.5);
    assert!(rpc_err(&st, "relatorios.irtCGuardar", json!({ "taxa": 150 })).await.contains("Taxa"));
    rpc(&st, "relatorios.irtCGuardar", json!({ "taxa": 10 })).await;
    assert_eq!(rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "irt_c" })).await[0]["irt"], 100000);
    let info = rpc(&st, "sistema.info", Value::Null).await;
    assert_eq!(info["documentos"], 3);
    assert!(info["migracoes"].as_i64().unwrap() >= 8);
    let _ = std::fs::remove_dir_all(&pasta);
}

/// SAF-T de inventários: existências a 31/12 reconstituídas pelos movimentos (exercício fechado) ou as de hoje
/// (exercício a decorrer), valorizadas ao custo médio; sem existências, «sem operações».
#[tokio::test]
async fn saft_inventario() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-inventario-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Armazém Teste, Lda", "nif": "5417189230" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let ano: i32 = db::hoje()[..4].parse().unwrap();

    // sem artigos: entrega-se na mesma, «sem operações»
    let v = rpc(&st, "saft.inventario", json!({ "ano": ano })).await;
    assert_eq!((v["sem_operacoes"].as_bool(), v["empresa"]["nif"].as_str()), (Some(true), Some("5417189230")), "{v}");

    // 10 unidades entradas em Junho do ano passado a 3 000,00; este ano saem 4
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "taxa_iva_id": 1, "preco1": 450000 })).await["id"].as_i64().unwrap();
    let s = rpc(&st, "artigos.guardar", json!({ "descricao": "Montagem", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": a, "quantidade": 10, "preco_unitario": 300000 }] })).await;
    sqlx::query("UPDATE movimentos_stock SET data_hora = ? WHERE artigo_id = ?")
        .bind(format!("{}-06-01T10:00:00", ano - 1))
        .bind(a)
        .execute(&st.pool)
        .await
        .unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "SS", "linhas": [{ "artigo_id": a, "quantidade": 4 }] })).await;

    // exercício fechado: o que havia a 31/12, ao custo médio dessa data
    let ant = rpc(&st, "saft.inventario", json!({ "ano": ano - 1 })).await;
    assert_eq!(ant["provisorio"], false);
    assert_eq!(ant["total_artigos"], 1, "os serviços não entram no inventário: {ant}");
    assert_eq!(ant["linhas"][0]["quantidade"].as_f64(), Some(10.0));
    assert_eq!(ant["linhas"][0]["valor"], 3000000);
    assert_eq!(ant["data_referencia"], format!("{}-12-31", ano - 1));
    assert_eq!(ant["metodo_custeio"], "Custo médio ponderado");
    // exercício a decorrer: as existências de hoje
    let atual = rpc(&st, "saft.inventario", json!({ "ano": ano })).await;
    assert_eq!((atual["provisorio"].as_bool(), atual["linhas"][0]["quantidade"].as_f64()), (Some(true), Some(6.0)), "{atual}");
    // antes de haver movimentos: sem operações; um ano futuro é recusado
    assert_eq!(rpc(&st, "saft.inventario", json!({ "ano": ano - 2 })).await["sem_operacoes"], true);
    assert!(rpc_err(&st, "saft.inventario", json!({ "ano": ano + 1 })).await.contains("exercício"));
    let _ = s;
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn parametrizacao_documentos() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-param-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja", "nif": "5000000003" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();

    let par = rpc(&st, "parametros.obter", Value::Null).await;
    assert!(par["tipos"].as_array().unwrap().iter().any(|t| t["codigo"] == "FR" && t["repetir_linhas"] == 1));
    assert!(!par["modos"].as_array().unwrap().iter().any(|m| m["codigo"] == "CS"), "a compensação não aparece nas opções de venda");
    assert_eq!(par["opcoes"]["mostrar_bancos"], true);
    assert_eq!(par["opcoes"]["valor_extenso"], false);

    // Parametrização do tipo: validade, formato e textos.
    assert!(rpc_err(&st, "parametros.tipoGuardar", json!({ "codigo": "PP", "descricao": "Proforma", "vias": 0 })).await.contains("vias"));
    assert!(rpc_err(&st, "parametros.tipoGuardar", json!({ "codigo": "PP", "descricao": "Proforma", "vias": 1, "formato": "A3" })).await.contains("Formato"));
    rpc(&st, "parametros.tipoGuardar", json!({ "codigo": "PP", "descricao": "Factura Pró-forma", "vias": 2, "validade_dias": 15, "repetir_linhas": false,
                                              "prefixo_extenso": "São", "texto_rodape": "Preços válidos 15 dias.", "formato": "A5" })).await;
    let pp = rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": 1, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    let hoje = db::hoje();
    let esperado = (chrono::NaiveDate::parse_from_str(&hoje, "%Y-%m-%d").unwrap() + chrono::Duration::days(15)).format("%Y-%m-%d").to_string();
    assert_eq!(pp["documento"]["data_vencimento"], esperado.as_str(), "a validade do tipo define o «válido até»");
    assert_eq!(pp["documento"]["tipo_desc"], "Factura Pró-forma");
    assert_eq!(pp["documento"]["formato"], "A5");
    assert_eq!(pp["documento"]["prefixo_extenso"], "São");
    assert_eq!(pp["documento"]["vias"], 2);
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    assert_eq!(ft["documento"]["data_vencimento"], hoje.as_str(), "sem validade, vence no próprio dia");

    // Opções de venda: a transferência passa a entrar na conta Banco.
    assert!(rpc_err(&st, "parametros.modoGuardar", json!({ "codigo": "TB", "conta_id": 999 })).await.contains("Conta"));
    assert!(rpc_err(&st, "parametros.modoGuardar", json!({ "codigo": "NU", "conta_id": 1, "ativo": false })).await.contains("numerário"));
    rpc(&st, "parametros.modoGuardar", json!({ "codigo": "TB", "conta_id": 2 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "TB", "valor": 228000 }] })).await;
    let contas = rpc(&st, "tesouraria.contas", Value::Null).await;
    let saldo = |id: i64| contas.as_array().unwrap().iter().find(|c| c["id"] == id).unwrap()["saldo"].as_i64().unwrap();
    assert_eq!(saldo(2), 228000);
    assert_eq!(saldo(1), 114000);

    // Definição de facturas e recibos.
    rpc(&st, "parametros.opcoesGuardar", json!({ "valor_extenso": true, "assinaturas": true, "mostrar_bancos": false })).await;
    let d = rpc(&st, "documentos.obter", json!({ "id": ft["documento"]["id"] })).await;
    assert_eq!(d["opcoes"]["valor_extenso"], true);
    assert_eq!(d["opcoes"]["assinaturas"], true);
    assert_eq!(d["opcoes"]["mostrar_bancos"], false);
    assert_eq!(d["opcoes"]["mostrar_operador"], true);
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Servidor local que imita os serviços `sigt/fe/v1` da AGT e guarda os pedidos recebidos.
type PedidosAgt = std::sync::Arc<std::sync::Mutex<Vec<(String, String, Value)>>>;

fn agt_simulada() -> (u16, PedidosAgt) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = l.local_addr().unwrap().port();
    let pedidos = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String, Value)>::new()));
    let reg = pedidos.clone();
    std::thread::spawn(move || {
        let mut lotes: Vec<Value> = Vec::new();
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut b = Vec::new();
            let mut buf = [0u8; 16384];
            let (cab, corpo) = loop {
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break (String::new(), Vec::new());
                }
                b.extend_from_slice(&buf[..n]);
                if let Some(f) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                    let cab = String::from_utf8_lossy(&b[..f]).to_string();
                    let tam: usize = cab.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                    while b.len() < f + 4 + tam {
                        let n = s.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        b.extend_from_slice(&buf[..n]);
                    }
                    break (cab, b[f + 4..].to_vec());
                }
            };
            let caminho = cab.split_whitespace().nth(1).unwrap_or("").to_string();
            let auth = cab.lines().find_map(|l| l.strip_prefix("Authorization: ").map(String::from)).unwrap_or_default();
            let v: Value = serde_json::from_slice(&corpo).unwrap_or(Value::Null);
            reg.lock().unwrap().push((caminho.clone(), auth, v.clone()));
            let resp = if caminho.starts_with("/erro/") {
                // recusa com erro que não identifica nenhum documento enviado
                let t = json!({ "errorList": [{ "idError": "E10", "descriptionError": "Dados do cliente incorrectos", "documentNo": "NAO EXISTE/1" }] }).to_string();
                let _ = s.write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}", t.len(), t).as_bytes());
                continue;
            } else if caminho.starts_with("/semid/") {
                json!({ "resultado": "ok" })
            } else if caminho.ends_with("/registarFactura") {
                lotes.push(v);
                json!({ "requestID": format!("2026{:011}", lotes.len()) })
            } else if caminho.ends_with("/obterEstado") {
                let req = v["requestID"].as_str().unwrap_or("");
                let k: usize = req[4..].parse().unwrap_or(0);
                let docs = lotes.get(k.wrapping_sub(1)).map(|l| l["documents"].as_array().cloned().unwrap_or_default()).unwrap_or_default();
                let lista: Vec<Value> = docs
                    .iter()
                    .map(|d| {
                        if d["customerTaxID"] == "5417000009" {
                            json!({ "documentNo": d["documentNo"], "documentStatus": "I", "errorList": [{ "errorCode": "E09", "errorDescription": "Factura já existe no sistema" }] })
                        } else if d["customerTaxID"] == "999999999" || d["documentStatus"] == "C" {
                            json!({ "documentNo": d["documentNo"], "documentStatus": "V" })
                        } else {
                            json!({ "documentNo": d["documentNo"], "documentStatus": "I", "errorList": [{ "errorCode": "E10", "errorDescription": "Dados fiscais do cliente incorrectos" }] })
                        }
                    })
                    .collect();
                json!({ "requestID": req, "resultCode": 1, "documentStatusList": lista })
            } else if caminho.ends_with("/solicitarSerie") {
                json!({ "resultCode": 1, "seriesFEResult": { "seriesCode": format!("{}{}AGT1N", v["documentType"].as_str().unwrap_or(""), v["seriesYear"]), "authorizedQuantity": "999999", "firstDocumentNo": "1", "lastDocumentNo": "999999" } })
            } else {
                json!({ "errorList": [{ "idError": "E96", "descriptionError": "Erro de estrutura" }] })
            };
            let t = resp.to_string();
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}", t.len(), t).as_bytes());
        }
    });
    (porta, pedidos)
}

#[tokio::test(flavor = "multi_thread")]
async fn facturacao_electronica() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    use sha2::Sha256;
    let pasta = std::env::temp_dir().join(format!("kigest-teste-fe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja FE, Lda", "nif": "5001636863" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Consultoria", "tipo": "S", "taxa_iva_id": 1, "preco1": 25000 })).await["id"].as_i64().unwrap();
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Exemplo Lda", "nif": "5417000001" })).await["id"].as_i64().unwrap();
    let (porta, pedidos) = agt_simulada();
    let ano: i64 = db::hoje()[..4].parse().unwrap();

    let mut rng = rand::thread_rng();
    let k_soft = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let k_contr = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let pem = |k: &rsa::RsaPrivateKey| k.to_pkcs8_pem(LineEnding::LF).unwrap().to_string();
    assert!(rpc_err(&st, "fe.guardar", json!({ "ativo": true, "ambiente": "HML", "utilizador": "", "numero_validacao": "", "estabelecimento": "SEDE" })).await.contains("utilizador"));
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('fe.endereco', ?)").bind(format!("http://127.0.0.1:{porta}/sigt/fe/v1")).execute(&st.pool).await.unwrap();
    rpc(&st, "fe.guardar", json!({ "ativo": true, "ambiente": "HML", "utilizador": "kigest", "senha": "segredo", "numero_validacao": "C_999", "estabelecimento": "SEDE",
                                  "chave_software": pem(&k_soft), "chave_contribuinte": pem(&k_contr) })).await;
    let est = rpc(&st, "fe.estado", Value::Null).await;
    assert_eq!(est["ativo"], true);
    assert_eq!(est["senha"], true, "a senha nunca é devolvida, só se está definida");
    assert!(est.get("chave_contribuinte").unwrap().as_bool().unwrap());

    // sem série da AGT não se emite
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 57000 }] })).await.contains("série da AGT"));
    let s = rpc(&st, "fe.solicitarSerie", json!({ "tipo": "FR", "ano": ano })).await;
    assert_eq!(s["serie"], format!("FR{ano}AGT1N"));
    rpc(&st, "fe.solicitarSerie", json!({ "tipo": "FT", "ano": ano })).await;
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 57000 }] })).await;
    assert_eq!(fr["documento"]["numero_doc"], format!("FR FR{ano}AGT1N/1"));
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    let ft_id = ft["documento"]["id"].as_i64().unwrap();
    // uma série que não é da AGT é recusada
    sqlx::query("INSERT INTO series (tipo_codigo, serie) VALUES ('FR', 'X2026')").execute(&st.pool).await.unwrap();
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "serie": "X2026", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 28500 }] })).await.contains("não foi atribuída pela AGT"));

    // envio em lote
    let r = rpc(&st, "fe.processar", Value::Null).await;
    assert_eq!(r["enviados"], 2);
    let (caminho, auth, corpo) = pedidos.lock().unwrap().iter().find(|p| p.0.ends_with("/registarFactura")).cloned().unwrap();
    assert_eq!(caminho, "/sigt/fe/v1/registarFactura");
    assert_eq!(auth, format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("kigest:segredo")));
    assert_eq!(corpo["numberOfEntries"], 2);
    assert_eq!(corpo["taxRegistrationNumber"], "5001636863");
    assert_eq!(corpo["schemaVersion"], "2.0");
    let verifica = |k: &rsa::RsaPrivateKey, t: &str| -> String {
        use rsa::signature::Verifier;
        let p: Vec<&str> = t.split('.').collect();
        let sig = rsa::pkcs1v15::Signature::try_from(URL_SAFE_NO_PAD.decode(p[2]).unwrap().as_slice()).unwrap();
        rsa::pkcs1v15::VerifyingKey::<Sha256>::new(k.to_public_key()).verify(format!("{}.{}", p[0], p[1]).as_bytes(), &sig).expect("assinatura RS256");
        String::from_utf8(URL_SAFE_NO_PAD.decode(p[1]).unwrap()).unwrap()
    };
    assert_eq!(verifica(&k_soft, corpo["softwareInfo"]["jwsSoftwareSignature"].as_str().unwrap()), format!(r#"{{"productId":"KanGest","productVersion":"{}","softwareValidationNumber":"C_999"}}"#, env!("CARGO_PKG_VERSION")));
    let d0 = &corpo["documents"][0];
    assert_eq!(d0["documentNo"], format!("FR FR{ano}AGT1N/1"));
    assert_eq!(d0["documentType"], "FR");
    assert_eq!(d0["documentStatus"], "N");
    assert_eq!(d0["customerTaxID"], "999999999");
    assert_eq!(d0["documentTotals"], json!({ "taxPayable": 70, "netTotal": 500, "grossTotal": 570 }));
    assert_eq!(
        verifica(&k_contr, d0["jwsDocumentSignature"].as_str().unwrap()),
        super::fe::payload_documento(d0["documentNo"].as_str().unwrap(), "5001636863", "FR", d0["documentDate"].as_str().unwrap(), "999999999", "AO", "Consumidor Final", 7000, 50000, 57000)
    );
    let l0 = &d0["lines"][0];
    assert_eq!(l0["operationType"], "SG", "serviço");
    assert_eq!(l0["quantity"], 2);
    assert_eq!(l0["unitPriceBase"], 250);
    assert_eq!(l0["creditAmount"], 500);
    assert_eq!(l0["taxes"][0], json!({ "taxType": "IVA", "taxCountryRegion": "AO", "taxCode": "NOR", "taxPercentage": 14, "taxContribution": 70 }));
    assert_eq!(corpo["documents"][1]["customerTaxID"], "5417000001");

    // validação assíncrona: a FR fica válida, a FT rejeitada (E10)
    rpc(&st, "fe.processar", Value::Null).await;
    let env = rpc(&st, "fe.envios", json!({})).await;
    let estado_de = |id: i64| env.as_array().unwrap().iter().find(|e| e["documento_id"] == id).unwrap().clone();
    assert_eq!(estado_de(fr["documento"]["id"].as_i64().unwrap())["estado"], "VALIDO");
    let e_ft = estado_de(ft_id);
    assert_eq!(e_ft["estado"], "INVALIDO");
    assert!(e_ft["erros"].as_str().unwrap().contains("E10"));
    let impresso = rpc(&st, "documentos.obter", json!({ "id": fr["documento"]["id"] })).await;
    assert_eq!(impresso["fe"], json!({ "estado": "VALIDO", "nif": "5001636863" }));

    // anulação: o documento aceite pela AGT não se anula só no KanGest; o rejeitado sim (fica para o fim)
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": fr["documento"]["id"], "motivo": "Engano no artigo" })).await.contains("nota de crédito"));
    // correcção: documento novo com documentStatus C e o número rejeitado
    assert!(rpc_err(&st, "fe.reenviar", json!({ "documento_id": ft_id })).await.contains("Corrigir"));
    let ft2 = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    rpc(&st, "fe.corrigir", json!({ "rejeitado_id": ft_id, "numero_novo": ft2["documento"]["numero_doc"] })).await;
    rpc(&st, "fe.processar", Value::Null).await;
    let ultimo = pedidos.lock().unwrap().iter().rfind(|p| p.0.ends_with("/registarFactura")).cloned().unwrap();
    let dc = &ultimo.2["documents"][0];
    assert_eq!(dc["documentStatus"], "C");
    assert_eq!(dc["rejectedDocumentNo"], ft["documento"]["numero_doc"]);
    rpc(&st, "fe.processar", Value::Null).await;
    assert_eq!(rpc(&st, "fe.envios", json!({ "estado": "VALIDO" })).await.as_array().unwrap().len(), 2);
    rpc(&st, "documentos.anular", json!({ "id": ft_id, "motivo": "Rejeitada pela AGT, substituída" })).await;

    // respostas anómalas da AGT não bloqueiam o ciclo nem perdem documentos
    let nova = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 28500 }] })).await;
    let nova_id = nova["documento"]["id"].as_i64().unwrap();
    let estado_fe = |id: i64| {
        let p = st.pool.clone();
        async move { sqlx::query_as::<_, (String, Option<String>)>("SELECT estado, erros FROM fe_envios WHERE documento_id = ?").bind(id).fetch_one(&p).await.unwrap() }
    };
    let endereco = |pfx: &str| format!("http://127.0.0.1:{porta}{pfx}/sigt/fe/v1");
    sqlx::query("UPDATE config SET valor = ? WHERE chave = 'fe.endereco'").bind(endereco("/erro")).execute(&st.pool).await.unwrap();
    let r = rpc(&st, "fe.processar", Value::Null).await;
    assert!(r["avisos"][0].as_str().unwrap().contains("E10"), "erro com n.º desconhecido: termina o ciclo e avisa");
    assert_eq!(estado_fe(nova_id).await.0, "PENDENTE");
    sqlx::query("UPDATE config SET valor = ? WHERE chave = 'fe.endereco'").bind(endereco("/semid")).execute(&st.pool).await.unwrap();
    let r = rpc(&st, "fe.processar", Value::Null).await;
    assert!(r["avisos"][0].as_str().unwrap().contains("sem n.º de pedido"));
    assert_eq!(estado_fe(nova_id).await.0, "PENDENTE", "sem requestID não fica «em validação» para sempre");
    sqlx::query("UPDATE config SET valor = ? WHERE chave = 'fe.endereco'").bind(endereco("")).execute(&st.pool).await.unwrap();
    // E09 (já existe na AGT) conta como válido
    let dup = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Duplicado", "nif": "5417000009" })).await["id"].as_i64().unwrap();
    let fd = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": dup, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    rpc(&st, "fe.processar", Value::Null).await;
    rpc(&st, "fe.processar", Value::Null).await;
    assert_eq!(estado_fe(nova_id).await.0, "VALIDO");
    assert_eq!(estado_fe(fd["documento"]["id"].as_i64().unwrap()).await, ("VALIDO".to_string(), None));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Volume realista (corre com `cargo test -- --ignored importacao_volume`): mede o tempo da importação.
#[tokio::test]
#[ignore]
async fn importacao_volume() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-volume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Volume", "nif": "5000000009" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let (n_cli, n_art, n_doc, n_rec) = (2000i64, 2000i64, 10000i64, 3000i64);
    let mut t: std::collections::HashMap<String, Vec<Registo>> = std::collections::HashMap::new();
    t.insert("tbzconfigTaxasIva".into(), vec![reg(&[("Id", Val::I(1)), ("Taxa", Val::F(14.0)), ("Descricao", s("IVA 14%"))])]);
    t.insert("tbArtigos".into(), (1..=n_art).map(|k| reg(&[("IdArtigo", Val::I(k)), ("Referencia", s(&format!("A{k}"))), ("Descricao", s(&format!("Artigo {k}"))), ("IdTaxa", Val::I(1)), ("Preco1", Val::F(100.0))])).collect());
    t.insert("tbClientes".into(), (1..=n_cli).map(|k| reg(&[("IdCliente", Val::I(k + 1)), ("Nome", s(&format!("Cliente {k}"))), ("NIF", s(&format!("54{k:08}")))])).collect());
    t.insert("tbzconfigTiposDocumentos".into(), vec![reg(&[("Id", Val::I(1)), ("Descricao", s("Fatura")), ("Abreviatura", s("FT"))])]);
    t.insert(
        "tbDocumentosCabessalho".into(),
        (1..=n_doc).map(|k| reg(&[("RefDocumento", s(&format!("FT 2025/{k}"))), ("IdTipoDocumento", Val::I(1)), ("DataEmissao", s("2025-06-01 00:00:00")), ("IdEntidade", Val::I(k % n_cli + 2)), ("ValTotal", Val::F(342.0)), ("ValTaxa", Val::F(42.0))])).collect(),
    );
    t.insert(
        "tbDocumentosDetalhe".into(),
        (1..=n_doc).flat_map(|k| (0..3).map(move |j| reg(&[("RefDocumento", s(&format!("FT 2025/{k}"))), ("IdArtigo", Val::I((k * 3 + j) % n_art + 1)), ("Qtd", Val::F(1.0)), ("ValorUnit", Val::F(100.0)), ("Taxaporcento", Val::F(14.0)), ("ValorLiquido", Val::F(100.0)), ("ValorTaxa", Val::F(14.0)), ("ValorTotal", Val::F(114.0))]))).collect(),
    );
    t.insert("tbContasCorrente".into(), (1..=n_doc).step_by(4).map(|k| reg(&[("Documento", s(&format!("FT 2025/{k}"))), ("ValorFalta", Val::F(342.0))])).collect());
    t.insert("vsSAFT_REC_Cabecalho".into(), (1..=n_rec).map(|k| reg(&[("NumeroRecibo", s(&format!("RC 2025/{k}"))), ("Data", s("2025-07-01 00:00:00")), ("EntidadeId", Val::I(k % n_cli + 2)), ("Total", Val::F(342.0))])).collect());
    t.insert("vsSAFT_ReciboLinha".into(), (1..=n_rec).map(|k| reg(&[("DocumentoReferencia", s(&format!("RC 2025/{k}"))), ("DocumentoOrigem", s(&format!("FT 2025/{}", k * 3))), ("ValorLiquido", Val::F(342.0))])).collect());
    let opcoes = Opcoes { empresa: true, artigos: true, stock: true, clientes: true, fornecedores: true, utilizadores: true, documentos: true, rh: true, lotes: true, oficina: true, tesouraria: true, simular: false };
    let mut fonte = Memoria(t);
    let t0 = std::time::Instant::now();
    let r = importar_dados(&st, &mut fonte, &opcoes, "volume").await.expect("importação");
    let primeira = t0.elapsed();
    let t1 = std::time::Instant::now();
    importar_dados(&st, &mut fonte, &opcoes, "volume").await.expect("reimportação");
    let segunda = t1.elapsed();
    eprintln!("importação: {primeira:?} · reimportação: {segunda:?} · {}", r["contagens"]);
    let docs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documentos").fetch_one(&st.pool).await.unwrap();
    let linhas: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documento_linhas").fetch_one(&st.pool).await.unwrap();
    assert_eq!((docs, linhas), (n_doc + n_rec, n_doc * 3));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Servidor de demonstração para a auditoria visual (corre com
/// `cargo test servidor_demo -- --ignored --nocapture`): base temporária com dados de exemplo,
/// servidor de rede na porta 7799 (chave `auditoria123`) e o posto com token `auditoria` já autenticado.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn servidor_demo() {
    let pasta = std::env::temp_dir().join("kigest-auditoria");
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Comércio Kianda, Lda", "nif": "5417189230", "endereco": "Rua Rainha Ginga, 45", "localidade": "Luanda", "telefone": "923 456 789", "email": "geral@kianda.ao" },
                                          "admin": { "nome": "Administrador", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let hoje = db::hoje();
    let (ano, mes): (i64, i64) = (hoje[..4].parse().unwrap(), hoje[5..7].parse().unwrap());

    // cadastros
    let cli1 = rpc(&st, "entidades.guardar", json!({ "nome": "Sonangol Distribuidora", "nif": "5410003456", "endereco": "Av. 4 de Fevereiro", "localidade": "Luanda", "limite_credito": 500000000 })).await["id"].as_i64().unwrap();
    let cli2 = rpc(&st, "entidades.guardar", json!({ "nome": "Hotel Baía Azul", "nif": "5417002211", "localidade": "Benguela" })).await["id"].as_i64().unwrap();
    rpc(&st, "entidades.guardar", json!({ "nome": "Maria da Conceição", "nif": "004567812LA045" })).await;
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Refriango, SA", "nif": "5401112223" })).await["id"].as_i64().unwrap();
    rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Import Global Trading", "nif": "5409998887", "pais": "PT" })).await;
    let mut arts = Vec::new();
    for (d, p, c) in [("Água Pura 1,5L", 25000, 12000), ("Coca-Cola 33cl", 30000, 18000), ("Arroz Tio Lucas 5kg", 450000, 320000), ("Óleo Fula 1L", 180000, 120000), ("Açúcar Branco 1kg", 90000, 60000), ("Leite Nido 400g", 520000, 380000)] {
        arts.push(rpc(&st, "artigos.guardar", json!({ "descricao": d, "taxa_iva_id": 1, "preco1": p, "custo": c })).await["id"].as_i64().unwrap());
    }
    let serv = rpc(&st, "artigos.guardar", json!({ "descricao": "Instalação e montagem", "tipo": "S", "taxa_iva_id": 1, "preco1": 1500000 })).await["id"].as_i64().unwrap();
    let med = rpc(&st, "artigos.guardar", json!({ "descricao": "Paracetamol 500mg (cx 20)", "taxa_iva_id": 2, "motivo_isencao": "M11", "preco1": 80000, "controla_lote": true })).await["id"].as_i64().unwrap();

    // compras e stock
    rpc(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "FT RF2026/1182", "linhas": arts.iter().map(|a| json!({ "artigo_id": a, "quantidade": 120, "preco_unitario": 10000 })).collect::<Vec<_>>() })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": med, "quantidade": 40, "preco_unitario": 50000, "lote": "PX2611", "validade": format!("{}-03-31", ano + 1) }, { "artigo_id": med, "quantidade": 10, "preco_unitario": 50000, "lote": "PX2604", "validade": format!("{ano}-12-15") }] })).await;

    // vendas
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 2000000 })).await;
    for k in 0..6 {
        let a = arts[k % arts.len()];
        let q = (k + 1) as i64;
        let v = rpc(&st, "artigos.obter", json!({ "id": a })).await["preco1"].as_i64().unwrap();
        let total = (v as f64 * q as f64 * 1.14).round() as i64;
        rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": q }], "pagamentos": [{ "modo": if k % 2 == 0 { "NU" } else { "MB" }, "valor": total }] })).await;
    }
    // vendas dos 29 dias anteriores como histórico importado (fora da cadeia de assinaturas, como o do KIGEST):
    // dão corpo ao gráfico do painel, às comparações com ontem e com o mês anterior e aos mais vendidos
    struct Ficha {
        id: i64,
        referencia: String,
        descricao: String,
        preco: i64,
        custo: i64,
    }
    let admin: i64 = sqlx::query_scalar("SELECT id FROM utilizadores WHERE login = 'admin'").fetch_one(&st.pool).await.unwrap();
    let mut fichas = Vec::new();
    for a in &arts {
        let (referencia, descricao, preco, custo): (String, String, i64, i64) =
            sqlx::query_as("SELECT referencia, descricao, preco1, custo_medio FROM artigos WHERE id = ?").bind(a).fetch_one(&st.pool).await.unwrap();
        fichas.push(Ficha { id: *a, referencia, descricao, preco, custo });
    }
    let mut n_hist = 0;
    for dias in (1..=29i64).rev() {
        let data_doc: String = sqlx::query_scalar("SELECT date('now', 'localtime', ?)").bind(format!("-{dias} day")).fetch_one(&st.pool).await.unwrap();
        for j in 0..(2 + (dias * 7) % 3) {
            n_hist += 1;
            // duas linhas por venda, com artigos e quantidades que variam de dia para dia
            let linhas: Vec<(&Ficha, f64)> = (0..2)
                .map(|l| (&fichas[((dias * 3 + j * 2 + l) % fichas.len() as i64) as usize], (2 + (dias * 5 + j * 3 + l * 7) % 9) as f64))
                .collect();
            let liquido: i64 = linhas.iter().map(|(f, q)| (f.preco as f64 * q).round() as i64).sum();
            let iva = (liquido as f64 * 0.14).round() as i64;
            let doc: i64 = sqlx::query_scalar(
                "INSERT INTO documentos (tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, entidade_nome, entidade_nif, utilizador_id,
                    estado, total_bruto, total_liquido, total_iva, total, valor_entregue, importado, origem)
                 VALUES ('FR', 'H', ?1, 'FR H/' || ?1, ?2, ?2 || 'T' || printf('%02d', 9 + ?3 * 3) || ':15:00', 'Consumidor Final', '999999999', ?4,
                    'N', ?5, ?5, ?6, ?5 + ?6, ?5 + ?6, 1, 'I') RETURNING id",
            )
            .bind(n_hist)
            .bind(&data_doc)
            .bind(j)
            .bind(admin)
            .bind(liquido)
            .bind(iva)
            .fetch_one(&st.pool)
            .await
            .unwrap();
            for (k, (f, q)) in linhas.iter().enumerate() {
                let base = (f.preco as f64 * q).round() as i64;
                let iva_l = (base as f64 * 0.14).round() as i64;
                sqlx::query(
                    "INSERT INTO documento_linhas (documento_id, linha, artigo_id, referencia, descricao, quantidade, unidade, preco_unitario, taxa_iva_id,
                        taxa_iva_pct, valor_liquido, valor_iva, total, custo)
                     VALUES (?, ?, ?, ?, ?, ?, 'UN', ?, 1, 14, ?, ?, ?, ?)",
                )
                .bind(doc)
                .bind(k as i64 + 1)
                .bind(f.id)
                .bind(&f.referencia)
                .bind(&f.descricao)
                .bind(q)
                .bind(f.preco)
                .bind(base)
                .bind(iva_l)
                .bind(base + iva_l)
                .bind(f.custo)
                .execute(&st.pool)
                .await
                .unwrap();
            }
        }
    }
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli1, "linhas": [{ "artigo_id": arts[2], "quantidade": 20 }, { "artigo_id": serv, "quantidade": 1 }] })).await;
    // factura vencida há muito (para os avisos e a antiguidade de saldos): o vencimento passa para Janeiro na base,
    // porque ao emitir já não se aceita um vencimento anterior à data do documento
    let ft_vencida = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli2, "linhas": [{ "artigo_id": arts[5], "quantidade": 8 }] })).await["documento"]["id"].as_i64().unwrap();
    sqlx::query("UPDATE documentos SET data_vencimento = ? WHERE id = ?").bind(format!("{ano}-01-15")).bind(ft_vencida).execute(&st.pool).await.unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": cli2, "linhas": [{ "artigo_id": serv, "quantidade": 2 }] })).await;
    // quadro de propostas: uma proforma já facturada, um orçamento expirado, uma nota de encomenda e uma encomenda a fornecedor
    let pp2 = rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": cli1, "linhas": [{ "artigo_id": arts[0], "quantidade": 40 }] })).await["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli1, "doc_origem_id": pp2, "linhas": [{ "artigo_id": arts[0], "quantidade": 40 }] })).await;
    let orc = rpc(&st, "documentos.emitir", json!({ "tipo": "OR", "entidade_id": cli2, "linhas": [{ "artigo_id": serv, "quantidade": 3 }] })).await["documento"]["id"].as_i64().unwrap();
    sqlx::query("UPDATE documentos SET data_vencimento = date('now', 'localtime', '-5 day') WHERE id = ?").bind(orc).execute(&st.pool).await.unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "NE", "entidade_id": cli1, "linhas": [{ "artigo_id": arts[1], "quantidade": 24 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "VNE", "entidade_id": forn, "linhas": [{ "artigo_id": arts[3], "quantidade": 60, "preco_unitario": 110000 }] })).await;
    let ft_id = ft["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": cli1, "liquidar": [{ "documento_id": ft_id, "valor": 500000 }], "pagamentos": [{ "modo": "TB", "valor": 500000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli1, "doc_origem_id": ft_id, "motivo": "Devolução de 2 sacos danificados", "linhas": [{ "artigo_id": arts[2], "quantidade": 2 }] })).await;

    // RH
    let f1 = rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Ana Paula Domingos", "nif": "001234567LA041", "salario_base": 30000000, "sub_alimentacao": 4000000, "sub_transporte": 2000000, "desconta_inss": true, "desconta_irt": true, "data_admissao": "2021-02-01", "banco": "BAI", "iban": "AO06004000000000000000001" })).await["id"].as_i64().unwrap();
    rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "João Manuel Kiala", "salario_base": 18000000, "sub_alimentacao": 3000000, "desconta_inss": true, "desconta_irt": true, "data_admissao": "2023-07-10" })).await;
    rpc(&st, "rh.funcionarioGuardar", json!({ "nome": "Carlos Tomás (consultor)", "independente": true, "honorario": 25000000, "tipo_honorario": "MENSAL" })).await;
    rpc(&st, "rh.alteracaoGuardar", json!({ "funcionario_id": f1, "ano": ano, "mes": mes, "rubrica": "006", "quantidade": 6 })).await;
    rpc(&st, "rh.processar", json!({ "ano": ano, "mes": mes })).await;

    // oficina e tesouraria
    let viat = rpc(&st, "oficina.viaturaGuardar", json!({ "entidade_id": cli2, "matricula": "LD-45-12-AB", "marca": "Toyota", "modelo": "Hilux", "cor": "Branco", "ano": 2019, "km": 84500 })).await["id"].as_i64().unwrap();
    rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat, "entidade_id": cli2, "avaria": "Ruído na travagem", "diagnostico": "Pastilhas gastas", "estado": "EM_CURSO", "tecnico": "Mário", "linhas": [{ "descricao": "Substituição de pastilhas", "quantidade": 1 }] })).await;
    // mais ordens para o quadro: uma aberta há 10 dias (parada), outra aberta hoje e uma concluída
    let viat2 = rpc(&st, "oficina.viaturaGuardar", json!({ "entidade_id": cli1, "matricula": "LD-12-34-CD", "marca": "Nissan", "modelo": "Navara", "cor": "Cinzento", "ano": 2021, "km": 61200 })).await["id"].as_i64().unwrap();
    let viat3 = rpc(&st, "oficina.viaturaGuardar", json!({ "entidade_id": cli2, "matricula": "LA-77-90-EF", "marca": "Hyundai", "modelo": "Tucson", "cor": "Preto", "ano": 2022, "km": 30400 })).await["id"].as_i64().unwrap();
    let parada = rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat2, "avaria": "Aguarda peça: bomba de água", "estado": "ABERTA" })).await["id"].as_i64().unwrap();
    sqlx::query("UPDATE of_ordens SET data_entrada = datetime('now', 'localtime', '-10 day') WHERE id = ?").bind(parada).execute(&st.pool).await.unwrap();
    rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat3, "avaria": "Revisão dos 30 000 km", "estado": "ABERTA" })).await;
    rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat3, "avaria": "Troca de óleo e filtros", "diagnostico": "Feito", "estado": "CONCLUIDA", "tecnico": "Domingos" })).await;
    rpc(&st, "tesouraria.movimento", json!({ "conta_id": 1, "tipo": "SAIDA", "valor": 15000000, "descricao": "Renda da loja", "centro_custo_id": 4 })).await;

    // utilizadores, perfis e registo de acessos
    rpc(&st, "seguranca.guardar", json!({ "senha_min": 6, "tentativas": 5, "bloqueio_min": 15, "inatividade_min": 10, "login_pin": true })).await;
    rpc(&st, "perfis.guardar", json!({ "descricao": "Armazém", "nivel": 2, "permissoes": ["artigos.criar", "artigos.editar", "stock.entrada", "stock.saida", "stock.transferencia", "stock.inventario", "fornecedores", "relatorios.ver"] })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana Paula Domingos", "login": "ana", "perfil": "CAIXA", "senha": "caixa123", "pin": "4321", "limite_desconto": 5, "mudar_senha": false })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Pedro Lukamba", "login": "pedro", "perfil": "GESTOR", "senha": "gestor123", "pin": "8642", "mudar_senha": false })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Rosa Ngola", "login": "rosa", "perfil": "ARMAZEM", "senha": "provisoria1", "pin": "1357" })).await;
    rpc(&st, "auth.login", json!({ "login": "ana", "senha": "caixa123" })).await;
    rpc(&st, "auth.bloquear", json!({})).await;
    rpc(&st, "auth.desbloquear", json!({ "pin": "4321" })).await;
    rpc(&st, "auth.logout", json!({})).await;
    rpc_err(&st, "auth.login", json!({ "login": "pedro", "senha": "errada1" })).await;
    rpc_err(&st, "auth.login", json!({ "login": "pedro", "senha": "errada2" })).await;
    rpc(&st, "auth.login", json!({ "login": "pedro", "senha": "gestor123" })).await;
    rpc(&st, "auth.logout", json!({})).await;
    rpc_err(&st, "auth.login", json!({ "login": "joao", "senha": "123456" })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // servidor de rede + posto da auditoria já autenticado
    let arc = std::sync::Arc::new(st);
    crate::rede::registar_estado(arc.clone());
    let cfg = crate::rede::Config { modo: "servidor".into(), porta: 7799, servidor: String::new(), chave: "auditoria123".into(), sal: String::new(), web: true };
    crate::rede::iniciar_servidor(arc.clone(), &cfg).expect("servidor");
    let cliente = crate::rede::Config { modo: "cliente".into(), servidor: "127.0.0.1:7799".into(), ..cfg.clone() };
    crate::rede::pedir_com_token(&cliente, "auditoria", "auth.login", json!({ "login": "admin", "senha": "1234" }), 1, "Auditoria").expect("entrar");

    // API de integração (porta 7800, só nesta máquina): uma chave da loja online com alguns pedidos no registo
    let loja = rpc(&arc, "utilizadores.guardar", json!({ "nome": "Loja Online", "login": "loja", "perfil": "CAIXA", "senha": "loja12345", "mudar_senha": false })).await["id"].as_i64().unwrap();
    let posto_w = rpc(&arc, "posto.guardar", json!({ "descricao": "Loja online", "armazem_id": 1, "serie_prefixo": "W" })).await["id"].as_i64().unwrap();
    let k = rpc(&arc, "api.chaveCriar", json!({ "nome": "Loja online (WooCommerce)", "utilizador_id": loja, "posto_id": posto_w,
        "permissoes": ["api.artigos", "api.stock", "api.entidades", "api.documentos", "clientes", "vendas.fatura_recibo", "vendas.fatura", "vendas.proforma"] })).await["chave"].as_str().unwrap().to_string();
    rpc(&arc, "api.chaveCriar", json!({ "nome": "Contabilidade", "utilizador_id": loja, "permissoes": ["api.documentos", "sistema.saft"], "ips": "192.168.1.0/24", "expira_em": format!("{}-12-31", ano + 1) })).await;
    crate::api::registar_estado(arc.clone());
    rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": 7800, "rede": false })).await;
    let ag = rpc(&arc, "artigos.obter", json!({ "id": arts[0] })).await;
    let venda = json!({ "tipo": "FR", "referencia": "WC-1001", "linhas": [{ "artigo_id": arts[0], "quantidade": 3 }], "pagamentos": [{ "modo": "TB", "valor": (ag["preco1"].as_i64().unwrap() as f64 * 3.0 * 1.14).round() as i64 }] });
    for (m, c, b) in [("GET", "/api/v1", None), ("GET", "/api/v1/artigos?alterado_desde=2026-01-01", None), ("POST", "/api/v1/documentos", Some(&venda)), ("GET", "/api/v1/artigos/codigo/000", None), ("POST", "/api/v1/documentos", Some(&json!({ "tipo": "NC" })))] {
        api_http(7800, m, c, Some(&k), b, &[]);
    }
    api_http(7800, "GET", "/api/v1/stock", Some("kg_chave_errada"), None, &[]);
    eprintln!("SERVIDOR-DEMO-PRONTO porta 7799 (API na porta 7800)");
    tokio::time::sleep(std::time::Duration::from_secs(3 * 3600)).await;
}

// =====================================================================================
// Importação a partir do ficheiro do ExportarKIGEST (.kigest = JSON por linhas em gzip)
// =====================================================================================

/// Escreve um ficheiro no formato do `ferramentas/exportar-kigest/ExportarKIGEST.ps1`.
fn escrever_kigest(caminho: &std::path::Path, gzip: bool, objectos: &[(&str, &str, &[&str], Vec<Value>)], erros: &[(&str, &str)], concluido: bool) {
    use std::io::Write;
    let mut txt = String::new();
    txt.push_str(&json!({ "formato": "kigest-exportacao", "versao": 1, "origem": "DESKTOP-G26LDJM/KigestGC", "data": "2026-11-12 09:30:00", "computador": "DESKTOP-G26LDJM", "objectos": objectos.len() }).to_string());
    txt.push('\n');
    let mut total = 0;
    for (nome, tipo, colunas, linhas) in objectos {
        txt.push_str(&json!({ "tabela": nome, "tipo": tipo, "colunas": colunas }).to_string());
        txt.push('\n');
        for l in linhas {
            txt.push_str(&l.to_string());
            txt.push('\n');
        }
        txt.push_str(&json!({ "fim": nome, "registos": linhas.len() }).to_string());
        txt.push('\n');
        total += linhas.len();
    }
    for (nome, erro) in erros {
        txt.push_str(&json!({ "fim": nome, "registos": 0, "erro": erro }).to_string());
        txt.push('\n');
    }
    if concluido {
        txt.push_str(&json!({ "concluido": true, "objectos": objectos.len() + erros.len(), "registos": total }).to_string());
        txt.push('\n');
    }
    let f = std::fs::File::create(caminho).unwrap();
    if gzip {
        let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        gz.write_all(txt.as_bytes()).unwrap();
        gz.finish().unwrap();
    } else {
        let mut f = f;
        f.write_all(txt.as_bytes()).unwrap();
    }
}

#[tokio::test]
async fn licencas_experiencia_e_assinatura() {
    use super::licenca::teste;
    use chrono::{Duration, Local};
    let pasta = std::env::temp_dir().join(format!("kigest-teste-licenca-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Licenciada, Lda", "nif": "5417189230" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let artigo = rpc(&st, "artigos.guardar", json!({ "descricao": "Assistência", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    let venda = json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] });
    let dia = |d: i64| (Local::now().date_naive() + Duration::days(d)).format("%Y-%m-%d").to_string();
    let licenca = |nif: &str, inicio: i64, fim: i64, extra: Value| {
        let mut l = json!({ "v": 1, "id": "LIC-TESTE", "nif": nif, "empresa": "Loja Licenciada, Lda", "plano": "COMERCIO", "postos": 2,
                            "inicio": dia(inicio), "fim": dia(fim), "tolerancia": 15, "emitida": dia(0) });
        if let Value::Object(m) = extra {
            l.as_object_mut().unwrap().extend(m);
        }
        teste::emitir(&l)
    };
    let gravar = |chave: &'static str, valor: String| {
        let pool = st.pool.clone();
        async move {
            sqlx::query("INSERT INTO config (chave, valor) VALUES (?, ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").bind(chave).bind(valor).execute(&pool).await.unwrap();
        }
    };

    // período de experiência: 30 dias a contar da criação da empresa
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!((e["modo"].as_str(), e["dias"].as_i64(), e["pode_emitir"].as_bool()), (Some("EXPERIENCIA"), Some(30), Some(true)), "{e}");
    assert!(e["pedido"].as_str().unwrap().starts_with("KG-5417189230-"), "{e}");
    rpc(&st, "documentos.emitir", venda.clone()).await;
    // sem registo no servidor, sincronizar não sai para a internet
    let r = rpc(&st, "licenca.sincronizar", Value::Null).await;
    assert_eq!((r["mudou"].as_bool(), r["estado"]["registada"].as_bool()), (Some(false), Some(false)), "{r}");
    assert_eq!(e["selo"], "SEM_SELO", "compilação de testes: sem selo do executável");

    // ---- selo dos dados da licença: mexer na tabela config à mão não dá mais dias ----
    let ler_cfg = |chave: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_optional(&pool).await.unwrap() }
    };
    let contar = |detalhe: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM auditoria WHERE acao = 'LICENCA' AND detalhe LIKE ?").bind(detalhe).fetch_one(&pool).await.unwrap() }
    };
    assert!(ler_cfg("licenca.selo").await.unwrap().starts_with("S1."), "o selo é criado na primeira vez");
    assert_eq!(contar("Selo dos dados da licença criado.").await, 1);
    gravar("licenca.instalada_em", dia(100)).await; // «instalado daqui a 100 dias»
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!((e["modo"].as_str(), e["dias"].as_i64()), (Some("EXPERIENCIA"), Some(30)), "vale o selo: {e}");
    assert_eq!(ler_cfg("licenca.instalada_em").await.unwrap(), dia(0), "e a data volta a ser a verdadeira");
    // apagar o selo e mudar a data: valem os registos da base (a empresa foi criada hoje)
    sqlx::query("DELETE FROM config WHERE chave = 'licenca.selo'").execute(&st.pool).await.unwrap();
    gravar("licenca.instalada_em", dia(100)).await;
    assert_eq!(rpc(&st, "licenca.estado", Value::Null).await["dias"].as_i64(), Some(30));
    assert_eq!(contar("%alterados fora do KanGest%").await, 1, "a alteração fica na auditoria");
    // um selo inventado também não serve
    let inventado = format!("S1.{}.AAAA", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}|{}T00:00:00", dia(100), dia(100))));
    gravar("licenca.selo", inventado).await;
    gravar("licenca.instalada_em", dia(100)).await;
    assert_eq!(rpc(&st, "licenca.estado", Value::Null).await["dias"].as_i64(), Some(30));
    // o selo lembra-se da última utilização: apagar o ultimo_visto não esconde um relógio atrasado
    gravar("licenca.selo", super::licenca::selo_teste(&dia(0), &format!("{}T10:00:00", dia(5)))).await;
    sqlx::query("DELETE FROM config WHERE chave = 'licenca.ultimo_visto'").execute(&st.pool).await.unwrap();
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!(e["modo"], "RELOGIO", "{e}");
    assert!(rpc_err(&st, "documentos.emitir", venda.clone()).await.contains("data deste computador"));
    gravar("licenca.selo", super::licenca::selo_teste(&dia(0), &format!("{}T00:00:00", dia(0)))).await;
    assert_eq!(rpc(&st, "licenca.estado", Value::Null).await["modo"], "EXPERIENCIA");

    // fim da experiência: consulta-se, mas não se emite
    gravar("licenca.instalada_em", dia(-40)).await;
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!((e["modo"].as_str(), e["pode_emitir"].as_bool()), (Some("EXPIRADA"), Some(false)), "{e}");
    assert!(rpc_err(&st, "documentos.emitir", venda.clone()).await.contains("experiência terminou"));
    assert_eq!(rpc(&st, "documentos.listar", json!({})).await.as_array().unwrap().len(), 1, "o histórico continua a ver-se");

    // licenças recusadas: outro NIF, alterada, outro computador, já terminada
    assert!(rpc_err(&st, "licenca.activar", json!({ "chave": licenca("5000000000", 0, 365, json!({})) })).await.contains("NIF"));
    let boa = licenca("5417189230", 0, 365, json!({}));
    let mut alterada: Vec<char> = boa.chars().collect();
    alterada[20] = if alterada[20] == 'A' { 'B' } else { 'A' };
    assert!(rpc_err(&st, "licenca.activar", json!({ "chave": alterada.into_iter().collect::<String>() })).await.contains("não é válida"));
    assert!(rpc_err(&st, "licenca.activar", json!({ "chave": "isto não é uma licença" })).await.contains("KG1"));
    assert!(rpc_err(&st, "licenca.activar", json!({ "chave": licenca("5417189230", 0, 365, json!({ "maquina": "0000-0000-0000" })) })).await.contains("computador"));
    assert!(rpc_err(&st, "licenca.activar", json!({ "chave": licenca("5417189230", -400, -30, json!({})) })).await.contains("terminou"));

    // licença boa (com espaços e quebras de linha, como chega por WhatsApp): volta a facturar
    let colada = format!("  {}\n{}  ", &boa[..40], &boa[40..]);
    let r = rpc(&st, "licenca.activar", json!({ "chave": colada })).await;
    assert_eq!((r["estado"]["modo"].as_str(), r["estado"]["plano"].as_str(), r["estado"]["postos"].as_i64()), (Some("ACTIVA"), Some("COMERCIO"), Some(2)), "{r}");
    rpc(&st, "documentos.emitir", venda.clone()).await;

    // a licença limita os postos (2): o segundo cria-se, o terceiro não
    rpc(&st, "posto.guardar", json!({ "descricao": "Posto#2", "armazem_id": 1, "serie_prefixo": "P2" })).await;
    assert!(rpc_err(&st, "posto.guardar", json!({ "descricao": "Posto#3", "armazem_id": 1, "serie_prefixo": "P3" })).await.contains("permite 2 postos"));

    // terminada há 5 dias, com 15 de tolerância: ainda factura, com aviso
    gravar("licenca.chave", licenca("5417189230", -365, -5, json!({}))).await;
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!((e["modo"].as_str(), e["pode_emitir"].as_bool(), e["aviso"].as_bool(), e["dias"].as_i64()), (Some("TOLERANCIA"), Some(true), Some(true), Some(11)), "{e}");
    rpc(&st, "documentos.emitir", venda.clone()).await;

    // experiência emitida pelo servidor (plano EXPERIENCIA): mostra-se como experiência, sem limite de postos
    gravar("licenca.chave", licenca("5417189230", 0, 9, json!({ "plano": "EXPERIENCIA", "postos": 0, "tolerancia": 0 }))).await;
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!((e["modo"].as_str(), e["dias"].as_i64(), e["pode_emitir"].as_bool()), (Some("EXPERIENCIA"), Some(10), Some(true)), "{e}");
    gravar("licenca.chave", licenca("5417189230", -30, -1, json!({ "plano": "EXPERIENCIA", "postos": 0, "tolerancia": 0 }))).await;
    assert!(rpc_err(&st, "documentos.emitir", venda.clone()).await.contains("experiência terminou"));

    // passada a tolerância: modo de consulta
    gravar("licenca.chave", licenca("5417189230", -365, -20, json!({}))).await;
    assert!(rpc_err(&st, "documentos.emitir", venda.clone()).await.contains("assinatura terminou"));

    // relógio atrasado (a última utilização foi «daqui a 5 dias»): não factura até acertar a data
    gravar("licenca.chave", boa.clone()).await;
    gravar("licenca.ultimo_visto", format!("{}T10:00:00", dia(5))).await;
    let e = rpc(&st, "licenca.estado", Value::Null).await;
    assert_eq!(e["modo"], "RELOGIO", "{e}");
    assert!(rpc_err(&st, "documentos.emitir", venda.clone()).await.contains("data deste computador"));
    gravar("licenca.ultimo_visto", format!("{}T00:00:00", dia(0))).await;
    assert_eq!(rpc(&st, "licenca.estado", Value::Null).await["modo"], "ACTIVA");
}

/// Serve uma base já existente (ex.: a criada por `importar_ficheiro_real`) na porta 7799, como o
/// `servidor_demo`, para ver e imprimir os dados reais no browser:
///   KANGEST_PASTA=/tmp/x cargo test --offline servidor_base -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn servidor_base() {
    let Ok(pasta) = std::env::var("KANGEST_PASTA") else { return };
    let pasta = std::path::PathBuf::from(pasta);
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    let arc = std::sync::Arc::new(st);
    crate::rede::registar_estado(arc.clone());
    let cfg = crate::rede::Config { modo: "servidor".into(), porta: 7799, servidor: String::new(), chave: "auditoria123".into(), sal: String::new(), web: true };
    crate::rede::iniciar_servidor(arc.clone(), &cfg).expect("servidor");
    let cliente = crate::rede::Config { modo: "cliente".into(), servidor: "127.0.0.1:7799".into(), ..cfg.clone() };
    crate::rede::pedir_com_token(&cliente, "auditoria", "auth.login", json!({ "login": "admin", "senha": "1234" }), 1, "Auditoria").expect("entrar");
    eprintln!("SERVIDOR-BASE-PRONTO porta 7799");
    tokio::time::sleep(std::time::Duration::from_secs(3 * 3600)).await;
}

/// Importa um ficheiro real do ExportarKIGEST para uma base à parte e grava o resultado em JSON:
///   KANGEST_FICHEIRO=…/KIGEST-exportacao-….kigest KANGEST_PASTA=/tmp/x cargo test --offline importar_ficheiro_real -- --ignored --nocapture
/// Na pasta ficam `kigest.db` (a base importada) e `resultado.json` (análise, importação, conferência, síntese).
#[tokio::test]
#[ignore]
async fn importar_ficheiro_real() {
    let Ok(ficheiro) = std::env::var("KANGEST_FICHEIRO") else { return };
    let pasta = std::path::PathBuf::from(std::env::var("KANGEST_PASTA").expect("KANGEST_PASTA"));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Empresa", "nif": "5000000000" }, "admin": { "nome": "Administrador", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    let analise = rpc(&st, "importar.analisarFicheiro", json!({ "caminho": ficheiro })).await;
    let simulacao = rpc(&st, "importar.executarFicheiro", json!({ "caminho": ficheiro, "opcoes": { "simular": true } })).await;
    let importacao = rpc(&st, "importar.executarFicheiro", json!({ "caminho": ficheiro, "opcoes": {} })).await;
    // segunda passagem: não pode duplicar nada
    let repeticao = rpc(&st, "importar.executarFicheiro", json!({ "caminho": ficheiro, "opcoes": {} })).await;

    let mut anos = serde_json::Map::new();
    let lista_anos = rpc(&st, "relatorios.sintese", json!({ "ano": db::hoje()[..4].parse::<i64>().unwrap() })).await["anos"].clone();
    for a in lista_anos.as_array().cloned().unwrap_or_default() {
        let ano = a.as_i64().unwrap();
        anos.insert(ano.to_string(), rpc(&st, "relatorios.sintese", json!({ "ano": ano })).await);
    }
    let resultado = json!({
        "analise": analise,
        "simulacao": simulacao,
        "importacao": importacao,
        "repeticao": repeticao,
        "sintese": anos,
        "empresa": rpc(&st, "empresa.obter", Value::Null).await,
        "utilizadores": rpc(&st, "utilizadores.listar", Value::Null).await,
    });
    std::fs::write(pasta.join("resultado.json"), serde_json::to_string_pretty(&resultado).unwrap()).unwrap();
    eprintln!("IMPORTACAO-REAL-CONCLUIDA {}", pasta.display());
}

#[tokio::test]
async fn importacao_por_ficheiro() {
    use super::importar::{Ficheiro, Fonte};
    let pasta = std::env::temp_dir().join(format!("kigest-teste-ficheiro-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Nova", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    let logo: Vec<u8> = [&[0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A][..], &[7u8; 40][..]].concat();
    let objectos: Vec<(&str, &str, &[&str], Vec<Value>)> = vec![
        ("tbzconfigCompany", "TABELA", &["NomeCompany", "NIF", "Endereco", "Logo", "SenhaEmail"], vec![json!(["Loja \"Kiala\" & Filhos\nLda", "5001112229", "Rua Ngola Kiluanje, n.º 5 — Luanda", { "b64": B64.encode(&logo) }, null])]),
        ("tbzconfigTaxasIva", "TABELA", &["Id", "Taxa", "Descricao"], vec![json!([1, 14.0, "IVA 14%"]), json!([2, 0.0, "Isento"])]),
        ("tbzconfigArmazem", "TABELA", &["IdArmazem", "NomeArmazem"], vec![json!([1, "Principal"])]),
        (
            "tbArtigos",
            "TABELA",
            &["IdArtigo", "Referencia", "Descricao", "CoBar", "IdTaxa", "Preco1", "Custo", "ControloStock"],
            vec![json!([10, "ARROZ", "Arroz Tio Lucas 5 kg", "6001234500012", 1, 4500.0, 3200.5, true]), json!([11, "FEIJAO", "Feijão manteiga 1 kg", null, 1, 1250, 900, false])],
        ),
        ("tbArtigosStock", "TABELA", &["IdArtigo", "IdArmazem", "QtdExistente", "CustoMedio"], vec![json!([10, 1, 12.5, 3200.5]), json!([11, 1, 40, 900])]),
        // há bases do KIGEST com códigos de cliente em texto
        (
            "tbClientes",
            "TABELA",
            &["IdCliente", "Nome", "NIF", "DataRegisto"],
            vec![
                json!([1, "Consumidor Final", "999999999", "2019-07-29 00:00:00"]),
                json!([7, "Mercado do Kinaxixi", "5401234567", "2024-01-31 10:15:00"]),
                json!(["Filial centro", "Associação do Centro", "999999999", "2025-03-01 09:00:00"]),
                json!(["000001234", "Cooperativa Leste", "5000001234", "2025-03-02 09:00:00"]),
                // só zeros, com comprimentos diferentes: são dois clientes
                json!(["0000000", "Cliente A", "999999999", "2025-03-03 09:00:00"]),
                json!(["0000000000", "Cliente B", "999999999", "2025-03-04 09:00:00"]),
            ],
        ),
        ("vsFornecedores", "VISTA", &["IdFornecedor", "Nome", "NIF"], vec![json!([3, "Grossista Huambo", "5409876543"])]),
        ("tbzconfigTiposDocumentos", "TABELA", &["Id", "Descricao", "Abreviatura"], vec![json!([1, "Fatura", "FT"])]),
        (
            "tbDocumentosCabessalho",
            "TABELA",
            &["RefDocumento", "IdTipoDocumento", "DataEmissao", "IdEntidade", "Nome", "NIF", "ValTotal", "ValTaxa", "ValLiquido", "Hash", "AssinaturaCurta", "EstadoDocumento", "EstadoDocsTotal"],
            // EstadoDocumento vale «A» em todos (como na base real); anulado é EstadoDocsTotal ≠ 0
            vec![
                json!(["FT 2026/31", 1, "2026-10-02", 7, "Mercado do Kinaxixi", "5401234567", 51300.0, 6300.0, 45000.0, "HASHFICHEIRO", "AbCd", "A", 0]),
                json!(["FT fatura/1", 1, "2025-09-15", "Filial centro", "Associação do Centro", "999999999", 196000.0, 0.0, 196000.0, "H2", "Ef", "A", 0]),
                json!(["FT fatura/2", 1, "2025-09-16", "000001234", "Cooperativa Leste", "5000001234", 1000.0, 0.0, 1000.0, "H3", "Gh", "A", 1]),
            ],
        ),
        (
            "tbDocumentosDetalhe",
            "TABELA",
            &["RefDocumento", "IdArtigo", "Referencia", "ArtigoDescricao", "Qtd", "ValorUnit", "Taxaporcento", "ValorLiquido", "ValorTaxa", "ValorTotal"],
            vec![json!(["FT 2026/31", 10, "ARROZ", "Arroz Tio Lucas 5 kg", 10, 4500.0, 14, 45000.0, 6300.0, 51300.0])],
        ),
        ("tbContasCorrente", "TABELA", &["Documento", "ValorFalta"], vec![json!(["FT 2026/31", 21300.0]), json!(["FT fatura/1", 196000.0])]),
        ("tbLogAcessos", "TABELA", &["Id", "Utilizador", "Data"], vec![json!([1, "sa", "2026-11-01 08:00:00"])]),
    ];
    let erros = [("vsSAFT_REC_Cabecalho", "Invalid object name 'tbReciboCabessalho'.")];
    let ficheiro = pasta.join("KIGEST-exportacao-20261112-0930.kigest");
    escrever_kigest(&ficheiro, true, &objectos, &erros, true);

    // leitura directa: tipos de valores, nomes sem distinção de maiúsculas, vistas e erros de exportação
    let mut f = Ficheiro::abrir(&ficheiro).expect("abrir .kigest");
    assert!(f.completo);
    assert_eq!((f.origem.as_str(), f.computador.as_str()), ("DESKTOP-G26LDJM/KigestGC", "DESKTOP-G26LDJM"));
    assert_eq!(f.erros.len(), 1);
    assert_eq!(f.tabelas().await.unwrap().iter().find(|(n, _)| n == "tbArtigos").unwrap().1, 2);
    let artigos = f.ler("TBARTIGOS").await.unwrap();
    assert_eq!(artigos.len(), 2);
    assert!(matches!(artigos[0]["idartigo"], Val::I(10)));
    assert!(matches!(artigos[0]["preco1"], Val::F(p) if p == 4500.0));
    assert!(matches!(artigos[0]["controlostock"], Val::I(1)));
    assert!(matches!(artigos[1]["cobar"], Val::Null));
    let emp = f.ler("tbzconfigCompany").await.unwrap();
    assert!(matches!(&emp[0]["logo"], Val::B(b) if *b == logo));
    assert!(matches!(&emp[0]["nomecompany"], Val::S(n) if n == "Loja \"Kiala\" & Filhos\nLda"));
    assert!(f.ler("tbQueNaoExiste").await.unwrap().is_empty());
    drop(f);

    // a lista de ficheiros encontra-o na pasta de exportação
    let lista = rpc(&st, "importar.ficheiros", Value::Null).await;
    assert!(lista.as_array().unwrap().iter().any(|x| x["nome"] == "KIGEST-exportacao-20261112-0930.kigest"), "{lista}");

    // análise
    let caminho = ficheiro.display().to_string();
    let a = rpc(&st, "importar.analisarFicheiro", json!({ "caminho": caminho })).await;
    assert_eq!(a["ficheiro"]["completo"], true);
    assert_eq!(a["ficheiro"]["erros"].as_array().unwrap().len(), 1);
    assert!(a["reconhecidas"].as_array().unwrap().iter().any(|x| x["conjunto"] == "fornecedores" && x["tabela"] == "vsFornecedores" && x["registos"] == 1));
    assert!(a["nao_importadas"].as_array().unwrap().iter().any(|x| x["tabela"] == "tbLogAcessos"), "{a}");

    // simulação e importação
    let sim = rpc(&st, "importar.executarFicheiro", json!({ "caminho": caminho, "opcoes": { "simular": true } })).await;
    assert_eq!(sim["simulacao"], true);
    assert_eq!(rpc(&st, "artigos.listar", json!({ "q": "Arroz" })).await.as_array().unwrap().len(), 0, "a simulação não grava");
    let r = rpc(&st, "importar.executarFicheiro", json!({ "caminho": caminho, "opcoes": {} })).await;
    assert!(r["origem"].as_str().unwrap().contains("DESKTOP-G26LDJM/KigestGC (ficheiro KIGEST-exportacao-20261112-0930.kigest)"));
    assert!(r["avisos"].as_array().unwrap().iter().any(|x| x.as_str().unwrap().contains("Tabela não exportada pelo KIGEST: vsSAFT_REC_Cabecalho")));
    let emp = rpc(&st, "empresa.obter", Value::Null).await;
    assert!(emp["nome"].as_str().unwrap().starts_with("Loja \"Kiala\" & Filhos"));
    assert!(emp["logo"].as_str().unwrap_or("").starts_with("data:image/png;base64,"));
    let arroz = rpc(&st, "artigos.codigo", json!({ "codigo": "6001234500012" })).await;
    assert_eq!(arroz["preco1"], 450000);
    assert_eq!(arroz["stock"].as_f64().unwrap(), 12.5);
    let mercado = rpc(&st, "entidades.listar", json!({ "tipo": "C", "q": "Kinaxixi" })).await[0].clone();
    assert_eq!(mercado["saldo"], 2130000);
    assert_eq!(rpc(&st, "entidades.listar", json!({ "tipo": "F", "q": "Grossista" })).await.as_array().unwrap().len(), 1);
    let ft = rpc(&st, "documentos.listar", json!({ "q": "FT 2026/31" })).await[0].clone();
    assert_eq!(ft["hash_controlo"], "AbCd");
    // o histórico entra nos relatórios: por mês e no relatório síntese (ano e ano anterior)
    let por_mes = rpc(&st, "relatorios.vendas", json!({ "de": "2026-01-01", "ate": "2026-12-31", "agrupar": "mes" })).await;
    assert!(por_mes.as_array().unwrap().iter().any(|l| l["chave"] == "2026-10" && l["total"] == 5130000), "{por_mes}");
    let sintese = rpc(&st, "relatorios.sintese", json!({ "ano": 2026 })).await;
    assert_eq!(sintese["meses"][9]["atual"], 5130000);
    assert_eq!(sintese["meses"][9]["anterior"], Value::Null);
    // todo o histórico importado (FT 2026/31 + FT fatura/1 de 2025); a FT fatura/2 anulada não conta
    assert_eq!(sintese["historico_kigest"], 24730000);
    assert!(sintese["anos"].as_array().unwrap().contains(&json!(2026)));
    assert_eq!(rpc(&st, "relatorios.sintese", json!({ "ano": 2027 })).await["meses"][9]["anterior"], 5130000);
    let conf = r["conferencia"].as_array().unwrap();
    let item = |n: &str| conf.iter().find(|c| c["item"] == n).cloned().unwrap();
    assert_eq!((item("Documentos")["kigest"].as_i64(), item("Documentos")["kigest_novo"].as_i64()), (Some(3), Some(3)));
    assert_eq!((item("Clientes")["kigest"].as_i64(), item("Clientes")["kigest_novo"].as_i64()), (Some(6), Some(6)));
    let divida = item("Valores em dívida (conta corrente)");
    assert_eq!((divida["kigest"].as_i64(), divida["kigest_novo"].as_i64()), (Some(21730000), Some(21730000)), "{divida}");
    // código de cliente em texto: o documento fica no cliente certo, não normal, com a dívida
    let centro = rpc(&st, "documentos.listar", json!({ "q": "FT fatura/1" })).await[0].clone();
    assert_eq!((centro["entidade_nome"].as_str(), centro["estado"].as_str(), centro["valor_pendente"].as_i64()), (Some("Associação do Centro"), Some("N"), Some(19600000)), "{centro}");
    let leste = rpc(&st, "documentos.listar", json!({ "q": "FT fatura/2" })).await[0].clone();
    assert_eq!((leste["entidade_nome"].as_str(), leste["estado"].as_str()), (Some("Cooperativa Leste"), Some("A")), "{leste}");
    assert_eq!(item("Quantidade total em stock")["kigest"].as_f64(), Some(52.5));
    assert_eq!(item("Quantidade total em stock")["kigest_novo"].as_f64(), Some(52.5));

    // ControloStock = falso no KIGEST: as vendas abatem na mesma o stock, só se deixa vender sem stock
    let feijao: (i64, i64) = sqlx::query_as("SELECT controla_stock, permite_negativo FROM artigos WHERE referencia = 'FEIJAO'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(feijao, (1, 1));
    let arroz: (i64, i64) = sqlx::query_as("SELECT controla_stock, permite_negativo FROM artigos WHERE referencia = 'ARROZ'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(arroz, (1, 0));

    // importação feita com uma versão antiga (tudo anulado, sem descontos, stock não controlado): repetir corrige
    sqlx::query("UPDATE documentos SET estado = 'A', valor_pendente = 0, entidade_id = 1 WHERE importado = 1").execute(&st.pool).await.unwrap();
    sqlx::query("UPDATE artigos SET controla_stock = 0, permite_negativo = 0 WHERE id_origem IS NOT NULL").execute(&st.pool).await.unwrap();
    let r2 = rpc(&st, "importar.executarFicheiro", json!({ "caminho": caminho, "opcoes": {} })).await;
    let corrigidos = r2["contagens"].as_array().unwrap().iter().find(|c| c["conjunto"] == "Documentos já importados corrigidos").cloned();
    // as 3 voltam ao cliente certo; a FT 2026/31 e a FT fatura/1 deixam de estar anuladas (a FT fatura/2 é mesmo anulada)
    assert_eq!(corrigidos.map(|c| c["registos"].clone()), Some(json!(3)), "{r2}");
    let centro = rpc(&st, "documentos.listar", json!({ "q": "FT fatura/1" })).await[0].clone();
    assert_eq!((centro["entidade_nome"].as_str(), centro["estado"].as_str(), centro["valor_pendente"].as_i64()), (Some("Associação do Centro"), Some("N"), Some(19600000)), "{centro}");
    let feijao: (i64, i64) = sqlx::query_as("SELECT controla_stock, permite_negativo FROM artigos WHERE referencia = 'FEIJAO'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(feijao, (1, 1));

    // sem gzip também serve
    let simples = pasta.join("simples.kigest");
    escrever_kigest(&simples, false, &objectos, &[], true);
    assert_eq!(Ficheiro::abrir(&simples).unwrap().registos_total(), 23);

    // exportação interrompida: lê-se para análise mas não se importa
    let parcial = pasta.join("parcial.kigest");
    escrever_kigest(&parcial, true, &objectos[..3], &[], false);
    let a = rpc(&st, "importar.analisarFicheiro", json!({ "caminho": parcial.display().to_string() })).await;
    assert_eq!(a["ficheiro"]["completo"], false);
    assert!(rpc_err(&st, "importar.executarFicheiro", json!({ "caminho": parcial.display().to_string(), "opcoes": {} })).await.contains("incompleto"));

    // outro ficheiro qualquer
    let outro = pasta.join("notas.kigest");
    std::fs::write(&outro, "isto não é uma exportação\n").unwrap();
    assert!(rpc_err(&st, "importar.analisarFicheiro", json!({ "caminho": outro.display().to_string() })).await.contains("não é uma exportação"));
    assert!(rpc_err(&st, "importar.analisarFicheiro", json!({ "caminho": pasta.join("nao-existe.kigest").display().to_string() })).await.contains("não encontrado"));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

// =====================================================================================
// Paridade com o KIGEST: promoções, reservas de encomendas, lista de preços e previsão de venda
// =====================================================================================
#[tokio::test]
async fn promocoes_reservas_e_listas() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-promo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Livraria Teste", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let hoje = db::hoje();
    let ano: i32 = hoje[..4].parse().unwrap();

    // ---- promoção: desconto automático só dentro do período ----
    let livro = rpc(
        &st,
        "artigos.guardar",
        json!({ "descricao": "O Desejado de Todas as Nações", "taxa_iva_id": 1, "preco1": 1000000, "custo": 600000,
                "promo_desconto": 10, "promo_inicio": format!("{}-01-01", ano - 1), "promo_fim": format!("{}-12-31", ano + 1) }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let futuro = rpc(
        &st,
        "artigos.guardar",
        json!({ "descricao": "Hinário Adventista", "taxa_iva_id": 1, "preco1": 500000, "custo": 300000,
                "promo_desconto": 20, "promo_inicio": format!("{}-01-01", ano + 1) }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "X", "taxa_iva_id": 1, "preco1": 1, "promo_desconto": 5, "promo_inicio": "2026-05-01", "promo_fim": "2026-04-01" })).await.contains("termina antes"));
    let a = rpc(&st, "artigos.obter", json!({ "id": livro })).await;
    assert_eq!(a["promo_atual"].as_f64(), Some(10.0));
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": futuro })).await["promo_atual"].as_f64(), Some(0.0));
    for (id, q) in [(livro, 20), (futuro, 10)] {
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": id, "quantidade": q, "preco_unitario": 600000 }] })).await;
    }
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Escola Adventista do Huambo", "nif": "5417000123" })).await["id"].as_i64().unwrap();
    // sem desconto indicado: aplica a promoção em vigor; o artigo com promoção futura vende ao preço normal
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": livro, "quantidade": 1 }, { "artigo_id": futuro, "quantidade": 1 }] })).await;
    let linhas = ft["linhas"].as_array().unwrap().clone();
    let l_livro = linhas.iter().find(|l| l["artigo_id"] == livro).unwrap();
    assert_eq!(l_livro["desconto_pct"].as_f64(), Some(10.0));
    assert_eq!(l_livro["valor_liquido"], 900000);
    assert_eq!(linhas.iter().find(|l| l["artigo_id"] == futuro).unwrap()["desconto_pct"].as_f64(), Some(0.0));
    // operador sem permissão de desconto: a promoção passa sem PIN, acima dela não
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Caixa Huambo", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false, "permissoes": ["vendas.fatura"] })).await;
    rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": livro, "quantidade": 1, "desconto_pct": 10 }] })).await;
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": livro, "quantidade": 1, "desconto_pct": 15 }] })).await.contains("PIN"));
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": futuro, "quantidade": 1, "desconto_pct": 20 }] })).await.contains("PIN"));
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // ---- reservas: encomenda de cliente por facturar e encomenda a fornecedor por receber ----
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Publicadora Servir", "nif": "5410000777" })).await["id"].as_i64().unwrap();
    let ne = rpc(&st, "documentos.emitir", json!({ "tipo": "NE", "entidade_id": cli, "linhas": [{ "artigo_id": livro, "quantidade": 5 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "VNE", "entidade_id": forn, "doc_externo": "ENC-77", "linhas": [{ "artigo_id": livro, "quantidade": 30, "preco_unitario": 600000 }] })).await;
    let linha_stock = |v: Value| v.as_array().unwrap().iter().find(|l| l["id"] == livro).cloned().unwrap();
    let s = linha_stock(rpc(&st, "stock.listar", json!({ "q": "Desejado" })).await);
    let existencia = s["quantidade"].as_f64().unwrap();
    assert_eq!(s["reservado"].as_f64(), Some(5.0));
    assert_eq!(s["encomendado"].as_f64(), Some(30.0));
    assert_eq!(s["disponivel"].as_f64(), Some(existencia - 5.0));
    // facturada a encomenda, a reserva desaparece
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "doc_origem_id": ne["documento"]["id"], "linhas": [{ "artigo_id": livro, "quantidade": 5 }] })).await;
    let s = linha_stock(rpc(&st, "stock.listar", json!({ "q": "Desejado" })).await);
    assert_eq!(s["reservado"].as_f64(), Some(0.0));
    assert_eq!(s["encomendado"].as_f64(), Some(30.0));

    // ---- lista de preços e previsão de venda ----
    let precos = rpc(&st, "relatorios.stock", json!({ "situacao": "precos" })).await;
    let p = precos.as_array().unwrap().iter().find(|l| l["descricao"] == "O Desejado de Todas as Nações").unwrap().clone();
    assert_eq!(p["pvp1"], 1140000, "PVP com IVA de 14%");
    assert_eq!(p["promo_atual"].as_f64(), Some(10.0));
    let prev = rpc(&st, "relatorios.stock", json!({ "situacao": "previsao" })).await;
    let p = prev.as_array().unwrap().iter().find(|l| l["descricao"] == "O Desejado de Todas as Nações").unwrap().clone();
    let q = p["quantidade"].as_f64().unwrap();
    assert_eq!(p["valor_venda"].as_i64(), Some((q * 1000000.0).round() as i64));
    assert_eq!(p["lucro"].as_i64(), Some(p["valor_venda"].as_i64().unwrap() - p["valor_custo"].as_i64().unwrap()));
    let exist = rpc(&st, "relatorios.stock", json!({ "situacao": "existencias" })).await;
    let e = exist.as_array().unwrap().iter().find(|l| l["descricao"] == "O Desejado de Todas as Nações").unwrap().clone();
    assert_eq!(e["quantidade"].as_f64(), Some(q));
    assert!(exist.as_array().unwrap().iter().any(|l| l["descricao"] == "Hinário Adventista"));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

// =====================================================================================
// Utilizadores, perfis de acesso e autenticação
// =====================================================================================
#[tokio::test]
async fn utilizadores_permissoes_autenticacao() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-auth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Livraria Teste", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "admin123" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;

    // ---- política de segurança (a pública não exige sessão) ----
    rpc(&st, "seguranca.guardar", json!({ "senha_min": 8, "tentativas": 3, "bloqueio_min": 15, "inatividade_min": 10, "login_pin": true })).await;
    let pub_ = rpc(&st, "seguranca.politica", Value::Null).await;
    assert_eq!((pub_["login_pin"].as_bool(), pub_["inatividade_min"].as_i64()), (Some(true), Some(10)));

    // ---- perfil próprio «Armazém» ----
    let arm = rpc(&st, "perfis.guardar", json!({ "descricao": "Armazém", "nivel": 1, "permissoes": ["stock.entrada", "stock.saida", "artigos.editar"] })).await;
    assert_eq!(arm["codigo"], "ARMAZEM");
    assert!(rpc_err(&st, "perfis.guardar", json!({ "descricao": "x", "nivel": 1, "permissoes": ["nao.existe"] })).await.contains("desconhecida"));
    let perfis = rpc(&st, "perfis.listar", Value::Null).await;
    assert!(perfis.as_array().unwrap().iter().any(|p| p["codigo"] == "ARMAZEM" && p["permissoes"].as_array().unwrap().len() == 3));
    assert!(perfis.as_array().unwrap().iter().any(|p| p["codigo"] == "GESTOR" && !p["permissoes"].as_array().unwrap().contains(&json!("sistema.importar"))));

    // ---- utilizador novo: senha conforme a política, PIN único, senha provisória ----
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "perfil": "ARMAZEM", "senha": "curta" })).await.contains("8 caracteres"));
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "perfil": "INEXISTENTE", "senha": "armazem01" })).await.contains("Perfil"));
    let ana = rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana Armazém", "login": "ana", "perfil": "ARMAZEM", "senha": "armazem01", "pin": "5678" })).await["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "Rui", "login": "rui", "perfil": "CAIXA", "senha": "caixa0001", "pin": "5678" })).await.contains("PIN"));
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "Outra Ana", "login": "ANA", "perfil": "CAIXA", "senha": "caixa0001" })).await.contains("Já existe"));
    // o último administrador não perde o acesso
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "id": 1, "nome": "Admin", "login": "admin", "perfil": "CAIXA" })).await.contains("único administrador"));
    // perfis em uso e de origem não se eliminam
    assert!(rpc_err(&st, "perfis.eliminar", json!({ "codigo": "ARMAZEM" })).await.contains("usam este perfil"));
    assert!(rpc_err(&st, "perfis.eliminar", json!({ "codigo": "GESTOR" })).await.contains("origem"));

    // ---- primeiro acesso: tem de mudar a senha provisória antes de tudo ----
    let s = rpc(&st, "auth.login", json!({ "login": "ana", "senha": "armazem01" })).await;
    assert_eq!(s["mudar_senha"], true);
    assert_eq!(s["nivel"], 1);
    assert!(rpc_err(&st, "artigos.listar", json!({ "q": "" })).await.contains("senha provisória"));
    assert!(rpc_err(&st, "auth.alterarSenha", json!({ "atual": "armazem01", "nova": "curta" })).await.contains("8 caracteres"));
    assert!(rpc_err(&st, "auth.alterarSenha", json!({ "atual": "armazem01", "nova": "armazem01" })).await.contains("diferente"));
    let s = rpc(&st, "auth.alterarSenha", json!({ "atual": "armazem01", "nova": "Livros#2026" })).await;
    assert_eq!(s["mudar_senha"], false);
    // agora trabalha, só com as permissões do perfil
    rpc(&st, "artigos.listar", json!({ "q": "" })).await;
    let livro = {
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;
        let id = rpc(&st, "artigos.guardar", json!({ "descricao": "Bíblia de Estudo", "taxa_iva_id": 1, "preco1": 1500000 })).await["id"].as_i64().unwrap();
        rpc(&st, "auth.login", json!({ "login": "ana", "senha": "Livros#2026" })).await;
        id
    };
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": livro, "quantidade": 10, "preco_unitario": 900000 }] })).await;
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": 1, "linhas": [{ "artigo_id": livro, "quantidade": 1 }] })).await.to_lowercase().contains("permiss"));
    assert!(rpc_err(&st, "importar.ficheiros", Value::Null).await.to_lowercase().contains("permiss"));

    // ---- sessão bloqueada: só se retoma com a senha ou o PIN do próprio ----
    let b = rpc(&st, "auth.bloquear", Value::Null).await;
    assert_eq!(b["bloqueada"], true);
    assert!(rpc_err(&st, "artigos.listar", json!({ "q": "" })).await.contains("bloqueada"));
    assert!(rpc_err(&st, "auth.desbloquear", json!({ "pin": "0000" })).await.contains("incorrectos"));
    let r = rpc(&st, "auth.desbloquear", json!({ "pin": "5678" })).await;
    assert_eq!(r["bloqueada"], false);
    rpc(&st, "artigos.listar", json!({ "q": "" })).await;
    rpc(&st, "auth.logout", Value::Null).await;

    // ---- tentativas falhadas: à 3.ª a conta bloqueia, mesmo com a senha certa ----
    assert!(rpc_err(&st, "auth.login", json!({ "login": "ana", "senha": "errada1" })).await.contains("Resta(m) 2"));
    assert!(rpc_err(&st, "auth.login", json!({ "login": "ana", "senha": "errada2" })).await.contains("Resta(m) 1"));
    assert!(rpc_err(&st, "auth.login", json!({ "login": "ana", "senha": "errada3" })).await.contains("Conta bloqueada"));
    assert!(rpc_err(&st, "auth.login", json!({ "login": "ana", "senha": "Livros#2026" })).await.contains("bloqueada"));
    assert!(rpc_err(&st, "auth.login", json!({ "login": "ninguem", "senha": "x" })).await.contains("incorrectos"));
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;
    let lista = rpc(&st, "utilizadores.listar", Value::Null).await;
    let a = lista.as_array().unwrap().iter().find(|u| u["login"] == "ana").unwrap().clone();
    assert!(a["bloqueado_ate"].is_string());
    assert_eq!(a["perfil_descricao"], "Armazém");
    rpc(&st, "utilizadores.desbloquear", json!({ "id": ana })).await;

    // ---- entrada por PIN ----
    assert!(rpc_err(&st, "auth.loginPin", json!({ "pin": "9999" })).await.contains("PIN incorrecto"));
    let p = rpc(&st, "auth.loginPin", json!({ "pin": "5678" })).await;
    assert_eq!(p["login"], "ana");

    // ---- senha redefinida pelo administrador: provisória outra vez ----
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;
    let nova = rpc(&st, "utilizadores.redefinirSenha", json!({ "id": ana })).await;
    let provisoria = nova["senha"].as_str().unwrap().to_string();
    assert!(provisoria.len() >= 8);
    let s = rpc(&st, "auth.login", json!({ "login": "ana", "senha": provisoria })).await;
    assert_eq!(s["mudar_senha"], true);

    // ---- permissões de um perfil de origem alteradas chegam aos seus utilizadores ----
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Gestor", "login": "gestor", "perfil": "GESTOR", "senha": "gestor2026", "mudar_senha": false })).await;
    rpc(&st, "perfis.guardar", json!({ "codigo": "GESTOR", "descricao": "Gestor", "nivel": 2, "permissoes": ["relatorios.ver", "clientes"] })).await;
    let g = rpc(&st, "auth.login", json!({ "login": "gestor", "senha": "gestor2026" })).await;
    assert_eq!(g["permissoes"], json!(["relatorios.ver", "clientes"]));
    assert_eq!(g["nivel"], 2);

    // ---- registo de acessos e auditoria ----
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "admin123" })).await;
    let hoje = db::hoje();
    let acessos = rpc(&st, "acessos.listar", json!({ "de": hoje, "ate": hoje })).await;
    let eventos: Vec<&str> = acessos.as_array().unwrap().iter().map(|a| a["evento"].as_str().unwrap()).collect();
    for e in ["LOGIN", "LOGOUT", "FALHA", "BLOQUEIO", "DESBLOQUEIO", "SENHA", "SESSAO_BLOQUEADA", "SESSAO_RETOMADA", "SENHA_REDEFINIDA"] {
        assert!(eventos.contains(&e), "falta {e} em {eventos:?}");
    }
    let falhas = rpc(&st, "acessos.listar", json!({ "de": hoje, "ate": hoje, "evento": "FALHA", "utilizador_id": ana })).await;
    assert!(falhas.as_array().unwrap().len() >= 4);
    let auditoria = rpc(&st, "auditoria.listar", json!({ "de": hoje, "ate": hoje })).await;
    assert!(auditoria.as_array().unwrap().iter().any(|a| a["acao"] == "PERFIL"));
    assert!(auditoria.as_array().unwrap().iter().any(|a| a["acao"] == "UTILIZADOR"));
    // um gestor não vê o registo de acessos
    rpc(&st, "auth.login", json!({ "login": "gestor", "senha": "gestor2026" })).await;
    assert!(rpc_err(&st, "acessos.listar", json!({ "de": hoje, "ate": hoje })).await.to_lowercase().contains("permiss"));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// O KanGest a falar com o servidor de licenças a correr no Node (servidor-licencas/teste/servidor-local.mjs):
///   KANGEST_CHAVE_SERVIDOR=~/.kangest-licencas/servidor/privada.pem ADMIN_TOKEN=<t> node teste/servidor-local.mjs 8787
///   KANGEST_LICENCAS_URL=http://127.0.0.1:8787 KANGEST_ADMIN_TOKEN=<t> cargo test --lib licencas_servidor_local -- --ignored
#[tokio::test]
#[ignore]
async fn licencas_servidor_local() {
    use crate::http;
    let url = std::env::var("KANGEST_LICENCAS_URL").expect("KANGEST_LICENCAS_URL");
    let token = std::env::var("KANGEST_ADMIN_TOKEN").expect("KANGEST_ADMIN_TOKEN");
    let admin = |caminho: &str, corpo: Option<Value>| {
        let (url, token, caminho) = (url.clone(), token.clone(), caminho.to_string());
        async move {
            tokio::task::spawn_blocking(move || {
                let dados = corpo.map(|c| serde_json::to_vec(&c).unwrap());
                let r = http::pedido(
                    if dados.is_some() { "POST" } else { "GET" },
                    &format!("{url}/admin/api/{caminho}"),
                    &[("Authorization", format!("Bearer {token}")), ("Content-Type", "application/json".into())],
                    dados.as_deref(),
                    std::time::Duration::from_secs(20),
                )
                .unwrap();
                serde_json::from_slice::<Value>(&r.corpo).unwrap()
            })
            .await
            .unwrap()
        }
    };

    let pasta = std::env::temp_dir().join(format!("kigest-teste-licenca-servidor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Empresa Exemplo, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // registo: pede o código e confirma-o
    assert!(rpc_err(&st, "licenca.registar", json!({ "email": "sem-arroba" })).await.contains("email"));
    let r = rpc(&st, "licenca.registar", json!({ "email": "Geral@Exemplo.ao", "telefone": "923000000" })).await;
    assert_eq!(r["enviado"].as_bool(), Some(false), "{r}");
    let maquina = super::licenca::id_maquina();
    let resumo = admin("resumo", None).await;
    let codigo = resumo["codigos"].as_array().unwrap().iter().find(|c| c["maquina"] == maquina.as_str()).expect("pedido no painel")["codigo_aberto"].as_str().unwrap().to_string();
    assert!(rpc_err(&st, "licenca.confirmar", json!({ "codigo": if codigo == "000000" { "111111" } else { "000000" } })).await.contains("Código errado"));
    let r = rpc(&st, "licenca.confirmar", json!({ "codigo": codigo })).await;
    let e = &r["estado"];
    assert_eq!((e["modo"].as_str(), e["plano"].as_str(), e["registada"].as_bool(), e["dias"].as_i64()), (Some("EXPERIENCIA"), Some("EXPERIENCIA"), Some(true), Some(30)), "{r}");
    assert!(e["licenca_id"].as_str().unwrap().starts_with("LIC-"));
    assert_eq!(e["email_registo"].as_str(), Some("geral@exemplo.ao"));

    // pagamento no painel: a licença nova chega ao sincronizar
    let pg = admin("pagamento", Some(json!({ "nif": "5000000000", "plano": "COMERCIO", "meses": 12, "valor": 0, "metodo": "TRANSFERENCIA" }))).await;
    assert_eq!(pg["ok"].as_bool(), Some(true), "{pg}");
    let r = rpc(&st, "licenca.sincronizar", Value::Null).await;
    assert_eq!(r["mudou"].as_bool(), Some(false), "acabou de sincronizar: espera");
    let r = rpc(&st, "licenca.sincronizar", json!({ "forcar": true })).await;
    let e = &r["estado"];
    assert_eq!((r["mudou"].as_bool(), e["modo"].as_str(), e["plano"].as_str(), e["postos"].as_i64(), e["fim"].as_str()), (Some(true), Some("ACTIVA"), Some("COMERCIO"), Some(3), pg["pago_ate"].as_str()), "{r}");
    assert!(e["mensagem"].as_str().unwrap().contains("Comércio"), "{e}");

    // sem mudanças no servidor: nada muda
    let r = rpc(&st, "licenca.sincronizar", json!({ "forcar": true })).await;
    assert_eq!(r["mudou"].as_bool(), Some(false), "{r}");

    // computador bloqueado no painel: a sincronização explica, a licença que já tem continua
    admin("maquina", Some(json!({ "nif": "5000000000", "maquina": maquina, "bloqueada": true }))).await;
    assert!(rpc_err(&st, "licenca.sincronizar", json!({ "forcar": true })).await.contains("bloqueado"));
    assert_eq!(rpc(&st, "licenca.estado", Value::Null).await["modo"].as_str(), Some("ACTIVA"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Base temporária com a empresa criada e o administrador com sessão iniciada.
/// Fecha o pool só quando todas as ligações estão fechadas de facto. O `close()` do sqlx pode acabar antes de a
/// última ligação usada voltar ao pool (isso faz-se numa tarefa à parte), e essa ligação, ao fechar, faria o
/// checkpoint do WAL antigo para dentro do ficheiro já trocado. No programa o restauro aplica-se no arranque,
/// antes de abrir a base, por isso só os testes precisam disto.
async fn fechar_de_vez(pool: &sqlx::SqlitePool) {
    let espera = std::time::Duration::from_millis(10);
    for _ in 0..500 {
        if pool.num_idle() == pool.size() as usize {
            break;
        }
        tokio::time::sleep(espera).await;
    }
    pool.close().await;
    for _ in 0..500 {
        if pool.size() == 0 {
            break;
        }
        tokio::time::sleep(espera).await;
    }
    assert_eq!(pool.size(), 0, "ficaram ligações abertas à base");
}

async fn base_com_admin(nome: &str) -> (AppState, std::path::PathBuf) {
    let pasta = std::env::temp_dir().join(format!("kigest-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.expect("posto 1");
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Teste, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    (st, pasta)
}

#[tokio::test]
async fn notas_de_credito_limitadas_a_origem() {
    let (st, pasta) = base_com_admin("nc-limites").await;
    let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
    let oleo = rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo", "taxa_iva_id": 1, "preco1": 500000, "custo": 50000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": arroz, "quantidade": 20, "preco_unitario": 50000 }, { "artigo_id": oleo, "quantidade": 20, "preco_unitario": 50000 }] })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 3 }], "pagamentos": [{ "modo": "NU", "valor": 342000 }] })).await;
    let fr_id = fr["documento"]["id"].as_i64().unwrap();
    // o preço mudou depois da venda: a nota usa o preço da factura
    rpc(&st, "artigos.guardar", json!({ "id": arroz, "descricao": "Arroz", "taxa_iva_id": 1, "preco1": 900000, "custo": 50000 })).await;
    let nc = |q: f64, artigo: i64| json!({ "tipo": "NC", "entidade_id": 1, "doc_origem_id": fr_id, "motivo": "Devolução de mercadoria", "linhas": [{ "artigo_id": artigo, "quantidade": q }] });

    // mais do que o facturado, ou um artigo que não estava na factura: recusado
    assert!(rpc_err(&st, "documentos.emitir", nc(5.0, arroz)).await.contains("só pode creditar 3"));
    assert!(rpc_err(&st, "documentos.emitir", nc(1.0, oleo)).await.contains("não consta do documento de origem"));
    // preço acima do facturado: o valor excede o que falta creditar
    let caro = json!({ "tipo": "NC", "entidade_id": 1, "doc_origem_id": fr_id, "motivo": "Devolução de mercadoria", "linhas": [{ "artigo_id": arroz, "quantidade": 1, "preco_unitario": 400000 }] });
    assert!(rpc_err(&st, "documentos.emitir", caro).await.contains("excede"));

    // duas notas parciais que somam a factura; a terceira já não cabe
    let n1 = rpc(&st, "documentos.emitir", nc(2.0, arroz)).await;
    assert_eq!(n1["documento"]["total"], 228000, "preço da factura (1 000,00 + IVA), não o preço novo");
    let disp = rpc(&st, "documentos.creditavel", json!({ "id": fr_id })).await;
    assert_eq!(disp["artigos"][0]["disponivel"].as_f64(), Some(1.0));
    assert_eq!(disp["disponivel"], 114000);
    let n2 = rpc(&st, "documentos.emitir", nc(1.0, arroz)).await;
    assert!(rpc_err(&st, "documentos.emitir", nc(1.0, arroz)).await.contains("só pode creditar 0"));
    // a devolução em dinheiro nunca passa do que a venda recebeu
    let cx = rpc(&st, "caixa.estado", Value::Null).await;
    assert_eq!(cx["saidas"], 342000);
    assert_eq!(cx["numerario_esperado"], 0);
    // a nota que devolveu dinheiro não se anula (regra que já existia)
    assert!(rpc_err(&st, "documentos.anular", json!({ "id": n2["documento"]["id"], "motivo": "Nota emitida por engano" })).await.contains("movimentado"));

    // factura a crédito: anular a nota liberta a quantidade
    let cli = rpc(&st, "entidades.guardar", json!({ "tipo": "C", "nome": "Cliente Lda", "nif": "5417000000" })).await["id"].as_i64().unwrap();
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": oleo, "quantidade": 2 }] })).await;
    let ft_id = ft["documento"]["id"].as_i64().unwrap();
    let nc_ft = |q: f64| json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": ft_id, "motivo": "Devolução de mercadoria", "linhas": [{ "artigo_id": oleo, "quantidade": q }] });
    let n3 = rpc(&st, "documentos.emitir", nc_ft(2.0)).await;
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": ft_id })).await["documento"]["valor_pendente"], 0);
    assert!(rpc_err(&st, "documentos.emitir", nc_ft(1.0)).await.contains("só pode creditar 0"));
    rpc(&st, "documentos.anular", json!({ "id": n3["documento"]["id"], "motivo": "Nota emitida por engano" })).await;
    assert_eq!(rpc(&st, "documentos.creditavel", json!({ "id": ft_id })).await["artigos"][0]["disponivel"].as_f64(), Some(2.0));
    rpc(&st, "documentos.emitir", nc_ft(1.0)).await;

    // compras: a V/Nota de crédito segue as mesmas regras
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Fornecedor SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();
    let fc = rpc(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "FT A/1", "linhas": [{ "artigo_id": oleo, "quantidade": 4, "preco_unitario": 60000 }] })).await;
    let vnc = |q: f64| json!({ "tipo": "VNC", "entidade_id": forn, "doc_externo": "NC 9", "motivo": "Devolução ao fornecedor", "doc_origem_id": fc["documento"]["id"], "linhas": [{ "artigo_id": oleo, "quantidade": q }] });
    assert!(rpc_err(&st, "documentos.emitir", vnc(5.0)).await.contains("só pode creditar 4"));
    let v = rpc(&st, "documentos.emitir", vnc(4.0)).await;
    assert_eq!(v["documento"]["total"], 273600, "4 × 600,00 + IVA, ao custo da factura do fornecedor");
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn integridade_da_cadeia_e_proteccao() {
    let (st, pasta) = base_com_admin("integridade").await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": art, "quantidade": 20, "preco_unitario": 50000 }] })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let mut ids = Vec::new();
    for q in [1, 2, 3] {
        let d = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": q }], "pagamentos": [{ "modo": "NU", "valor": 114000 * q }] })).await;
        ids.push(d["documento"]["id"].as_i64().unwrap());
    }
    let v = rpc(&st, "agt.verificarIntegridade", Value::Null).await;
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["verificados"].as_i64().unwrap() >= 3);
    assert_eq!(v["protecao"]["ativa"], true);

    // os dados fiscais de um documento assinado não se alteram nem se apagam
    let e = sqlx::query("UPDATE documentos SET total = 1 WHERE id = ?").bind(ids[0]).execute(&st.pool).await.unwrap_err().to_string();
    assert!(e.contains("Documento assinado"), "{e}");
    assert!(sqlx::query("DELETE FROM documentos WHERE id = ?").bind(ids[2]).execute(&st.pool).await.is_err());
    assert!(sqlx::query("UPDATE documento_linhas SET quantidade = 9 WHERE documento_id = ?").bind(ids[1]).execute(&st.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM documento_pagamentos WHERE documento_id = ?").bind(ids[1]).execute(&st.pool).await.is_err());
    assert!(sqlx::query("UPDATE series SET ultimo_numero = 1 WHERE tipo_codigo = 'FR'").execute(&st.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM series WHERE tipo_codigo = 'FR'").execute(&st.pool).await.is_err());
    // o que a aplicação muda depois de emitir continua a funcionar (anulação, conta corrente)
    rpc(&st, "documentos.anular", json!({ "id": ids[2], "motivo": "Venda registada por engano" })).await;
    assert!(sqlx::query("UPDATE documentos SET estado = 'N' WHERE id = ?").bind(ids[2]).execute(&st.pool).await.is_err(), "anulado não volta a valer");
    assert_eq!(rpc(&st, "agt.verificarIntegridade", Value::Null).await["ok"], true, "anular não quebra a cadeia");

    // quem desligar a protecção e mexer no total é apanhado pela verificação
    sqlx::query("DROP TRIGGER trg_doc_assinado_upd").execute(&st.pool).await.unwrap();
    sqlx::query("UPDATE documentos SET total = 1000 WHERE id = ?").bind(ids[1]).execute(&st.pool).await.unwrap();
    let v = rpc(&st, "agt.verificarIntegridade", Value::Null).await;
    assert_eq!(v["ok"], false);
    assert_eq!(v["protecao"]["em_falta"][0], "trg_doc_assinado_upd");
    let fr = v["series"].as_array().unwrap().iter().find(|s| s["tipo"] == "FR").unwrap().clone();
    let problemas: Vec<String> = fr["erros"].as_array().unwrap().iter().map(|e| format!("{}: {}", e["documento"], e["problema"])).collect();
    assert!(problemas.iter().any(|p| p.contains("/2") && p.contains("assinatura não confere")), "{problemas:?}");
    assert!(problemas.iter().any(|p| p.contains("/2") && p.contains("totais")), "{problemas:?}");

    // trocar de chave não estraga a verificação dos documentos antigos (só em desenvolvimento se carrega uma chave)
    if crate::produtor::chave_agt().unwrap().is_some() {
        let _ = std::fs::remove_dir_all(&pasta);
        return;
    }
    let (st2, pasta2) = base_com_admin("integridade-chave").await;
    let a2 = rpc(&st2, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    rpc(&st2, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    rpc(&st2, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a2, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let nova = {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap().to_pkcs8_pem(LineEnding::LF).unwrap().to_string()
    };
    rpc(&st2, "agt.importarChave", json!({ "pem": nova })).await;
    rpc(&st2, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a2, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let v2 = rpc(&st2, "agt.verificarIntegridade", Value::Null).await;
    assert_eq!(v2["ok"], true, "{v2}");
    assert_eq!(v2["chaves"], 2);
    let _ = std::fs::remove_dir_all(&pasta);
    let _ = std::fs::remove_dir_all(&pasta2);
}

#[tokio::test]
async fn chave_do_produtor_sai_da_base_de_dados() {
    // simula a passagem de uma instalação de desenvolvimento para a versão oficial
    if crate::produtor::chave_agt().unwrap().is_some() {
        return;
    }
    let (st, pasta) = base_com_admin("chave-produtor").await;
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    assert_eq!(fr["documento"]["hash_versao"], "0", "sem certificado: HashControl 0");
    let antes = agt::chave(&st.pool).await.unwrap();
    // o que a versão oficial faz no arranque: a chave privada que estava na base sai, fica a pública
    let produtor = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
    agt::retirar_chave_da_base(&st.pool, &produtor).await.unwrap();
    let privada: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(agt::CHAVE_CONFIG).fetch_optional(&st.pool).await.unwrap();
    assert!(privada.is_none(), "a chave privada já não está na base de dados");
    let arquivadas: Vec<(String, String)> = sqlx::query_as("SELECT chave_publica, origem FROM agt_chaves").fetch_all(&st.pool).await.unwrap();
    assert_eq!(arquivadas.len(), 1);
    assert_eq!(arquivadas[0], (agt::chave_publica_pem(&antes).unwrap(), "DESENVOLVIMENTO".to_string()));
    // os documentos já assinados continuam verificáveis pela chave pública arquivada
    let v = rpc(&st, "agt.verificarIntegridade", Value::Null).await;
    assert_eq!(v["series"][0]["total_erros"], 0, "{v}");
    // com certificado configurado, os documentos novos levam a versão da chave
    rpc(&st, "agt.guardarCertificado", json!({ "certificado": "123" })).await;
    let fr2 = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    assert_eq!(fr2["documento"]["hash_versao"], "1");
    let info = rpc(&st, "agt.info", Value::Null).await;
    assert_eq!((info["chave_origem"].as_str(), info["certificado"].as_str()), (Some("DESENVOLVIMENTO"), Some("123")));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Só faz alguma coisa numa compilação com a chave do produtor embutida (KANGEST_AGT_CHAVE), como no CI.
#[tokio::test]
async fn chave_embutida_quando_existe() {
    let Some(produtor) = crate::produtor::chave_agt().unwrap() else { return };
    let (st, pasta) = base_com_admin("chave-embutida").await;
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let privada: Option<String> = sqlx::query_scalar("SELECT valor FROM config WHERE chave = ?").bind(agt::CHAVE_CONFIG).fetch_optional(&st.pool).await.unwrap();
    assert!(privada.is_none(), "com a chave do produtor, nenhuma chave privada fica na base de dados");
    let info = rpc(&st, "agt.info", Value::Null).await;
    assert_eq!(info["chave_origem"], "PRODUTOR");
    assert_eq!(info["chave_publica"].as_str().unwrap(), agt::chave_publica_pem(produtor).unwrap());
    assert!(rpc_err(&st, "agt.importarChave", json!({ "pem": "x" })).await.contains("já traz a chave"));
    assert_eq!(rpc(&st, "agt.verificarIntegridade", Value::Null).await["ok"], true);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn segredos_cifrados_na_configuracao() {
    let (st, pasta) = base_com_admin("segredos").await;
    let pem = {
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap().to_pkcs8_pem(LineEnding::LF).unwrap().to_string()
    };
    rpc(&st, "fe.guardar", json!({ "ativo": false, "ambiente": "HML", "utilizador": "produtor", "senha": "senha-secreta", "numero_validacao": "C_1",
        "estabelecimento": "SEDE", "chave_contribuinte": pem })).await;
    let bruto = |k: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?").bind(k).fetch_one(&pool).await.unwrap() }
    };
    for k in ["fe.senha", "fe.chave_contribuinte"] {
        let v = bruto(k).await;
        assert!(v.starts_with("kgs1:") && !v.contains("BEGIN") && !v.contains("senha-secreta"), "{k} fica cifrado: {v}");
    }
    // o programa continua a ler os valores
    let estado = rpc(&st, "fe.estado", Value::Null).await;
    assert_eq!((estado["senha"].as_bool(), estado["chave_contribuinte"].as_bool()), (Some(true), Some(true)));
    // segredos gravados em texto por versões anteriores são cifrados no arranque
    sqlx::query("UPDATE config SET valor = 'antiga' WHERE chave = 'fe.senha'").execute(&st.pool).await.unwrap();
    assert_eq!(crate::segredos::cifrar_existentes(&st.pool).await.unwrap(), 1);
    assert!(bruto("fe.senha").await.starts_with("kgs1:"));
    assert_eq!(crate::segredos::ler(&st.pool, "fe.senha").await.unwrap().as_deref(), Some("antiga"));
    // a chave de desenvolvimento também fica cifrada
    if crate::produtor::chave_agt().unwrap().is_none() {
        assert!(bruto("agt.chave_privada").await.starts_with("kgs1:"));
    }
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn actualizacao_conferida_antes_de_instalar() {
    let (st, pasta) = base_com_admin("actualizacao").await;
    let origem = pasta.join("origem");
    std::fs::create_dir_all(&origem).unwrap();
    let plat = rpc(&st, "atualizacao.estado", Value::Null).await["plataforma"].as_str().unwrap().to_string();
    let pacote = b"instalador de teste";
    std::fs::write(origem.join("KanGest_9.9.9.bin"), pacote).unwrap();
    let manifesto = json!({ "versao": "9.9.9", "data": "2026-12-01", "notas": "teste",
        "pacotes": { plat: { "ficheiro": "KanGest_9.9.9.bin", "sha256": crate::services::atualizacao::sha256_hex(pacote) } } });
    std::fs::write(origem.join("kangest-atualizacao.json"), manifesto.to_string()).unwrap();
    rpc(&st, "atualizacao.guardar", json!({ "fonte": origem.display().to_string() })).await;
    let d = rpc(&st, "atualizacao.descarregar", Value::Null).await;
    let ficheiro = d["ficheiro"].as_str().unwrap().to_string();
    // trocado depois de descarregado: não se abre
    std::fs::write(&ficheiro, b"outro programa").unwrap();
    assert!(rpc_err(&st, "atualizacao.instalar", json!({ "ficheiro": ficheiro })).await.contains("alterado"));
    // sem chave embutida (desenvolvimento) a assinatura continua opcional; com ela é obrigatória
    if crate::produtor::chave_actualizacoes().is_some() {
        assert_eq!(rpc(&st, "atualizacao.estado", Value::Null).await["chave_embutida"], true);
    }
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn copias_externas_cifradas_e_restauro() {
    let (st, pasta) = base_com_admin("copias").await;
    let pen = pasta.join("pen");
    std::fs::create_dir_all(&pen).unwrap();
    rpc(&st, "artigos.guardar", json!({ "descricao": "Antes da cópia", "taxa_iva_id": 1, "preco1": 100000 })).await;
    // pasta externa inexistente é recusada; a pen, aceite, com cópias cifradas
    assert!(rpc_err(&st, "sistema.copiasGuardar", json!({ "intervalo_horas": 4, "manter": 5, "pasta_externa": pasta.join("nao-existe").display().to_string() })).await.contains("não existe"));
    assert!(rpc_err(&st, "sistema.copiasGuardar", json!({ "intervalo_horas": 4, "manter": 5, "pasta_externa": pen.display().to_string(), "cifrar_externa": true })).await.contains("senha"));
    rpc(&st, "sistema.copiasGuardar", json!({ "intervalo_horas": 2, "manter": 5, "pasta_externa": pen.display().to_string(), "cifrar_externa": true, "senha": "senha-da-pen-123" })).await;
    let c = rpc(&st, "sistema.copiaAgora", Value::Null).await;
    let externa = c["externa"].as_str().expect("cópia na pen").to_string();
    assert!(externa.ends_with(".kgc"));
    let bytes = std::fs::read(&externa).unwrap();
    assert!(bytes.starts_with(b"KGCOPIA1") && !bytes.windows(15).any(|w| w == b"Antes da c\xc3\xb3pia"), "a cópia da pen está cifrada");
    let lista = rpc(&st, "sistema.copias", Value::Null).await;
    assert_eq!(lista["externas"].as_array().unwrap().len(), 1);
    assert_eq!(lista["copias"][0]["verificada"], true);
    assert!(lista["estado"]["erro_externa"].is_null());

    // depois da cópia entra outro artigo; restaurar a cópia da pen (com a senha) volta ao estado anterior
    rpc(&st, "artigos.guardar", json!({ "descricao": "Depois da cópia", "taxa_iva_id": 1, "preco1": 100000 })).await;
    assert!(rpc_err(&st, "sistema.restaurar", json!({ "ficheiro": externa, "senha": "errada-errada" })).await.contains("Senha errada"));
    let r = rpc(&st, "sistema.restaurar", json!({ "ficheiro": externa, "senha": "senha-da-pen-123" })).await;
    assert_eq!(r["reiniciar"], true);
    assert!(std::path::Path::new(r["copia_da_base_atual"].as_str().unwrap()).exists(), "a base actual fica guardada antes do restauro");
    // o que o arranque faz: fecha a base, troca o ficheiro e volta a abrir
    fechar_de_vez(&st.pool).await;
    assert!(super::sistema::aplicar_restauro_pendente(&pasta).unwrap().is_some());
    let pool = db::abrir(&pasta.join("kigest.db")).await.unwrap();
    let nomes: Vec<String> = sqlx::query_scalar("SELECT descricao FROM artigos ORDER BY id").fetch_all(&pool).await.unwrap();
    assert!(nomes.contains(&"Antes da cópia".to_string()) && !nomes.contains(&"Depois da cópia".to_string()), "{nomes:?}");
    pool.close().await;

    // um ficheiro que não é uma cópia do KanGest é recusado
    let (st2, pasta2) = base_com_admin("copias-2").await;
    let lixo = pasta2.join("lixo.db");
    std::fs::write(&lixo, b"nada de sqlite aqui").unwrap();
    let e = rpc_err(&st2, "sistema.restaurar", json!({ "ficheiro": lixo.display().to_string() })).await;
    assert!(e.contains("não é uma cópia de segurança"), "{e}");
    // o restauro inicial só existe enquanto o KanGest não está configurado
    assert!(rpc_err(&st2, "setup.restaurar", json!({ "ficheiro": externa })).await.contains("já está configurado"));

    // uma cópia feita por uma versão mais recente (com migrações que esta não conhece) não se restaura:
    // depois da troca, a base deixaria de abrir e o programa não arrancava
    let nova = pasta2.join("kangest-versao-nova.db");
    super::sistema::copiar(&st2.pool, &nova).await.unwrap();
    {
        let p = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&nova)).await.unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (999999, 'de uma versão futura', 1, x'00', 0)")
            .execute(&p)
            .await
            .unwrap();
        p.close().await;
    }
    assert!(db::abrir(&nova).await.is_err(), "esta versão não abre uma base com migrações que não conhece");
    let e = rpc_err(&st2, "sistema.restaurar", json!({ "ficheiro": nova.display().to_string() })).await;
    assert!(e.contains("versão mais recente"), "{e}");
    assert!(!pasta2.join("restaurar.json").exists(), "nada fica marcado para o arranque");
    let _ = std::fs::remove_dir_all(&pasta);
    let _ = std::fs::remove_dir_all(&pasta2);
}

/// Dois postos a gravar ao mesmo tempo: a segunda gravação espera pela primeira em vez de falhar com
/// «database is locked» (as transacções de escrita começam com BEGIN IMMEDIATE).
#[tokio::test]
async fn gravacoes_simultaneas_esperam_pela_vez() {
    let (st, pasta) = base_com_admin("gravacoes-simultaneas").await;
    // outra ligação (outro posto) a meio de uma gravação, que só termina daqui a 300 ms
    let mut outra = crate::db::transacao(&st.pool).await.unwrap();
    sqlx::query("UPDATE empresa SET nome = nome WHERE id = 1").execute(&mut *outra).await.unwrap();
    let terminar = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        outra.commit().await.unwrap();
    };
    let gravar = dispatch(&st, "artigos.guardar", json!({ "descricao": "Gravado à espera", "taxa_iva_id": 1, "preco1": 100000 }));
    let (r, _) = tokio::join!(gravar, terminar);
    assert!(r.is_ok(), "a gravação devia esperar pela outra: {:?}", r.err());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artigos WHERE descricao = 'Gravado à espera'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(n, 1);
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Base restaurada noutro computador (compilação de desenvolvimento): a chave cifrada não se lê,
/// gera-se outra e os documentos antigos continuam verificáveis.
#[tokio::test]
async fn chave_ilegivel_depois_de_restaurar() {
    if crate::produtor::chave_agt().unwrap().is_some() {
        return;
    }
    let (st, pasta) = base_com_admin("chave-ilegivel").await;
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let venda = json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] });
    rpc(&st, "documentos.emitir", venda.clone()).await;
    // o que acontece com a chave mestra de outra máquina: o valor cifrado não abre
    sqlx::query("UPDATE config SET valor = 'kgs1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA' WHERE chave = ?").bind(agt::CHAVE_CONFIG).execute(&st.pool).await.unwrap();
    rpc(&st, "documentos.emitir", venda).await;
    let v = rpc(&st, "agt.verificarIntegridade", Value::Null).await;
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["chaves"], 2);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn fecho_de_caixa_cego_por_notas_e_formas_de_pagamento() {
    let (st, pasta) = base_com_admin("fecho-cego").await;
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    // o posto fecha às cegas; a operadora (perfil Caixa) não tem «ver totais»
    rpc(&st, "posto.guardar", json!({ "id": 1, "descricao": "Caixa 1", "armazem_id": 1, "opcoes": { "fecho_cego": true } })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "senha": "ana12345", "perfil": "CAIXA", "mudar_senha": false })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "ana", "senha": "ana12345" })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 500000 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 200000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 2 }], "pagamentos": [{ "modo": "MB", "valor": 228000 }] })).await;
    let aberto = rpc(&st, "caixa.estado", Value::Null).await;
    assert_eq!(aberto["cego"], true);
    assert!(aberto["numerario_esperado"].is_null() && aberto["total_vendido"].is_null(), "às cegas: {aberto}");
    assert!(aberto["por_modo"].as_array().unwrap().iter().all(|m| m["total"].is_null()));

    // nota que não existe é recusada; a contagem por notas dá o total
    assert!(rpc_err(&st, "caixa.fechar", json!({ "contagem": { "3000": 1 } })).await.contains("3000"));
    // esperado em numerário: 5 000 + 1 140 = 6 140 Kz; contados 6 100 (faltam 40) e o talão do TPA certo
    let z = rpc(&st, "caixa.fechar", json!({ "contagem": { "5000": 1, "1000": 1, "100": 1 }, "contado_modos": { "MB": 228000 }, "observacoes": "Faltam 40 Kz" })).await;
    assert_eq!(z["sessao"]["numero_z"], 1);
    assert_eq!(z["sessao"]["valor_contado"], 610000);
    assert_eq!(z["sessao"]["contagem"]["5000"], 1);
    assert_eq!(z["numerario_esperado"], 614000, "depois de fechar, o relatório Z mostra tudo");
    let conf = |c: &str| z["conferencia"].as_array().unwrap().iter().find(|m| m["codigo"] == c).cloned().unwrap();
    assert_eq!((conf("NU")["diferenca"].as_i64(), conf("MB")["diferenca"].as_i64()), (Some(-4000), Some(0)));
    // o fecho seguinte do mesmo posto é o Z n.º 2
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let z2 = rpc(&st, "caixa.fechar", json!({ "valor_contado": 0 })).await;
    assert_eq!(z2["sessao"]["numero_z"], 2);
    // quem tem «ver totais» (o gestor/administrador) vê o esperado mesmo às cegas
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 100 })).await;
    assert_eq!(rpc(&st, "caixa.estado", Value::Null).await["numerario_esperado"], 100);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn nif_angolano_validado() {
    let (st, pasta) = base_com_admin("nif").await;
    // válidos: empresa (10 algarismos), BI (com espaços e minúsculas, normalizado) e estrangeiro com outro formato
    let a = rpc(&st, "entidades.guardar", json!({ "tipo": "C", "nome": "Empresa A", "nif": "5417 189 230" })).await;
    assert_eq!(rpc(&st, "entidades.obter", json!({ "id": a["id"] })).await["nif"], "5417189230");
    let b = rpc(&st, "entidades.guardar", json!({ "tipo": "C", "nome": "Maria", "nif": "001234567la041" })).await;
    assert_eq!(rpc(&st, "entidades.obter", json!({ "id": b["id"] })).await["nif"], "001234567LA041");
    rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Fornecedor PT", "nif": "PT509999999", "pais": "PT" })).await;
    // inválidos em Angola
    for nif in ["12345", "54171892301", "ABC1234567", "001234567L4041"] {
        let e = rpc_err(&st, "entidades.guardar", json!({ "tipo": "C", "nome": "X", "nif": nif })).await;
        assert!(e.contains("NIF inválido"), "{nif}: {e}");
    }
    // uma ficha antiga com NIF mal escrito continua a gravar-se se o NIF não mudar
    sqlx::query("UPDATE entidades SET nif = '123' WHERE id = ?").bind(a["id"].as_i64().unwrap()).execute(&st.pool).await.unwrap();
    rpc(&st, "entidades.guardar", json!({ "id": a["id"], "tipo": "C", "nome": "Empresa A, Lda", "nif": "123" })).await;
    assert!(super::entidades::nif_angolano_valido("999999999"));
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn precos_com_iva_incluido() {
    let (st, pasta) = base_com_admin("iva-incluido").await;
    // PVP de prateleira: 1 000,00 Kz com IVA (14%)
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Leite", "tipo": "S", "taxa_iva_id": 1, "preco1": 0, "pvp1": 100000 })).await["id"].as_i64().unwrap();
    let art = rpc(&st, "artigos.obter", json!({ "id": a })).await;
    assert_eq!((art["preco1"].as_i64(), art["pvp1"].as_i64()), (Some(87719), Some(100000)), "o preço sem IVA sai do PVP");
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let venda = |q: i64| json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": q }], "pagamentos": [{ "modo": "NU", "valor": 100000 * q }] });

    // sem a opção: o cálculo antigo (a partir do preço sem IVA) dá 2 999,99
    let antes = rpc(&st, "documentos.emitir", venda(3)).await;
    assert_eq!(antes["documento"]["total"], 299999);

    // com a opção: 3 × 1 000,00 = 3 000,00 e o IVA sai do total
    rpc(&st, "parametros.opcoesGuardar", json!({ "precos_com_iva": true })).await;
    assert_eq!(rpc(&st, "tabelas", Value::Null).await["precos_com_iva"], true);
    let fr = rpc(&st, "documentos.emitir", venda(3)).await;
    let d = &fr["documento"];
    assert_eq!((d["total"].as_i64(), d["total_iva"].as_i64(), d["total_liquido"].as_i64()), (Some(300000), Some(36842), Some(263158)));
    assert_eq!(fr["linhas"][0]["pvp_unitario"], 100000);
    assert_eq!(fr["documento"]["troco"], 0);

    // desconto de linha (10%) e desconto global em Kz, como o cliente os vê
    let com_desc = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": a, "quantidade": 1, "desconto_pct": 10 }], "pagamentos": [{ "modo": "NU", "valor": 90000 }] })).await;
    assert_eq!(com_desc["documento"]["total"], 90000);
    let global = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "desconto_global_valor": 50000, "linhas": [{ "artigo_id": a, "quantidade": 3 }], "pagamentos": [{ "modo": "NU", "valor": 250000 }] })).await;
    assert_eq!(global["documento"]["total"], 250000, "3 000,00 − 500,00");

    // a nota de crédito parte do PVP da factura: 1 unidade da venda de 3 × 1 000,00 = 1 000,00
    let nc = rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": 1, "doc_origem_id": d["id"], "motivo": "Devolução", "linhas": [{ "artigo_id": a, "quantidade": 1 }] })).await;
    assert_eq!(nc["documento"]["total"], 100000);
    // a cadeia e os totais continuam coerentes
    assert_eq!(rpc(&st, "agt.verificarIntegridade", Value::Null).await["ok"], true);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn codigos_de_barras_embalagens_e_balanca() {
    let (st, pasta) = base_com_admin("codigos").await;
    let agua = rpc(&st, "artigos.guardar", json!({ "descricao": "Água 1,5L", "taxa_iva_id": 1, "preco1": 10000, "codigo_barras": "5601234567890",
        "codigos": [{ "codigo": "15601234567897", "descricao": "Pack de 12", "quantidade": 12 }] })).await["id"].as_i64().unwrap();
    let lido = rpc(&st, "artigos.codigo", json!({ "codigo": "15601234567897" })).await;
    assert_eq!((lido["id"].as_i64(), lido["quantidade_lida"].as_f64()), (Some(agua), Some(12.0)));
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": "5601234567890" })).await["quantidade_lida"].is_null());
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": agua })).await["codigos"][0]["quantidade"].as_f64(), Some(12.0));
    // editar sem referência mantém a que o artigo já tem (não gera outra)
    let referencia = rpc(&st, "artigos.obter", json!({ "id": agua })).await["referencia"].as_str().unwrap().to_string();
    let r = rpc(&st, "artigos.guardar", json!({ "id": agua, "descricao": "Água 1,5L (garrafa)", "taxa_iva_id": 1, "preco1": 10000, "codigo_barras": "5601234567890" })).await;
    assert_eq!(r["referencia"], referencia.as_str());
    // a API lê as mesmas embalagens: o pack vale 12 unidades
    let (id, leitura) = super::catalogo::ler_codigo(&st.pool, "15601234567897").await.unwrap().unwrap();
    assert_eq!((id, leitura.unwrap()["quantidade"].as_f64()), (agua, Some(12.0)));
    assert_eq!(super::catalogo::ler_codigo(&st.pool, "5601234567890").await.unwrap(), Some((agua, None)));
    // o mesmo código não pode ser de dois artigos
    let e = rpc_err(&st, "artigos.guardar", json!({ "descricao": "Outra", "taxa_iva_id": 1, "preco1": 100, "codigos": [{ "codigo": "5601234567890" }] })).await;
    assert!(e.contains("já está em uso"), "{e}");

    // balança: carne a 3 000,00 Kz/kg, PLU 123
    let carne = rpc(&st, "artigos.guardar", json!({ "descricao": "Carne de vaca", "taxa_iva_id": 1, "preco1": 0, "pvp1": 300000, "codigo_balanca": "123" })).await["id"].as_i64().unwrap();
    let ean = |doze: &str| format!("{doze}{}", super::catalogo::ean13_controlo(doze).unwrap());
    // etiqueta de peso: 21 00123 01250 → 1,250 kg
    let peso = rpc(&st, "artigos.codigo", json!({ "codigo": ean("210012301250") })).await;
    assert_eq!((peso["id"].as_i64(), peso["quantidade_lida"].as_f64()), (Some(carne), Some(1.25)));
    // etiqueta de preço: 22 00123 03750 → 3 750 Kz → 1,25 kg
    let preco = rpc(&st, "artigos.codigo", json!({ "codigo": ean("220012303750") })).await;
    assert_eq!(preco["quantidade_lida"].as_f64(), Some(1.25));
    // algarismo de controlo errado: não é uma etiqueta
    let certo = ean("210012301250");
    let errado = format!("{}{}", &certo[..12], (certo[12..].parse::<u32>().unwrap() + 1) % 10);
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": errado })).await.is_null());
    // vender pela etiqueta: o total bate com o preço da etiqueta
    rpc(&st, "parametros.opcoesGuardar", json!({ "precos_com_iva": true })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": carne, "quantidade": 50, "preco_unitario": 150000 }] })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let v = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": carne, "quantidade": 1.25 }], "pagamentos": [{ "modo": "NU", "valor": 375000 }] })).await;
    assert_eq!(v["documento"]["total"], 375000);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn promocoes_por_regras_e_cupoes() {
    let (st, pasta) = base_com_admin("promocoes").await;
    let familia = rpc(&st, "tabela.guardar", json!({ "tabela": "familias", "descricao": "Bebidas" })).await["id"].as_i64().unwrap();
    let sumo = rpc(&st, "artigos.guardar", json!({ "descricao": "Sumo", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000, "familia_id": familia })).await["id"].as_i64().unwrap();
    let agua = rpc(&st, "artigos.guardar", json!({ "descricao": "Água", "tipo": "S", "taxa_iva_id": 1, "preco1": 10000, "familia_id": familia })).await["id"].as_i64().unwrap();
    // regras inválidas são recusadas
    assert!(rpc_err(&st, "promocoes.regraGuardar", json!({ "descricao": "x", "tipo": "LEVE_PAGUE", "artigo_id": sumo, "leve": 2, "pague": 3 })).await.contains("maior"));
    rpc(&st, "promocoes.regraGuardar", json!({ "descricao": "Leve 3 pague 2", "tipo": "LEVE_PAGUE", "artigo_id": sumo, "leve": 3, "pague": 2 })).await;
    rpc(&st, "promocoes.regraGuardar", json!({ "descricao": "Bebidas: 5% a partir de 10", "tipo": "QUANTIDADE", "familia_id": familia, "quantidade_min": 10, "desconto_pct": 5 })).await;
    let calc = rpc(&st, "promocoes.calcular", json!({ "linhas": [{ "artigo_id": sumo, "quantidade": 3 }, { "artigo_id": agua, "quantidade": 12 }] })).await;
    assert_eq!(calc[0]["regra"], "Leve 3 pague 2");
    assert!((calc[0]["desconto"].as_f64().unwrap() - 33.3333).abs() < 0.001, "{calc}");
    assert_eq!((calc[1]["desconto"].as_f64(), calc[1]["regra"].as_str()), (Some(5.0), Some("Bebidas: 5% a partir de 10")));

    // a operadora (sem permissão de desconto) vende com a promoção sem pedir PIN
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Rita", "login": "rita", "senha": "rita1234", "perfil": "CAIXA", "mudar_senha": false })).await;
    assert!(rpc_err(&st, "cupoes.validar", json!({ "codigo": "nada", "total": 0 })).await.contains("não existe"));
    rpc(&st, "promocoes.cupaoGuardar", json!({ "codigo": "natal10", "descricao": "Natal", "tipo": "PCT", "valor": 10, "valor_minimo": 100000, "utilizacoes_max": 1 })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "rita", "senha": "rita1234" })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let v = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": sumo, "quantidade": 3 }], "pagamentos": [{ "modo": "NU", "valor": 228000 }] })).await;
    assert_eq!(v["documento"]["total"], 228000, "3 sumos pelo preço de 2 (2 000,00 + IVA)");
    // mais desconto do que a promoção já pede PIN
    let e = rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": sumo, "quantidade": 3, "desconto_pct": 50 }], "pagamentos": [{ "modo": "NU", "valor": 300000 }] })).await;
    assert!(e.contains("supervisor"), "{e}");

    // cupão: compra mínima, desconto sem PIN, uma só utilização, devolvida ao anular
    assert!(rpc_err(&st, "cupoes.validar", json!({ "codigo": "NATAL10", "total": 50000 })).await.contains("a partir de"));
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "cupao": "NATAL10", "linhas": [{ "artigo_id": agua, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 11400 }] })).await.contains("a partir de"));
    let c = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "cupao": "natal10", "linhas": [{ "artigo_id": sumo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    assert_eq!((c["documento"]["total"].as_i64(), c["documento"]["cupao"].as_str()), (Some(102600), Some("NATAL10")), "1 140,00 − 10%");
    assert!(rpc_err(&st, "documentos.emitir", json!({ "tipo": "FR", "cupao": "NATAL10", "linhas": [{ "artigo_id": sumo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await.contains("número de vezes"));
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    rpc(&st, "documentos.anular", json!({ "id": c["documento"]["id"], "motivo": "Venda repetida" })).await;
    assert_eq!(rpc(&st, "promocoes.listar", Value::Null).await["cupoes"][0]["utilizacoes"], 0);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn sugestao_de_encomendas_e_lancamentos_contabilisticos() {
    let (st, pasta) = base_com_admin("sugestao").await;
    let forn = rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Fornecedor SA", "nif": "5410000000" })).await["id"].as_i64().unwrap();
    let cli = rpc(&st, "entidades.guardar", json!({ "tipo": "C", "nome": "Cliente Lda", "nif": "5417000000" })).await["id"].as_i64().unwrap();
    let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz", "taxa_iva_id": 1, "preco1": 100000, "qtd_minima": 10, "qtd_maxima": 50 })).await["id"].as_i64().unwrap();
    let oleo = rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    // compra do arroz (5 un. a 600,00) e entrada do óleo (40 un.)
    rpc(&st, "documentos.emitir", json!({ "tipo": "FC", "entidade_id": forn, "doc_externo": "FT A/1", "linhas": [{ "artigo_id": arroz, "quantidade": 5, "preco_unitario": 60000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": oleo, "quantidade": 40, "preco_unitario": 50000 }] })).await;
    // vendem-se 30 óleos (1 por dia nos últimos 30 dias, em média)
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": oleo, "quantidade": 30 }], "pagamentos": [{ "modo": "NU", "valor": 3420000 }] })).await;
    // encomenda em curso de 3 arrozes: 5 + 3 = 8 continua abaixo do mínimo (10)
    rpc(&st, "documentos.emitir", json!({ "tipo": "VNE", "entidade_id": forn, "doc_externo": "ENC-1", "linhas": [{ "artigo_id": arroz, "quantidade": 3, "preco_unitario": 60000 }] })).await;
    let s = rpc(&st, "compras.sugestao", json!({ "dias": 30, "cobertura": 15 })).await;
    let linha = |id: i64| s["linhas"].as_array().unwrap().iter().find(|l| l["artigo_id"] == id).cloned().unwrap();
    let a = linha(arroz);
    assert_eq!((a["sugerido"].as_f64(), a["fornecedor"].as_str(), a["custo"].as_i64()), (Some(42.0), Some("Fornecedor SA"), Some(60000)), "até ao máximo: 50 − 5 − 3; {a}");
    let o = linha(oleo);
    assert_eq!((o["sugerido"].as_f64(), o["motivo"].as_str()), (Some(5.0), Some("não cobre os dias pedidos")), "15 dias × 1/dia − 10 em stock; {o}");

    // diário contabilístico: vendas a pronto e a crédito, recibo, nota de crédito e compra
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": oleo, "quantidade": 2 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "RC", "entidade_id": cli, "liquidar": [{ "documento_id": ft["documento"]["id"], "valor": 114000 }], "pagamentos": [{ "modo": "TB", "valor": 114000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": ft["documento"]["id"], "motivo": "Devolução", "linhas": [{ "artigo_id": oleo, "quantidade": 1 }] })).await;
    let hoje = db::hoje();
    let l = rpc(&st, "contabilidade.lancamentos", json!({ "de": hoje, "ate": hoje })).await;
    assert_eq!(l["equilibrado"], true, "débito = crédito: {}", l["balancete"]);
    let saldo = |conta: &str| l["balancete"].as_array().unwrap().iter().find(|c| c["conta"] == conta).map(|c| c["saldo"].as_i64().unwrap()).unwrap_or(0);
    // caixa: FR de 30 óleos (34 200,00); bancos: recibo (1 140,00); clientes: 2 280 − 1 140 − 1 140 = 0
    assert_eq!((saldo("45.1"), saldo("43.1"), saldo("31.1.1")), (3420000, 114000, 0));
    // vendas (crédito): 30 + 2 − 1 = 31 óleos a 1 000,00; IVA liquidado 14%
    assert_eq!((saldo("61.1"), saldo("34.5.3")), (-3100000, -434000));
    // compras: 600,00 × 5 + IVA, contra fornecedores
    assert_eq!((saldo("21.1"), saldo("34.5.2"), saldo("32.1.1")), (300000, 42000, -342000));
    // contas configuráveis
    rpc(&st, "contabilidade.contasGuardar", json!({ "caixa": "45.1.1" })).await;
    let l2 = rpc(&st, "contabilidade.lancamentos", json!({ "de": hoje, "ate": hoje })).await;
    assert!(l2["balancete"].as_array().unwrap().iter().any(|c| c["conta"] == "45.1.1"));
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn fidelizacao_pontos_ganhos_pagos_e_devolvidos() {
    let (st, pasta) = base_com_admin("fidelizacao").await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço de lavagem", "tipo": "S", "taxa_iva_id": 1, "preco1": 1000000 })).await["id"].as_i64().unwrap();
    let ana = rpc(&st, "entidades.guardar", json!({ "nome": "Ana Domingos", "nif": "5417000001" })).await["id"].as_i64().unwrap();
    let saldo = |v: Value| v["pontos"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let venda = |cliente: i64, qtd: i64, pagamentos: Value| json!({ "tipo": "FR", "entidade_id": cliente, "linhas": [{ "artigo_id": art, "quantidade": qtd }], "pagamentos": pagamentos });

    // desactivada: não dá pontos nem aceita pagamento com pontos
    rpc(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "NU", "valor": 1140000 }]))).await;
    assert_eq!(saldo(rpc(&st, "fidelizacao.saldo", json!({ "entidade_id": ana })).await), 0);
    assert!(rpc_err(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "DE", "valor": 1000 }, { "modo": "NU", "valor": 1139000 }]))).await.contains("não está activa"));

    // 1 ponto por cada 1 000 Kz, cada ponto vale 10 Kz, a partir de 50 pontos
    assert!(rpc_err(&st, "fidelizacao.configGuardar", json!({ "ativa": true, "compra_por_ponto": 0, "valor_ponto": 1000, "minimo_resgate": 50 })).await.contains("Indique"));
    rpc(&st, "fidelizacao.configGuardar", json!({ "ativa": true, "compra_por_ponto": 100000, "valor_ponto": 1000, "minimo_resgate": 50 })).await;
    let t = rpc(&st, "tabelas", Value::Null).await;
    assert_eq!(t["fidelizacao"]["ativa"], true);
    assert!(t["modos_pagamento"].as_array().unwrap().iter().all(|m| m["codigo"] != "DE"), "os pontos têm botão próprio no POS");

    // ganhos: 22 800 Kz → 22 pontos; a crédito (FT) também: 34 200 Kz → 34; consumidor final não acumula
    rpc(&st, "documentos.emitir", venda(ana, 2, json!([{ "modo": "NU", "valor": 2280000 }]))).await;
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": ana, "linhas": [{ "artigo_id": art, "quantidade": 3 }] })).await;
    rpc(&st, "documentos.emitir", venda(1, 1, json!([{ "modo": "NU", "valor": 1140000 }]))).await;
    assert_eq!(saldo(rpc(&st, "fidelizacao.saldo", json!({ "entidade_id": ana })).await), 56);
    assert_eq!(rpc(&st, "fidelizacao.saldo", json!({ "entidade_id": ana })).await["pode_pagar"], true);
    assert!(rpc_err(&st, "documentos.emitir", venda(1, 1, json!([{ "modo": "DE", "valor": 1000 }, { "modo": "NU", "valor": 1139000 }]))).await.contains("cliente identificado"));

    // paga 560 Kz com os 56 pontos e o resto em numerário: só o numerário dá pontos (10 840 Kz → 10)
    let pp = rpc(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "DE", "valor": 56000 }, { "modo": "NU", "valor": 1084000 }]))).await;
    assert_eq!(pp["documento"]["total"], 1140000, "pagar com pontos não é desconto: a factura e o IVA ficam iguais");
    assert_eq!(pp["fidelizacao"], json!({ "ganhos": 10, "gastos": 56, "saldo": 10 }));
    assert!(pp["pagamentos"].as_array().unwrap().iter().any(|p| p["modo_codigo"] == "DE" && p["valor"] == 56000));

    // abaixo do mínimo; ajuste manual; pontos insuficientes
    assert!(rpc_err(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "DE", "valor": 10000 }, { "modo": "NU", "valor": 1130000 }]))).await.contains("a partir de 50"));
    assert!(rpc_err(&st, "fidelizacao.ajustar", json!({ "entidade_id": ana, "pontos": 100, "motivo": "" })).await.contains("motivo"));
    assert_eq!(rpc(&st, "fidelizacao.ajustar", json!({ "entidade_id": ana, "pontos": 100, "motivo": "Campanha de aniversário" })).await["saldo"], 110);
    let e = rpc_err(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "DE", "valor": 120000 }, { "modo": "NU", "valor": 1020000 }]))).await;
    assert!(e.contains("Pontos insuficientes") && e.contains("120"), "{e}");
    // os pontos não dão troco
    assert!(rpc_err(&st, "documentos.emitir", venda(ana, 1, json!([{ "modo": "DE", "valor": 1200000 }]))).await.contains("troco"));

    // nota de crédito da venda paga com pontos: estorna os 10 ganhos, devolve os 56 em pontos e só o numerário em dinheiro
    let nc = rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": ana, "doc_origem_id": pp["documento"]["id"], "motivo": "Serviço não prestado",
                                                    "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    assert_eq!(nc["fidelizacao"], json!({ "ganhos": 56, "gastos": 10, "saldo": 156 }));
    let devolvido: i64 = sqlx::query_scalar("SELECT valor FROM caixa_movimentos WHERE descricao = ?").bind(format!("Devolução {}", nc["documento"]["numero_doc"].as_str().unwrap())).fetch_one(&st.pool).await.unwrap();
    assert_eq!(devolvido, 1084000, "em dinheiro só o que foi pago em dinheiro");
    // segunda nota de crédito da mesma venda: nada mais a creditar
    let e = rpc_err(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": ana, "doc_origem_id": pp["documento"]["id"], "motivo": "Repetida",
                                                      "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
    assert!(e.contains("creditar"), "{e}");

    // anular a FT tira os 34 pontos que deu
    rpc(&st, "documentos.anular", json!({ "id": ft["documento"]["id"], "motivo": "Factura em duplicado" })).await;
    assert_eq!(saldo(rpc(&st, "fidelizacao.saldo", json!({ "entidade_id": ana })).await), 122);
    let movs = rpc(&st, "fidelizacao.movimentos", json!({ "entidade_id": ana })).await;
    let tipos: Vec<&str> = movs.as_array().unwrap().iter().map(|m| m["tipo"].as_str().unwrap()).collect();
    for t in ["GANHO", "RESGATE", "AJUSTE", "ESTORNO", "DEVOLUCAO", "ANULACAO"] {
        assert!(tipos.contains(&t), "falta o movimento {t}: {tipos:?}");
    }
    let lista = rpc(&st, "fidelizacao.clientes", json!({ "q": "ana" })).await;
    assert_eq!((lista[0]["saldo"].as_i64(), lista[0]["gastos"].as_i64()), (Some(122), Some(56)));

    // o pagamento com pontos não entra na tesouraria; na contabilidade vai para a conta própria e o diário fecha
    let fluxo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vw_fluxo WHERE rubrica = 'Pontos de fidelização'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(fluxo, 0);
    let hoje = db::hoje();
    let l = rpc(&st, "contabilidade.lancamentos", json!({ "de": hoje, "ate": hoje })).await;
    assert_eq!(l["equilibrado"], true, "{l}");
    assert!(l["linhas"].as_array().unwrap().iter().any(|x| x["conta"] == "37.9" && x["debito"] == 56000), "{l}");

    // o SAF-T leva o mecanismo «DE» (dinheiro electrónico: cartões de pontos)
    let saft = rpc(&st, "saft.exportar", json!({ "de": "2000-01-01", "ate": "2100-12-31" })).await;
    let xml = std::fs::read_to_string(saft["ficheiro"].as_str().unwrap()).unwrap();
    assert!(xml.contains("<PaymentMechanism>DE</PaymentMechanism>"));
    validar_xsd(saft["ficheiro"].as_str().unwrap());
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Parte HTML (base64) de uma mensagem MIME recebida pelo servidor SMTP falso.
fn html_da_mensagem(dados: &str) -> String {
    use base64::Engine;
    let i = dados.find("Content-Type: text/html").expect("parte HTML");
    let resto = &dados[i..];
    let corpo = &resto[resto.find("\r\n\r\n").unwrap() + 4..];
    let b64: String = corpo.lines().take_while(|l| !l.starts_with("--")).collect::<Vec<_>>().join("");
    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64.trim()).unwrap()).unwrap()
}

#[tokio::test]
async fn recibo_digital_por_email_e_whatsapp() {
    let (st, pasta) = base_com_admin("recibo-digital").await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Pão & Bolos <especial>", "tipo": "S", "taxa_iva_id": 1, "preco1": 50000 })).await["id"].as_i64().unwrap();
    let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Hotel Baía Azul", "nif": "5417002211", "telefone": "923 456 789", "email": "reservas@baia.ao" })).await["id"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 120000 }] })).await;
    let id = fr["documento"]["id"].as_i64().unwrap();
    let numero = fr["documento"]["numero_doc"].as_str().unwrap().to_string();

    // sem configuração não envia; configuração validada; a senha fica cifrada e não volta ao ecrã
    assert!(rpc_err(&st, "documentos.enviarEmail", json!({ "id": id, "para": "reservas@baia.ao" })).await.contains("não está configurado"));
    assert!(rpc_err(&st, "email.configGuardar", json!({ "servidor": "", "porta": 465, "remetente": "loja@kianda.ao" })).await.contains("servidor"));
    let (porta, rx) = crate::smtp::testes::servidor_falso();
    let c = rpc(&st, "email.configGuardar", json!({ "servidor": "127.0.0.1", "porta": porta, "seguranca": "NENHUMA", "utilizador": "loja@kianda.ao",
                                                   "remetente": "loja@kianda.ao", "copia": "arquivo@kianda.ao", "senha": "segredo" })).await;
    assert_eq!((c["tem_senha"].as_bool(), c.get("senha").is_none()), (Some(true), true));
    let guardada: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'email.senha'").fetch_one(&st.pool).await.unwrap();
    assert!(guardada.starts_with("kgs1:") && !guardada.contains("segredo"), "senha cifrada");
    // gravar sem senha mantém a anterior
    rpc(&st, "email.configGuardar", json!({ "servidor": "127.0.0.1", "porta": porta, "seguranca": "NENHUMA", "utilizador": "loja@kianda.ao", "remetente": "loja@kianda.ao", "copia": "arquivo@kianda.ao" })).await;
    rpc(&st, "email.testar", json!({ "para": "gerente@kianda.ao" })).await;
    let (cmds, _) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(cmds.iter().any(|c| c == "RCPT TO:<gerente@kianda.ao>"), "{cmds:?}");

    // o documento por email: HTML com as linhas (escapadas), o total, o rodapé da AGT; cópia oculta para o arquivo
    assert!(rpc_err(&st, "documentos.enviarEmail", json!({ "id": id, "para": "reservas@baia" })).await.contains("inválido"));
    let r = rpc(&st, "documentos.enviarEmail", json!({ "id": id, "para": "reservas@baia.ao" })).await;
    assert!(r["resposta"].as_str().unwrap().contains("queued"));
    let (cmds, dados) = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(cmds.iter().any(|c| c == "RCPT TO:<reservas@baia.ao>") && cmds.iter().any(|c| c == "RCPT TO:<arquivo@kianda.ao>"), "{cmds:?}");
    let html = html_da_mensagem(&dados);
    assert!(html.contains(&numero) && html.contains("Pão &amp; Bolos &lt;especial&gt;") && html.contains("1.140,00") && html.contains("Processado por programa"), "{html}");
    let auditoria: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE acao = 'EMAIL' AND detalhe LIKE ?").bind(format!("{numero}%")).fetch_one(&st.pool).await.unwrap();
    assert_eq!(auditoria, 1);

    // WhatsApp: telefone da ficha com o indicativo de Angola, texto com o documento e o total
    let m = rpc(&st, "documentos.mensagem", json!({ "id": id })).await;
    assert_eq!((m["numero"].as_str(), m["email"].as_str()), (Some("244923456789"), Some("reservas@baia.ao")));
    assert!(m["url"].as_str().unwrap().starts_with("https://wa.me/244923456789?text="));
    let texto = m["texto"].as_str().unwrap();
    assert!(texto.contains(&numero) && texto.contains("*Total: 1.140,00 Kz*"), "{texto}");
    assert!(rpc_err(&st, "documentos.mensagem", json!({ "id": id, "telefone": "123" })).await.contains("Telefone inválido"));
    assert_eq!(rpc(&st, "documentos.mensagem", json!({ "id": id, "telefone": "+351 912 345 678" })).await["numero"], "351912345678");
    // só abre ligações do WhatsApp
    assert!(rpc_err(&st, "partilha.abrir", json!({ "url": "https://exemplo.com/" })).await.contains("não permitido"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// A API tem um só servidor e um só estado na aplicação: os testes que a ligam correm um de cada vez.
static API_UMA_DE_CADA_VEZ: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Pedido HTTP simples à API: (código, cabeçalhos, corpo em JSON — ou em texto se não for JSON).
fn api_http(porta: u16, metodo: &str, caminho: &str, chave: Option<&str>, corpo: Option<&Value>, extra: &[(&str, &str)]) -> (u16, String, Value) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).expect("ligar à API");
    let b = corpo.map(|c| c.to_string()).unwrap_or_default();
    let mut req = format!("{metodo} {caminho} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(k) = chave {
        req.push_str(&format!("Authorization: Bearer {k}\r\n"));
    }
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if corpo.is_some() {
        req.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    req.push_str(&b);
    s.write_all(req.as_bytes()).unwrap();
    let mut r = Vec::new();
    s.read_to_end(&mut r).unwrap();
    let r = String::from_utf8_lossy(&r).to_string();
    let (cab, corpo) = r.split_once("\r\n\r\n").expect("resposta HTTP");
    let estado = cab.split_whitespace().nth(1).unwrap().parse().unwrap();
    let v = serde_json::from_str(corpo).unwrap_or_else(|_| Value::String(corpo.to_string()));
    (estado, cab.to_string(), v)
}

/// API de integração: chaves de acesso, permissões por chave limitadas às do utilizador, IPs permitidos,
/// limite de pedidos, emissão com idempotência, cliente por NIF, artigos por referência, SAF-T e registo.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_integracao() {
    let _so_uma_api = API_UMA_DE_CADA_VEZ.lock().await;
    let pasta = std::env::temp_dir().join(format!("kigest-api-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();

    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja API, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "referencia": "ARZ5", "codigo_barras": "5601234567890", "taxa_iva_id": 1, "preco1": 100000, "custo": 60000 })).await["id"].as_i64().unwrap();
    let oleo = rpc(&st, "artigos.guardar", json!({ "descricao": "Óleo 1L", "referencia": "OLE1", "taxa_iva_id": 1, "preco1": 80000, "custo": 50000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": arroz, "quantidade": 50, "preco_unitario": 60000 }, { "artigo_id": oleo, "quantidade": 10, "preco_unitario": 50000 }] })).await;
    let posto = rpc(&st, "posto.guardar", json!({ "descricao": "Loja online", "armazem_id": 1, "serie_prefixo": "W" })).await["id"].as_i64().unwrap();
    let loja = rpc(&st, "utilizadores.guardar", json!({ "nome": "Loja Online", "login": "loja", "senha": "segredo99", "perfil": "CAIXA", "mudar_senha": false })).await["id"].as_i64().unwrap();
    assert!(!crate::state::permissoes_perfil("GESTOR").contains(&"sistema.api".to_string()), "só o administrador gere a API por omissão");

    // ---- chaves: validação e criação (o segredo só aparece aqui) ----
    assert!(rpc_err(&st, "api.chaveCriar", json!({ "nome": "X", "utilizador_id": loja, "permissoes": [] })).await.contains("pelo menos uma"));
    assert!(rpc_err(&st, "api.chaveCriar", json!({ "nome": "X", "utilizador_id": loja, "permissoes": ["sistema.utilizadores"] })).await.contains("desconhecida"));
    assert!(rpc_err(&st, "api.chaveCriar", json!({ "nome": "X", "utilizador_id": loja, "permissoes": ["api.artigos"], "ips": "300.1.1.1" })).await.contains("inválido"));
    let perms = json!(["api.artigos", "api.stock", "api.entidades", "api.documentos", "vendas.fatura_recibo", "vendas.fatura", "clientes", "stock.entrada"]);
    let criada = rpc(&st, "api.chaveCriar", json!({ "nome": "Loja online", "utilizador_id": loja, "posto_id": posto, "permissoes": perms })).await;
    let k1 = criada["chave"].as_str().unwrap().to_string();
    let k1_id = criada["id"].as_i64().unwrap();
    assert!(k1.starts_with("kg_") && k1.len() == 51, "{k1}");
    let est = rpc(&st, "api.estado", Value::Null).await;
    assert!(!est.to_string().contains(&k1), "o segredo não volta a ser mostrado");
    let c1 = est["chaves"].as_array().unwrap().iter().find(|c| c["id"] == k1_id).unwrap().clone();
    assert_eq!((c1["estado"].as_str(), c1["prefixo"].as_str()), (Some("ACTIVA"), Some(&k1[..11])));
    assert_eq!(c1["sem_efeito"], json!(["stock.entrada"]), "a caixa não faz entradas de stock: a chave também não");
    assert!(est["permissoes"].as_array().unwrap().len() >= 20);

    // ---- servidor da API (só nesta máquina, numa porta livre) ----
    let porta = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arc = std::sync::Arc::new(st);
    crate::api::registar_estado(arc.clone());
    assert!(rpc_err(&arc, "api.guardar", json!({ "ativo": true, "porta": 80 })).await.contains("1024"));
    let e = rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false })).await;
    assert_eq!(e["a_servir"], porta);
    let http = |m: &str, c: &str, k: Option<&str>, b: Option<&Value>| api_http(porta, m, c, k, b, &[]);

    // públicos, sem chave
    assert_eq!(http("GET", "/api/v1/saude", None, None).2["ok"], true);
    let (s, _, spec) = http("GET", "/api/v1/openapi.json", None, None);
    assert_eq!(s, 200);
    assert!(spec["openapi"].as_str().unwrap().starts_with("3."), "{spec}");
    // sem chave, chave errada, rota desconhecida, método errado
    let (s, cab, v) = http("GET", "/api/v1", None, None);
    assert_eq!((s, v["erro"]["codigo"].as_str()), (401, Some("nao_autenticado")));
    assert!(cab.contains("WWW-Authenticate"));
    let falsa = format!("kg_{}", "x".repeat(48));
    assert_eq!(http("GET", "/api/v1", Some(&falsa), None).2["erro"]["codigo"], "chave_invalida");
    assert_eq!(http("GET", "/api/v1/naoexiste", Some(&k1), None).0, 404);
    assert_eq!(http("DELETE", "/api/v1/artigos", Some(&k1), None).0, 405);
    // cada resposta leva o número do pedido no registo; nos erros, também dentro de «erro»
    let (_, cab, erro) = http("GET", "/api/v1/naoexiste", Some(&k1), None);
    let numero: i64 = cab.lines().find_map(|l| l.strip_prefix("X-KanGest-Pedido: ")).expect("cabeçalho X-KanGest-Pedido").trim().parse().unwrap();
    assert_eq!(erro["erro"]["pedido"], numero, "{erro}");
    let registado: String = sqlx::query_scalar("SELECT caminho FROM api_pedidos WHERE id = ?").bind(numero).fetch_one(&arc.pool).await.unwrap();
    assert_eq!(registado, "/api/v1/naoexiste");
    // a chave por X-API-Key também serve
    let (s, _, info) = api_http(porta, "GET", "/api/v1", None, None, &[("X-API-Key", &k1)]);
    assert_eq!(s, 200, "{info}");
    assert_eq!(info["empresa"]["nif"], "5000000000");
    assert_eq!(info["posto"]["serie_prefixo"], "W");
    assert_eq!(info["chave"]["utilizador"], "Loja Online");
    assert!(!info["chave"]["permissoes"].as_array().unwrap().contains(&json!("stock.entrada")), "permissão sem efeito não conta");

    // ---- artigos: sem custos (falta api.custos), preço com IVA, paginação por id ----
    let (s, _, l) = http("GET", "/api/v1/artigos?limite=1", Some(&k1), None);
    assert_eq!(s, 200, "{l}");
    assert_eq!((l["mais"].as_bool(), l["seguinte"].as_i64()), (Some(true), Some(arroz)));
    let a = &l["dados"][0];
    assert_eq!((a["referencia"].as_str(), a["preco1_iva"].as_i64(), a["stock"].as_f64(), a["ativo"].as_bool()), (Some("ARZ5"), Some(114000), Some(50.0), Some(true)));
    assert!(a.get("custo").is_none() && a.get("custo_medio").is_none(), "sem api.custos não há custos: {a}");
    let l2 = http("GET", &format!("/api/v1/artigos?desde_id={arroz}"), Some(&k1), None).2;
    assert_eq!((l2["dados"][0]["id"].as_i64(), l2["mais"].as_bool()), (Some(oleo), Some(false)));
    assert_eq!(http("GET", "/api/v1/artigos?q=%C3%93leo", Some(&k1), None).2["dados"].as_array().unwrap().len(), 1);
    assert_eq!(http("GET", "/api/v1/artigos/codigo/5601234567890", Some(&k1), None).2["id"], arroz);
    assert_eq!(http("GET", "/api/v1/artigos/codigo/000", Some(&k1), None).0, 404);
    assert_eq!(http("GET", "/api/v1/artigos?alterado_desde=2000-01-01", Some(&k1), None).2["dados"].as_array().unwrap().len(), 2);
    assert_eq!(http("GET", "/api/v1/artigos?alterado_desde=2999-01-01", Some(&k1), None).2["dados"].as_array().unwrap().len(), 0);
    let t = http("GET", "/api/v1/tabelas", Some(&k1), None).2;
    assert!(t["taxas_iva"].as_array().unwrap().iter().any(|x| x["percentagem"] == 14.0));
    // stock com o detalhe por armazém
    let sk = http("GET", &format!("/api/v1/stock?artigo_id={arroz}"), Some(&k1), None).2;
    assert_eq!((sk["dados"][0]["quantidade"].as_f64(), sk["dados"][0]["armazens"][0]["armazem"].as_str()), (Some(50.0), Some("Principal")));
    assert!(sk["dados"][0].get("custo_medio").is_none());

    // ---- venda FR pela referência, com Idempotency-Key: repetir não emite outra ----
    let venda = json!({ "tipo": "FR", "referencia": "ENC-1001", "linhas": [{ "referencia": "ARZ5", "quantidade": 2 }], "pagamentos": [{ "modo": "TB", "valor": 228000 }] });
    let (s, cab, fr) = api_http(porta, "POST", "/api/v1/documentos", Some(&k1), Some(&venda), &[("Idempotency-Key", "enc-1001")]);
    assert_eq!(s, 201, "{fr}");
    assert!(cab.contains("Location: /api/v1/documentos/"));
    let fr_id = fr["id"].as_i64().unwrap();
    assert!(fr["numero_doc"].as_str().unwrap().contains('W'), "série do posto da chave: {}", fr["numero_doc"]);
    assert_eq!((fr["total"].as_i64(), fr["api_chave_id"].as_i64(), fr["referencia"].as_str()), (Some(228000), Some(k1_id), Some("ENC-1001")));
    assert_eq!(fr["hash_controlo"].as_str().unwrap().len(), 4);
    assert!(fr["linhas"][0].get("custo").is_none());
    let (s, cab, de_novo) = api_http(porta, "POST", "/api/v1/documentos", Some(&k1), Some(&venda), &[("Idempotency-Key", "enc-1001")]);
    assert_eq!((s, de_novo["id"].as_i64()), (200, Some(fr_id)));
    assert!(cab.contains("Idempotent-Replayed: true"));
    assert_eq!(http("GET", &format!("/api/v1/stock?artigo_id={arroz}"), Some(&k1), None).2["dados"][0]["quantidade"], 48.0, "só uma venda");
    // preço com IVA igual ao da ficha não precisa de permissão; preço diferente precisa
    let ok = json!({ "tipo": "FR", "linhas": [{ "artigo_id": oleo, "quantidade": 1, "preco_com_iva": 91200 }], "pagamentos": [{ "modo": "NU", "valor": 91200 }] });
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&ok)).0, 201);
    let barato = json!({ "tipo": "FR", "linhas": [{ "artigo_id": oleo, "quantidade": 1, "preco_unitario": 1000 }], "pagamentos": [{ "modo": "NU", "valor": 1140 }] });
    let (s, _, v) = http("POST", "/api/v1/documentos", Some(&k1), Some(&barato));
    assert_eq!((s, v["erro"]["codigo"].as_str()), (403, Some("sem_permissao")), "{v}");
    assert!(v["erro"]["mensagem"].as_str().unwrap().contains("pos.alterar_preco"));
    // erros de negócio: 422 com a mensagem do programa
    let curto = json!({ "tipo": "FR", "linhas": [{ "artigo_id": oleo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 100 }] });
    let (s, _, v) = http("POST", "/api/v1/documentos", Some(&k1), Some(&curto));
    assert_eq!(s, 422);
    assert!(v["erro"]["mensagem"].as_str().unwrap().contains("insuficiente"), "{v}");
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "FR", "linhas": [{ "referencia": "NAO", "quantidade": 1 }] }))).0, 422);
    // valores absurdos ficam à porta (nada de totais que transbordem num documento assinado)
    for linha in [json!({ "artigo_id": oleo, "quantidade": 1e12 }), json!({ "artigo_id": oleo, "quantidade": 1, "preco_unitario": -5 }), json!({ "artigo_id": oleo, "quantidade": 1, "desconto_pct": 150 }), json!({ "artigo_id": oleo })] {
        let (s, _, v) = http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "FR", "linhas": [linha], "pagamentos": [{ "modo": "NU", "valor": 1 }] })));
        assert_eq!((s, v["erro"]["codigo"].as_str()), (422, Some("validacao")), "{v}");
    }
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "FR", "linhas": [], "pagamentos": [{ "modo": "NU", "valor": 1e300 }] }))).0, 422);
    // tipos sem permissão ou fora da API
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "NC" }))).2["erro"]["codigo"], "sem_permissao");
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "ES", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] }))).0, 403, "o utilizador não tem stock.entrada");
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&json!({ "tipo": "VRC" }))).0, 422);
    assert_eq!(http("POST", "/api/v1/artigos", Some(&k1), Some(&json!({ "descricao": "X", "taxa_iva_id": 1, "preco1": 1 }))).0, 403);
    // JSON inválido
    {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
        s.write_all(format!("POST /api/v1/clientes HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {k1}\r\nContent-Length: 4\r\n\r\n{{abc").as_bytes()).unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        assert!(r.starts_with("HTTP/1.1 400") && r.contains("JSON inv"), "{r}");
    }

    // corpo «chunked» malformado (antes de qualquer chave): 400, e o servidor continua a responder
    {
        use std::io::{Read, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
        s.write_all(b"POST /api/v1/documentos HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n0\r\n\r\n").unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        assert!(r.starts_with("HTTP/1.1 400"), "{r}");
        assert_eq!(http("GET", "/api/v1/saude", None, None).0, 200);
    }

    // ---- FT a crédito com o cliente indicado pelo NIF (criado na primeira vez) ----
    let ft = json!({ "tipo": "FT", "cliente": { "nif": "5000111222", "nome": "Empresa X, Lda", "email": "compras@x.ao" }, "linhas": [{ "codigo_barras": "5601234567890", "quantidade": 1 }] });
    let (s, _, f1) = http("POST", "/api/v1/documentos", Some(&k1), Some(&ft));
    assert_eq!(s, 201, "{f1}");
    assert_eq!((f1["entidade_nome"].as_str(), f1["valor_pendente"].as_i64()), (Some("Empresa X, Lda"), Some(114000)));
    let f2 = http("POST", "/api/v1/documentos", Some(&k1), Some(&ft)).2;
    assert_eq!(f2["entidade_id"], f1["entidade_id"], "o mesmo NIF não cria outro cliente");
    let cli = http("GET", "/api/v1/clientes?nif=5000111222", Some(&k1), None).2;
    assert_eq!((cli["dados"].as_array().unwrap().len(), cli["dados"][0]["saldo"].as_i64()), (1, Some(228000)));
    let pend = http("GET", &format!("/api/v1/clientes/{}/pendentes", f1["entidade_id"]), Some(&k1), None).2;
    assert_eq!(pend["dados"].as_array().unwrap().len(), 2);

    // ---- clientes: criar, alterar só um campo, campos desconhecidos ----
    let (s, _, c) = http("POST", "/api/v1/clientes", Some(&k1), Some(&json!({ "nome": "Cliente API", "nif": "5000999888", "email": "a@b.ao" })));
    assert_eq!(s, 201, "{c}");
    let cid = c["id"].as_i64().unwrap();
    let (s, _, c) = http("PATCH", &format!("/api/v1/clientes/{cid}"), Some(&k1), Some(&json!({ "telefone": "923000000" })));
    assert_eq!((s, c["nome"].as_str(), c["email"].as_str(), c["telefone"].as_str()), (200, Some("Cliente API"), Some("a@b.ao"), Some("923000000")));
    assert_eq!(http("POST", "/api/v1/clientes", Some(&k1), Some(&json!({ "nome": "Y", "campo_errado": 1 }))).2["erro"]["codigo"], "campo_desconhecido");
    assert_eq!(http("GET", &format!("/api/v1/fornecedores/{cid}"), Some(&k1), None).0, 404, "um cliente não é fornecedor");
    assert_eq!(http("POST", "/api/v1/fornecedores", Some(&k1), Some(&json!({ "nome": "Forn" }))).0, 403);

    // ---- documentos: lista por id com linhas, filtros, só os desta chave ----
    let ds = http("GET", "/api/v1/documentos?linhas=1", Some(&k1), None).2;
    let lista = ds["dados"].as_array().unwrap();
    assert!(lista.iter().any(|d| d["tipo_codigo"] == "ES"));
    assert!(lista.iter().all(|d| d["linhas"].is_array() && d["pagamentos"].is_array()));
    assert_eq!(http("GET", "/api/v1/documentos?tipo=FT", Some(&k1), None).2["dados"].as_array().unwrap().len(), 2);
    assert_eq!(http("GET", "/api/v1/documentos?tipo=FT,FR&desta_chave=1", Some(&k1), None).2["dados"].as_array().unwrap().len(), 4);
    let d = http("GET", &format!("/api/v1/documentos/{fr_id}"), Some(&k1), None).2;
    assert_eq!((d["numero_doc"].as_str(), d["pagamentos"][0]["modo_codigo"].as_str()), (fr["numero_doc"].as_str(), Some("TB")));
    assert!(d.get("empresa").is_none());
    assert_eq!(http("POST", &format!("/api/v1/documentos/{fr_id}/anular"), Some(&k1), Some(&json!({ "motivo": "Encomenda cancelada" }))).0, 403);

    // ---- conversão de uma nota de encomenda: sem pos.alterar_preco, preços e descontos vêm da origem ----
    let ne = rpc(&arc, "documentos.emitir", json!({ "tipo": "NE", "linhas": [{ "artigo_id": oleo, "quantidade": 2, "preco_unitario": 70000, "desconto_pct": 5 }] })).await;
    let ne_id = ne["documento"]["id"].as_i64().unwrap();
    let conv = |linhas: Value| json!({ "tipo": "FR", "doc_origem_id": ne_id, "linhas": linhas, "pagamentos": [{ "modo": "NU", "valor": 151620 }] });
    let (s, _, v) = http("POST", "/api/v1/documentos", Some(&k1), Some(&conv(json!([{ "artigo_id": oleo, "quantidade": 2, "preco_unitario": 1 }]))));
    assert_eq!(s, 403, "a origem não serve para mudar o preço: {v}");
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&conv(json!([{ "artigo_id": oleo, "quantidade": 2, "desconto_pct": 50 }])))).0, 403);
    assert_eq!(http("POST", "/api/v1/documentos", Some(&k1), Some(&conv(json!([{ "artigo_id": arroz, "quantidade": 1 }])))).0, 422, "artigo fora da origem");
    let (s, _, f) = http("POST", "/api/v1/documentos", Some(&k1), Some(&conv(json!([{ "artigo_id": oleo, "quantidade": 2 }]))));
    assert_eq!((s, f["total"].as_i64(), f["linhas"][0]["desconto_pct"].as_f64()), (201, Some(151620), Some(5.0)), "{f}");
    assert_eq!(rpc(&arc, "documentos.obter", json!({ "id": ne_id })).await["documento"]["estado"], "F", "a encomenda fica facturada");

    // ---- chave de catálogo (administrador): custos, criar e alterar artigos, limite de pedidos ----
    let admin_id = rpc(&arc, "utilizadores.listar", Value::Null).await.as_array().unwrap().iter().find(|u| u["login"] == "admin").unwrap()["id"].as_i64().unwrap();
    let k2 = rpc(&arc, "api.chaveCriar", json!({ "nome": "Catálogo", "utilizador_id": admin_id, "permissoes": ["api.artigos", "api.custos", "artigos.criar", "artigos.editar"], "limite_minuto": 6 })).await["chave"].as_str().unwrap().to_string();
    let (s, _, a) = http("POST", "/api/v1/artigos", Some(&k2), Some(&json!({ "descricao": "Feijão 1kg", "referencia": "FEI1", "taxa_iva_id": 1, "preco1_iva": 57000, "custo": 30000 })));
    assert_eq!(s, 201, "{a}");
    assert_eq!((a["preco1"].as_i64(), a["preco1_iva"].as_i64(), a["custo"].as_i64()), (Some(50000), Some(57000), Some(30000)));
    let fid = a["id"].as_i64().unwrap();
    let (s, _, a) = http("PUT", &format!("/api/v1/artigos/{fid}"), Some(&k2), Some(&json!({ "preco1_iva": 68400 })));
    assert_eq!((s, a["preco1"].as_i64(), a["descricao"].as_str(), a["custo"].as_i64(), a["referencia"].as_str()), (200, Some(60000), Some("Feijão 1kg"), Some(30000), Some("FEI1")), "{a}");
    assert_eq!(http("PUT", &format!("/api/v1/artigos/{fid}"), Some(&k2), Some(&json!({ "preco1": 1, "preco1_iva": 2 }))).0, 422);
    // 5 pedidos feitos neste minuto; o 7.º passa o limite de 6
    let mut ultimo = 0;
    for _ in 0..3 {
        ultimo = http("GET", "/api/v1/artigos?limite=1", Some(&k2), None).0;
    }
    if ultimo != 429 {
        // (o minuto pode ter mudado a meio: insiste até ao limite)
        for _ in 0..8 {
            ultimo = http("GET", "/api/v1/artigos?limite=1", Some(&k2), None).0;
            if ultimo == 429 {
                break;
            }
        }
    }
    assert_eq!(ultimo, 429, "limite de pedidos por minuto");
    let (_, cab, _) = http("GET", "/api/v1", Some(&k2), None);
    assert!(cab.contains("Retry-After"));

    // ---- IPs permitidos (e o IP indicado por um túnel na própria máquina) ----
    let base = json!({ "id": k1_id, "nome": "Loja online", "utilizador_id": loja, "posto_id": posto, "permissoes": perms });
    let mut com_ips = base.clone();
    com_ips["ips"] = json!("10.0.0.0/8");
    rpc(&arc, "api.chaveGuardar", com_ips).await;
    let (s, _, v) = http("GET", "/api/v1", Some(&k1), None);
    assert_eq!((s, v["erro"]["codigo"].as_str()), (403, Some("ip_nao_autorizado")));
    assert_eq!(api_http(porta, "GET", "/api/v1", Some(&k1), None, &[("X-Forwarded-For", "10.1.2.3")]).0, 200);

    // ---- validade, desactivar, renovar e revogar ----
    let mut expirada = base.clone();
    expirada["expira_em"] = json!("2001-01-01");
    rpc(&arc, "api.chaveGuardar", expirada).await;
    assert_eq!(http("GET", "/api/v1", Some(&k1), None).2["erro"]["codigo"], "chave_expirada");
    let mut inactiva = base.clone();
    inactiva["ativa"] = json!(false);
    rpc(&arc, "api.chaveGuardar", inactiva).await;
    assert_eq!(http("GET", "/api/v1", Some(&k1), None).2["erro"]["codigo"], "chave_inactiva");
    rpc(&arc, "api.chaveGuardar", base.clone()).await;
    assert_eq!(http("GET", "/api/v1", Some(&k1), None).0, 200);
    let nova = rpc(&arc, "api.chaveRenovar", json!({ "id": k1_id })).await["chave"].as_str().unwrap().to_string();
    assert_eq!(http("GET", "/api/v1", Some(&k1), None).0, 401, "o segredo antigo deixa de servir");
    assert_eq!(http("GET", "/api/v1", Some(&nova), None).0, 200);
    // o utilizador desactivado leva as suas chaves
    rpc(&arc, "utilizadores.guardar", json!({ "id": loja, "nome": "Loja Online", "login": "loja", "perfil": "CAIXA", "ativo": false })).await;
    assert_eq!(http("GET", "/api/v1", Some(&nova), None).2["erro"]["codigo"], "utilizador_inactivo");
    rpc(&arc, "api.chaveRevogar", json!({ "id": k1_id })).await;
    assert_eq!(http("GET", "/api/v1", Some(&nova), None).2["erro"]["codigo"], "chave_revogada");
    assert!(rpc_err(&arc, "api.chaveGuardar", base.clone()).await.contains("revogada"));
    assert!(rpc_err(&arc, "api.chaveRevogar", json!({ "id": k1_id })).await.contains("revogada"));

    // ---- vendas da API não entram no caixa aberto do posto (nem as devoluções saem da gaveta) ----
    rpc(&arc, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let k4 = rpc(&arc, "api.chaveCriar", json!({ "nome": "Balcão virtual", "utilizador_id": admin_id, "permissoes": ["vendas.fatura_recibo", "vendas.nota_credito", "api.documentos"] })).await["chave"].as_str().unwrap().to_string();
    let (s, _, fr4) = http("POST", "/api/v1/documentos", Some(&k4), Some(&json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })));
    assert_eq!(s, 201, "{fr4}");
    assert!(fr4["caixa_sessao_id"].is_null(), "a venda da API não fica no caixa do posto 1");
    assert!(fr4["api_chave_id"].as_i64().is_some());
    let nc4 = http("POST", "/api/v1/documentos", Some(&k4), Some(&json!({ "tipo": "NC", "doc_origem_id": fr4["id"], "motivo": "Devolução da encomenda", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] }))).2;
    assert_eq!(nc4["credito_disponivel"], 114000, "sem gaveta, o valor fica como crédito do cliente: {nc4}");
    let cx = rpc(&arc, "caixa.estado", Value::Null).await;
    assert!(!cx.to_string().contains("Devolução NC"), "nenhuma saída de dinheiro no caixa: {cx}");
    let fr_balcao = rpc(&arc, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": arroz, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    assert!(fr_balcao["documento"]["caixa_sessao_id"].as_i64().is_some(), "no programa, a venda continua a entrar no caixa");
    assert!(fr_balcao["documento"]["api_chave_id"].is_null());

    // ---- SAF-T pela API ----
    let k3 = rpc(&arc, "api.chaveCriar", json!({ "nome": "Contabilidade", "utilizador_id": admin_id, "permissoes": ["sistema.saft", "api.documentos"] })).await["chave"].as_str().unwrap().to_string();
    let hoje = db::hoje();
    let (s, cab, xml) = http("GET", &format!("/api/v1/saft?de={}-01&ate={hoje}", &hoje[..7]), Some(&k3), None);
    assert_eq!(s, 200, "{xml}");
    assert!(cab.contains("application/xml") && cab.contains("SAFT_AO_5000000000"));
    let xml = xml.as_str().unwrap();
    assert!(xml.starts_with("<?xml") && xml.contains("<AuditFile") && xml.contains(fr["numero_doc"].as_str().unwrap()));
    assert!(std::fs::read_dir(&pasta).unwrap().all(|f| !f.unwrap().file_name().to_string_lossy().starts_with("SAFT")), "a API não grava ficheiros");
    assert_eq!(http("GET", "/api/v1/saft?de=2026-02-30&ate=2026-03-01", Some(&k3), None).0, 400);
    assert_eq!(http("GET", "/api/v1/artigos", Some(&k3), None).0, 403);

    // ---- registo de pedidos e auditoria ----
    let reg = rpc(&arc, "api.pedidos", json!({ "chave_id": k1_id })).await;
    let reg = reg.as_array().unwrap();
    assert!(reg.iter().any(|r| r["estado"] == 201 && r["metodo"] == "POST"));
    assert!(reg.iter().any(|r| r["estado"] == 403 && r["erro"].as_str().is_some_and(|e| e.contains("10.0.0.0") || e.contains("autorizado"))));
    assert!(rpc(&arc, "api.pedidos", json!({ "erros": true })).await.as_array().unwrap().iter().all(|r| r["estado"].as_i64().unwrap() >= 400));
    let est = rpc(&arc, "api.estado", Value::Null).await;
    let c1 = est["chaves"].as_array().unwrap().iter().find(|c| c["id"] == k1_id).unwrap().clone();
    assert_eq!((c1["estado"].as_str(), c1["documentos"].as_i64()), (Some("REVOGADA"), Some(5)));
    assert!(c1["pedidos"].as_i64().unwrap() > 10);
    let hoje_ = db::hoje();
    let aud = rpc(&arc, "auditoria.listar", json!({ "de": hoje_, "ate": hoje_, "evento": "API_CHAVE" })).await;
    assert!(aud.as_array().unwrap().len() >= 4, "{aud}");

    // ---- plano: a API faz parte do plano Empresa (e da experiência); nos outros, com o módulo «api» ----
    {
        use super::licenca::teste;
        let dia = |d: i64| (chrono::Local::now().date_naive() + chrono::Duration::days(d)).format("%Y-%m-%d").to_string();
        let lic = |plano: &str, modulos: Value| {
            teste::emitir(&json!({ "v": 1, "id": format!("LIC-{plano}"), "nif": "5000000000", "empresa": "Loja API, Lda", "plano": plano, "postos": 0,
                                   "modulos": modulos, "inicio": dia(-1), "fim": dia(365), "tolerancia": 15, "emitida": dia(0) }))
        };
        assert_eq!(rpc(&arc, "api.estado", Value::Null).await["licenca"]["experiencia"], true);
        rpc(&arc, "licenca.activar", json!({ "chave": lic("COMERCIO", json!([])) })).await;
        let (s, _, v) = http("GET", "/api/v1", Some(&k4), None);
        assert_eq!((s, v["erro"]["codigo"].as_str()), (403, Some("plano_sem_api")), "{v}");
        assert!(v["erro"]["mensagem"].as_str().unwrap().contains("Comércio"));
        let e = rpc(&arc, "api.estado", Value::Null).await;
        assert_eq!((e["licenca"]["incluida"].as_bool(), e["licenca"]["plano"].as_str()), (Some(false), Some("Comércio")));
        assert!(rpc_err(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false })).await.contains("plano Empresa"));
        rpc(&arc, "licenca.activar", json!({ "chave": lic("COMERCIO", json!(["api"])) })).await;
        assert_eq!(http("GET", "/api/v1", Some(&k4), None).0, 200, "o módulo «api» acrescenta a API ao plano Comércio");
        rpc(&arc, "licenca.activar", json!({ "chave": lic("EMPRESA", json!([])) })).await;
        assert_eq!(http("GET", "/api/v1", Some(&k4), None).0, 200);
        assert_eq!(rpc(&arc, "api.estado", Value::Null).await["licenca"]["incluida"], true);
    }

    // ---- Cloudflare Access: o que chega pela Cloudflare tem de trazer o JWT do Access ----
    {
        use crate::api::teste_cf::{chave, jwks, jwt};
        let equipa = "kangest-teste.cloudflareaccess.com";
        let aud = "a".repeat(64);
        crate::api::cf_chaves_teste(equipa, &jwks(chave(), "k1"));
        assert!(rpc_err(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false, "cf_equipa": equipa })).await.contains("AUD"));
        assert!(rpc_err(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false, "cf_equipa": "exemplo.com", "cf_aud": aud })).await.contains("cloudflareaccess.com"));
        let e = rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false, "cf_equipa": format!("https://{equipa}/"), "cf_aud": aud })).await;
        assert_eq!(e["cf_equipa"], equipa);
        assert_eq!(http("GET", "/api/v1", Some(&k4), None).0, 200, "da rede local basta a chave de API");
        let pela_cf = [("CF-Connecting-IP", "41.63.1.2")];
        let (s, _, v) = api_http(porta, "GET", "/api/v1", Some(&k4), None, &pela_cf);
        assert_eq!((s, v["erro"]["codigo"].as_str()), (403, Some("cloudflare_access")), "{v}");
        let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let bom = jwt(chave(), "k1", &json!({ "aud": [aud], "iss": format!("https://{equipa}"), "exp": agora + 300, "iat": agora }));
        assert_eq!(api_http(porta, "GET", "/api/v1", Some(&k4), None, &[pela_cf[0], ("Cf-Access-Jwt-Assertion", &bom)]).0, 200);
        let outro = jwt(chave(), "k1", &json!({ "aud": ["b".repeat(64)], "iss": format!("https://{equipa}"), "exp": agora + 300 }));
        assert_eq!(api_http(porta, "GET", "/api/v1", Some(&k4), None, &[pela_cf[0], ("Cf-Access-Jwt-Assertion", &outro)]).0, 403);
        // o JWT não dispensa a chave de API
        assert_eq!(api_http(porta, "GET", "/api/v1", None, None, &[pela_cf[0], ("Cf-Access-Jwt-Assertion", &bom)]).0, 401);
        rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false })).await;
    }

    // ---- desligar: a porta fecha ----
    let e = rpc(&arc, "api.guardar", json!({ "ativo": false, "porta": porta, "rede": false })).await;
    assert!(e["a_servir"].is_null());
    assert!(std::net::TcpStream::connect(("127.0.0.1", porta)).is_err(), "a API desligada não aceita ligações");

    arc.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Loja online pela API (o caso da Livraria Adventista): encomenda (NE) na loja do levantamento com o stock reservado,
/// procurada pelo n.º da encomenda, facturada ao pagar (FR a partir da NE, no armazém da NE) ou anulada; vendas por
/// canal, por loja e por mês, mais vendidos, síntese do ano e existências de uma loja.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_loja_online() {
    let _so_uma_api = API_UMA_DE_CADA_VEZ.lock().await;
    let pasta = std::env::temp_dir().join(format!("kigest-api-loja-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Livraria, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    let livro = rpc(&st, "artigos.guardar", json!({ "descricao": "O Desejado de Todas as Nações", "referencia": "LIV-001", "taxa_iva_id": 1, "preco1": 500000, "custo": 300000 })).await["id"].as_i64().unwrap();
    let lubango = rpc(&st, "tabela.guardar", json!({ "tabela": "armazens", "descricao": "Livraria do Lubango" })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": livro, "quantidade": 20, "preco_unitario": 300000 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "armazem_id": lubango, "linhas": [{ "artigo_id": livro, "quantidade": 5, "preco_unitario": 300000 }] })).await;
    // uma venda ao balcão (sem chave): conta no canal «balcao»
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": livro, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 570000 }] })).await;
    let posto = rpc(&st, "posto.guardar", json!({ "descricao": "Loja online", "armazem_id": 1, "serie_prefixo": "W" })).await["id"].as_i64().unwrap();
    let admin = rpc(&st, "utilizadores.listar", Value::Null).await.as_array().unwrap().iter().find(|u| u["login"] == "admin").unwrap()["id"].as_i64().unwrap();
    let perms = json!(["api.artigos", "api.stock", "api.documentos", "vendas.proforma", "vendas.fatura_recibo", "documentos.anular", "relatorios.ver"]);
    let k = rpc(&st, "api.chaveCriar", json!({ "nome": "Livraria online", "utilizador_id": admin, "posto_id": posto, "permissoes": perms })).await["chave"].as_str().unwrap().to_string();
    let so_stock = rpc(&st, "api.chaveCriar", json!({ "nome": "Só stock", "utilizador_id": admin, "permissoes": ["api.stock"] })).await["chave"].as_str().unwrap().to_string();

    let porta = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arc = std::sync::Arc::new(st);
    crate::api::registar_estado(arc.clone());
    rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false })).await;
    let http = |m: &str, c: &str, b: Option<&Value>| api_http(porta, m, c, Some(&k), b, &[]);
    let emitir = |b: &Value, idem: &str| api_http(porta, "POST", "/api/v1/documentos", Some(&k), Some(b), &[("Idempotency-Key", idem)]);
    let lubango_de = |artigo: i64| {
        let sk = http("GET", &format!("/api/v1/stock?artigo_id={artigo}"), None).2;
        sk["dados"][0]["armazens"].as_array().unwrap().iter().find(|a| a["armazem_id"] == lubango).cloned().unwrap()
    };

    // as lojas: armazéns e postos (num servidor central, cada loja é um armazém)
    let t = http("GET", "/api/v1/tabelas", None).2;
    assert!(t["armazens"].as_array().unwrap().iter().any(|a| a["id"] == lubango));
    assert!(t["postos"].as_array().unwrap().iter().any(|p| p["id"] == posto && p["serie_prefixo"] == "W"), "{t}");

    // ---- encomenda feita no site, a levantar no Lubango: NE no armazém dessa loja, com o stock reservado ----
    let ne_corpo = |enc: &str| json!({ "tipo": "NE", "referencia": enc, "armazem_id": lubango, "linhas": [{ "referencia": "LIV-001", "quantidade": 2 }] });
    let (s, _, ne) = emitir(&ne_corpo("ENC-2001"), "ne-ENC-2001");
    assert_eq!(s, 201, "{ne}");
    let ne_id = ne["id"].as_i64().unwrap();
    assert_eq!(ne["armazem_id"], lubango);
    let l = lubango_de(livro);
    assert_eq!((l["quantidade"].as_f64(), l["reservado"].as_f64(), l["disponivel"].as_f64()), (Some(5.0), Some(2.0), Some(3.0)), "{l}");
    // o site encontra a encomenda pelo n.º dela
    let achados = http("GET", "/api/v1/documentos?referencia=ENC-2001", None).2;
    assert_eq!(achados["dados"].as_array().unwrap().len(), 1, "{achados}");
    assert_eq!((achados["dados"][0]["tipo_codigo"].as_str(), achados["dados"][0]["id"].as_i64()), (Some("NE"), Some(ne_id)));
    assert_eq!(http("GET", &format!("/api/v1/documentos?armazem_id={lubango}&tipo=NE"), None).2["dados"].as_array().unwrap().len(), 1);

    // ---- pagamento confirmado: FR a partir da NE (sem preços), sai do stock do Lubango, a NE fica facturada ----
    let fr_corpo = json!({ "tipo": "FR", "doc_origem_id": ne_id, "referencia": "ENC-2001", "linhas": [{ "referencia": "LIV-001", "quantidade": 2 }], "pagamentos": [{ "modo": "TB", "valor": 1140000 }] });
    let (s, _, fr) = emitir(&fr_corpo, "fr-ENC-2001");
    assert_eq!((s, fr["total"].as_i64(), fr["armazem_id"].as_i64()), (201, Some(1140000), Some(lubango)), "{fr}");
    assert!(fr["numero_doc"].as_str().unwrap().starts_with("FR W"), "série do posto da loja online: {fr}");
    let (s, cab, _) = emitir(&fr_corpo, "fr-ENC-2001");
    assert!(s == 200 && cab.contains("Idempotent-Replayed: true"), "repetir o pedido não factura duas vezes");
    let l = lubango_de(livro);
    assert_eq!((l["quantidade"].as_f64(), l["reservado"].as_f64()), (Some(3.0), Some(0.0)), "{l}");
    assert_eq!(http("GET", &format!("/api/v1/documentos/{ne_id}"), None).2["estado"], "F");
    assert_eq!(http("GET", "/api/v1/documentos?referencia=ENC-2001", None).2["dados"].as_array().unwrap().len(), 2);

    // ---- encomenda cancelada: anula-se a NE e a reserva desaparece ----
    let (_, _, ne2) = emitir(&ne_corpo("ENC-2002"), "ne-ENC-2002");
    assert_eq!(lubango_de(livro)["reservado"].as_f64(), Some(2.0));
    let (s, _, an) = http("POST", &format!("/api/v1/documentos/{}/anular", ne2["id"]), Some(&json!({ "motivo": "Encomenda cancelada no site" })));
    assert_eq!((s, an["estado"].as_str()), (200, Some("A")), "{an}");
    assert_eq!(lubango_de(livro)["reservado"].as_f64(), Some(0.0));

    // ---- relatórios: canal, loja e mês ----
    let hoje = db::hoje();
    let rel = |q: &str| http("GET", &format!("/api/v1/relatorios/vendas?de={hoje}&ate={hoje}&{q}"), None);
    let total = |q: &str| rel(q).2["dados"].as_array().unwrap().iter().map(|l| l["total"].as_i64().unwrap()).sum::<i64>();
    assert_eq!(total("agrupar=mes&canal=api"), 1140000, "a loja online");
    assert_eq!(total("agrupar=mes&canal=balcao"), 570000, "o balcão");
    assert_eq!(total(&format!("agrupar=dia&armazem_id={lubango}")), 1140000);
    let (s, _, v) = rel("agrupar=mes&canal=site");
    assert_eq!((s, v["erro"]["codigo"].as_str()), (400, Some("pedido_invalido")));
    let por_loja = rel("agrupar=mes_armazem").2;
    assert_eq!(por_loja["dados"].as_array().unwrap().len(), 2, "{por_loja}");
    let lub = por_loja["dados"].as_array().unwrap().iter().find(|l| l["armazem_id"] == lubango).unwrap();
    assert_eq!((lub["mes"].as_str(), lub["armazem"].as_str(), lub["total"].as_i64()), (Some(&hoje[..7]), Some("Livraria do Lubango"), Some(1140000)));
    assert_eq!(rel("agrupar=mes_posto&canal=api").2["dados"][0]["posto_id"], posto);
    // mais vendidos: com a referência e o id do artigo (para ligar ao catálogo do site), sem lucro sem api.custos
    let top = rel("agrupar=artigo&limite=1").2;
    let t0 = &top["dados"][0];
    assert_eq!((t0["referencia"].as_str(), t0["artigo_id"].as_i64(), t0["quantidade"].as_f64()), (Some("LIV-001"), Some(livro), Some(3.0)), "{top}");
    assert!(t0.get("lucro").is_none());
    // síntese do ano (um registo por loja e por ano, com os 12 meses)
    let mes = hoje[5..7].parse::<usize>().unwrap();
    let si = http("GET", &format!("/api/v1/relatorios/sintese?ano={}&armazem_id={lubango}", &hoje[..4]), None).2;
    assert_eq!(si["meses"][mes - 1]["atual"], 1140000, "{si}");
    assert_eq!(http("GET", &format!("/api/v1/relatorios/sintese?ano={}&canal=balcao", &hoje[..4]), None).2["meses"][mes - 1]["atual"], 570000);
    // existências de uma loja (sem valores ao custo sem api.custos)
    let ex = http("GET", &format!("/api/v1/relatorios/stock?situacao=existencias&armazem_id={lubango}"), None).2;
    let linha = ex["dados"].as_array().unwrap().iter().find(|l| l["referencia"] == "LIV-001").unwrap();
    assert_eq!(linha["quantidade"].as_f64(), Some(3.0), "{ex}");
    assert!(linha.get("valor_custo").is_none() && linha.get("custo_medio").is_none(), "{linha}");
    assert_eq!(http("GET", "/api/v1/relatorios/stock?situacao=inventada", None).0, 400);
    assert_eq!(api_http(porta, "GET", "/api/v1/relatorios/stock", Some(&so_stock), None, &[]).0, 403, "sem relatorios.ver");

    rpc(&arc, "api.guardar", json!({ "ativo": false, "porta": porta, "rede": false })).await;
    arc.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Contrato com a loja online da Livraria Adventista (supabase/functions/app/kangest.ts, repositório do site): os
/// pedidos exactamente como o site os faz — NE com o preço com IVA do site, produtos digitais no artigo genérico
/// (cada um com o seu título e preço), portes, cliente pelo NIF; FR a partir da NE com as linhas sem preço; nota de
/// crédito da FR; anulação da NE — e o que o site lê de volta (stock paginado, relatórios, avisos).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_loja_online_contrato() {
    let _so_uma_api = API_UMA_DE_CADA_VEZ.lock().await;
    let pasta = std::env::temp_dir().join(format!("kigest-loja-online-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Livraria, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let livro = rpc(&st, "artigos.guardar", json!({ "descricao": "O Desejado de Todas as Nações", "referencia": "LIV001", "taxa_iva_id": 1, "preco1": 500000 })).await["id"].as_i64().unwrap();
    rpc(&st, "artigos.guardar", json!({ "descricao": "Edição digital", "referencia": "DIGITAL", "tipo": "S", "taxa_iva_id": 1, "preco1": 0 })).await;
    rpc(&st, "artigos.guardar", json!({ "descricao": "Portes", "referencia": "PORTES", "tipo": "S", "taxa_iva_id": 1, "preco1": 0 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": livro, "quantidade": 10, "preco_unitario": 300000 }] })).await;
    let posto = rpc(&st, "posto.guardar", json!({ "descricao": "Loja online", "armazem_id": 1, "serie_prefixo": "W" })).await["id"].as_i64().unwrap();
    // servidor central: a segunda livraria é outro armazém
    let lubango = rpc(&st, "tabela.guardar", json!({ "tabela": "armazens", "descricao": "Livraria do Lubango" })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "armazem_id": lubango, "linhas": [{ "artigo_id": livro, "quantidade": 4, "preco_unitario": 300000 }] })).await;
    let utilizador = rpc(&st, "utilizadores.guardar", json!({ "nome": "Loja Online", "login": "loja", "senha": "segredo99", "perfil": "GESTOR", "mudar_senha": false })).await["id"].as_i64().unwrap();
    // as permissões que docs/KANGEST.md do site pede para a chave (sem pos.desconto)
    let perms = json!(["api.artigos", "api.stock", "api.documentos", "vendas.proforma", "vendas.fatura_recibo", "vendas.nota_credito", "documentos.anular", "clientes", "relatorios.ver", "pos.alterar_preco"]);
    let criada = rpc(&st, "api.chaveCriar", json!({ "nome": "Loja online", "utilizador_id": utilizador, "posto_id": posto, "permissoes": perms })).await;
    let chave = criada["chave"].as_str().unwrap().to_string();
    let chave_id = criada["id"].as_i64().unwrap();
    let estado = rpc(&st, "api.estado", Value::Null).await;
    let sem_efeito = estado["chaves"].as_array().unwrap().iter().find(|c| c["id"] == chave_id).unwrap()["sem_efeito"].clone();
    assert_eq!(sem_efeito, json!([]), "um utilizador Gestor tem todas as permissões da chave da loja online");
    let (porta_avisos, recebidos) = receptor_webhooks();
    rpc(&st, "api.webhookGuardar", json!({ "nome": "Loja online", "url": format!("http://127.0.0.1:{porta_avisos}/webhooks/kangest?ligacao=central"),
        "eventos": ["stock.alterado", "artigo.alterado", "documento.emitido", "documento.anulado"] })).await;

    let porta = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arc = std::sync::Arc::new(st);
    crate::api::registar_estado(arc.clone());
    rpc(&arc, "api.guardar", json!({ "ativo": true, "porta": porta, "rede": false })).await;
    // como o site: Authorization, Accept, Content-Type e, nas emissões, Idempotency-Key
    let pedir = |metodo: &str, caminho: &str, corpo: Option<&Value>, idem: Option<&str>| {
        let mut extra = vec![("Accept", "application/json")];
        if let Some(i) = idem {
            extra.push(("Idempotency-Key", i));
        }
        let (s, _, v) = api_http(porta, metodo, &format!("/api/v1{caminho}"), Some(&chave), corpo, &extra);
        (s, v)
    };

    // ---- encomenda 1001 feita: NE (corpoNE) ----
    let ne_corpo = json!({
        "tipo": "NE", "referencia": "1001", "observacoes": "Loja online · encomenda 1001", "armazem_id": 1,
        "linhas": [
            { "referencia": "LIV001", "quantidade": 2, "preco_com_iva": 513000 },
            { "referencia": "DIGITAL", "quantidade": 1, "preco_com_iva": 150000, "descricao": "Caminho a Cristo (e-book)" },
            { "referencia": "DIGITAL", "quantidade": 1, "preco_com_iva": 90000, "descricao": "Primeiros Escritos (e-book)" },
            { "referencia": "PORTES", "quantidade": 1, "preco_com_iva": 250000, "descricao": "Portes de envio" }
        ],
        "cliente": { "nif": "5417189230", "nome": "Igreja do Huambo", "email": "tesouraria@igreja.ao", "telefone": "923456789" }
    });
    let (s, ne) = pedir("POST", "/documentos", Some(&ne_corpo), Some("ne-1001"));
    assert_eq!(s, 201, "{ne}");
    let ne_id = ne["id"].as_i64().unwrap();
    assert!(ne["numero_doc"].as_str().unwrap().starts_with("NE W"), "{ne}");
    assert_eq!(ne["armazem_id"], 1, "o site guarda o armazém da NE: {ne}");
    let total_ne = ne["total"].as_i64().unwrap();
    assert_eq!(total_ne, 2 * 513000 + 150000 + 90000 + 250000, "o total é o que o site cobrou: {ne}");
    let descricoes: Vec<&str> = ne["linhas"].as_array().unwrap().iter().filter_map(|l| l["descricao"].as_str()).collect();
    assert!(descricoes.contains(&"Caminho a Cristo (e-book)") && descricoes.contains(&"Primeiros Escritos (e-book)"), "o título do produto vai na linha do genérico: {descricoes:?}");
    // repetir (ligação caída): o mesmo documento
    let (s, de_novo) = pedir("POST", "/documentos", Some(&ne_corpo), Some("ne-1001"));
    assert_eq!((s, de_novo["id"].as_i64()), (200, Some(ne_id)));

    // ---- stock (sincronizarStock): paginado, por armazém, com o reservado e o disponível ----
    let (s, pag) = pedir("GET", "/stock?limite=500&desde_id=0", None, None);
    assert_eq!(s, 200, "{pag}");
    assert_eq!(pag["mais"], false);
    let l = pag["dados"].as_array().unwrap().iter().find(|d| d["referencia"] == "LIV001").expect("o livro no stock").clone();
    let a = &l["armazens"][0];
    assert_eq!((a["armazem_id"].as_i64(), a["quantidade"].as_f64(), a["reservado"].as_f64(), a["disponivel"].as_f64()), (Some(1), Some(10.0), Some(2.0), Some(8.0)), "{l}");

    // ---- pagamento confirmado: FR a partir da NE (corpoFR), linhas sem preço, pagamento = total da NE ----
    let fr_corpo = json!({
        "tipo": "FR", "doc_origem_id": ne_id, "referencia": "1001",
        "linhas": [
            { "referencia": "LIV001", "quantidade": 2 },
            { "referencia": "DIGITAL", "quantidade": 1, "descricao": "Caminho a Cristo (e-book)" },
            { "referencia": "DIGITAL", "quantidade": 1, "descricao": "Primeiros Escritos (e-book)" },
            { "referencia": "PORTES", "quantidade": 1, "descricao": "Portes de envio" }
        ],
        "pagamentos": [{ "modo": "MB", "valor": total_ne }]
    });
    let (s, fr) = pedir("POST", "/documentos", Some(&fr_corpo), Some("fr-1001"));
    assert_eq!(s, 201, "{fr}");
    assert_eq!(fr["total"].as_i64(), Some(total_ne), "a FR tem os preços da NE, linha a linha: {fr}");
    assert_eq!(fr["armazem_id"], 1);
    let fr_id = fr["id"].as_i64().unwrap();
    let (_, ne_agora) = pedir("GET", &format!("/documentos/{ne_id}"), None, None);
    assert_eq!(ne_agora["estado"], "F", "a NE fica facturada: {ne_agora}");
    let (_, pag) = pedir("GET", "/stock?limite=500&desde_id=0", None, None);
    let l = pag["dados"].as_array().unwrap().iter().find(|d| d["referencia"] == "LIV001").unwrap().clone();
    assert_eq!((l["armazens"][0]["quantidade"].as_f64(), l["armazens"][0]["reservado"].as_f64()), (Some(8.0), Some(0.0)), "{l}");

    // ---- reembolso: NC a partir da FR (corpoNC) ----
    let nc_corpo = json!({
        "tipo": "NC", "doc_origem_id": fr_id, "referencia": "1001", "motivo": "Reembolso da encomenda 1001 na loja online",
        "linhas": fr_corpo["linhas"]
    });
    let (s, nc) = pedir("POST", "/documentos", Some(&nc_corpo), Some("nc-1001"));
    assert_eq!(s, 201, "{nc}");
    assert_eq!(nc["total"].as_i64().map(i64::abs), Some(total_ne), "{nc}");

    // ---- uma chave com preço e desconto: a FR também leva os preços da NE (não os da ficha) ----
    let k2 = rpc(&arc, "api.chaveCriar", json!({ "nome": "Loja online 2", "utilizador_id": utilizador, "posto_id": posto,
        "permissoes": ["api.documentos", "vendas.proforma", "vendas.fatura_recibo", "pos.alterar_preco", "pos.desconto"] })).await["chave"].as_str().unwrap().to_string();
    let pedir2 = |corpo: &Value, idem: &str| api_http(porta, "POST", "/api/v1/documentos", Some(&k2), Some(corpo), &[("Idempotency-Key", idem)]);
    let (s, _, ne3) = pedir2(&json!({ "tipo": "NE", "referencia": "1003", "linhas": [{ "referencia": "LIV001", "quantidade": 1, "preco_com_iva": 456000 },
        { "referencia": "DIGITAL", "quantidade": 1, "preco_com_iva": 120000, "descricao": "Hinário (e-book)" }] }), "ne-1003");
    assert_eq!(s, 201, "{ne3}");
    let (s, _, fr3) = pedir2(&json!({ "tipo": "FR", "doc_origem_id": ne3["id"], "referencia": "1003",
        "linhas": [{ "referencia": "LIV001", "quantidade": 1 }, { "referencia": "DIGITAL", "quantidade": 1, "descricao": "Hinário (e-book)" }],
        "pagamentos": [{ "modo": "TB", "valor": ne3["total"] }] }), "fr-1003");
    assert_eq!((s, fr3["total"].as_i64()), (201, ne3["total"].as_i64()), "{fr3}");

    // ---- encomenda 1004 para levantar no Lubango (servidor central): a NE reserva lá e a FR sai de lá ----
    let (s, ne4) = pedir("POST", "/documentos", Some(&json!({ "tipo": "NE", "referencia": "1004", "armazem_id": lubango,
        "linhas": [{ "referencia": "LIV001", "quantidade": 3, "preco_com_iva": 570000 }] })), Some("ne-1004"));
    assert_eq!((s, ne4["armazem_id"].as_i64()), (201, Some(lubango)), "{ne4}");
    let (s, fr4) = pedir("POST", "/documentos", Some(&json!({ "tipo": "FR", "doc_origem_id": ne4["id"], "referencia": "1004",
        "linhas": [{ "referencia": "LIV001", "quantidade": 3 }], "pagamentos": [{ "modo": "MB", "valor": ne4["total"] }] })), Some("fr-1004"));
    assert_eq!((s, fr4["armazem_id"].as_i64()), (201, Some(lubango)), "a FR sai do armazém da NE: {fr4}");
    let (_, pag) = pedir("GET", "/stock?limite=500&desde_id=0", None, None);
    let l = pag["dados"].as_array().unwrap().iter().find(|d| d["referencia"] == "LIV001").unwrap().clone();
    let no_lubango = l["armazens"].as_array().unwrap().iter().find(|a| a["armazem_id"] == lubango).unwrap().clone();
    assert_eq!((no_lubango["quantidade"].as_f64(), no_lubango["reservado"].as_f64()), (Some(1.0), Some(0.0)), "{l}");

    // ---- encomenda 1002 cancelada com a NE no KanGest: anular ----
    let (s, ne2) = pedir("POST", "/documentos", Some(&json!({ "tipo": "NE", "referencia": "1002", "linhas": [{ "referencia": "LIV001", "quantidade": 1, "preco_com_iva": 570000 }] })), Some("ne-1002"));
    assert_eq!(s, 201, "{ne2}");
    let (s, anulada) = pedir("POST", &format!("/documentos/{}/anular", ne2["id"]), Some(&json!({ "motivo": "Encomenda 1002 cancelada no site" })), None);
    assert_eq!(s, 200, "{anulada}");
    let (_, pag) = pedir("GET", "/stock?limite=500&desde_id=0", None, None);
    let l = pag["dados"].as_array().unwrap().iter().find(|d| d["referencia"] == "LIV001").unwrap().clone();
    assert_eq!(l["armazens"][0]["reservado"].as_f64(), Some(0.0), "a reserva desaparece: {l}");
    // onde está a encomenda? (o painel do site)
    let (_, docs) = pedir("GET", "/documentos?referencia=1001", None, None);
    assert_eq!(docs["dados"].as_array().unwrap().len(), 3, "NE, FR e NC da encomenda 1001: {docs}");

    // ---- vendas (sincronizarVendas): por mês, por canal; e por armazém num servidor central ----
    let ano = chrono::Local::now().format("%Y").to_string();
    let mes = chrono::Local::now().format("%Y-%m").to_string();
    for agrupar in ["mes", "mes_armazem"] {
        let (s, v) = pedir("GET", &format!("/relatorios/vendas?de={ano}-01-01&ate={ano}-12-31&agrupar={agrupar}&canal=api"), None, None);
        assert_eq!(s, 200, "{v}");
        let d = v["dados"].as_array().unwrap();
        // o site lê o mês de `mes` ou de `chave` (linhasVendas)
        let mes_de = |x: &Value| x["mes"].as_str().or(x["chave"].as_str()).unwrap_or_default().to_string();
        assert!(!d.is_empty() && d.iter().all(|x| mes_de(x).starts_with(&mes) && x["total"].is_number() && x["docs"].is_number()), "{agrupar}: {v}");
        assert_eq!(d.iter().map(|x| x["docs"].as_i64().unwrap()).sum::<i64>(), 4, "as FR e a NC da loja online (as NE não são vendas): {v}");
        if agrupar == "mes_armazem" {
            let armazens: Vec<i64> = d.iter().filter_map(|x| x["armazem_id"].as_i64()).collect();
            assert!(armazens.contains(&1) && armazens.contains(&lubango), "uma linha por livraria: {v}");
        }
        let (s, b) = pedir("GET", &format!("/relatorios/vendas?de={ano}-01-01&ate={ano}-12-31&agrupar={agrupar}&canal=balcao"), None, None);
        assert_eq!(s, 200, "{b}");
    }

    // ---- avisos (tratarAviso): o que o site lê de cada um ----
    rpc(&arc, "artigos.guardar", json!({ "id": livro, "descricao": "O Desejado de Todas as Nações", "referencia": "LIV001", "taxa_iva_id": 1, "preco1": 520000 })).await;
    let balcao = rpc(&arc, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": livro, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 592800 }] })).await;
    let balcao_id = balcao["documento"]["id"].as_i64().unwrap();
    while crate::webhooks::ciclo(&arc).await.unwrap() > 0 {}
    let corpos: Vec<Value> = recebidos.lock().unwrap().iter().filter(|x| x.0.starts_with("/webhooks/kangest?ligacao=central")).map(|x| serde_json::from_str(&x.2).unwrap()).collect();
    let ultimo = |evento: &str, f: &dyn Fn(&Value) -> bool| corpos.iter().rev().find(|c| c["evento"] == evento && f(&c["dados"])).cloned();
    let stock = ultimo("stock.alterado", &|d| d["referencia"] == "LIV001").expect("aviso de stock");
    let a = &stock["dados"]["armazens"][0];
    assert!(a["armazem_id"].is_number() && a["quantidade"].is_number() && a["reservado"].is_number() && a["disponivel"].is_number(), "{stock}");
    let artigo = ultimo("artigo.alterado", &|d| d["referencia"] == "LIV001").expect("aviso do artigo");
    assert_eq!(artigo["dados"]["preco1_iva"], 592800, "{artigo}");
    let venda = ultimo("documento.emitido", &|d| d["id"] == balcao_id).expect("aviso da venda ao balcão");
    let d = &venda["dados"];
    assert_eq!((d["tipo_codigo"].as_str(), d["api_chave_id"].is_null(), d["armazem_id"].as_i64(), d["estado"].as_str()), (Some("FR"), true, Some(1), Some("N")), "{venda}");
    assert!(d["data_hora"].as_str().is_some_and(|h| h.len() >= 16) && d["numero_doc"].is_string() && d["total"].is_number(), "{venda}");
    let linha = &d["linhas"][0];
    assert!(linha["referencia"] == "LIV001" && linha["descricao"].is_string() && linha["quantidade"].is_number() && linha["total"].is_number(), "{venda}");
    let da_loja = ultimo("documento.emitido", &|d| d["id"] == fr_id).expect("aviso da FR da loja online");
    assert_eq!(da_loja["dados"]["api_chave_id"], chave_id, "o site ignora as vendas da própria chave: {da_loja}");
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Receptor HTTP de avisos para os testes: guarda (caminho, cabeçalhos, corpo); responde 500 em /falha e 200 no resto.
/// Avisos recebidos pelo receptor de teste: (caminho do pedido, cabeçalhos, corpo).
type AvisosRecebidos = std::sync::Arc<std::sync::Mutex<Vec<(String, String, String)>>>;

fn receptor_webhooks() -> (u16, AvisosRecebidos) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = l.local_addr().unwrap().port();
    let recebidos = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let r = recebidos.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut dados = Vec::new();
            let mut buf = [0u8; 8192];
            let corpo = loop {
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break None;
                }
                dados.extend_from_slice(&buf[..n]);
                if let Some(fim) = dados.windows(4).position(|w| w == b"\r\n\r\n") {
                    let cab = String::from_utf8_lossy(&dados[..fim]).to_string();
                    let tamanho: usize = cab.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                    if dados.len() >= fim + 4 + tamanho {
                        break Some((cab, String::from_utf8_lossy(&dados[fim + 4..fim + 4 + tamanho]).to_string()));
                    }
                }
            };
            let Some((cab, corpo)) = corpo else { continue };
            let caminho = cab.split_whitespace().nth(1).unwrap_or("").to_string();
            let falha = caminho.starts_with("/falha");
            r.lock().unwrap().push((caminho, cab, corpo));
            let resp = if falha { "HTTP/1.1 500 Erro\r\nContent-Length: 5\r\nConnection: close\r\n\r\nerro!" } else { "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok" };
            let _ = s.write_all(resp.as_bytes());
        }
    });
    (porta, recebidos)
}

/// Avisos automáticos da API (webhooks): eventos por gatilhos, entregas assinadas (HMAC-SHA256), stock agregado
/// por artigo, novas tentativas, reenvio, teste «ping» e anulação.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn webhooks_api() {
    let pasta = std::env::temp_dir().join(format!("kigest-webhooks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Avisos, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let conta = |tipo: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM api_eventos WHERE tipo = ?").bind(tipo).fetch_one(&pool).await.unwrap() }
    };

    // sem webhooks, os gatilhos não registam nada
    let antes = rpc(&st, "artigos.guardar", json!({ "descricao": "Antes", "taxa_iva_id": 1, "preco1": 1000 })).await;
    assert!(antes["id"].as_i64().is_some());
    assert_eq!(conta("artigo.alterado").await, 0);

    // ---- criar: validação e segredo ----
    let (porta, recebidos) = receptor_webhooks();
    assert!(rpc_err(&st, "api.webhookGuardar", json!({ "nome": "X", "url": "ftp://x", "eventos": ["documento.emitido"] })).await.contains("endereço"));
    assert!(rpc_err(&st, "api.webhookGuardar", json!({ "nome": "X", "url": "https://x.ao/h", "eventos": [] })).await.contains("evento"));
    assert!(rpc_err(&st, "api.webhookGuardar", json!({ "nome": "X", "url": "https://x.ao/h", "eventos": ["nada"] })).await.contains("desconhecido"));
    let w = rpc(&st, "api.webhookGuardar", json!({ "nome": "Loja online", "url": format!("http://127.0.0.1:{porta}/avisos"),
        "eventos": ["documento.emitido", "documento.anulado", "stock.alterado", "artigo.alterado"] })).await;
    let segredo = w["segredo"].as_str().unwrap().to_string();
    assert!(segredo.starts_with("whsec_"));
    let falha = rpc(&st, "api.webhookGuardar", json!({ "nome": "Servidor em baixo", "url": format!("http://127.0.0.1:{porta}/falha"), "eventos": ["documento.emitido"] })).await["id"].as_i64().unwrap();

    // ---- eventos: artigo, entrada de stock (2 movimentos do mesmo artigo → um aviso de stock), venda ----
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Café 250g", "referencia": "CAF250", "taxa_iva_id": 1, "preco1": 150000, "custo": 90000 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": art, "quantidade": 5, "preco_unitario": 90000 }, { "artigo_id": art, "quantidade": 3, "preco_unitario": 90000 }] })).await;
    assert_eq!(conta("stock.alterado").await, 1, "vários movimentos, um aviso de stock enquanto não foi distribuído");
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 171000 }] })).await;
    let fr_id = fr["documento"]["id"].as_i64().unwrap();
    let enviados = crate::webhooks::ciclo(&st).await.unwrap();
    assert!(enviados >= 4, "artigo, ES, stock e FR: {enviados}");

    // ---- o que chegou: assinatura HMAC-SHA256 confere, cabeçalhos e dados ----
    let lista = recebidos.lock().unwrap().clone();
    let avisos: Vec<&(String, String, String)> = lista.iter().filter(|x| x.0 == "/avisos").collect();
    for (_, cab, corpo) in &avisos {
        let linha = cab.lines().find(|l| l.to_ascii_lowercase().starts_with("x-kangest-assinatura:")).expect("assinatura");
        let valor = linha.split_once(':').unwrap().1.trim();
        let t: u64 = valor.split(',').next().unwrap().trim_start_matches("t=").parse().unwrap();
        assert_eq!(valor, crate::webhooks::assinatura(&segredo, t, corpo), "a assinatura confere com o segredo");
    }
    let corpos: Vec<Value> = avisos.iter().map(|x| serde_json::from_str(&x.2).unwrap()).collect();
    let fr_aviso = corpos.iter().find(|c| c["evento"] == "documento.emitido" && c["dados"]["tipo_codigo"] == "FR").expect("aviso da FR");
    assert_eq!((fr_aviso["dados"]["numero_doc"].as_str(), fr_aviso["dados"]["total"].as_i64()), (fr["documento"]["numero_doc"].as_str(), Some(171000)));
    assert_eq!(fr_aviso["dados"]["linhas"][0]["referencia"], "CAF250");
    assert_eq!((fr_aviso["dados"]["armazem_id"].as_i64(), fr_aviso["dados"]["posto_id"].as_i64()), (Some(1), Some(1)), "a loja (armazém) e o posto da venda");
    assert!(fr_aviso["dados"]["linhas"][0].get("custo").is_none(), "os avisos não levam custos");
    let stock = corpos.iter().find(|c| c["evento"] == "stock.alterado").expect("aviso de stock");
    assert_eq!(stock["dados"]["quantidade"].as_f64(), Some(7.0), "o aviso leva o stock no momento do envio (8 − 1)");
    let artigo = corpos.iter().find(|c| c["evento"] == "artigo.alterado" && c["dados"]["id"] == art).expect("aviso do artigo");
    assert_eq!((artigo["dados"]["preco1_iva"].as_i64(), artigo["dados"]["ativo"].as_bool()), (Some(171000), Some(true)));
    assert!(artigo["dados"].get("custo").is_none());

    // ---- falhas: nova tentativa mais tarde, e o reenvio põe-na já na fila ----
    let estado = rpc(&st, "api.webhooks", Value::Null).await;
    let wf = estado["webhooks"].as_array().unwrap().iter().find(|x| x["id"] == falha).unwrap().clone();
    assert!(wf["falhas_seguidas"].as_i64().unwrap() >= 1 && wf["ultimo_estado"] == 500, "{wf}");
    let entrega = estado["entregas"].as_array().unwrap().iter().find(|e| e["webhook"] == "Servidor em baixo").unwrap().clone();
    assert_eq!((entrega["estado"].as_str(), entrega["tentativas"].as_i64()), (Some("PENDENTE"), Some(1)));
    assert!(entrega["proxima_em"].as_str().is_some() && entrega["erro"].as_str().unwrap().contains("500"));
    let n = recebidos.lock().unwrap().len();
    crate::webhooks::ciclo(&st).await.unwrap();
    assert_eq!(recebidos.lock().unwrap().len(), n, "antes da hora da nova tentativa não volta a enviar");
    rpc(&st, "api.entregaReenviar", json!({ "id": entrega["id"] })).await;
    crate::webhooks::ciclo(&st).await.unwrap();
    assert!(recebidos.lock().unwrap().len() > n, "o reenvio sai no ciclo seguinte");

    // ---- anulação e teste «ping» ----
    rpc(&st, "documentos.anular", json!({ "id": fr_id, "motivo": "venda de teste" })).await;
    crate::webhooks::ciclo(&st).await.unwrap();
    assert!(recebidos.lock().unwrap().iter().any(|x| x.1.contains("documento.anulado")));
    let t = rpc(&st, "api.webhookTestar", json!({ "id": w["id"] })).await;
    assert_eq!((t["aceite"].as_bool(), t["codigo_http"].as_i64()), (Some(true), Some(200)), "{t}");
    let t = rpc(&st, "api.webhookTestar", json!({ "id": falha })).await;
    assert_eq!((t["aceite"].as_bool(), t["codigo_http"].as_i64()), (Some(false), Some(500)));

    // ---- reservas: uma NE muda o disponível sem mexer na existência, e o aviso de stock diz quanto ----
    let ultimo_stock = || -> Value {
        recebidos.lock().unwrap().iter().rev().filter_map(|x| serde_json::from_str::<Value>(&x.2).ok()).find(|c| c["evento"] == "stock.alterado").expect("aviso de stock")
    };
    let ne = rpc(&st, "documentos.emitir", json!({ "tipo": "NE", "linhas": [{ "artigo_id": art, "quantidade": 2 }] })).await;
    crate::webhooks::ciclo(&st).await.unwrap();
    let s1 = ultimo_stock();
    assert_eq!((s1["dados"]["quantidade"].as_f64(), s1["dados"]["reservado"].as_f64(), s1["dados"]["disponivel"].as_f64()), (Some(8.0), Some(2.0), Some(6.0)), "{s1}");
    assert_eq!((s1["dados"]["armazens"][0]["reservado"].as_f64(), s1["dados"]["armazens"][0]["disponivel"].as_f64()), (Some(2.0), Some(6.0)));
    rpc(&st, "documentos.anular", json!({ "id": ne["documento"]["id"], "motivo": "encomenda cancelada" })).await;
    crate::webhooks::ciclo(&st).await.unwrap();
    let s2 = ultimo_stock();
    assert_eq!((s2["dados"]["reservado"].as_f64(), s2["dados"]["disponivel"].as_f64()), (Some(0.0), Some(8.0)), "a encomenda anulada liberta a reserva: {s2}");

    // ---- segredo novo; desactivar; apagar ----
    let novo = rpc(&st, "api.webhookSegredo", json!({ "id": w["id"] })).await["segredo"].as_str().unwrap().to_string();
    assert_ne!(novo, segredo);
    rpc(&st, "api.webhookGuardar", json!({ "id": falha, "nome": "Servidor em baixo", "url": format!("http://127.0.0.1:{porta}/falha"), "eventos": ["documento.emitido"], "ativo": false })).await;
    rpc(&st, "api.webhookApagar", json!({ "id": w["id"] })).await;
    let antes = conta("artigo.alterado").await;
    rpc(&st, "artigos.guardar", json!({ "descricao": "Depois", "taxa_iva_id": 1, "preco1": 1000 })).await;
    assert_eq!(conta("artigo.alterado").await, antes, "sem webhooks activos, os gatilhos param");
    let aud = rpc(&st, "auditoria.listar", json!({ "de": db::hoje(), "ate": db::hoje(), "evento": "API_WEBHOOK" })).await;
    assert!(aud.as_array().unwrap().len() >= 4);

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Comissões de vendedores: taxa do vendedor, taxa própria da família, NC descontam (no vendedor da origem),
/// anulados não contam, e o total do mês entra no processamento de salários (rubrica 019).
#[tokio::test]
async fn comissoes_de_vendedores() {
    use chrono::Datelike;
    let (st, pasta) = base_com_admin("comissoes").await;
    let hoje = chrono::Local::now().date_naive();
    let (ano, mes) = (hoje.year(), hoje.month());
    let inicio = format!("{ano:04}-{mes:02}-01");
    let servicos = rpc(&st, "tabela.guardar", json!({ "tabela": "familias", "descricao": "Serviços" })).await["id"].as_i64().unwrap();
    let telemoveis = rpc(&st, "tabela.guardar", json!({ "tabela": "familias", "descricao": "Telemóveis" })).await["id"].as_i64().unwrap();
    let artigo = |descricao: &'static str, preco: i64, familia: Option<i64>| {
        let st = &st;
        async move { rpc(st, "artigos.guardar", json!({ "descricao": descricao, "tipo": "S", "taxa_iva_id": 1, "preco1": preco, "familia_id": familia })).await["id"].as_i64().unwrap() }
    };
    let servico = artigo("Configuração", 1_000_000, Some(servicos)).await;
    let telemovel = artigo("Telemóvel X", 10_000_000, Some(telemoveis)).await;
    let capa = artigo("Capa", 200_000, None).await;
    let func = |nome: &'static str| {
        let st = &st;
        async move { rpc(st, "rh.funcionarioGuardar", json!({ "nome": nome, "salario_base": 20000000, "desconta_inss": true, "desconta_irt": true })).await["id"].as_i64().unwrap() }
    };
    let (ana, beto, carla) = (func("Ana").await, func("Beto").await, func("Carla").await);
    let cliente = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Comissões", "nif": "5417000000" })).await["id"].as_i64().unwrap();

    // ---- taxas ----
    assert!(rpc_err(&st, "comissoes.taxaGuardar", json!({ "tipo": "VENDEDOR", "id": ana, "pct": 150 })).await.contains("entre 0 e 100"));
    assert!(rpc_err(&st, "comissoes.taxaGuardar", json!({ "tipo": "OUTRO", "id": ana, "pct": 1 })).await.contains("inválido"));
    rpc(&st, "comissoes.taxaGuardar", json!({ "tipo": "VENDEDOR", "id": ana, "pct": 2 })).await;
    rpc(&st, "comissoes.taxaGuardar", json!({ "tipo": "VENDEDOR", "id": beto, "pct": 3 })).await;
    rpc(&st, "comissoes.taxaGuardar", json!({ "tipo": "FAMILIA", "id": servicos, "pct": 10 })).await;
    let t = rpc(&st, "comissoes.taxas", Value::Null).await;
    assert_eq!(t["vendedores"].as_array().unwrap().iter().find(|v| v["id"] == ana).unwrap()["pct"], 2.0);
    assert_eq!(t["vendedores"].as_array().unwrap().iter().find(|v| v["id"] == carla).unwrap()["pct"], 0.0);
    assert_eq!(t["familias"].as_array().unwrap().iter().find(|f| f["id"] == servicos).unwrap()["pct"], 10.0);

    // ---- vendas do mês ----
    let venda = |vendedor: i64, linhas: Value| {
        let st = &st;
        async move {
            rpc(st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cliente, "vendedor_id": vendedor, "linhas": linhas })).await["documento"].clone()
        }
    };
    let liquido = |id: i64, artigo: i64| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, i64>("SELECT SUM(valor_liquido) FROM documento_linhas WHERE documento_id = ? AND artigo_id = ?").bind(id).bind(artigo).fetch_one(&pool).await.unwrap() }
    };
    let ft1 = venda(ana, json!([{ "artigo_id": servico, "quantidade": 1 }, { "artigo_id": telemovel, "quantidade": 1 }])).await;
    let ft2 = venda(beto, json!([{ "artigo_id": capa, "quantidade": 2 }])).await;
    venda(carla, json!([{ "artigo_id": servico, "quantidade": 1 }])).await;
    let anulada = venda(ana, json!([{ "artigo_id": telemovel, "quantidade": 3 }])).await;
    rpc(&st, "documentos.anular", json!({ "id": anulada["id"], "motivo": "erro de digitação no documento" })).await;
    // devolução do telemóvel, sem vendedor na NC: desconta à Ana (vendedora da factura)
    rpc(&st, "documentos.emitir", json!({ "tipo": "NC", "entidade_id": cliente, "doc_origem_id": ft1["id"], "motivo": "Devolução", "linhas": [{ "artigo_id": telemovel, "quantidade": 1 }] })).await;

    let (ft1_id, ft2_id) = (ft1["id"].as_i64().unwrap(), ft2["id"].as_i64().unwrap());
    let servico_liq = liquido(ft1_id, servico).await;
    let capa_liq = liquido(ft2_id, capa).await;
    let r = rpc(&st, "comissoes.relatorio", json!({ "de": inicio, "ate": hoje.format("%Y-%m-%d").to_string() })).await;
    let de = |id: i64| r["vendedores"].as_array().unwrap().iter().find(|v| v["vendedor_id"] == id).cloned().unwrap_or(Value::Null);
    // Ana: serviços a 10 % (taxa da família); telemóvel vendido e devolvido (base 0); a factura anulada não conta
    assert_eq!(de(ana)["base"], servico_liq, "{r}");
    assert_eq!(de(ana)["comissao"], (servico_liq as f64 * 0.10).round() as i64, "{r}");
    let tel = de(ana)["familias"].as_array().unwrap().iter().find(|f| f["familia"] == "Telemóveis").cloned().unwrap();
    assert_eq!((tel["base"].as_i64(), tel["comissao"].as_i64()), (Some(0), Some(0)));
    // Beto: artigo sem família, à sua taxa
    assert_eq!(de(beto)["comissao"], (capa_liq as f64 * 0.03).round() as i64);
    // Carla: vende, mas sem comissão (a taxa da família só vale para quem tem comissão)
    assert_eq!(de(carla)["comissao"], 0);

    // ---- lançar no salário do mês (duas vezes: substitui, não duplica) ----
    let l = rpc(&st, "comissoes.lancar", json!({ "ano": ano, "mes": mes })).await;
    assert_eq!(l["lancados"], 2, "{l}");
    rpc(&st, "comissoes.lancar", json!({ "ano": ano, "mes": mes })).await;
    let alts = rpc(&st, "rh.alteracoes", json!({ "ano": ano, "mes": mes })).await;
    let comissoes: Vec<&Value> = alts.as_array().unwrap().iter().filter(|a| a["rubrica"] == "019").collect();
    assert_eq!(comissoes.len(), 2, "{alts}");
    let sim = rpc(&st, "rh.simular", json!({ "ano": ano, "mes": mes })).await;
    let recibo_ana = sim.as_array().unwrap().iter().find(|x| x["funcionario_id"] == ana).unwrap();
    let linha = recibo_ana["linhas"].as_array().unwrap().iter().find(|x| x["rubrica"] == "019").cloned().expect("linha de comissões no recibo");
    assert_eq!(linha["valor"], de(ana)["comissao"]);
    assert_eq!(recibo_ana["total_remuneracoes"], 20000000 + de(ana)["comissao"].as_i64().unwrap(), "a comissão é remuneração");
    // mês processado: as comissões já não mudam
    rpc(&st, "rh.processar", json!({ "ano": ano, "mes": mes })).await;
    assert!(rpc_err(&st, "comissoes.lancar", json!({ "ano": ano, "mes": mes })).await.contains("já foram processados"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Tempos das consultas mais usadas numa base com volume (para afinar os índices). Corre-se à mão sobre uma cópia:
///   KANGEST_BASE=/caminho/kigest.db cargo test --release --lib volume_tempos -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn volume_tempos() {
    use chrono::Datelike;
    let Ok(base) = std::env::var("KANGEST_BASE") else { return };
    let caminho = std::path::PathBuf::from(&base);
    let pool = db::abrir(&caminho).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = caminho.parent().unwrap().to_path_buf();
    st.pasta_exportacao = st.pasta_dados.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let hoje = chrono::Local::now().date_naive();
    let (mes, ano) = (format!("{}-{:02}-01", hoje.year(), hoje.month()), format!("{}-01-01", hoje.year()));
    let h = hoje.format("%Y-%m-%d").to_string();
    let cliente: i64 = sqlx::query_scalar("SELECT entidade_id FROM documentos WHERE entidade_id <> 1 GROUP BY entidade_id ORDER BY COUNT(*) DESC LIMIT 1").fetch_one(&st.pool).await.unwrap();
    let artigo: i64 = sqlx::query_scalar("SELECT artigo_id FROM movimentos_stock GROUP BY artigo_id ORDER BY COUNT(*) DESC LIMIT 1").fetch_one(&st.pool).await.unwrap();
    let casos: Vec<(&str, &str, Value)> = vec![
        ("Painel", "dashboard", Value::Null),
        ("Documentos do mês", "documentos.listar", json!({ "de": mes, "ate": h })),
        ("Documentos: pesquisa", "documentos.listar", json!({ "q": "Cliente Volume 12" })),
        ("Documentos de um cliente", "documentos.listar", json!({ "entidade_id": cliente })),
        ("Vendas por dia (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "dia" })),
        ("Vendas por artigo (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "artigo" })),
        ("Vendas por família (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "familia" })),
        ("Vendas por operador (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "operador" })),
        ("Vendas por vendedor (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "vendedor" })),
        ("Vendas por pagamento (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "pagamento" })),
        ("Vendas por cliente (ano)", "relatorios.vendas", json!({ "de": ano, "ate": h, "agrupar": "cliente" })),
        ("Síntese do ano", "relatorios.sintese", json!({ "ano": hoje.year() })),
        ("Tesouraria: contas e saldos", "tesouraria.contas", Value::Null),
        ("Tesouraria: fluxo do mês", "tesouraria.fluxo", json!({ "de": mes, "ate": h })),
        ("Tesouraria: resumo mensal", "tesouraria.resumoMensal", json!({ "ano": hoje.year().to_string() })),
        ("Contas correntes: pendentes", "contas.pendentes", json!({ "tipo": "C" })),
        ("Conta corrente de um cliente", "contas.extrato", json!({ "entidade_id": cliente })),
        ("Vencidos", "contas.vencidos", json!({ "tipo": "C" })),
        ("Clientes", "entidades.listar", json!({ "tipo": "C" })),
        ("Clientes: pesquisa", "entidades.listar", json!({ "q": "Volume 21" })),
        ("Stock", "stock.listar", json!({})),
        ("Movimentos de um artigo", "stock.movimentos", json!({ "artigo_id": artigo })),
        ("Stock parado", "relatorios.stock", json!({ "situacao": "parado", "dias": 90 })),
        ("Caixa: estado", "caixa.estado", Value::Null),
    ];
    let mut total = 0.0;
    for (nome, metodo, params) in casos {
        let mut melhor = f64::MAX;
        let mut tamanho = 0;
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let r = dispatch(&st, metodo, params.clone()).await;
            melhor = melhor.min(t.elapsed().as_secs_f64() * 1000.0);
            tamanho = r.map(|v| v.to_string().len()).unwrap_or_else(|e| { eprintln!("  {metodo}: {e}"); 0 });
        }
        total += melhor;
        eprintln!("{nome:<32} {melhor:>9.1} ms  {:>9} bytes", tamanho);
    }
    eprintln!("{:<32} {total:>9.1} ms", "TOTAL");
}

/// Ramos de actividade: dados de ramo nos artigos (pesquisáveis), ISBN na livraria e variantes na moda.
#[tokio::test]
async fn ramos_de_actividade() {
    let (st, pasta) = base_com_admin("ramos").await;
    assert_eq!(rpc(&st, "ramo.obter", Value::Null).await["ramo"], "comercio", "sem escolha, é comércio geral");
    assert!(rpc_err(&st, "ramo.definir", json!({ "ramo": "nave-espacial" })).await.contains("desconhecido"));

    // ---- livraria: ISBN-10 passa a ISBN-13, o autor e a editora encontram o livro ----
    rpc(&st, "ramo.definir", json!({ "ramo": "livraria" })).await;
    assert_eq!(rpc(&st, "tabelas", Value::Null).await["ramo"], "livraria");
    let livro = rpc(
        &st,
        "artigos.guardar",
        json!({ "descricao": "Mayombe", "taxa_iva_id": 1, "preco1": 450000, "codigo_barras": "972-20-1290-8",
                "atributos": { "autor": "Pepetela", "editora": "Dom Quixote", "ano": 1980, "vazio": "" } }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let a = rpc(&st, "artigos.obter", json!({ "id": livro })).await;
    assert_eq!(a["codigo_barras"], "9789722012904");
    let at: Value = serde_json::from_str(a["atributos"].as_str().unwrap()).unwrap();
    assert_eq!(at, json!({ "autor": "Pepetela", "editora": "Dom Quixote", "ano": 1980 }));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Livro errado", "taxa_iva_id": 1, "preco1": 1, "codigo_barras": "978-989-52-7385-9" })).await.contains("ISBN"));
    assert!(
        rpc_err(&st, "artigos.guardar", json!({ "descricao": "Mayombe (outro)", "taxa_iva_id": 1, "preco1": 1, "codigo_barras": "9789722012904" })).await.contains("já é do artigo"),
        "o mesmo ISBN não pode ficar em dois artigos"
    );
    let achados = rpc(&st, "artigos.listar", json!({ "q": "pepetela" })).await;
    assert_eq!(achados.as_array().unwrap().len(), 1, "o autor encontra o livro");
    assert!(rpc(&st, "artigos.listar", json!({ "q": "autor" })).await.as_array().unwrap().is_empty(), "o nome do campo não conta como texto");
    assert_eq!(rpc(&st, "artigos.codigo", json!({ "codigo": "9789722012904" })).await["id"], livro, "o leitor lê o ISBN");
    // gravar sem "atributos" mantém os que existem
    rpc(&st, "artigos.guardar", json!({ "id": livro, "descricao": "Mayombe (bolso)", "taxa_iva_id": 1, "preco1": 450000, "codigo_barras": "9789722012904" })).await;
    assert!(rpc(&st, "artigos.obter", json!({ "id": livro })).await["atributos"].as_str().unwrap().contains("Pepetela"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "X", "taxa_iva_id": 1, "preco1": 1, "atributos": { "Autor": "x" } })).await.contains("inválido"));

    // ---- moda: uma camisa em 3 tamanhos × 2 cores = 6 artigos, cada um com o seu stock ----
    rpc(&st, "ramo.definir", json!({ "ramo": "moda" })).await;
    let camisa = rpc(&st, "artigos.guardar", json!({ "referencia": "CAM01", "descricao": "Camisa Polo", "taxa_iva_id": 1, "preco1": 1200000, "atributos": { "marca": "Kianda" } }))
        .await["id"]
        .as_i64()
        .unwrap();
    let eixos = json!([{ "nome": "tamanho", "valores": ["S", "M", "L"] }, { "nome": "cor", "valores": ["Azul marinho", " Branco ", ""] }]);
    let r = rpc(&st, "artigos.variantes", json!({ "artigo_id": camisa, "eixos": eixos })).await;
    let criados = r["criados"].as_array().unwrap();
    assert_eq!(criados.len(), 6);
    assert_eq!(criados[0]["referencia"], "CAM01-S-AZULMA");
    assert_eq!(criados[0]["descricao"], "Camisa Polo S / Azul marinho");
    let v = rpc(&st, "artigos.obter", json!({ "id": criados[5]["id"] })).await;
    assert_eq!(v["preco1"], 1200000, "a variante herda o preço");
    let at: Value = serde_json::from_str(v["atributos"].as_str().unwrap()).unwrap();
    assert_eq!(at, json!({ "marca": "Kianda", "modelo": "CAM01", "tamanho": "L", "cor": "Branco" }));
    assert_eq!(rpc(&st, "artigos.listar", json!({ "q": "Azul marinho" })).await.as_array().unwrap().len(), 3);
    let outra = rpc(&st, "artigos.variantes", json!({ "artigo_id": camisa, "eixos": eixos })).await;
    assert_eq!((outra["criados"].as_array().unwrap().len(), outra["existentes"].as_i64()), (0, Some(6)), "não se repetem");
    assert!(rpc_err(&st, "artigos.variantes", json!({ "artigo_id": camisa, "eixos": [] })).await.contains("Indique"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Avenças: contratos facturados periodicamente; os períodos em atraso saem um a um e nunca em duplicado.
#[tokio::test]
async fn avencas_facturacao_periodica() {
    use chrono::{Datelike, Months};
    let (st, pasta) = base_com_admin("avencas").await;
    let hoje = chrono::Local::now().date_naive();
    let inicio = hoje.with_day(1).unwrap().checked_sub_months(Months::new(2)).unwrap();
    let servico = rpc(&st, "artigos.guardar", json!({ "descricao": "Contabilidade organizada", "tipo": "S", "taxa_iva_id": 1, "preco1": 15_000_000 })).await["id"].as_i64().unwrap();
    let cliente = rpc(&st, "entidades.guardar", json!({ "nome": "Padaria Kianda", "nif": "5417111111" })).await["id"].as_i64().unwrap();

    assert!(rpc_err(&st, "avencas.guardar", json!({ "entidade_id": 1, "descricao": "X", "periodicidade": "MENSAL", "proxima": inicio.to_string(), "linhas": [{ "artigo_id": servico, "quantidade": 1 }] }))
        .await
        .contains("cliente identificado"));
    assert!(rpc_err(&st, "avencas.guardar", json!({ "entidade_id": cliente, "descricao": "X", "periodicidade": "SEMANAL", "proxima": inicio.to_string(), "linhas": [{ "artigo_id": servico, "quantidade": 1 }] }))
        .await
        .contains("Periodicidade"));
    assert!(rpc_err(&st, "avencas.guardar", json!({ "entidade_id": cliente, "descricao": "X", "periodicidade": "MENSAL", "proxima": inicio.to_string(), "linhas": [] })).await.contains("serviço"));

    let id = rpc(
        &st,
        "avencas.guardar",
        json!({ "entidade_id": cliente, "descricao": "Contabilidade mensal", "periodicidade": "MENSAL", "proxima": inicio.to_string(),
                "linhas": [{ "artigo_id": servico, "quantidade": 1, "descricao": "Contabilidade — {periodo}", "desconto": 10 }] }),
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let lista = rpc(&st, "avencas.listar", Value::Null).await;
    let a = &lista.as_array().unwrap()[0];
    assert_eq!(a["por_facturar"], true);
    assert_eq!(a["valor"], 15_390_000, "15 000 000 − 10% + IVA 14%");
    assert_eq!(rpc(&st, "dashboard", Value::Null).await["alertas"]["avencas_por_facturar"], 1);

    // três meses em atraso (o de há dois meses, o passado e o actual): três facturas, uma por mês
    let r = rpc(&st, "avencas.emitir", json!({})).await;
    let emitidos = r["emitidos"].as_array().unwrap();
    assert_eq!(emitidos.len(), 3, "{r}");
    assert!(r["erros"].as_array().unwrap().is_empty());
    let doc = rpc(&st, "documentos.obter", json!({ "id": emitidos[0]["documento_id"] })).await;
    assert_eq!(doc["documento"]["tipo_codigo"], "FT");
    assert_eq!(doc["documento"]["entidade_id"], cliente);
    let rotulo = emitidos[0]["periodo"].as_str().unwrap().to_string();
    assert!(doc["linhas"][0]["descricao"].as_str().unwrap().ends_with(&rotulo), "o período vai na linha: {}", doc["linhas"][0]["descricao"]);
    assert!(doc["documento"]["observacoes"].as_str().unwrap().contains("Contabilidade mensal"));
    let proxima = rpc(&st, "avencas.listar", Value::Null).await[0]["proxima"].as_str().unwrap().to_string();
    assert_eq!(proxima, inicio.checked_add_months(Months::new(3)).unwrap().to_string());

    // de novo: nada por facturar (nem em duplicado)
    let r2 = rpc(&st, "avencas.emitir", json!({ "ids": [id] })).await;
    assert!(r2["emitidos"].as_array().unwrap().is_empty());
    assert_eq!(rpc(&st, "dashboard", Value::Null).await["alertas"]["avencas_por_facturar"], 0);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM avenca_emissoes WHERE avenca_id = ? AND documento_id IS NOT NULL").bind(id).fetch_one(&st.pool).await.unwrap();
    assert_eq!(n, 3);

    // com facturas emitidas, «apagar» só desactiva (fica o histórico)
    assert_eq!(rpc(&st, "avencas.apagar", json!({ "id": id })).await["desactivada"], true);
    assert_eq!(rpc(&st, "avencas.listar", Value::Null).await[0]["ativo"], 0);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[tokio::test]
async fn tamanho_do_despachante_temp() {
    let (st, pasta) = base_com_admin("tamanho").await;
    let f = super::dispatch(&st, "x", Value::Null);
    eprintln!("TAMANHO dispatch = {}", std::mem::size_of_val(&f));
    let g = super::avencas::emitir(&st, serde_json::from_value(json!({})).unwrap());
    eprintln!("TAMANHO avencas::emitir = {}", std::mem::size_of_val(&g));
    let h = super::documentos::emitir(&st, serde_json::from_value(json!({"tipo":"FT"})).unwrap());
    eprintln!("TAMANHO documentos::emitir = {}", std::mem::size_of_val(&h));
    drop((f, g, h));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Servidor de licenças falso: responde às rotas do registo e do acesso web e guarda os corpos recebidos.
/// Pedidos recebidos pelo servidor de licenças falso: (caminho, corpo).
type PedidosRecebidos = std::sync::Arc<std::sync::Mutex<Vec<(String, Value)>>>;

fn licencas_falso() -> (u16, PedidosRecebidos) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = l.local_addr().unwrap().port();
    let recebidos = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let r = recebidos.clone();
    std::thread::spawn(move || {
        for f in l.incoming().flatten() {
            let mut f = f;
            let mut dados = Vec::new();
            let mut b = [0u8; 8192];
            loop {
                let n = f.read(&mut b).unwrap_or(0);
                dados.extend_from_slice(&b[..n]);
                let texto = String::from_utf8_lossy(&dados).to_string();
                if let Some((cab, corpo)) = texto.split_once("\r\n\r\n") {
                    let tam: usize = cab.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                    if corpo.len() >= tam || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let texto = String::from_utf8_lossy(&dados).to_string();
            let caminho = texto.split_whitespace().nth(1).unwrap_or("").to_string();
            let corpo: Value = serde_json::from_str(texto.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("")).unwrap_or(Value::Null);
            r.lock().unwrap().push((caminho.clone(), corpo));
            let resposta = match caminho.as_str() {
                "/v1/confirmar" => json!({ "ok": true, "segredo": "segredo-do-computador-1234", "estado": "EXPERIENCIA", "licenca": null }),
                "/v1/web/estado" => json!({ "ok": true, "disponivel": true, "dominio": "kangest.ao", "nome": "kianda", "endereco": "https://kianda.kangest.ao", "neste_computador": true }),
                "/v1/web/activar" => json!({ "ok": true, "nome": "kianda", "endereco": "https://kianda.kangest.ao", "token": "token-do-tunel-1" }),
                _ => json!({ "ok": true, "licenca": null }),
            }
            .to_string();
            let _ = f.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resposta}", resposta.len()).as_bytes());
        }
    });
    (porta, recebidos)
}

/// Acesso pela Internet: o segredo do computador vem do registo e autentica a renovação e o pedido do subdomínio;
/// o token do túnel fica cifrado e o cloudflared é lançado com ele.
#[tokio::test]
async fn acesso_pela_internet_por_subdominio() {
    let (porta_lic, recebidos) = licencas_falso();
    std::env::set_var("KANGEST_LICENCAS_URL", format!("http://127.0.0.1:{porta_lic}"));
    let (st, pasta) = base_com_admin("acesso-web").await;
    let arc = std::sync::Arc::new(st);
    // um cloudflared falso no lugar do verdadeiro (assim o teste não descarrega nada)
    let exe = pasta.join("cloudflared").join(format!("cloudflared-{}{}", super::acesso_web::VERSAO_CLOUDFLARED, std::env::consts::EXE_SUFFIX));
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "#!/bin/sh\necho \"$(date) INF Registered tunnel connection token=${TUNNEL_TOKEN}\" >&2\nexec sleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let corpo = |caminho: &str| recebidos.lock().unwrap().iter().rev().find(|(c, _)| c == caminho).map(|(_, v)| v.clone());

    // fora do modo servidor: explica o que fazer (a configuração da rede destes testes é a de omissão, sozinho)
    if crate::rede::config().modo != "servidor" {
        assert!(rpc_err(&arc, "web.activar", json!({ "nome": "kianda" })).await.contains("servidor"));
    }
    // antes do registo não há segredo: explica o que fazer
    let porta = 7811;
    *super::acesso_web::SERVIDOR_TESTE.lock().unwrap() = Some(porta);
    assert!(rpc_err(&arc, "web.activar", json!({ "nome": "kianda" })).await.contains("Registe este KanGest"));

    // registo: o segredo fica cifrado na base e vai nas renovações
    rpc(&arc, "licenca.confirmar", json!({ "codigo": "123456" })).await;
    let guardado: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'licenca.segredo'").fetch_one(&arc.pool).await.unwrap();
    assert!(guardado.starts_with("kgs1:") && !guardado.contains("segredo-do-computador"), "{guardado}");
    rpc(&arc, "licenca.sincronizar", json!({ "forcar": true })).await;
    assert_eq!(corpo("/v1/renovar").unwrap()["segredo"], "segredo-do-computador-1234");

    // activar: pede o endereço com o segredo e a porta do servidor; o token fica cifrado e o túnel liga-se
    let e = rpc(&arc, "web.activar", json!({ "nome": " Kianda " })).await;
    let pedido = corpo("/v1/web/activar").unwrap();
    assert_eq!((pedido["nome"].as_str(), pedido["porta"].as_u64(), pedido["segredo"].as_str()), (Some("kianda"), Some(porta as u64), Some("segredo-do-computador-1234")));
    assert_eq!(e["endereco"], "https://kianda.kangest.ao");
    assert_eq!(e["activo"], true);
    let token: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'web.token'").fetch_one(&arc.pool).await.unwrap();
    assert!(token.starts_with("kgs1:") && !token.contains("token-do-tunel"), "o token do túnel fica cifrado");
    #[cfg(unix)]
    {
        let mut ligado = false;
        for _ in 0..50 {
            let t = rpc(&arc, "web.estado", Value::Null).await["tunel"].clone();
            if t["ligado"] == true {
                ligado = true;
                assert!(t["linhas"][0].as_str().unwrap().contains("token=token-do-tunel-1"), "o token vai pelo ambiente: {t}");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(ligado, "o cloudflared ligou o túnel");
    }
    let v = rpc(&arc, "web.estado", json!({ "verificar": true })).await;
    assert_eq!((v["remoto"]["disponivel"].as_bool(), v["remoto"]["dominio"].as_str()), (Some(true), Some("kangest.ao")));

    // desligar: pára o cloudflared, apaga o endereço no servidor e aqui
    let v = rpc(&arc, "web.desactivar", Value::Null).await;
    assert!(v.get("aviso").is_none(), "{v}");
    assert_eq!((v["activo"].as_bool(), v["endereco"].is_null(), v["tunel"]["a_correr"].as_bool()), (Some(false), true, Some(false)));
    assert!(corpo("/v1/web/desactivar").is_some());
    let auditoria: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE acao = 'ACESSO_WEB'").fetch_one(&arc.pool).await.unwrap();
    assert_eq!(auditoria, 2);

    // domínio da empresa: o token do túnel dela (aqui dentro do comando de instalação do painel da Cloudflare)
    use base64::Engine;
    let token = base64::engine::general_purpose::STANDARD.encode(r#"{"a":"conta-da-loja","t":"tunel-da-loja","s":"c2VncmVkbw=="}"#);
    assert!(rpc_err(&arc, "web.activarProprio", json!({ "endereco": "gestao", "token": token })).await.contains("endereço completo"));
    assert!(rpc_err(&arc, "web.activarProprio", json!({ "endereco": "gestao.loja.ao", "token": "abc" })).await.contains("token do túnel"));
    let e = rpc(&arc, "web.activarProprio", json!({ "endereco": "https://Gestao.Loja.ao/", "token": format!("sudo cloudflared service install {token}") })).await;
    assert_eq!((e["endereco"].as_str(), e["proprio"].as_bool(), e["activo"].as_bool()), (Some("https://gestao.loja.ao"), Some(true), Some(true)));
    let cifrado: String = sqlx::query_scalar("SELECT valor FROM config WHERE chave = 'web.token'").fetch_one(&arc.pool).await.unwrap();
    assert!(cifrado.starts_with("kgs1:") && !cifrado.contains(&token));
    #[cfg(unix)]
    {
        let mut ligado = false;
        for _ in 0..50 {
            let t = rpc(&arc, "web.estado", Value::Null).await["tunel"].clone();
            if t["ligado"] == true && t["linhas"][0].as_str().unwrap().contains(&token) {
                ligado = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(ligado, "o cloudflared corre com o token da empresa");
    }
    // enquanto usa o domínio da empresa, não pede um endereço do KanGest; desligar não fala com o servidor de licenças
    assert!(rpc_err(&arc, "web.activar", json!({ "nome": "kianda" })).await.contains("próprio domínio"));
    let pedidos_antes = recebidos.lock().unwrap().len();
    let v = rpc(&arc, "web.desactivar", Value::Null).await;
    assert_eq!((v["activo"].as_bool(), v["proprio"].as_bool(), v["endereco"].is_null()), (Some(false), Some(false), true));
    assert_eq!(recebidos.lock().unwrap().len(), pedidos_antes, "o túnel é da empresa: nada a apagar no servidor");
    std::env::remove_var("KANGEST_LICENCAS_URL");
    *super::acesso_web::SERVIDOR_TESTE.lock().unwrap() = None;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Verificação em dois passos: ligar com a senha e um código, entrar com código (uma vez cada), códigos de recuperação,
/// e pela Internet (túnel) a política que a exige e a recusa da entrada só com PIN.
#[tokio::test]
async fn verificacao_em_dois_passos() {
    use super::dois_passos::{codigo, de_base32};
    use crate::state::{Contexto, CONTEXTO};
    let (st, pasta) = base_com_admin("dois-passos").await;
    let passo = || chrono::Utc::now().timestamp().div_euclid(30);

    let n = rpc(&st, "auth.doisPassosNovo", Value::Null).await;
    let segredo = de_base32(n["segredo"].as_str().unwrap()).unwrap();
    assert_eq!(segredo.len(), 20);
    assert!(n["uri"].as_str().unwrap().starts_with("otpauth://totp/KanGest:admin?secret="));
    let ligar = |c: String, senha: &str| json!({ "segredo": n["segredo"], "codigo": c, "senha": senha });
    assert!(rpc_err(&st, "auth.doisPassosLigar", ligar(codigo(&segredo, passo()), "errada")).await.contains("senha"));
    let fora: String = (0..1_000_000).map(|x| format!("{x:06}")).find(|c| (-2..=2).all(|d| codigo(&segredo, passo() + d) != *c)).unwrap();
    assert!(rpc_err(&st, "auth.doisPassosLigar", ligar(fora.clone(), "1234")).await.contains("não confere"));
    let p0 = passo();
    let l = rpc(&st, "auth.doisPassosLigar", ligar(codigo(&segredo, p0), "1234")).await;
    let recuperacao: Vec<String> = serde_json::from_value(l["codigos"].clone()).unwrap();
    assert_eq!(recuperacao.len(), 8);
    let guardado: String = sqlx::query_scalar("SELECT totp_segredo FROM utilizadores WHERE login = 'admin'").fetch_one(&st.pool).await.unwrap();
    assert!(guardado.starts_with("kgs1:"), "o segredo fica cifrado");
    assert_eq!(rpc(&st, "auth.doisPassosEstado", Value::Null).await["activo"], true);

    // entrar: depois da senha certa, o código; sem sessão até lá
    rpc(&st, "auth.logout", Value::Null).await;
    let r = rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    assert_eq!(r, json!({ "dois_passos": true }));
    assert!(rpc(&st, "auth.sessao", Value::Null).await.is_null());
    rpc_err(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": codigo(&segredo, p0) })).await;
    let tentativas: i64 = sqlx::query_scalar("SELECT tentativas FROM utilizadores WHERE login = 'admin'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(tentativas, 1, "o código já usado não serve outra vez e conta como falha");
    assert!(rpc_err(&st, "auth.login", json!({ "login": "admin", "senha": "errada", "codigo": codigo(&segredo, p0 + 1) })).await.contains("incorrect"));
    let s = rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": codigo(&segredo, p0 + 1) })).await;
    assert_eq!(s["login"], "admin");
    // código de recuperação: uma vez
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": recuperacao[0].to_uppercase() })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    rpc_err(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": recuperacao[0] })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": recuperacao[1] })).await;
    assert_eq!(rpc(&st, "auth.doisPassosEstado", Value::Null).await["codigos_restantes"], 6);

    // pela Internet: com a política, quem não tem os dois passos não entra; o PIN não serve
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "senha": "ana12345", "perfil": "CAIXA", "pin": "4321", "mudar_senha": false })).await;
    let pol = rpc(&st, "seguranca.obter", Value::Null).await;
    let mut nova = pol.clone();
    nova["dois_passos_internet"] = json!(true);
    nova["login_pin"] = json!(true);
    rpc(&st, "seguranca.guardar", nova).await;
    let pela_internet = |metodo: &'static str, params: Value| {
        let ctx = Contexto { sessao: std::sync::Arc::new(std::sync::Mutex::new(None)), posto: st.posto(), api_chave: None, pela_internet: true };
        CONTEXTO.scope(ctx, super::dispatch(&st, metodo, params))
    };
    let e = pela_internet("auth.login", json!({ "login": "ana", "senha": "ana12345" })).await.unwrap_err().to_string();
    assert!(e.contains("dois passos"), "{e}");
    let e = pela_internet("auth.loginPin", json!({ "pin": "4321" })).await.unwrap_err().to_string();
    assert!(e.contains("PIN não se usa pela Internet"), "{e}");
    assert_eq!(pela_internet("auth.login", json!({ "login": "admin", "senha": "1234" })).await.unwrap(), json!({ "dois_passos": true }));
    // na loja, a Ana entra como sempre
    rpc(&st, "auth.logout", Value::Null).await;
    assert_eq!(rpc(&st, "auth.login", json!({ "login": "ana", "senha": "ana12345" })).await["login"], "ana");

    // o administrador repõe; o próprio desliga com senha e código
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234", "codigo": recuperacao[2] })).await;
    assert!(rpc_err(&st, "auth.doisPassosDesligar", json!({ "senha": "1234", "codigo": fora })).await.contains("não confere"));
    rpc(&st, "auth.doisPassosDesligar", json!({ "senha": "1234", "codigo": recuperacao[3] })).await;
    assert_eq!(rpc(&st, "auth.doisPassosEstado", Value::Null).await["activo"], false);
    let ana: i64 = sqlx::query_scalar("SELECT id FROM utilizadores WHERE login = 'ana'").fetch_one(&st.pool).await.unwrap();
    rpc(&st, "utilizadores.doisPassosRepor", json!({ "id": ana })).await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE acao = 'DOIS_PASSOS'").fetch_one(&st.pool).await.unwrap();
    assert!(n >= 4, "{n}");
    let _ = std::fs::remove_dir_all(&pasta);
}

/// «Esqueci-me da senha» no ecrã de entrada: só com a verificação em dois passos (código da aplicação ou de
/// recuperação), a mesma mensagem para tudo o que falha, códigos errados contam para o bloqueio, a autorização vale uma
/// vez e só na loja (pela Internet não).
#[tokio::test]
async fn recuperar_a_senha_com_dois_passos() {
    use super::dois_passos::{codigo, de_base32};
    use crate::state::{Contexto, CONTEXTO};
    let (st, pasta) = base_com_admin("recuperar-senha").await;
    let passo = || chrono::Utc::now().timestamp().div_euclid(30);
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "senha": "ana12345", "perfil": "CAIXA", "mudar_senha": false })).await;
    let n = rpc(&st, "auth.doisPassosNovo", Value::Null).await;
    let segredo = de_base32(n["segredo"].as_str().unwrap()).unwrap();
    let p0 = passo();
    let l = rpc(&st, "auth.doisPassosLigar", json!({ "segredo": n["segredo"], "codigo": codigo(&segredo, p0), "senha": "1234" })).await;
    let recuperacao: Vec<String> = serde_json::from_value(l["codigos"].clone()).unwrap();
    rpc(&st, "auth.logout", Value::Null).await;
    let tentativas = |login: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, i64>("SELECT tentativas FROM utilizadores WHERE login = ?").bind(login).fetch_one(&pool).await.unwrap() }
    };
    let verificar = |login: &str, c: &str| json!({ "login": login, "codigo": c });
    let fora: String = (0..1_000_000).map(|x| format!("{x:06}")).find(|c| (-2..=2).all(|d| codigo(&segredo, passo() + d) != *c)).unwrap();

    // sem dois passos, ou utilizador que não existe: a mesma mensagem, e não conta como tentativa
    let e_ana = rpc_err(&st, "auth.recuperarVerificar", verificar("ana", "123456")).await;
    assert!(e_ana.contains("peça ao administrador uma senha provisória"), "{e_ana}");
    assert_eq!(rpc_err(&st, "auth.recuperarVerificar", verificar("ninguem", "123456")).await, e_ana);
    assert_eq!(tentativas("ana").await, 0);
    // código errado: a mesma mensagem, conta como tentativa falhada
    assert_eq!(rpc_err(&st, "auth.recuperarVerificar", verificar("admin", &fora)).await, e_ana);
    assert_eq!(tentativas("admin").await, 1);

    // código certo: autorização; uma senha fraca não a gasta; a boa muda a senha e desfaz as tentativas
    let r = rpc(&st, "auth.recuperarVerificar", verificar("admin", &codigo(&segredo, p0 + 1))).await;
    assert_eq!(r["nome"], "Admin");
    let token = r["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 64);
    assert!(rpc_err(&st, "auth.recuperarConcluir", json!({ "token": token, "nova": "12" })).await.contains("caracteres"));
    rpc(&st, "auth.recuperarConcluir", json!({ "token": token, "nova": "nova-senha-9" })).await;
    assert!(rpc_err(&st, "auth.recuperarConcluir", json!({ "token": token, "nova": "outra-senha-8" })).await.contains("expirou"), "serve uma vez");
    assert!(rpc_err(&st, "auth.recuperarConcluir", json!({ "token": "inventado", "nova": "outra-senha-8" })).await.contains("expirou"));
    assert_eq!(tentativas("admin").await, 0);
    assert!(rpc_err(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await.contains("incorrect"), "a senha antiga já não serve");
    assert_eq!(rpc(&st, "auth.login", json!({ "login": "admin", "senha": "nova-senha-9" })).await, json!({ "dois_passos": true }), "os dois passos continuam");
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "nova-senha-9", "codigo": recuperacao[0] })).await;
    rpc(&st, "auth.logout", Value::Null).await;

    // com um código de recuperação, que se gasta
    let r = rpc(&st, "auth.recuperarVerificar", verificar("admin", &recuperacao[1])).await;
    rpc(&st, "auth.recuperarConcluir", json!({ "token": r["token"], "nova": "outra-senha-7" })).await;
    assert_eq!(rpc_err(&st, "auth.recuperarVerificar", verificar("admin", &recuperacao[1])).await, e_ana);
    let usados: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE detalhe LIKE 'Recuperação da senha com um código de recuperação%'")
        .fetch_one(&st.pool)
        .await
        .unwrap();
    assert_eq!(usados, 1);
    let registos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM acessos WHERE evento = 'RECUPERACAO' AND login = 'admin'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(registos, 2);

    // pela Internet (túnel) não se recupera
    let pela_internet = |metodo: &'static str, params: Value| {
        let ctx = Contexto { sessao: std::sync::Arc::new(std::sync::Mutex::new(None)), posto: st.posto(), api_chave: None, pela_internet: true };
        CONTEXTO.scope(ctx, super::dispatch(&st, metodo, params))
    };
    let e = pela_internet("auth.recuperarVerificar", verificar("admin", &recuperacao[2])).await.unwrap_err().to_string();
    assert!(e.contains("não se recupera pela Internet"), "{e}");

    // códigos errados seguidos bloqueiam a conta (política por omissão: 5); bloqueada, nem o código certo serve
    let mut ultimo = String::new();
    for _ in 0..5 {
        ultimo = rpc_err(&st, "auth.recuperarVerificar", verificar("admin", &fora)).await;
    }
    assert!(ultimo.contains("bloqueada"), "{ultimo}");
    assert!(rpc_err(&st, "auth.recuperarVerificar", verificar("admin", &recuperacao[2])).await.contains("bloqueada"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Aparência da empresa: o jogo de cores (na instalação e em Configurações → Aparência), que se lê antes de entrar; só
/// quem configura o sistema o muda.
#[tokio::test]
async fn aparencia_jogo_de_cores() {
    // na instalação: um jogo desconhecido recusa-se antes de criar seja o que for
    let pasta = std::env::temp_dir().join(format!("kigest-aparencia-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    let setup = |cores: Value| json!({ "empresa": { "nome": "Loja Cores, Lda", "nif": "5000000001" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" }, "cores": cores });
    assert!(rpc_err(&st, "setup.inicializar", setup(json!({ "paleta": "arco-iris" }))).await.contains("desconhecido"));
    assert_eq!(rpc(&st, "setup.estado", Value::Null).await["inicializado"], false);
    rpc(&st, "setup.inicializar", setup(json!({ "paleta": "oceano" }))).await;
    assert_eq!(rpc(&st, "setup.estado", Value::Null).await["cores"], json!({ "paleta": "oceano" }));

    // em Configurações → Aparência
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    assert!(rpc_err(&st, "aparencia.guardar", json!({ "cores": { "paleta": "personalizada", "marca": "#12345", "destaque": "#e09a2b" } })).await.contains("duas cores"));
    let r = rpc(&st, "aparencia.guardar", json!({ "cores": { "paleta": "personalizada", "marca": "#1F4E8C", "destaque": " #E09A2B " } })).await;
    assert_eq!(r["cores"], json!({ "paleta": "personalizada", "marca": "#1f4e8c", "destaque": "#e09a2b" }));
    rpc(&st, "aparencia.guardar", json!({ "cores": { "paleta": "vinho", "marca": "#000000" } })).await;

    // antes de entrar, o ecrã lê as cores; sem sessão nada se muda
    rpc(&st, "auth.logout", Value::Null).await;
    assert_eq!(rpc(&st, "setup.estado", Value::Null).await["cores"], json!({ "paleta": "vinho" }));
    assert!(!rpc_err(&st, "aparencia.guardar", json!({ "cores": { "paleta": "azul" } })).await.is_empty());

    // quem não configura o sistema (perfil de caixa) não muda a aparência
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Caixa", "login": "caixa", "senha": "caixa123", "perfil": "CAIXA", "mudar_senha": false })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "caixa", "senha": "caixa123" })).await;
    assert!(rpc_err(&st, "aparencia.guardar", json!({ "cores": { "paleta": "azul" } })).await.contains("permiss"));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// O meu perfil: cada um muda o seu nome, contactos e fotografia (e só os seus); o nome novo fica na sessão e o
/// antigo no registo de acessos; o utilizador (login) e o perfil não mudam por aqui.
#[tokio::test]
async fn o_meu_perfil() {
    let (st, pasta) = base_com_admin("perfil").await;
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Ana", "login": "ana", "senha": "ana12345", "perfil": "CAIXA", "mudar_senha": false })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    rpc(&st, "auth.login", json!({ "login": "ana", "senha": "ana12345" })).await;
    let p = rpc(&st, "auth.perfil", Value::Null).await;
    assert_eq!((p["nome"].clone(), p["login"].clone(), p["foto"].clone()), (json!("Ana"), json!("ana"), Value::Null));

    let guardar = |extra: Value| {
        let mut v = json!({ "nome": "Ana Paula", "email": "ana@loja.ao", "telefone": "923 000 111" });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    assert!(rpc_err(&st, "auth.guardarPerfil", guardar(json!({ "nome": "  " }))).await.contains("nome"));
    assert!(rpc_err(&st, "auth.guardarPerfil", guardar(json!({ "email": "ana loja" }))).await.contains("email"));
    assert!(rpc_err(&st, "auth.guardarPerfil", guardar(json!({ "foto": "data:image/svg+xml;base64,PHN2Zz4=" }))).await.contains("JPEG, PNG ou WebP"));
    let foto = "data:image/jpeg;base64,/9j/4AAQSkZJRg==";
    let s = rpc(&st, "auth.guardarPerfil", guardar(json!({ "foto": foto, "login": "outra", "perfil": "ADMIN" }))).await;
    assert_eq!((s["nome"].clone(), s["login"].clone(), s["perfil"].clone()), (json!("Ana Paula"), json!("ana"), json!("CAIXA")), "o login e o perfil não mudam");
    assert_eq!(rpc(&st, "auth.sessao", Value::Null).await["nome"], "Ana Paula");
    let p = rpc(&st, "auth.perfil", Value::Null).await;
    assert_eq!((p["email"].clone(), p["telefone"].clone(), p["foto"].clone()), (json!("ana@loja.ao"), json!("923 000 111"), json!(foto)));
    // sem `foto`, a fotografia fica; com "" sai
    rpc(&st, "auth.guardarPerfil", json!({ "nome": "Ana Paula", "email": "", "telefone": "" })).await;
    let p = rpc(&st, "auth.perfil", Value::Null).await;
    assert_eq!((p["foto"].clone(), p["email"].clone()), (json!(foto), json!("")));
    rpc(&st, "auth.guardarPerfil", json!({ "nome": "Ana Paula", "foto": "" })).await;
    assert!(rpc(&st, "auth.perfil", Value::Null).await["foto"].is_null());
    let registo: String = sqlx::query_scalar("SELECT detalhe FROM acessos WHERE evento = 'PERFIL' ORDER BY id LIMIT 1").fetch_one(&st.pool).await.unwrap();
    assert_eq!(registo, "nome: «Ana» → «Ana Paula»");

    // o administrador vê a fotografia na lista; sem sessão não há perfil
    rpc(&st, "auth.guardarPerfil", json!({ "nome": "Ana Paula", "foto": foto })).await;
    rpc(&st, "auth.logout", Value::Null).await;
    assert!(!rpc_err(&st, "auth.perfil", Value::Null).await.is_empty());
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let lista = rpc(&st, "utilizadores.listar", Value::Null).await;
    let ana = lista.as_array().unwrap().iter().find(|u| u["login"] == "ana").unwrap();
    assert_eq!((ana["nome"].clone(), ana["foto"].clone()), (json!("Ana Paula"), json!(foto)));
    assert!(rpc(&st, "auth.perfil", Value::Null).await["foto"].is_null(), "o perfil é o de quem está a usar");
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Fotografias dos artigos: conferidas, miniaturas em lote para o POS e a imagem pela API.
#[tokio::test]
async fn fotografias_dos_artigos() {
    let (st, pasta) = base_com_admin("fotos").await;
    let a = rpc(&st, "artigos.guardar", json!({ "descricao": "Sumo de múcua", "taxa_iva_id": 1, "preco1": 50000 })).await["id"].as_i64().unwrap();
    let webp = |extra: &str| format!("data:image/webp;base64,{}", B64.encode([b"RIFF\x10\x00\x00\x00WEBPVP8 ".as_slice(), extra.as_bytes()].concat()));
    assert!(rpc_err(&st, "fotos.guardar", json!({ "artigo_id": a, "miniatura": "data:image/svg+xml;base64,PHN2Zz4=", "imagem": webp("x") })).await.contains("WebP, JPEG ou PNG"));
    assert!(rpc_err(&st, "fotos.guardar", json!({ "artigo_id": 9999, "miniatura": webp("m"), "imagem": webp("i") })).await.contains("encontr"));
    rpc(&st, "fotos.guardar", json!({ "artigo_id": a, "miniatura": webp("m"), "imagem": webp("imagem grande") })).await;
    let lista = rpc(&st, "artigos.listar", json!({ "q": "múcua" })).await;
    let versao = lista[0]["foto"].as_str().unwrap().to_string();
    assert!(!versao.is_empty(), "a lista diz que há foto (e de quando, para a cache)");
    let m = rpc(&st, "fotos.miniaturas", json!({ "ids": [a, 9999] })).await;
    assert_eq!(m.as_object().unwrap().len(), 1);
    assert_eq!(m[a.to_string()], webp("m"));
    assert_eq!(rpc(&st, "fotos.obter", json!({ "artigo_id": a })).await["imagem"], webp("imagem grande"));
    // trocar a foto muda a versão; apagar tira-a
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    rpc(&st, "fotos.guardar", json!({ "artigo_id": a, "miniatura": webp("m2"), "imagem": webp("i2") })).await;
    assert_ne!(rpc(&st, "artigos.listar", json!({ "q": "múcua" })).await[0]["foto"].as_str().unwrap(), versao);
    rpc(&st, "fotos.apagar", json!({ "artigo_id": a })).await;
    assert!(rpc(&st, "artigos.listar", json!({ "q": "múcua" })).await[0]["foto"].is_null());
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Numerário em moeda estrangeira no POS: valor em Kz ao câmbio do dia, troco em Kz e gaveta conferida por moeda.
#[tokio::test]
async fn pagamento_em_moeda_estrangeira() {
    let (st, pasta) = base_com_admin("moeda_pos").await;
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Garrafão de água", "tipo": "S", "taxa_iva_id": 1, "preco1": 5_000_000 })).await["id"].as_i64().unwrap();
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 10_000_000 })).await;
    let venda = |pags: Value| json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": pags });
    // sem câmbio definido não se aceita
    assert!(rpc_err(&st, "documentos.emitir", venda(json!([{ "modo": "NU", "valor": 0, "moeda": "USD", "valor_moeda": 10_000 }]))).await.contains("câmbio"));
    rpc(&st, "moedas.guardar", json!({ "codigo": "USD", "descricao": "Dólar americano", "simbolo": "US$", "cambio": 900.0 })).await;
    assert!(rpc_err(&st, "documentos.emitir", venda(json!([{ "modo": "CC", "valor": 0, "moeda": "USD", "valor_moeda": 10_000 }]))).await.contains("numerário"));

    // total 57 000 Kz (50 000 + IVA 14%) pago com 100 US$ (= 90 000 Kz): troco de 33 000 Kz
    let d = rpc(&st, "documentos.emitir", venda(json!([{ "modo": "NU", "valor": 0, "moeda": "USD", "valor_moeda": 10_000 }]))).await;
    assert_eq!(d["documento"]["total"], 5_700_000);
    assert_eq!(d["documento"]["troco"], 3_300_000);
    let pg = &d["pagamentos"][0];
    assert_eq!((pg["modo_codigo"].as_str(), pg["valor"].as_i64(), pg["moeda"].as_str(), pg["valor_moeda"].as_i64()), (Some("NU"), Some(5_700_000), Some("USD"), Some(10_000)));

    // misto: 50 US$ (45 000 Kz) + 20 000 Kz em notas para 57 000 Kz → troco 8 000 Kz das notas
    let d2 = rpc(&st, "documentos.emitir", venda(json!([{ "modo": "NU", "valor": 0, "moeda": "USD", "valor_moeda": 5_000 }, { "modo": "NU", "valor": 2_000_000 }]))).await;
    assert_eq!(d2["documento"]["troco"], 800_000);
    let soma: i64 = d2["pagamentos"].as_array().unwrap().iter().map(|p| p["valor"].as_i64().unwrap()).sum();
    assert_eq!(soma, 5_700_000, "os pagamentos somam o total (SAF-T)");

    // gaveta: kwanzas = 100 000 inicial − 33 000 de troco + 20 000 − 8 000 = 79 000; dólares = 150 US$
    let c = rpc(&st, "caixa.estado", Value::Null).await;
    assert_eq!(c["numerario_esperado"], 7_900_000, "{c}");
    let usd = &c["moedas_estrangeiras"][0];
    assert_eq!((usd["moeda"].as_str(), usd["valor"].as_i64()), (Some("USD"), Some(15_000)));
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Utilizador de uma loja (servidor central com várias livrarias): só entra nos postos dela e só vê os documentos,
/// os relatórios e o painel dela; o administrador vê todas.
#[tokio::test]
async fn utilizador_de_uma_loja() {
    let (st, pasta) = base_com_admin("utilizador_loja").await;
    let lubango = rpc(&st, "tabela.guardar", json!({ "tabela": "armazens", "descricao": "Livraria do Lubango" })).await["id"].as_i64().unwrap();
    let posto_lub = rpc(&st, "posto.guardar", json!({ "descricao": "Caixa do Lubango", "armazem_id": lubango, "serie_prefixo": "L" })).await["id"].as_i64().unwrap();
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Livro", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    let fr_sede = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": art, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await["documento"]["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "armazem_id": lubango, "linhas": [{ "artigo_id": art, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 228000 }] })).await;
    // o administrador vê todas as lojas: não se lhe põe uma
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "Outro Admin", "login": "admin2", "perfil": "ADMIN", "senha": "segredo99", "armazem_id": lubango })).await.contains("administrador"));
    assert!(rpc_err(&st, "utilizadores.guardar", json!({ "nome": "X", "login": "x", "perfil": "GESTOR", "senha": "segredo99", "armazem_id": 999 })).await.contains("inválida"));
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Gerente do Lubango", "login": "gerlub", "perfil": "GESTOR", "senha": "segredo99", "mudar_senha": false, "armazem_id": lubango })).await;
    rpc(&st, "auth.logout", json!({})).await;

    // num posto de outra loja não entra
    assert!(rpc_err(&st, "auth.login", json!({ "login": "gerlub", "senha": "segredo99" })).await.contains("Livraria do Lubango"));
    // num posto da loja dele entra, e a sessão diz a loja
    st.carregar_posto(posto_lub).await.unwrap();
    let s = rpc(&st, "auth.login", json!({ "login": "gerlub", "senha": "segredo99" })).await;
    assert_eq!(s["armazem_id"], lubango);

    // documentos: só os da loja; o da sede não se abre
    let docs = rpc(&st, "documentos.listar", json!({ "tipo": "FR" })).await;
    assert_eq!(docs.as_array().unwrap().len(), 1, "{docs}");
    assert!(rpc_err(&st, "documentos.obter", json!({ "id": fr_sede })).await.contains("outra loja"));
    // relatórios: a loja dele, peça o que pedir
    let hoje = db::hoje();
    let mes = rpc(&st, "relatorios.vendas", json!({ "de": hoje, "ate": hoje, "agrupar": "dia", "armazem_id": 1 })).await;
    assert_eq!(mes[0]["total"], 228000, "{mes}");
    let ano: i64 = hoje[..4].parse().unwrap();
    let mes_n: usize = hoje[5..7].parse().unwrap();
    assert_eq!(rpc(&st, "relatorios.sintese", json!({ "ano": ano })).await["meses"][mes_n - 1]["atual"], 228000);
    // o painel inicial: as vendas de hoje da loja
    assert_eq!(rpc(&st, "dashboard", Value::Null).await["vendas_hoje"], 228000);

    // o administrador continua a ver tudo
    rpc(&st, "auth.logout", json!({})).await;
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    assert_eq!(rpc(&st, "documentos.listar", json!({ "tipo": "FR" })).await.as_array().unwrap().len(), 2);
    assert_eq!(rpc(&st, "dashboard", Value::Null).await["vendas_hoje"], 342000);
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Auditoria séria: o antes e o depois de cada ficha, sem senhas nem imagens, e o selo em cadeia que denuncia o que
/// se mexer na base de dados por fora (linhas alteradas no meio ou apagadas no fim).
#[tokio::test]
async fn auditoria_com_antes_depois_e_selo() {
    let (st, pasta) = base_com_admin("auditoria-seria").await;
    let hoje = db::hoje();

    // artigo: criar, alterar, gravar sem mudanças
    let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Caneta azul", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    let alterado = json!({ "id": art, "descricao": "Caneta azul BIC", "taxa_iva_id": 1, "preco1": 120000 });
    rpc(&st, "artigos.guardar", alterado.clone()).await;
    rpc(&st, "artigos.guardar", alterado).await;
    let h = rpc(&st, "auditoria.historico", json!({ "entidade": "artigos", "registo_id": art })).await;
    let h = h.as_array().unwrap();
    assert_eq!(h.len(), 2, "uma criação e uma alteração (a gravação sem mudanças não conta): {h:?}");
    assert_eq!(h[0]["acao"], "ALTERAR");
    assert_eq!(h[0]["antes"]["preco1"], 100000);
    assert_eq!(h[0]["depois"]["preco1"], 120000);
    assert_eq!(h[0]["depois"]["descricao"], "Caneta azul BIC");
    assert!(h[0]["antes"].get("taxa_iva_id").is_none(), "numa alteração só ficam os campos que mudaram");
    assert!(h[0]["detalhe"].as_str().unwrap().contains("preço 1"), "{}", h[0]["detalhe"]);
    assert_eq!(h[0]["origem"], "local");
    assert_eq!(h[0]["utilizador"], "Admin");
    assert_eq!(h[1]["acao"], "CRIAR");
    assert_eq!(h[1]["depois"]["descricao"], "Caneta azul");

    // utilizador: a senha e o PIN nunca vão para o registo
    let u = rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await["id"].as_i64().unwrap();
    rpc(&st, "utilizadores.guardar", json!({ "id": u, "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "pin": "4321" })).await;
    let hu = rpc(&st, "auditoria.historico", json!({ "entidade": "utilizadores", "registo_id": u })).await;
    let texto = hu.to_string();
    let hash: String = sqlx::query_scalar("SELECT senha_hash FROM utilizadores WHERE id = ?").bind(u).fetch_one(&st.pool).await.unwrap();
    assert!(!texto.contains(&hash) && !texto.contains("caixa123") && !texto.contains("4321"), "segredos no registo: {texto}");
    assert_eq!(hu[1]["depois"]["senha_hash"], "•••");
    assert!(hu[0]["depois"]["pin_hash"].as_str().unwrap().starts_with("•••"), "{}", hu[0]);

    // configurações: só as chaves que mudaram
    rpc(&st, "seguranca.guardar", json!({ "senha_min": 9, "tentativas": 4, "bloqueio_min": 15, "inatividade_min": 10, "login_pin": true })).await;
    let cfg = rpc(&st, "auditoria.listar", json!({ "de": hoje, "ate": hoje, "entidade": "config" })).await;
    let cfg = cfg.as_array().unwrap();
    assert_eq!(cfg.len(), 1, "{cfg:?}");
    assert!(cfg[0]["depois"].as_object().unwrap().values().any(|v| v == "9"), "{}", cfg[0]);
    assert_eq!(cfg[0]["entidade_nome"], "Configurações");

    // filtros: texto, acção e utilizador
    let por_texto = rpc(&st, "auditoria.listar", json!({ "de": hoje, "ate": hoje, "texto": "BIC" })).await;
    assert!(por_texto.as_array().unwrap().iter().all(|l| l["entidade"] == "artigos"));
    assert!(!por_texto.as_array().unwrap().is_empty());
    let criacoes = rpc(&st, "auditoria.listar", json!({ "de": hoje, "ate": hoje, "evento": "CRIAR" })).await;
    assert_eq!(criacoes.as_array().unwrap().len(), 2);
    let opcoes = rpc(&st, "auditoria.opcoes", Value::Null).await;
    assert!(opcoes["acoes"].as_array().unwrap().iter().any(|a| a == "ALTERAR"));
    assert!(opcoes["fichas"].as_array().unwrap().iter().any(|f| f["nome"] == "Artigo"));

    // selo: todas as linhas seladas e a cadeia íntegra
    let sem_selo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auditoria WHERE selo IS NULL").fetch_one(&st.pool).await.unwrap();
    assert_eq!(sem_selo, 0);
    let v = rpc(&st, "auditoria.verificar", Value::Null).await;
    assert_eq!(v["integra"], true, "{v}");

    // pelo programa não se apaga nem se altera
    assert!(sqlx::query("DELETE FROM auditoria").execute(&st.pool).await.is_err());
    assert!(sqlx::query("UPDATE auditoria SET detalhe = 'outra coisa' WHERE id = 1").execute(&st.pool).await.is_err());
    assert!(sqlx::query("UPDATE auditoria SET selo = 'x' WHERE id = 1").execute(&st.pool).await.is_err());

    // por fora (sem os gatilhos): mudar uma linha do meio parte a cadeia nessa linha
    let alvo: i64 = sqlx::query_scalar("SELECT id FROM auditoria WHERE acao = 'ALTERAR' AND entidade = 'artigos'").fetch_one(&st.pool).await.unwrap();
    sqlx::query("DROP TRIGGER auditoria_nao_se_altera").execute(&st.pool).await.unwrap();
    sqlx::query("UPDATE auditoria SET depois = '{\"preco1\":110000}' WHERE id = ?").bind(alvo).execute(&st.pool).await.unwrap();
    let v = rpc(&st, "auditoria.verificar", Value::Null).await;
    assert_eq!(v["integra"], false);
    assert_eq!(v["problema"]["id"], alvo, "{v}");
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);

    // noutra base: apagar as últimas linhas por fora fica escrito no próprio registo
    let (st, pasta) = base_com_admin("auditoria-quebra").await;
    rpc(&st, "entidades.guardar", json!({ "nome": "Cliente A", "nif": "5417000001" })).await;
    rpc(&st, "entidades.guardar", json!({ "nome": "Cliente B", "nif": "5417000002" })).await;
    assert_eq!(rpc(&st, "auditoria.verificar", Value::Null).await["integra"], true);
    let ultimo: i64 = sqlx::query_scalar("SELECT MAX(id) FROM auditoria").fetch_one(&st.pool).await.unwrap();
    sqlx::query("DROP TRIGGER auditoria_nao_se_apaga").execute(&st.pool).await.unwrap();
    sqlx::query("DELETE FROM auditoria WHERE id = ?").bind(ultimo).execute(&st.pool).await.unwrap();
    // a actividade seguinte dá pela falta e deixa-o escrito, selado
    rpc(&st, "entidades.guardar", json!({ "nome": "Cliente C", "nif": "5417000003" })).await;
    let quebra: String = sqlx::query_scalar("SELECT detalhe FROM auditoria WHERE acao = 'AUDITORIA_QUEBRA'").fetch_one(&st.pool).await.unwrap();
    assert!(quebra.contains(&format!("n.º {ultimo}")), "{quebra}");
    let v = rpc(&st, "auditoria.verificar", Value::Null).await;
    assert_eq!(v["integra"], false);
    assert_eq!(v["quebras"], 1);
    assert!(v["problema"]["motivo"].as_str().unwrap().contains("fora do KanGest"), "{v}");
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// CRUD seguros: eliminar só o que nunca foi usado (o resto fica desactivado), acções em lote e avisos de duplicados.
#[tokio::test]
async fn eliminar_com_seguranca_e_accoes_em_lote() {
    let (st, pasta) = base_com_admin("cruds").await;
    let hoje = db::hoje();

    // artigo nunca usado: elimina-se de vez
    let novo = rpc(&st, "artigos.guardar", json!({ "descricao": "Artigo de teste", "taxa_iva_id": 1, "preco1": 10000, "codigo_barras": "7700001" })).await["id"].as_i64().unwrap();
    assert_eq!(rpc(&st, "artigos.codigoUsado", json!({ "codigo": "7700001" })).await["id"], novo);
    assert!(rpc(&st, "artigos.codigoUsado", json!({ "codigo": "7700001", "id": novo })).await.is_null());
    assert_eq!(rpc(&st, "artigos.eliminar", json!({ "id": novo })).await["eliminado"], true);
    let existe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artigos WHERE id = ?").bind(novo).fetch_one(&st.pool).await.unwrap();
    assert_eq!(existe, 0);
    let h = rpc(&st, "auditoria.historico", json!({ "entidade": "artigos", "registo_id": novo })).await;
    assert_eq!(h[0]["acao"], "ELIMINAR");
    assert_eq!(h[0]["antes"]["descricao"], "Artigo de teste");

    // artigo com movimentos: fica desactivado (e reactiva-se em lote)
    let usado = rpc(&st, "artigos.guardar", json!({ "descricao": "Feijão 1kg", "taxa_iva_id": 1, "preco1": 87720 })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": usado, "quantidade": 5, "preco_unitario": 50000 }] })).await;
    let r = rpc(&st, "artigos.eliminar", json!({ "id": usado })).await;
    assert_eq!(r["desactivado"], true, "{r}");
    assert_eq!(rpc(&st, "auditoria.historico", json!({ "entidade": "artigos", "registo_id": usado })).await[0]["acao"], "DESACTIVAR");
    rpc(&st, "artigos.lote", json!({ "ids": [usado], "accao": "activar" })).await;
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": usado })).await["ativo"], 1);

    // preços em lote: +10% no PVP 1, arredondado a 50 Kz (1 000,00 c/ IVA → 1 100,00)
    let pvp = rpc(&st, "artigos.obter", json!({ "id": usado })).await["preco1_iva"].as_i64().unwrap();
    assert_eq!(pvp, 100001, "PVP de partida");
    let r = rpc(&st, "artigos.lote", json!({ "ids": [usado], "accao": "preco", "percentagem": 10, "arredondar": 50 })).await;
    assert_eq!(r["alterados"], 1);
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": usado })).await["preco1_iva"], 110000);
    assert!(rpc_err(&st, "artigos.lote", json!({ "ids": [usado], "accao": "preco", "percentagem": 0 })).await.contains("percentagem"));
    let familia = rpc(&st, "tabela.guardar", json!({ "tabela": "familias", "descricao": "Mercearia" })).await["id"].as_i64().unwrap();
    rpc(&st, "artigos.lote", json!({ "ids": [usado], "accao": "familia", "familia_id": familia })).await;
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": usado })).await["familia_id"], familia);
    let alt = rpc(&st, "auditoria.listar", json!({ "de": hoje, "ate": hoje, "entidade": "artigos", "registo_id": usado, "evento": "ALTERAR" })).await;
    assert!(alt.as_array().unwrap().iter().any(|l| l["antes"]["pvp1"].is_number() || l["antes"]["preco1"] == 87720), "{alt}");

    // entidades: sem documentos elimina-se; com documentos desactiva-se e sai da lista
    let sem = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Sem Nada", "nif": "5417000010" })).await["id"].as_i64().unwrap();
    assert_eq!(rpc(&st, "entidades.nifUsado", json!({ "nif": "5417000010" })).await["id"], sem);
    assert_eq!(rpc(&st, "entidades.eliminar", json!({ "id": sem })).await["eliminado"], true);
    let com = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente Com Proforma", "nif": "5417000011" })).await["id"].as_i64().unwrap();
    rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": com, "linhas": [{ "artigo_id": usado, "quantidade": 1 }] })).await;
    assert_eq!(rpc(&st, "entidades.eliminar", json!({ "id": com })).await["desactivado"], true);
    let activos = rpc(&st, "entidades.listar", json!({ "tipo": "C", "q": "Proforma" })).await;
    assert_eq!(activos.as_array().unwrap().len(), 0);
    let todos = rpc(&st, "entidades.listar", json!({ "tipo": "C", "q": "Proforma", "inativos": true })).await;
    assert_eq!(todos[0]["ativo"], 0);
    // reactivar: recusado se o NIF já estiver noutra ficha activa
    let outra = rpc(&st, "entidades.guardar", json!({ "nome": "Mesmo NIF", "nif": "5417000011" })).await["id"].as_i64().unwrap();
    assert!(rpc_err(&st, "entidades.lote", json!({ "ids": [com], "accao": "activar" })).await.contains("Mesmo NIF"));
    rpc(&st, "entidades.lote", json!({ "ids": [outra], "accao": "desactivar" })).await;
    assert_eq!(rpc(&st, "entidades.lote", json!({ "ids": [com], "accao": "activar" })).await["alterados"], 1);
    assert!(rpc_err(&st, "entidades.eliminar", json!({ "id": 1 })).await.contains("Consumidor Final"));
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Leitor de códigos: prefixo AIM, UPC-A contra EAN-13, GS1 (DataMatrix dos medicamentos) com lote e validade,
/// ISBN-10, e associar a um artigo um código que o leitor não conhecia.
#[tokio::test]
async fn leitor_de_codigos() {
    use super::catalogo::{gtin_valido, interpretar_codigo};
    assert!(gtin_valido("5601234567892") && gtin_valido("012345678905") && gtin_valido("05601234567892") && gtin_valido("96385074"));
    assert!(!gtin_valido("5601234567890"));
    let l = interpretar_codigo("]E05601234567892\r");
    assert_eq!(l.limpo, "5601234567892");
    assert!(interpretar_codigo("012345678905").candidatos.contains(&"0012345678905".to_string()));
    // DataMatrix com o prefixo AIM: GTIN e validade
    let g = interpretar_codigo("]d2010560123456789217271231");
    assert_eq!(g.gs1.as_ref().unwrap()["validade"], "2027-12-31");
    // validade só com o mês (dia 00), lote até ao separador FNC1, série
    let g = interpretar_codigo("01056012345678921727120010LOTE42\u{1d}21SER9");
    let gs1 = g.gs1.expect("GS1");
    assert_eq!(gs1["gtin"], "05601234567892");
    assert_eq!(gs1["validade"], "2027-12");
    assert_eq!(gs1["lote"], "LOTE42");
    assert_eq!(gs1["serie"], "SER9");
    assert!(g.candidatos.contains(&"5601234567892".to_string()));
    // com parênteses (o texto impresso por baixo)
    let g = interpretar_codigo("(01)05601234567892(17)271231(10)L7");
    assert_eq!(g.gs1.unwrap()["lote"], "L7");
    // ISBN-10 com hífenes → ISBN-13
    assert!(interpretar_codigo("0-306-40615-2").candidatos.contains(&"9780306406157".to_string()));

    let (st, pasta) = base_com_admin("leitor").await;
    let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz", "taxa_iva_id": 1, "preco1": 100000, "codigo_barras": "5601234567892" })).await["id"].as_i64().unwrap();
    let cola = rpc(&st, "artigos.guardar", json!({ "descricao": "Cola lata", "taxa_iva_id": 1, "preco1": 30000, "codigo_barras": "0012345678905" })).await["id"].as_i64().unwrap();
    assert_eq!(rpc(&st, "artigos.codigo", json!({ "codigo": "]E05601234567892" })).await["id"], arroz);
    assert_eq!(rpc(&st, "artigos.codigo", json!({ "codigo": "012345678905" })).await["id"], cola, "UPC-A de 12 algarismos encontra o EAN-13 com o zero");
    let r = rpc(&st, "artigos.codigo", json!({ "codigo": "01056012345678921727123110L9\u{1d}" })).await;
    assert_eq!(r["id"], arroz);
    assert_eq!(r["gs1"]["validade"], "2027-12-31");
    // a API de integração lê da mesma maneira
    assert_eq!(super::catalogo::ler_codigo(&st.pool, "012345678905").await.unwrap(), Some((cola, None)));

    // código desconhecido: associa-se a um artigo (caixa de 6) e passa a vender 6
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": "7891234567895" })).await.is_null());
    let r = rpc(&st, "artigos.associarCodigo", json!({ "artigo_id": cola, "codigo": "7891234567895", "quantidade": 6, "descricao": "Caixa de 6" })).await;
    assert_eq!(r["id"], cola);
    assert_eq!(r["quantidade_lida"], 6.0);
    assert_eq!(rpc(&st, "artigos.codigo", json!({ "codigo": "7891234567895" })).await["quantidade_lida"], 6.0);
    assert!(rpc_err(&st, "artigos.associarCodigo", json!({ "artigo_id": arroz, "codigo": "7891234567895" })).await.contains("Cola lata"));
    assert!(rpc_err(&st, "artigos.associarCodigo", json!({ "artigo_id": arroz, "codigo": "012345678905" })).await.contains("já está"));
    let h = rpc(&st, "auditoria.historico", json!({ "entidade": "artigos", "registo_id": cola })).await;
    assert_eq!(h[0]["depois"]["outro_codigo"], "7891234567895");
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Moedas no pagamento do POS: só as escolhidas (na instalação ou em Tabelas → Moedas), activas e com câmbio.
#[tokio::test]
async fn moedas_aceites_no_pos() {
    // instalação: escolhe-se o dólar com o câmbio; sem câmbio é recusado
    let pasta = std::env::temp_dir().join(format!("kigest-moedas-pos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
    let base = json!({ "empresa": { "nome": "Loja Teste, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } });
    let mut sem_cambio = base.clone();
    sem_cambio["moedas_pos"] = json!([{ "codigo": "USD", "cambio": 0 }]);
    assert!(rpc_err(&st, "setup.inicializar", sem_cambio).await.contains("câmbio"));
    let mut com = base.clone();
    com["moedas_pos"] = json!([{ "codigo": "USD", "cambio": 956.45 }]);
    rpc(&st, "setup.inicializar", com).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let moedas = rpc(&st, "moedas.listar", Value::Null).await;
    let m = |c: &str| moedas.as_array().unwrap().iter().find(|x| x["codigo"] == c).unwrap().clone();
    assert_eq!(m("USD")["pos"], 1);
    assert_eq!(m("USD")["cambio"], 956.45);
    assert_eq!(m("EUR")["pos"], 0);

    // Tabelas → Moedas: aceitar exige câmbio; gravar sem dizer mantém; desmarcar tira
    assert!(rpc_err(&st, "moedas.guardar", json!({ "codigo": "EUR", "descricao": "Euro", "simbolo": "€", "cambio": 0, "pos": true })).await.contains("câmbio"));
    rpc(&st, "moedas.guardar", json!({ "codigo": "EUR", "descricao": "Euro", "simbolo": "€", "cambio": 1100, "pos": true })).await;
    rpc(&st, "moedas.guardar", json!({ "codigo": "EUR", "descricao": "Euro", "simbolo": "€", "cambio": 1105 })).await;
    let eur = |v: &Value| v.as_array().unwrap().iter().find(|x| x["codigo"] == "EUR").unwrap()["pos"].clone();
    assert_eq!(eur(&rpc(&st, "moedas.listar", Value::Null).await), 1);
    rpc(&st, "moedas.guardar", json!({ "codigo": "EUR", "descricao": "Euro", "simbolo": "€", "cambio": 1105, "pos": false })).await;
    assert_eq!(eur(&rpc(&st, "moedas.listar", Value::Null).await), 0);
    assert!(rpc_err(&st, "moedas.guardar", json!({ "codigo": "AOA", "descricao": "Kwanza", "simbolo": "Kz", "cambio": 1, "pos": true })).await.contains("Kwanza"));
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

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
    assert_eq!(rpc(&st, "documentos.obter", json!({ "id": pp_id })).await["documento"]["estado"], "F");
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
    // contagem por notas e moedas: o valor físico é a soma (2×5000 + 1×2000 + 1×200 + 1×50 + 2×10 + 1×0,50 Kz = 12 270,50)
    assert_eq!(cx["fecho"]["notas"], json!([500000, 200000, 100000, 50000, 20000]));
    assert!(rpc_err(&st, "caixa.fechar", json!({ "contagem": [{ "valor": 500000, "quantidade": -1 }] })).await.contains("inválida"));
    assert!(rpc_err(&st, "caixa.fechar", json!({})).await.contains("valor contado"));
    let contagem = json!([{ "valor": 500000, "quantidade": 2 }, { "valor": 200000, "quantidade": 1 }, { "valor": 20000, "quantidade": 1 },
                          { "valor": 5000, "quantidade": 1 }, { "valor": 1000, "quantidade": 2 }, { "valor": 50, "quantidade": 1 }, { "valor": 100000, "quantidade": 0 }]);
    let fecho = rpc(&st, "caixa.fechar", json!({ "contagem": contagem })).await;
    assert_eq!(fecho["sessao"]["estado"], "FECHADA");
    assert_eq!(fecho["sessao"]["valor_contado"], 1227050);
    assert_eq!(fecho["contagem"].as_array().unwrap().len(), 6, "as notas sem quantidade não ficam");
    assert!(rpc(&st, "caixa.estado", Value::Null).await.is_null());


    // ---- painel e todos os relatórios executam ----
    let dash = rpc(&st, "dashboard", Value::Null).await;
    // FR1 228000 + FR2 114000 + FR3 114000 + FT(proforma) 342000 − NC 114000 (FT anulada não conta)
    assert_eq!(dash["vendas_hoje"], 228000 + 114000 + 114000 + 342000 - 114000);
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
    assert!(super::sistema::backup_automatico(&st.pool, &pasta).await.unwrap().is_some());
    assert!(super::sistema::backup_automatico(&st.pool, &pasta).await.unwrap().is_none(), "uma cópia automática por dia");
    assert_eq!(rpc(&st, "sistema.copias", Value::Null).await["copias"].as_array().unwrap().len(), 1);

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

    // fecho cego: o operador (sem relatórios) não vê o esperado nem os totais até fechar; o gestor vê
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    if !rpc(&st, "caixa.estado", Value::Null).await.is_null() {
        rpc(&st, "caixa.fechar", json!({ "valor_contado": 0 })).await;
    }
    assert!(rpc_err(&st, "caixa.configGuardar", json!({ "fecho_cego": true, "notas": "5000, abc", "moedas": "" })).await.contains("inválido"));
    let cfg = rpc(&st, "caixa.configGuardar", json!({ "fecho_cego": true, "notas": "5000,2000,1000,500,200", "moedas": "100 50 0.5" })).await;
    assert_eq!(cfg["moedas"], json!([10000, 5000, 50]));
    rpc(&st, "utilizadores.guardar", json!({ "nome": "Caixa Cega", "login": "cega", "perfil": "CAIXA", "senha": "cega1234", "mudar_senha": false })).await;
    rpc(&st, "auth.login", json!({ "login": "cega", "senha": "cega1234" })).await;
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 500000 })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let cego = rpc(&st, "caixa.estado", Value::Null).await;
    assert_eq!((cego["cego"].as_bool(), cego["numerario_esperado"].is_null(), cego["total_vendido"].is_null()), (Some(true), true, true), "{cego}");
    assert!(cego["por_modo"].as_array().unwrap().iter().all(|m| m["total"].is_null()));
    assert_eq!(cego["documentos"], 1);
    let fechado = rpc(&st, "caixa.fechar", json!({ "valor_contado": 614000 })).await;
    assert_eq!((fechado["numerario_esperado"].as_i64(), fechado["cego"].as_bool()), (Some(614000), Some(false)), "depois de fechar mostra tudo");
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    rpc(&st, "caixa.configGuardar", json!({ "fecho_cego": false, "notas": "5000,2000,1000,500,200", "moedas": "200,100,50,20,10,5,2,1,0.5" })).await;

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
    assert_eq!(agt_info["certificado"], "", "o certificado do KIGEST não é activado automaticamente");
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
    assert!(rpc_err(&st, "artigos.guardar", json!({ "id": kit, "descricao": "Kit", "taxa_iva_id": 1, "preco1": 1, "composto": true, "componentes": [{ "componente_id": kit, "quantidade": 1 }] })).await.contains("kit"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Kit de kits", "taxa_iva_id": 1, "preco1": 1, "composto": true, "componentes": [{ "componente_id": kit, "quantidade": 1 }] })).await.contains("composto"));
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
    assert!(xml.contains("<HashControl>0</HashControl>") && !xml.contains("<HashControl>1</HashControl>"), "sem certificado: HashControl 0");
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
    let cfg = crate::rede::Config { modo: "servidor".into(), porta, chave: "segredo123".into(), ..Default::default() };
    crate::rede::registar_estado(arc.clone());
    assert!(crate::rede::iniciar_servidor(arc.clone(), &crate::rede::Config { chave: "curta".into(), ..cfg.clone() }).is_err(), "a chave tem de ter 6 caracteres");
    assert_eq!(crate::rede::iniciar_servidor(arc.clone(), &cfg).unwrap(), porta);
    let gravado: Value = serde_json::from_str(&std::fs::read_to_string(pasta.join("rede.json")).unwrap()).unwrap();
    assert_eq!(B64.decode(gravado["sal"].as_str().unwrap()).unwrap().len(), 16, "o servidor cria e guarda o sal da cifra");

    // o posto 2, noutra máquina: pedidos cifrados com a chave de rede
    let cliente = crate::rede::Config { modo: "cliente".into(), porta, servidor: format!("127.0.0.1:{porta}"), chave: "segredo123".into(), ..Default::default() };
    let pedir_cfg = |c: &crate::rede::Config, token: &str, metodo: &str, params: Value| -> Value {
        match crate::rede::pedir_com_token(c, token, metodo, params, posto2, "Caixa 2") {
            Ok(d) => json!({ "ok": true, "dados": d }),
            Err(e) => json!({ "ok": false, "erro": e.to_string() }),
        }
    };
    let como = |token: &str, metodo: &str, params: Value| pedir_cfg(&cliente, token, metodo, params);
    let http = |metodo: &str, corpo: &str| -> (String, Value) {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).expect("ligar ao servidor");
        s.write_all(format!("{metodo} / HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len()).as_bytes()).unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        let (cab, corpo) = r.split_once("\r\n\r\n").expect("resposta HTTP");
        (cab.lines().next().unwrap().to_string(), serde_json::from_str(corpo.trim()).expect("JSON"))
    };

    // ---- a chave de rede é obrigatória e nunca viaja ----
    let mau = pedir_cfg(&crate::rede::Config { chave: "errada123".into(), ..cliente.clone() }, "t1", "setup.estado", Value::Null);
    assert_eq!(mau["ok"], false);
    assert!(mau["erro"].as_str().unwrap().contains("Chave de rede"), "{mau}");
    assert_eq!(como("t1", "setup.estado", Value::Null)["ok"], true, "com a chave certa, o posto volta a ligar-se");
    // um posto antigo (pedido em claro, com a chave no corpo) é recusado com uma explicação
    let (linha, antigo) = http("POST", &json!({ "chave": "segredo123", "token": "t9", "posto": posto2, "metodo": "setup.estado", "params": null }).to_string());
    assert!(linha.contains("400") && antigo["erro"].as_str().unwrap().contains("versão antiga"), "{linha} {antigo}");

    // ---- o que passa na rede vai cifrado: nem o método nem os dados se lêem ----
    let pedido = json!({ "token": "t9", "posto": posto2, "metodo": "setup.estado", "params": null, "t": crate::rede::teste::agora() });
    let (n, envelope) = crate::rede::teste::envelope(&cliente, &pedido);
    let texto = envelope.to_string();
    assert!(!texto.contains("setup") && !texto.contains("t9") && !texto.contains("segredo123"), "{texto}");
    let (_, resposta) = http("POST", &texto);
    assert!(resposta.get("ok").is_none() && resposta["c"].is_string(), "a resposta também vai cifrada: {resposta}");
    assert_eq!(crate::rede::teste::abrir_resposta(&cliente, n, &resposta).unwrap()["ok"], true);
    // a resposta só se abre com o número do pedido a que responde
    let (n2, _) = crate::rede::teste::envelope(&cliente, &pedido);
    assert!(crate::rede::teste::abrir_resposta(&cliente, n2, &resposta).is_none());
    // o mesmo pedido copiado da rede não se repete
    let (_, outra_vez) = http("POST", &texto);
    let outra_vez = crate::rede::teste::abrir_resposta(&cliente, n, &outra_vez).unwrap();
    assert_eq!(outra_vez["ok"], false);
    assert!(outra_vez["erro"].as_str().unwrap().contains("repetido"), "{outra_vez}");
    // um byte mexido e o servidor recusa
    let (_, envelope) = crate::rede::teste::envelope(&cliente, &pedido);
    let mut c = B64.decode(envelope["c"].as_str().unwrap()).unwrap();
    c[3] ^= 1;
    let (linha, mexido) = http("POST", &json!({ "v": 2, "n": envelope["n"], "c": B64.encode(&c) }).to_string());
    assert!(linha.contains("401") && mexido["erro"].as_str().unwrap().contains("adulterado"), "{linha} {mexido}");
    // um pedido antigo (hora fora dos 5 minutos) é recusado, mesmo com a cifra certa
    let velho = json!({ "token": "t9", "posto": posto2, "metodo": "setup.estado", "params": null, "t": crate::rede::teste::agora() - 600_000 });
    let (n, envelope) = crate::rede::teste::envelope(&cliente, &velho);
    let (_, r) = http("POST", &envelope.to_string());
    let r = crate::rede::teste::abrir_resposta(&cliente, n, &r).unwrap();
    assert!(r["ok"] == false && r["relogio"].is_u64(), "{r}");
    // um posto com o relógio adiantado acerta-se pelo servidor e o pedido passa
    crate::rede::teste::desacertar(3_600_000);
    assert_eq!(como("t9", "setup.estado", Value::Null)["ok"], true, "o posto acerta a hora pela do servidor");

    // ---- sem sessão não se lê nada; cada posto tem a sua ----
    let sem = como("t1", "artigos.listar", json!({}));
    assert_eq!(sem["ok"], false);
    assert!(sem["erro"].as_str().unwrap().contains("Sessão"));
    let entrada = como("t1", "auth.login", json!({ "login": "joana", "senha": "432100" }));
    assert_eq!(entrada["ok"], true);
    assert_eq!(entrada["dados"]["nome"], "Joana");
    assert_eq!(como("t1", "artigos.listar", json!({}))["dados"].as_array().unwrap().len(), 1);
    let outro = como("t2", "artigos.listar", json!({}));
    assert_eq!(outro["ok"], false, "a sessão de um posto não serve outro posto");
    // e a máquina do servidor continua sem sessão
    assert!(arc.sessao_opcional().is_none(), "o login de um posto em rede não entra na máquina do servidor");

    // ---- o posto do pedido é o que manda: série e caixa são do posto 2 ----
    assert_eq!(como("t1", "posto.obter", Value::Null)["dados"]["atual"]["descricao"], "Caixa 2");
    assert_eq!(como("t1", "caixa.abrir", json!({ "valor_inicial": 500000 }))["ok"], true);
    let venda = como("t1", "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 2 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] }));
    assert_eq!(venda["ok"], true, "{venda}");
    let numero = venda["dados"]["documento"]["numero_doc"].as_str().unwrap().to_string();
    assert!(numero.contains("P2"), "o documento tem de sair na série do posto 2: {numero}");
    assert_eq!(venda["dados"]["documento"]["posto_id"], posto2);
    assert_eq!(arc.posto().id, 1, "o posto desta máquina não muda por servir outro");

    // ---- o GET serve para confirmar que há um KanGest à escuta ----
    let mut s = std::net::TcpStream::connect(("127.0.0.1", porta)).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    assert!(r.contains("\"app\":\"kigest\""), "{r}");
    assert!(r.contains("\"sal\":") && r.contains("\"cifra\":\"aes-256-gcm\""), "{r}");

    // ---- os postos ligados aparecem no estado da rede ----
    rpc(&arc, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let estado = rpc(&arc, "rede.estado", Value::Null).await;
    assert_eq!(estado["modo"], "servidor");
    assert_eq!(estado["servidor_activo"], porta);
    let ligados = estado["ligados"].as_array().unwrap();
    assert!(ligados.iter().any(|l| l["utilizador"] == "Joana" && l["posto"] == "Caixa 2"), "{ligados:?}");
    assert!(rpc_err(&arc, "rede.testar", Value::Null).await.contains("posto cliente"));

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
fn agt_simulada() -> (u16, std::sync::Arc<std::sync::Mutex<Vec<(String, String, Value)>>>) {
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
    let ultimo = pedidos.lock().unwrap().iter().filter(|p| p.0.ends_with("/registarFactura")).last().cloned().unwrap();
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
    use std::io::{Read, Write};
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
    let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli1, "linhas": [{ "artigo_id": arts[2], "quantidade": 20 }, { "artigo_id": serv, "quantidade": 1 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli2, "data_vencimento": format!("{ano}-01-15"), "linhas": [{ "artigo_id": arts[5], "quantidade": 8 }] })).await;
    rpc(&st, "documentos.emitir", json!({ "tipo": "PP", "entidade_id": cli2, "linhas": [{ "artigo_id": serv, "quantidade": 2 }] })).await;
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
    rpc(&st, "oficina.ordemGuardar", json!({ "viatura_id": viat, "entidade_id": cli2, "avaria": "Ruído na travagem", "diagnostico": "Pastilhas gastas", "estado": "EM_CURSO", "linhas": [{ "descricao": "Substituição de pastilhas", "quantidade": 1 }] })).await;
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
    // o interface de auditoria (navegador) fala em claro: só aceite nas compilações de desenvolvimento
    let cfg = crate::rede::Config { modo: "servidor".into(), porta: 7799, chave: "auditoria123".into(), sem_cifra: true, ..Default::default() };
    crate::rede::iniciar_servidor(arc.clone(), &cfg).expect("servidor");
    let corpo = json!({ "chave": "auditoria123", "token": "auditoria", "posto": 1, "metodo": "auth.login", "params": { "login": "admin", "senha": "1234" } }).to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", 7799)).unwrap();
    s.write_all(format!("POST / HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len()).as_bytes()).unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    assert!(r.contains("\"ok\":true"), "{r}");

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
    use std::io::{Read, Write};
    let Ok(pasta) = std::env::var("KANGEST_PASTA") else { return };
    let pasta = std::path::PathBuf::from(pasta);
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    let arc = std::sync::Arc::new(st);
    crate::rede::registar_estado(arc.clone());
    // o interface de auditoria (navegador) fala em claro: só aceite nas compilações de desenvolvimento
    let cfg = crate::rede::Config { modo: "servidor".into(), porta: 7799, chave: "auditoria123".into(), sem_cifra: true, ..Default::default() };
    crate::rede::iniciar_servidor(arc.clone(), &cfg).expect("servidor");
    let corpo = json!({ "chave": "auditoria123", "token": "auditoria", "posto": 1, "metodo": "auth.login", "params": { "login": "admin", "senha": "1234" } }).to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", 7799)).unwrap();
    s.write_all(format!("POST / HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len()).as_bytes()).unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).unwrap();
    assert!(r.contains("\"ok\":true"), "{r}");
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

/// Dígito de controlo EAN-13 (para montar etiquetas da balança nos testes).
fn com_controlo(doze: &str) -> String {
    let soma: u32 = doze.bytes().enumerate().map(|(i, b)| { let d = (b - b'0') as u32; if i % 2 == 0 { d } else { d * 3 } }).sum();
    format!("{doze}{}", (10 - soma % 10) % 10)
}

/// Vários códigos de barras por artigo (embalagens) e etiquetas da balança (peso ou preço no código EAN-13).
#[tokio::test]
async fn codigos_embalagens_e_balanca() {
    let pasta = std::env::temp_dir().join(format!("kigest-codigos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Mercearia, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;

    // ---- embalagens: a caixa de 12 lê-se como 12 unidades ----
    let agua = rpc(&st, "artigos.guardar", json!({ "descricao": "Água 0,5L", "codigo_barras": "5601000000017", "taxa_iva_id": 1, "preco1": 10000,
        "codigos": [{ "codigo": "15601000000014", "quantidade": 12, "descricao": "Caixa de 12" }, { "codigo": "ALT-AGUA" }] })).await["id"].as_i64().unwrap();
    let a = rpc(&st, "artigos.obter", json!({ "id": agua })).await;
    assert_eq!(a["codigos"].as_array().unwrap().len(), 2);
    let cx = rpc(&st, "artigos.codigo", json!({ "codigo": "15601000000014" })).await;
    assert_eq!((cx["id"].as_i64(), cx["leitura"]["quantidade"].as_f64(), cx["leitura"]["origem"].as_str()), (Some(agua), Some(12.0), Some("embalagem")));
    let alt = rpc(&st, "artigos.codigo", json!({ "codigo": "ALT-AGUA" })).await;
    assert_eq!(alt["id"], agua);
    assert!(alt.get("leitura").is_none(), "outro código da mesma unidade vale 1");
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": "5601000000017" })).await.get("leitura").is_none());
    assert_eq!(rpc(&st, "artigos.listar", json!({ "q": "1560100" })).await.as_array().unwrap().len(), 1, "a pesquisa encontra pelos códigos extra");
    // um código não pode ser de dois artigos, nem repetir-se no mesmo
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Outro", "taxa_iva_id": 1, "preco1": 1, "codigos": [{ "codigo": "15601000000014" }] })).await.contains("já é do artigo"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Outro", "taxa_iva_id": 1, "preco1": 1, "codigos": [{ "codigo": "5601000000017" }] })).await.contains("já é do artigo"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Outro", "taxa_iva_id": 1, "preco1": 1, "codigos": [{ "codigo": "X1" }, { "codigo": "X1" }] })).await.contains("repetido"));
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Outro", "taxa_iva_id": 1, "preco1": 1, "codigos": [{ "codigo": "X2", "quantidade": 0 }] })).await.contains("positiva"));
    // gravar o artigo sem «codigos» não os apaga; com a lista vazia, sim
    rpc(&st, "artigos.guardar", json!({ "id": agua, "descricao": "Água 0,5 L", "codigo_barras": "5601000000017", "taxa_iva_id": 1, "preco1": 10000 })).await;
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": agua })).await["codigos"].as_array().unwrap().len(), 2);
    assert_eq!(rpc(&st, "artigos.obter", json!({ "id": agua })).await["referencia"], "A00001", "alterar sem referência mantém a que tinha");

    // ---- balança: desligada, a etiqueta não se lê ----
    let carne = rpc(&st, "artigos.guardar", json!({ "descricao": "Carne picada (kg)", "unidade_id": 2, "taxa_iva_id": 1, "preco1": 500000, "codigo_balanca": "01234" })).await["id"].as_i64().unwrap();
    let etiqueta_peso = com_controlo("200123400750"); // prefixo 20, artigo 01234, 00750 g
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": etiqueta_peso })).await.is_null());
    assert!(rpc_err(&st, "balanca.guardar", json!({ "ativo": true, "prefixos": "10", "digitos_codigo": 5, "tipo": "PESO", "decimais": 3 })).await.contains("20 a 29"));
    rpc(&st, "balanca.guardar", json!({ "ativo": true, "prefixos": "20, 21", "digitos_codigo": 5, "tipo": "PESO", "decimais": 3 })).await;
    let lido = rpc(&st, "artigos.codigo", json!({ "codigo": etiqueta_peso })).await;
    assert_eq!((lido["id"].as_i64(), lido["leitura"]["quantidade"].as_f64(), lido["leitura"]["origem"].as_str()), (Some(carne), Some(0.75), Some("balanca")), "{lido}");
    // dígito de controlo errado ou prefixo de fora: não é etiqueta
    let mut errada = etiqueta_peso.clone();
    errada.replace_range(12..13, &(((errada.as_bytes()[12] - b'0' + 1) % 10).to_string()));
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": errada })).await.is_null());
    assert!(rpc(&st, "artigos.codigo", json!({ "codigo": com_controlo("290123400750") })).await.is_null());
    // o código da balança é único
    assert!(rpc_err(&st, "artigos.guardar", json!({ "descricao": "Frango (kg)", "taxa_iva_id": 1, "preco1": 1, "codigo_balanca": "1234" })).await.contains("já é do artigo"));
    // venda com a quantidade lida: 0,750 kg × 5 000,00 = 3 750,00 + IVA
    let fr = rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": carne, "quantidade": 10, "preco_unitario": 300000 }] })).await;
    assert!(fr["documento"]["id"].as_i64().is_some());
    rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
    let v = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": carne, "quantidade": lido["leitura"]["quantidade"] }], "pagamentos": [{ "modo": "NU", "valor": 427500 }] })).await;
    assert_eq!(v["documento"]["total"], 427500);

    // ---- etiquetas com o preço (Kz, sem casas): a quantidade é a que dá esse preço ----
    rpc(&st, "balanca.guardar", json!({ "ativo": true, "prefixos": "20", "digitos_codigo": 5, "tipo": "PRECO", "decimais": 0 })).await;
    let etiqueta_preco = com_controlo("200123402850"); // 2 850 Kz com IVA = 0,5 kg a 5 700 Kz/kg (5 000 + 14%)
    let lido = rpc(&st, "artigos.codigo", json!({ "codigo": etiqueta_preco })).await;
    assert_eq!((lido["leitura"]["quantidade"].as_f64(), lido["leitura"]["preco_etiqueta"].as_i64()), (Some(0.5), Some(285000)), "{lido}");
    let cfg = rpc(&st, "balanca.obter", Value::Null).await;
    assert_eq!((cfg["tipo"].as_str(), cfg["prefixos"].as_str()), (Some("PRECO"), Some("20")));

    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Receptor HTTP de avisos para os testes: guarda (caminho, cabeçalhos, corpo); responde 500 em /falha e 200 no resto.
fn receptor_webhooks() -> (u16, std::sync::Arc<std::sync::Mutex<Vec<(String, String, String)>>>) {
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

/// Cofre: os segredos da base (chave AGT, Facturação Electrónica, avisos automáticos) ficam cifrados e continuam a servir.
#[tokio::test]
async fn cofre_segredos_cifrados() {
    use crate::cofre;
    let pasta = std::env::temp_dir().join(format!("kigest-teste-cofre-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
    let mut st = AppState::new(pool);
    st.pasta_dados = pasta.clone();
    st.pasta_exportacao = pasta.clone();
    st.carregar_posto(1).await.unwrap();
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Cofre, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let bruto = |chave: &'static str| {
        let pool = st.pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT valor FROM config WHERE chave = ?").bind(chave).fetch_one(&pool).await.unwrap() }
    };

    // a chave de assinatura AGT, gerada ao criar a empresa, está cifrada na base
    let guardada = bruto("agt.chave_privada").await;
    assert!(guardada.starts_with("KGC1.") && !guardada.contains("PRIVATE KEY"), "{guardada}");
    // e continua a assinar os documentos
    let artigo = rpc(&st, "artigos.guardar", json!({ "descricao": "Serviço", "tipo": "S", "taxa_iva_id": 1, "preco1": 100000 })).await["id"].as_i64().unwrap();
    let venda = rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;
    let hash = venda["documento"]["hash"].as_str().unwrap_or_default().to_string();
    assert!(hash.len() > 100, "documento assinado: {venda}");

    // uma base de uma versão anterior (chave em texto) lê-se na mesma e é cifrada no arranque
    let pem = cofre::ler_config(&st.pool, "agt.chave_privada").await.unwrap().unwrap();
    assert!(pem.contains("BEGIN PRIVATE KEY"));
    sqlx::query("UPDATE config SET valor = ? WHERE chave = 'agt.chave_privada'").bind(&pem).execute(&st.pool).await.unwrap();
    sqlx::query("INSERT INTO config (chave, valor) VALUES ('fe.senha', 'senha-antiga') ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor").execute(&st.pool).await.unwrap();
    assert_eq!(cofre::ler_config(&st.pool, "agt.chave_privada").await.unwrap().unwrap(), pem);
    assert_eq!(cofre::migrar(&st.pool).await.unwrap(), 2);
    assert_eq!(cofre::migrar(&st.pool).await.unwrap(), 0, "só uma vez");
    assert!(bruto("agt.chave_privada").await.starts_with("KGC1."));
    assert!(bruto("fe.senha").await.starts_with("KGC1."));
    assert_eq!(cofre::ler_config(&st.pool, "fe.senha").await.unwrap().unwrap(), "senha-antiga");
    rpc(&st, "documentos.emitir", json!({ "tipo": "FR", "linhas": [{ "artigo_id": artigo, "quantidade": 1 }], "pagamentos": [{ "modo": "NU", "valor": 114000 }] })).await;

    // cada valor só abre no sítio onde foi guardado: a chave AGT copiada para a senha da FE não se lê
    sqlx::query("UPDATE config SET valor = (SELECT valor FROM config WHERE chave = 'agt.chave_privada') WHERE chave = 'fe.senha'").execute(&st.pool).await.unwrap();
    let erro = cofre::ler_config(&st.pool, "fe.senha").await.unwrap_err().to_string();
    assert!(erro.contains("não abre"), "{erro}");
    // e a base de outra empresa (outro sal) também não abre os segredos desta
    let cifrado = cofre::para_config(&st.pool, "fe.senha", "123").await.unwrap();
    sqlx::query("UPDATE config SET valor = 'outro-sal' WHERE chave = 'cofre.sal'").execute(&st.pool).await.unwrap();
    assert!(cofre::decifrar(&st.pool, "config:fe.senha", &cifrado).await.is_err());

    // avisos automáticos: o segredo vai cifrado na base e aparece em claro só a quem gere a API
    let st2 = {
        let pool = db::abrir(&pasta.join("outra.db")).await.unwrap();
        let mut s = AppState::new(pool);
        s.pasta_dados = pasta.clone();
        s.carregar_posto(1).await.unwrap();
        s
    };
    rpc(&st2, "setup.inicializar", json!({ "empresa": { "nome": "Avisos, Lda", "nif": "5000000001" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st2, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let w = rpc(&st2, "api.webhookGuardar", json!({ "nome": "Loja", "url": "https://loja.exemplo/avisos", "eventos": ["documento.emitido"] })).await;
    let segredo = w["segredo"].as_str().unwrap().to_string();
    let na_base: String = sqlx::query_scalar("SELECT segredo FROM api_webhooks WHERE id = ?").bind(w["id"].as_i64()).fetch_one(&st2.pool).await.unwrap();
    assert!(na_base.starts_with("KGC1.") && !na_base.contains(&segredo));
    let lista = rpc(&st2, "api.webhooks", Value::Null).await;
    assert_eq!(lista["webhooks"][0]["segredo"], segredo.as_str());
    // segredo antigo, em texto: cifrado no arranque, sem mudar de valor
    sqlx::query("UPDATE api_webhooks SET segredo = 'whsec_antigo'").execute(&st2.pool).await.unwrap();
    assert_eq!(cofre::migrar(&st2.pool).await.unwrap(), 1);
    assert_eq!(rpc(&st2, "api.webhooks", Value::Null).await["webhooks"][0]["segredo"], "whsec_antigo");

    st.pool.close().await;
    st2.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

/// Cópia cifrada com senha e restauro: a cópia lista-se, abre só com a senha e a troca faz-se ao reabrir o programa,
/// guardando a base que estava em uso.
#[tokio::test]
async fn copia_cifrada_e_restauro() {
    let pasta = std::env::temp_dir().join(format!("kigest-teste-restauro-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let abrir = |pasta: std::path::PathBuf| async move {
        let pool = db::abrir(&pasta.join("kigest.db")).await.expect("abrir base");
        let mut st = AppState::new(pool);
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.join("exportados");
        std::fs::create_dir_all(&st.pasta_exportacao).unwrap();
        st.carregar_posto(1).await.unwrap();
        st
    };
    let st = abrir(pasta.clone()).await;
    rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Restauro, Lda", "nif": "5000000002" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    rpc(&st, "artigos.guardar", json!({ "descricao": "Antes da cópia", "taxa_iva_id": 1, "preco1": 1000 })).await;

    // ---- cópia cifrada ----
    assert!(rpc_err(&st, "sistema.backupCifrado", json!({ "senha": "curta" })).await.contains("8 caracteres"));
    let copia = rpc(&st, "sistema.backupCifrado", json!({ "senha": "senha da loja 2026" })).await;
    let ficheiro = copia["ficheiro"].as_str().unwrap().to_string();
    assert!(ficheiro.ends_with(".kgc"), "{copia}");
    let bytes = std::fs::read(&ficheiro).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("Restauro, Lda"), "o nome da empresa não aparece na cópia");
    assert!(std::fs::read_dir(&pasta).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with("copia-a-cifrar")), "sem ficheiros temporários");
    let lista = rpc(&st, "sistema.copiasRestaurar", Value::Null).await;
    assert!(lista["copias"].as_array().unwrap().iter().any(|c| c["caminho"] == ficheiro.as_str()), "{lista}");
    assert_eq!(lista["pendente"], Value::Null);

    // depois da cópia, mais trabalho
    rpc(&st, "artigos.guardar", json!({ "descricao": "Depois da cópia", "taxa_iva_id": 1, "preco1": 2000 })).await;

    // ---- restaurar: senha errada, ficheiro que não é do KanGest, e a cópia certa ----
    assert!(rpc_err(&st, "sistema.restaurar", json!({ "caminho": ficheiro })).await.contains("senha"));
    assert!(rpc_err(&st, "sistema.restaurar", json!({ "caminho": ficheiro, "senha": "outra senha" })).await.contains("Senha errada"));
    let estranho = pasta.join("exportados").join("kangest-estranho.db");
    std::fs::write(&estranho, b"isto nao e uma base").unwrap();
    assert!(rpc_err(&st, "sistema.restaurar", json!({ "caminho": estranho.display().to_string() })).await.contains("não é uma base"));
    let r = rpc(&st, "sistema.restaurar", json!({ "caminho": ficheiro, "senha": "senha da loja 2026" })).await;
    assert_eq!(r["copia"]["empresa"], "Restauro, Lda", "{r}");
    assert_eq!(rpc(&st, "sistema.copiasRestaurar", Value::Null).await["pendente"]["nif"], "5000000002");
    // desistir e voltar a preparar
    rpc(&st, "sistema.restaurarCancelar", Value::Null).await;
    assert_eq!(rpc(&st, "sistema.copiasRestaurar", Value::Null).await["pendente"], Value::Null);
    rpc(&st, "sistema.restaurar", json!({ "caminho": ficheiro, "senha": "senha da loja 2026" })).await;

    // ---- reabrir o programa: a troca faz-se antes de abrir a base ----
    st.pool.close().await;
    drop(st);
    let antes = super::sistema::aplicar_restauro(&pasta).await.unwrap().expect("restauro feito");
    assert!(antes.exists());
    assert!(super::sistema::aplicar_restauro(&pasta).await.unwrap().is_none(), "só uma vez");
    let st = abrir(pasta.clone()).await;
    rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
    let nomes: Vec<String> = sqlx::query_scalar("SELECT descricao FROM artigos ORDER BY id").fetch_all(&st.pool).await.unwrap();
    assert!(nomes.contains(&"Antes da cópia".to_string()) && !nomes.contains(&"Depois da cópia".to_string()), "{nomes:?}");
    // a base que estava em uso ficou guardada, com o trabalho feito depois da cópia
    let pool = db::abrir(&antes).await.unwrap();
    let guardados: Vec<String> = sqlx::query_scalar("SELECT descricao FROM artigos").fetch_all(&pool).await.unwrap();
    assert!(guardados.contains(&"Depois da cópia".to_string()), "{guardados:?}");
    pool.close().await;
    st.pool.close().await;
    let _ = std::fs::remove_dir_all(&pasta);
}

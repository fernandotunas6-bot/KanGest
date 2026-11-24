// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Qualidade dos dados: verificações que procuram o que está mal ou incompleto (NIF ou códigos repetidos, isentos sem
//! motivo, artigos sem preço, stock negativo…), com exemplos e o ecrã onde se corrige. É um registo declarativo, como
//! as fontes do construtor: cada verificação tem a sua permissão e duas consultas fixas (contar e exemplos), sem
//! parâmetros vindos do ecrã. Só lê: a correcção faz-se nos ecrãs de sempre, com as permissões e a auditoria deles.

use serde_json::{json, Value};

use crate::db;
use crate::error::AppResult;
use crate::state::AppState;

pub struct Verificacao {
    pub id: &'static str,
    pub titulo: &'static str,
    /// porque importa e como se corrige
    pub texto: &'static str,
    /// «erro» (pode dar problemas na AGT ou nas contas) ou «aviso»
    pub gravidade: &'static str,
    pub permissao: &'static str,
    /// uma linha com o número de casos
    pub contar: &'static str,
    /// até 20 casos, com as colunas «rotulo» e «detalhe»; no detalhe, `{kz:cêntimos}`, `{qtd:número}` e
    /// `{data:AAAA-MM-DD}` são valores que o ecrã mostra no formato do resto do programa (1 234,56 Kz, 31/01/2020)
    pub exemplos: &'static str,
    /// o ecrã onde se corrige
    pub corrigir: &'static str,
    /// a expressão do armazém (loja) dos casos: no lugar de `{LOJA}`, o utilizador de uma loja só conta os da dele;
    /// vazio nas verificações das fichas (artigos, clientes), que são de todas as lojas
    pub loja: &'static str,
}

pub const VERIFICACOES: &[Verificacao] = &[
    Verificacao {
        id: "isento_sem_motivo",
        titulo: "Artigos isentos de IVA sem o motivo de isenção",
        texto: "A AGT exige o motivo em cada linha isenta: facturar um destes artigos sai com um motivo genérico. Ponha o motivo certo na ficha do artigo.",
        gravidade: "erro",
        permissao: "artigos.editar",
        contar: "SELECT COUNT(*) FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id
                 WHERE a.ativo = 1 AND t.percentagem = 0 AND COALESCE(a.motivo_isencao, '') = ''",
        exemplos: "SELECT a.referencia || ' — ' || a.descricao AS rotulo, t.descricao AS detalhe FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id
                   WHERE a.ativo = 1 AND t.percentagem = 0 AND COALESCE(a.motivo_isencao, '') = '' ORDER BY a.descricao LIMIT 20",
        corrigir: "/artigos",
        loja: "",
    },
    Verificacao {
        id: "nif_repetido",
        titulo: "NIF repetido em clientes ou fornecedores",
        texto: "O mesmo contribuinte em várias fichas divide a conta corrente e o histórico. Junte-os numa ficha e desactive as outras.",
        gravidade: "erro",
        permissao: "clientes",
        contar: "SELECT COUNT(*) FROM (SELECT nif FROM entidades WHERE ativo = 1 AND nif <> '999999999' AND trim(nif) <> '' GROUP BY nif HAVING COUNT(*) > 1)",
        exemplos: "SELECT nif AS rotulo, group_concat(nome, ' · ') AS detalhe FROM entidades
                   WHERE ativo = 1 AND nif <> '999999999' AND trim(nif) <> '' GROUP BY nif HAVING COUNT(*) > 1 ORDER BY nif LIMIT 20",
        corrigir: "/clientes",
        loja: "",
    },
    Verificacao {
        id: "codigo_barras_repetido",
        titulo: "Código de barras repetido em artigos",
        texto: "Ao ler o código no POS sai sempre o mesmo artigo e o outro nunca. Deixe cada código num só artigo.",
        gravidade: "erro",
        permissao: "artigos.editar",
        contar: "SELECT COUNT(*) FROM (SELECT codigo_barras FROM artigos WHERE ativo = 1 AND trim(COALESCE(codigo_barras, '')) <> ''
                 GROUP BY codigo_barras HAVING COUNT(*) > 1)",
        exemplos: "SELECT codigo_barras AS rotulo, group_concat(referencia || ' ' || descricao, ' · ') AS detalhe FROM artigos
                   WHERE ativo = 1 AND trim(COALESCE(codigo_barras, '')) <> '' GROUP BY codigo_barras HAVING COUNT(*) > 1 ORDER BY codigo_barras LIMIT 20",
        corrigir: "/artigos",
        loja: "",
    },
    Verificacao {
        id: "artigo_sem_preco",
        titulo: "Artigos sem preço de venda",
        texto: "Vendem-se a zero se ninguém reparar. Ponha o preço na ficha, ou desactive os que já não se vendem.",
        gravidade: "aviso",
        permissao: "artigos.editar",
        contar: "SELECT COUNT(*) FROM artigos WHERE ativo = 1 AND composto = 0 AND preco1 = 0 AND COALESCE(pvp1, 0) = 0",
        exemplos: "SELECT referencia || ' — ' || descricao AS rotulo, CASE tipo WHEN 'S' THEN 'Serviço' ELSE 'Produto' END AS detalhe
                   FROM artigos WHERE ativo = 1 AND composto = 0 AND preco1 = 0 AND COALESCE(pvp1, 0) = 0 ORDER BY descricao LIMIT 20",
        corrigir: "/artigos",
        loja: "",
    },
    Verificacao {
        id: "preco_abaixo_custo",
        titulo: "Artigos com o preço abaixo do custo",
        texto: "O preço sem IVA é mais baixo do que o custo médio: cada venda dá prejuízo. Reveja o preço ou o custo.",
        gravidade: "aviso",
        permissao: "artigos.editar",
        contar: "SELECT COUNT(*) FROM artigos WHERE ativo = 1 AND preco1 > 0 AND custo_medio > preco1",
        exemplos: "SELECT referencia || ' — ' || descricao AS rotulo, 'preço {kz:' || preco1 || '} · custo médio {kz:' || custo_medio || '}' AS detalhe
                   FROM artigos WHERE ativo = 1 AND preco1 > 0 AND custo_medio > preco1 ORDER BY custo_medio - preco1 DESC LIMIT 20",
        corrigir: "/artigos",
        loja: "",
    },
    Verificacao {
        id: "stock_negativo",
        titulo: "Stock negativo",
        texto: "Saiu mais do que entrou: falta registar uma compra ou uma entrada, ou acertar a contagem no inventário.",
        gravidade: "aviso",
        permissao: "stock.inventario",
        contar: "SELECT COUNT(*) FROM stock s JOIN artigos a ON a.id = s.artigo_id WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade < 0{LOJA}",
        exemplos: "SELECT a.referencia || ' — ' || a.descricao AS rotulo, ar.nome || ': {qtd:' || s.quantidade || '}' AS detalhe
                   FROM stock s JOIN artigos a ON a.id = s.artigo_id JOIN armazens ar ON ar.id = s.armazem_id
                   WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade < 0{LOJA} ORDER BY s.quantidade LIMIT 20",
        corrigir: "/inventario",
        loja: "s.armazem_id",
    },
    Verificacao {
        id: "stock_sem_custo",
        titulo: "Stock sem custo",
        texto: "Há existências com o custo médio a zero: o valor do stock e o lucro ficam errados. Registe a compra com o preço, ou ponha o custo na ficha.",
        gravidade: "aviso",
        permissao: "stock.inventario",
        contar: "SELECT COUNT(*) FROM stock s JOIN artigos a ON a.id = s.artigo_id WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade > 0 AND a.custo_medio = 0{LOJA}",
        exemplos: "SELECT a.referencia || ' — ' || a.descricao AS rotulo, ar.nome || ': {qtd:' || s.quantidade || '}' AS detalhe
                   FROM stock s JOIN artigos a ON a.id = s.artigo_id JOIN armazens ar ON ar.id = s.armazem_id
                   WHERE a.ativo = 1 AND a.controla_stock = 1 AND s.quantidade > 0 AND a.custo_medio = 0{LOJA} ORDER BY a.descricao LIMIT 20",
        corrigir: "/artigos",
        loja: "s.armazem_id",
    },
    Verificacao {
        id: "vencidos_por_receber",
        titulo: "Documentos vencidos por receber",
        texto: "Facturas com a data de vencimento passada e ainda em dívida. Envie o aviso de vencimento ou registe o recibo.",
        gravidade: "aviso",
        permissao: "clientes",
        contar: "SELECT COUNT(*) FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                 WHERE t.grupo = 'VENDAS' AND d.estado = 'N' AND d.valor_pendente > 0 AND d.data_vencimento < date('now', 'localtime'){LOJA}",
        exemplos: "SELECT d.numero_doc || ' — ' || d.entidade_nome AS rotulo,
                          '{kz:' || d.valor_pendente || '} · venceu a {data:' || substr(d.data_vencimento, 1, 10) || '}' AS detalhe
                   FROM documentos d JOIN tipos_documento t ON t.codigo = d.tipo_codigo
                   WHERE t.grupo = 'VENDAS' AND d.estado = 'N' AND d.valor_pendente > 0 AND d.data_vencimento < date('now', 'localtime'){LOJA}
                   ORDER BY d.data_vencimento LIMIT 20",
        corrigir: "/avisos",
        loja: "d.armazem_id",
    },
];

/// Corre as verificações que este utilizador pode ver: o número de casos e até 20 exemplos de cada uma.
pub async fn verificar(st: &AppState) -> AppResult<Value> {
    let s = st.sessao()?;
    let mut out = Vec::new();
    for v in VERIFICACOES.iter().filter(|v| s.tem(v.permissao)) {
        // o utilizador de uma loja só conta os casos da dele (o id vem da sessão: é um inteiro, não texto do pedido)
        let loja = match st.loja() {
            Some(a) if !v.loja.is_empty() => format!(" AND {} = {a}", v.loja),
            _ => String::new(),
        };
        let n: i64 = sqlx::query_scalar(&v.contar.replace("{LOJA}", &loja)).fetch_one(&st.pool).await?;
        let exemplos = if n > 0 { db::query_json(&st.pool, &v.exemplos.replace("{LOJA}", &loja)).await? } else { json!([]) };
        out.push(json!({
            "id": v.id, "titulo": v.titulo, "texto": v.texto, "gravidade": v.gravidade,
            "casos": n, "exemplos": exemplos, "corrigir": v.corrigir,
        }));
    }
    Ok(Value::Array(out))
}

#[cfg(test)]
mod testes {
    use serde_json::{json, Value};

    use super::VERIFICACOES;
    use crate::db;
    use crate::state::AppState;

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }

    #[test]
    fn registo_coerente() {
        let mut ids = std::collections::HashSet::new();
        for v in VERIFICACOES {
            assert!(ids.insert(v.id), "verificação repetida: {}", v.id);
            assert!(crate::state::PERMISSOES.contains(&v.permissao), "{}: permissão desconhecida", v.id);
            assert!(matches!(v.gravidade, "erro" | "aviso"), "{}", v.id);
            assert!(v.corrigir.starts_with('/'), "{}", v.id);
            // com loja, as duas consultas têm o lugar dela; sem loja, nenhuma
            let tem = !v.loja.is_empty();
            assert_eq!((v.contar.contains("{LOJA}"), v.exemplos.contains("{LOJA}")), (tem, tem), "{}", v.id);
        }
    }

    /// Cada verificação encontra o que procura (dados postos à mão, como viriam de uma importação) e o operador de caixa
    /// só vê as que a permissão deixa.
    #[tokio::test]
    async fn encontra_os_problemas() {
        let pasta = std::env::temp_dir().join(format!("kangest-qualidade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let casos = |r: &Value, id: &str| r.as_array().unwrap().iter().find(|v| v["id"] == id).map(|v| v["casos"].as_i64().unwrap()).unwrap_or(-1);

        // base limpa: nada a apontar
        let r = rpc(&st, "dados.verificar", Value::Null).await;
        assert_eq!(r.as_array().unwrap().len(), VERIFICACOES.len());
        assert!(r.as_array().unwrap().iter().all(|v| v["casos"] == 0), "{r}");

        // como viria de uma importação antiga: NIF e código de barras repetidos, isento sem motivo, sem preço,
        // abaixo do custo, stock negativo e sem custo, factura vencida
        for (nome, nif) in [("Mercearia A", "5417000111"), ("Mercearia A, Lda", "5417000111")] {
            sqlx::query("INSERT INTO entidades (tipo, nome, nif) VALUES ('C', ?, ?)").bind(nome).bind(nif).execute(&st.pool).await.unwrap();
        }
        let arroz = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz", "codigo_barras": "560001", "taxa_iva_id": 1, "preco1": 100000, "custo": 60000 })).await["id"].as_i64().unwrap();
        sqlx::query("INSERT INTO artigos (referencia, codigo_barras, descricao, taxa_iva_id, preco1) VALUES ('X1', '560001', 'Arroz (cópia)', 1, 100000)").execute(&st.pool).await.unwrap();
        let isento: i64 = sqlx::query_scalar("SELECT id FROM taxas_iva WHERE percentagem = 0 LIMIT 1").fetch_one(&st.pool).await.unwrap();
        sqlx::query("INSERT INTO artigos (referencia, descricao, taxa_iva_id, preco1) VALUES ('LIV', 'Livro', ?, 200000)").bind(isento).execute(&st.pool).await.unwrap();
        sqlx::query("INSERT INTO artigos (referencia, descricao, taxa_iva_id, preco1) VALUES ('SP', 'Sem preço', 1, 0)").execute(&st.pool).await.unwrap();
        sqlx::query("UPDATE artigos SET custo_medio = 150000 WHERE id = ?").bind(arroz).execute(&st.pool).await.unwrap();
        sqlx::query("INSERT INTO stock (artigo_id, armazem_id, quantidade) VALUES (?, 1, -3)").bind(arroz).execute(&st.pool).await.unwrap();
        sqlx::query("INSERT INTO stock (artigo_id, armazem_id, quantidade) SELECT id, 1, 5 FROM artigos WHERE referencia = 'X1'").execute(&st.pool).await.unwrap();
        let cli: i64 = sqlx::query_scalar("SELECT id FROM entidades WHERE nome = 'Mercearia A'").fetch_one(&st.pool).await.unwrap();
        sqlx::query("UPDATE artigos SET permite_negativo = 1 WHERE id = ?").bind(arroz).execute(&st.pool).await.unwrap();
        rpc(&st, "parametros.opcoesGuardar", json!({ "sem_stock": "artigo" })).await;
        let ft = rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": arroz, "quantidade": 1 }] })).await;
        sqlx::query("UPDATE documentos SET data_vencimento = '2020-01-31' WHERE id = ?").bind(ft["documento"]["id"].as_i64()).execute(&st.pool).await.unwrap();

        let r = rpc(&st, "dados.verificar", Value::Null).await;
        assert_eq!(casos(&r, "nif_repetido"), 1, "{r}");
        assert_eq!(casos(&r, "codigo_barras_repetido"), 1);
        assert_eq!(casos(&r, "isento_sem_motivo"), 1);
        assert_eq!(casos(&r, "artigo_sem_preco"), 1);
        assert_eq!(casos(&r, "preco_abaixo_custo"), 1);
        assert_eq!(casos(&r, "stock_negativo"), 1, "a FT deixou o arroz em −4");
        assert_eq!(casos(&r, "stock_sem_custo"), 1, "a cópia do arroz tem stock e custo zero");
        assert_eq!(casos(&r, "vencidos_por_receber"), 1);
        let nif = r.as_array().unwrap().iter().find(|v| v["id"] == "nif_repetido").unwrap();
        assert_eq!(nif["exemplos"][0]["rotulo"], "5417000111");
        assert!(nif["exemplos"][0]["detalhe"].as_str().unwrap().contains("Mercearia A, Lda"), "{nif}");
        let detalhe = |id: &str| r.as_array().unwrap().iter().find(|v| v["id"] == id).unwrap()["exemplos"][0]["detalhe"].as_str().unwrap().to_string();
        assert_eq!(detalhe("preco_abaixo_custo"), "preço {kz:100000} · custo médio {kz:150000}");
        assert!(detalhe("stock_negativo").ends_with(": {qtd:-4.0}"), "{}", detalhe("stock_negativo"));
        assert!(detalhe("vencidos_por_receber").ends_with(" · venceu a {data:2020-01-31}"), "{}", detalhe("vencidos_por_receber"));

        // o operador de caixa só vê as verificações das suas permissões (clientes)
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "caixa1", "perfil": "CAIXA", "senha": "caixa123", "mudar_senha": false })).await;
        rpc(&st, "auth.login", json!({ "login": "caixa1", "senha": "caixa123" })).await;
        let r = rpc(&st, "dados.verificar", Value::Null).await;
        let ids: Vec<&str> = r.as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["nif_repetido", "vencidos_por_receber"], "{r}");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

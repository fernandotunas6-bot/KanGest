// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Regras de aprovação (decisão do dono, 2026-11-24). Quatro regras, cada uma ligada à parte pelo administrador, com
//! o valor a partir do qual se aplica:
//!
//! - **anular**: anular documentos (opcionalmente, só os de dias anteriores);
//! - **saidas**: saídas de tesouraria, pagamentos a fornecedores e devoluções de dinheiro a clientes;
//! - **vales**: vales de caixa (saídas de dinheiro da gaveta do POS);
//! - **creditos**: notas de crédito e regularizações de conta corrente.
//!
//! Quando uma regra se aplica, a operação pede o PIN de quem tenha a permissão de aprovar dessa regra
//! (`aprovar.*`). Pode ser o próprio utilizador, se a tiver. O ecrã reconhece o pedido pela frase [`PEDE_PIN`], pede o
//! PIN e repete a operação com ele (`autorizacao_pin`). Cada aprovação fica em `aprovacoes`, na mesma transacção da
//! operação: quem pediu, quem aprovou, o quê e quanto.
//!
//! As regras chegam desligadas: até o administrador as ligar, tudo funciona como antes. Os pedidos da API de integração
//! não passam por elas (não há quem escreva um PIN; as chaves têm as suas próprias permissões).

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// A frase que o ecrã reconhece para pedir o PIN e repetir a operação com ele (src/lib/api.ts).
pub const PEDE_PIN: &str = "precisa da aprovação de um supervisor (PIN)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Regra {
    Anular,
    Saidas,
    Vales,
    Creditos,
}

const REGRAS: [Regra; 4] = [Regra::Anular, Regra::Saidas, Regra::Vales, Regra::Creditos];

impl Regra {
    pub fn codigo(self) -> &'static str {
        match self {
            Regra::Anular => "anular",
            Regra::Saidas => "saidas",
            Regra::Vales => "vales",
            Regra::Creditos => "creditos",
        }
    }

    /// A permissão de quem pode aprovar com o PIN.
    pub fn permissao(self) -> &'static str {
        match self {
            Regra::Anular => "aprovar.anulacao",
            Regra::Saidas => "aprovar.saidas",
            Regra::Vales => "aprovar.vales",
            Regra::Creditos => "aprovar.creditos",
        }
    }

    fn de(codigo: &str) -> Option<Regra> {
        REGRAS.into_iter().find(|r| r.codigo() == codigo)
    }
}

/// Uma aprovação dada com o PIN, por gravar na transacção da operação ([`registar`]).
#[derive(Debug)]
pub struct Aprovacao {
    regra: Regra,
    operacao: String,
    valor: i64,
    pedido_por: Option<i64>,
    pub aprovado_por: i64,
}

/// Se a regra estiver ligada e se aplicar a esta operação, exige o PIN de quem tenha a permissão de aprovar dela.
/// `operacao` diz o que se pede, em português (aparece no pedido de PIN e no registo). `data` é a do documento, para
/// a opção «só os de dias anteriores». Devolve a aprovação a gravar com [`registar`], ou `None` se a regra não se
/// aplica.
pub async fn exigir(st: &AppState, regra: Regra, valor: i64, data: Option<&str>, operacao: impl Into<String>, pin: Option<&str>) -> AppResult<Option<Aprovacao>> {
    if crate::state::contexto().is_some_and(|c| c.api_chave.is_some()) {
        return Ok(None);
    }
    let r: Option<(i64, i64, i64)> = sqlx::query_as("SELECT ativo, valor_minimo, so_dias_anteriores FROM regras_aprovacao WHERE regra = ?")
        .bind(regra.codigo())
        .fetch_optional(&st.pool)
        .await?;
    let Some((ativo, minimo, so_anteriores)) = r else { return Ok(None) };
    let valor = valor.abs();
    if ativo == 0 || valor < minimo {
        return Ok(None);
    }
    if regra == Regra::Anular && so_anteriores == 1 && data.is_some_and(|d| d >= db::hoje().as_str()) {
        return Ok(None);
    }
    let operacao = operacao.into();
    let Some(pin) = pin.map(str::trim).filter(|p| !p.is_empty()) else {
        return Err(AppError::msg(format!("{operacao}: {PEDE_PIN}.")));
    };
    match super::auth::supervisor_por_pin(st, pin, &[regra.permissao()], 0.0).await? {
        Some(s) => Ok(Some(Aprovacao { regra, operacao, valor, pedido_por: st.sessao_opcional().map(|s| s.id), aprovado_por: s.id })),
        None => Err(AppError::msg("PIN de autorização inválido.")),
    }
}

/// Grava a aprovação (se a houve) na transacção da operação, com o documento ou o movimento que ela fez.
pub async fn registar(con: &mut SqliteConnection, a: Option<&Aprovacao>, referencia: &str) -> AppResult<()> {
    let Some(a) = a else { return Ok(()) };
    sqlx::query("INSERT INTO aprovacoes (regra, operacao, referencia, valor, pedido_por, aprovado_por, criado_em) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(a.regra.codigo())
        .bind(&a.operacao)
        .bind(referencia)
        .bind(a.valor)
        .bind(a.pedido_por)
        .bind(a.aprovado_por)
        .bind(db::agora())
        .execute(&mut *con)
        .await?;
    Ok(())
}

/// Quem pode aprovar uma regra: os utilizadores activos com a permissão dela (e se já têm PIN).
async fn aprovadores(st: &AppState) -> AppResult<Vec<(String, bool, Vec<String>)>> {
    let rows = sqlx::query("SELECT nome, perfil, permissoes, pin_hash IS NOT NULL AS tem_pin FROM utilizadores WHERE ativo = 1 ORDER BY nome")
        .fetch_all(&st.pool)
        .await?;
    let mut v = Vec::new();
    for r in rows {
        let perfil: String = r.get("perfil");
        let proprias: Option<String> = r.get("permissoes");
        let (_, lista) = crate::state::permissoes_efectivas(&st.pool, &perfil, proprias.as_deref()).await;
        v.push((r.get("nome"), r.get::<i64, _>("tem_pin") == 1, lista));
    }
    Ok(v)
}

/// As quatro regras, com quem pode aprovar cada uma.
pub async fn regras(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    let rows = sqlx::query("SELECT regra, ativo, valor_minimo, so_dias_anteriores FROM regras_aprovacao").fetch_all(&st.pool).await?;
    let pessoas = aprovadores(st).await?;
    let mut v = Vec::new();
    for regra in REGRAS {
        let Some(r) = rows.iter().find(|r| r.get::<String, _>("regra") == regra.codigo()) else { continue };
        let podem: Vec<Value> = pessoas
            .iter()
            .filter(|p| p.2.iter().any(|x| x == regra.permissao()))
            .map(|p| json!({ "nome": p.0, "tem_pin": p.1 }))
            .collect();
        v.push(json!({
            "regra": regra.codigo(),
            "permissao": regra.permissao(),
            "ativo": r.get::<i64, _>("ativo") == 1,
            "valor_minimo": r.get::<i64, _>("valor_minimo"),
            "so_dias_anteriores": r.get::<i64, _>("so_dias_anteriores") == 1,
            "aprovadores": podem,
        }));
    }
    Ok(Value::Array(v))
}

#[derive(Deserialize)]
pub struct RegraIn {
    regra: String,
    ativo: bool,
    #[serde(default)]
    valor_minimo: i64,
    #[serde(default)]
    so_dias_anteriores: bool,
}

/// Liga, desliga ou muda uma regra. Só se liga se alguém a puder aprovar (com a permissão e um PIN), senão a
/// operação ficaria impossível.
pub async fn guardar(st: &AppState, r: RegraIn) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    let regra = Regra::de(&r.regra).ok_or_else(|| AppError::msg("Regra desconhecida."))?;
    if r.valor_minimo < 0 {
        return Err(AppError::msg("O valor não pode ser negativo."));
    }
    if r.ativo {
        let pessoas = aprovadores(st).await?;
        if !pessoas.iter().any(|p| p.1 && p.2.iter().any(|x| x == regra.permissao())) {
            return Err(AppError::msg(
                "Ninguém poderia aprovar: dê um PIN (em Utilizadores e Permissões) a quem tem a permissão de aprovar desta regra, e depois ligue-a.",
            ));
        }
    }
    sqlx::query("UPDATE regras_aprovacao SET ativo = ?, valor_minimo = ?, so_dias_anteriores = ? WHERE regra = ?")
        .bind(r.ativo)
        .bind(r.valor_minimo)
        .bind(regra == Regra::Anular && r.so_dias_anteriores)
        .bind(regra.codigo())
        .execute(&st.pool)
        .await?;
    Ok(json!({ "regra": regra.codigo() }))
}

#[derive(Deserialize, Default)]
pub struct ListarIn {
    #[serde(default)]
    de: Option<String>,
    #[serde(default)]
    ate: Option<String>,
}

/// As aprovações dadas (as mais recentes primeiro).
pub async fn listar(st: &AppState, f: ListarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.utilizadores")?;
    let rows = sqlx::query(
        "SELECT a.id, a.regra, a.operacao, a.referencia, a.valor, a.criado_em, up.nome AS pedido_por, ua.nome AS aprovado_por
         FROM aprovacoes a LEFT JOIN utilizadores up ON up.id = a.pedido_por LEFT JOIN utilizadores ua ON ua.id = a.aprovado_por
         WHERE (?1 IS NULL OR substr(a.criado_em, 1, 10) >= ?1) AND (?2 IS NULL OR substr(a.criado_em, 1, 10) <= ?2)
         ORDER BY a.id DESC LIMIT 500",
    )
    .bind(f.de.filter(|d| d.len() == 10))
    .bind(f.ate.filter(|d| d.len() == 10))
    .fetch_all(&st.pool)
    .await?;
    Ok(db::rows_json(&rows))
}

#[cfg(test)]
mod testes {
    use super::*;

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
    fn com_pin(mut p: Value, pin: &str) -> Value {
        p["autorizacao_pin"] = json!(pin);
        p
    }

    /// Desligadas, nada muda. Ligadas: o operador sem a permissão de aprovar precisa do PIN de quem a tenha (o PIN
    /// errado e o de quem não a tem são recusados); quem a tem aprova com o próprio PIN; abaixo do valor mínimo e nas
    /// entradas não se pede nada; a anulação «só de dias anteriores» deixa passar as de hoje; cada aprovação fica
    /// registada com quem pediu e quem aprovou; não se liga uma regra que ninguém possa aprovar.
    #[tokio::test]
    async fn regras_de_aprovacao() {
        let pasta = std::env::temp_dir().join(format!("kangest-aprovacoes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let regras = rpc(&st, "aprovacoes.regras", Value::Null).await;
        assert!(regras.as_array().unwrap().iter().all(|r| r["ativo"] == false), "chegam desligadas: {regras}");
        // ninguém tem PIN: não se liga
        assert!(erro(&st, "aprovacoes.guardar", json!({ "regra": "anular", "ativo": true })).await.contains("Ninguém poderia aprovar"));
        rpc(&st, "utilizadores.guardar", json!({ "id": 1, "nome": "Admin", "login": "admin", "perfil": "ADMIN", "pin": "4321" })).await;
        // um operador que faz as operações mas não aprova nenhuma, e um gestor (aprova todas, pelo perfil)
        let operacoes = ["vendas.fatura_recibo", "vendas.nota_credito", "documentos.anular", "tesouraria.fluxo", "tesouraria.recibo", "tesouraria.regularizar", "caixa.movimentos"];
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Operador", "login": "op", "perfil": "CAIXA", "senha": "op12345", "mudar_senha": false, "pin": "1111", "permissoes": operacoes })).await;
        rpc(&st, "utilizadores.guardar", json!({ "nome": "Gestora", "login": "gestora", "perfil": "GESTOR", "senha": "ge12345", "mudar_senha": false, "pin": "2222" })).await;

        // o que se vai anular, regularizar e creditar
        let art = rpc(&st, "artigos.guardar", json!({ "descricao": "Arroz 5kg", "taxa_iva_id": 1, "preco1": 100000, "custo": 50000 })).await["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "ES", "linhas": [{ "artigo_id": art, "quantidade": 20, "preco_unitario": 50000 }] })).await;
        let cli = rpc(&st, "entidades.guardar", json!({ "nome": "Cliente A", "nif": "5000000001" })).await["id"].as_i64().unwrap();
        rpc(&st, "caixa.abrir", json!({ "valor_inicial": 0 })).await;
        let venda = |n: i64| json!({ "tipo": "FR", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": n }], "pagamentos": [{ "modo": "NU", "valor": 114000 * n }] });
        let fr1 = rpc(&st, "documentos.emitir", venda(1)).await["documento"]["id"].as_i64().unwrap();
        let fr2 = rpc(&st, "documentos.emitir", venda(1)).await["documento"]["id"].as_i64().unwrap();
        let fr3 = rpc(&st, "documentos.emitir", venda(2)).await["documento"]["id"].as_i64().unwrap();
        let fr4 = rpc(&st, "documentos.emitir", venda(1)).await["documento"]["id"].as_i64().unwrap();
        rpc(&st, "documentos.emitir", json!({ "tipo": "FT", "entidade_id": cli, "linhas": [{ "artigo_id": art, "quantidade": 1 }] })).await;
        let banco: i64 = sqlx::query_scalar("SELECT id FROM contas ORDER BY id LIMIT 1").fetch_one(&st.pool).await.unwrap();

        // desligadas: o operador anula sem PIN
        rpc(&st, "auth.login", json!({ "login": "op", "senha": "op12345" })).await;
        rpc(&st, "documentos.anular", json!({ "id": fr1, "motivo": "venda em duplicado" })).await;
        assert!(erro(&st, "aprovacoes.guardar", json!({ "regra": "anular", "ativo": true })).await.contains("permissão"), "o operador não mexe nas regras");

        // o administrador liga as quatro (saídas e vales a partir de 1 000,00 Kz; créditos de qualquer valor)
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "anular", "ativo": true })).await;
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "saidas", "ativo": true, "valor_minimo": 100_000 })).await;
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "vales", "ativo": true, "valor_minimo": 100_000 })).await;
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "creditos", "ativo": true })).await;
        let regras = rpc(&st, "aprovacoes.regras", Value::Null).await;
        let anular = regras.as_array().unwrap().iter().find(|r| r["regra"] == "anular").unwrap().clone();
        let nomes: Vec<&str> = anular["aprovadores"].as_array().unwrap().iter().map(|a| a["nome"].as_str().unwrap()).collect();
        assert_eq!(nomes, ["Admin", "Gestora"], "o operador não aprova: {anular}");

        // anular: sem PIN pede-o; PIN errado e PIN de quem não aprova são recusados; o do administrador aprova
        rpc(&st, "auth.login", json!({ "login": "op", "senha": "op12345" })).await;
        let anular2 = json!({ "id": fr2, "motivo": "cliente desistiu" });
        let e = erro(&st, "documentos.anular", anular2.clone()).await;
        assert!(e.contains(PEDE_PIN) && e.contains("FR "), "{e}");
        assert!(erro(&st, "documentos.anular", com_pin(anular2.clone(), "9999")).await.contains("PIN de autorização inválido"));
        assert!(erro(&st, "documentos.anular", com_pin(anular2.clone(), "1111")).await.contains("não tem permissão"));
        rpc(&st, "documentos.anular", com_pin(anular2, "4321")).await;

        // saídas: abaixo do mínimo e entradas passam; acima pede o PIN
        let mov = |tipo: &str, valor: i64| json!({ "tipo": tipo, "conta_id": banco, "valor": valor, "descricao": "Despesa" });
        rpc(&st, "tesouraria.movimento", mov("SAIDA", 50_000)).await;
        rpc(&st, "tesouraria.movimento", mov("ENTRADA", 500_000)).await;
        assert!(erro(&st, "tesouraria.movimento", mov("SAIDA", 150_000)).await.contains(PEDE_PIN));
        rpc(&st, "tesouraria.movimento", com_pin(mov("SAIDA", 150_000), "2222")).await;

        // vales de caixa
        let vale = json!({ "tipo": "SAIDA", "valor": 120_000, "descricao": "Combustível", "beneficiario": "Motorista" });
        assert!(erro(&st, "caixa.movimento", vale.clone()).await.contains(PEDE_PIN));
        rpc(&st, "caixa.movimento", com_pin(vale, "2222")).await;
        rpc(&st, "caixa.movimento", json!({ "tipo": "SAIDA", "valor": 20_000, "descricao": "Água" })).await;

        // créditos: a nota de crédito e a regularização
        let nc = json!({ "tipo": "NC", "entidade_id": cli, "doc_origem_id": fr3, "motivo": "Devolução de 1 unidade", "linhas": [{ "artigo_id": art, "quantidade": 1 }] });
        assert!(erro(&st, "documentos.emitir", nc.clone()).await.contains(PEDE_PIN));
        let nc = rpc(&st, "documentos.emitir", com_pin(nc, "4321")).await;
        assert_eq!(nc["documento"]["autorizado_por_nome"], "Admin", "o documento mostra quem aprovou");
        let reg = json!({ "entidade_id": cli, "tipo": "CREDITO", "valor": 4000, "justificacao": "Arredondamento acordado" });
        assert!(erro(&st, "contas.regularizar", reg.clone()).await.contains(PEDE_PIN));
        rpc(&st, "contas.regularizar", com_pin(reg, "4321")).await;

        // a gestora aprova com o próprio PIN
        rpc(&st, "auth.login", json!({ "login": "gestora", "senha": "ge12345" })).await;
        assert!(erro(&st, "tesouraria.movimento", mov("SAIDA", 300_000)).await.contains(PEDE_PIN));
        rpc(&st, "tesouraria.movimento", com_pin(mov("SAIDA", 300_000), "2222")).await;

        // anular só os de dias anteriores: a de hoje passa sem PIN
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "anular", "ativo": true, "so_dias_anteriores": true })).await;
        rpc(&st, "auth.login", json!({ "login": "op", "senha": "op12345" })).await;
        rpc(&st, "documentos.anular", json!({ "id": fr4, "motivo": "venda registada por engano" })).await;

        // o registo: quem pediu, quem aprovou, o quê
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        let lista = rpc(&st, "aprovacoes.listar", Value::Null).await;
        let linhas: Vec<(String, String, String, i64)> = lista
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .map(|a| (a["regra"].as_str().unwrap().into(), a["pedido_por"].as_str().unwrap().into(), a["aprovado_por"].as_str().unwrap().into(), a["valor"].as_i64().unwrap()))
            .collect();
        let esperado: Vec<(String, String, String, i64)> = [
            ("anular", "Operador", "Admin", 114_000),
            ("saidas", "Operador", "Gestora", 150_000),
            ("vales", "Operador", "Gestora", 120_000),
            ("creditos", "Operador", "Admin", 114_000),
            ("creditos", "Operador", "Admin", 4_000),
            ("saidas", "Gestora", "Gestora", 300_000),
        ]
        .into_iter()
        .map(|(a, b, c, d)| (a.into(), b.into(), c.into(), d))
        .collect();
        assert_eq!(linhas, esperado, "{lista}");
        assert!(lista.as_array().unwrap().iter().all(|a| a["referencia"].as_str().is_some_and(|r| !r.is_empty())), "{lista}");

        // desligada outra vez, a saída grande passa sem PIN
        rpc(&st, "aprovacoes.guardar", json!({ "regra": "saidas", "ativo": false, "valor_minimo": 100_000 })).await;
        rpc(&st, "tesouraria.movimento", mov("SAIDA", 900_000)).await;
        let _ = std::fs::remove_dir_all(&pasta);
    }
}

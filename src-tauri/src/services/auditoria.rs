// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Auditoria das fichas: o antes e o depois de cada gravação (o selo em cadeia está em `crate::auditoria`).
//!
//! O `dispatch` pergunta a `alvo` se o método mexe numa ficha. Se sim, lê a ficha antes, corre o método e, se correu
//! bem, lê-a outra vez e regista o que mudou: numa criação a ficha nova, numa alteração só os campos que mudaram, numa
//! eliminação a ficha como estava. Senhas, PIN e segredos nunca vão para o registo (ficam «•••») e as imagens ficam só
//! como «imagem».

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::auditoria::{self as cadeia, Linha};
use crate::db;
use crate::error::AppResult;
use crate::state::{contexto, AppState};

/// Fichas auditadas: tabela → nome para mostrar.
pub const FICHAS: &[(&str, &str)] = &[
    ("artigos", "Artigo"),
    ("entidades", "Entidade"),
    ("entidade_enderecos", "Endereço"),
    ("utilizadores", "Utilizador"),
    ("perfis", "Perfil"),
    ("moedas", "Moeda"),
    ("familias", "Família"),
    ("subfamilias", "Subfamília"),
    ("unidades", "Unidade"),
    ("armazens", "Armazém"),
    ("taxas_iva", "Taxa de IVA"),
    ("contas", "Conta"),
    ("centros_custo", "Centro de custo"),
    ("movimentos_tesouraria", "Movimento de tesouraria"),
    ("cc_regularizacoes", "Regularização"),
    ("tipos_documento", "Tipo de documento"),
    ("modos_pagamento", "Modo de pagamento"),
    ("series", "Série"),
    ("postos", "Posto"),
    ("api_chaves", "Chave da API"),
    ("api_webhooks", "Webhook"),
    ("promocao_regras", "Promoção"),
    ("cupoes", "Cupão"),
    ("funcionarios", "Funcionário"),
    ("rh_departamentos", "Departamento"),
    ("rh_funcoes", "Função"),
    ("rh_categorias", "Categoria"),
    ("of_viaturas", "Viatura"),
    ("of_ordens", "Ordem de reparação"),
    ("relatorio_modelos", "Modelo de relatório"),
    ("documentos", "Documento"),
    ("empresa", "Empresa"),
    ("config", "Configurações"),
];

fn rotulo(tabela: &str) -> &str {
    FICHAS.iter().find(|f| f.0 == tabela).map(|f| f.1).unwrap_or(tabela)
}

/// Uma ficha que um método grava: a tabela, a coluna da chave, o campo do pedido que a traz (vazio: a ficha é única,
/// como a da empresa) e a acção, quando não é criar ou alterar.
#[derive(Clone, Copy)]
pub(super) struct Alvo {
    tabela: &'static str,
    chave: &'static str,
    param: &'static str,
    acao: Option<&'static str>,
}

/// Métodos que gravam a configuração (tabela `config`, chave a chave): fica o que mudou.
const CONFIGURACOES: &[&str] = &[
    "seguranca.guardar",
    "parametros.opcoesGuardar",
    "fidelizacao.configGuardar",
    "email.configGuardar",
    "fe.guardar",
    "aparencia.guardar",
    "rh.configGuardar",
    "atualizacao.guardar",
    "sistema.copiasGuardar",
    "internet.guardar",
    "rede.guardar",
    "relatorios.irtCGuardar",
    "contabilidade.contasGuardar",
    "ramo.definir",
    "modo.definir",
    "api.guardar",
    "web.activar",
    "web.activarProprio",
    "web.desactivar",
];

pub(super) fn alvo(metodo: &str, params: &Value) -> Option<Alvo> {
    let a = |tabela: &'static str, chave: &'static str, param: &'static str, acao: Option<&'static str>| Some(Alvo { tabela, chave, param, acao });
    let id = |tabela: &'static str| a(tabela, "id", "id", None);
    if CONFIGURACOES.contains(&metodo) {
        return a("config", "chave", "", None);
    }
    match metodo {
        "artigos.guardar" => id("artigos"),
        "artigos.eliminar" => a("artigos", "id", "id", Some("ELIMINAR")),
        "entidades.eliminar" => a("entidades", "id", "id", Some("ELIMINAR")),
        "entidades.guardar" => id("entidades"),
        "entidades.enderecoGuardar" => id("entidade_enderecos"),
        "utilizadores.guardar" => id("utilizadores"),
        "perfis.guardar" => a("perfis", "codigo", "codigo", None),
        "perfis.eliminar" => a("perfis", "codigo", "codigo", Some("ELIMINAR")),
        "moedas.guardar" => a("moedas", "codigo", "codigo", None),
        "tabela.guardar" => id(match params.get("tabela")?.as_str()? {
            "familias" => "familias",
            "subfamilias" => "subfamilias",
            "unidades" => "unidades",
            "armazens" => "armazens",
            "taxas" => "taxas_iva",
            _ => return None,
        }),
        "rh.tabelaGuardar" => id(match params.get("tabela")?.as_str()? {
            "departamentos" => "rh_departamentos",
            "funcoes" => "rh_funcoes",
            "categorias" => "rh_categorias",
            _ => return None,
        }),
        "tesouraria.contaGuardar" => id("contas"),
        "tesouraria.centroGuardar" => id("centros_custo"),
        "tesouraria.anular" => a("movimentos_tesouraria", "id", "id", Some("ANULAR")),
        "contas.regularizacaoAnular" => a("cc_regularizacoes", "id", "id", Some("ANULAR")),
        "parametros.tipoGuardar" => a("tipos_documento", "codigo", "codigo", None),
        "parametros.modoGuardar" => a("modos_pagamento", "codigo", "codigo", None),
        "series.guardar" => id("series"),
        "series.eliminar" => a("series", "id", "id", Some("ELIMINAR")),
        "posto.guardar" => id("postos"),
        "api.chaveCriar" | "api.chaveGuardar" | "api.chaveRenovar" => id("api_chaves"),
        "api.chaveRevogar" => a("api_chaves", "id", "id", Some("REVOGAR")),
        "api.webhookGuardar" | "api.webhookSegredo" => id("api_webhooks"),
        "api.webhookApagar" => a("api_webhooks", "id", "id", Some("ELIMINAR")),
        "promocoes.regraGuardar" => id("promocao_regras"),
        "promocoes.cupaoGuardar" => id("cupoes"),
        "rh.funcionarioGuardar" => id("funcionarios"),
        "oficina.viaturaGuardar" => id("of_viaturas"),
        "oficina.ordemGuardar" | "oficina.ordemEstado" => id("of_ordens"),
        "documentos.anular" => a("documentos", "id", "id", Some("ANULAR")),
        "dados.modeloGuardar" => id("relatorio_modelos"),
        "dados.modeloEliminar" => a("relatorio_modelos", "id", "id", Some("ELIMINAR")),
        "empresa.guardar" => a("empresa", "id", "", None),
        _ => None,
    }
}

/// A chave da ficha em texto (ids a zero contam como ficha nova).
fn texto_chave(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) if n.as_i64() != Some(0) => Some(n.to_string()),
        Value::String(s) if !s.trim().is_empty() && s.trim() != "0" => Some(s.trim().to_string()),
        _ => None,
    }
}

fn chave_do_pedido(a: &Alvo, params: &Value) -> Option<String> {
    if a.param.is_empty() {
        return (a.tabela == "empresa").then(|| "1".to_string());
    }
    let c = texto_chave(params.get(a.param)?)?;
    // os códigos das moedas gravam-se em maiúsculas
    Some(if a.tabela == "moedas" { c.to_uppercase() } else { c })
}

/// Configurações que mudam sozinhas ou que não são do utilizador (licença, selo da própria auditoria).
fn config_interna(chave: &str) -> bool {
    ["licenca.", "auditoria.", "rede.ultimo", "fe.ultimo", "atualizacao.ultima"].iter().any(|p| chave.starts_with(p))
}

/// Campos (e chaves da configuração) que nunca vão para o registo com o valor: senhas, PIN, segredos, chaves privadas
/// e códigos de recuperação. Conta só o nome do campo, sem o prefixo da configuração («email.senha» → «senha»).
fn secreto(campo: &str) -> bool {
    let c = campo.to_ascii_lowercase();
    if crate::segredos::CHAVES.contains(&c.as_str()) {
        return true;
    }
    let nome = c.rsplit('.').next().unwrap_or(&c);
    matches!(nome, "senha" | "pin" | "segredo" | "token" | "chave_privada" | "chave_software" | "chave_contribuinte")
        || nome.ends_with("_hash")
        || nome.ends_with("_senha")
        || nome.ends_with("_segredo")
        || nome.ends_with("_token")
        || nome.starts_with("totp")
}

/// Campos que mudam sem ser o utilizador a mudá-los (contadores, últimos acessos).
fn ignorado(campo: &str) -> bool {
    matches!(campo, "tentativas" | "bloqueado_ate" | "ultimo_acesso" | "actualizado_em" | "atualizado_em" | "alterado_em" | "totp_ultimo" | "ultimo_uso")
}

/// O valor que fica no registo: sem segredos, as imagens só como «imagem» e os textos muito longos cortados.
fn limpo(campo: &str, v: &Value) -> Value {
    if v.is_null() {
        return Value::Null;
    }
    if secreto(campo) {
        return json!("•••");
    }
    match v.as_str() {
        Some(s) if s.starts_with("data:") => json!(format!("(imagem, {} KB)", (s.len() * 3 / 4).div_ceil(1024))),
        Some(s) if s.chars().count() > 2000 => json!(format!("{}…", s.chars().take(2000).collect::<String>())),
        _ => v.clone(),
    }
}

/// A ficha inteira, limpa (para uma criação ou eliminação): sem os campos vazios ou a zero, nem o id e a data de
/// criação (já estão na linha da auditoria).
fn ficha_limpa(f: &Value) -> Value {
    let mut m = Map::new();
    for (k, v) in f.as_object().into_iter().flatten() {
        let vazio = v.is_null() || v.as_str() == Some("") || v.as_i64() == Some(0) || v.as_f64() == Some(0.0);
        if !vazio && !ignorado(k) && !matches!(k.as_str(), "id" | "criado_em") {
            m.insert(k.clone(), limpo(k, v));
        }
    }
    Value::Object(m)
}

/// Só os campos que mudaram, limpos: (antes, depois). Um segredo que mudou fica «•••» → «••• (novo)».
fn diferencas(antes: &Value, depois: &Value) -> (Map<String, Value>, Map<String, Value>) {
    let (mut ma, mut md) = (Map::new(), Map::new());
    let vazio = Map::new();
    let a = antes.as_object().unwrap_or(&vazio);
    let d = depois.as_object().unwrap_or(&vazio);
    let campos = d.keys().chain(a.keys().filter(|k| !d.contains_key(*k)));
    for k in campos {
        let (va, vd) = (a.get(k).unwrap_or(&Value::Null), d.get(k).unwrap_or(&Value::Null));
        if va == vd || ignorado(k) {
            continue;
        }
        let (la, mut ld) = (limpo(k, va), limpo(k, vd));
        if la == ld && !ld.is_null() {
            ld = json!(format!("{} (novo)", ld.as_str().unwrap_or("•••")));
        }
        ma.insert(k.clone(), la);
        md.insert(k.clone(), ld);
    }
    (ma, md)
}

/// Nome de um campo para o resumo («preco1» → «preço 1»).
fn nome_campo(c: &str) -> String {
    let fixo = match c {
        "descricao" => "descrição",
        "descricao_curta" => "descrição curta",
        "codigo" => "código",
        "codigo_barras" => "código de barras",
        "referencia" => "referência",
        "familia_id" => "família",
        "subfamilia_id" => "subfamília",
        "unidade_id" => "unidade",
        "taxa_iva_id" => "taxa de IVA",
        "armazem_id" => "loja",
        "ativo" => "activo",
        "nif" => "NIF",
        "senha_hash" => "senha",
        "pin_hash" => "PIN",
        "foto" => "fotografia",
        "permissoes" => "permissões",
        "limite_desconto" => "limite de desconto",
        "cambio" => "câmbio",
        "simbolo" => "símbolo",
        "stock_minimo" => "stock mínimo",
        "preco_custo" | "custo" => "custo",
        "telefone" => "telefone",
        _ => "",
    };
    if !fixo.is_empty() {
        return fixo.to_string();
    }
    if let Some(n) = c.strip_prefix("preco").filter(|n| n.chars().all(|x| x.is_ascii_digit()) && !n.is_empty()) {
        return format!("preço {n}");
    }
    c.replace('_', " ")
}

/// Como a ficha se chama, para o resumo.
fn nome_ficha(f: &Value) -> Option<String> {
    ["numero_doc", "descricao", "nome", "designacao", "matricula", "login", "serie", "codigo", "referencia"]
        .iter()
        .find_map(|c| f.get(*c).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(String::from))
}

fn origem() -> &'static str {
    match contexto() {
        None => "local",
        Some(c) if c.api_chave.is_some() => "api",
        Some(c) if c.pela_internet => "internet",
        Some(_) => "rede",
    }
}

/// O estado da ficha (ou de toda a configuração) num momento.
async fn ler(st: &AppState, a: &Alvo, chave: Option<&str>) -> Option<Value> {
    if a.tabela == "config" {
        let linhas: Vec<(String, String)> = sqlx::query_as("SELECT chave, valor FROM config").fetch_all(&st.pool).await.ok()?;
        return Some(Value::Object(linhas.into_iter().filter(|(k, _)| !config_interna(k)).map(|(k, v)| (k, Value::String(v))).collect()));
    }
    let chave = chave?;
    let sql = format!("SELECT * FROM {} WHERE {} = ?", a.tabela, a.chave);
    let q = sqlx::query(&sql);
    let q = match (a.chave, chave.parse::<i64>()) {
        ("id", Ok(n)) => q.bind(n),
        _ => q.bind(chave.to_string()),
    };
    q.fetch_optional(&st.pool).await.ok().flatten().map(|r| db::row_json(&r))
}

/// Antes de correr o método: a ficha como está.
pub(super) async fn antes(st: &AppState, a: &Alvo, params: &Value) -> Option<Value> {
    ler(st, a, chave_do_pedido(a, params).as_deref()).await
}

/// Depois de o método correr bem: compara e regista. Uma gravação sem mudanças não fica no registo.
pub(super) async fn depois(st: &AppState, a: &Alvo, params: &Value, antes: Option<Value>, resultado: &Value) -> AppResult<()> {
    let chave = chave_do_pedido(a, params).or_else(|| {
        let v = resultado.get(a.chave).unwrap_or(resultado);
        texto_chave(v).map(|c| if a.tabela == "moedas" { c.to_uppercase() } else { c })
    });
    let agora = ler(st, a, chave.as_deref()).await;
    let motivo = params.get("motivo").and_then(|m| m.as_str());
    registar_mudanca(st, *a, chave.as_deref(), antes, agora, motivo).await
}

/// Uma ficha lida pela chave (para quem regista mudanças por conta própria, como as acções em lote).
pub(super) async fn ficha(st: &AppState, tabela: &'static str, id: i64) -> Option<Value> {
    ler(st, &Alvo { tabela, chave: "id", param: "id", acao: None }, Some(&id.to_string())).await
}

/// Regista uma mudança numa ficha com chave `id` (acções em lote): o mesmo que o `dispatch` faz num método só.
pub(super) async fn registar_ficha(st: &AppState, tabela: &'static str, id: i64, antes: Option<Value>, agora: Option<Value>, acao: Option<&'static str>) -> AppResult<()> {
    registar_mudanca(st, Alvo { tabela, chave: "id", param: "id", acao }, Some(&id.to_string()), antes, agora, None).await
}

/// Uma nota numa ficha que não muda a linha dela (ex.: um código de barras novo noutra tabela): fica como alteração.
pub(super) async fn registar_nota(st: &AppState, tabela: &'static str, id: i64, detalhe: &str, depois: Value) -> AppResult<()> {
    cadeia::registar(
        &st.pool,
        Linha {
            utilizador_id: st.sessao_opcional().map(|s| s.id),
            acao: "ALTERAR",
            detalhe,
            entidade: Some(tabela),
            registo_id: Some(&id.to_string()),
            antes: None,
            depois: Some(&depois),
            posto_id: Some(st.posto().id),
            origem: origem(),
        },
    )
    .await
}

async fn registar_mudanca(st: &AppState, a: Alvo, chave: Option<&str>, antes: Option<Value>, agora: Option<Value>, motivo: Option<&str>) -> AppResult<()> {
    // «eliminar» uma ficha já usada desactiva-a: fica como desactivação
    let acao = match (a.acao, &agora) {
        (Some("ELIMINAR"), Some(_)) => "DESACTIVAR",
        (Some(x), _) => x,
        (None, _) if antes.is_none() => "CRIAR",
        (None, _) => "ALTERAR",
    };
    let (va, vd) = match (&antes, &agora) {
        (Some(x), Some(y)) => {
            let (ma, md) = diferencas(x, y);
            if md.is_empty() && ma.is_empty() {
                if a.acao.is_none() {
                    return Ok(());
                }
                (None, None)
            } else {
                (Some(Value::Object(ma)), Some(Value::Object(md)))
            }
        }
        (None, Some(y)) => (None, Some(ficha_limpa(y))),
        (Some(x), None) => (Some(ficha_limpa(x)), None),
        (None, None) => return Ok(()),
    };
    let ficha = agora.as_ref().or(antes.as_ref());
    let mut detalhe = rotulo(a.tabela).to_string();
    if a.tabela != "config" {
        if let Some(nome) = ficha.and_then(nome_ficha) {
            detalhe.push_str(&format!(" «{nome}»"));
        }
        if let Some(c) = chave.filter(|_| a.chave == "id" && a.tabela != "empresa") {
            detalhe.push_str(&format!(" (n.º {c})"));
        }
    }
    if acao == "ALTERAR" || a.tabela == "config" {
        if let Some(Value::Object(m)) = &vd {
            let campos: Vec<String> = m.keys().map(|k| if a.tabela == "config" { k.clone() } else { nome_campo(k) }).collect();
            detalhe.push_str(&format!(": {}", campos.join(", ")));
        }
    }
    if let Some(motivo) = motivo.map(str::trim).filter(|m| !m.is_empty()) {
        detalhe.push_str(&format!(" — motivo: {motivo}"));
    }
    let registo_id = if a.tabela == "config" { None } else { chave };
    cadeia::registar(
        &st.pool,
        Linha {
            utilizador_id: st.sessao_opcional().map(|s| s.id),
            acao,
            detalhe: &detalhe,
            entidade: Some(a.tabela),
            registo_id,
            antes: va.as_ref(),
            depois: vd.as_ref(),
            posto_id: Some(st.posto().id),
            origem: origem(),
        },
    )
    .await
}

// ============================== consulta ==============================

#[derive(Deserialize)]
pub struct ListarIn {
    de: String,
    ate: String,
    #[serde(default)]
    utilizador_id: Option<i64>,
    /// acção (CRIAR, ALTERAR, LOGIN, ...)
    #[serde(default)]
    evento: Option<String>,
    #[serde(default)]
    entidade: Option<String>,
    #[serde(default)]
    registo_id: Option<Value>,
    #[serde(default)]
    texto: Option<String>,
    #[serde(default)]
    origem: Option<String>,
}

const LIMITE: i64 = 5000;

fn linhas_json(rows: &[sqlx::sqlite::SqliteRow]) -> Value {
    let mut v = db::rows_json(rows);
    for l in v.as_array_mut().into_iter().flatten() {
        for c in ["antes", "depois"] {
            if let Some(Value::String(s)) = l.get(c) {
                let j = serde_json::from_str(s).unwrap_or(Value::Null);
                l[c] = j;
            }
        }
        let e = l.get("entidade").and_then(|e| e.as_str()).map(|e| rotulo(e).to_string());
        l["entidade_nome"] = json!(e);
    }
    v
}

const SELECCAO: &str = "SELECT a.id, a.data_hora, a.acao, a.detalhe, a.entidade, a.registo_id, a.antes, a.depois, a.origem, a.utilizador_id,
        u.nome AS utilizador, p.descricao AS posto
   FROM auditoria a LEFT JOIN utilizadores u ON u.id = a.utilizador_id LEFT JOIN postos p ON p.id = a.posto_id";

pub async fn listar(st: &AppState, f: ListarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    let vazio = |s: Option<String>| s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    let texto = vazio(f.texto).map(|t| format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let sql = format!(
        "{SELECCAO}
          WHERE a.data_hora >= ?1 AND a.data_hora < date(?2, '+1 day')
            AND (?3 IS NULL OR a.utilizador_id = ?3) AND (?4 IS NULL OR a.acao = ?4)
            AND (?5 IS NULL OR a.entidade = ?5) AND (?6 IS NULL OR a.registo_id = ?6)
            AND (?7 IS NULL OR a.detalhe LIKE ?7 ESCAPE '\\' OR a.antes LIKE ?7 ESCAPE '\\' OR a.depois LIKE ?7 ESCAPE '\\' OR u.nome LIKE ?7 ESCAPE '\\')
            AND (?8 IS NULL OR a.origem = ?8)
          ORDER BY a.id DESC LIMIT {LIMITE}"
    );
    let rows = sqlx::query(&sql)
        .bind(f.de.trim())
        .bind(f.ate.trim())
        .bind(f.utilizador_id)
        .bind(vazio(f.evento))
        .bind(vazio(f.entidade))
        .bind(f.registo_id.as_ref().and_then(texto_chave))
        .bind(texto)
        .bind(vazio(f.origem))
        .fetch_all(&st.pool)
        .await?;
    Ok(linhas_json(&rows))
}

/// As acções e as fichas que já aparecem no registo (para os filtros).
pub async fn opcoes(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    let acoes: Vec<String> = sqlx::query_scalar("SELECT DISTINCT acao FROM auditoria ORDER BY acao").fetch_all(&st.pool).await?;
    let usadas: Vec<String> = sqlx::query_scalar("SELECT DISTINCT entidade FROM auditoria WHERE entidade IS NOT NULL").fetch_all(&st.pool).await?;
    let mut fichas: Vec<Value> = usadas.iter().map(|e| json!({ "entidade": e, "nome": rotulo(e) })).collect();
    fichas.sort_by(|a, b| a["nome"].as_str().cmp(&b["nome"].as_str()));
    Ok(json!({ "acoes": acoes, "fichas": fichas, "limite": LIMITE }))
}

#[derive(Deserialize)]
pub struct HistoricoIn {
    entidade: String,
    registo_id: Value,
}

/// O histórico de uma ficha: todas as criações, alterações e eliminações, da mais recente para a mais antiga.
pub async fn historico(st: &AppState, h: HistoricoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    let Some(id) = texto_chave(&h.registo_id) else { return Ok(json!([])) };
    let rows = sqlx::query(&format!("{SELECCAO} WHERE a.entidade = ? AND a.registo_id = ? ORDER BY a.id DESC LIMIT 1000"))
        .bind(h.entidade.trim())
        .bind(id)
        .fetch_all(&st.pool)
        .await?;
    Ok(linhas_json(&rows))
}

/// Confere o selo em cadeia de todo o registo.
pub async fn verificar(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.auditoria")?;
    cadeia::verificar(&st.pool).await
}

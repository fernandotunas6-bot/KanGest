use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::error::{AppError, AppResult};

/// Permissões por operação (equivalentes às do KIGEST: Stock, Vendas, Compras, Tesouraria, POS, Sistema).
pub const PERMISSOES: &[&str] = &[
    "artigos.criar",
    "artigos.editar",
    "stock.entrada",
    "stock.saida",
    "stock.transferencia",
    "stock.inventario",
    "vendas.fatura",
    "vendas.fatura_recibo",
    "vendas.proforma",
    "vendas.nota_credito",
    "vendas.nota_debito",
    "vendas.limite_credito",
    "vendas.documento_manual",
    "clientes",
    "compras.fatura",
    "fornecedores",
    "tesouraria.recibo",
    "tesouraria.pagamento",
    "tesouraria.fluxo",
    "documentos.anular",
    "caixa.movimentos",
    "pos.alterar_preco",
    "pos.desconto",
    "pos.quantidade",
    "pos.eliminar_linha",
    "pos.suspenso",
    "pos.fecho_caixa",
    "pos.relatorio",
    "pos.config_posto",
    "relatorios.ver",
    "rh.funcionarios",
    "rh.processamento",
    "oficina",
    "sistema.saft",
    "sistema.backup",
    "sistema.series",
    "sistema.utilizadores",
    // antes eram só por escalão (administrador ou gestor)
    "sistema.configuracoes",
    "sistema.tabelas",
    "sistema.atualizacoes",
    "sistema.fe",
    "sistema.importar",
    "sistema.parametrizacao",
    "sistema.certificacao",
    "sistema.auditoria",
    "tesouraria.regularizar",
    "rh.configuracao",
];

/// Permissões que, por omissão, só o administrador tem (o gestor tem todas as outras).
pub const SO_ADMIN: &[&str] = &[
    "sistema.utilizadores",
    "sistema.configuracoes",
    "sistema.atualizacoes",
    "sistema.fe",
    "sistema.importar",
    "sistema.parametrizacao",
    "sistema.certificacao",
    "sistema.auditoria",
    "rh.configuracao",
];

/// Permissões por omissão de cada perfil (quando o utilizador não tem lista própria).
pub fn permissoes_perfil(perfil: &str) -> Vec<String> {
    match perfil {
        "ADMIN" => PERMISSOES.iter().map(|s| s.to_string()).collect(),
        "GESTOR" => PERMISSOES.iter().filter(|p| !SO_ADMIN.contains(p)).map(|s| s.to_string()).collect(),
        _ => [
            "vendas.fatura",
            "vendas.fatura_recibo",
            "vendas.proforma",
            "clientes",
            "tesouraria.recibo",
            "pos.quantidade",
            "pos.eliminar_linha",
            "pos.suspenso",
            "pos.fecho_caixa",
            "pos.relatorio",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

/// Escalão de origem de cada perfil (os perfis criados pela empresa têm o seu na tabela `perfis`).
pub fn nivel_perfil(perfil: &str) -> u8 {
    match perfil {
        "ADMIN" => 3,
        "GESTOR" => 2,
        _ => 1,
    }
}

/// Escalão e permissões efectivas de um utilizador: a lista própria (JSON) ou a do perfil
/// (tabela `perfis`, ou a de origem). O administrador tem sempre todas.
pub async fn permissoes_efectivas(pool: &SqlitePool, perfil: &str, proprias: Option<&str>) -> (u8, Vec<String>) {
    let linha = sqlx::query("SELECT nivel, permissoes FROM perfis WHERE codigo = ?").bind(perfil).fetch_optional(pool).await.ok().flatten();
    let nivel = linha.as_ref().map(|r| r.get::<i64, _>("nivel").clamp(1, 3) as u8).unwrap_or_else(|| nivel_perfil(perfil));
    if perfil == "ADMIN" {
        return (3, permissoes_perfil("ADMIN"));
    }
    let do_perfil = linha.and_then(|r| r.get::<Option<String>, _>("permissoes")).and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());
    let lista = proprias
        .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .or(do_perfil)
        .unwrap_or_else(|| permissoes_perfil(perfil));
    (nivel, lista.into_iter().filter(|p| PERMISSOES.contains(&p.as_str())).collect())
}


#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Sessao {
    pub id: i64,
    pub nome: String,
    pub login: String,
    pub perfil: String,
    pub permissoes: Vec<String>,
    pub limite_desconto: f64,
    /// Escalão do perfil (1 caixa · 2 gestor · 3 administrador); 0 = calcular pelo nome do perfil.
    #[serde(default)]
    pub nivel: u8,
    /// A senha é provisória: até a mudar, só pode mudar a senha ou sair.
    #[serde(default)]
    pub mudar_senha: bool,
    /// Sessão bloqueada (por inactividade ou pelo operador): só se retoma com a senha ou o PIN.
    #[serde(default)]
    pub bloqueada: bool,
    /// Nome do perfil para mostrar (ex.: «Caixa (POS)», «Armazém»).
    #[serde(default)]
    pub perfil_descricao: Option<String>,
}

impl Sessao {
    pub fn tem(&self, p: &str) -> bool {
        self.permissoes.iter().any(|x| x == p)
    }
}

/// Configuração do posto desta máquina (n.º, armazém, série e opções do POS).
#[derive(Debug, Clone, Serialize)]
pub struct PostoAtual {
    pub id: i64,
    pub descricao: String,
    pub armazem_id: i64,
    pub serie_prefixo: String,
    pub opcoes: Value,
}

impl Default for PostoAtual {
    fn default() -> Self {
        Self { id: 1, descricao: "Posto#1".into(), armazem_id: 1, serie_prefixo: String::new(), opcoes: Value::Null }
    }
}

/// Contexto de um pedido que não vem desta máquina: cada posto em rede tem a sua sessão
/// e o seu posto, por isso a sessão global do processo não pode ser usada para o servir.
#[derive(Clone)]
pub struct Contexto {
    pub sessao: Arc<Mutex<Option<Sessao>>>,
    pub posto: PostoAtual,
}

tokio::task_local! {
    pub static CONTEXTO: Contexto;
}

/// O contexto do pedido em curso, quando vem de um posto em rede.
pub fn contexto() -> Option<Contexto> {
    CONTEXTO.try_with(|c| c.clone()).ok()
}

pub struct AppState {
    pub pool: SqlitePool,
    pub sessao: Mutex<Option<Sessao>>,
    /// Serializa a emissão de documentos (numeração + cadeia de hash sem saltos).
    pub emissao: tokio::sync::Mutex<()>,
    pub posto: RwLock<PostoAtual>,
    /// Pasta onde se gravam exportações (SAF-T, cópias de segurança manuais). Por omissão: Transferências.
    pub pasta_exportacao: PathBuf,
    /// Pasta de dados da aplicação (base de dados, posto.json, cópias automáticas).
    pub pasta_dados: PathBuf,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            sessao: Mutex::new(None),
            emissao: tokio::sync::Mutex::new(()),
            posto: RwLock::new(PostoAtual::default()),
            pasta_exportacao: std::env::temp_dir(),
            pasta_dados: std::env::temp_dir(),
        }
    }

    pub fn posto(&self) -> PostoAtual {
        match contexto() {
            Some(c) => c.posto,
            None => self.posto.read().unwrap().clone(),
        }
    }

    /// Lê um posto da base de dados sem mexer na cache desta máquina (se não existir, usa o posto 1).
    pub async fn ler_posto(&self, id: i64) -> AppResult<PostoAtual> {
        let row = sqlx::query("SELECT * FROM postos WHERE id = ? OR id = 1 ORDER BY id = ? DESC LIMIT 1")
            .bind(id)
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        let opcoes: Option<String> = row.get("opcoes");
        Ok(PostoAtual {
            id: row.get("id"),
            descricao: row.get("descricao"),
            armazem_id: row.get("armazem_id"),
            serie_prefixo: row.get("serie_prefixo"),
            opcoes: opcoes.and_then(|o| serde_json::from_str(&o).ok()).unwrap_or(Value::Null),
        })
    }

    /// Carrega o posto `id` e passa a ser o posto desta máquina.
    pub async fn carregar_posto(&self, id: i64) -> AppResult<PostoAtual> {
        let p = self.ler_posto(id).await?;
        if contexto().is_none() {
            *self.posto.write().unwrap() = p.clone();
        }
        Ok(p)
    }

    /// Guarda o posto que o servidor devolveu (modo cliente: a base de dados é remota).
    pub fn definir_posto(&self, p: PostoAtual) {
        *self.posto.write().unwrap() = p;
    }

    pub fn sessao(&self) -> AppResult<Sessao> {
        self.sessao_opcional().ok_or(AppError::NaoAutenticado)
    }

    pub fn sessao_opcional(&self) -> Option<Sessao> {
        match contexto() {
            Some(c) => c.sessao.lock().unwrap().clone(),
            None => self.sessao.lock().unwrap().clone(),
        }
    }

    /// Início e fim de sessão (do posto em rede que fez o pedido, ou desta máquina).
    pub fn definir_sessao(&self, s: Option<Sessao>) {
        match contexto() {
            Some(c) => *c.sessao.lock().unwrap() = s,
            None => *self.sessao.lock().unwrap() = s,
        }
    }


    /// Exige uma permissão por operação.
    pub fn exigir_perm(&self, perm: &str) -> AppResult<Sessao> {
        let s = self.sessao()?;
        if !s.tem(perm) {
            return Err(AppError::SemPermissao);
        }
        Ok(s)
    }
}

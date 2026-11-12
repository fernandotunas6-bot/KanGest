//! Importação de dados do KIGEST (SQL Server `KigestGC`) para o KanGest novo.
//!
//! Duas camadas:
//! * `Fonte` — de onde se lêem as tabelas (SQL Server via TDS, ou memória nos testes);
//! * `importar_dados` — mapeia as tabelas do KIGEST (tbzconfigCompany, tbArtigos, tbArtigosStock,
//!   tbDocumentosCabessalho/Detalhe, tbContasCorrente, tbUsuario…) para o esquema novo.
//!
//! O mapeamento é adaptativo: cada campo é procurado por vários nomes de coluna possíveis.
//! Os documentos importados ficam como histórico (só leitura) com as assinaturas originais;
//! o sistema novo continua em séries próprias.

use std::collections::{HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, Sqlite, Transaction};

use super::auth::hash_senha;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ------------------------------------------------------------------ valores
#[derive(Debug, Clone)]
pub enum Val {
    Null,
    I(i64),
    F(f64),
    S(String),
    B(Vec<u8>),
}

impl Val {
    fn texto(&self) -> Option<String> {
        match self {
            Val::S(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
            Val::I(i) => Some(i.to_string()),
            Val::F(f) => Some(if f.fract() == 0.0 { format!("{}", *f as i64) } else { f.to_string() }),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Val::I(i) => Some(*i as f64),
            Val::F(f) => Some(*f),
            Val::S(s) => s.trim().replace(',', ".").parse().ok(),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        self.num().map(|f| f.round() as i64)
    }
    fn bool(&self) -> Option<bool> {
        match self {
            Val::S(s) => {
                let s = s.trim().to_lowercase();
                Some(matches!(s.as_str(), "1" | "s" | "sim" | "true" | "verdadeiro" | "y" | "yes"))
            }
            v => v.num().map(|n| n != 0.0),
        }
    }
}

/// Uma linha de uma tabela de origem, com as colunas em minúsculas.
pub type Registo = HashMap<String, Val>;

/// Primeiro campo existente e não vazio entre os nomes candidatos.
fn campo<'a>(r: &'a Registo, nomes: &[&str]) -> Option<&'a Val> {
    nomes.iter().filter_map(|n| r.get(&n.to_lowercase())).find(|v| !matches!(v, Val::Null) && !matches!(v, Val::S(s) if s.trim().is_empty()))
}
fn t(r: &Registo, nomes: &[&str]) -> Option<String> {
    campo(r, nomes).and_then(|v| v.texto())
}
fn n(r: &Registo, nomes: &[&str]) -> Option<f64> {
    campo(r, nomes).and_then(|v| v.num())
}
fn i(r: &Registo, nomes: &[&str]) -> Option<i64> {
    campo(r, nomes).and_then(|v| v.int())
}
fn b(r: &Registo, nomes: &[&str]) -> Option<bool> {
    campo(r, nomes).and_then(|v| v.bool())
}
/// Código de cliente ou fornecedor do KIGEST, tal como está: pode ser número ou texto (há bases com
/// códigos como «Filial centro», «000001234», «0000000» e «0000000000», que são clientes diferentes).
fn ent(r: &Registo, nomes: &[&str]) -> Option<String> {
    campo(r, nomes)?.texto()
}
/// Kwanzas (decimal) → cêntimos.
fn kz(r: &Registo, nomes: &[&str]) -> i64 {
    n(r, nomes).map(|v| (v * 100.0).round() as i64).unwrap_or(0)
}
fn data(r: &Registo, nomes: &[&str]) -> Option<String> {
    t(r, nomes).filter(|s| s.len() >= 10 && s.as_bytes()[4] == b'-').map(|s| s[..10].to_string())
}
fn data_hora(r: &Registo, nomes: &[&str]) -> Option<String> {
    t(r, nomes).filter(|s| s.len() >= 19 && s.as_bytes()[4] == b'-').map(|s| s[..19].replace(' ', "T"))
}

// ------------------------------------------------------------------ fontes
pub trait Fonte {
    /// Tabelas e vistas disponíveis, com o n.º de registos (-1 se desconhecido).
    async fn tabelas(&mut self) -> AppResult<Vec<(String, i64)>>;
    async fn ler(&mut self, tabela: &str) -> AppResult<Vec<Registo>>;
    /// Colunas de cada tabela/vista (para a análise; vazio se a fonte não souber).
    async fn colunas(&mut self) -> AppResult<HashMap<String, Vec<String>>> {
        Ok(HashMap::new())
    }
}

/// Fonte em memória (testes).
#[cfg(test)]
pub struct Memoria(pub HashMap<String, Vec<Registo>>);

#[cfg(test)]
impl Fonte for Memoria {
    async fn tabelas(&mut self) -> AppResult<Vec<(String, i64)>> {
        Ok(self.0.iter().map(|(k, v)| (k.clone(), v.len() as i64)).collect())
    }
    async fn ler(&mut self, tabela: &str) -> AppResult<Vec<Registo>> {
        Ok(self.0.get(tabela).cloned().unwrap_or_default())
    }
    async fn colunas(&mut self) -> AppResult<HashMap<String, Vec<String>>> {
        Ok(self.0.iter().map(|(k, v)| (k.clone(), v.first().map(|r| r.keys().cloned().collect()).unwrap_or_default())).collect())
    }
}

#[derive(Deserialize, Clone)]
pub struct ConexaoIn {
    /// Nome ou IP do servidor (ex.: DESKTOP-G26LDJM ou 192.168.1.10). Aceita "SERVIDOR\\INSTANCIA".
    servidor: String,
    instancia: Option<String>,
    porta: Option<u16>,
    base: String,
    utilizador: String,
    senha: String,
}

pub struct SqlServer {
    cliente: tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
}

impl SqlServer {
    pub async fn ligar(c: &ConexaoIn) -> AppResult<Self> {
        // Tenta primeiro com encriptação (servidores recentes); se o TLS falhar, sem encriptação (SQL Server antigos).
        match Self::tentar(c, tiberius::EncryptionLevel::Required).await {
            Ok(s) => Ok(s),
            Err(e1) => Self::tentar(c, tiberius::EncryptionLevel::NotSupported)
                .await
                .map_err(|e2| AppError::msg(format!("Não foi possível ligar ao SQL Server: {e2} (com TLS: {e1})"))),
        }
    }

    async fn tentar(c: &ConexaoIn, enc: tiberius::EncryptionLevel) -> Result<Self, String> {
        use tiberius::SqlBrowser;
        use tokio_util::compat::TokioAsyncWriteCompatExt;
        let (host, inst_host) = match c.servidor.split_once('\\') {
            Some((h, i)) => (h.trim().to_string(), Some(i.trim().to_string())),
            None => (c.servidor.trim().to_string(), None),
        };
        let instancia = c.instancia.clone().filter(|s| !s.trim().is_empty()).or(inst_host);
        let mut cfg = tiberius::Config::new();
        cfg.host(&host);
        if let Some(p) = c.porta {
            cfg.port(p);
        }
        if let Some(inst) = &instancia {
            cfg.instance_name(inst);
        }
        cfg.database(&c.base);
        cfg.authentication(tiberius::AuthMethod::sql_server(&c.utilizador, &c.senha));
        cfg.trust_cert();
        cfg.encryption(enc);
        let tcp = if instancia.is_some() && c.porta.is_none() {
            tokio::net::TcpStream::connect_named(&cfg).await.map_err(|e| e.to_string())?
        } else {
            tokio::net::TcpStream::connect(cfg.get_addr()).await.map_err(|e| e.to_string())?
        };
        tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        let cliente = tiberius::Client::connect(cfg, tcp.compat_write()).await.map_err(|e| e.to_string())?;
        Ok(Self { cliente })
    }
}

fn valor(row: &tiberius::Row, idx: usize) -> Val {
    if let Ok(Some(v)) = row.try_get::<i32, _>(idx) {
        return Val::I(v as i64);
    }
    if let Ok(Some(v)) = row.try_get::<i64, _>(idx) {
        return Val::I(v);
    }
    if let Ok(Some(v)) = row.try_get::<i16, _>(idx) {
        return Val::I(v as i64);
    }
    if let Ok(Some(v)) = row.try_get::<u8, _>(idx) {
        return Val::I(v as i64);
    }
    if let Ok(Some(v)) = row.try_get::<bool, _>(idx) {
        return Val::I(v as i64);
    }
    if let Ok(Some(v)) = row.try_get::<f64, _>(idx) {
        return Val::F(v);
    }
    if let Ok(Some(v)) = row.try_get::<f32, _>(idx) {
        return Val::F(v as f64);
    }
    if let Ok(Some(v)) = row.try_get::<tiberius::numeric::Numeric, _>(idx) {
        return Val::F(v.value() as f64 / 10f64.powi(v.scale() as i32));
    }
    if let Ok(Some(v)) = row.try_get::<&str, _>(idx) {
        return Val::S(v.to_string());
    }
    if let Ok(Some(v)) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
        return Val::S(v.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    if let Ok(Some(v)) = row.try_get::<chrono::NaiveDate, _>(idx) {
        return Val::S(v.format("%Y-%m-%d").to_string());
    }
    if let Ok(Some(v)) = row.try_get::<&[u8], _>(idx) {
        return Val::B(v.to_vec());
    }
    Val::Null
}

impl Fonte for SqlServer {
    async fn tabelas(&mut self) -> AppResult<Vec<(String, i64)>> {
        let sql = "SELECT t.name, CAST(SUM(p.rows) AS BIGINT) AS linhas FROM sys.tables t
                   JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0, 1) GROUP BY t.name
                   UNION ALL SELECT v.name, CAST(-1 AS BIGINT) FROM sys.views v";
        let rows = self
            .cliente
            .simple_query(sql)
            .await
            .map_err(|e| AppError::msg(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| AppError::msg(e.to_string()))?;
        Ok(rows
            .iter()
            .filter_map(|r| Some((r.try_get::<&str, _>(0).ok()??.to_string(), r.try_get::<i64, _>(1).ok().flatten().unwrap_or(-1))))
            .collect())
    }

    async fn ler(&mut self, tabela: &str) -> AppResult<Vec<Registo>> {
        if !tabela.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(AppError::msg("Nome de tabela inválido."));
        }
        let rows = self
            .cliente
            .simple_query(format!("SELECT * FROM [{tabela}]"))
            .await
            .map_err(|e| AppError::msg(format!("{tabela}: {e}")))?
            .into_first_result()
            .await
            .map_err(|e| AppError::msg(format!("{tabela}: {e}")))?;
        Ok(rows
            .iter()
            .map(|r| r.columns().iter().enumerate().map(|(k, c)| (c.name().to_lowercase(), valor(r, k))).collect())
            .collect())
    }

    async fn colunas(&mut self) -> AppResult<HashMap<String, Vec<String>>> {
        let rows = self
            .cliente
            .simple_query("SELECT TABLE_NAME, COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS ORDER BY TABLE_NAME, ORDINAL_POSITION")
            .await
            .map_err(|e| AppError::msg(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| AppError::msg(e.to_string()))?;
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        for r in &rows {
            if let (Ok(Some(t)), Ok(Some(c))) = (r.try_get::<&str, _>(0), r.try_get::<&str, _>(1)) {
                m.entry(t.to_string()).or_default().push(c.to_string());
            }
        }
        Ok(m)
    }
}

// ------------------------------------------------------------------ ficheiro exportado
/// Ficheiro «.kigest» gerado pelo ExportarKIGEST (`ferramentas/exportar-kigest`): JSON por linhas,
/// normalmente em gzip. Linhas de controlo começam por «{» (cabeçalho, `tabela` com as colunas,
/// `fim`, `concluido`); os registos são listas («[»), pela ordem das colunas.
/// Descomprime-se uma vez para um ficheiro temporário, com o início de cada tabela indexado.
pub struct Ficheiro {
    tmp: std::path::PathBuf,
    indice: Vec<TabelaFicheiro>,
    pub origem: String,
    pub data: String,
    pub computador: String,
    pub completo: bool,
    pub erros: Vec<String>,
}

struct TabelaFicheiro {
    nome: String,
    colunas: Vec<String>,
    inicio: u64,
    registos: i64,
}

impl Drop for Ficheiro {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tmp);
    }
}

impl Ficheiro {
    pub fn abrir(caminho: &std::path::Path) -> AppResult<Self> {
        use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
        let erro_io = |e: std::io::Error| AppError::msg(format!("Não foi possível ler {}: {e}", caminho.display()));
        let mut f = std::fs::File::open(caminho).map_err(erro_io)?;
        let mut magia = [0u8; 2];
        let n = f.read(&mut magia).map_err(erro_io)?;
        f.seek(SeekFrom::Start(0)).map_err(erro_io)?;
        let leitor: Box<dyn Read> = if n == 2 && magia == [0x1f, 0x8b] { Box::new(flate2::read::MultiGzDecoder::new(f)) } else { Box::new(f) };
        let mut entrada = BufReader::with_capacity(1 << 16, leitor);
        let unico = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("kigest-importacao-{}-{unico}.jsonl", std::process::id()));
        let mut fic = Ficheiro { tmp, indice: Vec::new(), origem: String::new(), data: String::new(), computador: String::new(), completo: false, erros: Vec::new() };
        let mut saida = BufWriter::new(std::fs::File::create(&fic.tmp).map_err(|e| AppError::msg(format!("Não foi possível criar um ficheiro temporário: {e}")))?);
        let invalido = || AppError::msg("Este ficheiro não é uma exportação do KIGEST (use o ExportarKIGEST no computador do KIGEST).");
        let (mut pos, mut linha, mut primeira, mut atual): (u64, String, bool, Option<usize>) = (0, String::new(), true, None);
        loop {
            linha.clear();
            let lidos = match entrada.read_line(&mut linha) {
                Ok(n) => n,
                Err(_) if primeira => return Err(invalido()),
                Err(e) => return Err(AppError::msg(format!("O ficheiro está danificado ou incompleto: {e}"))),
            };
            if lidos == 0 {
                break;
            }
            saida.write_all(linha.as_bytes()).map_err(|e| AppError::msg(format!("Sem espaço para descomprimir o ficheiro: {e}")))?;
            pos += lidos as u64;
            let l = linha.trim_start_matches('\u{feff}').trim_end();
            if primeira {
                primeira = false;
                let v: Value = serde_json::from_str(l).map_err(|_| invalido())?;
                if v["formato"] != "kigest-exportacao" {
                    return Err(invalido());
                }
                fic.origem = v["origem"].as_str().unwrap_or("").to_string();
                fic.data = v["data"].as_str().unwrap_or("").to_string();
                fic.computador = v["computador"].as_str().unwrap_or("").to_string();
                continue;
            }
            if l.starts_with('[') {
                if let Some(k) = atual {
                    fic.indice[k].registos += 1;
                }
            } else if l.starts_with('{') {
                let v: Value = serde_json::from_str(l).map_err(|e| AppError::msg(format!("Linha de controlo inválida no ficheiro: {e}")))?;
                if let Some(nome) = v["tabela"].as_str() {
                    let colunas = v["colunas"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    fic.indice.push(TabelaFicheiro { nome: nome.to_string(), colunas, inicio: pos, registos: 0 });
                    atual = Some(fic.indice.len() - 1);
                } else if let Some(nome) = v["fim"].as_str() {
                    if let Some(e) = v["erro"].as_str() {
                        fic.erros.push(format!("{nome}: {e}"));
                    }
                    atual = None;
                } else if v["concluido"] == true {
                    fic.completo = true;
                }
            }
        }
        if primeira {
            return Err(invalido());
        }
        saida.flush().map_err(|e| AppError::msg(format!("Sem espaço para descomprimir o ficheiro: {e}")))?;
        Ok(fic)
    }

    fn valor_json(v: Value) -> Val {
        match v {
            Value::Null | Value::Array(_) => Val::Null,
            Value::Bool(b) => Val::I(b as i64),
            Value::Number(n) => n.as_i64().map(Val::I).unwrap_or_else(|| Val::F(n.as_f64().unwrap_or(0.0))),
            Value::String(s) => Val::S(s),
            Value::Object(o) => o.get("b64").and_then(|x| x.as_str()).and_then(|b| B64.decode(b).ok()).map(Val::B).unwrap_or(Val::Null),
        }
    }

    pub fn registos_total(&self) -> i64 {
        self.indice.iter().map(|t| t.registos).sum()
    }
}

impl Fonte for Ficheiro {
    async fn tabelas(&mut self) -> AppResult<Vec<(String, i64)>> {
        Ok(self.indice.iter().map(|t| (t.nome.clone(), t.registos)).collect())
    }

    async fn ler(&mut self, tabela: &str) -> AppResult<Vec<Registo>> {
        use std::io::{BufRead, BufReader, Seek, SeekFrom};
        let Some(t) = self.indice.iter().find(|t| t.nome.eq_ignore_ascii_case(tabela)) else { return Ok(vec![]) };
        let mut f = std::fs::File::open(&self.tmp).map_err(|e| AppError::msg(format!("Ficheiro temporário da importação: {e}")))?;
        f.seek(SeekFrom::Start(t.inicio)).map_err(|e| AppError::msg(e.to_string()))?;
        let colunas: Vec<String> = t.colunas.iter().map(|c| c.to_lowercase()).collect();
        let mut regs = Vec::with_capacity(t.registos.max(0) as usize);
        let mut linha = String::new();
        let mut leitor = BufReader::with_capacity(1 << 16, f);
        while (regs.len() as i64) < t.registos {
            linha.clear();
            if leitor.read_line(&mut linha).map_err(|e| AppError::msg(e.to_string()))? == 0 {
                break;
            }
            let l = linha.trim_end();
            if !l.starts_with('[') {
                break;
            }
            let valores: Vec<Value> = serde_json::from_str(l).map_err(|e| AppError::msg(format!("{tabela}: registo inválido no ficheiro ({e})")))?;
            regs.push(colunas.iter().cloned().zip(valores.into_iter().map(Self::valor_json)).collect());
        }
        Ok(regs)
    }

    async fn colunas(&mut self) -> AppResult<HashMap<String, Vec<String>>> {
        Ok(self.indice.iter().map(|t| (t.nome.clone(), t.colunas.clone())).collect())
    }
}

#[derive(Deserialize)]
pub struct FicheiroIn {
    caminho: String,
}

async fn abrir_ficheiro(caminho: &str) -> AppResult<Ficheiro> {
    let caminho = std::path::PathBuf::from(caminho.trim().trim_matches('"'));
    if !caminho.is_file() {
        return Err(AppError::msg(format!("Ficheiro não encontrado: {}", caminho.display())));
    }
    tokio::task::spawn_blocking(move || Ficheiro::abrir(&caminho)).await.map_err(|e| AppError::msg(e.to_string()))?
}

fn info_ficheiro(f: &Ficheiro) -> Value {
    json!({ "origem": f.origem, "data": f.data, "computador": f.computador, "completo": f.completo, "erros": f.erros, "registos": f.registos_total() })
}

pub async fn analisar_ficheiro(st: &AppState, e: FicheiroIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let mut fonte = abrir_ficheiro(&e.caminho).await?;
    let mut a = analisar_fonte(&mut fonte).await?;
    a["ficheiro"] = info_ficheiro(&fonte);
    Ok(a)
}

#[derive(Deserialize)]
pub struct ExecutarFicheiroIn {
    caminho: String,
    opcoes: Opcoes,
}

pub async fn executar_ficheiro(st: &AppState, e: ExecutarFicheiroIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let mut fonte = abrir_ficheiro(&e.caminho).await?;
    if !fonte.completo {
        return Err(AppError::msg("O ficheiro está incompleto: a exportação não chegou ao fim. Volte a correr o ExportarKIGEST."));
    }
    let nome = std::path::Path::new(e.caminho.trim()).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let origem = if fonte.origem.is_empty() { nome } else { format!("{} (ficheiro {nome})", fonte.origem) };
    let mut r = importar_dados(st, &mut fonte, &e.opcoes, &origem).await?;
    if !fonte.erros.is_empty() {
        if let Some(av) = r["avisos"].as_array_mut() {
            for x in &fonte.erros {
                av.push(json!(format!("Tabela não exportada pelo KIGEST: {x}")));
            }
        }
    }
    Ok(r)
}

/// Ficheiros «.kigest» à mão: Transferências, Ambiente de Trabalho, Documentos e pens/discos externos.
pub async fn ficheiros(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let pasta_exp = st.pasta_exportacao.clone();
    let lista = tokio::task::spawn_blocking(move || procurar_ficheiros(&pasta_exp)).await.map_err(|e| AppError::msg(e.to_string()))?;
    Ok(json!(lista))
}

fn procurar_ficheiros(pasta_exportacao: &std::path::Path) -> Vec<Value> {
    use std::path::PathBuf;
    let mut pastas: Vec<(PathBuf, usize)> = vec![(pasta_exportacao.to_path_buf(), 1)];
    let casa = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    if let Some(c) = &casa {
        for p in ["Desktop", "Downloads", "Documents", "OneDrive/Desktop", "OneDrive/Ambiente de Trabalho", "OneDrive/Documentos"] {
            pastas.push((c.join(p), 1));
        }
    }
    if cfg!(windows) {
        for letra in b'D'..=b'Z' {
            pastas.push((PathBuf::from(format!("{}:\\", letra as char)), 1));
        }
    } else if let Ok(vols) = std::fs::read_dir("/Volumes") {
        for v in vols.flatten() {
            pastas.push((v.path(), 1));
        }
    }
    let mut vistos = HashSet::new();
    let mut achados = Vec::new();
    fn visitar(p: &std::path::Path, nivel: usize, achados: &mut Vec<Value>, vistos: &mut HashSet<std::path::PathBuf>) {
        let Ok(it) = std::fs::read_dir(p) else { return };
        for e in it.flatten().take(500) {
            let caminho = e.path();
            let Ok(tipo) = e.file_type() else { continue };
            if tipo.is_dir() {
                let oculto = e.file_name().to_string_lossy().starts_with('.');
                if nivel > 0 && !oculto {
                    visitar(&caminho, nivel - 1, achados, vistos);
                }
            } else if caminho.extension().is_some_and(|x| x.eq_ignore_ascii_case("kigest")) && vistos.insert(caminho.clone()) {
                let meta = e.metadata().ok();
                let modificado = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%dT%H:%M:%S").to_string())
                    .unwrap_or_default();
                achados.push(json!({
                    "caminho": caminho.display().to_string(),
                    "nome": e.file_name().to_string_lossy(),
                    "tamanho": meta.map(|m| m.len()).unwrap_or(0),
                    "modificado": modificado,
                }));
            }
        }
    }
    for (p, nivel) in pastas {
        if p.is_dir() {
            visitar(&p, nivel, &mut achados, &mut vistos);
        }
    }
    achados.sort_by(|a, b| b["modificado"].as_str().cmp(&a["modificado"].as_str()));
    achados
}

// ------------------------------------------------------------------ opções e análise
#[derive(Deserialize, Clone)]
pub struct Opcoes {
    #[serde(default = "sim")]
    pub empresa: bool,
    #[serde(default = "sim")]
    pub artigos: bool,
    #[serde(default = "sim")]
    pub stock: bool,
    #[serde(default = "sim")]
    pub clientes: bool,
    #[serde(default = "sim")]
    pub fornecedores: bool,
    #[serde(default = "sim")]
    pub utilizadores: bool,
    #[serde(default = "sim")]
    pub documentos: bool,
    /// Funcionários, departamentos, funções, categorias, agregado familiar e férias.
    #[serde(default = "sim")]
    pub rh: bool,
    /// Lotes de artigos (saldo e validade por lote).
    #[serde(default = "sim")]
    pub lotes: bool,
    /// Viaturas e ordens de serviço.
    #[serde(default = "sim")]
    pub oficina: bool,
    /// Contas de caixa e banco, centros de custo, movimentos do fluxo de caixa, fechos de caixa, moedas e prazos.
    #[serde(default = "sim")]
    pub tesouraria: bool,
    /// Ensaio: corre a importação toda e desfaz no fim (mostra contagens e avisos sem gravar nada).
    #[serde(default)]
    pub simular: bool,
}
fn sim() -> bool {
    true
}

/// Tabelas do KIGEST reconhecidas (nome canónico → candidatos).
const TABELAS: &[(&str, &[&str])] = &[
    ("empresa", &["tbzconfigCompany", "tbEmpresa"]),
    ("taxas", &["tbzconfigTaxasIva"]),
    ("motivos", &["tbzconfigTaxaMotivoIsencao"]),
    ("unidades", &["tbArtigosUnidade"]),
    ("familias", &["tbArtigosFamilia"]),
    ("subfamilias", &["tbArtigosSubFamilia"]),
    ("armazens", &["tbzconfigArmazem"]),
    ("artigos", &["tbArtigos"]),
    ("stock", &["tbArtigosStock"]),
    ("clientes", &["tbClientes", "tbCliente", "vsClientes"]),
    ("fornecedores", &["tbFornecedores", "tbFornecedor", "vsFornecedores"]),
    ("utilizadores", &["tbUsuario", "tbUsuarios"]),
    ("tipos", &["tbzconfigTiposDocumentos"]),
    ("documentos", &["tbDocumentosCabessalho"]),
    ("linhas", &["tbDocumentosDetalhe"]),
    ("pagamentos", &["tbDocsModoPagamentos"]),
    ("conta_corrente", &["tbContasCorrente"]),
    ("rh_funcionarios", &["tbRhFuncionarios"]),
    ("rh_departamentos", &["tbZconfigRhDepartamento"]),
    ("rh_funcoes", &["tbZconfigRhFuncao"]),
    ("rh_categorias", &["tbZconfigRhCategoria"]),
    ("rh_agregado", &["tbRhAgregadoFamiliar"]),
    ("rh_ferias", &["tbRhMapaFerias"]),
    ("lotes", &["tbArtigosLotes"]),
    ("viaturas", &["vsOficinaFichaAutomovel"]),
    ("ordens", &["vsOrdemServicoCabessalho"]),
    ("ordem_linhas", &["vsOrdemServicoDetalhes"]),
    ("recibos", &["vsSAFT_REC_Cabecalho", "tbReciboCabessalho"]),
    ("recibo_linhas", &["vsSAFT_ReciboLinha", "tbReciboDetalhes"]),
    ("recibo_pagamentos", &["vsTotalporPagtoRecibos"]),
    ("retencoes", &["tbDocsRetencaoFonte"]),
    ("custos", &["tbDocsDetalhesCustoMecadorias", "tbDocsDetalhesCustoMercadorias"]),
    ("tipos_custo", &["tbzconfigTiposCustosMercadorias"]),
    ("proformas", &["vsDocsProformaCabessalho", "tbProformas"]),
    ("proforma_linhas", &["vsDocsProformaDetalhes", "vsProformaDetalhes"]),
    ("tes_contas", &["tbFluxoCaixaContas"]),
    ("bancos", &["tbBancos"]),
    ("bancos_contas", &["tbBancosContas", "vsBancosContas"]),
    ("centros_custo", &["tbzconfigCentroCusto"]),
    ("tes_movimentos", &["tbFlulxoaixaCabessalho", "tbFluxoCaixa"]),
    ("caixa_fechos", &["tbCaixaDiaria"]),
    ("moedas", &["tbzconfigMoeda"]),
    ("prazos", &["tbzconfigTiposPagamento"]),
    ("rh_alteracoes", &["tbRhAlteracoesMensais"]),
    ("rh_adiantamentos", &["tbRhAdiantamentoSalario"]),
    ("rh_proc_cab", &["tbRhProcessamentoCabessalho"]),
    ("rh_proc_linhas", &["tbRhProcessamentoRecibo"]),
    ("rh_honorarios", &["vsRhProcesamentoHonorarios", "tbRhProcesamentoHonorarios"]),
];

fn resolver(disponiveis: &[(String, i64)]) -> HashMap<&'static str, (String, i64)> {
    let mut m = HashMap::new();
    for (canon, candidatos) in TABELAS {
        for c in *candidatos {
            if let Some((nome, linhas)) = disponiveis.iter().find(|(nome, _)| nome.eq_ignore_ascii_case(c)) {
                m.insert(*canon, (nome.clone(), *linhas));
                break;
            }
        }
    }
    m
}

pub async fn analisar_fonte<F: Fonte>(fonte: &mut F) -> AppResult<Value> {
    let disp = fonte.tabelas().await?;
    let r = resolver(&disp);
    let cols = fonte.colunas().await.unwrap_or_default();
    let cols_de = |nome: &str| cols.iter().find(|(k, _)| k.eq_ignore_ascii_case(nome)).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut reconhecidas: Vec<Value> = TABELAS
        .iter()
        .map(|(canon, cand)| match r.get(canon) {
            Some((nome, linhas)) => json!({ "conjunto": canon, "tabela": nome, "registos": linhas, "colunas": cols_de(nome) }),
            None => json!({ "conjunto": canon, "tabela": null, "candidatos": cand }),
        })
        .collect();
    reconhecidas.sort_by_key(|v| v["tabela"].is_null());
    // tabelas com dados que o KanGest não importa (para rever com o suporte)
    let usadas: HashSet<String> = r.values().map(|(n, _)| n.to_lowercase()).collect();
    let mut outras: Vec<Value> = disp
        .iter()
        .filter(|(n, linhas)| *linhas > 0 && !usadas.contains(&n.to_lowercase()))
        .map(|(n, linhas)| json!({ "tabela": n, "registos": linhas, "colunas": cols_de(n) }))
        .collect();
    outras.sort_by(|a, b| a["tabela"].as_str().cmp(&b["tabela"].as_str()));
    Ok(json!({ "tabelas_total": disp.len(), "reconhecidas": reconhecidas, "nao_importadas": outras }))
}

pub async fn analisar(st: &AppState, c: ConexaoIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let mut fonte = SqlServer::ligar(&c).await?;
    analisar_fonte(&mut fonte).await
}

#[derive(Deserialize)]
pub struct ExecutarIn {
    conexao: ConexaoIn,
    opcoes: Opcoes,
}

pub async fn executar(st: &AppState, e: ExecutarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let mut fonte = SqlServer::ligar(&e.conexao).await?;
    let origem = format!("{}/{}", e.conexao.servidor, e.conexao.base);
    importar_dados(st, &mut fonte, &e.opcoes, &origem).await
}

/// N.º do certificado no texto «Processado por programa validado nº.123/AGT/2020» → «123/AGT/2020».
fn certificado_no_rodape(rodape: &str) -> Option<String> {
    let baixo = rodape.to_lowercase();
    if !baixo.contains("process") {
        return None;
    }
    let inicio = ["n.º", "nº.", "nº", "n°", "no."].iter().filter_map(|m| baixo.find(m).map(|p| p + m.len())).min()?;
    let resto: String = baixo[inicio..].trim_start_matches(['.', ' ', ':']).chars().take_while(|c| !c.is_whitespace()).collect();
    let resto = resto.trim_end_matches(['/', '.', ',']).to_uppercase();
    resto.chars().next().filter(|c| c.is_ascii_digit()).map(|_| resto)
}

// ------------------------------------------------------------------ importação
struct Mapas {
    taxas: HashMap<i64, (i64, f64)>, // id KIGEST → (id novo, %)
    unidades: HashMap<i64, i64>,
    familias: HashMap<i64, (i64, bool)>, // → (id novo, é serviço)
    subfamilias: HashMap<i64, i64>,
    armazens: HashMap<i64, i64>,
    artigos: HashMap<i64, i64>,
    artigos_ref: HashMap<String, i64>,
    // código KIGEST (número ou texto, ver `ent`) → id novo
    clientes: HashMap<String, i64>,
    fornecedores: HashMap<String, i64>,
    utilizadores: HashMap<i64, i64>,
}

#[derive(Default)]
struct Relatorio {
    contagens: Vec<(String, i64)>,
    avisos: Vec<String>,
    senhas: Vec<(String, String)>,
}

impl Relatorio {
    fn conta(&mut self, nome: &str, v: i64) {
        self.contagens.push((nome.to_string(), v));
    }
}

fn tipo_documento(abrev: Option<&str>, desc: Option<&str>, id: Option<i64>) -> Option<&'static str> {
    let a = abrev.unwrap_or("").trim().to_uppercase();
    let d = desc.unwrap_or("").to_lowercase();
    let compra = d.starts_with("v/") || d.contains("fornecedor") || matches!(id, Some(12..=20));
    let por_abrev = match a.as_str() {
        "FT" | "FA" if !compra => Some("FT"),
        "FR" | "VD" | "TV" if !compra => Some("FR"),
        "NC" if !compra => Some("NC"),
        "ND" if !compra => Some("ND"),
        "PP" | "FP" => Some("PP"),
        "OR" if !compra => Some("OR"),
        "NE" if !compra => Some("NE"),
        "GT" | "GR" if !compra => Some("GT"),
        "RC" | "RE" | "RG" if !compra => Some("RC"),
        "AF" => Some("AF"),
        _ => None,
    };
    if por_abrev.is_some() {
        return por_abrev;
    }
    let tem = |s: &str| d.contains(s);
    if matches!(id, Some(23)) || tem("entrada") {
        return Some("ES");
    }
    if matches!(id, Some(24)) || tem("saída") || tem("saida") || tem("consumo") {
        return Some(if tem("consumo") { "CI" } else { "SS" });
    }
    if matches!(id, Some(25)) || tem("transfer") {
        return Some("TS");
    }
    if compra {
        return Some(if tem("recibo") && tem("fact") || tem("recibo") && tem("fat") {
            "VFR"
        } else if tem("crédito") || tem("credito") {
            "VNC"
        } else if tem("débito") || tem("debito") {
            "VND"
        } else if tem("encomenda") {
            "VNE"
        } else if tem("orçamento") || tem("orcamento") {
            "VOR"
        } else if tem("guia") {
            "VGT"
        } else if tem("pagamento") || tem("recibo") {
            "VRC"
        } else {
            "FC"
        });
    }
    if tem("autofact") || tem("autofat") {
        Some("AF")
    } else if (tem("factura") || tem("fatura")) && tem("recibo") || tem("venda a dinheiro") {
        Some("FR")
    } else if tem("crédito") || tem("credito") {
        Some("NC")
    } else if tem("débito") || tem("debito") {
        Some("ND")
    } else if tem("proforma") || tem("pró-forma") {
        Some("PP")
    } else if tem("orçamento") || tem("orcamento") {
        Some("OR")
    } else if tem("encomenda") {
        Some("NE")
    } else if tem("guia") {
        Some("GT")
    } else if tem("recibo") {
        Some("RC")
    } else if tem("factura") || tem("fatura") {
        Some("FT")
    } else {
        None
    }
}

/// Série histórica e número: "FR 2026/15" → ("KFR2026", 15); "VD 1/4" → ("KVD1", 4).
/// O prefixo original entra na série para que tipos diferentes do KIGEST nunca colidam.
fn serie_numero(r: &str) -> (String, Option<i64>) {
    let (prefixo, resto) = r.split_once(' ').unwrap_or(("", r));
    let (serie, numero) = match resto.rsplit_once('/') {
        Some((s, num)) => (s.trim(), num.trim().parse().ok()),
        None => (resto.trim(), None),
    };
    let limpo: String = format!("K{prefixo}{serie}").chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    (limpo, numero)
}

fn logo_data_url(bytes: &[u8]) -> Option<String> {
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        return None;
    };
    (bytes.len() < 1_000_000).then(|| format!("data:{mime};base64,{}", B64.encode(bytes)))
}

async fn ler_opt<F: Fonte>(fonte: &mut F, r: &HashMap<&'static str, (String, i64)>, canon: &str) -> AppResult<Vec<Registo>> {
    match r.get(canon) {
        Some((nome, _)) => fonte.ler(nome).await,
        None => Ok(vec![]),
    }
}

pub async fn importar_dados<F: Fonte>(st: &AppState, fonte: &mut F, op: &Opcoes, origem: &str) -> AppResult<Value> {
    let sessao = st.exigir_perm("sistema.importar")?;
    let disp = fonte.tabelas().await?;
    let tabs = resolver(&disp);

    // 1) ler tudo da origem (antes de abrir a transacção local)
    let empresa = if op.empresa { ler_opt(fonte, &tabs, "empresa").await? } else { vec![] };
    let precisa_tabelas = op.artigos || op.documentos;
    let taxas = if precisa_tabelas { ler_opt(fonte, &tabs, "taxas").await? } else { vec![] };
    let motivos = if precisa_tabelas { ler_opt(fonte, &tabs, "motivos").await? } else { vec![] };
    let unidades = if precisa_tabelas { ler_opt(fonte, &tabs, "unidades").await? } else { vec![] };
    let familias = if precisa_tabelas { ler_opt(fonte, &tabs, "familias").await? } else { vec![] };
    let subfamilias = if precisa_tabelas { ler_opt(fonte, &tabs, "subfamilias").await? } else { vec![] };
    let armazens = ler_opt(fonte, &tabs, "armazens").await?;
    let artigos = if op.artigos || op.documentos || op.lotes || op.oficina { ler_opt(fonte, &tabs, "artigos").await? } else { vec![] };
    let stock = if op.stock { ler_opt(fonte, &tabs, "stock").await? } else { vec![] };
    let clientes = if op.clientes || op.documentos || op.oficina { ler_opt(fonte, &tabs, "clientes").await? } else { vec![] };
    let fornecedores = if op.fornecedores || op.documentos { ler_opt(fonte, &tabs, "fornecedores").await? } else { vec![] };
    let utilizadores = if op.utilizadores { ler_opt(fonte, &tabs, "utilizadores").await? } else { vec![] };
    let (tipos, docs, linhas, pagamentos, cc) = if op.documentos {
        (
            ler_opt(fonte, &tabs, "tipos").await?,
            ler_opt(fonte, &tabs, "documentos").await?,
            ler_opt(fonte, &tabs, "linhas").await?,
            ler_opt(fonte, &tabs, "pagamentos").await?,
            ler_opt(fonte, &tabs, "conta_corrente").await?,
        )
    } else {
        (vec![], vec![], vec![], vec![], vec![])
    };

    let mut extras = Extras::default();
    if op.rh {
        extras.departamentos = ler_opt(fonte, &tabs, "rh_departamentos").await?;
        extras.funcoes = ler_opt(fonte, &tabs, "rh_funcoes").await?;
        extras.categorias = ler_opt(fonte, &tabs, "rh_categorias").await?;
        extras.funcionarios = ler_opt(fonte, &tabs, "rh_funcionarios").await?;
        extras.agregado = ler_opt(fonte, &tabs, "rh_agregado").await?;
        extras.ferias = ler_opt(fonte, &tabs, "rh_ferias").await?;
    }
    if op.lotes {
        extras.lotes = ler_opt(fonte, &tabs, "lotes").await?;
    }
    if op.documentos {
        extras.recibos = ler_opt(fonte, &tabs, "recibos").await?;
        extras.recibo_linhas = ler_opt(fonte, &tabs, "recibo_linhas").await?;
        extras.recibo_pagamentos = ler_opt(fonte, &tabs, "recibo_pagamentos").await?;
        extras.retencoes = ler_opt(fonte, &tabs, "retencoes").await?;
        extras.custos = ler_opt(fonte, &tabs, "custos").await?;
        extras.tipos_custo = ler_opt(fonte, &tabs, "tipos_custo").await?;
        extras.proformas = ler_opt(fonte, &tabs, "proformas").await?;
        extras.proforma_linhas = ler_opt(fonte, &tabs, "proforma_linhas").await?;
    }
    if op.tesouraria {
        for (canon, destino) in [
            ("tes_contas", &mut extras.tes_contas),
            ("bancos", &mut extras.bancos),
            ("bancos_contas", &mut extras.bancos_contas),
            ("centros_custo", &mut extras.centros_custo),
            ("tes_movimentos", &mut extras.tes_movimentos),
            ("caixa_fechos", &mut extras.caixa_fechos),
            ("moedas", &mut extras.moedas),
            ("prazos", &mut extras.prazos),
        ] {
            *destino = ler_opt(fonte, &tabs, canon).await?;
        }
    }
    if op.rh {
        extras.rh_alteracoes = ler_opt(fonte, &tabs, "rh_alteracoes").await?;
        extras.rh_adiantamentos = ler_opt(fonte, &tabs, "rh_adiantamentos").await?;
        extras.rh_proc_cab = ler_opt(fonte, &tabs, "rh_proc_cab").await?;
        extras.rh_proc_linhas = ler_opt(fonte, &tabs, "rh_proc_linhas").await?;
        extras.rh_honorarios = ler_opt(fonte, &tabs, "rh_honorarios").await?;
    }
    if op.oficina {
        extras.viaturas = ler_opt(fonte, &tabs, "viaturas").await?;
        extras.ordens = ler_opt(fonte, &tabs, "ordens").await?;
        extras.ordem_linhas = ler_opt(fonte, &tabs, "ordem_linhas").await?;
    }

    let mut rel = Relatorio::default();
    let mut mapas = Mapas {
        taxas: HashMap::new(),
        unidades: HashMap::new(),
        familias: HashMap::new(),
        subfamilias: HashMap::new(),
        armazens: HashMap::new(),
        artigos: HashMap::new(),
        artigos_ref: HashMap::new(),
        clientes: HashMap::new(),
        fornecedores: HashMap::new(),
        utilizadores: HashMap::new(),
    };
    let _guard = st.emissao.lock().await;
    let mut tx = st.pool.begin().await?;

    // 2) empresa (+ certificado do sistema anterior, só como referência)
    if let Some(e) = empresa.first() {
        let bancos: Vec<Value> = (1..=3)
            .filter_map(|k| {
                let banco = t(e, &[&format!("Banco_{k}")])?;
                Some(json!({ "banco": banco, "conta": t(e, &[&format!("Conta_{k}")]), "iban": t(e, &[&format!("Iban_{k}")]).unwrap_or_default() }))
            })
            .collect();
        let logo = campo(e, &["Logo", "LogoMarca", "Logotipo"]).and_then(|v| if let Val::B(bts) = v { logo_data_url(bts) } else { None });
        sqlx::query(
            "UPDATE empresa SET nome = COALESCE(?1, nome), designacao_comercial = COALESCE(?2, designacao_comercial), nif = COALESCE(?3, nif),
               endereco = COALESCE(?4, endereco), provincia = COALESCE(?5, provincia), localidade = COALESCE(?6, localidade),
               telefone = COALESCE(?7, telefone), email = COALESCE(?8, email), rodape = COALESCE(?9, rodape),
               bancos = COALESCE(?10, bancos), logo = COALESCE(?11, logo) WHERE id = 1",
        )
        .bind(t(e, &["NomeCompany", "Nome", "RazaoSocial"]))
        .bind(t(e, &["DesignacaoComercial"]))
        .bind(t(e, &["NIF", "Contribuinte", "NIFCompany"]))
        .bind(t(e, &["Endereco", "Morada"]))
        .bind(t(e, &["Provincia"]))
        .bind(t(e, &["Localidade", "Cidade", "Municipio"]))
        .bind(t(e, &["Telefone", "Telenone", "Telemovel"]))
        .bind(t(e, &["Email"]))
        .bind(t(e, &["RodapeDocumento", "Rodape"]))
        .bind((!bancos.is_empty()).then(|| Value::Array(bancos).to_string()))
        .bind(logo)
        .execute(&mut *tx)
        .await?;
        // n.º do certificado: numa coluna própria ou no texto do rodapé («Processado por programa validado nº.123/»)
        let do_rodape = t(e, &["Rodape"]).and_then(|r| certificado_no_rodape(&r));
        if let Some(cert) = t(e, &["NumCertificado", "NumeroCertificado", "Certificado", "CertificadoAGT", "NCertificado", "NumValidacao", "SoftwareValidationNumber"]).or(do_rodape) {
            sqlx::query("INSERT INTO config (chave, valor) VALUES ('agt.certificado_anterior', ?) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor")
                .bind(&cert)
                .execute(&mut *tx)
                .await?;
            rel.avisos.push(format!(
                "Certificado AGT do KIGEST ({cert}) guardado como referência. Não foi activado no sistema novo: o certificado pertence ao produtor e ao programa certificado."
            ));
        }
        rel.conta("Empresa", 1);
    }

    // 3) tabelas auxiliares
    for r in &taxas {
        let Some(id) = i(r, &["Id", "IdTaxa"]) else { continue };
        let pct = n(r, &["Taxa", "Percentagem", "Valor"]).unwrap_or(0.0);
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM taxas_iva WHERE abs(percentagem - ?) < 0.0001 ORDER BY id LIMIT 1")
            .bind(pct)
            .fetch_optional(&mut *tx)
            .await?;
        let novo = match existente {
            Some(x) => x,
            None => sqlx::query("INSERT INTO taxas_iva (codigo, descricao, percentagem) VALUES (?, ?, ?)")
                .bind(if pct == 0.0 { "ISE" } else if pct >= 14.0 { "NOR" } else { "RED" })
                .bind(t(r, &["Descricao", "Descritivo"]).unwrap_or_else(|| format!("IVA {pct}%")))
                .bind(pct)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid(),
        };
        mapas.taxas.insert(id, (novo, pct));
    }
    for r in &motivos {
        if let Some(cod) = t(r, &["Codigo", "Id", "CodigoIsencao"]).filter(|c| c.to_uppercase().starts_with('M')) {
            sqlx::query("INSERT OR IGNORE INTO motivos_isencao (codigo, descricao) VALUES (?, ?)")
                .bind(cod.to_uppercase())
                .bind(t(r, &["Descricao", "Descritivo"]).unwrap_or_else(|| cod.clone()))
                .execute(&mut *tx)
                .await?;
        }
    }
    for r in &unidades {
        let Some(id) = i(r, &["IdUnidade", "Id"]) else { continue };
        let desc = t(r, &["Descricao", "Descritivo"]).unwrap_or_else(|| "Unidade".into());
        let abrev = t(r, &["Abreviatura", "Sigla", "Simbolo"]).unwrap_or_else(|| desc.chars().take(3).collect::<String>().to_uppercase());
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM unidades WHERE abreviatura = ? COLLATE NOCASE LIMIT 1")
            .bind(&abrev)
            .fetch_optional(&mut *tx)
            .await?;
        let novo = match existente {
            Some(x) => x,
            None => sqlx::query("INSERT INTO unidades (descricao, abreviatura) VALUES (?, ?)").bind(&desc).bind(&abrev).execute(&mut *tx).await?.last_insert_rowid(),
        };
        mapas.unidades.insert(id, novo);
    }
    for r in &familias {
        let Some(id) = i(r, &["IdFamilia", "Id"]) else { continue };
        let desc = t(r, &["Descricao", "Descritivo"]).unwrap_or_else(|| format!("Família {id}"));
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE descricao = ? COLLATE NOCASE LIMIT 1").bind(&desc).fetch_optional(&mut *tx).await?;
        let novo = match existente {
            Some(x) => x,
            None => sqlx::query("INSERT INTO familias (descricao) VALUES (?)").bind(&desc).execute(&mut *tx).await?.last_insert_rowid(),
        };
        mapas.familias.insert(id, (novo, desc.to_lowercase().contains("serv")));
    }
    for r in &subfamilias {
        let Some(id) = i(r, &["Id", "IdSubFamilia"]) else { continue };
        let fam = i(r, &["IdFamilia"]).and_then(|f| mapas.familias.get(&f).map(|x| x.0)).unwrap_or(1);
        let desc = t(r, &["Descricao", "Descritivo"]).unwrap_or_else(|| format!("Subfamília {id}"));
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM subfamilias WHERE familia_id = ? AND descricao = ? COLLATE NOCASE LIMIT 1")
            .bind(fam)
            .bind(&desc)
            .fetch_optional(&mut *tx)
            .await?;
        let novo = match existente {
            Some(x) => x,
            None => sqlx::query("INSERT INTO subfamilias (familia_id, descricao) VALUES (?, ?)").bind(fam).bind(&desc).execute(&mut *tx).await?.last_insert_rowid(),
        };
        mapas.subfamilias.insert(id, novo);
    }
    for r in &armazens {
        let Some(id) = i(r, &["IdArmazem", "Id"]) else { continue };
        let nome = t(r, &["NomeArmazem", "Descricao", "Nome"]).unwrap_or_else(|| format!("Armazém {id}"));
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM armazens WHERE nome = ? COLLATE NOCASE LIMIT 1").bind(&nome).fetch_optional(&mut *tx).await?;
        let novo = match existente {
            Some(x) => x,
            None => sqlx::query("INSERT INTO armazens (nome) VALUES (?)").bind(&nome).execute(&mut *tx).await?.last_insert_rowid(),
        };
        mapas.armazens.insert(id, novo);
    }

    // 4) artigos
    let mut n_art = 0;
    for r in &artigos {
        let id_k = i(r, &["IdArtigo", "Id"]);
        let referencia = t(r, &["Referencia", "Codigo"]).or_else(|| id_k.map(|x| format!("K{x}")));
        let Some(referencia) = referencia else { continue };
        let descricao = t(r, &["Descricao", "ArtigoDescricao"]).unwrap_or_else(|| referencia.clone());
        let (taxa_id, pct) = i(r, &["IdTaxa"]).and_then(|x| mapas.taxas.get(&x).copied()).unwrap_or((1, 14.0));
        let mut motivo = t(r, &["CodigoIsencao", "MotivoIsencao"]).map(|m| m.to_uppercase()).filter(|m| m.starts_with('M'));
        if pct == 0.0 && motivo.is_none() {
            motivo = Some("M00".into());
        }
        if let Some(m) = &motivo {
            sqlx::query("INSERT OR IGNORE INTO motivos_isencao (codigo, descricao) VALUES (?, ?)").bind(m).bind(m).execute(&mut *tx).await?;
        }
        let fam = i(r, &["IdFamilia"]).and_then(|f| mapas.familias.get(&f).copied());
        let servico = fam.map(|f| f.1).unwrap_or(false) || t(r, &["Tipo", "TipoArtigo"]).map(|x| x.to_uppercase().starts_with('S')).unwrap_or(false);
        let ativo = b(r, &["Ativo", "Activo"]).unwrap_or(true) && !b(r, &["Descontinuado"]).unwrap_or(false);
        let custo = kz(r, &["Custo", "PrecoCusto", "CustoMedio"]);
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ?").bind(&referencia).fetch_optional(&mut *tx).await?;
        let campos = (
            t(r, &["CoBar", "CodigoBarra", "CodBarra", "CodigoBarras"]),
            t(r, &["DescricaoCurta"]),
            if servico { "S" } else { "P" },
            fam.map(|f| f.0),
            i(r, &["IdSubFamilia"]).and_then(|s| mapas.subfamilias.get(&s).copied()),
            i(r, &["IdUnidadeVenda", "IdUnidade"]).and_then(|u| mapas.unidades.get(&u).copied()).or(Some(1)),
        );
        // No KIGEST as vendas abatem o stock mesmo com ControloStock = «N» (na base real, em 250 de 276 artigos
        // vendidos o stock é entradas − vendas): esse campo só decide se deixa vender sem stock.
        let controla = !servico;
        let negativo = b(r, &["ControloStock", "ControlaStock"]) == Some(false) || b(r, &["StockZerado", "PermiteStockNegativo"]).unwrap_or(false);
        let id = match existente {
            Some(id) => {
                sqlx::query("UPDATE artigos SET descricao = ?, preco1 = ?, preco2 = ?, preco3 = ?, preco4 = ?, custo = ?, taxa_iva_id = ?, motivo_isencao = ?, id_origem = ?, controla_stock = ?, permite_negativo = ? WHERE id = ?")
                    .bind(&descricao)
                    .bind(kz(r, &["Preco1"]))
                    .bind(kz(r, &["Preco2"]))
                    .bind(kz(r, &["Preco3"]))
                    .bind(kz(r, &["Preco4"]))
                    .bind(custo)
                    .bind(taxa_id)
                    .bind(&motivo)
                    .bind(id_k.map(|x| x.to_string()))
                    .bind(controla as i64)
                    .bind(negativo as i64)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                id
            }
            None => sqlx::query(
                "INSERT INTO artigos (referencia, codigo_barras, descricao, descricao_curta, tipo, familia_id, subfamilia_id, unidade_id,
                    taxa_iva_id, motivo_isencao, custo, custo_medio, preco1, preco2, preco3, preco4, qtd_minima, qtd_maxima,
                    controla_stock, permite_negativo, ativo, id_origem)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&referencia)
            .bind(&campos.0)
            .bind(&descricao)
            .bind(&campos.1)
            .bind(campos.2)
            .bind(campos.3)
            .bind(campos.4)
            .bind(campos.5)
            .bind(taxa_id)
            .bind(&motivo)
            .bind(custo)
            .bind(custo)
            .bind(kz(r, &["Preco1"]))
            .bind(kz(r, &["Preco2"]))
            .bind(kz(r, &["Preco3"]))
            .bind(kz(r, &["Preco4"]))
            .bind(n(r, &["QtdMinimo", "StockMinimo"]).unwrap_or(0.0))
            .bind(n(r, &["QtdMaximo", "StockMaximo"]).unwrap_or(0.0))
            .bind(controla as i64)
            .bind(negativo as i64)
            .bind(ativo as i64)
            .bind(id_k.map(|x| x.to_string()))
            .execute(&mut *tx)
            .await?
            .last_insert_rowid(),
        };
        // promoção do KIGEST (Promocao + PromoData_ini/PromoData_Fim + Desconto % ou ValDesconto em Kz)
        if b(r, &["Promocao"]).unwrap_or(false) {
            let pct = n(r, &["Desconto", "DescontoPromocao"]).filter(|x| *x > 0.0).or_else(|| {
                let v = n(r, &["ValDesconto"]).unwrap_or(0.0);
                let base = n(r, &["Preco1Taxa"]).or_else(|| n(r, &["Preco1"])).unwrap_or(0.0);
                (v > 0.0 && base > 0.0).then(|| (v / base * 10000.0).round() / 100.0)
            });
            if let Some(pct) = pct.filter(|p| *p > 0.0 && *p <= 100.0) {
                sqlx::query("UPDATE artigos SET promo_desconto = ?, promo_inicio = ?, promo_fim = ? WHERE id = ?")
                    .bind(pct)
                    .bind(data(r, &["PromoData_ini", "PromoDataIni", "PromoInicio"]))
                    .bind(data(r, &["PromoData_Fim", "PromoDataFim", "PromoFim"]))
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        if let Some(k) = id_k {
            mapas.artigos.insert(k, id);
        }
        mapas.artigos_ref.insert(referencia.to_lowercase(), id);
        n_art += 1;
    }
    if !artigos.is_empty() {
        rel.conta("Artigos", n_art);
    }

    // 5) stock (existências por armazém) — saldo inicial
    if op.stock {
        let agora = db::agora();
        let mut n_stk = 0;
        // (artigo, armazém, quantidade, custo médio, última entrada, última saída) — as datas são as do KIGEST
        let mut itens: Vec<(i64, i64, f64, Option<i64>, Option<String>, Option<String>)> = Vec::new();
        if !stock.is_empty() {
            for r in &stock {
                let artigo = i(r, &["IdArtigo"])
                    .and_then(|x| mapas.artigos.get(&x).copied())
                    .or_else(|| t(r, &["Referencia"]).and_then(|x| mapas.artigos_ref.get(&x.to_lowercase()).copied()));
                let Some(artigo) = artigo else { continue };
                let armazem = i(r, &["IdArmazem"]).and_then(|x| mapas.armazens.get(&x).copied()).unwrap_or(1);
                let qtd = n(r, &["QtdExistente", "Quantidade", "Stock", "Qtd"]).unwrap_or(0.0);
                let cm = n(r, &["CustoMedio"]).map(|v| (v * 100.0).round() as i64);
                // no KIGEST vêm só com o dia («2026-10-23») ou com dia e hora
                let quando = |c: &str| data_hora(r, &[c]).or_else(|| data(r, &[c]).map(|d| format!("{d}T00:00:00")));
                itens.push((artigo, armazem, qtd, cm, quando("UltimaEntrada"), quando("UltimaSaida")));
            }
        } else {
            for r in &artigos {
                let artigo = i(r, &["IdArtigo", "Id"]).and_then(|x| mapas.artigos.get(&x).copied());
                if let (Some(artigo), Some(qtd)) = (artigo, n(r, &["QtdStock", "Stock"])) {
                    itens.push((artigo, 1, qtd, None, None, None));
                }
            }
        }
        for (artigo, armazem, qtd, cm, entrada, saida) in itens {
            let atual: f64 = sqlx::query_scalar("SELECT quantidade FROM stock WHERE artigo_id = ? AND armazem_id = ?")
                .bind(artigo)
                .bind(armazem)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or(0.0);
            sqlx::query(
                "INSERT INTO stock (artigo_id, armazem_id, quantidade, ultima_entrada, ultima_saida) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(artigo_id, armazem_id) DO UPDATE SET quantidade = ?3,
                    ultima_entrada = COALESCE(?6, ultima_entrada), ultima_saida = COALESCE(?5, ultima_saida)",
            )
            .bind(artigo)
            .bind(armazem)
            .bind(qtd)
            .bind(entrada.clone().unwrap_or_else(|| agora.clone()))
            .bind(&saida)
            .bind(&entrada)
            .execute(&mut *tx)
            .await?;
            if let Some(cm) = cm.filter(|c| *c > 0) {
                sqlx::query("UPDATE artigos SET custo_medio = ? WHERE id = ?").bind(cm).bind(artigo).execute(&mut *tx).await?;
            }
            if (qtd - atual).abs() > 1e-9 {
                sqlx::query(
                    "INSERT INTO movimentos_stock (data_hora, artigo_id, armazem_id, quantidade, saldo, custo, descricao)
                     VALUES (?, ?, ?, ?, ?, (SELECT custo_medio FROM artigos WHERE id = ?), 'Saldo inicial importado do KIGEST')",
                )
                .bind(&agora)
                .bind(artigo)
                .bind(armazem)
                .bind(qtd - atual)
                .bind(qtd)
                .bind(artigo)
                .execute(&mut *tx)
                .await?;
            }
            n_stk += 1;
        }
        rel.conta("Existências de stock", n_stk);
    }

    // 6) clientes e fornecedores
    for (tipo, regs, ativo_op) in [("C", &clientes, op.clientes), ("F", &fornecedores, op.fornecedores)] {
        let mut n_ent = 0;
        for r in regs.iter() {
            let id_k = ent(r, &["IdCliente", "IdFornecedor", "IdEntidade", "Id", "Codigo"]);
            let Some(nome) = t(r, &["Nome", "NomeCliente", "NomeFornecedor", "Designacao", "EntidadeNome"]) else { continue };
            let nif = t(r, &["NIF", "Contribuinte", "NContribuinte", "NumContribuinte"]).unwrap_or_else(|| "999999999".into());
            let consumidor = nif == "999999999" && nome.to_lowercase().contains("consumidor");
            let id = if consumidor {
                1
            } else if !ativo_op {
                continue;
            } else {
                let existente: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM entidades WHERE (tipo = ?1 OR tipo = 'A') AND ((nif = ?2 AND nif <> '999999999') OR (nif = '999999999' AND nome = ?3)) LIMIT 1",
                )
                .bind(tipo)
                .bind(&nif)
                .bind(&nome)
                .fetch_optional(&mut *tx)
                .await?;
                match existente {
                    Some(x) => x,
                    None => {
                        n_ent += 1;
                        sqlx::query(
                            "INSERT INTO entidades (tipo, nome, nif, endereco, localidade, provincia, telefone, email, desconto, limite_credito, id_origem)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(tipo)
                        .bind(&nome)
                        .bind(&nif)
                        .bind(t(r, &["Endereco", "Morada"]))
                        .bind(t(r, &["Localidade", "Cidade", "Municipio"]))
                        .bind(t(r, &["Provincia"]))
                        .bind(t(r, &["Telefone", "Telemovel", "Contacto"]))
                        .bind(t(r, &["Email"]))
                        .bind(n(r, &["Desconto"]).unwrap_or(0.0).clamp(0.0, 100.0))
                        .bind(kz(r, &["LimiteCredito", "Plafond"]))
                        .bind(id_k.as_ref().map(|x| format!("{tipo}:{x}")))
                        .execute(&mut *tx)
                        .await?
                        .last_insert_rowid()
                    }
                }
            };
            if let Some(k) = id_k {
                if tipo == "C" {
                    mapas.clientes.insert(k, id);
                } else {
                    mapas.fornecedores.insert(k, id);
                }
            }
        }
        if ativo_op && !regs.is_empty() {
            rel.conta(if tipo == "C" { "Clientes" } else { "Fornecedores" }, n_ent);
        }
    }

    // 7) utilizadores (senhas provisórias — as do KIGEST não são reaproveitáveis)
    if op.utilizadores {
        let mut n_u = 0;
        for r in &utilizadores {
            let Some(login) = t(r, &["Usuario", "Login", "NomeUsuario", "Utilizador"]) else { continue };
            let id_k = i(r, &["IdUsuario", "Id"]);
            let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM utilizadores WHERE login = ? COLLATE NOCASE").bind(&login).fetch_optional(&mut *tx).await?;
            let id = match existente {
                Some(x) => x,
                None => {
                    let senha: String = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
                    let admin = login.to_lowercase().starts_with("admin");
                    let pin = t(r, &["CodPin", "Pin"]).filter(|p| (4..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_digit()));
                    // senha provisória: obrigatório mudá-la no primeiro acesso
                    let id = sqlx::query("INSERT INTO utilizadores (nome, login, senha_hash, perfil, ativo, pin_hash, id_origem, mudar_senha) VALUES (?, ?, ?, ?, ?, ?, ?, 1)")
                        .bind(t(r, &["Nome", "NomeCompleto"]).unwrap_or_else(|| login.clone()))
                        .bind(&login)
                        .bind(hash_senha(&senha)?)
                        .bind(if admin { "ADMIN" } else { "CAIXA" })
                        .bind(b(r, &["Ativo", "Activo", "Status"]).unwrap_or(true) as i64)
                        .bind(match &pin {
                            Some(p) => Some(hash_senha(p)?),
                            None => None,
                        })
                        .bind(id_k.map(|x| x.to_string()))
                        .execute(&mut *tx)
                        .await?
                        .last_insert_rowid();
                    rel.senhas.push((login.clone(), senha));
                    n_u += 1;
                    id
                }
            };
            if let Some(k) = id_k {
                mapas.utilizadores.insert(k, id);
            }
        }
        if !utilizadores.is_empty() {
            rel.conta("Utilizadores", n_u);
            if n_u > 0 {
                rel.avisos.push("Os utilizadores importados receberam senhas provisórias (listadas abaixo) e o perfil CAIXA. Reveja perfis e permissões.".into());
            }
        }
    }

    // 8) documentos (histórico com assinaturas originais) + valores em dívida
    if op.documentos && !docs.is_empty() {
        n_docs(&mut tx, &mut rel, &mapas, &tipos, &docs, &linhas, &pagamentos, &cc, sessao.id).await?;
        // O sistema novo continua em séries próprias (evita colisões com a numeração do KIGEST).
        let prefixo: String = sqlx::query_scalar("SELECT serie_prefixo FROM postos WHERE id = 1").fetch_one(&mut *tx).await?;
        if prefixo.is_empty() {
            sqlx::query("UPDATE postos SET serie_prefixo = 'N' WHERE id = 1").execute(&mut *tx).await?;
            rel.avisos.push("As novas séries do Posto#1 passam a usar o prefixo \"N\" (ex.: FR N2026/1) para não colidirem com a numeração do KIGEST.".into());
        }
    }

    // 9) recursos humanos, lotes e oficina
    n_rh(&mut tx, &mut rel, &extras).await?;
    n_lotes(&mut tx, &mut rel, &mapas, &extras.lotes).await?;
    n_oficina(&mut tx, &mut rel, &mapas, &extras, sessao.id).await?;
    // 10) recibos, retenções e proformas (histórico), tesouraria e caixa, salários processados
    n_recibos(&mut tx, &mut rel, &mapas, &extras, sessao.id).await?;
    n_retencoes(&mut tx, &mut rel, &extras.retencoes).await?;
    n_custos(&mut tx, &mut rel, &extras).await?;
    n_proformas(&mut tx, &mut rel, &mapas, &extras, sessao.id).await?;
    n_tesouraria(&mut tx, &mut rel, &mapas, &extras, sessao.id).await?;
    n_rh_historico(&mut tx, &mut rel, &extras, sessao.id).await?;

    let conferencia = conferir(&mut tx, op, &mapas, &artigos, &stock, &clientes, &fornecedores, &docs, &cc, &extras).await?;
    let relatorio = json!({
        "origem": origem,
        "contagens": rel.contagens.iter().map(|(k, v)| json!({ "conjunto": k, "registos": v })).collect::<Vec<_>>(),
        "avisos": rel.avisos,
        "conferencia": conferencia,
    });
    sqlx::query("INSERT INTO importacoes (data_hora, origem, relatorio) VALUES (?, ?, ?)")
        .bind(db::agora())
        .bind(origem)
        .bind(relatorio.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'IMPORTAR', ?)").bind(sessao.id).bind(origem).execute(&mut *tx).await?;
    if op.simular {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    drop(_guard);
    if op.documentos && !docs.is_empty() && !op.simular {
        st.carregar_posto(st.posto().id).await?;
    }

    let mut saida = relatorio;
    saida["simulacao"] = json!(op.simular);
    if op.simular {
        // numa simulação não há utilizadores criados: não se mostram senhas
        rel.senhas.clear();
    }
    saida["senhas"] = json!(rel.senhas.iter().map(|(l, s)| json!({ "login": l, "senha": s })).collect::<Vec<_>>());
    Ok(saida)
}

/// Conferência depois de importar: o total no KIGEST ao lado do que dele ficou no KanGest
/// (só os registos vindos do KIGEST; os que já existiam no KanGest não contam).
/// `unidade`: "n" (registos), "qtd" (quantidade) ou "kz" (cêntimos).
#[allow(clippy::too_many_arguments)]
async fn conferir(
    tx: &mut Transaction<'_, Sqlite>,
    op: &Opcoes,
    mapas: &Mapas,
    artigos: &[Registo],
    stock: &[Registo],
    clientes: &[Registo],
    fornecedores: &[Registo],
    docs: &[Registo],
    cc: &[Registo],
    x: &Extras,
) -> AppResult<Vec<Value>> {
    let mut v = Vec::new();
    let item = |nome: &str, unidade: &str, kigest: Value, novo: Value| json!({ "item": nome, "unidade": unidade, "kigest": kigest, "kigest_novo": novo });
    async fn escalar(tx: &mut Transaction<'_, Sqlite>, sql: &str) -> AppResult<i64> {
        Ok(sqlx::query_scalar::<_, Option<i64>>(sql).fetch_one(&mut **tx).await?.unwrap_or(0))
    }
    if op.artigos && !artigos.is_empty() {
        v.push(item("Artigos", "n", json!(artigos.len()), json!(escalar(tx, "SELECT COUNT(*) FROM artigos WHERE id_origem IS NOT NULL").await?)));
    }
    let qtds: Vec<f64> = if stock.is_empty() {
        artigos.iter().filter_map(|r| n(r, &["QtdStock", "Stock"])).collect()
    } else {
        stock.iter().filter_map(|r| n(r, &["QtdExistente", "Quantidade", "Stock", "Qtd"])).collect()
    };
    if op.stock && !qtds.is_empty() {
        // «+ 0.0»: a soma de nada em f64 é −0.0
        let origem: f64 = qtds.iter().sum::<f64>() + 0.0;
        let novo: f64 = sqlx::query_scalar::<_, Option<f64>>("SELECT SUM(quantidade) FROM stock WHERE artigo_id IN (SELECT id FROM artigos WHERE id_origem IS NOT NULL)")
            .fetch_one(&mut **tx)
            .await?
            .unwrap_or(0.0)
            + 0.0;
        v.push(item("Quantidade total em stock", "qtd", json!(origem), json!(novo)));
    }
    if op.clientes && !clientes.is_empty() {
        v.push(item("Clientes", "n", json!(clientes.len()), json!(mapas.clientes.len())));
    }
    if op.fornecedores && !fornecedores.is_empty() {
        v.push(item("Fornecedores", "n", json!(fornecedores.len()), json!(mapas.fornecedores.len())));
    }
    if op.documentos && !docs.is_empty() {
        let existentes: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT numero_doc FROM documentos WHERE importado = 1").fetch_all(&mut **tx).await?.into_iter().collect();
        let refs: Vec<String> = docs.iter().filter_map(|d| t(d, &["RefDocumento", "NumDocumento"])).collect();
        let presentes = refs.iter().filter(|r| existentes.contains(*r)).count();
        v.push(item("Documentos", "n", json!(refs.len()), json!(presentes)));
        if !cc.is_empty() {
            let divida: i64 = cc.iter().map(|r| kz(r, &["ValorFalta", "Pendente", "Saldo"])).filter(|x| *x > 0).sum();
            v.push(item("Valores em dívida (conta corrente)", "kz", json!(divida), json!(escalar(tx, "SELECT SUM(valor_pendente) FROM documentos WHERE importado = 1").await?)));
        }
    }
    if op.tesouraria && !x.tes_movimentos.is_empty() {
        let saldo: i64 = x.tes_movimentos.iter().filter_map(movimento_tesouraria).map(|(tipo, valor)| if tipo == "ENTRADA" { valor } else { -valor }).sum();
        let novo = escalar(tx, "SELECT SUM(CASE WHEN tipo = 'ENTRADA' THEN valor ELSE -valor END) FROM movimentos_tesouraria WHERE id_origem IS NOT NULL AND anulado = 0").await?;
        v.push(item("Saldo das contas de tesouraria", "kz", json!(saldo), json!(novo)));
    }
    if op.rh && !x.funcionarios.is_empty() {
        v.push(item("Funcionários", "n", json!(x.funcionarios.len()), json!(escalar(tx, "SELECT COUNT(*) FROM funcionarios WHERE id_origem IS NOT NULL").await?)));
    }
    Ok(v)
}

#[allow(clippy::too_many_arguments)]
async fn n_docs(
    tx: &mut Transaction<'_, Sqlite>,
    rel: &mut Relatorio,
    mapas: &Mapas,
    tipos: &[Registo],
    docs: &[Registo],
    linhas: &[Registo],
    pagamentos: &[Registo],
    cc: &[Registo],
    utilizador_padrao: i64,
) -> AppResult<()> {
    // tipos do KIGEST → códigos do sistema novo
    let mut tipo_por_id: HashMap<i64, &'static str> = HashMap::new();
    for r in tipos {
        if let (Some(id), Some(cod)) = (
            i(r, &["Id", "IdTipoDocumento"]),
            tipo_documento(t(r, &["Abreviatura", "Sigla", "Codigo", "Prefixo"]).as_deref(), t(r, &["Descricao", "Descritivo"]).as_deref(), i(r, &["Id"])),
        ) {
            tipo_por_id.insert(id, cod);
        }
    }
    let classes: HashMap<String, (String, String, i64)> = sqlx::query("SELECT codigo, classe, grupo, conta_corrente FROM tipos_documento")
        .fetch_all(&mut **tx)
        .await?
        .iter()
        .map(|r| (r.get::<String, _>("codigo"), (r.get::<String, _>("classe"), r.get::<String, _>("grupo"), r.get::<i64, _>("conta_corrente"))))
        .collect();

    let mut linhas_por_doc: HashMap<String, Vec<&Registo>> = HashMap::new();
    for l in linhas {
        if let Some(rf) = t(l, &["RefDocumento", "Documento"]) {
            linhas_por_doc.entry(rf).or_default().push(l);
        }
    }
    let mut pag_por_doc: HashMap<String, Vec<&Registo>> = HashMap::new();
    for p in pagamentos {
        if let Some(rf) = t(p, &["RefDocumento", "Documento"]) {
            pag_por_doc.entry(rf).or_default().push(p);
        }
    }
    let pendente: HashMap<String, i64> = cc
        .iter()
        .filter_map(|r| Some((t(r, &["Documento", "RefDocumento"])?, kz(r, &["ValorFalta", "Pendente", "Saldo"]))))
        .collect();

    let mut ignorados: HashSet<String> = HashSet::new();
    let mut colisoes: Vec<String> = Vec::new();
    let (mut n_ok, mut n_pend, mut n_corrigidos) = (0i64, 0i64, 0i64);
    for d in docs {
        let Some(rf) = t(d, &["RefDocumento", "NumDocumento"]) else { continue };
        let cod = i(d, &["IdTipoDocumento", "TipoDocumento"])
            .and_then(|x| tipo_por_id.get(&x).copied())
            .or_else(|| tipo_documento(rf.split_whitespace().next(), None, None));
        let Some(cod) = cod else {
            ignorados.insert(rf.split_whitespace().next().unwrap_or("?").to_string());
            continue;
        };
        let (_classe, grupo, conta_corrente) = classes.get(cod).cloned().unwrap_or(("VENDA".into(), "VENDAS".into(), 0));
        let ent_k = ent(d, &["IdEntidade", "IdCliente", "IdFornecedor", "Entidade"]);
        let entidade = match grupo.as_str() {
            "COMPRAS" => ent_k.and_then(|k| mapas.fornecedores.get(&k).copied()),
            "STOCK" => None,
            _ => ent_k.and_then(|k| mapas.clientes.get(&k).copied()).or(Some(1)),
        };
        let data_emissao = data(d, &["DataEmissao", "Data"]).unwrap_or_else(db::hoje);
        let dh = data_hora(d, &["DataHoraEmissao", "DataHora"]).unwrap_or_else(|| format!("{data_emissao}T00:00:00"));
        let (serie, numero) = serie_numero(&rf);
        let total = kz(d, &["ValTotal", "Total", "ValTotal_Com_Imposto"]);
        let iva = kz(d, &["ValTaxa", "Imposto", "TotalImposto"]);
        let liquido = n(d, &["ValLiquido"]).map(|v| (v * 100.0).round() as i64).unwrap_or(total - iva);
        // O KIGEST guarda o total sem imposto antes dos descontos (ValSemTaxa), o desconto das linhas
        // (ValDescontoLinha) e o desconto global (ValDesconto) em separado.
        let (bruto, desconto) = match n(d, &["ValSemTaxa"]).map(|v| (v * 100.0).round() as i64).filter(|b| *b >= liquido) {
            Some(b) => (b, b - liquido),
            None => {
                let desc = kz(d, &["ValDescontoLinha"]) + kz(d, &["ValDesconto", "TotalDeconto", "TotalDesconto"]);
                (liquido + desc, desc)
            }
        };
        // No KIGEST o documento anulado tem EstadoDocsTotal ≠ 0 (os relatórios filtram «EstadoDocsTotal=0»).
        // O EstadoDocumento não serve: há bases em que vale «A» em todos os documentos, anulados ou não.
        let anulado = match b(d, &["EstadoDocsTotal", "Anulado", "CanceladoStatus"]) {
            Some(x) => x,
            None => t(d, &["Estado", "Status", "StatusDocumento"]).map(|s| {
                let s = s.to_uppercase();
                s == "A" || s.starts_with("ANUL")
            }).unwrap_or(false),
        };
        let hash = t(d, &["Hash", "HashDocumento", "Assinatura"]).unwrap_or_default();
        let controlo = t(d, &["AssinaturaCurta", "HashControlo"]).unwrap_or_default();
        let pend = if conta_corrente == 1 && !anulado { pendente.get(&rf).copied().unwrap_or(0).max(0) } else { 0 };
        let existe: Option<i64> = sqlx::query_scalar("SELECT importado FROM documentos WHERE numero_doc = ?").bind(&rf).fetch_optional(&mut **tx).await?;
        if let Some(importado) = existe {
            if importado == 0 {
                // número já usado por um documento emitido no KanGest
                colisoes.push(rf.clone());
            } else {
                // repetição da importação: acerta o que versões anteriores do importador liam mal (estado anulado,
                // descontos, cliente com código em texto, valor em dívida). A dívida só se repõe se o documento
                // ainda não teve recibos no KanGest.
                let r = sqlx::query(
                    "UPDATE documentos SET estado = ?1, total_bruto = ?2, total_desconto = ?3, entidade_id = COALESCE(?4, entidade_id),
                        valor_pendente = CASE WHEN EXISTS (SELECT 1 FROM recibo_documentos r WHERE r.documento_id = documentos.id) THEN valor_pendente ELSE ?5 END
                     WHERE numero_doc = ?6 AND importado = 1
                       AND (estado <> ?1 OR total_bruto <> ?2 OR total_desconto <> ?3 OR IFNULL(entidade_id, 0) <> IFNULL(COALESCE(?4, entidade_id), 0)
                            OR (valor_pendente <> ?5 AND NOT EXISTS (SELECT 1 FROM recibo_documentos r WHERE r.documento_id = documentos.id)))",
                )
                .bind(if anulado { "A" } else { "N" })
                .bind(bruto)
                .bind(desconto)
                .bind(entidade)
                .bind(pend)
                .bind(&rf)
                .execute(&mut **tx)
                .await?;
                n_corrigidos += r.rows_affected() as i64;
            }
            continue;
        }
        let doc_id = sqlx::query(
            "INSERT INTO documentos (tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, data_vencimento, entidade_id,
                entidade_nome, entidade_nif, entidade_endereco, utilizador_id, armazem_id, estado, total_bruto, total_desconto,
                total_liquido, total_iva, total, valor_entregue, troco, valor_pendente, hash, hash_controlo, observacoes, motivo,
                referencia, importado)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(cod)
        .bind(&serie)
        .bind(numero.unwrap_or(n_ok + 1))
        .bind(&rf)
        .bind(&data_emissao)
        .bind(&dh)
        .bind(data(d, &["DataVencimento"]))
        .bind(entidade)
        .bind(t(d, &["Nome", "NomeEntidade", "EntidadeNome"]).unwrap_or_else(|| "Consumidor Final".into()))
        .bind(t(d, &["NIF", "Contribuinte"]).unwrap_or_else(|| "999999999".into()))
        .bind(t(d, &["Endereco", "Morada"]))
        .bind(i(d, &["IdUsuario"]).and_then(|u| mapas.utilizadores.get(&u).copied()).unwrap_or(utilizador_padrao))
        .bind(i(d, &["IdArmazem", "ArmazemOrigem"]).and_then(|a| mapas.armazens.get(&a).copied()))
        .bind(if anulado { "A" } else { "N" })
        .bind(bruto)
        .bind(desconto)
        .bind(liquido)
        .bind(iva)
        .bind(total)
        .bind(kz(d, &["ValEntregue"]))
        .bind(kz(d, &["ValTroco"]))
        .bind(pend)
        .bind(&hash)
        .bind(&controlo)
        .bind(t(d, &["Obs", "Observacao", "Observacoes"]))
        .bind(t(d, &["MotivoEmissao", "Motivo"]))
        .bind(t(d, &["RefObra", "Referencia"]))
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
        if pend > 0 {
            n_pend += 1;
        }

        inserir_linhas(tx, mapas, doc_id, linhas_por_doc.get(&rf).map(|v| v.as_slice()).unwrap_or(&[])).await?;
        for p in pag_por_doc.get(&rf).map(|v| v.as_slice()).unwrap_or(&[]) {
            let valor = kz(p, &["Valor"]);
            if valor <= 0 {
                continue;
            }
            let modo = match i(p, &["IdModopagamento", "IdModoPagamento", "Modo"]) {
                Some(1) => "NU",
                Some(2) => "MB",
                Some(3) | Some(4) => "TB",
                _ => "OU",
            };
            sqlx::query("INSERT INTO documento_pagamentos (documento_id, modo_codigo, valor) VALUES (?, ?, ?)").bind(doc_id).bind(modo).bind(valor).execute(&mut **tx).await?;
        }
        n_ok += 1;
    }
    rel.conta("Documentos (histórico)", n_ok);
    if n_corrigidos > 0 {
        rel.conta("Documentos já importados corrigidos", n_corrigidos);
    }
    if n_pend > 0 {
        rel.conta("Documentos com valor em dívida", n_pend);
    }
    if !colisoes.is_empty() {
        let exemplos = colisoes.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        rel.avisos.push(format!(
            "{} documento(s) do KIGEST têm o mesmo número que documentos já emitidos no KanGest e não foram importados (ex.: {exemplos}).",
            colisoes.len()
        ));
    }
    if !ignorados.is_empty() {
        let mut v: Vec<_> = ignorados.into_iter().collect();
        v.sort();
        rel.avisos.push(format!("Documentos ignorados por tipo desconhecido: {}", v.join(", ")));
    }
    Ok(())
}


// ------------------------------------------------------------------ RH, lotes e oficina
#[derive(Default)]
struct Extras {
    departamentos: Vec<Registo>,
    funcoes: Vec<Registo>,
    categorias: Vec<Registo>,
    funcionarios: Vec<Registo>,
    agregado: Vec<Registo>,
    ferias: Vec<Registo>,
    lotes: Vec<Registo>,
    viaturas: Vec<Registo>,
    ordens: Vec<Registo>,
    ordem_linhas: Vec<Registo>,
    recibos: Vec<Registo>,
    recibo_linhas: Vec<Registo>,
    recibo_pagamentos: Vec<Registo>,
    retencoes: Vec<Registo>,
    custos: Vec<Registo>,
    tipos_custo: Vec<Registo>,
    proformas: Vec<Registo>,
    proforma_linhas: Vec<Registo>,
    tes_contas: Vec<Registo>,
    bancos: Vec<Registo>,
    bancos_contas: Vec<Registo>,
    centros_custo: Vec<Registo>,
    tes_movimentos: Vec<Registo>,
    caixa_fechos: Vec<Registo>,
    moedas: Vec<Registo>,
    prazos: Vec<Registo>,
    rh_alteracoes: Vec<Registo>,
    rh_adiantamentos: Vec<Registo>,
    rh_proc_cab: Vec<Registo>,
    rh_proc_linhas: Vec<Registo>,
    rh_honorarios: Vec<Registo>,
}

/// Tabela auxiliar do RH (departamentos, funções, categorias): id KIGEST → id novo (por descrição).
async fn rh_tabela(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, tabela: &str, regs: &[Registo]) -> AppResult<HashMap<i64, i64>> {
    let mut m = HashMap::new();
    for r in regs {
        let Some(desc) = t(r, &["Descritivo", "Descricao", "Departamento", "Funcao", "Categoria", "Nome"]) else { continue };
        let id = rh_descricao(tx, tabela, &desc).await?;
        if let Some(k) = i(r, &["Id", "ID"]) {
            m.insert(k, id);
        }
    }
    Ok(m)
}

async fn rh_descricao(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, tabela: &str, desc: &str) -> AppResult<i64> {
    let existe: Option<i64> = sqlx::query_scalar(&format!("SELECT id FROM {tabela} WHERE lower(descricao) = lower(?)")).bind(desc.trim()).fetch_optional(&mut **tx).await?;
    Ok(match existe {
        Some(id) => id,
        None => sqlx::query(&format!("INSERT INTO {tabela} (descricao) VALUES (?)")).bind(desc.trim()).execute(&mut **tx).await?.last_insert_rowid(),
    })
}

fn contrato(c: Option<String>) -> &'static str {
    let c = c.unwrap_or_default().to_lowercase();
    if c.contains("indeter") {
        "Indeterminado"
    } else if c.contains("determ") || c.contains("prazo") {
        "Determinado"
    } else if c.contains("est") && c.contains("gi") {
        "Estágio"
    } else if c.contains("prest") || c.contains("servi") {
        "Prestação de serviços"
    } else {
        "Indeterminado"
    }
}

async fn n_rh(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, rel: &mut Relatorio, x: &Extras) -> AppResult<()> {
    if x.funcionarios.is_empty() {
        return Ok(());
    }
    let deps = rh_tabela(tx, "rh_departamentos", &x.departamentos).await?;
    let funs = rh_tabela(tx, "rh_funcoes", &x.funcoes).await?;
    let cats = rh_tabela(tx, "rh_categorias", &x.categorias).await?;
    let hoje = db::hoje();
    let mut func_map: HashMap<String, i64> = HashMap::new();
    let mut n_f = 0;
    for r in &x.funcionarios {
        let Some(nome) = t(r, &["Nome", "NomeFuncionario"]) else { continue };
        let origem = t(r, &["Id", "IdFuncionario", "Codigo"]).unwrap_or_else(|| nome.clone());
        // departamento / função / categoria: pelo id da tabela auxiliar ou pelo texto
        let aux = |mapa: &HashMap<i64, i64>, ids: &[&str], textos: &[&str]| -> (Option<i64>, Option<String>) { (i(r, ids).and_then(|k| mapa.get(&k).copied()), t(r, textos)) };
        let (dep_id, dep_txt) = aux(&deps, &["Id_Departamento"], &["Departamento"]);
        let (fun_id, fun_txt) = aux(&funs, &["Id_Funcao"], &["Funcao", "Profissao"]);
        let (cat_id, cat_txt) = aux(&cats, &["Id_categoria", "Id_Categoria"], &["Categoria"]);
        let dep = match (dep_id, dep_txt) {
            (Some(v), _) => Some(v),
            (None, Some(d)) => Some(rh_descricao(tx, "rh_departamentos", &d).await?),
            _ => None,
        };
        let fun = match (fun_id, fun_txt) {
            (Some(v), _) => Some(v),
            (None, Some(d)) => Some(rh_descricao(tx, "rh_funcoes", &d).await?),
            _ => None,
        };
        let cat = match (cat_id, cat_txt) {
            (Some(v), _) => Some(v),
            (None, Some(d)) => Some(rh_descricao(tx, "rh_categorias", &d).await?),
            _ => None,
        };
        let saida = data(r, &["Termino_Funcoes", "DataSaida"]);
        let ativo = b(r, &["Ativa_Salario", "Ativo"]).unwrap_or(true) && saida.as_deref().map(|d| d >= hoje.as_str()).unwrap_or(true);
        let processamento = t(r, &["TipoProcessamento"]).unwrap_or_default().to_lowercase();
        let independente = ["própria", "propria", "diarista", "independente"].iter().any(|k| processamento.contains(k));
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM funcionarios WHERE id_origem = ?").bind(&origem).fetch_optional(&mut **tx).await?;
        let q = if existente.is_some() {
            "UPDATE funcionarios SET numero=?1, nome=?2, sexo=?3, data_nascimento=?4, estado_civil=?5, nacionalidade=?6, naturalidade=?7, bi_numero=?8,
               bi_local_emissao=?9, bi_validade=?10, nif=?11, inss_numero=?12, morada=?13, localidade=?14, municipio=?15, provincia=?16, telefone=?17,
               email=?18, departamento_id=?19, funcao_id=?20, categoria_id=?21, tipo_contrato=?22, data_admissao=?23, data_saida=?24, motivo_saida=?25,
               salario_base=?26, sub_alimentacao=?27, sub_transporte=?28, sub_chefia=?29, sub_turno=?30, abono_falhas=?31, sub_arrendamento=?32,
               sub_atavio=?33, sub_isencao_horario=?34, sub_deslocacao=?35, desconta_inss=?36, desconta_irt=?37, iban=?38, ativo=?39, independente=?40
             WHERE id_origem = ?41"
        } else {
            "INSERT INTO funcionarios (numero, nome, sexo, data_nascimento, estado_civil, nacionalidade, naturalidade, bi_numero, bi_local_emissao,
               bi_validade, nif, inss_numero, morada, localidade, municipio, provincia, telefone, email, departamento_id, funcao_id, categoria_id,
               tipo_contrato, data_admissao, data_saida, motivo_saida, salario_base, sub_alimentacao, sub_transporte, sub_chefia, sub_turno,
               abono_falhas, sub_arrendamento, sub_atavio, sub_isencao_horario, sub_deslocacao, desconta_inss, desconta_irt, iban, ativo,
               independente, id_origem)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
               ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41)"
        };
        sqlx::query(q)
            .bind(&origem)
            .bind(&nome)
            .bind(t(r, &["Sexo"]))
            .bind(data(r, &["DataNascimento"]))
            .bind(t(r, &["EstadoCivil"]))
            .bind(t(r, &["Nacionalidade"]))
            .bind(t(r, &["Naturalidade"]))
            .bind(t(r, &["N_BI", "BI", "NumeroBI"]))
            .bind(t(r, &["LocalEmissao"]))
            .bind(data(r, &["BI_DataValidade"]))
            .bind(t(r, &["NIF"]))
            .bind(t(r, &["NumeroINSS", "INSS"]))
            .bind(t(r, &["Morada", "Endereco"]))
            .bind(t(r, &["Localidade"]))
            .bind(t(r, &["Municipio"]))
            .bind(t(r, &["Provincia"]))
            .bind(t(r, &["Telefone", "Telemovel"]))
            .bind(t(r, &["Email"]))
            .bind(dep)
            .bind(fun)
            .bind(cat)
            .bind(contrato(t(r, &["Contrato", "TipoContrato"])))
            .bind(data(r, &["Inicio_Funcoes", "DataAdmissao", "DataCadastro"]))
            .bind(&saida)
            .bind(t(r, &["MotivoFimContrato"]))
            .bind(kz(r, &["Salario_Base", "SalarioBase"]))
            .bind(kz(r, &["Subsidio_Alimentacao"]))
            .bind(kz(r, &["Subsidio_Transporte"]))
            .bind(kz(r, &["Sub_Chefia"]))
            .bind(kz(r, &["Sub_Turno"]))
            .bind(kz(r, &["Sub_Falhas"]))
            .bind(kz(r, &["Sub_Arrendamento"]))
            .bind(kz(r, &["SubsidioAtavio"]))
            .bind(kz(r, &["SubsidioIsencaoHorario"]))
            .bind(kz(r, &["SubsidioDeslocacao"]))
            .bind(b(r, &["Desconto_INSS"]).unwrap_or(true) as i64)
            .bind(b(r, &["Desconto_IRT"]).unwrap_or(true) as i64)
            .bind(t(r, &["Func_pagto_IBAN", "IBAN"]))
            .bind(ativo as i64)
            .bind(independente as i64)
            .bind(&origem)
            .execute(&mut **tx)
            .await?;
        let id: i64 = sqlx::query_scalar("SELECT id FROM funcionarios WHERE id_origem = ?").bind(&origem).fetch_one(&mut **tx).await?;
        func_map.insert(origem.to_lowercase(), id);
        n_f += 1;
    }
    rel.conta("Funcionários", n_f);

    // agregado familiar (substitui o existente de cada funcionário importado)
    let mut n_a = 0;
    let mut limpos = std::collections::HashSet::new();
    for r in &x.agregado {
        let Some(fid) = t(r, &["IdFuncionario"]).and_then(|k| func_map.get(&k.to_lowercase()).copied()) else { continue };
        let Some(nome) = t(r, &["NomeAgregado", "Nome"]) else { continue };
        if limpos.insert(fid) {
            sqlx::query("DELETE FROM rh_agregado WHERE funcionario_id = ?").bind(fid).execute(&mut **tx).await?;
        }
        sqlx::query("INSERT INTO rh_agregado (funcionario_id, nome, parentesco, data_nascimento) VALUES (?, ?, ?, ?)")
            .bind(fid)
            .bind(nome)
            .bind(t(r, &["Parentesco", "GrauParentesco"]))
            .bind(data(r, &["DataNascimento"]))
            .execute(&mut **tx)
            .await?;
        n_a += 1;
    }
    sqlx::query("UPDATE funcionarios SET dependentes = (SELECT COUNT(*) FROM rh_agregado a WHERE a.funcionario_id = funcionarios.id) WHERE id_origem IS NOT NULL")
        .execute(&mut **tx)
        .await?;
    if !x.agregado.is_empty() {
        rel.conta("Agregado familiar", n_a);
    }

    // férias: um período por funcionário, data inicial e final (o mapa do KIGEST repete-o por mês)
    let mut n_fe = 0;
    let mut vistos = std::collections::HashSet::new();
    for r in &x.ferias {
        let Some(fid) = t(r, &["IdFuncionario"]).and_then(|k| func_map.get(&k.to_lowercase()).copied()) else { continue };
        let (Some(ini), Some(fim)) = (data(r, &["DataInicial", "Ferias_Inicio"]), data(r, &["DataFinal", "Ferias_Termino"])) else { continue };
        if !vistos.insert((fid, ini.clone(), fim.clone())) {
            continue;
        }
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_ferias WHERE funcionario_id = ? AND data_inicio = ? AND data_fim = ?").bind(fid).bind(&ini).bind(&fim).fetch_one(&mut **tx).await?;
        if ja > 0 {
            continue;
        }
        let dias = i(r, &["Dias"]).unwrap_or_else(|| {
            let a = chrono::NaiveDate::parse_from_str(&ini, "%Y-%m-%d");
            let b = chrono::NaiveDate::parse_from_str(&fim, "%Y-%m-%d");
            match (a, b) {
                (Ok(a), Ok(b)) => (b - a).num_days() + 1,
                _ => 0,
            }
        });
        sqlx::query("INSERT INTO rh_ferias (funcionario_id, ano, data_inicio, data_fim, dias, estado, observacoes) VALUES (?, ?, ?, ?, ?, ?, 'Importado do KIGEST')")
            .bind(fid)
            .bind(i(r, &["Ano"]).unwrap_or_else(|| ini[..4].parse().unwrap_or(0)))
            .bind(&ini)
            .bind(&fim)
            .bind(dias)
            .bind(if fim.as_str() < hoje.as_str() { "GOZADA" } else { "MARCADA" })
            .execute(&mut **tx)
            .await?;
        n_fe += 1;
    }
    if !x.ferias.is_empty() {
        rel.conta("Férias", n_fe);
    }
    rel.avisos.push("Funcionários importados: confirme subsídios, IBAN e a tabela de IRT na Configuração Salarial antes do primeiro processamento.".into());
    Ok(())
}

async fn artigo_k(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, mapas: &Mapas, r: &Registo) -> AppResult<Option<i64>> {
    if let Some(k) = i(r, &["IdArtigo"]) {
        if let Some(id) = mapas.artigos.get(&k) {
            return Ok(Some(*id));
        }
        if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM artigos WHERE id_origem = ?").bind(k.to_string()).fetch_optional(&mut **tx).await? {
            return Ok(Some(id));
        }
    }
    if let Some(rf) = t(r, &["Referencia", "ReferenciaArtigo"]) {
        if let Some(id) = mapas.artigos_ref.get(&rf.to_lowercase()) {
            return Ok(Some(*id));
        }
        return Ok(sqlx::query_scalar("SELECT id FROM artigos WHERE lower(referencia) = lower(?)").bind(rf).fetch_optional(&mut **tx).await?);
    }
    Ok(None)
}

async fn n_lotes(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, rel: &mut Relatorio, mapas: &Mapas, lotes: &[Registo]) -> AppResult<()> {
    if lotes.is_empty() {
        return Ok(());
    }
    let mut n_l = 0;
    let mut tocados: Vec<(i64, i64, i64)> = Vec::new(); // (artigo, armazém, dias de aviso)
    for r in lotes {
        let Some(artigo) = artigo_k(tx, mapas, r).await? else { continue };
        let Some(lote) = t(r, &["Lote"]) else { continue };
        let armazem = i(r, &["IdArmazem"]).and_then(|a| mapas.armazens.get(&a).copied()).unwrap_or(1);
        let qtd = n(r, &["Qtd", "Quantidade"]).unwrap_or(0.0);
        sqlx::query(
            "INSERT INTO lotes (artigo_id, armazem_id, lote, data_fabrico, validade, quantidade, saldo_inicial, custo, ultima_entrada, ultima_saida, ativo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(artigo_id, armazem_id, lote) DO UPDATE SET
               saldo_inicial = ?6, data_fabrico = COALESCE(?4, data_fabrico), validade = COALESCE(?5, validade),
               custo = CASE WHEN ?7 > 0 THEN ?7 ELSE custo END,
               quantidade = ?6 + COALESCE((SELECT SUM(m.quantidade) FROM lote_movimentos m WHERE m.lote_id = lotes.id), 0)",
        )
        .bind(artigo)
        .bind(armazem)
        .bind(&lote)
        .bind(data(r, &["DataFabrico"]))
        .bind(data(r, &["DataValidade", "Validade"]))
        .bind(qtd)
        .bind(kz(r, &["PrecoCusto", "CustoMedio"]))
        .bind(data_hora(r, &["UltimaEntrada"]))
        .bind(data_hora(r, &["UltimaSaida"]))
        .bind(b(r, &["Status", "Ativo"]).unwrap_or(true) as i64)
        .execute(&mut **tx)
        .await?;
        tocados.push((artigo, armazem, i(r, &["DiasAviso"]).unwrap_or(30)));
        n_l += 1;
    }
    rel.conta("Lotes", n_l);
    // O artigo passa a ser controlado por lotes só quando, em todos os armazéns, a soma dos lotes
    // é igual ao stock (senão a saída pelo lote que expira primeiro falharia).
    let mut artigos: Vec<(i64, i64)> = tocados.iter().map(|x| (x.0, x.2)).collect();
    artigos.sort();
    artigos.dedup_by_key(|x| x.0);
    let mut diferentes = 0;
    for (artigo, aviso) in artigos {
        let difs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (
               SELECT armazem_id, SUM(q_lotes) AS l, SUM(q_stock) AS s FROM (
                 SELECT armazem_id, quantidade AS q_lotes, 0 AS q_stock FROM lotes WHERE artigo_id = ?1
                 UNION ALL SELECT armazem_id, 0, quantidade FROM stock WHERE artigo_id = ?1)
               GROUP BY armazem_id HAVING ABS(l - s) > 0.000001)",
        )
        .bind(artigo)
        .fetch_one(&mut **tx)
        .await?;
        if difs == 0 {
            sqlx::query("UPDATE artigos SET controla_lote = 1, aviso_validade_dias = ? WHERE id = ? AND composto = 0").bind(aviso.clamp(0, 3650)).bind(artigo).execute(&mut **tx).await?;
        } else {
            diferentes += 1;
        }
    }
    if diferentes > 0 {
        rel.avisos.push(format!(
            "{diferentes} artigo(s) têm lotes cuja soma não coincide com o stock: os lotes foram importados, mas o controlo por lote ficou desligado nesses artigos (ver Lotes de Artigo)."
        ));
    }
    Ok(())
}

async fn cliente_k(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, mapas: &Mapas, k: Option<String>) -> AppResult<Option<i64>> {
    let Some(k) = k else { return Ok(None) };
    if let Some(id) = mapas.clientes.get(&k) {
        return Ok(Some(*id));
    }
    Ok(sqlx::query_scalar("SELECT id FROM entidades WHERE id_origem = ?").bind(format!("C:{k}")).fetch_optional(&mut **tx).await?)
}

fn estado_os(estado: Option<String>, facturada: bool) -> &'static str {
    if facturada {
        return "FACTURADA";
    }
    let e = estado.unwrap_or_default().to_lowercase();
    if e.contains("cancel") || e.contains("anul") {
        "CANCELADA"
    } else if e.contains("conclu") || e.contains("termin") || e.contains("fech") || e.contains("entreg") {
        "CONCLUIDA"
    } else if e.contains("curso") || e.contains("andamento") || e.contains("repar") {
        "EM_CURSO"
    } else {
        "ABERTA"
    }
}

async fn n_oficina(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, rel: &mut Relatorio, mapas: &Mapas, x: &Extras, utilizador: i64) -> AppResult<()> {
    let mut viat: HashMap<String, i64> = HashMap::new();
    let mut n_v = 0;
    for r in &x.viaturas {
        let Some(origem) = t(r, &["IdViatura", "Id"]) else { continue };
        let marca = t(r, &["Marca"]).unwrap_or_else(|| "—".into());
        let entidade = cliente_k(tx, mapas, ent(r, &["IdCliente"])).await?;
        let existente: Option<i64> = sqlx::query_scalar("SELECT id FROM of_viaturas WHERE id_origem = ?").bind(&origem).fetch_optional(&mut **tx).await?;
        let campos = (t(r, &["Matricula"]), t(r, &["Modelo"]), t(r, &["Cor"]), t(r, &["NumeroChassi", "Chassi"]), t(r, &["NumeroSerie", "Motor"]), t(r, &["DescritivoCombustivel", "Combustivel"]), t(r, &["Tipo", "TipoRegisto"]));
        let km = i(r, &["KilometragenIncial", "Kilometragem", "Km"]).unwrap_or(0);
        let id = match existente {
            Some(id) => {
                sqlx::query("UPDATE of_viaturas SET entidade_id = COALESCE(?, entidade_id), matricula = ?, marca = ?, modelo = ?, cor = ?, chassi = ?, motor = ?, combustivel = ?, categoria = ?, km = MAX(km, ?) WHERE id = ?")
                    .bind(entidade).bind(&campos.0).bind(&marca).bind(&campos.1).bind(&campos.2).bind(&campos.3).bind(&campos.4).bind(&campos.5).bind(&campos.6).bind(km).bind(id)
                    .execute(&mut **tx)
                    .await?;
                id
            }
            None => sqlx::query("INSERT INTO of_viaturas (entidade_id, matricula, marca, modelo, cor, chassi, motor, combustivel, categoria, km, observacoes, id_origem) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(entidade).bind(&campos.0).bind(&marca).bind(&campos.1).bind(&campos.2).bind(&campos.3).bind(&campos.4).bind(&campos.5).bind(&campos.6).bind(km)
                .bind(t(r, &["Descricao", "Observacao"]))
                .bind(&origem)
                .execute(&mut **tx)
                .await?
                .last_insert_rowid(),
        };
        viat.insert(origem, id);
        n_v += 1;
    }
    if !x.viaturas.is_empty() {
        rel.conta("Viaturas", n_v);
    }

    let mut n_o = 0;
    let mut sem_viatura = 0;
    for r in &x.ordens {
        let rf = t(r, &["RefDocumento", "NumeroDocumento"]).or_else(|| t(r, &["IdOrdemServico"]).map(|k| format!("OS {k}")));
        let Some(rf) = rf else { continue };
        let viatura = match t(r, &["IdViatura"]) {
            Some(k) => match viat.get(&k) {
                Some(v) => Some(*v),
                None => sqlx::query_scalar::<_, i64>("SELECT id FROM of_viaturas WHERE id_origem = ?").bind(&k).fetch_optional(&mut **tx).await?,
            },
            None => None,
        };
        let Some(viatura) = viatura else {
            sem_viatura += 1;
            continue;
        };
        let doc_assoc = t(r, &["DocVenvaAssociado", "DocVendaAssociado"]);
        let documento: Option<i64> = match &doc_assoc {
            Some(d) => sqlx::query_scalar("SELECT id FROM documentos WHERE numero_doc = ?").bind(d).fetch_optional(&mut **tx).await?,
            None => None,
        };
        let novo = sqlx::query(
            "INSERT INTO of_ordens (numero, data_entrada, viatura_id, entidade_id, km_entrada, km_saida, referencia, diagnostico, tecnico, estado, documento_id, observacoes, utilizador_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Importada do KIGEST', ?)
             ON CONFLICT(numero) DO NOTHING",
        )
        .bind(&rf)
        .bind(data(r, &["DataEmissao", "Data"]).unwrap_or_else(db::hoje))
        .bind(viatura)
        .bind(cliente_k(tx, mapas, ent(r, &["IdCliente"])).await?)
        .bind(i(r, &["KilometragenIncial", "KmUnidadeInical"]))
        .bind(i(r, &["KilometragenFinal", "KmUnidadeFinal"]))
        .bind(doc_assoc.clone())
        .bind(t(r, &["Diagnostico"]))
        .bind(t(r, &["NomeUsuario", "Tecnico"]))
        .bind(estado_os(t(r, &["Estado"]), documento.is_some()))
        .bind(documento)
        .bind(utilizador)
        .execute(&mut **tx)
        .await?;
        if novo.rows_affected() == 0 {
            continue; // já importada
        }
        let ordem = novo.last_insert_rowid();
        for l in x.ordem_linhas.iter().filter(|l| t(l, &["RefDocumento"]).as_deref() == Some(rf.as_str())) {
            let Some(desc) = t(l, &["Descricao", "DescritivoPecas"]) else { continue };
            sqlx::query("INSERT INTO of_ordem_linhas (ordem_id, artigo_id, descricao, quantidade) VALUES (?, ?, ?, ?)")
                .bind(ordem)
                .bind(artigo_k(tx, mapas, l).await?)
                .bind(desc)
                .bind(n(l, &["Qtd", "Quantidade"]).unwrap_or(1.0))
                .execute(&mut **tx)
                .await?;
        }
        n_o += 1;
    }
    if !x.ordens.is_empty() {
        rel.conta("Ordens de serviço", n_o);
    }
    if sem_viatura > 0 {
        rel.avisos.push(format!("{sem_viatura} ordem(ns) de serviço sem viatura correspondente não foram importadas."));
    }
    Ok(())
}

/// Linhas de um documento histórico (documentos e proformas do KIGEST).
async fn inserir_linhas(tx: &mut Transaction<'_, Sqlite>, mapas: &Mapas, doc_id: i64, linhas: &[&Registo]) -> AppResult<()> {
    for (k, l) in linhas.iter().enumerate() {
        let artigo = i(l, &["IdArtigo"])
            .and_then(|x| mapas.artigos.get(&x).copied())
            .or_else(|| t(l, &["Referencia"]).and_then(|x| mapas.artigos_ref.get(&x.to_lowercase()).copied()));
        let taxa = i(l, &["IdTaxa"]).and_then(|x| mapas.taxas.get(&x).copied());
        let pct = n(l, &["Taxaporcento", "Taxa", "TaxaPercentagem"]).or(taxa.map(|x| x.1)).unwrap_or(0.0);
        let qtd = n(l, &["Qtd", "Quantidade"]).unwrap_or(1.0);
        let liq = n(l, &["ValorLiquido"]).map(|v| (v * 100.0).round() as i64);
        let total_l = kz(l, &["ValorTotal", "Total"]);
        let iva_l = n(l, &["ValorTaxa"]).map(|v| (v * 100.0).round() as i64).unwrap_or(0);
        sqlx::query(
            "INSERT INTO documento_linhas (documento_id, linha, artigo_id, referencia, descricao, quantidade, unidade, preco_unitario,
                desconto_pct, valor_desconto, taxa_iva_id, taxa_iva_pct, motivo_isencao, valor_liquido, valor_iva, total, custo)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(doc_id)
        .bind(k as i64 + 1)
        .bind(artigo)
        .bind(t(l, &["Referencia"]).unwrap_or_default())
        .bind(t(l, &["ArtigoDescricao", "Descricao"]).unwrap_or_default())
        .bind(qtd)
        .bind(t(l, &["Unidade"]))
        .bind(kz(l, &["ValorUnit", "PrecoUnitario", "Preco"]))
        .bind(n(l, &["DescontoPercento", "Desconto"]).unwrap_or(0.0))
        .bind(kz(l, &["DescontoTotal", "ValDesconto"]))
        .bind(taxa.map(|x| x.0))
        .bind(pct)
        .bind(t(l, &["CodigoIsencao"]))
        .bind(liq.unwrap_or(total_l - iva_l))
        .bind(iva_l)
        .bind(total_l)
        .bind(kz(l, &["CustoMedio", "Custo"]))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}


// ------------------------------------------------------------------ recibos, retenções, proformas
fn modo_de_texto(t: &str) -> &'static str {
    let t = t.to_lowercase();
    if t.contains("numer") || t.contains("dinheiro") || t.contains("cash") {
        "NU"
    } else if t.contains("multicaixa") || t.contains("tpa") || t.contains("cart") || t.contains("débito") || t.contains("debito") {
        "MB"
    } else if t.contains("transf") || t.contains("banc") || t.contains("depós") || t.contains("depos") {
        "TB"
    } else if t.contains("cheq") {
        "CH"
    } else {
        "OU"
    }
}

async fn entidade_k(tx: &mut Transaction<'_, Sqlite>, mapas: &Mapas, fornecedor: bool, k: Option<String>) -> AppResult<Option<i64>> {
    let Some(k) = k else { return Ok(None) };
    let m = if fornecedor { &mapas.fornecedores } else { &mapas.clientes };
    if let Some(id) = m.get(&k) {
        return Ok(Some(*id));
    }
    Ok(sqlx::query_scalar("SELECT id FROM entidades WHERE id_origem = ?").bind(format!("{}:{k}", if fornecedor { "F" } else { "C" })).fetch_optional(&mut **tx).await?)
}

async fn n_recibos(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, mapas: &Mapas, x: &Extras, utilizador: i64) -> AppResult<()> {
    if x.recibos.is_empty() {
        return Ok(());
    }
    let chave = |r: &Registo| t(r, &["DocumentoReferencia", "RefRecibo", "NumeroRecibo", "RefDocumento"]);
    let mut linhas: HashMap<String, Vec<&Registo>> = HashMap::new();
    for l in &x.recibo_linhas {
        if let Some(k) = chave(l) {
            linhas.entry(k).or_default().push(l);
        }
    }
    let mut pags: HashMap<String, Vec<&Registo>> = HashMap::new();
    for p in &x.recibo_pagamentos {
        if let Some(k) = t(p, &["RefDocumento", "NumeroRecibo"]) {
            pags.entry(k).or_default().push(p);
        }
    }
    let (mut n_r, mut sem_doc) = (0, 0);
    for r in &x.recibos {
        let Some(rf) = t(r, &["NumeroRecibo", "RefDocumento", "NumDocumento"]) else { continue };
        if sqlx::query_scalar::<_, i64>("SELECT id FROM documentos WHERE numero_doc = ?").bind(&rf).fetch_optional(&mut **tx).await?.is_some() {
            continue;
        }
        let prefixo = rf.split_whitespace().next().unwrap_or("");
        let fornecedor = t(r, &["TipoCliente", "TipoEntidade"]).map(|x| x.to_lowercase().starts_with('f')).unwrap_or(false)
            || prefixo.to_uppercase().starts_with('V')
            || t(r, &["TipoDocumento"]).map(|x| x.to_lowercase().contains("fornec")).unwrap_or(false);
        let tipo = if fornecedor { "VRC" } else { "RC" };
        let entidade = entidade_k(tx, mapas, fornecedor, ent(r, &["EntidadeId", "IdEntidade", "IdCliente", "IdFornecedor"])).await?;
        let (nome, nif): (String, String) = match entidade {
            Some(e) => sqlx::query_as("SELECT nome, nif FROM entidades WHERE id = ?").bind(e).fetch_one(&mut **tx).await?,
            None => (t(r, &["Nome", "EntidadeNome"]).unwrap_or_else(|| "Consumidor Final".into()), t(r, &["NIF"]).unwrap_or_else(|| "999999999".into())),
        };
        let data_r = data(r, &["Data", "DataEmissao"]).unwrap_or_else(db::hoje);
        let dh = data_hora(r, &["Data_Hora_Emissao", "DataHoraEmissao"]).unwrap_or_else(|| format!("{data_r}T00:00:00"));
        let total = kz(r, &["Total", "ValorTotal", "ValTotal"]);
        let retencao = kz(r, &["TotalRetencao"]);
        let anulado = b(r, &["CanceladoStatus", "Anulado"]).unwrap_or(false)
            || t(r, &["Estado", "Status"]).map(|e| e.to_uppercase().starts_with('A')).unwrap_or(false);
        let (serie, numero) = serie_numero(&rf);
        let doc_id = sqlx::query(
            "INSERT INTO documentos (tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, entidade_id, entidade_nome, entidade_nif,
                utilizador_id, estado, total_bruto, total_desconto, total_liquido, total_iva, total, total_retencao, valor_pendente, hash, hash_controlo,
                motivo, importado, origem)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, 0, ?, ?, 0, '', '', ?, 1, 'I')",
        )
        .bind(tipo)
        .bind(&serie)
        .bind(numero.unwrap_or(n_r as i64 + 1))
        .bind(&rf)
        .bind(&data_r)
        .bind(&dh)
        .bind(entidade)
        .bind(&nome)
        .bind(&nif)
        .bind(i(r, &["IdUsuario"]).and_then(|u| mapas.utilizadores.get(&u).copied()).unwrap_or(utilizador))
        .bind(if anulado { "A" } else { "N" })
        .bind(total)
        .bind(total)
        .bind(total)
        .bind(retencao)
        .bind(t(r, &["MotivoAlteracao"]))
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
        for l in linhas.get(&rf).map(|v| v.as_slice()).unwrap_or(&[]) {
            let origem = t(l, &["DocumentoOrigem", "RefFactura", "DocumentoLiquidado", "Documento"]).filter(|o| *o != rf);
            let Some(origem) = origem else { continue };
            let valor = n(l, &["ValorLiquido", "Valor", "ValorPago"])
                .map(|v| (v * 100.0).round() as i64)
                .unwrap_or_else(|| kz(l, &["TotalsemTaxa"]) + kz(l, &["TotalTaxa"]));
            match sqlx::query_scalar::<_, i64>("SELECT id FROM documentos WHERE numero_doc = ?").bind(&origem).fetch_optional(&mut **tx).await? {
                Some(doc) if valor > 0 => {
                    sqlx::query("INSERT INTO recibo_documentos (recibo_id, documento_id, valor) VALUES (?, ?, ?)").bind(doc_id).bind(doc).bind(valor).execute(&mut **tx).await?;
                }
                Some(_) => {}
                None => sem_doc += 1,
            }
        }
        let meios = pags.get(&rf).map(|v| v.as_slice()).unwrap_or(&[]);
        if meios.is_empty() {
            if total > 0 {
                let modo = t(r, &["MeioPagto", "ModoPagto"]).map(|m| modo_de_texto(&m)).unwrap_or("NU");
                sqlx::query("INSERT INTO documento_pagamentos (documento_id, modo_codigo, valor) VALUES (?, ?, ?)").bind(doc_id).bind(modo).bind(total - retencao).execute(&mut **tx).await?;
            }
        } else {
            for p in meios {
                let valor = kz(p, &["Valor", "ValorTotal"]);
                if valor > 0 {
                    let modo = t(p, &["ModoPagto", "Descricao"]).map(|m| modo_de_texto(&m)).unwrap_or("OU");
                    sqlx::query("INSERT INTO documento_pagamentos (documento_id, modo_codigo, valor) VALUES (?, ?, ?)").bind(doc_id).bind(modo).bind(valor).execute(&mut **tx).await?;
                }
            }
        }
        n_r += 1;
    }
    rel.conta("Recibos (histórico)", n_r);
    if sem_doc > 0 {
        rel.avisos.push(format!("{sem_doc} linha(s) de recibo referem documentos que não foram encontrados; o recibo foi importado sem essa ligação."));
    }
    Ok(())
}

async fn n_retencoes(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, regs: &[Registo]) -> AppResult<()> {
    if regs.is_empty() {
        return Ok(());
    }
    let mut n_ret = 0;
    for r in regs {
        let (Some(rf), Some(codigo)) = (t(r, &["RefDocumento", "Documento"]), t(r, &["Codigo", "Tipo"])) else { continue };
        let codigo = codigo.trim().to_uppercase();
        let valor = kz(r, &["Valor", "ValorRetencao"]);
        if valor <= 0 {
            continue;
        }
        // só nos documentos importados do KIGEST (os do sistema novo têm as suas próprias retenções)
        let Some(doc) = sqlx::query_scalar::<_, i64>("SELECT id FROM documentos WHERE numero_doc = ? AND importado = 1").bind(&rf).fetch_optional(&mut **tx).await? else { continue };
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documento_retencoes WHERE documento_id = ? AND codigo = ?").bind(doc).bind(&codigo).fetch_one(&mut **tx).await?;
        if ja > 0 {
            continue;
        }
        let pct = n(r, &["Percentagem", "Taxa"]).unwrap_or(0.0);
        let base = if pct > 0.0 { (valor as f64 * 100.0 / pct).round() as i64 } else { 0 };
        sqlx::query("INSERT INTO documento_retencoes (documento_id, codigo, descricao, percentagem, base, valor) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(doc)
            .bind(&codigo)
            .bind(t(r, &["Motivo", "Descricao"]).unwrap_or_else(|| codigo.clone()))
            .bind(pct)
            .bind(base)
            .bind(valor)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE documentos SET total_retencao = total_retencao + ? WHERE id = ?").bind(valor).bind(doc).execute(&mut **tx).await?;
        n_ret += 1;
    }
    rel.conta("Retenções na fonte", n_ret);
    Ok(())
}

/// Custos adicionais das compras (frete, seguro, despacho…), em Kz, nas compras importadas.
async fn n_custos(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, x: &Extras) -> AppResult<()> {
    if x.custos.is_empty() {
        return Ok(());
    }
    let nomes: HashMap<String, String> = x
        .tipos_custo
        .iter()
        .filter_map(|r| Some((t(r, &["Codigo", "Id"])?, t(r, &["Designacao", "Descricao", "Descritivo"])?)))
        .collect();
    let (mut n_c, mut sem_doc) = (0, 0);
    for r in &x.custos {
        let Some(rf) = t(r, &["RefDocumento", "Documento"]) else { continue };
        let valor = match kz(r, &["Contravalor", "ContraValor"]) {
            0 => kz(r, &["Valor"]),
            v => v,
        };
        if valor <= 0 {
            continue;
        }
        let Some(doc) = sqlx::query_scalar::<_, i64>("SELECT id FROM documentos WHERE numero_doc = ? AND importado = 1").bind(&rf).fetch_optional(&mut **tx).await? else {
            sem_doc += 1;
            continue;
        };
        let codigo = t(r, &["Cod_TipoCusto", "CodTipoCusto", "TipoCusto"]).unwrap_or_default();
        let descricao = nomes.get(&codigo).cloned().or_else(|| t(r, &["Designacao", "Descricao"])).unwrap_or_else(|| if codigo.is_empty() { "Custo adicional".into() } else { codigo.clone() });
        let d = descricao.to_lowercase();
        let tipo = if d.contains("frete") {
            "FRETE"
        } else if d.contains("seguro") {
            "SEGURO"
        } else if d.contains("alfand") || d.contains("aduan") {
            "DESALF"
        } else if d.contains("despach") {
            "DESPACHO"
        } else if d.contains("transp") {
            "TRANSP"
        } else {
            "OUTROS"
        };
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documento_custos WHERE documento_id = ? AND descricao = ? AND valor = ?")
            .bind(doc)
            .bind(&descricao)
            .bind(valor)
            .fetch_one(&mut **tx)
            .await?;
        if ja > 0 {
            continue;
        }
        sqlx::query("INSERT INTO documento_custos (documento_id, tipo, descricao, valor) VALUES (?, ?, ?, ?)")
            .bind(doc)
            .bind(tipo)
            .bind(&descricao)
            .bind(valor)
            .execute(&mut **tx)
            .await?;
        n_c += 1;
    }
    rel.conta("Custos de importação", n_c);
    if sem_doc > 0 {
        rel.avisos.push(format!("{sem_doc} custo(s) adicional(is) referem compras que não foram importadas."));
    }
    Ok(())
}

async fn n_proformas(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, mapas: &Mapas, x: &Extras, utilizador: i64) -> AppResult<()> {
    if x.proformas.is_empty() {
        return Ok(());
    }
    let mut linhas: HashMap<String, Vec<&Registo>> = HashMap::new();
    for l in &x.proforma_linhas {
        if let Some(k) = t(l, &["RefDocumento", "NumDocumento"]) {
            linhas.entry(k).or_default().push(l);
        }
    }
    let mut n_p = 0;
    for r in &x.proformas {
        let Some(rf) = t(r, &["RefDocumento", "RefLong", "NumDocumento"]) else { continue };
        if sqlx::query_scalar::<_, i64>("SELECT id FROM documentos WHERE numero_doc = ?").bind(&rf).fetch_optional(&mut **tx).await?.is_some() {
            continue;
        }
        let tipo = match tipo_documento(rf.split_whitespace().next(), None, None) {
            Some(c @ ("PP" | "OR" | "NE")) => c,
            _ => "PP",
        };
        let ls = linhas.get(&rf).map(|v| v.as_slice()).unwrap_or(&[]);
        let soma = |nomes: &[&str]| ls.iter().map(|l| kz(l, nomes)).sum::<i64>();
        let total = Some(kz(r, &["ValTotal", "Total", "ValorTotal"])).filter(|v| *v != 0).unwrap_or_else(|| soma(&["ValorTotal"]));
        let iva = Some(kz(r, &["ValTaxa", "ValorTaxa", "TotalImposto"])).filter(|v| *v != 0).unwrap_or_else(|| soma(&["ValorTaxa"]));
        let entidade = entidade_k(tx, mapas, false, ent(r, &["IdEntidade", "IdCliente"])).await?.or(Some(1));
        let (nome, nif): (String, String) = sqlx::query_as("SELECT nome, nif FROM entidades WHERE id = ?").bind(entidade).fetch_one(&mut **tx).await?;
        let data_p = data(r, &["DataEmissao", "Data"]).unwrap_or_else(db::hoje);
        let (serie, numero) = serie_numero(&rf);
        let doc_id = sqlx::query(
            "INSERT INTO documentos (tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, data_vencimento, entidade_id, entidade_nome, entidade_nif,
                utilizador_id, estado, total_bruto, total_desconto, total_liquido, total_iva, total, valor_pendente, hash, hash_controlo, importado, origem)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'N', ?, 0, ?, ?, ?, 0, '', '', 1, 'I')",
        )
        .bind(tipo)
        .bind(&serie)
        .bind(numero.unwrap_or(n_p as i64 + 1))
        .bind(&rf)
        .bind(&data_p)
        .bind(data_hora(r, &["DataHoraEmissao"]).unwrap_or_else(|| format!("{data_p}T00:00:00")))
        .bind(data(r, &["Validade", "DataValidade", "DataVencimento"]))
        .bind(entidade)
        .bind(t(r, &["Nome", "NomeCliente"]).unwrap_or(nome))
        .bind(t(r, &["NIF"]).unwrap_or(nif))
        .bind(i(r, &["IdUsuario"]).and_then(|u| mapas.utilizadores.get(&u).copied()).unwrap_or(utilizador))
        .bind(total - iva)
        .bind(total - iva)
        .bind(iva)
        .bind(total)
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
        inserir_linhas(tx, mapas, doc_id, ls).await?;
        n_p += 1;
    }
    rel.conta("Proformas e orçamentos (histórico)", n_p);
    Ok(())
}

// ------------------------------------------------------------------ tesouraria, caixa, moedas, prazos
async fn conta_origem(tx: &mut Transaction<'_, Sqlite>, origem: &str, descricao: &str, tipo: &str, banco: Option<String>, agencia: Option<String>, numero: Option<String>, iban: Option<String>) -> AppResult<i64> {
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM contas WHERE id_origem = ?").bind(origem).fetch_optional(&mut **tx).await? {
        return Ok(id);
    }
    // aproveita uma conta com o mesmo nome ainda sem origem (ex.: "Caixa Principal")
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM contas WHERE lower(descricao) = lower(?) AND id_origem IS NULL").bind(descricao).fetch_optional(&mut **tx).await? {
        sqlx::query("UPDATE contas SET id_origem = ?, iban = COALESCE(?, iban), banco = COALESCE(?, banco) WHERE id = ?").bind(origem).bind(&iban).bind(&banco).bind(id).execute(&mut **tx).await?;
        return Ok(id);
    }
    Ok(sqlx::query("INSERT INTO contas (descricao, tipo, banco, agencia, numero, iban, id_origem) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(descricao)
        .bind(tipo)
        .bind(banco)
        .bind(agencia)
        .bind(numero)
        .bind(iban)
        .bind(origem)
        .execute(&mut **tx)
        .await?
        .last_insert_rowid())
}

fn codigo_moeda(r: &Registo) -> Option<&'static str> {
    let s = format!("{} {}", t(r, &["Simbolo"]).unwrap_or_default(), t(r, &["Descricao"]).unwrap_or_default()).to_lowercase();
    [("usd", "USD"), ("dólar", "USD"), ("dolar", "USD"), ("us$", "USD"), ("eur", "EUR"), ("€", "EUR"), ("zar", "ZAR"), ("rand", "ZAR"), ("cny", "CNY"), ("yuan", "CNY"), ("renminbi", "CNY"), ("gbp", "GBP"), ("libra", "GBP"), ("brl", "BRL"), ("real", "BRL"), ("aoa", "AOA"), ("kwanza", "AOA")]
        .iter()
        .find(|(k, _)| s.contains(k))
        .map(|(_, c)| *c)
}

/// Entrada ou saída de um movimento do fluxo de caixa do KIGEST (crédito/débito ou valor com sinal).
fn movimento_tesouraria(r: &Registo) -> Option<(&'static str, i64)> {
    let credito = kz(r, &["ValorCredito", "Credito", "Entrada"]);
    let debito = kz(r, &["ValorDebito", "Debito", "Saida"]);
    if credito > 0 {
        return Some(("ENTRADA", credito));
    }
    if debito > 0 {
        return Some(("SAIDA", debito));
    }
    let v = kz(r, &["Valor", "TotalDocs"]);
    let saida = t(r, &["Movimento", "TipoMovimento", "Tipo"]).map(|m| {
        let m = m.to_lowercase();
        m.starts_with('s') || m.starts_with('d') || m.contains("pag")
    });
    match (v, saida) {
        (v, Some(true)) if v != 0 => Some(("SAIDA", v.abs())),
        (v, _) if v > 0 => Some(("ENTRADA", v)),
        (v, _) if v < 0 => Some(("SAIDA", -v)),
        _ => None,
    }
}

async fn n_tesouraria(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, mapas: &Mapas, x: &Extras, utilizador: i64) -> AppResult<()> {
    // centros de custo
    let mut centros: HashMap<i64, i64> = HashMap::new();
    for r in &x.centros_custo {
        let Some(desc) = t(r, &["Descritivo", "Descricao"]) else { continue };
        let id = match sqlx::query_scalar::<_, i64>("SELECT id FROM centros_custo WHERE lower(descricao) = lower(?)").bind(&desc).fetch_optional(&mut **tx).await? {
            Some(id) => id,
            None => sqlx::query("INSERT INTO centros_custo (descricao, tipo) VALUES (?, 'DESPESA')").bind(&desc).execute(&mut **tx).await?.last_insert_rowid(),
        };
        if let Some(k) = i(r, &["Id", "ID"]) {
            centros.insert(k, id);
        }
    }
    if !x.centros_custo.is_empty() {
        rel.conta("Centros de custo", centros.len() as i64);
    }
    // contas do fluxo de caixa e contas bancárias
    let mut contas: HashMap<i64, i64> = HashMap::new();
    for r in &x.tes_contas {
        let (Some(k), Some(desc)) = (i(r, &["ID", "Id"]), t(r, &["Descricao"])) else { continue };
        let iban = t(r, &["Conta_Iban", "IBAN"]);
        let banco = t(r, &["Tipo"]).map(|x| x.to_lowercase().contains("banc")).unwrap_or(false) || iban.is_some();
        let id = conta_origem(tx, &format!("FC:{k}"), &desc, if banco { "BANCO" } else { "CAIXA" }, None, None, None, iban).await?;
        contas.insert(k, id);
    }
    let nomes_banco: HashMap<String, String> = x.bancos.iter().filter_map(|r| Some((t(r, &["Codigo", "CodBanco", "Id"])?, t(r, &["Nome", "Descricao"])?))).collect();
    let mut n_bc = 0;
    for r in &x.bancos_contas {
        let (Some(k), Some(desc)) = (i(r, &["Id", "ID"]), t(r, &["Descricao", "Conta"])) else { continue };
        let banco = t(r, &["CodBanco"]).and_then(|c| nomes_banco.get(&c).cloned());
        conta_origem(tx, &format!("BC:{k}"), &desc, "BANCO", banco, t(r, &["AgenciaRef", "Agencia"]), t(r, &["Conta", "NumeroConta"]), t(r, &["IBAN", "Iban"])).await?;
        n_bc += 1;
    }
    if !x.tes_contas.is_empty() || n_bc > 0 {
        rel.conta("Contas de tesouraria e bancárias", contas.len() as i64 + n_bc);
    }
    // movimentos do fluxo de caixa (o saldo de cada conta fica igual ao do KIGEST)
    let (mut n_m, mut sem_conta) = (0, 0);
    for (idx, r) in x.tes_movimentos.iter().enumerate() {
        let conta = i(r, &["IdConta", "IdContaCaixa", "Conta"]).and_then(|k| contas.get(&k).copied());
        let Some(conta) = conta else {
            sem_conta += 1;
            continue;
        };
        let Some((tipo, valor)) = movimento_tesouraria(r) else { continue };
        let data_m = data(r, &["Data", "DataMovimento", "DataEmissao", "DataRegisto", "DataDocumento", "DataOperacao"]).unwrap_or_else(db::hoje);
        let dh = data_hora(r, &["DataHora", "DataHoraEmissao", "DataRegisto"]).unwrap_or_else(|| format!("{data_m}T00:00:00"));
        let origem = t(r, &["Id", "ID", "Referencia"]).unwrap_or_else(|| format!("{idx}:{data_m}:{conta}:{valor}"));
        let origem = format!("FX:{origem}");
        if sqlx::query_scalar::<_, i64>("SELECT id FROM movimentos_tesouraria WHERE id_origem = ?").bind(&origem).fetch_optional(&mut **tx).await?.is_some() {
            continue;
        }
        sqlx::query(
            "INSERT INTO movimentos_tesouraria (data, data_hora, conta_id, tipo, valor, descricao, centro_custo_id, documento, utilizador_id, id_origem)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&data_m)
        .bind(&dh)
        .bind(conta)
        .bind(tipo)
        .bind(valor)
        .bind(t(r, &["Desc_Geral", "Descricao", "Historico", "Observacao", "Desc_CCusto"]).unwrap_or_else(|| "Movimento importado do KIGEST".into()))
        .bind(i(r, &["Id_CentroCusto", "IdCentroCusto"]).and_then(|k| centros.get(&k).copied()))
        .bind(t(r, &["RefDocumento", "Documento", "NumDocumento"]))
        .bind(i(r, &["IdUsuario"]).and_then(|u| mapas.utilizadores.get(&u).copied()).unwrap_or(utilizador))
        .bind(&origem)
        .execute(&mut **tx)
        .await?;
        n_m += 1;
    }
    if !x.tes_movimentos.is_empty() {
        rel.conta("Movimentos do fluxo de caixa", n_m);
    }
    if sem_conta > 0 {
        rel.avisos.push(format!("{sem_conta} movimento(s) do fluxo de caixa sem conta conhecida não foram importados."));
    }
    // fechos de caixa do POS (histórico)
    let mut n_f = 0;
    for r in &x.caixa_fechos {
        let Some(abertura) = data_hora(r, &["DataAbertura"]).or_else(|| data(r, &["DataAbertura"]).map(|d| format!("{d}T00:00:00"))) else { continue };
        let origem = t(r, &["Referencia", "Id", "ID"]).unwrap_or_else(|| abertura.clone());
        if sqlx::query_scalar::<_, i64>("SELECT id FROM caixa_sessoes WHERE id_origem = ?").bind(&origem).fetch_optional(&mut **tx).await?.is_some() {
            continue;
        }
        let fecho = data_hora(r, &["DataFecho"]).or_else(|| data(r, &["DataFecho"]).map(|d| format!("{d}T23:59:59"))).unwrap_or_else(|| abertura.clone());
        sqlx::query("INSERT INTO caixa_sessoes (posto_id, utilizador_id, aberta_em, fechada_em, valor_inicial, valor_contado, estado, id_origem) VALUES (NULL, ?, ?, ?, ?, ?, 'FECHADA', ?)")
            .bind(i(r, &["IdUsusario", "IdUsuario"]).and_then(|u| mapas.utilizadores.get(&u).copied()).unwrap_or(utilizador))
            .bind(&abertura)
            .bind(&fecho)
            .bind(kz(r, &["Saldo_Inicial", "SaldoInicial"]))
            .bind(kz(r, &["SaldoFinal", "Saldo_Final"]))
            .bind(&origem)
            .execute(&mut **tx)
            .await?;
        n_f += 1;
    }
    if !x.caixa_fechos.is_empty() {
        rel.conta("Fechos de caixa (histórico)", n_f);
    }
    // moedas e câmbios
    let mut n_md = 0;
    for r in &x.moedas {
        let Some(cod) = codigo_moeda(r) else { continue };
        if cod == "AOA" {
            continue;
        }
        let cambio = n(r, &["Cambio", "Taxa"]).unwrap_or(0.0);
        let desc = t(r, &["Descricao"]).unwrap_or_else(|| cod.into());
        let simb = t(r, &["Simbolo"]).unwrap_or_else(|| cod.into());
        sqlx::query(
            "INSERT INTO moedas (codigo, descricao, simbolo, cambio) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(codigo) DO UPDATE SET cambio = CASE WHEN ?4 > 0 THEN ?4 ELSE cambio END, ativo = 1",
        )
        .bind(cod)
        .bind(&desc)
        .bind(&simb)
        .bind(cambio)
        .execute(&mut **tx)
        .await?;
        n_md += 1;
    }
    if !x.moedas.is_empty() {
        rel.conta("Moedas e câmbios", n_md);
    }
    // prazos de pagamento
    let mut n_pz = 0;
    for r in &x.prazos {
        let (Some(desc), Some(dias)) = (t(r, &["Descricao"]), i(r, &["Dias"])) else { continue };
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prazos_pagamento WHERE dias = ?").bind(dias).fetch_one(&mut **tx).await?;
        if ja == 0 {
            sqlx::query("INSERT OR IGNORE INTO prazos_pagamento (codigo, descricao, dias) VALUES (?, ?, ?)").bind(format!("K{dias}")).bind(desc).bind(dias).execute(&mut **tx).await?;
            n_pz += 1;
        }
    }
    if !x.prazos.is_empty() {
        rel.conta("Prazos de pagamento novos", n_pz);
    }
    Ok(())
}

// ------------------------------------------------------------------ RH: histórico salarial, honorários, alterações por processar
const MESES_PT: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

fn mes_de(r: &Registo, nomes: &[&str]) -> Option<i64> {
    let v = campo(r, nomes)?;
    if let Some(m) = v.int().filter(|m| (1..=12).contains(m)) {
        return Some(m);
    }
    let s = v.texto()?.to_lowercase();
    MESES_PT.iter().position(|m| s.starts_with(m)).map(|p| p as i64 + 1)
}

async fn funcionario_k(tx: &mut Transaction<'_, Sqlite>, r: &Registo, nomes: &[&str]) -> AppResult<Option<i64>> {
    let Some(k) = t(r, nomes) else { return Ok(None) };
    Ok(sqlx::query_scalar("SELECT id FROM funcionarios WHERE lower(id_origem) = lower(?)").bind(k).fetch_optional(&mut **tx).await?)
}

fn processado(r: &Registo) -> bool {
    let s = t(r, &["StatusProcessamento", "Estado"]).unwrap_or_default().to_lowercase();
    (s.contains("process") || s == "s" || s == "1" || s == "sim") && !s.contains("não") && !s.contains("nao") && !s.contains("pend")
}

async fn n_rh_historico(tx: &mut Transaction<'_, Sqlite>, rel: &mut Relatorio, x: &Extras, utilizador: i64) -> AppResult<()> {
    let rubricas: HashMap<String, String> = sqlx::query_as::<_, (String, String)>("SELECT codigo, tipo FROM rh_rubricas").fetch_all(&mut **tx).await?.into_iter().collect();

    // processamentos salariais (um por mês) com os recibos e as linhas
    let mut meses: std::collections::BTreeMap<(i64, i64), Vec<&Registo>> = std::collections::BTreeMap::new();
    for r in &x.rh_proc_cab {
        if let (Some(a), Some(m)) = (i(r, &["Ano", "AnoProcessamento"]), mes_de(r, &["Mes", "MesProcessamento", "MesExtenso"])) {
            meses.entry((a, m)).or_default().push(r);
        }
    }
    let (mut n_proc, mut n_rec) = (0, 0);
    for ((ano, mes), regs) in &meses {
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_processamentos WHERE ano = ? AND mes = ? AND estado = 'PROCESSADO'").bind(ano).bind(mes).fetch_one(&mut **tx).await?;
        if ja > 0 {
            continue;
        }
        let proc = sqlx::query("INSERT INTO rh_processamentos (ano, mes, data, estado, utilizador_id, total_liquido, pago, importado) VALUES (?, ?, ?, 'PROCESSADO', ?, 0, 1, 1)")
            .bind(ano)
            .bind(mes)
            .bind(format!("{ano:04}-{mes:02}-28"))
            .bind(utilizador)
            .execute(&mut **tx)
            .await?
            .last_insert_rowid();
        let mut total = 0;
        for r in regs {
            let Some(fid) = funcionario_k(tx, r, &["IdFuncionario", "Id"]).await? else { continue };
            let f: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as("SELECT nome, numero, nif, inss_numero FROM funcionarios WHERE id = ?").bind(fid).fetch_one(&mut **tx).await?;
            let liquido = kz(r, &["Liquido", "TotalLiquido", "ValorLiquido"]);
            let remun = Some(kz(r, &["Iliquido", "TotalIliquido", "TotalRemuneracoes"])).filter(|v| *v > 0).unwrap_or(liquido);
            let base_inss = kz(r, &["BaseTributavelINSS", "BaseINSS"]);
            let inss = kz(r, &["ContribuicaoINSS", "INSS", "INSSTrabalhador"]);
            let inss_emp = Some(kz(r, &["INSSEmpresa", "ContribuicaoINSSEmpresa", "INSSEntidade"])).filter(|v| *v > 0).unwrap_or((base_inss as f64 * 0.08).round() as i64);
            let rec = sqlx::query(
                "INSERT INTO rh_recibos (processamento_id, funcionario_id, numero, nome, nif, inss_numero, departamento, funcao, categoria, iban,
                    salario_base, total_remuneracoes, base_inss, inss_trabalhador, inss_empregador, base_irt, irt, total_descontos, liquido)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(proc)
            .bind(fid)
            .bind(&f.1)
            .bind(t(r, &["Nome"]).unwrap_or(f.0))
            .bind(t(r, &["NIF"]).or(f.2))
            .bind(t(r, &["NumeroINSS"]).or(f.3))
            .bind(t(r, &["Departamento"]))
            .bind(t(r, &["Funcao"]))
            .bind(t(r, &["Categoria"]))
            .bind(t(r, &["pagto_IBAN", "IBAN"]))
            .bind(kz(r, &["SalarioBase", "Salario_Base"]))
            .bind(remun)
            .bind(base_inss)
            .bind(inss)
            .bind(inss_emp)
            .bind(kz(r, &["BaseTributavelIRT", "BaseIRT"]))
            .bind(kz(r, &["IRT"]))
            .bind((remun - liquido).max(0))
            .bind(liquido)
            .execute(&mut **tx)
            .await?
            .last_insert_rowid();
            let k_func = t(r, &["IdFuncionario", "Id"]).unwrap_or_default().to_lowercase();
            for l in x.rh_proc_linhas.iter().filter(|l| {
                t(l, &["IdFuncionario"]).map(|v| v.to_lowercase()) == Some(k_func.clone())
                    && i(l, &["Ano", "AnoProcessamento"]) == Some(*ano)
                    && mes_de(l, &["Mes", "MesProcessamento", "MesExtenso"]) == Some(*mes)
            }) {
                let rubrica = t(l, &["CodDescritivo", "Codigo", "Rubrica"]).unwrap_or_else(|| "OUT".into());
                let tipo = rubricas.get(&rubrica).cloned().unwrap_or_else(|| {
                    let m = t(l, &["Movimento", "Tipo"]).unwrap_or_default().to_lowercase();
                    if m.starts_with('d') || m == "2" || m == "-1" { "DESCONTO".into() } else { "REMUNERACAO".into() }
                });
                sqlx::query("INSERT INTO rh_recibo_linhas (recibo_id, rubrica, descricao, tipo, quantidade, valor) VALUES (?, ?, ?, ?, ?, ?)")
                    .bind(rec)
                    .bind(&rubrica)
                    .bind(t(l, &["Descritivo", "Descricao", "Abreviatura"]).unwrap_or_else(|| rubrica.clone()))
                    .bind(tipo)
                    .bind(n(l, &["Quantidade", "QTD", "Qtd"]))
                    .bind(kz(l, &["Valor"]))
                    .execute(&mut **tx)
                    .await?;
            }
            total += liquido;
            n_rec += 1;
        }
        sqlx::query("UPDATE rh_processamentos SET total_liquido = ? WHERE id = ?").bind(total).bind(proc).execute(&mut **tx).await?;
        n_proc += 1;
    }
    if !x.rh_proc_cab.is_empty() {
        rel.conta("Processamentos salariais (meses)", n_proc);
        rel.conta("Recibos de salário", n_rec);
    }

    // honorários de trabalhadores por conta própria
    let mut n_h = 0;
    for r in &x.rh_honorarios {
        let Some(fid) = funcionario_k(tx, r, &["IdDiarista", "IdFuncionario"]).await? else { continue };
        let (Some(ano), Some(mes)) = (i(r, &["Ano"]), mes_de(r, &["Mes", "MesExtenso"])) else { continue };
        let iliquido = kz(r, &["ValorIliquido", "Iliquido"]);
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_honorarios WHERE funcionario_id = ? AND ano = ? AND mes = ? AND iliquido = ?").bind(fid).bind(ano).bind(mes).bind(iliquido).fetch_one(&mut **tx).await?;
        if ja > 0 {
            continue;
        }
        let tp = t(r, &["TipoProcessamento"]).unwrap_or_default().to_lowercase();
        let tipo = if tp.contains("hora") { "HORA" } else if tp.contains("dia") { "DIA" } else { "MENSAL" };
        sqlx::query("UPDATE funcionarios SET independente = 1 WHERE id = ?").bind(fid).execute(&mut **tx).await?;
        sqlx::query(
            "INSERT INTO rh_honorarios (funcionario_id, data, ano, mes, tipo, valor_base, quantidade, iliquido, irt_pct, irt, liquido, descricao, estado, pago, utilizador_id, importado)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Importado do KIGEST', 'PROCESSADO', 1, ?, 1)",
        )
        .bind(fid)
        .bind(data(r, &["DataProcessamento", "Data"]).unwrap_or_else(|| format!("{ano:04}-{mes:02}-28")))
        .bind(ano)
        .bind(mes)
        .bind(tipo)
        .bind(kz(r, &["Honorario", "ValorBase"]))
        .bind(n(r, &["TotalDias", "Quantidade"]).unwrap_or(1.0))
        .bind(iliquido)
        .bind(n(r, &["IRTPorcento", "TaxaIRT"]).unwrap_or(0.0))
        .bind(kz(r, &["IRT"]))
        .bind(kz(r, &["TotalLiquido", "Liquido"]))
        .bind(utilizador)
        .execute(&mut **tx)
        .await?;
        n_h += 1;
    }
    if !x.rh_honorarios.is_empty() {
        rel.conta("Honorários (histórico)", n_h);
    }

    // alterações e parcelas de adiantamento ainda por processar
    let mut n_alt = 0;
    let mut sem_rubrica = 0;
    for (r, adiant) in x.rh_alteracoes.iter().map(|r| (r, false)).chain(x.rh_adiantamentos.iter().map(|r| (r, true))) {
        if processado(r) {
            continue;
        }
        let Some(fid) = funcionario_k(tx, r, &["IdFuncionario"]).await? else { continue };
        let (Some(ano), Some(mes)) = (i(r, &["AnoProcessamento", "Ano"]), mes_de(r, &["MesProcessamento", "Mes"])) else { continue };
        let (rubrica, valor, qtd, desc) = if adiant {
            ("102".to_string(), kz(r, &["ValorParcela", "Valor"]), 0.0, format!("Adiantamento (KIGEST) — parcela {}", t(r, &["Parcela"]).unwrap_or_default()))
        } else {
            let Some(rb) = t(r, &["CodDescritivo", "Codigo"]) else { continue };
            (rb, kz(r, &["Valor"]), n(r, &["Quantidade", "QTD"]).unwrap_or(0.0), t(r, &["Descritivo"]).unwrap_or_default())
        };
        if !rubricas.contains_key(&rubrica) {
            sem_rubrica += 1;
            continue;
        }
        let ja: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rh_alteracoes WHERE funcionario_id = ? AND ano = ? AND mes = ? AND rubrica = ? AND valor = ? AND quantidade = ?")
            .bind(fid)
            .bind(ano)
            .bind(mes)
            .bind(&rubrica)
            .bind(valor)
            .bind(qtd)
            .fetch_one(&mut **tx)
            .await?;
        if ja > 0 {
            continue;
        }
        sqlx::query("INSERT INTO rh_alteracoes (funcionario_id, ano, mes, rubrica, quantidade, valor, descricao, data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(fid)
            .bind(ano)
            .bind(mes)
            .bind(&rubrica)
            .bind(qtd)
            .bind(valor)
            .bind(Some(desc).filter(|d| !d.is_empty()))
            .bind(db::hoje())
            .execute(&mut **tx)
            .await?;
        n_alt += 1;
    }
    if !x.rh_alteracoes.is_empty() || !x.rh_adiantamentos.is_empty() {
        rel.conta("Alterações e adiantamentos por processar", n_alt);
    }
    if sem_rubrica > 0 {
        rel.avisos.push(format!("{sem_rubrica} alteração(ões) mensal(is) com rubricas que não existem no KanGest não foram importadas; lance-as em Alterações do Mês."));
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::certificado_no_rodape;

    #[test]
    fn certificado_do_rodape() {
        assert_eq!(certificado_no_rodape("Processado por programa validado nº.123/").as_deref(), Some("123"));
        assert_eq!(certificado_no_rodape("Processado por programa válido n.º 123/AGT/2020").as_deref(), Some("123/AGT/2020"));
        assert_eq!(certificado_no_rodape("VOLTE SEMPRE"), None);
        assert_eq!(certificado_no_rodape("Processado por computador"), None);
    }
}

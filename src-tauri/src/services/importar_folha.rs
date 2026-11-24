// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Importação de uma folha de cálculo (Excel `.xlsx`/`.xls`, LibreOffice `.ods`) ou de um CSV: a lista de artigos,
//! de clientes ou de fornecedores que qualquer loja tem, mesmo sem programa de facturação.
//!
//! Cada coluna da folha liga-se a um campo do KanGest. O programa sugere a ligação pelos títulos («Preço», «PVP»,
//! «Contribuinte», «Telemóvel»…) e quem importa confirma ou corrige no ecrã. Nada se duplica:
//!
//! - **artigos**: procura-se pela referência, senão pelo código de barras, senão pela descrição. Num artigo que já
//!   existe só se preenche o que lhe falta, a não ser que se peça para **actualizar os preços** (uma tabela de preços
//!   nova). O **stock** só entra nos artigos novos, como saldo inicial no armazém escolhido;
//! - **clientes** e **fornecedores**: procura-se pelo NIF, senão pelo nome. Num que já existe só se preenche o que
//!   falta (telefone, email, morada).
//!
//! Os números aceitam os formatos de cá («1 234,50», «1.234,50», «Kz 1500», «14%»). «Simular» corre tudo e desfaz no
//! fim, como nas outras importações.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::services::documentos::{com_iva, sem_iva};
use crate::state::AppState;

/// Linhas a mais do que isto numa folha ficam de fora (uma lista de artigos real anda pelos milhares).
const MAX_LINHAS: usize = 200_000;
/// Um CSV maior do que isto não é uma lista de artigos ou de clientes.
const MAX_BYTES_CSV: u64 = 100 * 1024 * 1024;

pub(crate) struct Folha {
    pub(crate) nome: String,
    pub(crate) linhas: Vec<Vec<String>>,
}

/// Lê todas as folhas do ficheiro, já como texto (os números do Excel com vírgula decimal, como se escrevem cá).
pub(crate) fn ler(caminho: &Path) -> AppResult<Vec<Folha>> {
    let ext = caminho.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !caminho.is_file() {
        return Err(AppError::msg(format!("Não encontro o ficheiro {}.", caminho.display())));
    }
    let mut folhas = match ext.as_str() {
        "csv" | "txt" | "tsv" => vec![ler_csv(caminho)?],
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => ler_livro(caminho)?,
        _ => return Err(AppError::msg("Escolha uma folha do Excel (.xlsx, .xls), do LibreOffice (.ods) ou um ficheiro CSV.")),
    };
    for f in &mut folhas {
        for l in &mut f.linhas {
            for c in l.iter_mut() {
                let t = c.trim();
                if t.len() != c.len() {
                    *c = t.to_string();
                }
            }
            while l.last().is_some_and(|c| c.is_empty()) {
                l.pop();
            }
        }
        // as linhas vazias ficam, para os avisos dizerem a linha certa do Excel; só saem as do fim
        while f.linhas.last().is_some_and(|l| l.is_empty()) {
            f.linhas.pop();
        }
        f.linhas.truncate(MAX_LINHAS);
    }
    folhas.retain(|f| !f.linhas.is_empty());
    if folhas.is_empty() {
        return Err(AppError::msg("A folha está vazia."));
    }
    Ok(folhas)
}

fn ler_livro(caminho: &Path) -> AppResult<Vec<Folha>> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut livro = open_workbook_auto(caminho).map_err(|e| AppError::msg(format!("Não consegui abrir a folha de cálculo ({e}). Se tiver palavra-passe, tire-a no Excel e volte a tentar.")))?;
    let mut folhas = Vec::new();
    for nome in livro.sheet_names() {
        let Ok(r) = livro.worksheet_range(&nome) else { continue };
        // o calamine começa na primeira linha com dados: as de cima voltam vazias, para a numeração ser a do Excel
        let acima = r.start().map(|(l, _)| l as usize).unwrap_or(0).min(1000);
        let linhas = std::iter::repeat_n(Vec::new(), acima)
            .chain(r.rows().take(MAX_LINHAS + 50).map(|l| {
                l.iter()
                    .map(|c| match c {
                        Data::Empty | Data::Error(_) => String::new(),
                        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
                        Data::Int(i) => i.to_string(),
                        Data::Float(f) => numero_em_texto(*f),
                        Data::Bool(b) => (if *b { "SIM" } else { "NÃO" }).into(),
                        Data::DateTime(d) => numero_em_texto(d.as_f64()),
                    })
                    .collect()
            }))
            .collect();
        folhas.push(Folha { nome, linhas });
    }
    Ok(folhas)
}
/// Um número do Excel como se escreve cá: vírgula decimal, sem separador de milhares e sem o «,0» dos inteiros.
fn numero_em_texto(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        // o arredondamento a 6 casas tira o ruído do binário (0,1 + 0,2)
        let s = format!("{:.6}", f);
        s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
    }
}

fn ler_csv(caminho: &Path) -> AppResult<Folha> {
    let erro = |e: std::io::Error| AppError::msg(format!("Não consegui ler {}: {e}", caminho.display()));
    let tamanho = std::fs::metadata(caminho).map_err(erro)?.len();
    if tamanho > MAX_BYTES_CSV {
        return Err(AppError::msg("O ficheiro CSV é grande de mais para uma lista de artigos ou de clientes."));
    }
    let bytes = std::fs::read(caminho).map_err(erro)?;
    let texto = descodificar(&bytes);
    let primeira = texto.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut sep = [';', ',', '\t', '|'].into_iter().max_by_key(|s| contar_fora_de_aspas(primeira, *s)).unwrap_or(';');
    // uma primeira linha sem separadores (o título de um extracto do banco): conta-se nas 20 primeiras linhas, e nos
    // empates fica o primeiro da lista
    if contar_fora_de_aspas(primeira, sep) == 0 {
        let amostra: Vec<&str> = texto.lines().filter(|l| !l.trim().is_empty()).take(20).collect();
        let mut melhor = 0;
        sep = ';';
        for s in [';', ',', '\t', '|'] {
            let n: usize = amostra.iter().map(|l| contar_fora_de_aspas(l, s)).sum();
            if n > melhor {
                (sep, melhor) = (s, n);
            }
        }
    }
    let nome = caminho.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "CSV".into());
    Ok(Folha { nome, linhas: separar_csv(&texto, sep) })
}

/// UTF-8 (com ou sem BOM), UTF-16 (o «Texto Unicode» do Excel) ou Windows-1252 (o «CSV» do Excel em português).
fn descodificar(b: &[u8]) -> String {
    if let Some(r) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(r).into_owned();
    }
    if b.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(b).0.into_owned();
    }
    if b.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(b).0.into_owned();
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(b).0.into_owned(),
    }
}

fn contar_fora_de_aspas(linha: &str, sep: char) -> usize {
    let mut dentro = false;
    linha
        .chars()
        .filter(|c| {
            if *c == '"' {
                dentro = !dentro;
            }
            !dentro && *c == sep
        })
        .count()
}

/// CSV com aspas (RFC 4180): `""` dentro de aspas é uma aspa, e uma quebra de linha dentro de aspas fica no campo.
fn separar_csv(texto: &str, sep: char) -> Vec<Vec<String>> {
    let mut linhas = Vec::new();
    let mut linha = Vec::new();
    let mut campo = String::new();
    let mut aspas = false;
    let mut it = texto.chars().peekable();
    while let Some(c) = it.next() {
        if aspas {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    campo.push('"');
                    it.next();
                } else {
                    aspas = false;
                }
            } else {
                campo.push(c);
            }
        } else if c == '"' && campo.trim().is_empty() {
            campo.clear();
            aspas = true;
        } else if c == sep {
            linha.push(std::mem::take(&mut campo));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            linha.push(std::mem::take(&mut campo));
            linhas.push(std::mem::take(&mut linha));
            if linhas.len() > MAX_LINHAS + 50 {
                break;
            }
        } else {
            campo.push(c);
        }
    }
    if !campo.is_empty() || !linha.is_empty() {
        linha.push(campo);
        linhas.push(linha);
    }
    linhas
}

/// Um número escrito à mão ou exportado: «1 234,50», «1.234,50», «1,234.50», «1234.5», «Kz 1500», «14%», «-3».
/// Só com pontos, um ponto seguido de exactamente três algarismos é dos milhares («1.500» = 1500).
pub(crate) fn numero(s: &str) -> Option<f64> {
    let negativo = s.trim_start().starts_with('-') || (s.trim_start().starts_with('(') && s.trim_end().ends_with(')'));
    let limpo: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    if !limpo.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let (virgulas, pontos) = (limpo.matches(',').count(), limpo.matches('.').count());
    let normal = match (virgulas, pontos) {
        (0, 0) => limpo,
        (_, 0) if virgulas == 1 => limpo.replace(',', "."),
        (_, 0) => limpo.replace(',', ""),
        (0, 1) => {
            let depois = limpo.split('.').nth(1).unwrap_or("");
            if depois.len() == 3 {
                limpo.replace('.', "")
            } else {
                limpo
            }
        }
        (0, _) => limpo.replace('.', ""),
        _ => {
            // os dois: o último é o decimal
            if limpo.rfind(',') > limpo.rfind('.') {
                limpo.replace('.', "").replace(',', ".")
            } else {
                limpo.replace(',', "")
            }
        }
    };
    let v: f64 = normal.parse().ok()?;
    Some(if negativo { -v } else { v })
}

fn centimos(s: &str) -> Option<i64> {
    numero(s).map(|v| (v * 100.0).round() as i64)
}

/// Minúsculas, sem acentos nem pontuação: «Preço de Venda (Kz)» → «preco de venda kz», «P.V.P.» → «pvp».
pub(crate) fn normalizar(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        if matches!(c, '.' | '\'' | '’') {
            continue;
        }
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'º' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ª' => 'a',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        r.push(c);
    }
    r.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Os campos de cada tipo, com os títulos que costumam ter nas folhas (já normalizados).
fn campos(tipo: &str) -> &'static [(&'static str, &'static [&'static str])] {
    const ARTIGOS: &[(&str, &[&str])] = &[
        ("codigo_barras", &["codigo de barras", "codigo barras", "cod barras", "cod de barras", "ean", "ean13", "barcode", "cb"]),
        ("referencia", &["referencia", "ref", "codigo", "cod", "codigo artigo", "cod artigo", "codigo do artigo", "codigo produto", "sku", "codigo interno"]),
        ("descricao", &["descricao", "designacao", "nome", "produto", "artigo", "descricao do artigo", "nome do produto", "denominacao", "descricao artigo"]),
        ("preco", &["preco", "pvp", "preco de venda", "preco venda", "pv", "preco1", "preco 1", "preco unitario", "valor", "preco com iva", "pvp com iva"]),
        ("custo", &["custo", "preco de custo", "preco custo", "preco de compra", "preco compra", "pc", "custo unitario"]),
        ("iva", &["iva", "taxa", "taxa iva", "taxa de iva", "iva %", "iva percentagem"]),
        ("familia", &["familia", "categoria", "grupo", "seccao", "departamento", "familia do artigo"]),
        ("stock", &["stock", "quantidade", "qtd", "qtde", "existencia", "existencias", "stock actual", "stock atual", "qtd stock", "quant"]),
    ];
    const ENTIDADES: &[(&str, &[&str])] = &[
        ("nome", &["nome", "cliente", "fornecedor", "designacao", "razao social", "entidade", "nome do cliente", "nome do fornecedor", "empresa"]),
        ("nif", &["nif", "contribuinte", "n contribuinte", "no contribuinte", "n o contribuinte", "numero de contribuinte", "nif cliente", "num contribuinte", "bi nif"]),
        ("telefone", &["telefone", "telemovel", "tel", "contacto", "contactos", "celular", "movel", "telefone 1"]),
        ("email", &["email", "e mail", "correio electronico", "correio eletronico", "mail"]),
        ("endereco", &["morada", "endereco", "rua", "endereco completo"]),
        ("localidade", &["localidade", "cidade", "municipio", "bairro", "provincia"]),
    ];
    if tipo == "artigos" {
        ARTIGOS
    } else {
        ENTIDADES
    }
}

/// Liga cada campo à coluna cujo título melhor lhe corresponde: igual vale mais do que «contém», e o sinónimo mais
/// comprido ganha («código de barras» é das barras, não da referência).
fn sugerir(cabecalho: &[String], tipo: &str) -> HashMap<&'static str, usize> {
    let titulos: Vec<String> = cabecalho.iter().map(|t| normalizar(t)).collect();
    let mut pares: Vec<(usize, &'static str, usize)> = Vec::new();
    for (campo, sinonimos) in campos(tipo) {
        for (col, t) in titulos.iter().enumerate() {
            if t.is_empty() {
                continue;
            }
            let palavras = format!(" {t} ");
            let pontos = sinonimos
                .iter()
                .map(|s| if t == s { 1000 + s.len() } else if palavras.contains(&format!(" {s} ")) { s.len() } else { 0 })
                .max()
                .unwrap_or(0);
            if pontos > 0 {
                pares.push((pontos, campo, col));
            }
        }
    }
    pares.sort_by_key(|p| std::cmp::Reverse(p.0));
    let mut r = HashMap::new();
    let mut usadas = Vec::new();
    for (_, campo, col) in pares {
        if !r.contains_key(campo) && !usadas.contains(&col) {
            r.insert(campo, col);
            usadas.push(col);
        }
    }
    r
}

/// A linha dos títulos: a primeira, das dez de cima, com pelo menos dois títulos de texto (há folhas com o nome da
/// loja ou a data nas primeiras linhas).
fn linha_dos_titulos(linhas: &[Vec<String>]) -> usize {
    let com_dados = || linhas.iter().enumerate().filter(|(_, l)| !l.is_empty());
    com_dados()
        .take(10)
        .find(|(_, l)| l.iter().filter(|c| !c.is_empty() && numero(c).is_none_or(|_| c.chars().any(char::is_alphabetic))).count() >= 2)
        .or_else(|| com_dados().next())
        .map(|(n, _)| n)
        .unwrap_or(0)
}

#[derive(Deserialize)]
pub struct AnalisarIn {
    caminho: String,
}

/// O que a folha traz: as folhas do livro, os títulos, as primeiras linhas e a ligação sugerida das colunas.
pub async fn analisar(st: &AppState, i: AnalisarIn) -> AppResult<Value> {
    st.exigir_perm("sistema.importar")?;
    let caminho = std::path::PathBuf::from(i.caminho.trim());
    let folhas = tokio::task::spawn_blocking(move || ler(&caminho)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let taxas = db::query_json(&st.pool, "SELECT id, descricao, percentagem, codigo FROM taxas_iva WHERE ativo = 1 ORDER BY id").await?;
    let armazens = db::query_json(&st.pool, "SELECT id, nome FROM armazens WHERE ativo = 1 ORDER BY id").await?;
    let folhas: Vec<Value> = folhas
        .iter()
        .map(|f| {
            let t = linha_dos_titulos(&f.linhas);
            let cabecalho = &f.linhas[t];
            let art = sugerir(cabecalho, "artigos");
            let ent = sugerir(cabecalho, "entidades");
            // artigos se a folha tem preço, stock, código de barras ou família; senão, pessoas se tem NIF, telefone ou email
            let pontos_art = ["preco", "custo", "stock", "codigo_barras", "familia", "iva"].iter().filter(|c| art.contains_key(*c)).count();
            let pontos_ent = ["nif", "telefone", "email", "endereco", "localidade"].iter().filter(|c| ent.contains_key(*c)).count();
            let nome = normalizar(&f.nome);
            let tipo = if pontos_ent > pontos_art {
                if nome.contains("fornec") {
                    "fornecedores"
                } else {
                    "clientes"
                }
            } else {
                "artigos"
            };
            json!({
                "nome": f.nome,
                "linha_titulos": t,
                "cabecalho": cabecalho,
                "amostra": f.linhas.iter().skip(t + 1).filter(|l| !l.is_empty()).take(8).collect::<Vec<_>>(),
                "linhas": f.linhas.iter().skip(t + 1).filter(|l| !l.is_empty()).count(),
                "tipo": tipo,
                "colunas": { "artigos": art, "entidades": ent },
            })
        })
        .collect();
    Ok(json!({ "folhas": folhas, "taxas": taxas, "armazens": armazens }))
}

#[derive(Deserialize)]
pub struct ExecutarIn {
    caminho: String,
    /// Nome da folha (num livro com várias).
    folha: Option<String>,
    /// «artigos», «clientes» ou «fornecedores».
    tipo: String,
    /// Campo → índice da coluna (a contar da primeira coluna com dados).
    colunas: HashMap<String, usize>,
    #[serde(default)]
    simular: bool,
    /// Os preços da folha já trazem o IVA (o PVP da etiqueta). Por omissão, sim.
    #[serde(default = "sim")]
    precos_com_iva: bool,
    /// Taxa de IVA dos artigos quando a folha não a diz (ou não se reconhece). Por omissão, a normal.
    taxa_iva_id: Option<i64>,
    /// Armazém do stock inicial. Por omissão, o primeiro.
    armazem_id: Option<i64>,
    /// Nos artigos que já existem, pôr o preço (e o custo) da folha em vez de só preencher o que falta.
    #[serde(default)]
    actualizar_precos: bool,
}
fn sim() -> bool {
    true
}

/// O valor de um campo numa linha, ou None se a coluna não está ligada ou a célula está vazia.
fn valor<'a>(l: &'a [String], colunas: &HashMap<String, usize>, campo: &str) -> Option<&'a str> {
    colunas.get(campo).and_then(|c| l.get(*c)).map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// Importa a folha (ou simula: corre tudo e desfaz no fim).
pub async fn executar(st: &AppState, i: ExecutarIn) -> AppResult<Value> {
    let sessao = st.exigir_perm("sistema.importar")?;
    if !matches!(i.tipo.as_str(), "artigos" | "clientes" | "fornecedores") {
        return Err(AppError::msg("Escolha o que a folha tem: artigos, clientes ou fornecedores."));
    }
    let obrigatorio = if i.tipo == "artigos" { "descricao" } else { "nome" };
    if !i.colunas.contains_key(obrigatorio) {
        return Err(AppError::msg(if i.tipo == "artigos" {
            "Diga qual é a coluna da descrição dos artigos."
        } else {
            "Diga qual é a coluna do nome."
        }));
    }
    let caminho = std::path::PathBuf::from(i.caminho.trim());
    let origem = caminho.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let folhas = tokio::task::spawn_blocking(move || ler(&caminho)).await.map_err(|e| AppError::msg(e.to_string()))??;
    let folha = match &i.folha {
        Some(n) => folhas.iter().find(|f| &f.nome == n).ok_or_else(|| AppError::msg(format!("A folha «{n}» já não está no ficheiro.")))?,
        None => &folhas[0],
    };
    let t = linha_dos_titulos(&folha.linhas);
    let linhas = &folha.linhas[t + 1..];
    let mut tx = db::transacao(&st.pool).await?;
    // número da linha no Excel: a dos títulos é a t + 1
    let base = t + 2;
    let rel = if i.tipo == "artigos" { artigos(&mut tx, &i, linhas, base, &origem).await? } else { entidades(&mut tx, &i, linhas, base).await? };
    sqlx::query("INSERT INTO auditoria (utilizador_id, acao, detalhe) VALUES (?, 'IMPORTAR', ?)")
        .bind(sessao.id)
        .bind(format!("Folha {origem} ({}): {}", folha.nome, i.tipo))
        .execute(&mut *tx)
        .await?;
    if i.simular {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }
    let mut rel = rel;
    rel["simulacao"] = json!(i.simular);
    rel["tipo"] = json!(i.tipo);
    Ok(rel)
}

/// Junta as linhas com o mesmo problema: «Linhas 4, 9 e 12: sem descrição.»
#[derive(Default)]
struct Ignoradas(Vec<(String, Vec<usize>)>);
impl Ignoradas {
    fn junta(&mut self, motivo: &str, linha: usize) {
        match self.0.iter_mut().find(|(m, _)| m == motivo) {
            Some((_, v)) => v.push(linha),
            None => self.0.push((motivo.to_string(), vec![linha])),
        }
    }
    fn total(&self) -> usize {
        self.0.iter().map(|(_, v)| v.len()).sum()
    }
    fn avisos(&self, avisos: &mut Vec<String>) {
        for (motivo, v) in &self.0 {
            let lista = v.iter().take(10).map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
            let mais = if v.len() > 10 { format!(" e mais {}", v.len() - 10) } else { String::new() };
            avisos.push(format!("{} {lista}{mais}: {motivo}; ficaram de fora.", if v.len() == 1 { "Linha" } else { "Linhas" }));
        }
    }
}

async fn artigos(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, i: &ExecutarIn, linhas: &[Vec<String>], base: usize, origem: &str) -> AppResult<Value> {
    let c = &i.colunas;
    let taxas: Vec<(i64, f64, String)> = sqlx::query_as("SELECT id, percentagem, codigo FROM taxas_iva WHERE ativo = 1 ORDER BY id").fetch_all(&mut **tx).await?;
    let normal = taxas.iter().find(|t| t.2 == "NOR").map(|t| t.0).unwrap_or(1);
    let padrao = i.taxa_iva_id.filter(|id| taxas.iter().any(|t| t.0 == *id)).unwrap_or(normal);
    let armazem: i64 = match i.armazem_id {
        Some(a) => a,
        None => sqlx::query_scalar("SELECT id FROM armazens WHERE ativo = 1 ORDER BY id LIMIT 1").fetch_optional(&mut **tx).await?.unwrap_or(1),
    };
    let agora = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut familias: HashMap<String, i64> = HashMap::new();
    let (mut novos, mut existentes, mut actualizados, mut familias_novas, mut com_stock, mut isentos, mut taxa_desconhecida) = (0, 0, 0, 0, 0, 0, 0);
    let mut ign = Ignoradas::default();
    let mut vistas: HashSet<String> = HashSet::new();
    for (n, l) in linhas.iter().enumerate() {
        let linha = base + n;
        let Some(descricao) = valor(l, c, "descricao") else {
            if l.iter().any(|x| !x.is_empty()) {
                ign.junta("sem descrição", linha);
            }
            continue;
        };
        let referencia = valor(l, c, "referencia").map(str::to_string);
        let barras = valor(l, c, "codigo_barras").map(str::to_string);
        let chave = referencia.clone().or_else(|| barras.clone()).unwrap_or_else(|| descricao.to_lowercase());
        if !vistas.insert(chave) {
            ign.junta("repetida(s) na folha", linha);
            continue;
        }
        let taxa = match valor(l, c, "iva").and_then(numero) {
            Some(p) => {
                let p = if p > 0.0 && p < 1.0 { p * 100.0 } else { p };
                match taxas.iter().find(|t| (t.1 - p).abs() < 0.001) {
                    Some(t) => t.0,
                    None => {
                        taxa_desconhecida += 1;
                        padrao
                    }
                }
            }
            None => padrao,
        };
        let percentagem = taxas.iter().find(|t| t.0 == taxa).map(|t| t.1).unwrap_or(14.0);
        if percentagem == 0.0 {
            isentos += 1;
        }
        let preco = valor(l, c, "preco").and_then(centimos).filter(|p| *p >= 0);
        // preço sem IVA e PVP, com a taxa do artigo (a da folha num novo, a que já tem num que existe)
        let precos = |percentagem: f64| match preco {
            Some(p) if i.precos_com_iva => (Some(sem_iva(p, percentagem)), Some(p)),
            Some(p) => (Some(p), Some(com_iva(p, percentagem))),
            None => (None, None),
        };
        let custo = valor(l, c, "custo").and_then(centimos).filter(|p| *p >= 0);
        let familia = match valor(l, c, "familia") {
            Some(f) => {
                let k = f.to_lowercase();
                match familias.get(&k) {
                    Some(id) => Some(*id),
                    None => {
                        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM familias WHERE descricao = ? COLLATE NOCASE ORDER BY id LIMIT 1").bind(f).fetch_optional(&mut **tx).await?;
                        let id = match id {
                            Some(id) => id,
                            None => {
                                familias_novas += 1;
                                sqlx::query("INSERT INTO familias (descricao) VALUES (?)").bind(f).execute(&mut **tx).await?.last_insert_rowid()
                            }
                        };
                        familias.insert(k, id);
                        Some(id)
                    }
                }
            }
            None => None,
        };
        const TAXA: &str = "SELECT a.id, t.percentagem FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE";
        let existente: Option<(i64, f64)> = match (&referencia, &barras) {
            (Some(r), _) => sqlx::query_as(&format!("{TAXA} a.referencia = ?")).bind(r).fetch_optional(&mut **tx).await?,
            (None, Some(b)) => sqlx::query_as(&format!("{TAXA} a.codigo_barras = ?1 OR a.referencia = ?1 ORDER BY a.id LIMIT 1")).bind(b).fetch_optional(&mut **tx).await?,
            (None, None) => sqlx::query_as(&format!("{TAXA} a.descricao = ? COLLATE NOCASE ORDER BY a.id LIMIT 1")).bind(descricao).fetch_optional(&mut **tx).await?,
        };
        if let Some((id, percentagem_actual)) = existente {
            existentes += 1;
            let (preco1, pvp1) = precos(percentagem_actual);
            // um código de barras que já é de outro artigo não se repete
            let barras_livre = match &barras {
                Some(b) => sqlx::query_scalar::<_, i64>("SELECT id FROM artigos WHERE codigo_barras = ? AND id <> ?").bind(b).bind(id).fetch_optional(&mut **tx).await?.is_none(),
                None => false,
            };
            // só se toca no artigo quando alguma coisa muda (a data de alteração leva-o à loja online e à API)
            let r = sqlx::query(
                "UPDATE artigos SET codigo_barras = COALESCE(codigo_barras, ?1), familia_id = COALESCE(familia_id, ?2),
                    preco1 = CASE WHEN ?3 IS NOT NULL AND (?5 OR preco1 = 0) THEN ?3 ELSE preco1 END,
                    pvp1 = CASE WHEN ?3 IS NOT NULL AND (?5 OR preco1 = 0) THEN ?4 ELSE pvp1 END,
                    custo = CASE WHEN ?6 IS NOT NULL AND (?5 OR custo = 0) THEN ?6 ELSE custo END,
                    alterado_em = datetime('now','localtime')
                 WHERE id = ?7 AND ((codigo_barras IS NULL AND ?1 IS NOT NULL) OR (familia_id IS NULL AND ?2 IS NOT NULL)
                    OR (?3 IS NOT NULL AND (?5 OR preco1 = 0) AND (preco1 <> ?3 OR pvp1 IS NOT ?4))
                    OR (?6 IS NOT NULL AND (?5 OR custo = 0) AND custo <> ?6))",
            )
            .bind(barras.as_ref().filter(|_| barras_livre))
            .bind(familia)
            .bind(preco1)
            .bind(pvp1)
            .bind(i.actualizar_precos)
            .bind(custo)
            .bind(id)
            .execute(&mut **tx)
            .await?;
            if r.rows_affected() > 0 {
                actualizados += 1;
            }
            continue;
        }
        if let Some(b) = &barras {
            let dono: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE codigo_barras = ?").bind(b).fetch_optional(&mut **tx).await?;
            if dono.is_some() {
                ign.junta("o código de barras já é de outro artigo", linha);
                continue;
            }
        }
        let referencia = match referencia {
            Some(r) => r,
            None => {
                let mut n: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) + 1 FROM artigos").fetch_one(&mut **tx).await?;
                loop {
                    let r = format!("A{n:05}");
                    let usada: Option<i64> = sqlx::query_scalar("SELECT id FROM artigos WHERE referencia = ?").bind(&r).fetch_optional(&mut **tx).await?;
                    if usada.is_none() {
                        break r;
                    }
                    n += 1;
                }
            }
        };
        let stock = valor(l, c, "stock").and_then(numero).filter(|q| *q > 0.0);
        let custo = custo.unwrap_or(0);
        let (preco1, pvp1) = precos(percentagem);
        let id = sqlx::query(
            "INSERT INTO artigos (referencia, codigo_barras, descricao, tipo, familia_id, unidade_id, taxa_iva_id, custo, custo_medio, preco1, pvp1, controla_stock, alterado_em)
             VALUES (?, ?, ?, 'P', ?, 1, ?, ?, ?, ?, ?, 1, datetime('now','localtime'))",
        )
        .bind(&referencia)
        .bind(&barras)
        .bind(descricao)
        .bind(familia)
        .bind(taxa)
        .bind(custo)
        .bind(custo)
        .bind(preco1.unwrap_or(0))
        .bind(pvp1.unwrap_or(0))
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
        novos += 1;
        if let Some(q) = stock {
            sqlx::query("INSERT INTO stock (artigo_id, armazem_id, quantidade, ultima_entrada) VALUES (?, ?, ?, ?)")
                .bind(id)
                .bind(armazem)
                .bind(q)
                .bind(&agora)
                .execute(&mut **tx)
                .await?;
            sqlx::query("INSERT INTO movimentos_stock (data_hora, artigo_id, armazem_id, quantidade, saldo, custo, descricao) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&agora)
                .bind(id)
                .bind(armazem)
                .bind(q)
                .bind(q)
                .bind(custo)
                .bind(format!("Saldo inicial importado de {origem}"))
                .execute(&mut **tx)
                .await?;
            com_stock += 1;
        }
    }
    let mut avisos = Vec::new();
    ign.avisos(&mut avisos);
    if taxa_desconhecida > 0 {
        avisos.push(format!("{taxa_desconhecida} artigo(s) com uma taxa de IVA que o KanGest não tem: ficaram com a taxa escolhida; confira-os em Artigos."));
    }
    if isentos > 0 {
        avisos.push(format!("{isentos} artigo(s) isento(s) de IVA: ponha-lhes o motivo de isenção em Artigos antes de facturar."));
    }
    if c.contains_key("stock") && existentes > 0 {
        avisos.push(format!("O stock dos {existentes} artigo(s) que já existiam não mudou: acerta-se por um inventário."));
    }
    Ok(json!({
        "linhas": linhas.iter().filter(|l| !l.is_empty()).count(),
        "novos": novos,
        "existentes": existentes,
        "actualizados": actualizados,
        "ignoradas": ign.total(),
        "familias_novas": familias_novas,
        "com_stock": com_stock,
        "avisos": avisos,
    }))
}

async fn entidades(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, i: &ExecutarIn, linhas: &[Vec<String>], base: usize) -> AppResult<Value> {
    let c = &i.colunas;
    let tipo = if i.tipo == "fornecedores" { "F" } else { "C" };
    let (mut novos, mut existentes) = (0, 0);
    let mut ign = Ignoradas::default();
    let mut vistas: HashSet<String> = HashSet::new();
    for (n, l) in linhas.iter().enumerate() {
        let linha = base + n;
        let Some(nome) = valor(l, c, "nome") else {
            if l.iter().any(|x| !x.is_empty()) {
                ign.junta("sem nome", linha);
            }
            continue;
        };
        let nif = valor(l, c, "nif").map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()).filter(|n| n.len() >= 5 && n != "999999999");
        let chave = nif.clone().unwrap_or_else(|| nome.to_lowercase());
        if !vistas.insert(chave) {
            ign.junta("repetida(s) na folha", linha);
            continue;
        }
        let (telefone, email, endereco, localidade) = (valor(l, c, "telefone"), valor(l, c, "email"), valor(l, c, "endereco"), valor(l, c, "localidade"));
        let existente: Option<(i64, String)> = match &nif {
            Some(n) => sqlx::query_as("SELECT id, tipo FROM entidades WHERE nif = ? ORDER BY id LIMIT 1").bind(n).fetch_optional(&mut **tx).await?,
            None => sqlx::query_as("SELECT id, tipo FROM entidades WHERE nome = ? COLLATE NOCASE AND nif = '999999999' ORDER BY id LIMIT 1").bind(nome).fetch_optional(&mut **tx).await?,
        };
        if let Some((id, t)) = existente {
            existentes += 1;
            sqlx::query(
                "UPDATE entidades SET tipo = CASE WHEN tipo <> ?1 THEN 'A' ELSE tipo END, telefone = COALESCE(telefone, ?2), email = COALESCE(email, ?3),
                    endereco = COALESCE(endereco, ?4), localidade = COALESCE(localidade, ?5)
                 WHERE id = ?6 AND (tipo <> ?1 OR (telefone IS NULL AND ?2 IS NOT NULL) OR (email IS NULL AND ?3 IS NOT NULL)
                    OR (endereco IS NULL AND ?4 IS NOT NULL) OR (localidade IS NULL AND ?5 IS NOT NULL))",
            )
            .bind(if t == "A" { "A" } else { tipo })
            .bind(telefone)
            .bind(email)
            .bind(endereco)
            .bind(localidade)
            .bind(id)
            .execute(&mut **tx)
            .await?;
            continue;
        }
        sqlx::query("INSERT INTO entidades (tipo, nome, nif, endereco, localidade, telefone, email) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(tipo)
            .bind(nome)
            .bind(nif.as_deref().unwrap_or("999999999"))
            .bind(endereco)
            .bind(localidade)
            .bind(telefone)
            .bind(email)
            .execute(&mut **tx)
            .await?;
        novos += 1;
    }
    let mut avisos = Vec::new();
    ign.avisos(&mut avisos);
    Ok(json!({ "linhas": linhas.iter().filter(|l| !l.is_empty()).count(), "novos": novos, "existentes": existentes, "ignoradas": ign.total(), "avisos": avisos }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn numeros_escritos_ca() {
        let casos = [
            ("1 234,50", Some(1234.5)),
            ("1.234,50", Some(1234.5)),
            ("1,234.50", Some(1234.5)),
            ("1234.5", Some(1234.5)),
            ("1.500", Some(1500.0)),
            ("12,99", Some(12.99)),
            ("Kz 1 500,00", Some(1500.0)),
            ("14%", Some(14.0)),
            ("-3", Some(-3.0)),
            ("1.234.567", Some(1234567.0)),
            ("", None),
            ("abc", None),
        ];
        for (s, esperado) in casos {
            assert_eq!(numero(s), esperado, "{s}");
        }
        assert_eq!(numero_em_texto(2.125), "2,125");
        assert_eq!(numero_em_texto(1500.0), "1500");
        assert_eq!(numero(&numero_em_texto(0.1 + 0.2)), Some(0.3));
    }

    #[test]
    fn csv_com_aspas_e_acentos() {
        let t = "Ref;Descrição;Preço\r\n\"A1\";\"Arroz \"\"Tio\"\" 5kg\";1 500,00\r\nA2;\"Óleo\nde soja\";900\n";
        let l = separar_csv(t, ';');
        assert_eq!(l.len(), 3);
        assert_eq!(l[1], vec!["A1", "Arroz \"Tio\" 5kg", "1 500,00"]);
        assert_eq!(l[2][1], "Óleo\nde soja");
        // o «CSV» do Excel em português vem em Windows-1252
        let b = encoding_rs::WINDOWS_1252.encode("Descrição;Preço").0;
        assert_eq!(descodificar(&b), "Descrição;Preço");
        assert_eq!(contar_fora_de_aspas("a;\"b;c\";d", ';'), 2);
    }

    #[test]
    fn sugere_as_colunas_pelos_titulos() {
        let cab: Vec<String> = ["Código", "Código de Barras", "Designação", "P.V.P. (Kz)", "Preço de Custo", "IVA %", "Família", "Stock Actual"].iter().map(|s| s.to_string()).collect();
        let s = sugerir(&cab, "artigos");
        assert_eq!(s.get("referencia"), Some(&0));
        assert_eq!(s.get("codigo_barras"), Some(&1));
        assert_eq!(s.get("descricao"), Some(&2));
        assert_eq!(s.get("preco"), Some(&3));
        assert_eq!(s.get("custo"), Some(&4));
        assert_eq!(s.get("iva"), Some(&5));
        assert_eq!(s.get("familia"), Some(&6));
        assert_eq!(s.get("stock"), Some(&7));
        let cab: Vec<String> = ["Nome do Cliente", "Nº Contribuinte", "Telemóvel", "E-mail", "Morada"].iter().map(|s| s.to_string()).collect();
        let s = sugerir(&cab, "entidades");
        assert_eq!((s.get("nome"), s.get("nif"), s.get("telefone"), s.get("email"), s.get("endereco")), (Some(&0), Some(&1), Some(&2), Some(&3), Some(&4)));
    }

    /// Um .xlsx mínimo, como o Excel o grava (ZIP sem compressão), com células de texto em linha e números.
    fn xlsx(linhas: &[(u32, &[(&str, &str)])]) -> Vec<u8> {
        let mut folha = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#);
        for (r, celulas) in linhas {
            folha.push_str(&format!(r#"<row r="{r}">"#));
            for (col, v) in celulas.iter() {
                if numero(v).is_some() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    folha.push_str(&format!(r#"<c r="{col}{r}"><v>{v}</v></c>"#));
                } else {
                    folha.push_str(&format!(r#"<c r="{col}{r}" t="inlineStr"><is><t>{v}</t></is></c>"#));
                }
            }
            folha.push_str("</row>");
        }
        folha.push_str("</sheetData></worksheet>");
        let partes = [
            ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_string()),
            ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_string()),
            ("xl/workbook.xml", r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Artigos" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_string()),
            ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_string()),
            ("xl/worksheets/sheet1.xml", folha),
        ];
        let (mut zip, mut central) = (Vec::new(), Vec::new());
        for (nome, dados) in &partes {
            let mut crc = flate2::Crc::new();
            crc.update(dados.as_bytes());
            let (crc, tam, n, pos) = (crc.sum(), dados.len() as u32, nome.len() as u16, zip.len() as u32);
            for b in [&0x04034b50u32.to_le_bytes()[..], &20u16.to_le_bytes(), &[0; 8], &crc.to_le_bytes(), &tam.to_le_bytes(), &tam.to_le_bytes(), &n.to_le_bytes(), &[0; 2], nome.as_bytes(), dados.as_bytes()] {
                zip.extend_from_slice(b);
            }
            for b in [&0x02014b50u32.to_le_bytes()[..], &20u16.to_le_bytes(), &20u16.to_le_bytes(), &[0; 8], &crc.to_le_bytes(), &tam.to_le_bytes(), &tam.to_le_bytes(), &n.to_le_bytes(), &[0; 12], &pos.to_le_bytes(), nome.as_bytes()] {
                central.extend_from_slice(b);
            }
        }
        let (k, inicio, tam) = (partes.len() as u16, zip.len() as u32, central.len() as u32);
        zip.extend_from_slice(&central);
        for b in [&0x06054b50u32.to_le_bytes()[..], &[0; 4], &k.to_le_bytes(), &k.to_le_bytes(), &tam.to_le_bytes(), &inicio.to_le_bytes(), &[0; 2]] {
            zip.extend_from_slice(b);
        }
        zip
    }

    async fn rpc(st: &AppState, m: &str, p: Value) -> Value {
        match crate::services::dispatch(st, m, p).await {
            Ok(v) => v,
            Err(e) => panic!("{m} falhou: {e}"),
        }
    }

    #[tokio::test]
    async fn importa_artigos_do_excel_e_clientes_do_csv() {
        let pasta = std::env::temp_dir().join(format!("kangest-folha-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        let mut st = AppState::new(db::abrir(&pasta.join("kigest.db")).await.unwrap());
        st.pasta_dados = pasta.clone();
        st.pasta_exportacao = pasta.clone();
        st.carregar_posto(1).await.unwrap();
        rpc(&st, "setup.inicializar", json!({ "empresa": { "nome": "Loja Nova, Lda", "nif": "5000000000" }, "admin": { "nome": "Admin", "login": "admin", "senha": "1234" } })).await;
        rpc(&st, "auth.login", json!({ "login": "admin", "senha": "1234" })).await;
        // já existe, com preço: só se lhe preenche o que falta, a não ser que se peça para actualizar os preços
        rpc(&st, "artigos.guardar", json!({ "referencia": "ARR5", "descricao": "Arroz 5 kg (KanGest)", "taxa_iva_id": 1, "preco1": 120000 })).await;

        // o nome da loja em A1, os títulos na linha 3 a partir da coluna B, uma linha sem descrição e uma repetida
        let livro = xlsx(&[
            (1, &[("A", "Tabela de preços da Loja Antiga")]),
            (3, &[("B", "Código"), ("C", "Código de Barras"), ("D", "Designação"), ("E", "P.V.P. (Kz)"), ("F", "Família"), ("G", "IVA %"), ("H", "Stock")]),
            (4, &[("B", "ARR5"), ("C", "5601234567890"), ("D", "Arroz 5 kg"), ("E", "1425"), ("F", "Mercearia"), ("G", "14"), ("H", "30")]),
            (5, &[("B", "OLE1"), ("D", "Óleo de soja 1 L &amp; tampa"), ("E", "1 140,00"), ("F", "mercearia"), ("G", "14%"), ("H", "12.5")]),
            (6, &[("B", "LIV1"), ("D", "Livro escolar"), ("E", "2500"), ("F", "Papelaria"), ("G", "0")]),
            (7, &[("B", "X9"), ("E", "100")]),
            (8, &[("B", "OLE1"), ("D", "Óleo repetido"), ("E", "1")]),
        ]);
        let f = pasta.join("precos.xlsx");
        std::fs::write(&f, livro).unwrap();
        let caminho = f.display().to_string();

        let a = rpc(&st, "importar.analisarFolha", json!({ "caminho": caminho })).await;
        let folha = &a["folhas"][0];
        assert_eq!((folha["nome"].as_str(), folha["tipo"].as_str(), folha["linha_titulos"].as_i64(), folha["linhas"].as_i64()), (Some("Artigos"), Some("artigos"), Some(2), Some(5)));
        let colunas = folha["colunas"]["artigos"].clone();
        // a A1 tem o nome da loja: as colunas contam-se a partir da A, e «Código» (B) é a 1
        assert_eq!((colunas["referencia"].as_i64(), colunas["codigo_barras"].as_i64(), colunas["descricao"].as_i64(), colunas["stock"].as_i64()), (Some(1), Some(2), Some(3), Some(7)));
        assert_eq!(folha["amostra"][0][3], "Arroz 5 kg");

        let pedido = json!({ "caminho": caminho, "folha": "Artigos", "tipo": "artigos", "colunas": colunas, "simular": true });
        let s = rpc(&st, "importar.executarFolha", pedido.clone()).await;
        assert_eq!((s["simulacao"].as_bool(), s["novos"].as_i64(), s["existentes"].as_i64(), s["ignoradas"].as_i64()), (Some(true), Some(2), Some(1), Some(2)), "{s}");
        assert!(s["avisos"].to_string().contains("Linha 7: sem descrição"), "{}", s["avisos"]);
        assert!(s["avisos"].to_string().contains("Linha 8: repetida"), "{}", s["avisos"]);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artigos").fetch_one(&st.pool).await.unwrap();
        assert_eq!(n, 1, "a simulação não grava");

        let mut real = pedido.clone();
        real["simular"] = json!(false);
        let r = rpc(&st, "importar.executarFolha", real.clone()).await;
        assert_eq!((r["novos"].as_i64(), r["actualizados"].as_i64(), r["familias_novas"].as_i64(), r["com_stock"].as_i64()), (Some(2), Some(1), Some(2), Some(1)), "{r}");
        let (preco, pvp, barras): (i64, i64, Option<String>) = sqlx::query_as("SELECT preco1, pvp1, codigo_barras FROM artigos WHERE referencia = 'ARR5'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((preco, pvp, barras.as_deref()), (120000, 136800, Some("5601234567890")), "o preço que já existia manda; o código de barras em falta preenche-se");
        let (desc, preco, pvp, taxa): (String, i64, i64, f64) = sqlx::query_as(
            "SELECT a.descricao, a.preco1, a.pvp1, t.percentagem FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.referencia = 'OLE1'",
        )
        .fetch_one(&st.pool)
        .await
        .unwrap();
        assert_eq!((desc.as_str(), preco, pvp, taxa), ("Óleo de soja 1 L & tampa", 100000, 114000, 14.0), "o PVP da folha traz o IVA");
        let q: f64 = sqlx::query_scalar("SELECT quantidade FROM stock s JOIN artigos a ON a.id = s.artigo_id WHERE a.referencia = 'OLE1'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(q, 12.5);
        let mov: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM movimentos_stock m JOIN artigos a ON a.id = m.artigo_id WHERE a.referencia = 'OLE1'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(mov, 1, "o saldo inicial fica nos movimentos");
        let (taxa, familias): (f64, i64) = sqlx::query_as(
            "SELECT t.percentagem, (SELECT COUNT(*) FROM familias WHERE descricao = 'mercearia' COLLATE NOCASE) FROM artigos a JOIN taxas_iva t ON t.id = a.taxa_iva_id WHERE a.referencia = 'LIV1'",
        )
        .fetch_one(&st.pool)
        .await
        .unwrap();
        assert_eq!((taxa, familias), (0.0, 1), "«Mercearia» e «mercearia» são a mesma família");
        assert!(r["avisos"].to_string().contains("isento"), "{}", r["avisos"]);

        // outra vez, a pedir para actualizar os preços: nada se duplica e o ARR5 passa ao preço da folha
        real["actualizar_precos"] = json!(true);
        let r2 = rpc(&st, "importar.executarFolha", real).await;
        assert_eq!((r2["novos"].as_i64(), r2["existentes"].as_i64(), r2["actualizados"].as_i64()), (Some(0), Some(3), Some(1)), "só o ARR5 muda de preço: {r2}");
        let pvp: i64 = sqlx::query_scalar("SELECT pvp1 FROM artigos WHERE referencia = 'ARR5'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(pvp, 142500);
        let q: f64 = sqlx::query_scalar("SELECT quantidade FROM stock s JOIN artigos a ON a.id = s.artigo_id WHERE a.referencia = 'OLE1'").fetch_one(&st.pool).await.unwrap();
        assert_eq!(q, 12.5, "o stock dos que já existem não muda");

        // clientes de um «CSV (separado por vírgulas)» do Excel em português: ponto e vírgula e Windows-1252
        rpc(&st, "entidades.guardar", json!({ "tipo": "F", "nome": "Distribuidora Kwanza", "nif": "5417000999" })).await;
        let csv = "Nome do Cliente;Nº Contribuinte;Telemóvel;E-mail\r\nMaria João;;923 000 111;maria@exemplo.ao\r\nDistribuidora Kwanza, Lda;5417000999;222 000 000;\r\n";
        let f = pasta.join("clientes.csv");
        std::fs::write(&f, encoding_rs::WINDOWS_1252.encode(csv).0).unwrap();
        let caminho = f.display().to_string();
        let a = rpc(&st, "importar.analisarFolha", json!({ "caminho": caminho })).await;
        let folha = &a["folhas"][0];
        assert_eq!(folha["tipo"], "clientes");
        assert_eq!(folha["cabecalho"][1], "Nº Contribuinte");
        let colunas = folha["colunas"]["entidades"].clone();
        let r = rpc(&st, "importar.executarFolha", json!({ "caminho": caminho, "tipo": "clientes", "colunas": colunas })).await;
        assert_eq!((r["novos"].as_i64(), r["existentes"].as_i64()), (Some(1), Some(1)), "{r}");
        let (tipo, tel): (String, Option<String>) = sqlx::query_as("SELECT tipo, telefone FROM entidades WHERE nif = '5417000999'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((tipo.as_str(), tel.as_deref()), ("A", Some("222 000 000")), "o fornecedor passa a ser também cliente, e ganha o telefone que faltava");
        let (nif, email): (String, Option<String>) = sqlx::query_as("SELECT nif, email FROM entidades WHERE nome = 'Maria João'").fetch_one(&st.pool).await.unwrap();
        assert_eq!((nif.as_str(), email.as_deref()), ("999999999", Some("maria@exemplo.ao")));
        let r = rpc(&st, "importar.executarFolha", json!({ "caminho": caminho, "tipo": "clientes", "colunas": colunas })).await;
        assert_eq!((r["novos"].as_i64(), r["existentes"].as_i64()), (Some(0), Some(2)), "sem NIF procura-se pelo nome");
        let _ = std::fs::remove_dir_all(&pasta);
    }
}


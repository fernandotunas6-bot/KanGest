//! Impressão térmica directa em ESC/POS e abertura de gaveta (Fr_Impressao do KIGEST POS).
//!
//! Sem ESC/POS o talão sai pelo diálogo de impressão do sistema (HTML), que não corta o papel
//! nem abre a gaveta. Aqui o talão é montado em bytes e enviado em modo "raw" à impressora:
//!
//! - `fila`     — fila de impressão local (`lp -o raw` no macOS/Linux; `copy /b` para a partilha
//!                `\\localhost\<nome>` no Windows, por ser a forma de enviar raw sem drivers);
//! - `rede`     — impressora de rede directa em TCP (porta 9100 por omissão);
//! - `ficheiro` — porta ou dispositivo (ex.: `/dev/usb/lp0`, `\\.\COM3`) ou ficheiro para teste.
//!
//! A configuração é por máquina: fica em `postos.opcoes.escpos`.
//! As escritas são bloqueantes de propósito — um talão são poucos kB e termina em milissegundos.

use std::io::Write;
use std::process::Command;
#[cfg(not(windows))]
use std::process::Stdio;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
pub const VIAS: [&str; 4] = ["Original", "Duplicado", "Triplicado", "Quadruplicado"];

// ============================== configuração ==============================

#[derive(Debug, Clone)]
pub struct Config {
    pub ativo: bool,
    pub ligacao: String,
    pub destino: String,
    pub colunas: usize,
    pub cortar: bool,
    pub acentos: bool,
    /// "nunca" | "numerario" | "sempre"
    pub gaveta: String,
    /// Pino da gaveta no conector RJ11: 2 (quase todas) ou 5.
    pub pino: u8,
    /// Logótipo da empresa já convertido em imagem de 1 bit (feito no ecrã de configuração).
    pub logo: Option<Raster>,
}

/// Imagem de 1 bit para `GS v 0`: `bytes_linha` × 8 pontos de largura, `altura` linhas, 1 = ponto preto.
#[derive(Debug, Clone)]
pub struct Raster {
    pub bytes_linha: u16,
    pub altura: u16,
    pub dados: Vec<u8>,
}

fn raster_de(v: &Value) -> Option<Raster> {
    let bytes_linha = u16::try_from(v["bytes_linha"].as_u64()?).ok()?;
    let altura = u16::try_from(v["altura"].as_u64()?).ok()?;
    let dados = B64.decode(v["dados"].as_str()?).ok()?;
    let ok = (1..=128).contains(&bytes_linha) && (1..=1200).contains(&altura) && dados.len() == bytes_linha as usize * altura as usize;
    ok.then_some(Raster { bytes_linha, altura, dados })
}

fn config(st: &AppState) -> Config {
    let o = st.posto().opcoes;
    let e = o.get("escpos").cloned().unwrap_or(Value::Null);
    Config {
        ativo: e["ativo"].as_bool().unwrap_or(false),
        ligacao: e["ligacao"].as_str().unwrap_or("fila").to_string(),
        destino: e["destino"].as_str().unwrap_or("").trim().to_string(),
        colunas: e["colunas"].as_u64().unwrap_or(42).clamp(24, 64) as usize,
        cortar: e["cortar"].as_bool().unwrap_or(true),
        acentos: e["acentos"].as_bool().unwrap_or(true),
        gaveta: e["gaveta"].as_str().unwrap_or("numerario").to_string(),
        pino: if e["pino"].as_u64() == Some(5) { 5 } else { 2 },
        logo: if e["logo_ativo"].as_bool().unwrap_or(false) { raster_de(&e["logo"]) } else { None },
    }
}

fn exigir_config(st: &AppState) -> AppResult<Config> {
    let c = config(st);
    if !c.ativo {
        return Err(AppError::msg(
            "A impressão directa (ESC/POS) não está activada neste posto. Active-a em Configurações → Impressora térmica.",
        ));
    }
    if c.destino.is_empty() {
        return Err(AppError::msg("Indique o destino da impressora térmica nas opções do posto."));
    }
    Ok(c)
}

// ============================== codificação ==============================

/// Página de código 850 (multilingue), a que os clones POS80 trazem de origem.
fn cp850(c: char) -> Option<u8> {
    Some(match c {
        'á' => 0xA0, 'à' => 0x85, 'â' => 0x83, 'ã' => 0xC6, 'ä' => 0x84,
        'é' => 0x82, 'è' => 0x8A, 'ê' => 0x88, 'ë' => 0x89,
        'í' => 0xA1, 'ì' => 0x8D, 'î' => 0x8C, 'ï' => 0x8B,
        'ó' => 0xA2, 'ò' => 0x95, 'ô' => 0x93, 'õ' => 0xE4, 'ö' => 0x94,
        'ú' => 0xA3, 'ù' => 0x97, 'û' => 0x96, 'ü' => 0x81,
        'ç' => 0x87, 'ñ' => 0xA4,
        'Á' => 0xB5, 'À' => 0xB7, 'Â' => 0xB6, 'Ã' => 0xC7, 'Ä' => 0x8E,
        'É' => 0x90, 'È' => 0xD4, 'Ê' => 0xD2,
        'Í' => 0xD6, 'Ì' => 0xDE, 'Î' => 0xD7,
        'Ó' => 0xE0, 'Ò' => 0xE3, 'Ô' => 0xE2, 'Õ' => 0xE5, 'Ö' => 0x99,
        'Ú' => 0xE9, 'Ù' => 0xEB, 'Û' => 0xEA, 'Ü' => 0x9A,
        'Ç' => 0x80, 'Ñ' => 0xA5,
        'º' => 0xA7, 'ª' => 0xA6, '·' => 0xFA, '°' => 0xF8,
        _ => return None,
    })
}

/// Equivalente sem acentos, para impressoras que só têm ASCII.
fn ascii(c: char) -> &'static str {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => "a",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => "o",
        'ú' | 'ù' | 'û' | 'ü' => "u",
        'ç' => "c",
        'ñ' => "n",
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => "A",
        'É' | 'È' | 'Ê' | 'Ë' => "E",
        'Í' | 'Ì' | 'Î' | 'Ï' => "I",
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => "O",
        'Ú' | 'Ù' | 'Û' | 'Ü' => "U",
        'Ç' => "C",
        'Ñ' => "N",
        'º' => "o",
        'ª' => "a",
        '·' => "-",
        '°' => "o",
        '€' => "EUR",
        // espaços e sinais tipográficos dos valores formatados e das descrições
        '\u{a0}' | '\u{202f}' | '\u{2009}' => " ",
        '\u{2212}' | '–' | '—' => "-",
        '“' | '”' | '«' | '»' => "\"",
        '‘' | '’' => "'",
        '…' => "...",
        '×' => "x",
        _ => "?",
    }
}

fn codificar(s: &str, acentos: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            // o LF é escrito pelas funções de linha; aqui ignora-se qualquer controlo
            if c == '\n' || c == '\r' || c == '\t' {
                out.push(b' ');
            } else {
                out.push(c as u8);
            }
        } else if acentos {
            match cp850(c) {
                Some(b) => out.push(b),
                None => out.extend_from_slice(ascii(c).as_bytes()),
            }
        } else {
            out.extend_from_slice(ascii(c).as_bytes());
        }
    }
    out
}

// ============================== montagem do talão ==============================

struct Talao {
    b: Vec<u8>,
    cols: usize,
    acentos: bool,
}

impl Talao {
    fn novo(c: &Config) -> Self {
        let mut t = Talao { b: Vec::with_capacity(2048), cols: c.colunas, acentos: c.acentos };
        t.cmd(&[ESC, b'@']); // inicializar
        // 48 colunas em papel de 80 mm só cabem com a letra pequena (fonte B); 32 e 42 usam a fonte A
        t.cmd(&[ESC, b'M', if c.colunas >= 48 { 1 } else { 0 }]);
        if c.acentos {
            t.cmd(&[ESC, b't', 2]); // página de código 850
        }
        t
    }
    fn cmd(&mut self, b: &[u8]) {
        self.b.extend_from_slice(b);
    }
    fn centrar(&mut self, on: bool) {
        self.cmd(&[ESC, b'a', if on { 1 } else { 0 }]);
    }
    fn negrito(&mut self, on: bool) {
        self.cmd(&[ESC, b'E', on as u8]);
    }
    fn grande(&mut self, on: bool) {
        self.cmd(&[GS, b'!', if on { 0x11 } else { 0x00 }]);
    }
    fn nl(&mut self) {
        self.b.push(b'\n');
    }
    /// Uma linha de texto, quebrada à largura do papel.
    fn linha(&mut self, s: &str) {
        for parte in envolver(s, self.cols) {
            self.b.extend_from_slice(&codificar(&parte, self.acentos));
            self.nl();
        }
    }
    /// Linha já alinhada em colunas: escreve-se tal como está (sem juntar os espaços), cortada à largura.
    fn fixa(&mut self, s: &str) {
        let s: String = s.chars().take(self.cols).collect();
        self.b.extend_from_slice(&codificar(&s, self.acentos));
        self.nl();
    }
    /// Etiqueta à esquerda, valor à direita, na mesma linha.
    fn par(&mut self, k: &str, v: &str) {
        let v: String = v.chars().take(self.cols).collect();
        let livre = self.cols.saturating_sub(v.chars().count());
        let k: String = k.chars().take(livre.saturating_sub(1)).collect();
        let espacos = livre - k.chars().count();
        let s = format!("{k}{}{v}", " ".repeat(espacos));
        self.b.extend_from_slice(&codificar(&s, self.acentos));
        self.nl();
    }
    fn separador(&mut self) {
        self.b.extend_from_slice("-".repeat(self.cols).as_bytes());
        self.nl();
    }
    /// Imagem de 1 bit (`GS v 0`), em bandas de até 128 linhas para as impressoras com pouca memória.
    fn imagem(&mut self, r: &Raster) {
        self.centrar(true);
        let linha = r.bytes_linha as usize;
        let total = (r.altura as usize * linha).min(r.dados.len());
        for banda in r.dados[..total].chunks(linha * 128) {
            let alt = (banda.len() / linha) as u16;
            self.cmd(&[GS, b'v', b'0', 0, (r.bytes_linha & 0xFF) as u8, (r.bytes_linha >> 8) as u8, (alt & 0xFF) as u8, (alt >> 8) as u8]);
            self.cmd(banda);
        }
        self.centrar(false);
    }
    fn gaveta(&mut self, pino: u8) {
        // ESC p m t1 t2 — impulso de ~50 ms no pino 2 (m=0) ou 5 (m=1)
        self.cmd(&[ESC, b'p', if pino == 5 { 1 } else { 0 }, 25, 250]);
    }
    fn fim(&mut self, cortar: bool) {
        self.centrar(false);
        self.cmd(&[ESC, b'd', 4]); // avança 4 linhas para libertar o papel
        if cortar {
            self.cmd(&[GS, b'V', 1]); // corte parcial
        }
    }
}

fn envolver(s: &str, cols: usize) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return vec![String::new()];
    }
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in s.split_whitespace() {
        let n = palavra.chars().count();
        if n > cols {
            // palavra maior que o papel (ex.: referência longa): corta-se
            if !atual.is_empty() {
                linhas.push(std::mem::take(&mut atual));
            }
            let chars: Vec<char> = palavra.chars().collect();
            for pedaco in chars.chunks(cols) {
                linhas.push(pedaco.iter().collect());
            }
            continue;
        }
        if atual.is_empty() {
            atual = palavra.to_string();
        } else if atual.chars().count() + 1 + n <= cols {
            atual.push(' ');
            atual.push_str(palavra);
        } else {
            linhas.push(std::mem::take(&mut atual));
            atual = palavra.to_string();
        }
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    linhas
}

// ============================== formatação ==============================

/// Cêntimos em formato angolano: 1.234.567,89
fn kz(c: i64) -> String {
    let neg = c < 0;
    let v = c.unsigned_abs();
    let inteiros = (v / 100).to_string();
    let mut com_pontos = String::new();
    for (i, d) in inteiros.chars().enumerate() {
        if i > 0 && (inteiros.len() - i) % 3 == 0 {
            com_pontos.push('.');
        }
        com_pontos.push(d);
    }
    format!("{}{com_pontos},{:02}", if neg { "-" } else { "" }, v % 100)
}

fn qtd(x: f64) -> String {
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    s.replace('.', ",")
}

fn pct(x: f64) -> String {
    format!("{}%", qtd(x))
}

/// "2026-11-10" → "10-11-2026"
fn data_curta(s: &str) -> String {
    match s.get(..10).map(|d| d.split('-').collect::<Vec<_>>()).as_deref() {
        Some([a, m, dia]) => format!("{dia}/{m}/{a}"),
        _ => s.to_string(),
    }
}

/// "2026-11-10T14:33:02" ou "2026-11-10 14:33:02" → "10-11-2026 14:33"
fn data_hora(s: &str) -> String {
    let s = s.replace('T', " ");
    let (d, h) = s.split_once(' ').unwrap_or((s.as_str(), ""));
    let data = match d.split('-').collect::<Vec<_>>()[..] {
        [a, m, dia] => format!("{dia}/{m}/{a}"),
        _ => d.to_string(),
    };
    let hora: String = h.chars().take(5).collect();
    if hora.is_empty() {
        data
    } else {
        format!("{data} {hora}")
    }
}

fn txt(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").trim().to_string()
}
fn num(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn real(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(0.0)
}
fn lista<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v[k].as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

const MOTIVOS: [(&str, &str); 3] = [
    ("M00", "Regime Transitório"),
    ("M02", "Transmissão de bens e serviço não sujeita"),
    ("M04", "IVA – Regime de Não Sujeição"),
];

/// Rodapé obrigatório: caracteres de controlo da assinatura + n.º do certificado AGT.
fn rodape_agt(d: &Value) -> Option<String> {
    let doc = &d["documento"];
    let ctrl = txt(doc, "hash_controlo");
    if ctrl.is_empty() {
        return None;
    }
    let importado = num(doc, "importado") == 1;
    let campo = if importado { "certificado_anterior" } else { "certificado" };
    let cert = txt(&d["empresa"], campo);
    Some(if importado {
        format!("{ctrl}-Processado por programa válido n.º {}", if cert.is_empty() { "(KIGEST)".into() } else { cert })
    } else {
        format!("{ctrl}-Processado por programa válido n.º {}/AGT", if cert.is_empty() { "(por certificar)".into() } else { cert })
    })
}

/// Larguras das colunas Qtd · Un · P. Unit · Desc.% · Taxa% · Valor para `cols` caracteres
/// (48 no talão de 80 mm com a fonte B, 32 no de 58 mm); o valor fica com o que sobra.
fn larguras(cols: usize) -> [usize; 6] {
    let [q, u, p, d, t] = if cols >= 42 { [5, 4, 11, 6, 6] } else { [4, 3, 8, 4, 4] };
    [q, u, p, d, t, cols.saturating_sub(q + u + p + d + t).max(6)]
}

/// Uma fila de colunas: a primeira e a segunda alinhadas à esquerda, as outras à direita.
fn colunas_linha(cols: usize, campos: [&str; 6]) -> String {
    let w = larguras(cols);
    let mut s = String::new();
    for (k, (c, l)) in campos.iter().zip(w).enumerate() {
        let c: String = c.chars().take(l.saturating_sub(if k == 5 { 0 } else { 1 })).collect();
        let n = c.chars().count();
        if k < 2 {
            s.push_str(&c);
            s.push_str(&" ".repeat(l - n));
        } else {
            s.push_str(&" ".repeat(l - n));
            s.push_str(&c);
        }
    }
    s
}

/// Talão de um documento com o desenho do Fatura.fr3 do KIGEST POS (igual ao talão em HTML): empresa ao centro,
/// título e n.º, data e hora, cliente, linhas em duas filas, totais, modo de pagamento, resumo de imposto,
/// código de certificação, operador e «OBRIGADO VOLTE SEMPRE».
fn render(cfg: &Config, d: &Value, via: &str) -> Vec<u8> {
    let doc = &d["documento"];
    let e = &d["empresa"];
    let mut t = Talao::novo(cfg);
    let classe = txt(doc, "classe");
    let recibo = classe == "RECIBO" || classe == "PAGAMENTO";

    if let Some(logo) = &cfg.logo {
        t.imagem(logo);
    }
    t.centrar(true);
    t.negrito(true);
    let nome = txt(e, "nome");
    t.linha(&nome);
    t.negrito(false);
    let comercial = txt(e, "designacao_comercial");
    if !comercial.is_empty() && comercial != nome {
        t.linha(&comercial);
    }
    let endereco = txt(e, "endereco");
    if !endereco.is_empty() {
        t.linha(&endereco);
    }
    let tel = txt(e, "telefone");
    if !tel.is_empty() {
        t.linha(&format!("Tel.: {tel}"));
    }
    t.linha(&format!("NIF: {}", txt(e, "nif")));
    let local = [txt(e, "localidade"), txt(e, "provincia")].into_iter().find(|x| !x.is_empty()).unwrap_or_else(|| "Luanda".into());
    t.linha(&format!("{local} - Angola"));
    if via.starts_with("2.") {
        t.nl();
        t.negrito(true);
        t.linha("2.ª via em conformidade com o original");
        t.negrito(false);
    }
    t.centrar(false);
    t.nl();
    t.negrito(true);
    t.par(&txt(doc, "numero_doc"), &txt(doc, "tipo_desc"));
    t.negrito(false);
    if !via.starts_with("2.") && via != "Original" {
        t.par("", via);
    }
    let dh = txt(doc, "data_hora");
    t.par(&format!("Data: {}", data_curta(&txt(doc, "data_emissao"))), &format!("Hora: {}", dh.get(11..16).unwrap_or("")));
    t.linha(&format!("Cliente : {}", txt(doc, "entidade_nome")));
    let nif = txt(doc, "entidade_nif");
    t.linha(&format!("NIF : {}", if nif == "999999999" { "Consumidor Final".into() } else { nif }));
    // nota de crédito/débito: documento de origem e motivo; factura a crédito: vencimento (como no A4)
    let origem = txt(doc, "doc_origem");
    if !origem.is_empty() {
        t.linha(&format!("Ref.: {origem}"));
    }
    let motivo = txt(doc, "motivo");
    if !motivo.is_empty() {
        t.linha(&format!("Motivo: {motivo}"));
    }
    let venc = txt(doc, "data_vencimento");
    if !venc.is_empty() && venc.get(..10) != txt(doc, "data_emissao").get(..10) && !recibo {
        t.linha(&format!("{}: {}", if classe == "PROFORMA" { "Válido até" } else { "Vencimento" }, data_curta(&venc)));
    }
    if classe == "PROFORMA" || recibo || classe == "TRANSPORTE" {
        t.centrar(true);
        t.linha("Este documento não serve de factura");
        t.centrar(false);
    }
    if txt(doc, "estado") == "A" {
        t.centrar(true);
        t.negrito(true);
        t.linha("*** DOCUMENTO ANULADO ***");
        t.negrito(false);
        t.centrar(false);
    }

    let cols = t.cols;
    if recibo {
        t.separador();
        t.negrito(true);
        t.par("Documento", "Valor");
        t.negrito(false);
        for l in lista(d, "liquidacoes") {
            t.par(&format!("{} {}", txt(l, "numero_doc"), data_curta(&txt(l, "data_emissao"))), &kz(num(l, "valor")));
        }
    } else {
        t.nl();
        t.negrito(true);
        t.linha("Descrição");
        t.fixa(&colunas_linha(cols, ["Qtd", "Un", "P. Unit", "Desc%", "Taxa%", "Valor"]));
        t.negrito(false);
        t.separador();
        for l in lista(d, "linhas") {
            t.linha(&txt(l, "descricao"));
            let un = txt(l, "unidade");
            let desc = real(l, "desconto_pct");
            t.fixa(&colunas_linha(
                cols,
                [
                    &qtd(real(l, "quantidade")),
                    if un.is_empty() { "un" } else { &un },
                    &kz(num(l, "preco_unitario")),
                    &if desc > 0.0 { pct(desc) } else { "0".into() },
                    &pct(real(l, "taxa_iva_pct")),
                    &kz(num(l, "total")),
                ],
            ));
            let motivo = txt(l, "motivo_isencao");
            if !motivo.is_empty() {
                let texto = MOTIVOS.iter().find(|(c, _)| *c == motivo).map(|(_, d)| *d).unwrap_or("Isento");
                t.linha(&format!("{motivo} {texto}"));
            }
        }
    }
    t.separador();

    if !recibo {
        t.par("Total sem Imposto", &kz(num(doc, "total_bruto")));
        t.par("Descontos", &kz(num(doc, "total_desconto")));
        t.par("Total Imposto", &kz(num(doc, "total_iva")));
    }
    t.negrito(true);
    t.grande(true);
    // em corpo duplo cabem metade das colunas
    t.cols = cols / 2;
    let moeda = Some(txt(doc, "moeda")).filter(|m| !m.is_empty()).unwrap_or_else(|| "AOA".into());
    t.par(&format!("Total ({moeda})"), &kz(num(doc, "total")));
    t.cols = cols;
    t.grande(false);
    t.negrito(false);
    let entregue = num(doc, "valor_entregue");
    if entregue > 0 {
        t.par("Valor Entregue", &kz(entregue));
    }
    let troco = num(doc, "troco");
    if troco > 0 {
        t.par("Troco", &kz(troco));
    }

    let pagamentos = lista(d, "pagamentos");
    if !pagamentos.is_empty() {
        t.separador();
        t.centrar(true);
        t.linha("Modo de Pagamento");
        t.centrar(false);
        t.separador();
        for p in pagamentos {
            let extra = if txt(p, "modo_codigo") == "NU" { troco } else { 0 };
            t.par(&txt(p, "descricao"), &kz(num(p, "valor") + extra));
        }
    }
    let resumo = lista(d, "resumo_iva");
    if !recibo && !resumo.is_empty() {
        t.separador();
        t.centrar(true);
        t.linha("Resumo de Imposto");
        t.centrar(false);
        t.separador();
        let w = cols / 3;
        let tres = |a: &str, b: &str, c: &str| format!("{a:<w$}{b:^w$}{c:>r$}", r = cols - 2 * w);
        t.fixa(&tres("Taxa", "Incidência", "Total Imposto"));
        for r in resumo {
            let motivo = txt(r, "motivo_isencao");
            t.fixa(&tres(&format!("{}{}", pct(real(r, "taxa")), if motivo.is_empty() { String::new() } else { format!(" {motivo}") }), &kz(num(r, "base")), &kz(num(r, "iva"))));
        }
    }

    t.nl();
    t.centrar(true);
    if classe == "VENDA" {
        t.linha("Os bens e/ou serviços foram colocados à disposição do adquirente na data e local do documento.");
    }
    // Facturação electrónica: QR code da AGT (consulta do documento no Quiosque AGT)
    if let Some(fe) = d.get("fe").filter(|f| f.is_object()) {
        let url = url_agt(&txt(fe, "nif"), &txt(doc, "numero_doc"));
        t.cmd(&qr_escpos(&url));
        t.nl();
        t.linha(if txt(fe, "estado") == "VALIDO" { "Documento validado pela AGT" } else { "Documento comunicado à AGT" });
    }
    if let Some(r) = rodape_agt(d) {
        t.linha(&r);
    }
    t.centrar(false);
    t.linha(&format!("Operador: {}", txt(doc, "utilizador")));
    t.centrar(true);
    if classe == "PROFORMA" {
        for b in lista(e, "bancos") {
            t.linha(&format!("{}: {}", txt(b, "banco"), txt(b, "iban")));
        }
    }
    let texto_rodape = txt(doc, "texto_rodape");
    if !texto_rodape.is_empty() {
        t.linha(&texto_rodape);
    }
    let rodape = txt(e, "rodape");
    if !rodape.is_empty() {
        t.linha(&rodape);
    }
    t.linha("* OBRIGADO VOLTE SEMPRE *");
    t.fim(cfg.cortar);
    t.b
}

/// Endereço do QR code da Facturação Electrónica (espaços do n.º como %20).
pub fn url_agt(nif: &str, documento: &str) -> String {
    format!("https://quiosqueagt.minfin.gov.ao/facturacao-eletronica/consultar-fe?emissor={nif}&document={}", documento.replace(' ', "%20"))
}

/// QR code nativo da impressora (ESC/POS `GS ( k`): modelo 2, módulo 6, correcção M.
pub fn qr_escpos(dados: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[GS, b'(', b'k', 4, 0, 49, 65, 50, 0]); // modelo 2
    b.extend_from_slice(&[GS, b'(', b'k', 3, 0, 49, 67, 6]); // tamanho do módulo
    b.extend_from_slice(&[GS, b'(', b'k', 3, 0, 49, 69, 49]); // correcção M
    let n = dados.len() + 3;
    b.extend_from_slice(&[GS, b'(', b'k', (n & 0xff) as u8, (n >> 8) as u8, 49, 80, 48]);
    b.extend_from_slice(dados.as_bytes());
    b.extend_from_slice(&[GS, b'(', b'k', 3, 0, 49, 81, 48]); // imprime
    b
}

#[cfg(test)]
mod testes_impressoras {
    #[test]
    fn nomes_com_espacos_no_windows() {
        let win = "EPSON TM-T20II Receipt\r\nMicrosoft Print to PDF\r\n\r\n";
        assert_eq!(super::nomes_impressoras(win, true), vec!["EPSON TM-T20II Receipt", "Microsoft Print to PDF"]);
        let mac = "POS80 accepting requests since Mon\nHP_LaserJet accepting requests since Tue\n";
        assert_eq!(super::nomes_impressoras(mac, false), vec!["POS80", "HP_LaserJet"]);
    }
}

#[cfg(test)]
mod testes_qr {
    #[test]
    fn qr_escpos_bytes() {
        let url = super::url_agt("5001636863", "FT FT6325S2C/10006");
        assert_eq!(url, "https://quiosqueagt.minfin.gov.ao/facturacao-eletronica/consultar-fe?emissor=5001636863&document=FT%20FT6325S2C/10006");
        let b = super::qr_escpos(&url);
        let n = url.len() + 3;
        let i = b.windows(3).position(|w| w == [49, 80, 48]).unwrap();
        assert_eq!((b[i - 2] as usize) | ((b[i - 1] as usize) << 8), n, "pL pH = tamanho + 3");
        assert!(b.ends_with(&[0x1D, b'(', b'k', 3, 0, 49, 81, 48]));
    }
}

// ============================== envio ==============================

fn enviar(cfg: &Config, bytes: &[u8]) -> AppResult<()> {
    match cfg.ligacao.as_str() {
        "rede" => rede(&cfg.destino, bytes),
        "ficheiro" => {
            // append: nunca apaga o que já saiu (num dispositivo é indiferente, num ficheiro de teste acumula)
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&cfg.destino)
                .map_err(|e| AppError::msg(format!("Não foi possível abrir {}: {e}", cfg.destino)))?;
            f.write_all(bytes).map_err(|e| AppError::msg(format!("Falha a escrever em {}: {e}", cfg.destino)))?;
            f.flush().ok();
            Ok(())
        }
        _ => fila(&cfg.destino, bytes),
    }
}

fn rede(destino: &str, bytes: &[u8]) -> AppResult<()> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let alvo = if destino.contains(':') { destino.to_string() } else { format!("{destino}:9100") };
    let addr = alvo
        .to_socket_addrs()
        .map_err(|e| AppError::msg(format!("Endereço de impressora inválido ({alvo}): {e}")))?
        .next()
        .ok_or_else(|| AppError::msg(format!("Endereço de impressora inválido: {alvo}")))?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(4))
        .map_err(|e| AppError::msg(format!("Sem resposta da impressora em {alvo}: {e}")))?;
    s.set_write_timeout(Some(Duration::from_secs(5))).ok();
    s.write_all(bytes).map_err(|e| AppError::msg(format!("Falha a enviar para {alvo}: {e}")))?;
    s.flush().ok();
    Ok(())
}

#[cfg(windows)]
fn fila(destino: &str, bytes: &[u8]) -> AppResult<()> {
    // No Windows o caminho sem drivers para enviar bytes crus é copiar para a partilha da impressora.
    let tmp = std::env::temp_dir().join(format!("kigest-talao-{}.prn", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| AppError::msg(format!("Não foi possível gravar o talão: {e}")))?;
    let alvo = if destino.starts_with("\\\\") { destino.to_string() } else { format!("\\\\localhost\\{destino}") };
    let mut comando = Command::new("cmd");
    sem_consola(&mut comando);
    let r = comando
        .args(["/c", "copy", "/b"])
        .arg(&tmp)
        .arg(&alvo)
        .output()
        .map_err(|e| AppError::msg(format!("Não foi possível executar o comando de impressão: {e}")))?;
    std::fs::remove_file(&tmp).ok();
    if !r.status.success() {
        let erro = String::from_utf8_lossy(if r.stderr.is_empty() { &r.stdout } else { &r.stderr }).trim().to_string();
        return Err(AppError::msg(format!(
            "A impressora «{destino}» não aceitou o talão. Partilhe a impressora com esse nome ou use a ligação por rede. {erro}"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn fila(destino: &str, bytes: &[u8]) -> AppResult<()> {
    let mut filho = Command::new("lp")
        .args(["-d", destino, "-o", "raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::msg(format!("Não foi possível executar o «lp»: {e}")))?;
    filho
        .stdin
        .as_mut()
        .ok_or_else(|| AppError::msg("Não foi possível enviar o talão para o «lp»."))?
        .write_all(bytes)
        .map_err(|e| AppError::msg(format!("Falha a enviar o talão: {e}")))?;
    let r = filho.wait_with_output().map_err(|e| AppError::msg(format!("Falha a imprimir: {e}")))?;
    if !r.status.success() {
        let erro = String::from_utf8_lossy(&r.stderr).trim().to_string();
        return Err(AppError::msg(format!("A fila «{destino}» recusou o talão. {erro}")));
    }
    Ok(())
}

/// No Windows os comandos auxiliares (cmd, powershell) correm sem abrir uma consola por cima do KanGest.
#[cfg(windows)]
pub fn sem_consola(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn sem_consola(_c: &mut Command) {}

/// Filas de impressão da máquina, para a lista de escolha na configuração.
fn impressoras() -> Vec<String> {
    #[cfg(windows)]
    let saida = {
        let mut c = Command::new("powershell");
        sem_consola(&mut c);
        c.args(["-NoProfile", "-Command", "Get-Printer | Select-Object -ExpandProperty Name"]).output()
    };
    #[cfg(not(windows))]
    let saida = Command::new("lpstat").arg("-a").output();

    let Ok(r) = saida else { return Vec::new() };
    nomes_impressoras(&String::from_utf8_lossy(&r.stdout), cfg!(windows))
}

/// Windows (Get-Printer): um nome por linha, com espaços («EPSON TM-T20II Receipt»).
/// macOS/Linux (lpstat -a): o nome é a primeira palavra de cada linha.
fn nomes_impressoras(saida: &str, windows: bool) -> Vec<String> {
    saida
        .lines()
        .map(str::trim)
        .filter_map(|l| if windows { Some(l) } else { l.split_whitespace().next() })
        .filter(|n| !n.is_empty())
        .map(String::from)
        .collect()
}

// ============================== métodos ==============================

/// Configuração actual + filas disponíveis (para o ecrã de configuração do posto).
pub async fn estado(st: &AppState) -> AppResult<Value> {
    st.sessao()?;
    let c = config(st);
    Ok(json!({
        "config": {
            "ativo": c.ativo, "ligacao": c.ligacao, "destino": c.destino, "colunas": c.colunas,
            "cortar": c.cortar, "acentos": c.acentos, "gaveta": c.gaveta, "pino": c.pino,
        },
        "impressoras": impressoras(),
        "sistema": std::env::consts::OS,
    }))
}

#[derive(Deserialize)]
pub struct TalaoIn {
    pub id: i64,
    #[serde(default)]
    pub vias: Option<usize>,
    /// Marca as cópias como 2.ª via (reimpressão).
    #[serde(default)]
    pub segunda_via: bool,
    /// Força (ou impede) a abertura da gaveta, independentemente da configuração.
    #[serde(default)]
    pub gaveta: Option<bool>,
}

/// Imprime o talão de um documento na impressora térmica e, se configurado, abre a gaveta.
pub async fn talao(st: &AppState, i: TalaoIn) -> AppResult<Value> {
    let cfg = exigir_config(st)?;
    let d = crate::rede::chamar(st, "documentos.obter", json!({ "id": i.id })).await?;
    let vias = i.vias.unwrap_or(1).clamp(1, 4);
    let numerario = lista(&d, "pagamentos").iter().any(|p| txt(p, "modo_codigo") == "NU");
    let abrir = i.gaveta.unwrap_or(match cfg.gaveta.as_str() {
        "sempre" => true,
        "numerario" => numerario,
        _ => false,
    });

    let mut bytes = Vec::new();
    for v in 0..vias {
        let via = if i.segunda_via {
            format!("2.ª via ({})", VIAS[v.min(3)])
        } else {
            VIAS[v.min(3)].to_string()
        };
        bytes.extend_from_slice(&render(&cfg, &d, &via));
    }
    if abrir {
        let mut g = Talao::novo(&cfg);
        g.gaveta(cfg.pino);
        bytes.extend_from_slice(&g.b);
    }
    enviar(&cfg, &bytes)?;
    Ok(json!({ "vias": vias, "gaveta": abrir }))
}

// ============================== visor de cliente ==============================
//
// Visor de coluna (VFD de 2 linhas × 20) ligado a uma porta série (COM no Windows, /dev/cu.* no macOS).
// Protocolos: «cd5220» (ESC Q A/B — o da maioria dos visores) e «escpos» (Epson DM-D: HOME + 2 linhas).
// O visor em segundo ecrã (terminais de ecrã duplo) é uma janela da aplicação e não passa por aqui.

#[derive(Debug, Clone)]
pub struct VisorCfg {
    pub modo: String,
    pub porta: String,
    pub velocidade: u32,
    pub colunas: usize,
    pub protocolo: String,
}

fn visor_config(st: &AppState) -> VisorCfg {
    let o = st.posto().opcoes;
    let v = o.get("visor").cloned().unwrap_or(Value::Null);
    VisorCfg {
        modo: v["modo"].as_str().unwrap_or("nenhum").to_string(),
        porta: v["porta"].as_str().unwrap_or("").trim().to_string(),
        velocidade: v["velocidade"].as_u64().map(|x| x as u32).filter(|x| [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200].contains(x)).unwrap_or(9600),
        colunas: v["colunas"].as_u64().unwrap_or(20).clamp(16, 40) as usize,
        protocolo: v["protocolo"].as_str().unwrap_or("cd5220").to_string(),
    }
}

/// Duas linhas já ajustadas à largura do visor (sem acentos: os VFD só têm ASCII seguro).
pub fn visor_bytes(protocolo: &str, colunas: usize, l1: &str, l2: &str) -> Vec<u8> {
    let ajustar = |s: &str| -> Vec<u8> {
        let mut b = codificar(s, false);
        b.truncate(colunas);
        b.resize(colunas, b' ');
        b
    };
    let (a, b) = (ajustar(l1), ajustar(l2));
    let mut out = Vec::with_capacity(colunas * 2 + 8);
    if protocolo == "escpos" {
        out.push(0x0B); // HOME: cursor no início da linha 1; escrever 2 × colunas preenche as duas linhas
        out.extend_from_slice(&a);
        out.extend_from_slice(&[0x1F, b'$', 1, 2]); // cursor na coluna 1 da linha 2
        out.extend_from_slice(&b);
    } else {
        out.extend_from_slice(&[ESC, b'Q', b'A']);
        out.extend_from_slice(&a);
        out.push(0x0D);
        out.extend_from_slice(&[ESC, b'Q', b'B']);
        out.extend_from_slice(&b);
        out.push(0x0D);
    }
    out
}

#[cfg(windows)]
fn caminho_porta(p: &str) -> String {
    let u = p.to_uppercase();
    if (u.starts_with("COM") || u.starts_with("LPT")) && !p.starts_with(r"\\") {
        format!(r"\\.\{p}")
    } else {
        p.to_string()
    }
}
#[cfg(not(windows))]
fn caminho_porta(p: &str) -> String {
    p.to_string()
}

/// Velocidade da porta (uma vez por porta e velocidade em cada arranque).
#[cfg(windows)]
fn configurar_porta(p: &str, vel: u32) {
    let nome = p.trim_start_matches(r"\\.\");
    if !nome.to_uppercase().starts_with("COM") {
        return;
    }
    let mut c = Command::new("cmd");
    sem_consola(&mut c);
    let _ = c.args(["/C", "mode", &format!("{nome}:"), &format!("BAUD={vel}"), "PARITY=N", "DATA=8", "STOP=1"]).output();
}
#[cfg(target_os = "macos")]
fn configurar_porta(p: &str, vel: u32) {
    let _ = Command::new("stty").args(["-f", p, &vel.to_string()]).output();
}
#[cfg(all(unix, not(target_os = "macos")))]
fn configurar_porta(p: &str, vel: u32) {
    let _ = Command::new("stty").args(["-F", p, &vel.to_string(), "raw", "-echo"]).output();
}

static PORTAS_CONFIGURADAS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();

/// Escreve na porta do visor com um tempo limite: uma porta pendurada (sem visor, controlo de fluxo)
/// não pode prender a venda.
fn escrever_porta(porta: &str, velocidade: u32, bytes: Vec<u8>) -> AppResult<()> {
    if porta.is_empty() {
        return Err(AppError::msg("Indique a porta do visor de cliente (ex.: COM3 ou /dev/cu.usbserial)."));
    }
    let chave = format!("{porta}@{velocidade}");
    let feitas = PORTAS_CONFIGURADAS.get_or_init(Default::default);
    if feitas.lock().map(|f| !f.contains(&chave)).unwrap_or(true) {
        configurar_porta(porta, velocidade);
        if let Ok(mut f) = feitas.lock() {
            f.insert(chave);
        }
    }
    let caminho = caminho_porta(porta);
    let porta_txt = porta.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = std::fs::OpenOptions::new()
            .write(true)
            .open(&caminho)
            .and_then(|mut f| f.write_all(&bytes).and_then(|_| f.flush()));
        let _ = tx.send(r);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(AppError::msg(format!("Visor de cliente em {porta_txt}: {e}"))),
        Err(_) => Err(AppError::msg(format!("O visor de cliente em {porta_txt} não respondeu."))),
    }
}

#[derive(Deserialize)]
pub struct VisorIn {
    #[serde(default)]
    pub linha1: String,
    #[serde(default)]
    pub linha2: String,
}

/// Mostra duas linhas no visor de cliente deste posto (sem visor de porta série configurado, não faz nada).
pub async fn visor(st: &AppState, v: VisorIn) -> AppResult<Value> {
    st.sessao()?;
    let cfg = visor_config(st);
    if cfg.modo != "serie" {
        return Ok(json!({ "visor": false }));
    }
    let bytes = visor_bytes(&cfg.protocolo, cfg.colunas, &v.linha1, &v.linha2);
    escrever_porta(&cfg.porta, cfg.velocidade, bytes)?;
    Ok(json!({ "visor": true }))
}

/// Teste do visor: régua de colunas na linha 2 para conferir a largura.
pub async fn visor_teste(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("pos.config_posto")?;
    let cfg = visor_config(st);
    if cfg.modo != "serie" {
        return Err(AppError::msg("Escolha «Visor de coluna (porta série)» e grave antes de testar."));
    }
    let regua: String = (1..=cfg.colunas).map(|i| char::from(b'0' + (i % 10) as u8)).collect();
    let bytes = visor_bytes(&cfg.protocolo, cfg.colunas, "KanGest - visor OK", &regua);
    escrever_porta(&cfg.porta, cfg.velocidade, bytes)?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize, Default)]
pub struct GavetaIn {
    /// Venda acabada de emitir: abre (ou não) conforme a regra configurada.
    /// Sem documento é o botão do POS, que abre sempre e exige permissão.
    #[serde(default)]
    pub documento_id: Option<i64>,
}

/// Abre a gaveta do dinheiro.
pub async fn gaveta(st: &AppState, g: GavetaIn) -> AppResult<Value> {
    let cfg = match g.documento_id {
        None => {
            st.exigir_perm("caixa.movimentos")?;
            exigir_config(st)?
        }
        Some(id) => {
            let c = config(st);
            if !c.ativo || c.destino.is_empty() || c.gaveta == "nunca" {
                return Ok(json!({ "gaveta": false }));
            }
            if c.gaveta == "numerario" {
                let d = crate::rede::chamar(st, "documentos.obter", json!({ "id": id })).await?;
                if !lista(&d, "pagamentos").iter().any(|p| txt(p, "modo_codigo") == "NU") {
                    return Ok(json!({ "gaveta": false }));
                }
            }
            c
        }
    };
    let mut t = Talao::novo(&cfg);
    t.gaveta(cfg.pino);
    enviar(&cfg, &t.b)?;
    Ok(json!({ "gaveta": true }))
}

/// Talão de teste: confirma a impressora, as colunas, os acentos, o corte e a gaveta.
pub async fn teste(st: &AppState) -> AppResult<Value> {
    st.exigir_perm("pos.config_posto")?;
    let cfg = exigir_config(st)?;
    let empresa = crate::rede::chamar(st, "empresa.obter", Value::Null).await?;
    let posto = st.posto();
    let mut t = Talao::novo(&cfg);
    t.centrar(true);
    t.negrito(true);
    t.linha(&txt(&empresa, "nome"));
    t.negrito(false);
    t.linha("TESTE DE IMPRESSÃO");
    t.centrar(false);
    t.separador();
    t.linha(&format!("Posto: {} (#{})", posto.descricao, posto.id));
    t.linha(&format!("Ligação: {} → {}", cfg.ligacao, cfg.destino));
    t.linha(&format!("Colunas: {}", cfg.colunas));
    t.linha(&format!("Data: {}", data_hora(&crate::db::agora())));
    t.linha("Acentuação: ação, português, José, Nº 1 — çÇãõáé");
    t.separador();
    t.par("Exemplo de valor", &kz(1234567));
    t.par("Quantidade", &qtd(2.5));
    t.negrito(true);
    t.grande(true);
    let cols = t.cols;
    t.cols = cols / 2;
    t.par("TOTAL", &kz(1234567));
    t.cols = cols;
    t.grande(false);
    t.negrito(false);
    t.separador();
    t.centrar(true);
    t.linha("Se este talão saiu direito, a impressora está pronta.");
    t.fim(cfg.cortar);
    if cfg.gaveta != "nunca" {
        t.gaveta(cfg.pino);
    }
    let bytes = t.b.clone();
    enviar(&cfg, &bytes)?;
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod testes_equipamento {
    use super::*;

    fn cfg(colunas: usize) -> Config {
        Config {
            ativo: true,
            ligacao: "ficheiro".into(),
            destino: "x".into(),
            colunas,
            cortar: true,
            acentos: false,
            gaveta: "nunca".into(),
            pino: 2,
            logo: None,
        }
    }
    fn vazio(doc: Value) -> Value {
        json!({ "documento": doc, "empresa": { "nome": "Livraria" }, "linhas": [], "pagamentos": [], "resumo_iva": [] })
    }
    fn texto(c: &Config, d: &Value) -> String {
        String::from_utf8_lossy(&render(c, d, "Original")).to_string()
    }

    #[test]
    fn letra_pequena_nas_48_colunas() {
        let d = vazio(json!({}));
        assert!(render(&cfg(48), &d, "Original").windows(3).any(|w| w == [ESC, b'M', 1]));
        assert!(render(&cfg(42), &d, "Original").windows(3).any(|w| w == [ESC, b'M', 0]));
        assert!(render(&cfg(32), &d, "Original").windows(3).any(|w| w == [ESC, b'M', 0]));
    }

    #[test]
    fn origem_motivo_e_vencimento_como_no_a4() {
        let nc = vazio(json!({ "tipo_desc": "Nota de Crédito", "numero_doc": "NC N2026/3", "doc_origem": "FT N2026/10", "motivo": "Devolução de livro com defeito",
                               "data_emissao": "2026-11-12", "data_vencimento": "2026-11-12", "data_hora": "2026-11-12T10:00:00", "classe": "NOTA_CREDITO" }));
        let t = texto(&cfg(42), &nc);
        assert!(t.contains("Ref.: FT N2026/10"), "{t}");
        assert!(t.contains("Motivo: Devolucao de livro com"), "{t}");
        assert!(!t.contains("Vencimento"));
        let ft = vazio(json!({ "data_emissao": "2026-11-12", "data_vencimento": "2026-12-12", "classe": "VENDA" }));
        assert!(texto(&cfg(42), &ft).contains("Vencimento: 12/12/2026"));
        let pp = vazio(json!({ "data_emissao": "2026-11-12", "data_vencimento": "2026-11-27", "classe": "PROFORMA" }));
        assert!(texto(&cfg(42), &pp).contains("Valido ate: 27/11/2026"));
    }

    #[test]
    fn logotipo_em_bandas_de_128_linhas() {
        let mut c = cfg(42);
        c.logo = Some(Raster { bytes_linha: 2, altura: 200, dados: vec![0xAA; 400] });
        let b = render(&c, &vazio(json!({})), "Original");
        let cabecalhos: Vec<usize> = b.windows(4).enumerate().filter(|(_, w)| *w == [GS, b'v', b'0', 0]).map(|(i, _)| i).collect();
        assert_eq!(cabecalhos.len(), 2, "200 linhas = 128 + 72");
        assert_eq!(&b[cabecalhos[0] + 4..cabecalhos[0] + 8], &[2, 0, 128, 0]);
        assert_eq!(&b[cabecalhos[1] + 4..cabecalhos[1] + 8], &[2, 0, 72, 0]);
        // configuração vinda do ecrã: base64 com o tamanho certo; tamanho errado é ignorado
        let ok = json!({ "bytes_linha": 2, "altura": 3, "dados": B64.encode([1u8, 2, 3, 4, 5, 6]) });
        assert!(raster_de(&ok).is_some());
        let mau = json!({ "bytes_linha": 2, "altura": 4, "dados": B64.encode([1u8, 2, 3, 4, 5, 6]) });
        assert!(raster_de(&mau).is_none());
    }

    #[test]
    fn visor_de_coluna_nos_dois_protocolos() {
        let b = visor_bytes("cd5220", 20, "Arroz Tio Lucas 5kg Grande", "TOTAL\u{a0}\u{a0}\u{a0}4\u{a0}360,50");
        assert_eq!(b.len(), 3 + 20 + 1 + 3 + 20 + 1);
        assert_eq!(&b[..3], &[ESC, b'Q', b'A']);
        assert_eq!(&b[3..23], b"Arroz Tio Lucas 5kg ");
        assert_eq!(&b[24..27], &[ESC, b'Q', b'B']);
        assert_eq!(&b[27..47], b"TOTAL   4 360,50    ");
        let e = visor_bytes("escpos", 20, "Olá", "Troco \u{2212}");
        assert_eq!(e[0], 0x0B);
        assert_eq!(&e[1..21], b"Ola                 ");
        assert_eq!(&e[21..25], &[0x1F, b'$', 1, 2]);
        assert_eq!(&e[25..32], b"Troco -");
    }

    #[test]
    fn valores_formatados_sem_pontos_de_interrogacao() {
        assert_eq!(codificar("1\u{a0}234,50 \u{2212}5 — «Livro»", false), b"1 234,50 -5 - \"Livro\"".to_vec());
    }
}

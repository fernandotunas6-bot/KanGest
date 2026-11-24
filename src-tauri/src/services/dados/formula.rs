// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Fórmulas do construtor de relatórios: indicadores calculados a partir das colunas do relatório, como
//! `{soma:lucro} / {soma:total} * 100` (margem %) ou `{soma:total} / {contar}` (valor médio por documento).
//!
//! Segurança: a fórmula nunca entra no SQL como texto. Um analisador pequeno lê só colunas entre chavetas (que têm de
//! existir no relatório e ser números), números, `+ − × ÷` e parênteses; o SQL monta-se com as expressões dessas
//! colunas, já validadas pelo registo das fontes. Dividir por zero dá vazio (`NULLIF`).

use crate::error::{AppError, AppResult};

/// Tamanho máximo de uma fórmula e profundidade máxima dos parênteses.
const MAX_TEXTO: usize = 300;
const MAX_PROFUNDIDADE: usize = 30;

#[derive(Debug, Clone, PartialEq)]
enum Peca {
    Numero(f64),
    Coluna(String),
    Op(char),
    Abre,
    Fecha,
}

fn erro(m: impl Into<String>) -> AppError {
    AppError::msg(m)
}

fn pecas(s: &str) -> AppResult<Vec<Peca>> {
    let cs: Vec<char> = s.chars().collect();
    let mut v = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '+' => {
                v.push(Peca::Op('+'));
                i += 1;
            }
            '-' | '−' => {
                v.push(Peca::Op('-'));
                i += 1;
            }
            '*' | '×' => {
                v.push(Peca::Op('*'));
                i += 1;
            }
            '/' | '÷' => {
                v.push(Peca::Op('/'));
                i += 1;
            }
            '(' => {
                v.push(Peca::Abre);
                i += 1;
            }
            ')' => {
                v.push(Peca::Fecha);
                i += 1;
            }
            '{' => {
                let fim = cs[i + 1..].iter().position(|x| *x == '}').ok_or_else(|| erro("Falta fechar uma coluna com «}» na fórmula."))?;
                let nome: String = cs[i + 1..i + 1 + fim].iter().collect();
                let nome = nome.trim();
                if nome.is_empty() {
                    return Err(erro("Há uma coluna vazia («{}») na fórmula."));
                }
                v.push(Peca::Coluna(nome.to_string()));
                i += fim + 2;
            }
            c if c.is_ascii_digit() || c == '.' || c == ',' => {
                let ini = i;
                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.' || cs[i] == ',') {
                    i += 1;
                }
                let texto: String = cs[ini..i].iter().collect::<String>().replace(',', ".");
                let n: f64 = texto.parse().map_err(|_| erro(format!("Número inválido na fórmula: «{}».", cs[ini..i].iter().collect::<String>())))?;
                if !n.is_finite() {
                    return Err(erro("Número inválido na fórmula."));
                }
                v.push(Peca::Numero(n));
            }
            outro => {
                return Err(erro(format!(
                    "Na fórmula só entram colunas entre chavetas, números, + − × ÷ e parênteses (não «{outro}»)."
                )))
            }
        }
    }
    Ok(v)
}

struct Leitor<'a> {
    p: Vec<Peca>,
    i: usize,
    prof: usize,
    coluna: &'a dyn Fn(&str) -> AppResult<String>,
}

impl Leitor<'_> {
    fn op(&self) -> Option<char> {
        match self.p.get(self.i) {
            Some(Peca::Op(o)) => Some(*o),
            _ => None,
        }
    }

    // soma e subtracção
    fn expressao(&mut self) -> AppResult<String> {
        let mut a = self.termo()?;
        while let Some(o @ ('+' | '-')) = self.op() {
            self.i += 1;
            let b = self.termo()?;
            a = format!("({a} {o} {b})");
        }
        Ok(a)
    }

    // multiplicação e divisão (a divisão em vírgula flutuante e sem dividir por zero)
    fn termo(&mut self) -> AppResult<String> {
        let mut a = self.factor()?;
        while let Some(o @ ('*' | '/')) = self.op() {
            self.i += 1;
            let b = self.factor()?;
            a = if o == '*' { format!("({a} * {b})") } else { format!("({a} * 1.0 / NULLIF({b}, 0))") };
        }
        Ok(a)
    }

    fn factor(&mut self) -> AppResult<String> {
        self.prof += 1;
        if self.prof > MAX_PROFUNDIDADE {
            return Err(erro("A fórmula tem parênteses a mais."));
        }
        let r = match self.p.get(self.i).cloned() {
            Some(Peca::Op('-')) => {
                self.i += 1;
                format!("(-{})", self.factor()?)
            }
            Some(Peca::Op('+')) => {
                self.i += 1;
                self.factor()?
            }
            Some(Peca::Numero(n)) => {
                self.i += 1;
                literal(n)
            }
            Some(Peca::Coluna(nome)) => {
                self.i += 1;
                format!("({})", (self.coluna)(&nome)?)
            }
            Some(Peca::Abre) => {
                self.i += 1;
                let e = self.expressao()?;
                if self.p.get(self.i) != Some(&Peca::Fecha) {
                    return Err(erro("Falta fechar um parêntese na fórmula."));
                }
                self.i += 1;
                format!("({e})")
            }
            _ => return Err(erro("A fórmula está incompleta: falta um número ou uma coluna.")),
        };
        self.prof -= 1;
        Ok(r)
    }
}

fn literal(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// O SQL de uma fórmula. `coluna` dá a expressão SQL de uma coluna do relatório pela chave escrita entre chavetas
/// (ou recusa-a, se não existir ou não for um número).
pub fn sql(texto: &str, coluna: &dyn Fn(&str) -> AppResult<String>) -> AppResult<String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Err(erro("Escreva a fórmula."));
    }
    if texto.chars().count() > MAX_TEXTO {
        return Err(erro(format!("A fórmula tem mais de {MAX_TEXTO} caracteres.")));
    }
    let p = pecas(texto)?;
    let mut l = Leitor { p, i: 0, prof: 0, coluna };
    let e = l.expressao()?;
    if l.i != l.p.len() {
        return Err(erro(match l.p[l.i] {
            Peca::Fecha => "Há um parêntese a fechar a mais na fórmula.".to_string(),
            _ => "Falta um operador (+ − × ÷) entre dois valores da fórmula.".to_string(),
        }));
    }
    Ok(e)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn col(nome: &str) -> AppResult<String> {
        match nome {
            "soma:lucro" => Ok("SUM(l)".into()),
            "soma:total" => Ok("SUM(t)".into()),
            "contar" => Ok("COUNT(*)".into()),
            _ => Err(erro(format!("A coluna «{nome}» não está no relatório."))),
        }
    }

    #[test]
    fn formulas_viram_sql_seguro() {
        assert_eq!(sql("{soma:lucro} / {soma:total} * 100", &col).unwrap(), "(((SUM(l)) * 1.0 / NULLIF((SUM(t)), 0)) * 100)");
        assert_eq!(sql("{soma:total} ÷ {contar}", &col).unwrap(), "((SUM(t)) * 1.0 / NULLIF((COUNT(*)), 0))");
        assert_eq!(sql("({soma:total} − {soma:lucro}) × 1,5", &col).unwrap(), "((((SUM(t)) - (SUM(l)))) * 1.5)");
        assert_eq!(sql("-{soma:lucro} + 2", &col).unwrap(), "((-(SUM(l))) + 2)");
        // o que não é fórmula é recusado antes de chegar ao SQL
        assert!(sql("{soma:total}; DROP TABLE documentos", &col).unwrap_err().to_string().contains("só entram"));
        assert!(sql("{soma:total} || 'x'", &col).unwrap_err().to_string().contains("só entram"));
        assert!(sql("SUM(t)", &col).unwrap_err().to_string().contains("só entram"));
        assert!(sql("{d.hash}", &col).unwrap_err().to_string().contains("não está no relatório"));
        assert!(sql("{soma:total} {contar}", &col).unwrap_err().to_string().contains("Falta um operador"));
        assert!(sql("({soma:total}", &col).unwrap_err().to_string().contains("parêntese"));
        assert!(sql("{soma:total})", &col).unwrap_err().to_string().contains("a mais"));
        assert!(sql("{soma:total} /", &col).unwrap_err().to_string().contains("incompleta"));
        assert!(sql("{soma:total", &col).unwrap_err().to_string().contains("«}»"));
        assert!(sql("1.2.3", &col).unwrap_err().to_string().contains("Número inválido"));
        assert!(sql(&"(".repeat(40), &col).unwrap_err().to_string().contains("parênteses a mais"));
        assert!(sql("   ", &col).unwrap_err().to_string().contains("Escreva"));
    }
}

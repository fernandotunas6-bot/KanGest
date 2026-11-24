// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

//! Fontes do motor central de dados: o que se pode consultar, com que permissão, e os campos de cada fonte com a sua
//! expressão SQL fixa. É um registo declarativo, como o das fichas da auditoria (`services/auditoria.rs`): o motor de
//! consultas (`consulta.rs`) só monta SQL com estas expressões, nunca com texto vindo do ecrã.
//!
//! As fontes de vendas usam a mesma regra dos relatórios (`relatorios::VENDAS` e `relatorios::SINAL`: facturas,
//! facturas-recibo e notas de débito somam, notas de crédito subtraem, anulados de fora), para os totais baterem certo.
//! `{VENDAS}` e `{SINAL}` substituem-se ao montar a consulta.

/// Tipo do campo: diz como se filtra, como se soma e como se mostra.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tipo {
    Texto,
    Numero,
    /// cêntimos (inteiros), mostrados em Kz
    Dinheiro,
    /// AAAA-MM-DD
    Data,
    Logico,
}

impl Tipo {
    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Texto => "texto",
            Tipo::Numero => "numero",
            Tipo::Dinheiro => "dinheiro",
            Tipo::Data => "data",
            Tipo::Logico => "logico",
        }
    }
    /// Pode somar-se ou fazer-se a média.
    pub const fn quantitativo(self) -> bool {
        matches!(self, Tipo::Numero | Tipo::Dinheiro)
    }
}

pub struct Campo {
    pub id: &'static str,
    pub rotulo: &'static str,
    pub tipo: Tipo,
    /// expressão SQL sobre as tabelas de `Fonte::de`
    pub expr: &'static str,
    /// soma-se no total e no agrupamento (um preço unitário ou uma taxa não: só a média, o mínimo e o máximo)
    pub somavel: bool,
}

pub struct Fonte {
    pub id: &'static str,
    pub rotulo: &'static str,
    pub descricao: &'static str,
    /// uma das permissões que já existem (`state::PERMISSOES`)
    pub permissao: &'static str,
    /// `tabelas … WHERE condição base`: tem sempre um WHERE (os filtros juntam-se com AND)
    pub de: &'static str,
    /// o campo de data principal (o período do relatório)
    pub data: &'static str,
    pub campos: &'static [Campo],
}

impl Fonte {
    pub fn campo(&self, id: &str) -> Option<&Campo> {
        self.campos.iter().find(|c| c.id == id)
    }
}

const fn c(id: &'static str, rotulo: &'static str, tipo: Tipo, expr: &'static str) -> Campo {
    Campo { id, rotulo, tipo, expr, somavel: tipo.quantitativo() }
}

/// Um número que não se soma (preço unitário, taxa): só média, mínimo e máximo.
const fn nao_somavel(id: &'static str, rotulo: &'static str, tipo: Tipo, expr: &'static str) -> Campo {
    Campo { id, rotulo, tipo, expr, somavel: false }
}

use Tipo::*;

pub const FONTES: &[Fonte] = &[
    Fonte {
        id: "vendas",
        rotulo: "Vendas (documentos)",
        descricao: "Facturas, facturas-recibo, notas de débito e notas de crédito (a subtrair), sem os anulados",
        permissao: "relatorios.ver",
        de: "documentos d LEFT JOIN utilizadores u ON u.id = d.utilizador_id LEFT JOIN utilizadores v ON v.id = d.vendedor_id
             LEFT JOIN postos p ON p.id = d.posto_id LEFT JOIN armazens ar ON ar.id = d.armazem_id WHERE {VENDAS}",
        data: "data",
        campos: &[
            c("data", "Data", Data, "d.data_emissao"),
            c("mes", "Mês", Texto, "substr(d.data_emissao, 1, 7)"),
            c("ano", "Ano", Texto, "substr(d.data_emissao, 1, 4)"),
            c("documento", "Documento", Texto, "d.numero_doc"),
            c("tipo", "Tipo de documento", Texto, "d.tipo_codigo"),
            c("cliente", "Cliente", Texto, "d.entidade_nome"),
            c("nif", "NIF do cliente", Texto, "d.entidade_nif"),
            c("operador", "Operador", Texto, "u.nome"),
            c("vendedor", "Vendedor", Texto, "v.nome"),
            c("posto", "Posto", Texto, "p.descricao"),
            c("loja", "Loja (armazém)", Texto, "ar.nome"),
            c("canal", "Canal", Texto, "CASE WHEN d.api_chave_id IS NULL THEN 'Balcão' ELSE 'API / loja online' END"),
            c("importado", "Histórico importado", Logico, "d.importado"),
            c("liquido", "Valor sem IVA", Dinheiro, "{SINAL}*d.total_liquido"),
            c("desconto", "Descontos", Dinheiro, "{SINAL}*d.total_desconto"),
            c("iva", "IVA", Dinheiro, "{SINAL}*d.total_iva"),
            c("total", "Total", Dinheiro, "{SINAL}*d.total"),
            c("pendente", "Por receber", Dinheiro, "d.valor_pendente"),
        ],
    },
    Fonte {
        id: "linhas_venda",
        rotulo: "Vendas por artigo (linhas)",
        descricao: "Cada linha vendida, com o artigo, a família, a quantidade, o custo e o lucro",
        permissao: "relatorios.ver",
        de: "documento_linhas l JOIN documentos d ON d.id = l.documento_id LEFT JOIN artigos a ON a.id = l.artigo_id
             LEFT JOIN familias f ON f.id = a.familia_id LEFT JOIN utilizadores u ON u.id = d.utilizador_id
             LEFT JOIN armazens ar ON ar.id = d.armazem_id WHERE {VENDAS}",
        data: "data",
        campos: &[
            c("data", "Data", Data, "d.data_emissao"),
            c("mes", "Mês", Texto, "substr(d.data_emissao, 1, 7)"),
            c("ano", "Ano", Texto, "substr(d.data_emissao, 1, 4)"),
            c("documento", "Documento", Texto, "d.numero_doc"),
            c("cliente", "Cliente", Texto, "d.entidade_nome"),
            c("operador", "Operador", Texto, "u.nome"),
            c("loja", "Loja (armazém)", Texto, "ar.nome"),
            c("referencia", "Referência", Texto, "l.referencia"),
            c("artigo", "Artigo", Texto, "l.descricao"),
            c("familia", "Família", Texto, "COALESCE(f.descricao, '(sem família)')"),
            nao_somavel("taxa_iva", "Taxa de IVA (%)", Numero, "l.taxa_iva_pct"),
            c("quantidade", "Quantidade", Numero, "{SINAL}*l.quantidade"),
            nao_somavel("preco", "Preço unitário (sem IVA)", Dinheiro, "l.preco_unitario"),
            c("liquido", "Valor sem IVA", Dinheiro, "{SINAL}*l.valor_liquido"),
            c("iva", "IVA", Dinheiro, "{SINAL}*l.valor_iva"),
            c("total", "Total", Dinheiro, "{SINAL}*l.total"),
            c("custo", "Custo", Dinheiro, "{SINAL}*CAST(ROUND(l.custo * l.quantidade) AS INTEGER)"),
            c("lucro", "Lucro", Dinheiro, "{SINAL}*(l.valor_liquido - CAST(ROUND(l.custo * l.quantidade) AS INTEGER))"),
        ],
    },
    Fonte {
        id: "tesouraria",
        rotulo: "Tesouraria (entradas e saídas)",
        descricao: "O fluxo de todas as contas: recebimentos e pagamentos dos documentos, caixa e movimentos de tesouraria",
        permissao: "tesouraria.fluxo",
        de: "vw_fluxo f LEFT JOIN contas c ON c.id = f.conta_id WHERE f.anulado = 0",
        data: "data",
        campos: &[
            c("data", "Data", Data, "f.data"),
            c("mes", "Mês", Texto, "substr(f.data, 1, 7)"),
            c("ano", "Ano", Texto, "substr(f.data, 1, 4)"),
            c("conta", "Conta", Texto, "c.descricao"),
            c("tipo", "Entrada ou saída", Texto, "CASE f.tipo WHEN 'ENTRADA' THEN 'Entrada' ELSE 'Saída' END"),
            c("origem", "Origem", Texto, "f.origem"),
            c("documento", "Documento", Texto, "f.documento"),
            c("descricao", "Descrição", Texto, "f.descricao"),
            c("rubrica", "Rubrica / forma de pagamento", Texto, "f.rubrica"),
            c("entrada", "Entradas", Dinheiro, "CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE 0 END"),
            c("saida", "Saídas", Dinheiro, "CASE WHEN f.tipo = 'ENTRADA' THEN 0 ELSE f.valor END"),
            c("saldo", "Saldo (entradas − saídas)", Dinheiro, "CASE WHEN f.tipo = 'ENTRADA' THEN f.valor ELSE -f.valor END"),
        ],
    },
];

pub fn fonte(id: &str) -> Option<&'static Fonte> {
    FONTES.iter().find(|f| f.id == id)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn registo_coerente() {
        let mut ids = std::collections::HashSet::new();
        for f in FONTES {
            assert!(ids.insert(f.id), "fonte repetida: {}", f.id);
            assert!(f.de.contains(" WHERE "), "{}: o FROM tem de acabar num WHERE (os filtros juntam-se com AND)", f.id);
            assert!(crate::state::PERMISSOES.contains(&f.permissao), "{}: permissão desconhecida {}", f.id, f.permissao);
            assert!(f.campo(f.data).is_some_and(|c| c.tipo == Data), "{}: o campo de data tem de existir", f.id);
            let mut campos = std::collections::HashSet::new();
            for c in f.campos {
                assert!(campos.insert(c.id), "{}: campo repetido {}", f.id, c.id);
                // os ids vão para o ecrã e para os modelos guardados: só letras minúsculas, algarismos e _
                assert!(c.id.chars().all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_'), "{}.{}", f.id, c.id);
            }
        }
    }
}

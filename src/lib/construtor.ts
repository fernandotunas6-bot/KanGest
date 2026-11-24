// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// O que o construtor de relatórios partilha com o resto do programa: a definição de um relatório (o que um modelo
// guarda), os períodos relativos, os relatórios prontos (que o painel abre ao clicar num indicador) e a comparação
// com outro período. Tudo corre pelo motor central de dados (services/dados): aqui só se decide o que pedir.
import { hojeISO } from "@/lib/format";
import type { AgregadoDados, CampoFonte, FormulaDados, OrdemDados, Operador, ResultadoDados, ValorDados } from "@/lib/dados";

export type Modo = "resumo" | "detalhe";

/** Um filtro como está no ecrã (o valor em texto, convertido só ao consultar). */
export interface FiltroEcra {
  campo: string;
  op: Operador;
  texto: string;
}

/** Período relativo: um modelo guardado com «este mês» abre sempre no mês em curso. */
export type Periodo = "hoje" | "mes" | "mes_anterior" | "ano" | "ano_anterior" | "30dias" | "personalizado";
export const PERIODOS: [Periodo, string][] = [
  ["hoje", "Hoje"],
  ["mes", "Este mês"],
  ["mes_anterior", "Mês passado"],
  ["ano", "Este ano"],
  ["ano_anterior", "Ano passado"],
  ["30dias", "Últimos 30 dias"],
  ["personalizado", "Personalizado"],
];

export const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export function datasDe(p: Periodo): [string, string] | null {
  const hoje = hojeISO();
  const a = Number(hoje.slice(0, 4));
  const m = Number(hoje.slice(5, 7));
  switch (p) {
    case "hoje":
      return [hoje, hoje];
    case "mes":
      return [`${hoje.slice(0, 8)}01`, hoje];
    case "mes_anterior":
      return [iso(new Date(a, m - 2, 1)), iso(new Date(a, m - 1, 0))];
    case "ano":
      return [`${a}-01-01`, hoje];
    case "ano_anterior":
      return [`${a - 1}-01-01`, `${a - 1}-12-31`];
    case "30dias": {
      const d = new Date();
      d.setDate(d.getDate() - 29);
      return [iso(d), hoje];
    }
    default:
      return null;
  }
}

/** Comparar o resumo com outro período: o anterior (do mesmo tamanho) ou o mesmo do ano anterior. */
export type Comparar = "" | "anterior" | "ano";
export const COMPARACOES: [Comparar, string][] = [
  ["", "Sem comparação"],
  ["anterior", "Período anterior"],
  ["ano", "Mesmo período do ano anterior"],
];

/** O que um modelo guarda (dados.modeloGuardar): tudo o que define o relatório. */
export interface Definicao {
  modo: Modo;
  periodo: Periodo;
  de: string;
  ate: string;
  campos: string[];
  agrupar: string[];
  agregados: AgregadoDados[];
  filtros: FiltroEcra[];
  ordenar: OrdemDados[];
  vista: "tabela" | "grafico";
  /** os modelos antigos não têm: sem comparação */
  comparar?: Comparar;
  /** indicadores calculados (os modelos antigos não têm) */
  formulas?: FormulaDados[];
}

const dataDe = (s: string) => new Date(Number(s.slice(0, 4)), Number(s.slice(5, 7)) - 1, Number(s.slice(8, 10)));
const ultimoDia = (a: number, m: number) => new Date(a, m + 1, 0).getDate();

/** Soma meses a uma data, encostada ao fim do mês (31/03 − 1 mês = 28/02; o último dia de um mês dá o último do outro). */
function somarMeses(s: string, n: number): string {
  const a = Number(s.slice(0, 4));
  const m = Number(s.slice(5, 7)) - 1;
  const dia = Number(s.slice(8, 10));
  const alvo = new Date(a, m + n, 1);
  const fim = ultimoDia(alvo.getFullYear(), alvo.getMonth());
  return iso(new Date(alvo.getFullYear(), alvo.getMonth(), dia === ultimoDia(a, m) ? fim : Math.min(dia, fim)));
}

/**
 * O período com que se compara. «Ano anterior»: as mesmas datas um ano antes. «Anterior»: um mês (ou o mês até hoje)
 * compara com o mês anterior até ao mesmo dia; um ano (ou o ano até hoje) com o ano anterior; outro intervalo com os
 * mesmos dias imediatamente antes (hoje com ontem, os últimos 30 dias com os 30 antes, mesmo que comecem num dia 1).
 */
export function periodoComparado(de: string, ate: string, c: Exclude<Comparar, "">, periodo?: Periodo): [string, string] {
  const mesmoMes = de.slice(0, 7) === ate.slice(0, 7);
  const mesmoAno = de.slice(0, 4) === ate.slice(0, 4);
  const porDias = periodo === "hoje" || periodo === "30dias";
  if (c === "ano") return [somarMeses(de, -12), somarMeses(ate, -12)];
  if (!porDias && de.endsWith("-01-01") && mesmoAno && !mesmoMes) return [somarMeses(de, -12), somarMeses(ate, -12)];
  if (!porDias && de.endsWith("-01") && mesmoMes) return [somarMeses(de, -1), somarMeses(ate, -1)];
  const dias = Math.round((dataDe(ate).getTime() - dataDe(de).getTime()) / 86_400_000) + 1;
  const ini = dataDe(de);
  const fimAnt = new Date(ini.getFullYear(), ini.getMonth(), ini.getDate() - 1);
  const iniAnt = new Date(ini.getFullYear(), ini.getMonth(), ini.getDate() - dias);
  return [iso(iniAnt), iso(fimAnt)];
}

/** Variação em percentagem (uma casa), ou null sem base de comparação. */
function variacao(x: ValorDados | undefined, y: ValorDados | undefined): number | null {
  const b = Number(y ?? 0);
  if (!b) return null;
  return Math.round(((Number(x ?? 0) - b) / Math.abs(b)) * 1000) / 10;
}

/** Id das colunas que a comparação acrescenta (não vêm do motor: não se ordenam no servidor). */
export const colunaComparada = (id: string) => id.includes("#");

/**
 * Junta o resumo actual com o do período comparado, grupo a grupo (pelos valores dos campos agrupados): a seguir a
 * cada valor vêm o valor do outro período e a variação. Os grupos que só existiam no outro período (um cliente que
 * este mês não comprou) entram no fim, com o actual vazio e a variação de −100 %. O total compara-se da mesma maneira.
 */
export function juntarComparacao(atual: ResultadoDados, anterior: ResultadoDados, nGrupos: number): ResultadoDados {
  const grupos = atual.colunas.slice(0, nGrupos);
  const valores = atual.colunas.slice(nGrupos);
  const colunas: CampoFonte[] = [...grupos];
  for (const c of valores) {
    colunas.push(c, { id: `${c.id}#anterior`, rotulo: `${c.rotulo} (anterior)`, tipo: c.tipo }, { id: `${c.id}#var`, rotulo: "Variação", tipo: "percentagem" });
  }
  const chave = (l: ValorDados[]) => JSON.stringify(l.slice(0, nGrupos));
  const antes = new Map(anterior.linhas.map((l) => [chave(l), l] as const));
  const linha = (g: ValorDados[], a?: ValorDados[], b?: ValorDados[]): ValorDados[] => [
    ...g,
    ...valores.flatMap((_, i) => {
      const x = a ? a[nGrupos + i] : null;
      const y = b ? b[nGrupos + i] : null;
      return [x, y, variacao(x, y)];
    }),
  ];
  const vistos = new Set<string>();
  const linhas = atual.linhas.map((l) => {
    const k = chave(l);
    vistos.add(k);
    return linha(l.slice(0, nGrupos), l, antes.get(k));
  });
  // com o actual cortado (mais de mil grupos) não se sabe se um grupo falta mesmo: só se juntam com a lista inteira
  let extra = 0;
  if (!atual.truncado) {
    for (const b of anterior.linhas) {
      if (!vistos.has(chave(b))) {
        linhas.push(linha(b.slice(0, nGrupos), undefined, b));
        extra++;
      }
    }
  }
  const totais: Record<string, ValorDados> = {};
  for (const c of valores) {
    if (!(c.id in atual.totais)) continue;
    totais[c.id] = atual.totais[c.id];
    totais[`${c.id}#anterior`] = anterior.totais[c.id] ?? null;
    totais[`${c.id}#var`] = variacao(atual.totais[c.id], anterior.totais[c.id]);
  }
  return { ...atual, colunas, linhas, totais, total_linhas: atual.total_linhas + extra };
}

/** Um relatório pronto: um ponto de partida com nome, que o painel abre ao clicar num indicador. */
export interface Pronto {
  id: string;
  nome: string;
  fonte: string;
  definicao: Definicao;
}

const def = (d: Partial<Definicao>): Definicao => ({
  modo: "resumo",
  periodo: "mes",
  de: "",
  ate: "",
  campos: [],
  agrupar: [],
  agregados: [],
  filtros: [],
  ordenar: [],
  vista: "tabela",
  ...d,
});
const soma = (campo: string): AgregadoDados => ({ funcao: "soma", campo });
const contar: AgregadoDados = { funcao: "contar" };
const desc = (campo: string): OrdemDados => ({ campo, desc: true });
// a margem sobre o valor sem IVA (o lucro também é sem IVA)
const margem: FormulaDados = { nome: "Margem %", expressao: "{Lucro} / {Valor sem IVA} * 100", tipo: "numero" };
const asc = (campo: string): OrdemDados => ({ campo, desc: false });

/** Só aparecem os das fontes que o utilizador pode consultar (dados.fontes já vem filtrado pela permissão). */
export const PRONTOS: Pronto[] = [
  {
    id: "vendas_hoje",
    nome: "Vendas de hoje, documento a documento",
    fonte: "vendas",
    definicao: def({ modo: "detalhe", periodo: "hoje", campos: ["documento", "tipo", "cliente", "operador", "posto", "total"], ordenar: [desc("documento")] }),
  },
  {
    id: "vendas_dia",
    nome: "Vendas por dia",
    fonte: "vendas",
    definicao: def({ agrupar: ["data"], agregados: [contar, soma("liquido"), soma("iva"), soma("total")], ordenar: [asc("data")], vista: "grafico" }),
  },
  {
    id: "vendas_mes",
    nome: "Vendas por mês",
    fonte: "vendas",
    definicao: def({ periodo: "ano", agrupar: ["mes"], agregados: [contar, soma("liquido"), soma("iva"), soma("total")], ordenar: [asc("mes")], vista: "grafico" }),
  },
  {
    id: "vendas_operador",
    nome: "Vendas por operador",
    fonte: "vendas",
    definicao: def({
      agrupar: ["operador"],
      agregados: [contar, soma("total")],
      formulas: [{ nome: "Valor médio", expressao: "{Total} / {N.º de registos}", tipo: "dinheiro" }],
      ordenar: [desc("soma:total")],
    }),
  },
  {
    id: "lucro_familia",
    nome: "Vendas e lucro por família",
    fonte: "linhas_venda",
    definicao: def({ agrupar: ["familia"], agregados: [soma("quantidade"), soma("liquido"), soma("total"), soma("lucro")], formulas: [margem], ordenar: [desc("soma:total")] }),
  },
  {
    id: "artigos_mais_vendidos",
    nome: "Artigos mais vendidos",
    fonte: "linhas_venda",
    definicao: def({ agrupar: ["artigo"], agregados: [soma("quantidade"), soma("liquido"), soma("total"), soma("lucro")], formulas: [margem], ordenar: [desc("soma:total")] }),
  },
  {
    id: "receitas_despesas",
    nome: "Receitas × despesas por mês",
    fonte: "tesouraria",
    definicao: def({ periodo: "ano", agrupar: ["mes"], agregados: [soma("entrada"), soma("saida"), soma("saldo")], ordenar: [asc("mes")] }),
  },
  {
    id: "antiguidade",
    nome: "Antiguidade das dívidas de clientes",
    fonte: "conta_corrente",
    definicao: def({ agrupar: ["escalao"], agregados: [contar, soma("pendente")], ordenar: [asc("escalao")] }),
  },
  {
    id: "dividas_cliente",
    nome: "Dívidas por cliente",
    fonte: "conta_corrente",
    definicao: def({ agrupar: ["cliente"], agregados: [contar, soma("pendente"), { funcao: "max", campo: "dias_atraso" }], ordenar: [desc("soma:pendente")] }),
  },
  {
    id: "stock_minimo",
    nome: "Artigos no mínimo ou abaixo",
    fonte: "stock",
    definicao: def({
      modo: "detalhe",
      campos: ["referencia", "artigo", "loja", "existencia", "minimo"],
      filtros: [{ campo: "abaixo_minimo", op: "=", texto: "1" }],
      ordenar: [asc("existencia")],
    }),
  },
  {
    id: "stock_familia",
    nome: "Valor do stock por família",
    fonte: "stock",
    definicao: def({ agrupar: ["familia"], agregados: [soma("existencia"), soma("valor")], ordenar: [desc("soma:valor")] }),
  },
  {
    id: "compras_fornecedor",
    nome: "Compras por fornecedor",
    fonte: "compras",
    definicao: def({ periodo: "ano", agrupar: ["fornecedor"], agregados: [contar, soma("total"), soma("pendente")], ordenar: [desc("soma:total")] }),
  },
];

/** A ligação para abrir um relatório pronto no construtor (com outro período, se se quiser). */
export const linkPronto = (id: string, periodo?: Periodo) => `/relatorios/construtor?pronto=${id}${periodo ? `&periodo=${periodo}` : ""}`;

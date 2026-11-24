// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Motor central de dados no ecrã: os tipos das fontes e das consultas (services/dados no servidor), a apresentação
// de cada valor pelo tipo do campo e a exportação para Excel e CSV com as ajudas que já existem (lib/csv.ts).
import { csv, csvKz, dataXL, exportarExcel, exportarFicheiro, kzXL, type CelulaXL } from "@/lib/csv";
import { data, kzNum, numero } from "@/lib/format";

/** «percentagem» não vem do motor: é a variação que o ecrã calcula ao comparar períodos */
export type TipoCampo = "texto" | "numero" | "dinheiro" | "data" | "logico" | "percentagem";

export interface CampoFonte {
  id: string;
  rotulo: string;
  tipo: TipoCampo;
}

export interface FonteDados {
  id: string;
  rotulo: string;
  descricao: string;
  /** o campo de data principal (o período); vazio numa fonte que é um retrato de agora (stock, contas a receber) */
  data: string;
  campos: CampoFonte[];
}

export type Operador = "=" | "<>" | ">" | ">=" | "<" | "<=" | "contem" | "comeca" | "entre" | "em" | "vazio" | "naovazio";
export interface FiltroDados {
  campo: string;
  op: Operador;
  valor?: unknown;
}
export type FuncaoAgregado = "soma" | "media" | "min" | "max" | "contar";
export interface AgregadoDados {
  funcao: FuncaoAgregado;
  campo?: string;
}
export interface OrdemDados {
  campo: string;
  desc: boolean;
}

export interface ConsultaDados {
  fonte: string;
  campos?: string[];
  filtros?: FiltroDados[];
  agrupar?: string[];
  agregados?: AgregadoDados[];
  ordenar?: OrdemDados[];
  limite?: number;
}

export type ValorDados = string | number | null;
export interface ResultadoDados {
  fonte: string;
  colunas: CampoFonte[];
  linhas: ValorDados[][];
  /** total geral (todas as linhas que passam nos filtros), por id da coluna */
  totais: Record<string, ValorDados>;
  total_linhas: number;
  truncado: boolean;
}

/** Um valor como se mostra no ecrã, pelo tipo do campo. */
export function mostrar(v: ValorDados | undefined, tipo: TipoCampo): string {
  if (v === null || v === undefined || v === "") return "—";
  switch (tipo) {
    case "dinheiro":
      return kzNum(Number(v));
    case "numero":
      return numero(Number(v), 3);
    case "data":
      return data(String(v));
    case "logico":
      return Number(v) ? "Sim" : "Não";
    case "percentagem":
      return `${Number(v) > 0 ? "+" : ""}${numero(Number(v), 1)}\u00a0%`;
    default:
      return String(v);
  }
}

export const alinhaDireita = (tipo: TipoCampo) => tipo === "dinheiro" || tipo === "numero" || tipo === "percentagem";

/** Excel com dinheiro e datas como valores (não texto) e a linha de totais no fim. */
export async function exportarResultadoExcel(r: ResultadoDados, titulo: string) {
  const celula = (v: ValorDados, tipo: TipoCampo): CelulaXL => {
    if (v === null || v === undefined) return null;
    if (tipo === "dinheiro") return kzXL(Number(v));
    if (tipo === "data") return dataXL(String(v));
    if (tipo === "numero" || tipo === "percentagem") return Number(v);
    if (tipo === "logico") return Number(v) ? "Sim" : "Não";
    return String(v);
  };
  const linhas: CelulaXL[][] = [r.colunas.map((c) => c.rotulo), ...r.linhas.map((l) => l.map((v, i) => celula(v, r.colunas[i].tipo)))];
  if (Object.keys(r.totais).length) {
    linhas.push(r.colunas.map((c, i) => (c.id in r.totais ? celula(r.totais[c.id], c.tipo) : i === 0 ? "Total" : null)));
  }
  await exportarExcel(`${nomeFicheiro(titulo)}.xlsx`, linhas, { titulo });
}

/** CSV (ponto e vírgula, para o Excel em português), com a linha de totais no fim. */
export async function exportarResultadoCsv(r: ResultadoDados, titulo: string) {
  const texto = (v: ValorDados, tipo: TipoCampo) =>
    v === null || v === undefined
      ? ""
      : tipo === "dinheiro"
        ? csvKz(Number(v))
        : tipo === "logico"
          ? Number(v)
            ? "Sim"
            : "Não"
          : tipo === "percentagem"
            ? numero(Number(v), 1)
            : String(v);
  const linhas = [r.colunas.map((c) => c.rotulo), ...r.linhas.map((l) => l.map((v, i) => texto(v, r.colunas[i].tipo)))];
  if (Object.keys(r.totais).length) {
    linhas.push(r.colunas.map((c, i) => (c.id in r.totais ? texto(r.totais[c.id], c.tipo) : i === 0 ? "Total" : "")));
  }
  await exportarFicheiro(`${nomeFicheiro(titulo)}.csv`, csv(linhas));
}

function nomeFicheiro(titulo: string) {
  const hoje = new Date().toISOString().slice(0, 10);
  return `${titulo.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase() || "relatorio"}-${hoje}`;
}

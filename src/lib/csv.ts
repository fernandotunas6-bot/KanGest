import { toast } from "sonner";
import { call } from "./api";
import { base64, livroXLSX, type CelulaXL, type FolhaXL } from "./xlsx";

export { dataXL, kzXL, type CelulaXL } from "./xlsx";

type Celula = string | number | null | undefined;

/** CSV com ";" (Excel em português) e valores entre aspas quando necessário. */
export function csv(linhas: Celula[][]): string {
  const cel = (v: Celula) => {
    const s = v === null || v === undefined ? "" : String(v);
    return /[;"\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return linhas.map((l) => l.map(cel).join(";")).join("\r\n") + "\r\n";
}

/** Cêntimos → "1234,56" (sem separador de milhares, para folhas de cálculo). */
export const csvKz = (c: number | null | undefined) => ((c ?? 0) / 100).toFixed(2).replace(".", ",");

/** Grava o ficheiro na pasta de exportação (Transferências) e avisa onde ficou. */
export async function exportarFicheiro(nome: string, conteudo: string) {
  try {
    const r = await call<{ ficheiro: string }>("sistema.exportar", { nome, conteudo });
    toast.success("Ficheiro exportado", { description: r.ficheiro, duration: 10000 });
  } catch (e) {
    toast.error((e as Error).message);
  }
}

/**
 * Livro Excel (.xlsx) na pasta de exportação: números como números, kwanzas com «#,##0.00» (kzXL) e datas como
 * datas (dataXL). `linhas` começa pelo cabeçalho da tabela; com `titulo`, o título e a data vão por cima.
 */
export async function exportarExcel(
  nome: string,
  linhas: CelulaXL[][],
  opcoes: { titulo?: string; folha?: string; extra?: FolhaXL[]; /** linha (1.ª = 1) do cabeçalho da tabela dentro de `linhas` */ linhaCabecalho?: number } = {},
) {
  const topo: CelulaXL[][] = opcoes.titulo ? [[opcoes.titulo], [`Exportado em ${new Date().toLocaleString("pt-PT")}`], []] : [];
  const cab = topo.length + (opcoes.linhaCabecalho ?? 1);
  const principal: FolhaXL = {
    nome: opcoes.folha ?? (opcoes.titulo ?? nome.replace(/\.xlsx$/i, "")).slice(0, 31),
    linhas: [...topo, ...linhas],
    cabecalho: cab,
    linhaTabela: cab,
  };
  try {
    const r = await call<{ ficheiro: string }>("sistema.exportarBinario", { nome, base64: base64(livroXLSX([principal, ...(opcoes.extra ?? [])])) });
    toast.success("Ficheiro Excel exportado", { description: r.ficheiro, duration: 10000 });
  } catch (e) {
    toast.error((e as Error).message);
  }
}

import { toast } from "sonner";
import { call } from "./api";

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

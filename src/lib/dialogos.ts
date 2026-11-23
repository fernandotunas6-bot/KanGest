// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Janelas nativas de guardar e abrir (tauri-plugin-dialog): escolher uma cópia de segurança, a pasta externa (pen,
// OneDrive) ou onde guardar a cópia exportada, em vez de escrever o caminho à mão. Só na aplicação instalada e no
// computador que guarda a base: num posto em rede a cópia e o restauro correm no servidor, onde o caminho do posto
// não existe.
import { useQuery } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { call, NA_APP } from "@/lib/api";

/** Há janelas nativas para escolher ficheiros que o próprio KanGest vai ler ou gravar (não é um posto em rede). */
export function useEscolherNoComputador(): boolean {
  const { data } = useQuery({
    queryKey: ["rede-modo"],
    queryFn: () => call<{ modo: string }>("rede.estado"),
    enabled: NA_APP,
    staleTime: 60_000,
  });
  return NA_APP && !!data && data.modo !== "cliente";
}

/** Uma cópia de segurança do KanGest (`.db` ou `.kgc`), ou null se se cancelou. */
export async function escolherCopia(): Promise<string | null> {
  const r = await open({ multiple: false, directory: false, title: "Escolher a cópia de segurança", filters: [{ name: "Cópias do KanGest", extensions: ["db", "kgc"] }] });
  return typeof r === "string" ? r : null;
}

/** Um ficheiro para o KanGest ler (SAF-T, folha de cálculo), ou null se se cancelou. */
export async function escolherFicheiro(titulo: string, filtros: { name: string; extensions: string[] }[]): Promise<string | null> {
  const r = await open({ multiple: false, directory: false, title: titulo, filters: filtros });
  return typeof r === "string" ? r : null;
}

/** Uma pasta (pen, disco de rede, OneDrive), ou null se se cancelou. */
export async function escolherPasta(titulo: string): Promise<string | null> {
  const r = await open({ multiple: false, directory: true, title: titulo });
  return typeof r === "string" ? r : null;
}

/** Onde guardar a cópia exportada, ou null se se cancelou. */
export async function guardarCopiaComo(): Promise<string | null> {
  const hoje = new Date().toISOString().slice(0, 10);
  return await save({ title: "Guardar a cópia de segurança", defaultPath: `kangest-copia-${hoje}.db`, filters: [{ name: "Cópia do KanGest", extensions: ["db"] }] });
}

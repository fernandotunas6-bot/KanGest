// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useSyncExternalStore } from "react";
import { kz as kzBase } from "@/lib/format";

/**
 * «Ocultar valores» (no ecrã de entrada e no painel): os valores em kwanzas do painel aparecem como ••••••, para quem
 * está do outro lado do balcão não os ver. Fica guardado neste aparelho.
 */
const CHAVE = "kangest-ocultar-valores";
const ouvintes = new Set<() => void>();

function ler(): boolean {
  try {
    return localStorage.getItem(CHAVE) === "1";
  } catch {
    return false;
  }
}

export function definirOcultarValores(v: boolean) {
  try {
    if (v) localStorage.setItem(CHAVE, "1");
    else localStorage.removeItem(CHAVE);
  } catch {
    /* sem armazenamento: fica como estava */
  }
  ouvintes.forEach((f) => f());
}

function subscrever(f: () => void) {
  ouvintes.add(f);
  const outraJanela = (e: StorageEvent) => {
    if (e.key === CHAVE) f();
  };
  window.addEventListener("storage", outraJanela);
  return () => {
    ouvintes.delete(f);
    window.removeEventListener("storage", outraJanela);
  };
}

export function useOcultarValores(): [boolean, (v: boolean) => void] {
  return [useSyncExternalStore(subscrever, ler, () => false), definirOcultarValores];
}

export const VALOR_OCULTO = "•••••• Kz";

/** Kwanzas como `kz` (format.ts), ou ocultos com «Ocultar valores». */
export function useKz() {
  const [ocultar] = useOcultarValores();
  return (c: number | null | undefined) => (ocultar ? VALOR_OCULTO : kzBase(c));
}

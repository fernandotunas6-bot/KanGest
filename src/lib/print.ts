import { invoke } from "@tauri-apps/api/core";

const mac = /Mac/i.test(navigator.userAgent);

/** Abre o diálogo de impressão do sistema para o conteúdo de #print-area. */
export function imprimir() {
  // pequeno atraso para o portal de impressão renderizar
  setTimeout(() => {
    if (mac && "__TAURI_INTERNALS__" in window) {
      invoke("imprimir").catch(() => window.print());
    } else {
      window.print();
    }
  }, 80);
}

export const NOMES_VIAS = ["Original", "Duplicado", "Triplicado", "Quadruplicado"];

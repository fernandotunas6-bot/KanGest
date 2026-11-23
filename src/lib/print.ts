// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { invoke } from "@tauri-apps/api/core";

const mac = /Mac/i.test(navigator.userAgent);

/** As caixas de margem da página vazias: o browser deixa de escrever nelas o endereço do programa, a data, o título e
 * «1/2» (o «Cabeçalhos e rodapés» da janela de impressão), mesmo com margens. */
const SEM_CABECALHOS =
  "@page { @top-left { content: ''; } @top-center { content: ''; } @top-right { content: ''; } @bottom-left { content: ''; } @bottom-center { content: ''; } @bottom-right { content: ''; } }";

/**
 * A página certa para o que se imprime:
 * - talão (80 ou 58 mm): a largura do papel e a altura do talão, sem margens. Sem isto o sistema imprimia numa folha
 *   A4 (o rolo avançava o resto em branco). Cada via fica na sua página e a impressora corta entre elas;
 * - o resto: A4 com margens de 10 mm (num computador com papel Carta por omissão a factura passava para 2 páginas),
 *   a não ser que o próprio ecrã diga outra página (A5, folhas de etiquetas).
 * Em todos, sem os cabeçalhos e rodapés do browser.
 */
function ajustarPagina() {
  document.getElementById("kg-pagina")?.remove();
  const area = document.getElementById("print-area");
  if (!area) return;
  const s = document.createElement("style");
  s.id = "kg-pagina";
  const taloes = [...area.querySelectorAll<HTMLElement>(".print-ticket")];
  if (taloes.length) {
    // mede fora do ecrã, com a largura do papel (no ecrã o #print-area está escondido)
    const antes = area.getAttribute("style");
    area.setAttribute("style", "display:block;position:fixed;left:-10000px;top:0;visibility:hidden");
    area.classList.add("a-medir");
    const alturaPx = Math.max(...taloes.map((t) => t.getBoundingClientRect().height));
    area.classList.remove("a-medir");
    if (antes === null) area.removeAttribute("style");
    else area.setAttribute("style", antes);
    const papel = taloes[0].classList.contains("talao-58") ? 58 : 80;
    // + 3 mm de folga para a impressora não cortar a última linha
    const altura = Math.ceil((alturaPx * 25.4) / 96) + 3;
    s.textContent = `@media print { @page { size: ${papel}mm ${altura}mm; margin: 0; } ${SEM_CABECALHOS} }`;
  } else {
    const propria = [...area.querySelectorAll("style")].some((x) => x.textContent?.includes("@page"));
    s.textContent = `@media print { ${propria ? "" : "@page { size: A4; margin: 10mm; }"} ${SEM_CABECALHOS} }`;
  }
  document.head.appendChild(s);
}

/** Abre o diálogo de impressão do sistema para o conteúdo de #print-area. */
export function imprimir() {
  // pequeno atraso para o portal de impressão renderizar
  setTimeout(() => {
    ajustarPagina();
    if (mac && "__TAURI_INTERNALS__" in window) {
      invoke("imprimir").catch(() => window.print());
    } else {
      window.print();
    }
  }, 80);
}

export const NOMES_VIAS = ["Original", "Duplicado", "Triplicado", "Quadruplicado"];

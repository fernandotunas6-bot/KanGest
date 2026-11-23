// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

/**
 * PostCSS para o WebKit do macOS 12 e anteriores (Safari 15, que é o motor do KanGest nesses Macs: o programa usa o
 * WebKit do sistema). O Tailwind v4 gera CSS pensado para o Safari 16.4 ou mais recente, e o Safari 15 deita fora o que
 * não percebe. Ficava assim:
 *
 *   1. Degradés sem nada: o Tailwind escreve `to bottom in oklab` e o Safari 15 não conhece `in oklab`, por isso a
 *      declaração inteira cai e o degradé desaparece (a camada escura dos cartões do ecrã de entrada, as sombras
 *      suaves em baixo…). Aqui o degradé volta sem `in oklab`, e o `in oklab` regressa só onde o motor o percebe.
 *   2. Cores com transparência (`bg-nav/95`, `ring-brand/15`, `bg-sunken/60`…): o Tailwind escreve `color-mix()`,
 *      que o Safari 15 não tem, e deixa antes uma cor fixa do jogo por omissão. Funcionava no verde-petróleo claro,
 *      mas com outro jogo de cores ou com o tema escuro a transparência saía na cor errada. Aqui a cor de recurso
 *      passa a seguir o tema, com `rgb(var(--rgb-…) / 95%)`.
 *
 * Para o ponto 2, cada cor `--color-x: #rrggbb` ganha ao lado um canal `--rgb-x: r g b`. As cores escolhidas em
 * Configurações → Aparência fazem o mesmo (src/lib/cores.ts, `aplicarCores`). Nos motores modernos nada muda: o
 * `color-mix()` e o `in oklab` do Tailwind ficam depois e prevalecem.
 */
import postcss from "postcss";

const SUPORTA_OKLAB = "(background-image: linear-gradient(in oklab, red, blue))";
const EM_OKLAB = /\s+in\s+oklab\b/;
// color-mix(in oklab, var(--color-nav) 95%, transparent) — só as misturas com uma cor do tema e transparência
const COR_MISTURADA = /color-mix\(\s*in\s+(?:oklab|srgb)\s*,\s*var\(--color-([a-z0-9-]+)\)\s+([\d.]+)%\s*,\s*transparent\s*\)/g;

/** `#rgb` ou `#rrggbb` → «r g b», ou null. */
function canal(valor) {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(valor.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  const n = parseInt(h, 16);
  return `${(n >> 16) & 255} ${(n >> 8) & 255} ${n & 255}`;
}

const plugin = () => ({
  postcssPlugin: "kangest-webkit15",
  Once(raiz) {
    // 1. um canal RGB por cada cor hexadecimal do tema (claro, escuro e as que o Tailwind põe por omissão)
    const conhecidas = new Set(["white", "black"]);
    const cores = [];
    raiz.walkDecls(/^--color-/, (d) => {
      if (canal(d.value)) cores.push(d);
    });
    for (const d of cores) {
      const nome = d.prop.slice("--color-".length);
      conhecidas.add(nome);
      d.after(postcss.decl({ prop: `--rgb-${nome}`, value: canal(d.value) }));
    }
    const rgb = (nome, pct) =>
      nome === "white" ? `rgb(255 255 255 / ${pct}%)` : nome === "black" ? `rgb(0 0 0 / ${pct}%)` : conhecidas.has(nome) ? `rgb(var(--rgb-${nome}) / ${pct}%)` : null;

    // 2. antes de cada bloco `@supports (color: color-mix(…))` do Tailwind, uma cópia com rgb(var(--rgb-…)): o motor
    //    antigo salta o bloco e fica com a cópia; o moderno aplica o bloco, que vem depois
    const blocos = [];
    raiz.walkAtRules("supports", (at) => {
      if (/color-mix/.test(at.params)) blocos.push(at);
    });
    for (const at of blocos) {
      const copias = [];
      at.each((no) => {
        if (no.type !== "rule" && no.type !== "atrule") return;
        const copia = no.clone();
        copia.walkDecls((d) => {
          let trocou = false;
          const novo = d.value.replace(COR_MISTURADA, (m, nome, pct) => {
            const r = rgb(nome, pct);
            if (r) trocou = true;
            return r ?? m;
          });
          if (trocou && !/color-mix/.test(novo)) d.value = novo;
          else d.remove();
        });
        // sem o que ficou por trocar, apaga-se o que ficou vazio
        copia.walk((n) => {
          if ((n.type === "rule" || n.type === "atrule") && n.nodes && n.nodes.length === 0) n.remove();
        });
        if (copia.nodes && copia.nodes.length) copias.push(copia);
      });
      for (const c of copias) at.before(c);
    }

    // 3. degradés: sem `in oklab` por omissão, e com ele onde o motor o percebe
    const degrades = [];
    raiz.walkDecls("--tw-gradient-position", (d) => {
      if (d.parent?.type === "rule" && EM_OKLAB.test(d.value) && d.value.replace(EM_OKLAB, "").trim()) degrades.push(d);
    });
    for (const d of degrades) {
      const original = d.value;
      const regra = d.parent;
      d.value = original.replace(EM_OKLAB, "").trim();
      const so = regra.clone();
      so.removeAll();
      so.append(postcss.decl({ prop: d.prop, value: original }));
      regra.after(postcss.atRule({ name: "supports", params: SUPORTA_OKLAB }).append(so));
    }
  },
});
plugin.postcss = true;

export default plugin;

// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Confirma que o CSS compilado (dist/assets/*.css) funciona no WebKit do macOS 12 e anteriores, que é o motor do
// KanGest nesses Macs (Safari 15): node scripts/verificar-css.mjs [pasta]. Corre na CI depois de `vite build`.
//
// O Tailwind v4 gera `color-mix()` e degradés com `in oklab`, que o Safari 15 não percebe e deita fora (o degradé
// desaparecia e as cores com transparência saíam na cor errada). scripts/css-webkit15.mjs trata disto; se uma
// actualização do Tailwind mudar o que gera e o plugin deixar de o apanhar, isto falha em vez de o utilizador
// ver o ecrã de entrada desajustado. Só pode haver `color-mix` e `in oklab` dentro de um `@supports` que os teste.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import postcss from "postcss";

const pasta = process.argv[2] ?? "dist/assets";
const ficheiros = readdirSync(pasta).filter((f) => f.endsWith(".css"));
if (!ficheiros.length) {
  console.error(`Sem CSS em ${pasta}: corra primeiro «npx vite build».`);
  process.exit(2);
}

const dentroDeSupports = (no, re) => {
  for (let p = no.parent; p; p = p.parent) if (p.type === "atrule" && p.name === "supports" && re.test(p.params)) return true;
  return false;
};

let erros = 0;
for (const f of ficheiros) {
  const raiz = postcss.parse(readFileSync(join(pasta, f), "utf8"));
  let canais = 0;
  raiz.walkDecls((d) => {
    const onde = `${f}: ${d.parent?.selector ?? d.parent?.type} { ${d.prop}: ${d.value.slice(0, 80)} }`;
    if (d.prop.startsWith("--rgb-")) canais++;
    if (/color-mix\(/.test(d.value) && !dentroDeSupports(d, /color-mix/)) {
      console.error(`color-mix fora de um @supports: ${onde}`);
      erros++;
    }
    if (/\bin\s+oklab\b/.test(d.value) && !/color-mix\(/.test(d.value) && !dentroDeSupports(d, /in oklab/)) {
      console.error(`degradé com «in oklab» fora de um @supports: ${onde}`);
      erros++;
    }
  });
  if (!canais) {
    console.error(`${f}: não há canais --rgb-… (scripts/css-webkit15.mjs não correu?)`);
    erros++;
  }
}
if (erros) {
  console.error(`\n${erros} problema(s): o ecrã sairia desajustado no WebKit do macOS 12 e anteriores (docs/BUILD.md).`);
  process.exit(1);
}
console.log(`CSS para o WebKit do macOS 12: certo (${ficheiros.join(", ")}).`);

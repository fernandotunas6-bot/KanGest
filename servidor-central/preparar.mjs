// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Copia o núcleo do servidor (servidor-licencas/src) para a função kangest, antes de a testar ou publicar:
//   node servidor-central/preparar.mjs
// O núcleo só se edita em servidor-licencas; a cópia (supabase/functions/kangest/nucleo) não vai para o repositório.
// O painel antigo (painel.js) gera-se de painel.html, como o construir.mjs faz para o Worker.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const origem = new URL("../servidor-licencas/src/", import.meta.url);
const destino = new URL("supabase/functions/kangest/nucleo/", import.meta.url);
mkdirSync(destino, { recursive: true });

const worker = readFileSync(new URL("worker.js", origem), "utf8");
copyFileSync(new URL("worker.js", origem), new URL("worker.js", destino));
const copiados = ["worker.js"];
// tudo o que o worker.js importa de ao pé dele (hoje só ./painel.js), para a função não arrancar sem um módulo
for (const [, nome] of worker.matchAll(/^import\s[^;]*?from\s+"\.\/([^"]+)";/gm)) {
  if (nome === "painel.js") {
    const html = readFileSync(new URL("painel.html", origem), "utf8");
    writeFileSync(new URL("painel.js", destino), `// Gerado por servidor-central/preparar.mjs a partir de painel.html: não editar.\nexport const PAINEL = ${JSON.stringify(html)};\n`);
  } else {
    copyFileSync(new URL(nome, origem), new URL(nome, destino));
  }
  copiados.push(nome);
}
console.log(`Núcleo copiado para supabase/functions/kangest/nucleo: ${copiados.join(", ")}.`);

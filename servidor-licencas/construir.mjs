// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Gera src/painel.js a partir de src/painel.html (o Worker importa o painel como texto).
import { readFileSync, writeFileSync } from "node:fs";
const aqui = new URL(".", import.meta.url);
const html = readFileSync(new URL("src/painel.html", aqui), "utf8");
writeFileSync(new URL("src/painel.js", aqui), `// Gerado por construir.mjs a partir de painel.html: não editar.\nexport const PAINEL = ${JSON.stringify(html)};\n`);
console.log(`src/painel.js: ${html.length} caracteres`);

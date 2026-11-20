// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

/** Aviso de direitos de autor no topo de cada ficheiro JavaScript da compilação. Entra depois da minificação
 * (`generateBundle`), que tira os comentários do código. */
const AVISO =
  "/*! KanGest — © 2026 Valentino Canguele (926 445 277 · valentinocanguele@gmail.com). Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor. */";
function avisoDireitos(): Plugin {
  return {
    name: "kangest-aviso-direitos",
    apply: "build",
    enforce: "post",
    generateBundle(_, bundle) {
      for (const f of Object.values(bundle)) if (f.type === "chunk") f.code = `${AVISO}\n${f.code}`;
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), avisoDireitos()],
  resolve: { alias: { "@": path.resolve(__dirname, "src") } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: ["es2021", "chrome105", "safari15"],
    sourcemap: false,
  },
});

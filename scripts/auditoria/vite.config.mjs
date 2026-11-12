import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import { fileURLToPath } from "node:url";
const raiz = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
export default defineConfig({
  root: raiz,
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(raiz, "src") } },
  clearScreen: false,
  server: {
    port: 1422,
    strictPort: true,
    proxy: { "/rpc": { target: "http://127.0.0.1:7799", rewrite: () => "/" } },
    watch: { ignored: ["**/src-tauri/**"] },
  },
});

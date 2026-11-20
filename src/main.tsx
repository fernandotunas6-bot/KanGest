// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter } from "react-router-dom";
import { Toaster } from "sonner";
import App from "./App";
import VisorCliente from "./pages/VisorCliente";
import { useAparencia } from "./lib/aparencia";
import { registarServiceWorker } from "./lib/pwa";
import "./index.css";

// crypto.randomUUID só existe em páginas seguras (https/localhost); no browser por http da rede local
// (telemóvel ligado ao servidor da loja) faz-se com o gerador aleatório, que existe sempre.
if (typeof crypto !== "undefined" && !crypto.randomUUID) {
  Object.defineProperty(crypto, "randomUUID", {
    value: () => {
      const b = crypto.getRandomValues(new Uint8Array(16));
      b[6] = (b[6] & 0x0f) | 0x40;
      b[8] = (b[8] & 0x3f) | 0x80;
      const h = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
      return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}` as `${string}-${string}-${string}-${string}-${string}`;
    },
  });
}

registarServiceWorker();

const qc = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
});

/** As notificações seguem o tema escolhido (claro/escuro). */
function Notificacoes() {
  const tema = useAparencia((a) => a.temaEfectivo);
  return <Toaster position="top-right" richColors closeButton theme={tema === "escuro" ? "dark" : "light"} />;
}

// Ecrã do cliente (segundo monitor): janela própria, sem sessão nem menus
const visor = location.hash.startsWith("#/visor-cliente");

ReactDOM.createRoot(document.getElementById("root")!).render(
  visor ? (
    <React.StrictMode>
      <VisorCliente />
    </React.StrictMode>
  ) : (
  <React.StrictMode>
    <QueryClientProvider client={qc}>
      <HashRouter>
        <App />
      </HashRouter>
      <Notificacoes />
    </QueryClientProvider>
  </React.StrictMode>
  ),
);

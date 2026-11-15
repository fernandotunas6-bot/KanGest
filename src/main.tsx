import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter } from "react-router-dom";
import { Toaster } from "sonner";
import App from "./App";
import VisorCliente from "./pages/VisorCliente";
import { useAparencia } from "./lib/aparencia";
import "./index.css";

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

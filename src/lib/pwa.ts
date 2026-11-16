import { useSyncExternalStore } from "react";
import { MODO_WEB } from "./api";

/**
 * Aplicação instalável (PWA) quando o KanGest é aberto no browser: Android/Chrome e Edge mostram
 * «Instalar», o iPhone/iPad usa «Partilhar → Adicionar ao ecrã principal». A instalação completa
 * (ícone próprio, ecrã inteiro, arranque sem rede da casca da aplicação) exige HTTPS — por exemplo
 * pelo túnel da Cloudflare (docs/WEB-PWA.md). Na rede local por http funciona no browser e como atalho.
 */

interface PedidoInstalacao extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

let pedido: PedidoInstalacao | null = null;
const ouvintes = new Set<() => void>();
const avisar = () => ouvintes.forEach((f) => f());

if (typeof window !== "undefined" && MODO_WEB) {
  window.addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    pedido = e as PedidoInstalacao;
    avisar();
  });
  window.addEventListener("appinstalled", () => {
    pedido = null;
    avisar();
  });
}

/** A página já corre como aplicação instalada (sem barra do browser). */
export function instalada(): boolean {
  return (
    window.matchMedia?.("(display-mode: standalone)").matches ||
    window.matchMedia?.("(display-mode: fullscreen)").matches ||
    (navigator as { standalone?: boolean }).standalone === true
  );
}

export const IOS = typeof navigator !== "undefined" && (/iPad|iPhone|iPod/.test(navigator.userAgent) || (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1));

function estado() {
  return pedido ? "pode" : instalada() ? "instalada" : IOS ? "ios" : "nao";
}

/** "pode" (há botão de instalar), "instalada", "ios" (instruções manuais) ou "nao". */
export function usePwa(): "pode" | "instalada" | "ios" | "nao" {
  return useSyncExternalStore(
    (f) => {
      ouvintes.add(f);
      return () => ouvintes.delete(f);
    },
    estado,
    () => "nao",
  );
}

export async function instalarPwa(): Promise<boolean> {
  if (!pedido) return false;
  await pedido.prompt();
  const r = await pedido.userChoice;
  pedido = null;
  avisar();
  return r.outcome === "accepted";
}

/** O service worker guarda a casca da aplicação (só em HTTPS ou localhost, e nunca os dados). */
export function registarServiceWorker() {
  if (!MODO_WEB || import.meta.env.DEV || !("serviceWorker" in navigator) || !window.isSecureContext) return;
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {});
  });
}

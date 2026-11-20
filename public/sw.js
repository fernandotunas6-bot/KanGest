// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

/*
 * Service worker do KanGest no browser (PWA). Guarda só a «casca» da aplicação — a página, o código,
 * os estilos e os ícones — para abrir depressa e mostrar uma mensagem clara quando o servidor não
 * responde. Nunca guarda dados: os pedidos cifrados (/rpc) e a informação do servidor vão sempre à rede.
 */
const CASCA = "kangest-casca-v1";
const ACTIVOS = "kangest-activos-v1";
const MAX_ACTIVOS = 80;
const ESSENCIAIS = ["/", "/manifest.webmanifest", "/favicon.svg", "/icons/icon-192.png", "/icons/icon-512.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(
    caches
      .open(CASCA)
      .then((c) => c.addAll(ESSENCIAIS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((ks) => Promise.all(ks.filter((k) => k !== CASCA && k !== ACTIVOS).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

/** Os ficheiros com impressão digital nunca mudam; guardam-se os mais recentes e apagam-se os antigos. */
async function guardarActivo(pedido, resposta) {
  const c = await caches.open(ACTIVOS);
  await c.put(pedido, resposta);
  const ks = await c.keys();
  for (const k of ks.slice(0, Math.max(0, ks.length - MAX_ACTIVOS))) await c.delete(k);
}

self.addEventListener("fetch", (e) => {
  const pedido = e.request;
  if (pedido.method !== "GET") return;
  const url = new URL(pedido.url);
  if (url.origin !== self.location.origin) return;
  // dados e informação do servidor: sempre à rede
  if (url.pathname === "/rpc" || url.pathname.startsWith("/rede/")) return;

  // navegação: a página mais recente do servidor; sem rede, a guardada (a aplicação explica o que se passa)
  if (pedido.mode === "navigate") {
    e.respondWith(
      fetch(pedido)
        .then((r) => {
          if (r.ok) {
            const copia = r.clone();
            caches.open(CASCA).then((c) => c.put("/", copia));
          }
          return r;
        })
        .catch(() => caches.match("/").then((r) => r || Response.error())),
    );
    return;
  }

  if (url.pathname.startsWith("/assets/")) {
    e.respondWith(
      caches.match(pedido).then(
        (guardada) =>
          guardada ||
          fetch(pedido).then((r) => {
            if (r.ok) guardarActivo(pedido, r.clone());
            return r;
          }),
      ),
    );
    return;
  }

  // ícones e manifesto: o guardado já, e actualiza-se por trás
  e.respondWith(
    caches.match(pedido).then((guardada) => {
      const rede = fetch(pedido)
        .then((r) => {
          if (r.ok) caches.open(CASCA).then((c) => c.put(pedido, r.clone()));
          return r;
        })
        .catch(() => guardada || Response.error());
      return guardada || rede;
    }),
  );
});

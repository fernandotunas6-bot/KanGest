// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Encaminhamento temporário no Supabase (docs/ARQUITETURA.md): os KanGest 1.4.0 falam com esta função, porque o servidor
// de licenças esteve no Supabase quando a 1.4.0 saiu. Cada pedido segue, tal como veio, para o Worker da Cloudflare, que é
// o servidor de licenças. Sem nada do Deno (testa-se no Node, em servidor-licencas/teste/encaminhamento.mjs).
export const WORKER = "https://kangest-licencas.valentinocanguele.workers.dev";

// a função responde em /kangest/… (e /functions/v1/kangest/…, conforme o caminho que o Supabase passa)
const PREFIXO = /^(?:\/functions\/v1)?\/kangest(?=\/|$)/;
// cabeçalhos da viagem até aqui: a Cloudflare põe os seus no pedido ao Worker (e um cdn-loop repetido parecia um ciclo)
const DA_VIAGEM = ["host", "content-length", "accept-encoding", "forwarded", "x-forwarded-for", "x-forwarded-host", "x-forwarded-proto",
  "x-forwarded-port", "x-real-ip", "true-client-ip", "cdn-loop", "cf-connecting-ip", "cf-ipcountry", "cf-ray", "cf-visitor", "cf-worker"];

/** Um pedido que chegou à função, levado ao Worker; `buscar` é o fetch (troca-se nos testes). */
export async function encaminhar(req, buscar = fetch) {
  const url = new URL(req.url);
  const destino = WORKER + (url.pathname.replace(PREFIXO, "") || "/") + url.search;
  const cabecalhos = new Headers(req.headers);
  for (const c of DA_VIAGEM) cabecalhos.delete(c);
  try {
    const r = await buscar(destino, {
      method: req.method,
      headers: cabecalhos,
      body: req.method === "GET" || req.method === "HEAD" ? undefined : await req.arrayBuffer(),
      redirect: "manual",
    });
    // o corpo já vem descomprimido: sem content-encoding nem content-length, que já não correspondem
    const saida = new Headers(r.headers);
    saida.delete("content-encoding");
    saida.delete("content-length");
    return new Response(r.body, { status: r.status, statusText: r.statusText, headers: saida });
  } catch {
    return new Response(JSON.stringify({ ok: false, erro: "O servidor de licenças não respondeu. Tente mais tarde." }), {
      status: 502,
      headers: { "content-type": "application/json; charset=utf-8" },
    });
  }
}

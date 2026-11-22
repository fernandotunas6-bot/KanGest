// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// O Worker antigo depois da passagem para o Supabase (fase 4, docs/ARQUITETURA.md): encaminha cada pedido, tal como veio,
// para a função kangest. Os KanGest instalados e o site continuam a funcionar por este endereço até terem o novo.
//
// Variáveis: NOVO_SERVIDOR (ex.: https://<projecto>.supabase.co/functions/v1/kangest) e o segredo ENCAMINHADOR_SEGREDO,
// igual ao da função. Com ele, a função confia no IP e no país que este Worker lhe diz (para o limite por IP e as
// sessões); sem ele, todos os pedidos pareciam vir do IP da Cloudflare.
//   npx wrangler deploy src/encaminhar.js --var NOVO_SERVIDOR:https://<projecto>.supabase.co/functions/v1/kangest
//   npx wrangler secret put ENCAMINHADOR_SEGREDO

const SAIDA = { "content-type": "application/json; charset=utf-8" };

export default {
  async fetch(req, env) {
    const novo = String(env.NOVO_SERVIDOR ?? "").replace(/\/+$/, "");
    if (!/^https:\/\//.test(novo)) {
      return new Response(JSON.stringify({ ok: false, erro: "O servidor de licenças está em mudança. Tente daqui a pouco." }), { status: 503, headers: SAIDA });
    }
    const url = new URL(req.url);
    const cabecalhos = new Headers(req.headers);
    for (const c of ["host", "x-kangest-encaminhador", "x-kangest-ip", "x-kangest-pais"]) cabecalhos.delete(c);
    if (env.ENCAMINHADOR_SEGREDO) {
      cabecalhos.set("x-kangest-encaminhador", env.ENCAMINHADOR_SEGREDO);
      const ip = req.headers.get("cf-connecting-ip");
      if (ip) cabecalhos.set("x-kangest-ip", ip);
      if (req.cf?.country) cabecalhos.set("x-kangest-pais", String(req.cf.country));
    }
    try {
      const r = await fetch(novo + url.pathname + url.search, {
        method: req.method,
        headers: cabecalhos,
        body: req.method === "GET" || req.method === "HEAD" ? undefined : await req.arrayBuffer(),
        redirect: "manual",
      });
      return new Response(r.body, { status: r.status, statusText: r.statusText, headers: r.headers });
    } catch {
      return new Response(JSON.stringify({ ok: false, erro: "O servidor de licenças não respondeu. Tente mais tarde." }), { status: 502, headers: SAIDA });
    }
  },
};

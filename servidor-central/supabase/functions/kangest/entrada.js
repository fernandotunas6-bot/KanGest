// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// A entrada da função kangest, sem nada do Deno (testa-se no Node, em servidor-central/teste/funcao.mjs): o pedido chega
// ao worker.js como o Worker da Cloudflare o recebia, com as variáveis que ele lê. O index.ts liga isto ao Deno.
import worker from "./nucleo/worker.js";

/**
 * As variáveis que a função lê no Supabase (docs/ARQUITETURA.md, «Segredos da função»): as do worker.js e o
 * ENCAMINHADOR_SEGREDO (pedidos que o Worker antigo encaminha). ACCESS_EQUIPA e ACCESS_AUD ficam de fora: o Cloudflare
 * Access só existe à frente do Worker, e com eles a área do produtor fechava-se.
 */
export const VARIAVEIS = [
  "CHAVE_PRIVADA", "ADMIN_TOKEN", "ADMIN_EMAIL", "ADMIN_SENHA", "ADMIN_ORIGENS", "RESEND_API_KEY", "EMAIL_REMETENTE",
  "TURNSTILE_SITE_KEY", "TURNSTILE_SECRET_KEY", "WEB_DOMINIO", "CF_API_TOKEN", "CF_CONTA", "CF_ZONA", "ENCAMINHADOR_SEGREDO",
];

/** O env do worker.js: a base (API do D1) e as variáveis definidas; ler(nome) devolve o valor ou nada. */
export function criarEnv(DB, ler) {
  const env = { DB };
  for (const nome of VARIAVEIS) {
    const v = ler(nome);
    if (v) env[nome] = v;
  }
  return env;
}

// a função responde em /kangest/… (e /functions/v1/kangest/… conforme o caminho que o Supabase passa)
const PREFIXO = /^(?:\/functions\/v1)?\/kangest(?=\/|$)/;
const MAX_CORPO = 1024 * 1024;

/**
 * O IP de quem pede, para o limite de pedidos por hora: o que a Cloudflare (à frente do Supabase) põe em
 * CF-Connecting-IP ou, sem ele, o primeiro do X-Forwarded-For.
 */
export function ipDe(cabecalhos) {
  const ip = cabecalhos.get("cf-connecting-ip") || cabecalhos.get("x-forwarded-for")?.split(",")[0] || cabecalhos.get("x-real-ip");
  return ip?.trim() || null;
}

/** Comparação em tempo constante. */
function iguais(a, b) {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let dif = 0;
  for (let i = 0; i < a.length; i++) dif |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return dif === 0;
}

// cabeçalhos do Worker antigo quando encaminha (servidor-licencas/src/encaminhar.js); nunca chegam ao worker.js
const DO_ENCAMINHADOR = ["x-kangest-encaminhador", "x-kangest-ip", "x-kangest-pais"];

/**
 * O pedido como o Worker o recebia: sem o prefixo /kangest, com o IP em CF-Connecting-IP e o país em `cf`. Se veio do
 * Worker antigo (com o ENCAMINHADOR_SEGREDO certo), o IP e o país são os que ele indica: os de quem lhe pediu.
 */
export async function adaptarPedido(req, env = {}) {
  const url = new URL(req.url);
  url.pathname = url.pathname.replace(PREFIXO, "") || "/";
  const cabecalhos = new Headers(req.headers);
  for (const c of DO_ENCAMINHADOR) cabecalhos.delete(c);
  const encaminhado = Boolean(env.ENCAMINHADOR_SEGREDO) && iguais(req.headers.get("x-kangest-encaminhador") ?? "", env.ENCAMINHADOR_SEGREDO);
  const ip = encaminhado ? req.headers.get("x-kangest-ip")?.trim() || null : ipDe(req.headers);
  if (ip) cabecalhos.set("cf-connecting-ip", ip);
  else cabecalhos.delete("cf-connecting-ip");
  const corpo = req.method === "GET" || req.method === "HEAD" ? null : await req.arrayBuffer();
  const pedido = new Request(url, { method: req.method, headers: cabecalhos, body: corpo });
  const pais = ((encaminhado ? req.headers.get("x-kangest-pais") : req.headers.get("cf-ipcountry")) ?? "").toUpperCase();
  Object.defineProperty(pedido, "cf", { value: /^[A-Z]{2}$/.test(pais) && pais !== "XX" ? { country: pais } : {} });
  return pedido;
}

/** A página antiga do painel (/admin) está no site: as funções do Supabase não servem páginas HTML. */
function areaDoProdutor(env) {
  const site = String(env.ADMIN_ORIGENS || "https://kangest.pages.dev").split(",")[0].trim().replace(/\/$/, "");
  return `${site}/admin.html`;
}

/** Um pedido que chegou à função kangest. */
export async function tratar(req, env) {
  if (Number(req.headers.get("content-length") ?? 0) > MAX_CORPO) {
    return new Response(JSON.stringify({ ok: false, erro: "Pedido demasiado grande." }), { status: 413, headers: { "content-type": "application/json; charset=utf-8" } });
  }
  const pedido = await adaptarPedido(req, env);
  if (pedido.method === "GET" && new URL(pedido.url).pathname.replace(/\/+$/, "") === "/admin") {
    return Response.redirect(areaDoProdutor(env), 302);
  }
  return worker.fetch(pedido, env);
}

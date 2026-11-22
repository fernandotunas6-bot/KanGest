// O encaminhamento no Supabase para os KanGest 1.4.0, sem rede: node servidor-licencas/teste/encaminhamento.mjs
// Pedidos como o Supabase os entrega → encaminhar.js → o Worker (o mesmo worker.js sobre uma D1 imitada).
// Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker from "../src/worker.js";
import { WORKER, encaminhar } from "../encaminhamento-supabase/supabase/functions/kangest/encaminhar.js";

const ADMIN = "e".repeat(40);
const env = {
  DB: d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8")),
  CHAVE_PRIVADA: generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" }),
  ADMIN_TOKEN: ADMIN,
};
const vistos = [];
// o Worker da Cloudflare, imitado: o pedido como lhe chega e a resposta comprimida a dizer que vem comprimida
const buscar = async (url, init) => {
  const r = new Request(url, init);
  vistos.push(r.clone());
  const resposta = await worker.fetch(r, env);
  const h = new Headers(resposta.headers);
  h.set("content-encoding", "br");
  return new Response(resposta.body, { status: resposta.status, headers: h });
};
const peloSupabase = (caminho, init = {}) =>
  encaminhar(new Request("https://fissktkxilseymadzppn.supabase.co" + caminho, {
    ...init,
    headers: { "cf-connecting-ip": "203.0.113.5", "cf-ray": "abc-LAD", "cdn-loop": "cloudflare", "x-forwarded-for": "203.0.113.5", "accept-encoding": "br, gzip", ...(init.headers ?? {}) },
  }), buscar);

// o caminho chega ao Worker sem o prefixo, com a consulta
let r = await peloSupabase("/kangest/v1/planos?x=1");
assert.equal(r.status, 200); assert.equal((await r.json()).planos.length, 3);
assert.equal(vistos.at(-1).url, WORKER + "/v1/planos?x=1");
assert.equal((await peloSupabase("/functions/v1/kangest")).status, 200);
assert.equal(vistos.at(-1).url, WORKER + "/");
// os cabeçalhos da viagem ficam para trás; os do pedido seguem; a resposta não diz que vem comprimida
for (const c of ["cf-connecting-ip", "cf-ray", "cdn-loop", "x-forwarded-for", "accept-encoding", "host"]) assert.equal(vistos.at(-1).headers.get(c), null, c);
assert.equal(r.headers.get("content-encoding"), null);

// um registo completo pelo encaminhamento: o corpo do POST chega inteiro ao Worker
r = await peloSupabase("/kangest/v1/registo", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ nif: "5000000007", empresa: "Loja 1.4.0", email: "loja140@exemplo.test", maquina: "ABCD-1234-EF61", versao: "1.4.0" }) });
assert.equal(r.status, 200); assert.equal((await r.json()).ok, true);
const codigo = (await env.DB.prepare("SELECT codigo_aberto FROM codigos WHERE nif = '5000000007'").first()).codigo_aberto;
r = await peloSupabase("/kangest/v1/confirmar", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ nif: "5000000007", maquina: "ABCD-1234-EF61", codigo }) });
const conf = await r.json();
assert.equal(conf.estado, "EXPERIENCIA"); assert.match(conf.licenca, /^KG1\./);
r = await peloSupabase("/kangest/v1/renovar", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ nif: "5000000007", maquina: "ABCD-1234-EF61", segredo: conf.segredo, versao: "1.4.0" }) });
assert.equal((await r.json()).id, conf.id, "a mesma licença, na mesma D1");

// a área do produtor, o CORS e o painel antigo passam tal como o Worker os responde
r = await peloSupabase("/kangest/admin/api/resumo", { headers: { authorization: "Bearer " + ADMIN } });
assert.equal(r.status, 200); assert.equal((await r.json()).clientes.length, 1);
r = await peloSupabase("/kangest/admin/api/resumo", { method: "OPTIONS", headers: { origin: "https://kangest.pages.dev" } });
assert.equal(r.status, 204); assert.equal(r.headers.get("access-control-allow-origin"), "https://kangest.pages.dev");
assert.equal((await peloSupabase("/kangest/admin/api/resumo")).status, 401);

// o Worker em baixo: 502 com uma mensagem para o KanGest
r = await encaminhar(new Request("https://fissktkxilseymadzppn.supabase.co/kangest/v1/planos"), async () => { throw new Error("sem rede"); });
assert.equal(r.status, 502); assert.match((await r.json()).erro, /não respondeu/);

console.log("Encaminhamento no Supabase: todos os testes passaram.");

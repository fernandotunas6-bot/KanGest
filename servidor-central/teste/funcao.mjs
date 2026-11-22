// A função kangest de ponta a ponta, sem rede nem Deno: node servidor-central/teste/funcao.mjs (depois de «npm ci»)
// Pedidos HTTP como o Supabase os entrega (prefixo /kangest, IP no X-Forwarded-For) → entrada.js → worker.js → d1.js →
// postgres.js → Postgres (PGlite com as migrações). Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { generateKeyPairSync, verify } from "node:crypto";
import { basePostgres } from "./pglite.mjs";

execFileSync(process.execPath, [new URL("../preparar.mjs", import.meta.url).pathname], { stdio: "inherit" });
const { adaptarPedido, criarEnv, ipDe, tratar } = await import("../supabase/functions/kangest/entrada.js");

const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const ADMIN = "c".repeat(40);
const SEGREDO_ENCAMINHADOR = "e".repeat(43);
const variaveis = {
  CHAVE_PRIVADA: privateKey.export({ type: "pkcs8", format: "pem" }),
  ADMIN_TOKEN: ADMIN,
  ENCAMINHADOR_SEGREDO: SEGREDO_ENCAMINHADOR,
  ACCESS_EQUIPA: "equipa.cloudflareaccess.com", // definidas por engano no Supabase: não podem fechar a área do produtor
  ACCESS_AUD: "aud",
  RESEND_API_KEY: "", // vazia = sem envio de email: o código fica para o produtor dar
};
const DB = await basePostgres();
const env = criarEnv(DB, (n) => variaveis[n]);
assert.ok(!("ACCESS_EQUIPA" in env) && !("RESEND_API_KEY" in env) && env.ADMIN_TOKEN === ADMIN);

const NIF = "5000000002";
const MAQ = "ABCD-1234-EF56";
const IP = "198.51.100.7";
async function pedir(metodo, caminho, corpo, token, cabecalhos = {}) {
  const r = await tratar(
    new Request("https://projecto.supabase.co/kangest" + caminho, {
      method: metodo,
      headers: {
        "content-type": "application/json",
        "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0 Safari/537.36",
        "x-forwarded-for": `${IP}, 10.0.0.1`,
        ...(token ? { authorization: "Bearer " + token } : {}),
        ...cabecalhos,
      },
      body: corpo ? JSON.stringify(corpo) : undefined,
      redirect: "manual",
    }),
    env,
  );
  const tipo = r.headers.get("content-type") ?? "";
  return { estado: r.status, corpo: tipo.includes("json") ? await r.json() : await r.text(), cabecalhos: r.headers };
}
const assinada = (lic) => {
  const [v, corpo, ass] = lic.split(".");
  return v === "KG1" && verify(null, Buffer.from(corpo), publicKey, Buffer.from(ass, "base64url"));
};

// a entrada: prefixo, IP e país
assert.equal(ipDe(new Headers({ "x-forwarded-for": " 203.0.113.9 , 10.0.0.1" })), "203.0.113.9");
assert.equal(ipDe(new Headers({ "cf-connecting-ip": "203.0.113.1", "x-forwarded-for": "203.0.113.9" })), "203.0.113.1", "a Cloudflare primeiro");
assert.equal(ipDe(new Headers()), null);
for (const [de, para] of [["/kangest", "/"], ["/kangest/v1/planos", "/v1/planos"], ["/functions/v1/kangest/v1/renovar", "/v1/renovar"], ["/kangestx/v1", "/kangestx/v1"]]) {
  assert.equal(new URL((await adaptarPedido(new Request("https://x.test" + de))).url).pathname, para);
}
assert.deepEqual((await adaptarPedido(new Request("https://x.test/kangest", { headers: { "cf-ipcountry": "ao" } }))).cf, { country: "AO" });
assert.deepEqual((await adaptarPedido(new Request("https://x.test/kangest", { headers: { "cf-ipcountry": "XX" } }))).cf, {});

// público
assert.equal((await pedir("GET", "")).corpo.servico, "Licenças do KanGest");
const planos = (await pedir("GET", "/v1/planos")).corpo;
assert.deepEqual(planos.planos?.map((p) => p.codigo) ?? planos.map((p) => p.codigo), ["ESSENCIAL", "COMERCIO", "EMPRESA"]);

// registo → código (sem email, fica para o produtor) → confirmar → licença de experiência assinada
const reg = await pedir("POST", "/v1/registo", { nif: NIF, empresa: "Loja de Teste", email: "Loja@Exemplo.test", telefone: "900 000 001", maquina: MAQ, versao: "1.4.0" });
assert.equal(reg.estado, 200, JSON.stringify(reg.corpo)); assert.equal(reg.corpo.enviado, false);
const resumo = (await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo;
const codigo = resumo.codigos.find((c) => c.tipo === "registo" && c.nif === NIF)?.codigo_aberto;
assert.match(codigo, /^\d{6}$/);
assert.equal((await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo: "000000" })).corpo.erro, "Código errado. Restam 4 tentativas.");
const conf = (await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo, versao: "1.4.0" })).corpo;
assert.equal(conf.estado, "EXPERIENCIA"); assert.ok(assinada(conf.licenca), "licença assinada com a CHAVE_PRIVADA");
assert.match(conf.segredo, /^[\w-]{43}$/);

// renovar: a mesma licença (não emite outra igual), o estado da instalação fica guardado
const ren = (await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.1", estado: { modo: "loja", licenca: { estado: "EXPERIENCIA", fim: conf.fim }, equipa_versao: 0 } })).corpo;
assert.equal(ren.ok, true); assert.equal(ren.id, conf.id); assert.equal(ren.equipa_gerida, undefined);
assert.equal((await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: "errado" })).estado, 401);

// área do produtor: pagamento (cêntimos, bigint), ficha do cliente, bloquear o computador
const pag = (await pedir("POST", "/admin/api/pagamento", { nif: NIF, plano: "COMERCIO", meses: 12, valor: 150000.5, metodo: "TRANSFERENCIA", referencia: "TRF-1" }, ADMIN)).corpo;
assert.equal(pag.ok, true, JSON.stringify(pag)); assert.equal(pag.licencas[0].estado, "ACTIVA"); assert.ok(assinada(pag.licencas[0].licenca));
const ficha = (await pedir("GET", `/admin/api/cliente?nif=${NIF}`, null, ADMIN)).corpo;
assert.equal(ficha.pagamentos[0].valor, 15000050); assert.equal(typeof ficha.pagamentos[0].id, "number");
assert.equal(ficha.maquinas[0].bloqueada, 0); assert.equal(ficha.maquinas[0].versao, "1.4.1"); assert.equal(ficha.maquinas[0].modo, "loja");
assert.equal(ficha.cliente.email, "loja@exemplo.test"); assert.equal(ficha.licencas.length, 2);
assert.equal((await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo.clientes.find((c) => c.nif === NIF).maquinas, 1, "COUNT vem como número");
assert.equal((await pedir("POST", "/admin/api/maquina", { nif: NIF, maquina: MAQ, bloqueada: true }, ADMIN)).estado, 200);
assert.equal((await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo })).estado, 403, "bloqueado");
assert.equal((await pedir("POST", "/admin/api/maquina", { nif: NIF, maquina: "FFFF-FFFF-FFFF", bloqueada: true }, ADMIN)).estado, 404, "meta.changes = 0");
assert.equal((await pedir("GET", "/admin/api/resumo", null, "errado")).estado, 401);

// conta da empresa: o país vem da Cloudflare (CF-IPCountry) e fica na sessão
await pedir("POST", "/v1/conta/codigo", { email: "loja@exemplo.test" });
const codigoConta = (await DB.prepare("SELECT codigo_aberto FROM codigos_conta WHERE email = ?").bind("loja@exemplo.test").first()).codigo_aberto;
assert.equal((await pedir("POST", "/v1/conta/definir-senha", { email: "loja@exemplo.test", codigo: codigoConta, senha: "Senha-de-teste-2026" })).estado, 200);
const token = (await pedir("POST", "/v1/conta/entrar", { email: "loja@exemplo.test", senha: "Senha-de-teste-2026" }, null, { "cf-ipcountry": "AO" })).corpo.token;
const sessoes = (await pedir("GET", "/v1/conta/sessoes", null, token)).corpo.sessoes;
assert.equal(sessoes[0].local, "AO"); assert.equal(sessoes[0].aparelho, "Chrome (Windows)");

// o painel antigo encaminha para o site; o site chama a API com CORS
const admin = await pedir("GET", "/admin");
assert.equal(admin.estado, 302); assert.equal(admin.cabecalhos.get("location"), "https://kangest.pages.dev/admin.html");
const pre = await pedir("OPTIONS", "/admin/api/resumo", null, null, { origin: "https://kangest.pages.dev" });
assert.equal(pre.estado, 204); assert.equal(pre.cabecalhos.get("access-control-allow-origin"), "https://kangest.pages.dev");
assert.equal((await pedir("POST", "/v1/registo", { nif: "x" }, null, { "content-length": String(2 * 1024 * 1024) })).estado, 413);

// o limite por IP usou o IP do X-Forwarded-For
const chaves = (await DB.prepare("SELECT chave FROM limites ORDER BY chave").all()).results.map((l) => l.chave);
assert.ok(chaves.includes(`registo:${IP}`) && chaves.includes(`confirmar:${IP}`) && chaves.includes(`renovar:${IP}`), chaves.join(", "));

// fase 4: o Worker antigo (encaminhar.js) passa os pedidos à função; o IP e o país de quem pediu seguem com o segredo
{
  const { default: encaminhar } = await import("../../servidor-licencas/src/encaminhar.js");
  const fetchOriginal = globalThis.fetch;
  const vistos = [];
  globalThis.fetch = async (url, init) => {
    const r = new Request(url, init);
    vistos.push(r.clone());
    return tratar(r, env);
  };
  const ENV_WORKER = { NOVO_SERVIDOR: "https://projecto.supabase.co/functions/v1/kangest/", ENCAMINHADOR_SEGREDO: SEGREDO_ENCAMINHADOR };
  const peloAntigo = (caminho, corpo, { pais, token } = {}) => {
    const r = new Request("https://kangest-licencas.exemplo.workers.dev" + caminho, {
      method: corpo ? "POST" : "GET",
      headers: {
        "content-type": "application/json",
        "user-agent": "KanGest/1.4.0",
        "cf-connecting-ip": "203.0.113.77",
        "x-kangest-ip": "6.6.6.6", // inventado por quem pede: o Worker substitui-o
        ...(token ? { authorization: "Bearer " + token } : {}),
      },
      body: corpo ? JSON.stringify(corpo) : undefined,
    });
    if (pais) Object.defineProperty(r, "cf", { value: { country: pais } });
    return encaminhar.fetch(r, ENV_WORKER);
  };
  const pl = await peloAntigo("/v1/planos?x=1");
  assert.equal(pl.status, 200); assert.equal((await pl.json()).planos.length, 3);
  assert.equal(new URL(vistos[0].url).pathname + new URL(vistos[0].url).search, "/functions/v1/kangest/v1/planos?x=1");
  assert.equal(vistos[0].headers.get("x-kangest-ip"), "203.0.113.77", "o IP verdadeiro, não o que quem pede inventou");
  await peloAntigo("/v1/registo", { nif: "5000000003", empresa: "Outra Loja", email: "outra@exemplo.test", maquina: "ABCD-1234-EF57" });
  assert.equal((await DB.prepare("SELECT n FROM limites WHERE chave = ?").bind("registo:203.0.113.77").first())?.n, 1, "o limite conta no IP de quem pediu");
  const tk = (await (await peloAntigo("/v1/conta/entrar", { email: "loja@exemplo.test", senha: "Senha-de-teste-2026" }, { pais: "PT" })).json()).token;
  assert.equal((await (await peloAntigo("/v1/conta/sessoes", null, { token: tk })).json()).sessoes[0].local, "PT", "o país chega às sessões");
  const adm = await peloAntigo("/admin");
  assert.equal(adm.status, 302); assert.equal(adm.headers.get("location"), "https://kangest.pages.dev/admin.html");
  // sem o segredo certo, a função ignora os cabeçalhos do encaminhador e usa o IP que o Supabase lhe dá
  await tratar(new Request("https://projecto.supabase.co/kangest/v1/registo", {
    method: "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": "198.51.100.99", "x-kangest-encaminhador": "errado", "x-kangest-ip": "6.6.6.6" },
    body: JSON.stringify({ nif: "5000000004", empresa: "Mais Uma", email: "mais@exemplo.test", maquina: "ABCD-1234-EF58" }),
  }), env);
  const lim = (await DB.prepare("SELECT chave FROM limites").all()).results.map((l) => l.chave);
  assert.ok(lim.includes("registo:198.51.100.99") && !lim.some((c) => c.includes("6.6.6.6")), lim.join(", "));
  // a função em baixo: 502; sem endereço novo configurado: 503
  globalThis.fetch = async () => { throw new Error("sem rede"); };
  assert.equal((await peloAntigo("/v1/planos")).status, 502);
  assert.equal((await encaminhar.fetch(new Request("https://x.workers.dev/v1/planos"), {})).status, 503);
  globalThis.fetch = fetchOriginal;
}

await DB.fechar();
console.log("Função kangest: todos os testes passaram (Postgres).");

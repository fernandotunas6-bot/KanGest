// Teste do servidor de licenças sem rede: node teste/testar.mjs
// Usa uma chave de teste gerada na hora (ou a de KANGEST_CHAVE_SERVIDOR, para gerar exemplos para o KanGest).
import assert from "node:assert/strict";
import { createPrivateKey, createPublicKey, generateKeyPairSync, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker, { hoje } from "../src/worker.js";

const pem = process.env.KANGEST_CHAVE_SERVIDOR
  ? readFileSync(process.env.KANGEST_CHAVE_SERVIDOR, "utf8")
  : generateKeyPairSync("ed25519").privateKey.export({ format: "pem", type: "pkcs8" });
const publica = createPublicKey(createPrivateKey(pem));
const ADMIN = "a".repeat(40);
const env = { DB: d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8")), CHAVE_PRIVADA: pem, ADMIN_TOKEN: ADMIN };

async function pedir(metodo, caminho, corpo, token) {
  const r = await worker.fetch(
    new Request("https://licencas.teste" + caminho, {
      method: metodo,
      headers: { "content-type": "application/json", ...(token ? { authorization: "Bearer " + token } : {}) },
      body: corpo ? JSON.stringify(corpo) : undefined,
    }),
    env,
  );
  const tipo = r.headers.get("content-type") ?? "";
  return { estado: r.status, corpo: tipo.includes("json") ? await r.json() : await r.text() };
}
const post = (c, b) => pedir("POST", c, b);
const admin = (c, b) => pedir(b ? "POST" : "GET", "/admin/api/" + c, b, ADMIN);

/** Confirma a assinatura e devolve os dados da licença (o mesmo que o licenca::ler do KanGest). */
function ler(chave) {
  const [pref, corpo, assinatura] = chave.split(".");
  assert.equal(pref, "KG1");
  assert.ok(verify(null, Buffer.from(corpo), publica, Buffer.from(assinatura, "base64url")), "assinatura válida");
  return JSON.parse(Buffer.from(corpo, "base64url").toString("utf8"));
}

const somar = (d, n) => { const x = new Date(d + "T12:00:00Z"); x.setUTCDate(x.getUTCDate() + n); return x.toISOString().slice(0, 10); };
const HOJE = hoje();
const NIF = "5000000000", M1 = "AAAA-BBBB-CCCC", M2 = "1111-2222-3333", M3 = "DEAD-BEEF-0001";
let r;

// planos públicos
r = await pedir("GET", "/v1/planos");
assert.equal(r.estado, 200);
assert.deepEqual(r.corpo.planos.map((p) => p.codigo), ["ESSENCIAL", "COMERCIO", "EMPRESA"]);

// validações do registo
r = await post("/v1/registo", { nif: NIF, empresa: "Empresa Exemplo, Lda", email: "nao-e-email", maquina: M1 });
assert.equal(r.estado, 400); assert.match(r.corpo.erro, /email/);
r = await post("/v1/registo", { nif: "12", empresa: "X Lda", email: "a@b.ao", maquina: M1 });
assert.equal(r.estado, 400); assert.match(r.corpo.erro, /NIF/);
r = await post("/v1/registo", { nif: NIF, empresa: "X Lda", email: "a@b.ao", maquina: "zz" });
assert.equal(r.estado, 400);
r = await post("/v1/renovar", { nif: NIF, maquina: M1 });
assert.equal(r.estado, 404, "renovar antes de registar");

// registo sem email configurado: o código fica visível no painel
r = await post("/v1/registo", { nif: "500 000 0000", empresa: "  Empresa Exemplo, Lda ", email: "Geral@Exemplo.ao", telefone: "923 000 000", maquina: M1.toLowerCase(), versao: "1.0.0" });
assert.equal(r.estado, 200, JSON.stringify(r.corpo));
assert.equal(r.corpo.enviado, false);
r = await admin("resumo");
const pedido = r.corpo.codigos.find((c) => c.maquina === M1);
assert.equal(pedido.nif, NIF); assert.equal(pedido.email, "geral@exemplo.ao"); assert.match(pedido.codigo_aberto, /^\d{6}$/);
const codigo = pedido.codigo_aberto;
const errado = codigo === "000000" ? "111111" : "000000";

// código errado conta tentativas
r = await post("/v1/confirmar", { nif: NIF, maquina: M1, codigo: errado });
assert.equal(r.estado, 400); assert.match(r.corpo.erro, /Restam 4 tentativas/);

// código certo: experiência de 30 dias, presa a este computador
r = await post("/v1/confirmar", { nif: NIF, maquina: M1, codigo: ` ${codigo.slice(0, 3)} ${codigo.slice(3)} ` });
assert.equal(r.estado, 200, JSON.stringify(r.corpo));
assert.equal(r.corpo.estado, "EXPERIENCIA");
let lic = ler(r.corpo.licenca);
assert.deepEqual(Object.keys(lic), ["v", "id", "nif", "empresa", "plano", "postos", "modulos", "inicio", "fim", "tolerancia", "emitida", "maquina"]);
assert.equal(lic.nif, NIF); assert.equal(lic.empresa, "Empresa Exemplo, Lda"); assert.equal(lic.plano, "EXPERIENCIA");
assert.equal(lic.inicio, HOJE); assert.equal(lic.fim, somar(HOJE, 29)); assert.equal(lic.tolerancia, 0); assert.equal(lic.maquina, M1);
const idExperiencia = lic.id;

// o código só serve uma vez
r = await post("/v1/confirmar", { nif: NIF, maquina: M1, codigo });
assert.equal(r.estado, 404);

// renovar sem mudanças devolve a mesma licença (não emite outra)
r = await post("/v1/renovar", { nif: NIF, maquina: M1, versao: "1.0.1" });
assert.equal(r.estado, 200); assert.equal(ler(r.corpo.licenca).id, idExperiencia);

// segundo computador do mesmo NIF: a experiência NÃO recomeça
const registar = async (maquina) => {
  const a = await post("/v1/registo", { nif: NIF, empresa: "Empresa Exemplo, Lda", email: "geral@exemplo.ao", maquina });
  if (a.estado !== 200) return a;
  const c = (await admin("resumo")).corpo.codigos.find((x) => x.maquina === maquina).codigo_aberto;
  return post("/v1/confirmar", { nif: NIF, maquina, codigo: c });
};
await env.DB.prepare("UPDATE clientes SET experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, 5), NIF).run();
r = await registar(M2);
assert.equal(r.estado, 200); lic = ler(r.corpo.licenca);
assert.equal(lic.fim, somar(HOJE, 5)); assert.equal(lic.maquina, M2);

// terceiro computador: passa o limite (2 por omissão)
r = await post("/v1/registo", { nif: NIF, empresa: "Empresa Exemplo, Lda", email: "geral@exemplo.ao", maquina: M3 });
assert.equal(r.estado, 409); assert.match(r.corpo.erro, /2 computadores/);

// experiência acabada: sem licença, com explicação
await env.DB.prepare("UPDATE clientes SET experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, -1), NIF).run();
r = await post("/v1/renovar", { nif: NIF, maquina: M1 });
assert.equal(r.corpo.licenca, null); assert.equal(r.corpo.estado, "EXPIRADA"); assert.match(r.corpo.mensagem, /experiência/);

// painel: sem chave não entra
assert.equal((await pedir("GET", "/admin/api/resumo")).estado, 401);
assert.equal((await pedir("GET", "/admin/api/resumo", null, "b".repeat(40))).estado, 401);
r = await pedir("GET", "/admin");
assert.equal(r.estado, 200); assert.match(r.corpo, /Painel do produtor/);

// preços
r = await admin("plano", { codigo: "COMERCIO", nome: "Comércio", postos: 3, preco_mes: "15000", preco_ano: 150000, activo: true, ordem: 2 });
assert.equal(r.estado, 200);
r = await pedir("GET", "/v1/planos");
assert.equal(r.corpo.planos.find((p) => p.codigo === "COMERCIO").preco_mes, 15000);

// pagamento de 12 meses: licença COMERCIO para os dois computadores
r = await admin("pagamento", { nif: NIF, plano: "COMERCIO", meses: 12, valor: "150000", metodo: "TRANSFERENCIA", referencia: "TRF-001" });
assert.equal(r.estado, 200, JSON.stringify(r.corpo));
const ate = r.corpo.pago_ate;
const esperado = new Date(HOJE + "T12:00:00Z"); esperado.setUTCMonth(esperado.getUTCMonth() + 12); esperado.setUTCDate(esperado.getUTCDate() - 1);
assert.equal(ate, esperado.toISOString().slice(0, 10));
assert.equal(r.corpo.licencas.length, 2);
r = await post("/v1/renovar", { nif: NIF, maquina: M1 });
lic = ler(r.corpo.licenca);
assert.equal(r.corpo.estado, "ACTIVA"); assert.equal(lic.plano, "COMERCIO"); assert.equal(lic.postos, 3); assert.equal(lic.fim, ate); assert.equal(lic.tolerancia, 15); assert.equal(lic.maquina, M1);
const exemplo = r.corpo.licenca;

// segundo pagamento soma a partir do fim já pago
r = await admin("pagamento", { nif: NIF, plano: "COMERCIO", meses: 1, valor: 15000, metodo: "MULTICAIXA" });
assert.equal(r.corpo.pago_ate > ate, true);
r = await admin("cliente?nif=" + NIF);
assert.equal(r.corpo.pagamentos.length, 2); assert.equal(r.corpo.maquinas.length, 2);
assert.equal(r.corpo.cliente.estado, "ACTIVA");

// postos especiais para o cliente
await admin("cliente", { nif: NIF, postos: 5 });
lic = ler((await post("/v1/renovar", { nif: NIF, maquina: M2 })).corpo.licenca);
assert.equal(lic.postos, 5);

// bloquear um computador
r = await admin("maquina", { nif: NIF, maquina: M2, bloqueada: true });
assert.equal(r.estado, 200);
r = await post("/v1/renovar", { nif: NIF, maquina: M2 });
assert.equal(r.estado, 403);

// licença manual (cliente sem internet), a partir do código do pedido
r = await admin("emitir", { nif: "5417189230", empresa: "Loja Offline, Lda", plano: "ESSENCIAL", inicio: "2027-01-01", meses: 6, maquina: "abcd-ef01-2345" });
assert.equal(r.estado, 200, JSON.stringify(r.corpo));
lic = ler(r.corpo.chave);
assert.equal(lic.plano, "ESSENCIAL"); assert.equal(lic.postos, 1); assert.equal(lic.inicio, "2027-01-01"); assert.equal(lic.fim, "2027-06-30"); assert.equal(lic.maquina, "ABCD-EF01-2345");
r = await admin("emitir", { nif: "5417189230", empresa: "Loja", plano: "XPTO" });
assert.equal(r.estado, 400);

// demasiados pedidos de código
for (let i = 0; i < 5; i++) await post("/v1/registo", { nif: "5111111111", empresa: "Outra, Lda", email: "o@o.ao", maquina: M3 });
r = await post("/v1/registo", { nif: "5111111111", empresa: "Outra, Lda", email: "o@o.ao", maquina: M3 });
assert.equal(r.estado, 429);

// tentativas esgotadas
for (let i = 0; i < 5; i++) await post("/v1/confirmar", { nif: "5111111111", maquina: M3, codigo: "x" });
r = await post("/v1/confirmar", { nif: "5111111111", maquina: M3, codigo: "123456" });
assert.equal(r.estado, 429);

// pedidos estranhos
assert.equal((await pedir("POST", "/v1/registo", [1, 2])).estado, 400);
assert.equal((await pedir("GET", "/nada")).estado, 404);

// ------------------------------------------------------------------ protecção

// cabeçalhos de segurança em todas as respostas
{
  const resp = await worker.fetch(new Request("https://licencas.teste/v1/planos"), env);
  assert.equal(resp.headers.get("x-content-type-options"), "nosniff");
  assert.match(resp.headers.get("strict-transport-security") ?? "", /max-age=/);
  assert.equal(resp.headers.get("access-control-allow-origin"), "*", "os cabeçalhos próprios ficam");
}

// limite por IP: a confirmação aceita LIMITES.confirmar pedidos por hora do mesmo IP
{
  const { LIMITES } = await import("../src/worker.js");
  const deIp = (ip, caminho, corpo) =>
    worker.fetch(new Request("https://licencas.teste" + caminho, { method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": ip }, body: JSON.stringify(corpo) }), env);
  for (let i = 0; i < LIMITES.confirmar; i++) {
    const x = await deIp("41.63.9.9", "/v1/confirmar", { nif: NIF, maquina: "9999-9999-9999", codigo: "000000" });
    assert.notEqual(x.status, 429, `pedido ${i + 1}`);
  }
  assert.equal((await deIp("41.63.9.9", "/v1/confirmar", { nif: NIF, maquina: "9999-9999-9999", codigo: "000000" })).status, 429);
  assert.notEqual((await deIp("41.63.9.10", "/v1/confirmar", { nif: NIF, maquina: "9999-9999-9999", codigo: "000000" })).status, 429, "outro IP não é afectado");

  // painel: tokens errados contam; ao fim de LIMITES.admin, o IP fica bloqueado (mesmo com o token certo)
  const painel = (ip, token) => worker.fetch(new Request("https://licencas.teste/admin/api/resumo", { headers: { "cf-connecting-ip": ip, authorization: "Bearer " + token } }), env);
  for (let i = 0; i < LIMITES.admin; i++) assert.equal((await painel("41.63.7.7", "errado")).status, 401);
  assert.equal((await painel("41.63.7.7", ADMIN)).status, 429);
  assert.equal((await painel("41.63.7.8", ADMIN)).status, 200);
}

// Cloudflare Access à frente do painel: sem o JWT da equipa (ou com o de outra aplicação), nem com o token se entra
{
  const { generateKeyPairSync, sign } = await import("node:crypto");
  const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const jwk = { ...publicKey.export({ format: "jwk" }), kid: "k1", alg: "RS256" };
  const EQUIPA = "kangest.cloudflareaccess.com", AUD = "c".repeat(64);
  const fetchOriginal = globalThis.fetch;
  let buscas = 0;
  globalThis.fetch = async (url) => {
    if (String(url) === `https://${EQUIPA}/cdn-cgi/access/certs`) {
      buscas++;
      return new Response(JSON.stringify({ keys: [jwk] }), { headers: { "content-type": "application/json" } });
    }
    return fetchOriginal(url);
  };
  const b64u = (x) => Buffer.from(x).toString("base64url");
  const jwt = (claims, kid = "k1") => {
    const h = b64u(JSON.stringify({ alg: "RS256", kid, typ: "JWT" }));
    const p = b64u(JSON.stringify(claims));
    return `${h}.${p}.${sign("RSA-SHA256", Buffer.from(`${h}.${p}`), privateKey).toString("base64url")}`;
  };
  const agoraS = Math.floor(Date.now() / 1000);
  const envAccess = { ...env, ACCESS_EQUIPA: EQUIPA, ACCESS_AUD: AUD };
  const painel = (extra) => worker.fetch(new Request("https://licencas.teste/admin/api/resumo", { headers: { authorization: "Bearer " + ADMIN, ...extra } }), envAccess);
  assert.equal((await painel({})).status, 403, "sem o JWT do Access");
  const bom = jwt({ aud: [AUD], iss: `https://${EQUIPA}`, exp: agoraS + 300, iat: agoraS, email: "dono@kangest.ao" });
  assert.equal((await painel({ "cf-access-jwt-assertion": bom })).status, 200);
  assert.equal((await painel({ cookie: `outro=1; CF_Authorization=${bom}` })).status, 200, "também pelo cookie do browser");
  assert.equal((await painel({ "cf-access-jwt-assertion": jwt({ aud: ["d".repeat(64)], iss: `https://${EQUIPA}`, exp: agoraS + 300 }) })).status, 403, "outra aplicação");
  assert.equal((await painel({ "cf-access-jwt-assertion": jwt({ aud: [AUD], iss: `https://${EQUIPA}`, exp: agoraS - 300 }) })).status, 403, "expirado");
  assert.equal((await painel({ "cf-access-jwt-assertion": bom.slice(0, -4) + "AAAA" })).status, 403, "assinatura alterada");
  assert.equal((await worker.fetch(new Request("https://licencas.teste/admin", { headers: { "cf-access-jwt-assertion": bom } }), envAccess)).status, 200, "a página do painel");
  assert.equal((await worker.fetch(new Request("https://licencas.teste/admin"), envAccess)).status, 403);
  // um kid desconhecido só volta a buscar as chaves de 5 em 5 minutos
  const antes = buscas;
  await painel({ "cf-access-jwt-assertion": jwt({ aud: [AUD], iss: `https://${EQUIPA}`, exp: agoraS + 300 }, "k9") });
  await painel({ "cf-access-jwt-assertion": jwt({ aud: [AUD], iss: `https://${EQUIPA}`, exp: agoraS + 300 }, "k9") });
  assert.ok(buscas - antes <= 1, "não vai à Cloudflare a cada JWT inventado");
  // as rotas públicas não pedem o Access
  assert.equal((await worker.fetch(new Request("https://licencas.teste/v1/planos"), envAccess)).status, 200);
  globalThis.fetch = fetchOriginal;
}

console.log("Servidor de licenças: todos os testes passaram.");
if (process.env.KANGEST_CHAVE_SERVIDOR) console.log("Licença de exemplo:", exemplo);

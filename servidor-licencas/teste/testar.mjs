// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

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
// o computador recebe o seu segredo (só agora); na base fica só o resumo
const SEGREDO_M1 = r.corpo.segredo;
assert.match(SEGREDO_M1, /^[A-Za-z0-9_-]{43}$/);
assert.ok(!JSON.stringify(await env.DB.prepare("SELECT * FROM segredos_maquina").all()).includes(SEGREDO_M1), "o segredo não fica em claro");

// o código só serve uma vez
r = await post("/v1/confirmar", { nif: NIF, maquina: M1, codigo });
assert.equal(r.estado, 404);

// renovar sem mudanças devolve a mesma licença (não emite outra); com segredo, só com ele
r = await post("/v1/renovar", { nif: NIF, maquina: M1, versao: "1.0.1" });
assert.equal(r.estado, 401, "quem só sabe o NIF e o código do computador já não renova por ele");
r = await post("/v1/renovar", { nif: NIF, maquina: M1, versao: "1.0.1", segredo: SEGREDO_M1.slice(0, -1) + "x" });
assert.equal(r.estado, 401);
r = await post("/v1/renovar", { nif: NIF, maquina: M1, versao: "1.0.1", segredo: SEGREDO_M1 });
assert.equal(r.estado, 200); assert.equal(ler(r.corpo.licenca).id, idExperiencia);

// segundo computador do mesmo NIF: a experiência NÃO recomeça
// renovação com o segredo de cada computador (recebido ao confirmar o registo)
const SEGREDOS = {};
const renovar = (corpo) => post("/v1/renovar", { ...corpo, segredo: SEGREDOS[corpo.maquina] });
const registar = async (maquina) => {
  const a = await post("/v1/registo", { nif: NIF, empresa: "Empresa Exemplo, Lda", email: "geral@exemplo.ao", maquina });
  if (a.estado !== 200) return a;
  const c = (await admin("resumo")).corpo.codigos.find((x) => x.maquina === maquina && x.nif === NIF).codigo_aberto;
  const x = await post("/v1/confirmar", { nif: NIF, maquina, codigo: c });
  if (x.corpo.segredo) SEGREDOS[maquina] = x.corpo.segredo;
  return x;
};
SEGREDOS[M1] = SEGREDO_M1;
await env.DB.prepare("UPDATE clientes SET experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, 5), NIF).run();
r = await registar(M2);
assert.equal(r.estado, 200); lic = ler(r.corpo.licenca);
assert.equal(lic.fim, somar(HOJE, 5)); assert.equal(lic.maquina, M2);

// terceiro computador: passa o limite (2 por omissão)
r = await post("/v1/registo", { nif: NIF, empresa: "Empresa Exemplo, Lda", email: "geral@exemplo.ao", maquina: M3 });
assert.equal(r.estado, 409); assert.match(r.corpo.erro, /2 computadores/);

// experiência acabada: sem licença, com explicação
await env.DB.prepare("UPDATE clientes SET experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, -1), NIF).run();
r = await renovar({ nif: NIF, maquina: M1 });
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
r = await renovar({ nif: NIF, maquina: M1 });
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
lic = ler((await renovar({ nif: NIF, maquina: M2 })).corpo.licenca);
assert.equal(lic.postos, 5);

// bloquear um computador
r = await admin("maquina", { nif: NIF, maquina: M2, bloqueada: true });
assert.equal(r.estado, 200);
r = await renovar({ nif: NIF, maquina: M2 });
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


// ------------------------------------------------------------------ acesso pela Internet (subdomínios)
{
  const pedidosCf = [];
  const fetchOriginal = globalThis.fetch;
  let n = 0;
  let estadoTunel = "healthy";
  globalThis.fetch = async (url, op = {}) => {
    const u = String(url);
    assert.ok(u.startsWith("https://api.cloudflare.com/client/v4/"), u);
    assert.equal(op.headers.authorization, "Bearer token-do-produtor");
    const caminho = u.slice("https://api.cloudflare.com/client/v4".length);
    pedidosCf.push(`${op.method} ${caminho}`);
    const corpo = op.body ? JSON.parse(op.body) : null;
    let result = {};
    if (op.method === "POST" && caminho.endsWith("/cfd_tunnel")) result = { id: `tunel-${++n}`, name: corpo.name };
    else if (op.method === "POST" && caminho.endsWith("/dns_records")) { result = { id: `dns-${++n}`, ...corpo }; pedidosCf.push(JSON.stringify(corpo)); }
    else if (op.method === "PUT") pedidosCf.push(JSON.stringify(corpo));
    else if (op.method === "GET" && caminho.endsWith("/token")) result = "token-do-tunel";
    else if (op.method === "GET" && /\/cfd_tunnel\/[^/]+$/.test(caminho)) result = { id: caminho.split("/").pop(), status: estadoTunel };
    return new Response(JSON.stringify({ success: true, result }), { headers: { "content-type": "application/json" } });
  };
  const envWeb = { ...env, WEB_DOMINIO: "kangest.ao", CF_CONTA: "conta1", CF_ZONA: "zona1", CF_API_TOKEN: "token-do-produtor" };
  const web = async (accao, corpo, e = envWeb) => {
    const x = await worker.fetch(new Request("https://licencas.teste/v1/web/" + accao, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(corpo) }), e);
    return { estado: x.status, corpo: await x.json() };
  };
  const eu = { nif: NIF, maquina: M1, segredo: SEGREDO_M1 };

  // sem o segredo do computador, nada
  assert.equal((await web("estado", { nif: NIF, maquina: M1 })).estado, 401);
  // sem a configuração do produtor: não disponível
  r = await web("estado", eu, env);
  assert.equal(r.estado, 200); assert.equal(r.corpo.disponivel, false);
  assert.equal((await web("activar", { ...eu, nome: "kianda" }, env)).estado, 503);

  // nomes inválidos e reservados
  for (const nome of ["ab", "-loja", "loja-", "a--b", "Loja Kianda", "x".repeat(31)]) {
    assert.equal((await web("activar", { ...eu, nome })).estado, 400, nome);
  }
  r = await web("activar", { ...eu, nome: "admin" });
  assert.equal(r.estado, 400); assert.match(r.corpo.erro, /reservado/);

  // activar: túnel, configuração (localhost:7799), DNS CNAME com proxy e o token
  r = await web("activar", { ...eu, nome: "Kianda" });
  assert.equal(r.estado, 200, JSON.stringify(r.corpo));
  assert.deepEqual([r.corpo.nome, r.corpo.endereco, r.corpo.token], ["kianda", "https://kianda.kangest.ao", "token-do-tunel"]);
  assert.ok(pedidosCf.includes("POST /accounts/conta1/cfd_tunnel"));
  assert.ok(pedidosCf.some((x) => x.includes('"hostname":"kianda.kangest.ao","service":"http://localhost:7799"') && x.includes("http_status:404")));
  assert.ok(pedidosCf.some((x) => x.includes('"type":"CNAME"') && x.includes('"content":"tunel-1.cfargotunnel.com"') && x.includes('"proxied":true')));
  r = await web("estado", eu);
  assert.deepEqual([r.corpo.disponivel, r.corpo.nome, r.corpo.endereco, r.corpo.neste_computador], [true, "kianda", "https://kianda.kangest.ao", true]);

  // página de entrada: endereço da empresa pelo nome, e se o computador da loja está ligado; público, com CORS
  const entrar = async (nome, e = envWeb) => {
    const x = await worker.fetch(new Request("https://licencas.teste/v1/web/endereco?nome=" + encodeURIComponent(nome)), e);
    assert.equal(x.headers.get("access-control-allow-origin"), "*");
    return { estado: x.status, corpo: await x.json() };
  };
  r = await entrar(" Kianda ");
  assert.equal(r.estado, 200, JSON.stringify(r.corpo));
  assert.deepEqual([r.corpo.endereco, r.corpo.ligado], ["https://kianda.kangest.ao", true]);
  estadoTunel = "down";
  assert.equal((await entrar("kianda")).corpo.ligado, false);
  estadoTunel = "healthy";
  r = await entrar("nao-existe");
  assert.equal(r.estado, 404); assert.match(r.corpo.erro, /nao-existe\.kangest\.ao/);
  assert.equal((await entrar("a b")).estado, 400);
  assert.equal((await entrar("kianda", env)).estado, 503);

  // activar outra vez (ex.: reinstalação) reaproveita o túnel e o DNS
  pedidosCf.length = 0;
  r = await web("activar", { ...eu, nome: "kianda" });
  assert.equal(r.estado, 200);
  assert.ok(!pedidosCf.some((x) => x.startsWith("POST")), "nem túnel nem DNS novos: " + pedidosCf.join(" | "));

  // outro computador da mesma empresa não o pode tomar; outra empresa não usa o mesmo nome
  await env.DB.prepare("UPDATE clientes SET maquinas_max = 3, experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, 10), NIF).run();
  await env.DB.prepare("DELETE FROM limites").run();
  r = await registar(M3);
  assert.equal(r.estado, 200, JSON.stringify(r.corpo));
  const eu3 = { nif: NIF, maquina: M3, segredo: r.corpo.segredo };
  r = await web("activar", { ...eu3, nome: "kianda" });
  assert.equal(r.estado, 409); assert.match(r.corpo.erro, /noutro computador/);
  await env.DB.prepare("INSERT INTO subdominios (nif, nome, maquina, tunel_id, dns_id, porta, criado_em, actualizado_em) VALUES ('5999999999', 'outra', 'X', 't', 'd', 7799, '', '')").run();
  r = await web("activar", { ...eu, nome: "outra" });
  assert.equal(r.estado, 409); assert.match(r.corpo.erro, /já está em uso/);

  // mudar de nome: DNS novo e o antigo apagado, o mesmo túnel
  pedidosCf.length = 0;
  r = await web("activar", { ...eu, nome: "kianda-luanda", porta: 7800 });
  assert.equal(r.estado, 200); assert.equal(r.corpo.endereco, "https://kianda-luanda.kangest.ao");
  assert.ok(pedidosCf.some((x) => x.startsWith("DELETE /zones/zona1/dns_records/dns-")), pedidosCf.join(" | "));
  assert.ok(!pedidosCf.includes("POST /accounts/conta1/cfd_tunnel"));
  assert.ok(pedidosCf.some((x) => x.includes("http://localhost:7800")));

  // desligar (de qualquer computador registado da empresa): apaga DNS, ligações e túnel
  pedidosCf.length = 0;
  r = await web("desactivar", eu3);
  assert.equal(r.estado, 200);
  assert.ok(pedidosCf.some((x) => x.startsWith("DELETE /zones/zona1/dns_records/")));
  assert.ok(pedidosCf.some((x) => /^DELETE \/accounts\/conta1\/cfd_tunnel\/tunel-1$/.test(x)), pedidosCf.join(" | "));
  r = await web("estado", eu);
  assert.equal(r.corpo.nome, null);

  // assinatura terminada: não activa
  await env.DB.prepare("UPDATE clientes SET pago_ate = NULL, experiencia_ate = ? WHERE nif = ?").bind(somar(HOJE, -60), NIF).run();
  assert.equal((await web("activar", { ...eu, nome: "kianda" })).estado, 402);
  globalThis.fetch = fetchOriginal;
}

// verificação de humano (Turnstile): sem configuração não se exige; com ela, o Cloudflare decide; sem resposta, passa
{
  let r = await pedir("GET", "/v1/web/humano");
  assert.equal(r.corpo.site_key, null);
  assert.equal((await post("/v1/web/humano", { token: "x" })).corpo.humano, true, "sem configuração não se exige");
  Object.assign(env, { TURNSTILE_SITE_KEY: "0x4AAA-site", TURNSTILE_SECRET_KEY: "0x4AAA-segredo", WEB_DOMINIO: "kangest.ao" });
  r = await pedir("GET", "/v1/web/humano");
  assert.equal(r.corpo.site_key, "0x4AAA-site"); assert.equal(r.corpo.dominio, "kangest.ao");
  assert.equal(JSON.stringify(r.corpo).includes("segredo"), false, "o segredo nunca sai");
  const original = globalThis.fetch;
  let enviado = null;
  let resposta = { success: true };
  globalThis.fetch = async (url, init) => {
    if (String(url).includes("siteverify")) {
      enviado = Object.fromEntries(init.body);
      if (resposta === "falha") throw new Error("rede");
      return new Response(JSON.stringify(resposta), { status: 200, headers: { "content-type": "application/json" } });
    }
    return original(url, init);
  };
  r = await post("/v1/web/humano", { token: "tok-bom", ip: "203.0.113.7" });
  assert.equal(r.corpo.humano, true);
  assert.deepEqual(enviado, { secret: "0x4AAA-segredo", response: "tok-bom", remoteip: "203.0.113.7" });
  resposta = { success: false, "error-codes": ["timeout-or-duplicate"] };
  r = await post("/v1/web/humano", { token: "tok-repetido" });
  assert.equal(r.corpo.humano, false); assert.equal(r.corpo.motivo, "timeout-or-duplicate");
  assert.equal((await post("/v1/web/humano", { token: "" })).corpo.humano, false, "sem token, não");
  resposta = { success: false, "error-codes": ["invalid-input-secret"] };
  assert.deepEqual((await post("/v1/web/humano", { token: "t" })).corpo, { ok: true, humano: true, degradado: true }, "a nossa chave errada não tranca ninguém");
  resposta = "falha";
  assert.equal((await post("/v1/web/humano", { token: "t" })).corpo.degradado, true, "sem resposta do Cloudflare, passa");
  globalThis.fetch = original;
  for (const k of ["TURNSTILE_SITE_KEY", "TURNSTILE_SECRET_KEY", "WEB_DOMINIO"]) delete env[k];
}

console.log("Servidor de licenças: todos os testes passaram.");
if (process.env.KANGEST_CHAVE_SERVIDOR) console.log("Licença de exemplo:", exemplo);

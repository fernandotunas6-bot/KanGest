// Os emails do servidor de licenças, sem rede: node servidor-licencas/teste/emails.mjs
// A ligação EMAIL da Cloudflare Email Service imitada (guarda o que «envia»): os códigos do registo e da conta, o aviso ao
// produtor e as boas-vindas; HTML escapado, versão em texto, domínios de teste de fora e o Resend como alternativa.
// Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker from "../src/worker.js";

const SCHEMA = readFileSync(new URL("../schema.sql", import.meta.url), "utf8");
const enviados = [];
let falhar = null; // um destinatário para o qual a «Cloudflare» recusa o envio
const EMAIL = {
  async send(m) {
    if (falhar && m.to === falhar) throw Object.assign(new Error("limite"), { code: "E_RATE_LIMIT_EXCEEDED" });
    enviados.push(m);
    return { messageId: `m${enviados.length}` };
  },
};
const base = () => ({
  DB: d1Local(SCHEMA),
  CHAVE_PRIVADA: generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" }),
  ADMIN_TOKEN: "f".repeat(40),
  ADMIN_EMAIL: "Produtor@kangest-exemplo.ao",
  EMAIL_REMETENTE: "KanGest <codigos@kangest.exemplo-livraria.ao>",
  EMAIL,
});
let env = base();
// como o Worker: o que fica para depois da resposta espera-se aqui
async function pedir(metodo, caminho, corpo) {
  const pendentes = [];
  const r = await worker.fetch(
    new Request("https://licencas.teste" + caminho, { method: metodo, headers: { "content-type": "application/json" }, body: corpo ? JSON.stringify(corpo) : undefined }),
    env,
    { waitUntil: (p) => pendentes.push(p) },
  );
  await Promise.all(pendentes);
  return { estado: r.status, corpo: await r.json() };
}
const EMPRESA = "Loja <Ouro> & Filhos";
const NIF = "5000000008";
const MAQ = "ABCD-1234-EF62";
const CLIENTE = "loja@empresa-exemplo.ao";

// registo: o código para a empresa e o aviso para o produtor
let r = await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, telefone: "923 000 001", maquina: MAQ, versao: "1.4.1" });
assert.equal(r.corpo.enviado, true); assert.match(r.corpo.mensagem, /Enviámos um código/);
assert.equal(enviados.length, 2);
const [cod, aviso] = enviados;
assert.deepEqual(cod.from, { email: "codigos@kangest.exemplo-livraria.ao", name: "KanGest" });
assert.equal(cod.to, CLIENTE);
const codigo = cod.subject.match(/(\d{6})$/)?.[1];
assert.match(codigo ?? "", /^\d{6}$/); assert.equal(cod.subject, `KanGest: código de confirmação ${codigo}`);
assert.ok(cod.html.includes(`>${codigo}</div>`) && cod.text.includes(`    ${codigo}`), "o código no HTML e no texto");
assert.ok(cod.html.includes("Loja &lt;Ouro&gt; &amp; Filhos") && !cod.html.includes("<Ouro>"), "o nome da empresa escapado");
assert.ok(cod.text.includes(EMPRESA) && cod.text.includes("Sistema → Licença e Assinatura"));
assert.ok(cod.html.startsWith("<!doctype html>") && cod.html.includes('lang="pt"') && cod.html.includes("#e6f2f0"), "o visual do site");
assert.ok(cod.html.includes('<img src="https://kangest.pages.dev/assets/kangest-email.png" width="36" height="36"'), "o logótipo em PNG, do site");
assert.equal((await env.DB.prepare("SELECT codigo_aberto FROM codigos WHERE nif = ?").bind(NIF).first()).codigo_aberto, null, "enviado: o código não fica à vista");
assert.equal(aviso.to, "produtor@kangest-exemplo.ao"); assert.equal(aviso.subject, `KanGest: pedido de registo de ${EMPRESA}`);
assert.ok(aviso.text.includes(`NIF: ${NIF}`) && aviso.text.includes(`Email: ${CLIENTE}`) && aviso.text.includes("Versão: 1.4.1"));
assert.ok(!aviso.text.includes(codigo) && !aviso.html.includes(codigo), "o código chegou à empresa: o aviso não o leva");
assert.ok(aviso.html.includes("https://kangest.pages.dev/admin.html"));

// pedir o código outra vez na mesma hora: novo código para a empresa, sem novo aviso ao produtor
enviados.length = 0;
await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, maquina: MAQ });
assert.deepEqual(enviados.map((m) => m.to), [CLIENTE]);

// o envio para a empresa falha: o código fica à vista e o aviso ao produtor leva-o
enviados.length = 0;
falhar = CLIENTE;
r = await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, maquina: MAQ });
falhar = null;
assert.equal(r.corpo.enviado, false);
const aberto = (await env.DB.prepare("SELECT codigo_aberto FROM codigos WHERE nif = ?").bind(NIF).first()).codigo_aberto;
assert.match(aberto, /^\d{6}$/);
assert.equal(enviados.length, 1); assert.ok(enviados[0].text.includes(aberto) && enviados[0].text.includes("não foi enviado"), "o produtor recebe o código para o dar");

// confirmar: boas-vindas, só da primeira vez
enviados.length = 0;
r = await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo: aberto });
assert.equal(r.corpo.estado, "EXPERIENCIA");
assert.equal(enviados.length, 1);
const ola = enviados[0];
assert.equal(ola.to, CLIENTE); assert.equal(ola.subject, `KanGest: registo de ${EMPRESA} confirmado`);
const ate = r.corpo.fim;
assert.ok(ola.text.includes(`${ate.slice(8, 10)}/${ate.slice(5, 7)}/${ate.slice(0, 4)}`) && ola.text.includes(MAQ), "a data da experiência e o computador");
assert.ok(ola.html.includes("https://kangest.pages.dev/conta.html"));
enviados.length = 0;
await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, maquina: MAQ });
const outro = enviados.find((m) => m.to === CLIENTE).subject.match(/(\d{6})$/)[1];
enviados.length = 0;
await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo: outro });
assert.equal(enviados.length, 0, "confirmar de novo não volta a dar as boas-vindas");

// conta da empresa: o código com a ligação para o site (as origens autorizadas dão o endereço)
env.ADMIN_ORIGENS = "https://kangest.exemplo-livraria.ao, https://kangest.pages.dev";
await pedir("POST", "/v1/conta/codigo", { email: CLIENTE });
assert.equal(enviados.length, 1);
const conta = enviados[0];
assert.match(conta.subject, /^KanGest: código \d{6} para a conta de Loja <Ouro> & Filhos$/);
assert.ok(conta.html.includes("https://kangest.exemplo-livraria.ao/conta.html") && conta.text.includes("Abrir a conta da empresa: https://kangest.exemplo-livraria.ao/conta.html"));
assert.equal((await env.DB.prepare("SELECT codigo_aberto FROM codigos_conta WHERE email = ?").bind(CLIENTE).first()).codigo_aberto, null);

// domínios de teste (a prova de fumo): nada sai, nem para o produtor
enviados.length = 0;
r = await pedir("POST", "/v1/registo", { nif: "FUMO00000001", empresa: "Prova de fumo", email: "fumo-1@kangest.invalid", maquina: "F0F0-0000-0001" });
assert.equal(r.corpo.enviado, false); assert.equal(enviados.length, 0);

// sem ligação nem Resend: nada sai e o código fica para o produtor
env = { ...base(), EMAIL: undefined };
r = await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, maquina: MAQ });
assert.equal(r.corpo.enviado, false); assert.equal(enviados.length, 0);

// o Resend, quando não há a ligação da Cloudflare
const fetchOriginal = globalThis.fetch;
const pedidosResend = [];
globalThis.fetch = async (url, init) => {
  if (String(url) === "https://api.resend.com/emails") {
    pedidosResend.push({ auth: init.headers.authorization, corpo: JSON.parse(init.body) });
    return new Response("{}", { status: 200 });
  }
  return fetchOriginal(url, init);
};
env = { ...base(), EMAIL: undefined, RESEND_API_KEY: "re_teste" };
r = await pedir("POST", "/v1/registo", { nif: NIF, empresa: EMPRESA, email: CLIENTE, maquina: MAQ });
globalThis.fetch = fetchOriginal;
assert.equal(r.corpo.enviado, true);
assert.equal(pedidosResend.length, 2);
assert.equal(pedidosResend[0].auth, "Bearer re_teste");
assert.equal(pedidosResend[0].corpo.from, "KanGest <codigos@kangest.exemplo-livraria.ao>");
assert.ok(pedidosResend[0].corpo.html.includes("Confirme o registo do KanGest") && pedidosResend[0].corpo.text.includes("Confirme o registo do KanGest"));

console.log("Emails: todos os testes passaram.");

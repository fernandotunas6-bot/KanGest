// A conta criada no site antes de instalar o KanGest, sem rede: node servidor-licencas/teste/conta-nova.mjs
// O código por email, a empresa «por registar», a experiência que só começa no primeiro registo do programa, a protecção
// contra quem crie a conta com o NIF de outra empresa e o que o produtor vê. Os dados são de exemplo (fictícios).
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker, { hoje as hojeEmLuanda } from "../src/worker.js";

const ADMIN = "c".repeat(40);
const enviados = [];
const EMAIL = { async send(m) { enviados.push(m); return { messageId: `m${enviados.length}` }; } };
const env = {
  DB: d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8")),
  ADMIN_TOKEN: ADMIN,
  CHAVE_PRIVADA: generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" }),
  ADMIN_EMAIL: "produtor@kangest-exemplo.ao",
  EMAIL_REMETENTE: "KanGest <codigos@kangest.exemplo-livraria.ao>",
  EMAIL,
};
async function pedir(metodo, caminho, corpo, token) {
  const pendentes = [];
  const r = await worker.fetch(
    new Request("https://licencas.teste" + caminho, {
      method: metodo,
      headers: { "content-type": "application/json", "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0 Safari/537.36", ...(token ? { authorization: "Bearer " + token } : {}) },
      body: corpo ? JSON.stringify(corpo) : undefined,
    }),
    env,
    { waitUntil: (p) => pendentes.push(p) },
  );
  await Promise.all(pendentes);
  return { estado: r.status, corpo: await r.json() };
}
const post = (c, b, t) => pedir("POST", "/v1/conta/" + c, b, t);
const codigoDe = (m) => m.subject.match(/(\d{6})/)?.[1];
const hoje = hojeEmLuanda(); // o dia do servidor (Luanda, UTC+1): entre as 23h e a meia-noite UTC já é amanhã
const somar = (d, n) => { const x = new Date(d + "T12:00:00Z"); x.setUTCDate(x.getUTCDate() + n); return x.toISOString().slice(0, 10); };

const NIF = "5000000011";
const DONO = "dona@papelaria-exemplo.ao";
const NOVA = { empresa: "Papelaria <Sol> Lda", nif: " 500 000 0011 ", email: " Dona@Papelaria-Exemplo.ao ", telefone: "923 000 111" };

// os dados têm de estar certos antes de sair qualquer código
assert.match((await post("criar", { ...NOVA, empresa: "x" })).corpo.erro, /nome da empresa/);
assert.match((await post("criar", { ...NOVA, email: "isto-nao-e-email" })).corpo.erro, /email válido/);
assert.match((await post("criar", { ...NOVA, nif: "12" })).corpo.erro, /NIF/);
assert.match((await post("criar", { ...NOVA, telefone: "liga-me" })).corpo.erro, /telefone/);
assert.equal(enviados.length, 0);

// pedir: o código vai para o email, só o resumo fica guardado
let r = await post("criar", NOVA);
assert.equal(r.estado, 200); assert.equal(r.corpo.enviado, true); assert.match(r.corpo.mensagem, /^Enviámos um código/);
assert.equal(enviados.length, 1);
const [m1] = enviados;
assert.equal(m1.to, DONO);
assert.equal(m1.subject, `KanGest: código ${codigoDe(m1)} para criar a conta de Papelaria <Sol> Lda`);
assert.ok(m1.html.includes("Papelaria &lt;Sol&gt; Lda") && !m1.html.includes("<Sol>"), "o nome escapado");
const pedido = await env.DB.prepare("SELECT * FROM pedidos_conta WHERE email = ?").bind(DONO).first();
assert.equal(pedido.nif, NIF); assert.equal(pedido.codigo_aberto, null); assert.ok(!pedido.codigo_hash.includes(codigoDe(m1)));
assert.equal((await env.DB.prepare("SELECT COUNT(*) AS n FROM clientes").first()).n, 0, "a empresa só nasce ao confirmar");
assert.equal((await post("criar", NOVA)).estado, 429, "outro código só depois de um minuto");

// o produtor vê o pedido em «Pedidos», sem o código (foi por email)
let resumo = (await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo;
const visto = resumo.codigos.find((c) => c.tipo === "conta-nova");
assert.ok(visto && visto.nif === NIF && visto.empresa === "Papelaria <Sol> Lda" && visto.codigo_aberto === null);

// confirmar: código errado, senha fraca, senha certa
assert.equal((await post("criar/confirmar", { email: DONO, codigo: "000000", senha: "Senha-forte-2026" })).corpo.erro, "Código errado.");
assert.match((await post("criar/confirmar", { email: DONO, codigo: codigoDe(m1), senha: "curta1" })).corpo.erro, /entre 10 e 128/);
enviados.length = 0;
r = await post("criar/confirmar", { email: DONO, codigo: codigoDe(m1), senha: "Senha-forte-2026" });
assert.equal(r.estado, 200); assert.match(r.corpo.token, /^c1\./); assert.equal(r.corpo.email, DONO);
const tokenDona = r.corpo.token;
assert.equal(await env.DB.prepare("SELECT 1 AS x FROM pedidos_conta WHERE email = ?").bind(DONO).first(), null, "o pedido desfaz-se");
assert.match((await env.DB.prepare("SELECT senha FROM contas WHERE nif = ?").bind(NIF).first()).senha, /^pbkdf2-sha256\$/);
assert.equal((await post("criar/confirmar", { email: DONO, codigo: codigoDe(m1), senha: "Senha-forte-2026" })).estado, 400, "o código só serve uma vez");
// o produtor recebe o aviso da conta nova
assert.equal(enviados.length, 1);
assert.equal(enviados[0].to, "produtor@kangest-exemplo.ao"); assert.equal(enviados[0].subject, "KanGest: conta nova de Papelaria <Sol> Lda");
assert.ok(enviados[0].text.includes(`NIF: ${NIF}`) && enviados[0].text.includes(`Email: ${DONO}`));

// a conta abre-se logo: a empresa está «por registar», sem experiência a contar
let conta = (await pedir("GET", "/v1/conta/resumo", null, tokenDona)).corpo;
assert.equal(conta.empresa.nif, NIF); assert.equal(conta.empresa.estado, "POR_REGISTAR"); assert.equal(conta.empresa.experiencia_ate, null);
resumo = (await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo;
assert.equal(resumo.clientes.find((c) => c.nif === NIF).estado, "POR_REGISTAR");
// a área do produtor não consegue «apagar» uma experiência a meio para dar outra; numa empresa por registar fica vazia
assert.equal((await pedir("POST", "/admin/api/cliente", { nif: NIF, experiencia_ate: null }, ADMIN)).estado, 200);
assert.equal((await env.DB.prepare("SELECT experiencia_ate FROM clientes WHERE nif = ?").bind(NIF).first()).experiencia_ate, null);

// o mesmo NIF não se cria duas vezes; um email que já tem conta recebe o aviso, e a resposta não o revela
assert.equal((await post("criar", { ...NOVA, email: "outra@papelaria-exemplo.ao" })).estado, 409);
enviados.length = 0;
const semConta = await post("criar", { ...NOVA, nif: "5000000099", email: "alguem@outra-exemplo.ao" });
const comConta = await post("criar", { ...NOVA, nif: "5000000098", email: DONO });
assert.equal(comConta.estado, 200);
assert.equal(comConta.corpo.mensagem.replace(/para .+?\./, ""), semConta.corpo.mensagem.replace(/para .+?\./, ""), "a mesma resposta");
assert.equal(enviados.find((m) => m.to === DONO).subject, "KanGest: este email já tem conta");
assert.equal(await env.DB.prepare("SELECT 1 AS x FROM pedidos_conta WHERE email = ?").bind(DONO).first(), null, "sem pedido: não há código");

// o programa regista-se com o mesmo NIF e email: a experiência começa hoje e chegam as boas-vindas
await env.DB.prepare("UPDATE clientes SET criado_em = '2026-01-05T09:00:00Z' WHERE nif = ?").bind(NIF).run(); // conta criada há tempo
const MAQ = "ABCD-0000-0011";
enviados.length = 0;
await pedir("POST", "/v1/registo", { nif: NIF, empresa: "Papelaria Sol", email: DONO, telefone: "923 000 111", maquina: MAQ, versao: "1.4.1" });
let cod = codigoDe(enviados.find((m) => m.to === DONO));
enviados.length = 0;
r = await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo: cod });
assert.equal(r.corpo.estado, "EXPERIENCIA"); assert.equal(r.corpo.fim, somar(hoje, 29), "30 dias a partir do registo, não da conta");
assert.equal((await env.DB.prepare("SELECT inicio FROM licencas WHERE nif = ?").bind(NIF).first()).inicio, hoje, "a licença começa no registo");
assert.equal(enviados.length, 1); assert.equal(enviados[0].subject, "KanGest: registo de Papelaria Sol confirmado");
conta = (await pedir("GET", "/v1/conta/resumo", null, tokenDona)).corpo;
assert.equal(conta.empresa.estado, "EXPERIENCIA"); assert.equal(conta.empresa.nome, "Papelaria Sol");
// registar outro computador não volta a dar experiência nem boas-vindas
enviados.length = 0;
await pedir("POST", "/v1/registo", { nif: NIF, empresa: "Papelaria Sol", email: DONO, maquina: "ABCD-0000-0012" });
cod = codigoDe(enviados.find((m) => m.to === DONO));
enviados.length = 0;
assert.equal((await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: "ABCD-0000-0012", codigo: cod })).corpo.fim, somar(hoje, 29));
assert.equal(enviados.length, 0);

// alguém cria a conta com o NIF de outra empresa: quando a empresa regista o programa com o seu email, essa conta desfaz-se
const NIF2 = "5000000022";
const ALHEIO = "intruso@exemplo-alheio.ao";
const LOJA = "geral@mercearia-exemplo.ao";
enviados.length = 0;
await post("criar", { empresa: "Mercearia Lua", nif: NIF2, email: ALHEIO });
r = await post("criar/confirmar", { email: ALHEIO, codigo: codigoDe(enviados[0]), senha: "Intruso-2026-xx" });
const tokenAlheio = r.corpo.token;
assert.equal((await pedir("GET", "/v1/conta/resumo", null, tokenAlheio)).estado, 200);
enviados.length = 0;
await pedir("POST", "/v1/registo", { nif: NIF2, empresa: "Mercearia Lua", email: LOJA, maquina: "ABCD-0000-0022" });
cod = codigoDe(enviados.find((m) => m.to === LOJA));
enviados.length = 0;
r = await pedir("POST", "/v1/confirmar", { nif: NIF2, maquina: "ABCD-0000-0022", codigo: cod });
assert.equal(r.corpo.estado, "EXPERIENCIA");
assert.equal(await env.DB.prepare("SELECT 1 AS x FROM contas WHERE nif = ?").bind(NIF2).first(), null, "a conta do outro email desfaz-se");
assert.equal((await pedir("GET", "/v1/conta/resumo", null, tokenAlheio)).estado, 401, "e a sessão dele termina");
assert.equal((await post("entrar", { email: ALHEIO, senha: "Intruso-2026-xx" })).estado, 401);
assert.deepEqual(enviados.map((m) => m.to), [LOJA], "as boas-vindas vão para o email do registo");
assert.equal((await env.DB.prepare("SELECT email FROM clientes WHERE nif = ?").bind(NIF2).first()).email, LOJA);
// quem tem o email do registo cria a conta em «Activar a conta» (o código da conta)
enviados.length = 0;
await post("codigo", { email: LOJA });
assert.equal(enviados.length, 1);

// o código expira e as tentativas acabam
enviados.length = 0;
const TARDE = "tarde@exemplo-tarde.ao";
await post("criar", { empresa: "Loja Tarde", nif: "5000000033", email: TARDE });
const codTarde = codigoDe(enviados[0]);
for (let i = 0; i < 5; i++) await post("criar/confirmar", { email: TARDE, codigo: "111111", senha: "Senha-forte-2026" });
assert.match((await post("criar/confirmar", { email: TARDE, codigo: codTarde, senha: "Senha-forte-2026" })).corpo.erro, /expirou ou não é válido/, "5 tentativas e acabou");
await env.DB.prepare("UPDATE pedidos_conta SET tentativas = 0, expira_em = '2020-01-01T00:00:00Z' WHERE email = ?").bind(TARDE).run();
assert.match((await post("criar/confirmar", { email: TARDE, codigo: codTarde, senha: "Senha-forte-2026" })).corpo.erro, /expirou/);
// os pedidos esquecidos limpam-se no pedido seguinte de qualquer empresa
await post("criar", { empresa: "Loja Outra", nif: "5000000044", email: "outra@exemplo-outra.ao" });
assert.equal(await env.DB.prepare("SELECT 1 AS x FROM pedidos_conta WHERE email = ?").bind(TARDE).first(), null);

// entretanto, o programa registou o NIF: a confirmação recusa e o pedido desfaz-se
enviados.length = 0;
const NIF3 = "5000000055";
await post("criar", { empresa: "Loja Corrida", nif: NIF3, email: "corrida@exemplo-corrida.ao" });
const codCorrida = codigoDe(enviados[0]);
await pedir("POST", "/v1/registo", { nif: NIF3, empresa: "Loja Corrida", email: "corrida@exemplo-corrida.ao", maquina: "ABCD-0000-0055" });
const codRegisto = codigoDe(enviados.find((m) => m.subject.startsWith("KanGest: código de confirmação")));
await pedir("POST", "/v1/confirmar", { nif: NIF3, maquina: "ABCD-0000-0055", codigo: codRegisto });
r = await post("criar/confirmar", { email: "corrida@exemplo-corrida.ao", codigo: codCorrida, senha: "Senha-forte-2026" });
assert.equal(r.estado, 409); assert.match(r.corpo.erro, /acabou de ser registado/);

// sem envio de email, o código fica para o produtor dar (e aparece na área do produtor)
const semEmail = { ...env };
delete env.EMAIL;
r = await post("criar", { empresa: "Loja Sem Email", nif: "5000000066", email: "sem@exemplo-sem.ao" });
env.EMAIL = semEmail.EMAIL;
assert.equal(r.corpo.enviado, false); assert.match(r.corpo.mensagem, /suporte do KanGest/);
resumo = (await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo;
assert.match(resumo.codigos.find((c) => c.email === "sem@exemplo-sem.ao").codigo_aberto, /^\d{6}$/);

console.log("Conta nova no site: todos os testes passaram.");

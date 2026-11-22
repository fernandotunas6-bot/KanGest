// Testes das contas das empresas no site (portal do cliente), sem rede: node servidor-licencas/teste/contas.mjs
// Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker, { equipaParaKangest, guardarEstado } from "../src/worker.js";

const ADMIN = "b".repeat(40);
const env = { DB: d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8")), ADMIN_TOKEN: ADMIN };
const NIF = "5000000001";
const EMAIL = "dono@loja-exemplo.test";

async function pedir(metodo, caminho, corpo, token, cabecalhos = {}) {
  const r = await worker.fetch(
    new Request("https://licencas.teste" + caminho, {
      method: metodo,
      headers: { "content-type": "application/json", "user-agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/131.0 Safari/537.36", ...(token ? { authorization: "Bearer " + token } : {}), ...cabecalhos },
      body: corpo ? JSON.stringify(corpo) : undefined,
    }),
    env,
  );
  const tipo = r.headers.get("content-type") ?? "";
  return { estado: r.status, corpo: tipo.includes("json") ? await r.json() : await r.text(), cabecalhos: r.headers };
}
const post = (c, b, t) => pedir("POST", "/v1/conta/" + c, b, t);
const get = (c, t) => pedir("GET", "/v1/conta/" + c, null, t);
const codigoAberto = async () => (await env.DB.prepare("SELECT codigo_aberto FROM codigos_conta WHERE email = ?").bind(EMAIL).first())?.codigo_aberto;

// uma empresa registada pelo KanGest, com um computador que já mandou o estado
await env.DB.prepare("INSERT INTO clientes (nif, empresa, email, telefone, criado_em, experiencia_ate) VALUES (?, 'Loja Exemplo', ?, '900 000 000', '2026-11-01T10:00:00Z', '2026-12-01')").bind(NIF, EMAIL).run();
await env.DB.prepare("INSERT INTO maquinas (nif, maquina, primeiro_contacto, ultimo_contacto, versao) VALUES (?, 'AAAA-BBBB-CCCC', '2026-11-01T10:00:00Z', '2026-11-22T10:00:00Z', '1.4.0')").bind(NIF).run();
await env.DB.prepare("INSERT INTO instalacoes (nif, maquina, recebido_em, versao, modo, licenca_estado, licenca_fim, copia_ultima, copia_externa_erro) VALUES (?, 'AAAA-BBBB-CCCC', '2026-11-22T10:00:00Z', '1.4.0', 'loja', 'EXPERIENCIA', '2026-12-01', '2026-11-22T09:00:00', 0)").bind(NIF).run();

// pedir o código: a mesma resposta para quem existe e quem não existe
const r1 = await post("codigo", { email: "ninguem@exemplo.test" });
const r2 = await post("codigo", { email: " DONO@loja-exemplo.test " });
assert.equal(r1.estado, 200); assert.equal(r1.corpo.mensagem, r2.corpo.mensagem, "não revela que emails existem");
assert.equal((await post("codigo", { email: "isto-nao-e-email" })).estado, 400);
const codigo = await codigoAberto();
assert.match(codigo, /^\d{6}$/, "sem envio de email, o código fica para o produtor dar");

// o produtor vê-o no painel, em «Pedidos de registo»
const resumoProdutor = await pedir("GET", "/admin/api/resumo", null, ADMIN);
assert.ok(resumoProdutor.corpo.codigos.some((c) => c.tipo === "conta" && c.codigo_aberto === codigo && c.empresa === "Loja Exemplo"));

// definir a senha: código errado, senha fraca, depois certo
assert.equal((await post("definir-senha", { email: EMAIL, codigo: "000000", senha: "Senha-forte-2026" })).corpo.erro, "Código errado.");
assert.match((await post("definir-senha", { email: EMAIL, codigo, senha: "curta1" })).corpo.erro, /entre 10 e 128/);
assert.match((await post("definir-senha", { email: EMAIL, codigo, senha: "semalgarismos" })).corpo.erro, /letras e algarismos/);
assert.equal((await post("definir-senha", { email: EMAIL, codigo, senha: "Senha-forte-2026" })).estado, 200);
assert.equal((await post("definir-senha", { email: EMAIL, codigo, senha: "Outra-senha-2026" })).estado, 400, "o código só serve uma vez");
const guardada = (await env.DB.prepare("SELECT senha FROM contas WHERE nif = ?").bind(NIF).first()).senha;
assert.match(guardada, /^pbkdf2-sha256\$100000\$/); assert.ok(!guardada.includes("Senha-forte"), "só o resumo");

// entrar
assert.equal((await post("entrar", { email: EMAIL, senha: "errada-2026x" })).estado, 401);
assert.equal((await post("entrar", { email: "ninguem@exemplo.test", senha: "Senha-forte-2026" })).estado, 401);
const e1 = await post("entrar", { email: EMAIL, senha: "Senha-forte-2026" });
assert.equal(e1.estado, 200); assert.match(e1.corpo.token, /^c1\./);
const t1 = e1.corpo.token;

// o que a conta mostra
const res = (await get("resumo", t1)).corpo;
assert.equal(res.empresa.nome, "Loja Exemplo"); assert.equal(res.empresa.estado, "EXPERIENCIA");
assert.equal(res.maquinas[0].licenca_estado, "EXPERIENCIA"); assert.equal(res.maquinas[0].copia_ultima, "2026-11-22T09:00:00");
assert.equal((await get("resumo")).estado, 401, "sem sessão, nada");
assert.equal((await get("resumo", ADMIN)).estado, 401, "a chave do produtor não abre contas");
const [v, dados, ass] = t1.split(".");
const outra = `${v}.${Buffer.from(JSON.stringify({ n: "5000000002", exp: Date.now() + 9e9, s: "x" })).toString("base64url")}.${ass}`;
assert.equal((await get("resumo", outra)).estado, 401, "não se troca de empresa mexendo no token");

// a equipa que o KanGest comunicou: só os campos conhecidos (o que viesse a mais não se guarda)
assert.equal(res.equipa, null, "antes de o KanGest comunicar, não há equipa");
await guardarEstado(env, NIF, "AAAA-BBBB-CCCC", { versao: "1.4.0", estado: { equipa: {
  utilizadores: [
    { login: "admin", nome: "Administrador", perfil: "ADMIN", ativo: true, loja: null, dois_passos: true, ultimo_acesso: "2026-11-22 09:10:00", senha_hash: "$argon2id$segredo", pin: "1234" },
    { login: "caixa1", nome: "Operador de Caixa", perfil: "CAIXA", ativo: false, loja: "Loja Centro", dois_passos: false, ultimo_acesso: null, email: "x@y.test" },
  ],
  perfis: [{ codigo: "ADMIN", descricao: "Administrador" }, { codigo: "CAIXA", descricao: "Caixa" }],
  lojas: [{ nome: "Loja Centro", ativo: true }],
  postos: [{ descricao: "Caixa 1", loja: "Loja Centro" }],
} } });
const eq = (await get("resumo", t1)).corpo.equipa;
assert.equal(eq.utilizadores.length, 2); assert.equal(eq.utilizadores[0].ultimo_acesso, "2026-11-22T09:10:00");
assert.equal(eq.utilizadores[1].ativo, false); assert.equal(eq.postos[0].loja, "Loja Centro"); assert.equal(eq.maquina, "AAAA-BBBB-CCCC");
const textoEq = JSON.stringify(eq);
assert.ok(!textoEq.includes("argon") && !textoEq.includes("1234") && !textoEq.includes("x@y.test"), "senhas, PIN e emails nunca se guardam");

// perfil
assert.equal((await post("perfil", { telefone: "923 111 222" }, t1)).estado, 200);
assert.equal((await get("resumo", t1)).corpo.empresa.telefone, "923 111 222");
assert.equal((await post("perfil", { telefone: "<script>" }, t1)).estado, 400);

// onde fez login e sair em todos
const t2 = (await post("entrar", { email: EMAIL, senha: "Senha-forte-2026" })).corpo.token;
let ses = (await get("sessoes", t2)).corpo.sessoes;
assert.equal(ses.length, 2); assert.equal(ses[0].actual, true); assert.equal(ses[0].aparelho, "Chrome (Mac OS X)");
assert.ok(ses.every((x) => !("ip" in x)));
assert.equal((await post("sessoes/terminar-outras", {}, t2)).corpo.terminadas, 1);
assert.equal((await get("resumo", t1)).estado, 401, "a outra sessão terminou");

// mudar a senha: precisa da actual; termina as outras sessões
const t3 = (await post("entrar", { email: EMAIL, senha: "Senha-forte-2026" })).corpo.token;
assert.equal((await post("senha", { actual: "errada", nova: "Nova-senha-2027" }, t2)).estado, 400);
assert.equal((await post("senha", { actual: "Senha-forte-2026", nova: "Nova-senha-2027" }, t2)).estado, 200);
assert.equal((await get("resumo", t3)).estado, 401, "mudar a senha termina as outras sessões");
assert.equal((await get("resumo", t2)).estado, 200, "esta continua");
assert.equal((await post("entrar", { email: EMAIL, senha: "Senha-forte-2026" })).estado, 401, "a senha antiga já não entra");

// equipa gerida no site (fase 3b)
{
  const ap = (c, b) => post("equipa/" + c, b, t2);
  assert.equal(await equipaParaKangest(env, NIF, "AAAA-BBBB-CCCC", 0), null, "sem nada gerido, nada a aplicar");
  assert.match((await ap("funcionario", { login: "maria silva", nome: "Maria Caixa", perfil: "CAIXA" })).corpo.erro, /letras minúsculas/, "login com espaço");
  assert.match((await ap("funcionario", { login: "maria", nome: "Maria Caixa", perfil: "DONO" })).corpo.erro, /Perfil desconhecido/);
  assert.match((await ap("funcionario", { login: "maria", nome: "Maria Caixa", perfil: "CAIXA", loja: "Loja Nova" })).corpo.erro, /Loja desconhecida/);
  assert.equal((await ap("loja", { nome: "Loja Nova" })).estado, 200);
  const nova = await ap("funcionario", { login: "maria", nome: "Maria Caixa", perfil: "CAIXA", loja: "Loja Nova" });
  assert.equal(nova.estado, 200); assert.match(nova.corpo.senha_provisoria, /^[A-Za-z2-9]{4}-[A-Za-z2-9]{4}-[A-Za-z2-9]{4}$/);
  const adoptado = await ap("funcionario", { login: "admin", nome: "Administrador", perfil: "ADMIN" });
  assert.equal(adoptado.corpo.senha_provisoria, null, "quem já existe no programa mantém a senha");
  assert.match((await ap("funcionario", { login: "maria", nome: "Outra", perfil: "CAIXA" })).corpo.erro, /Já há um funcionário/);
  assert.equal((await ap("posto", { descricao: "Caixa 3", loja: "Loja Nova" })).estado, 200);
  assert.match((await ap("posto", { descricao: "Caixa 4" })).corpo.erro, /Escolha a loja/);
  const g = (await get("equipa", t2)).corpo;
  assert.equal(g.funcionarios.length, 2); assert.equal(g.versao.versao, 4); assert.equal(g.versao.aplicada, 0);
  assert.equal(g.funcionarios.find((f) => f.login === "maria").senha_pendente, true);
  assert.ok(!JSON.stringify(g).includes("pbkdf2"), "o painel não recebe os resumos");
  // a verificação do KanGest: recebe a equipa enquanto estiver atrás; o resumo da senha provisória vai, a senha não
  const paraK = await equipaParaKangest(env, NIF, "AAAA-BBBB-CCCC", 0);
  assert.equal(paraK.versao, 4); assert.equal(paraK.lojas[0].nome, "Loja Nova"); assert.equal(paraK.postos[0].loja, "Loja Nova");
  const maria = paraK.funcionarios.find((f) => f.login === "maria");
  assert.match(maria.senha_provisoria, /^pbkdf2-sha256\$100000\$/); assert.ok(!JSON.stringify(paraK).includes(nova.corpo.senha_provisoria));
  assert.equal(paraK.funcionarios.find((f) => f.login === "admin").senha_provisoria, null);
  // aplicada: fica registado e os resumos apagam-se
  assert.equal(await equipaParaKangest(env, NIF, "AAAA-BBBB-CCCC", 4), null);
  const depois = (await get("equipa", t2)).corpo;
  assert.equal(depois.versao.aplicada, 4); assert.equal(depois.versao.aplicada_maquina, "AAAA-BBBB-CCCC");
  assert.equal(depois.funcionarios.find((f) => f.login === "maria").senha_pendente, false);
  // repor a senha: nova provisória, nova versão
  const reposta = await ap("repor-senha", { id: depois.funcionarios.find((f) => f.login === "maria").id });
  assert.match(reposta.corpo.senha_provisoria, /-/);
  assert.equal((await equipaParaKangest(env, NIF, "AAAA-BBBB-CCCC", 4)).versao, 5);
  // outra empresa não vê nem mexe
  assert.equal((await get("equipa")).estado, 401);
}

// sair
await post("sair", {}, t2);
assert.equal((await get("resumo", t2)).estado, 401);

// do site: CORS só para as origens autorizadas
const pre = await worker.fetch(new Request("https://licencas.teste/v1/conta/resumo", { method: "OPTIONS", headers: { origin: "https://kangest.pages.dev" } }), env);
assert.equal(pre.status, 204); assert.match(pre.headers.get("access-control-allow-headers"), /authorization/);
const fora = await worker.fetch(new Request("https://licencas.teste/v1/conta/resumo", { method: "OPTIONS", headers: { origin: "https://intruso.example" } }), env);
assert.equal(fora.status, 403);

// limite de pedidos por IP: conta por hora na tabela «limites» e recusa acima do limite
{
  const comIp = () => pedir("POST", "/v1/conta/codigo", { email: "ninguem@exemplo.test" }, null, { "cf-connecting-ip": "192.0.2.10" });
  assert.equal((await comIp()).estado, 200); assert.equal((await comIp()).estado, 200);
  assert.equal((await env.DB.prepare("SELECT n FROM limites WHERE chave = ?").bind("conta:192.0.2.10").first()).n, 2);
  await env.DB.prepare("UPDATE limites SET janela = ? WHERE chave = ?").bind("2000-01-01T00", "conta:192.0.2.10").run();
  assert.equal((await comIp()).estado, 200, "numa hora nova, a contagem recomeça");
  assert.equal((await env.DB.prepare("SELECT n FROM limites WHERE chave = ?").bind("conta:192.0.2.10").first()).n, 1);
  await env.DB.prepare("UPDATE limites SET n = ? WHERE chave = ?").bind(1000, "conta:192.0.2.10").run();
  assert.equal((await comIp()).estado, 429);
}

console.log("Contas das empresas: todos os testes passaram.");

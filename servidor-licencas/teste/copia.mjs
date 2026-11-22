// A cópia de segurança da D1 (copia-d1.mjs), sem rede: node servidor-licencas/teste/copia.mjs
// Uma D1 imitada com dados feitos pelo próprio servidor e textos difíceis → cópia pela API da Cloudflare imitada →
// carregada noutra D1 → as mesmas linhas, carregar duas vezes não duplica, e o servidor continua a trabalhar sobre ela.
// Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker from "../src/worker.js";
import { POR_PAGINA, copiarD1, ensaiar, ligarD1 } from "../copia-d1.mjs";

const SCHEMA = readFileSync(new URL("../schema.sql", import.meta.url), "utf8");
const d1 = d1Local(SCHEMA);
const ADMIN = "d".repeat(40);
const CHAVE = generateKeyPairSync("ed25519").privateKey.export({ type: "pkcs8", format: "pem" });
const pedir = async (DB, metodo, caminho, corpo, token) => {
  const r = await worker.fetch(
    new Request("https://licencas.teste" + caminho, {
      method: metodo,
      headers: { "content-type": "application/json", ...(token ? { authorization: "Bearer " + token } : {}) },
      body: corpo ? JSON.stringify(corpo) : undefined,
    }),
    { DB, CHAVE_PRIVADA: CHAVE, ADMIN_TOKEN: ADMIN },
  );
  return { estado: r.status, corpo: await r.json() };
};

// dados feitos pelo servidor: registo, confirmação, plano com preço, pagamento, um pedido pendente
const NIF = "5000000005";
const MAQ = "ABCD-1234-EF59";
await pedir(d1, "POST", "/v1/registo", { nif: NIF, empresa: "Loja «Aspas» d'Ouro", email: "copia@exemplo.test", telefone: "900 000 002", maquina: MAQ, versao: "1.4.1" });
const codigo = (await d1.prepare("SELECT codigo_aberto FROM codigos WHERE nif = ?").bind(NIF).first()).codigo_aberto;
const conf = (await pedir(d1, "POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo })).corpo;
assert.equal(conf.estado, "EXPERIENCIA");
assert.equal((await pedir(d1, "POST", "/admin/api/plano", { codigo: "COMERCIO", nome: "Comércio", postos: 3, preco_mes: 15000, preco_ano: 150000, ordem: 2 }, ADMIN)).estado, 200);
const pag = await pedir(d1, "POST", "/admin/api/pagamento", { nif: NIF, plano: "COMERCIO", meses: 12, valor: 150000.25, metodo: "TRANSFERENCIA", referencia: "TRF-9" }, ADMIN);
assert.equal(pag.estado, 200);
await pedir(d1, "POST", "/v1/registo", { nif: "5000000006", empresa: "Pedido Pendente", email: "pendente@exemplo.test", maquina: "ABCD-1234-EF60" });
// textos difíceis (plicas, aspas, quebras de linha, acentos, emoji), JSON, NULL e mais linhas do que uma página
const dificil = "Ana d'Almeida \"Caixa\"\nLinha 2 — ção 🙂; DROP TABLE clientes; --";
await d1.prepare("INSERT INTO equipas (nif, maquina, recebido_em, dados) VALUES (?, ?, ?, ?)").bind(NIF, MAQ, "2026-11-22T10:00:00Z", JSON.stringify({ utilizadores: [{ login: "ana", nome: dificil }] })).run();
await d1.prepare("INSERT INTO funcionarios (nif, id, login, nome, perfil, loja, ativo, senha_provisoria, criado_em, actualizado_em) VALUES (?, 'F1', 'ana', ?, 'CAIXA', NULL, 0, NULL, ?, ?)").bind(NIF, dificil, "2026-11-22T10:00:00Z", "2026-11-22T10:00:00Z").run();
for (let i = 0; i < POR_PAGINA + 37; i++) {
  await d1.prepare("INSERT INTO sessoes_painel (id, email, criada_em, ultima_em, aparelho, local) VALUES (?, 'produtor@exemplo.test', ?, ?, 'Chrome (Mac OS X)', NULL)").bind(`S${i}`, "2026-11-22T10:00:00Z", "2026-11-22T10:00:00Z").run();
}
await d1.prepare("INSERT INTO limites (chave, janela, n) VALUES ('registo:192.0.2.1', '2026-11-22T10', 3)").run();

// a API da Cloudflare, imitada sobre a D1 de cima (só leituras)
const consultas = [];
const buscar = async (url, init) => {
  const u = new URL(url);
  if (init.headers.authorization !== "Bearer token-cf") return Response.json({ success: false, errors: [{ message: "Authentication error" }] }, { status: 403 });
  if (u.pathname === "/client/v4/accounts") return Response.json({ success: true, result: [{ id: "conta1" }] });
  if (u.pathname === "/client/v4/accounts/conta1/d1/database") return Response.json({ success: true, result: [{ uuid: "u1", name: u.searchParams.get("name") }] });
  if (u.pathname === "/client/v4/accounts/conta1/d1/database/u1/query") {
    const { sql, params } = JSON.parse(init.body);
    assert.match(sql, /^SELECT /, "a cópia só lê");
    consultas.push(sql);
    return Response.json({ success: true, result: [{ results: (await d1.prepare(sql).bind(...params).all()).results, success: true }] });
  }
  return Response.json({ success: false, errors: [{ message: "Não encontrado" }] }, { status: 404 });
};
await assert.rejects(ligarD1({ token: "errado", buscar }), /Authentication error/);
const ligacao = await ligarD1({ token: "token-cf", buscar });
assert.deepEqual([ligacao.conta, ligacao.base], ["conta1", "u1"], "a conta vem do token");
const { sql, contagens } = await copiarD1(ligacao);
assert.equal(contagens.sessoes_painel, POR_PAGINA + 37, "lê página a página");
assert.ok(!("limites" in contagens), "o limite por IP não entra");
assert.ok(!/CREATE|DROP TABLE clientes;\n/.test(sql.replace(dificil.replace(/'/g, "''"), "")), "só dados, nenhum esquema");

// carregada noutra D1: as mesmas linhas; duas vezes, o mesmo
const outra = d1Local(SCHEMA);
await outra.exec(sql);
await outra.exec(sql);
const tabelas = (await d1.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> 'limites'").all()).results.map((t) => t.name);
for (const t of tabelas) {
  const a = (await d1.prepare(`SELECT * FROM ${t}`).all()).results.map((l) => JSON.stringify(l)).sort();
  const b = (await outra.prepare(`SELECT * FROM ${t}`).all()).results.map((l) => JSON.stringify(l)).sort();
  assert.deepEqual(b, a, t);
}
assert.equal(JSON.parse((await outra.prepare("SELECT dados FROM equipas").first()).dados).utilizadores[0].nome, dificil);

// o servidor continua a trabalhar sobre a cópia: a mesma licença na renovação, os pagamentos continuam a numeração
const { clientes, DB } = await ensaiar(sql);
assert.equal(clientes, 1, "o pedido pendente ainda não é cliente");
assert.equal((await DB.prepare("SELECT empresa FROM codigos WHERE nif = '5000000006'").first())?.empresa, "Pedido Pendente", "e veio na cópia");
const ren = (await pedir(DB, "POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.1" })).corpo;
assert.equal(ren.estado, "ACTIVA"); assert.equal(ren.id, pag.corpo.licencas[0].id, "o segredo do computador e os pagamentos vieram");
assert.equal((await pedir(DB, "POST", "/admin/api/pagamento", { nif: NIF, plano: "COMERCIO", meses: 1, valor: 15000, metodo: "MULTICAIXA" }, ADMIN)).estado, 200);
assert.deepEqual((await DB.prepare("SELECT id FROM pagamentos ORDER BY id").all()).results.map((l) => l.id), [1, 2]);

console.log("Cópia da D1: todos os testes passaram.");

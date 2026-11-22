// A cópia dos dados da D1 para o Postgres (fase 4), sem rede: node servidor-central/teste/copia.mjs (depois de «npm ci»)
// Uma D1 imitada (SQLite com o schema.sql), com dados feitos pelo próprio servidor e casos difíceis → exportar pela API
// da Cloudflare imitada → importar no Postgres local → as mesmas linhas, e o servidor continua a trabalhar sobre elas.
// Os dados são de exemplo (empresa, NIF e emails fictícios).
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "../../servidor-licencas/teste/d1-local.mjs";
import worker from "../../servidor-licencas/src/worker.js";
import { POR_PAGINA, exportarD1, importar, lerCopia, verificarComServidor } from "../copiar-d1.mjs";
import { ESQUEMAS, d1SobrePostgresJs } from "../supabase/functions/kangest/d1.js";
import { postgresLocal } from "./pglite.mjs";

const d1 = d1Local(readFileSync(new URL("../../servidor-licencas/schema.sql", import.meta.url), "utf8"));
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

// ------------------------------------------------------------------ a D1 com dados

const NIF = "5000000005";
const MAQ = "ABCD-1234-EF59";
await pedir(d1, "POST", "/v1/registo", { nif: NIF, empresa: "Loja «Aspas» d'Ouro", email: "copia@exemplo.test", telefone: "900 000 002", maquina: MAQ, versao: "1.4.0" });
const codigo = (await d1.prepare("SELECT codigo_aberto FROM codigos WHERE nif = ?").bind(NIF).first()).codigo_aberto;
const conf = (await pedir(d1, "POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo })).corpo;
assert.equal(conf.estado, "EXPERIENCIA");
assert.equal((await pedir(d1, "POST", "/admin/api/plano", { codigo: "COMERCIO", nome: "Comércio", postos: 3, preco_mes: 15000, preco_ano: 150000, ordem: 2 }, ADMIN)).estado, 200);
const pag = await pedir(d1, "POST", "/admin/api/pagamento", { nif: NIF, plano: "COMERCIO", meses: 12, valor: 150000.25, metodo: "TRANSFERENCIA", referencia: "TRF-9" }, ADMIN);
assert.equal(pag.estado, 200);
await pedir(d1, "POST", "/v1/registo", { nif: "5000000006", empresa: "Pedido Pendente", email: "pendente@exemplo.test", maquina: "ABCD-1234-EF60" });
// casos difíceis: plicas, aspas, quebras de linha, acentos e emoji; JSON; NULL; mais linhas do que uma página
const difícil = "Ana d'Almeida \"Caixa\"\nLinha 2 — ção 🙂";
await d1.prepare("INSERT INTO equipas (nif, maquina, recebido_em, dados) VALUES (?, ?, ?, ?)").bind(NIF, MAQ, "2026-11-22T10:00:00Z", JSON.stringify({ utilizadores: [{ login: "ana", nome: difícil }] })).run();
await d1.prepare("INSERT INTO funcionarios (nif, id, login, nome, perfil, loja, ativo, senha_provisoria, criado_em, actualizado_em) VALUES (?, 'F1', 'ana', ?, 'CAIXA', NULL, 0, NULL, ?, ?)").bind(NIF, difícil, "2026-11-22T10:00:00Z", "2026-11-22T10:00:00Z").run();
await d1.prepare("INSERT INTO equipa_versoes (nif, versao, actualizado_em, aplicada) VALUES (?, 7, ?, 5)").bind(NIF, "2026-11-22T10:00:00Z").run();
await d1.prepare("INSERT INTO subdominios (nif, nome, maquina, tunel_id, dns_id, porta, criado_em, actualizado_em) VALUES (?, 'loja-ouro', ?, 't1', NULL, 7799, 'x', 'x')").bind(NIF, MAQ).run();
for (let i = 0; i < POR_PAGINA + 37; i++) {
  await d1.prepare("INSERT INTO limites (chave, janela, n) VALUES (?, ?, ?)").bind(`renovar:10.0.${i >> 8}.${i & 255}`, "2026-11-22T10", i).run();
}

// ------------------------------------------------------------------ exportar pela API da Cloudflare (imitada)

const consultas = [];
const buscar = async (url, init) => {
  const u = new URL(url);
  if (init.headers.authorization !== "Bearer token-cf") return Response.json({ success: false, errors: [{ message: "Authentication error" }] }, { status: 403 });
  if (init.method === "GET" && u.pathname === "/client/v4/accounts/conta1/d1/database") {
    return Response.json({ success: true, result: [{ uuid: "u1", name: u.searchParams.get("name") }] });
  }
  if (init.method === "POST" && u.pathname === "/client/v4/accounts/conta1/d1/database/u1/query") {
    const { sql, params } = JSON.parse(init.body);
    consultas.push(sql);
    return Response.json({ success: true, result: [{ results: (await d1.prepare(sql).bind(...params).all()).results, success: true }] });
  }
  return Response.json({ success: false, errors: [{ message: "Não encontrado" }] }, { status: 404 });
};
await assert.rejects(exportarD1({ token: "errado", conta: "conta1", buscar }), /Authentication error/);
await assert.rejects(exportarD1({ token: "token-cf", conta: "conta1", nome: "outra", buscar: async (u, i) => (u.includes("?name=") ? Response.json({ success: true, result: [] }) : buscar(u, i)) }), /Não há nenhuma base D1 «outra»/);
const exportada = await exportarD1({ token: "token-cf", conta: "conta1", buscar });
assert.equal(exportada.tabelas.limites.length, POR_PAGINA + 37, "lê página a página");
assert.ok(consultas.some((s) => s.includes(`OFFSET ${POR_PAGINA}`)));
assert.deepEqual(Object.keys(exportada.tabelas).sort(), Object.values(ESQUEMAS).flat().sort(), "todas as tabelas, nenhuma interna");
const copia = await lerCopia(JSON.stringify(exportada), "d1.json"); // como fica no ficheiro

// a mesma cópia pelo .sql do «wrangler d1 export»: lida pelo próprio SQLite
const literal = (v) => (v === null ? "NULL" : typeof v === "number" ? String(v) : `'${String(v).replace(/'/g, "''")}'`);
const despejo = ["PRAGMA defer_foreign_keys=TRUE;"];
for (const { name, sql } of (await d1.prepare("SELECT name, sql FROM sqlite_master WHERE type = 'table' ORDER BY name").all()).results) {
  despejo.push(`${sql};`);
  for (const l of (await d1.prepare(`SELECT * FROM "${name}" ORDER BY rowid`).all()).results) {
    despejo.push(`INSERT INTO "${name}" (${Object.keys(l).map((c) => `"${c}"`).join(",")}) VALUES(${Object.values(l).map(literal).join(",")});`);
  }
}
assert.deepEqual((await lerCopia(despejo.join("\n"), "d1.sql")).tabelas, copia.tabelas, "o .sql dá as mesmas linhas");
assert.throws(() => JSON.parse("{"));
await assert.rejects(lerCopia(JSON.stringify({ outra: 1 }), "x.json"), /não é uma cópia da D1/);

// ------------------------------------------------------------------ importar no Postgres

const { sql, fechar } = await postgresLocal();
const rel = await importar(sql, copia);
for (const l of rel.tabelas) if (l.tabela !== "empresas.planos") assert.equal(l.destino, l.origem, l.tabela);
// as mesmas linhas, valor a valor
for (const [esquema, tabelas] of Object.entries(ESQUEMAS)) {
  for (const t of tabelas) {
    const naD1 = (await d1.prepare(`SELECT * FROM ${t}`).all()).results.map((l) => JSON.stringify(l)).sort();
    const noPg = (await sql.unsafe(`SELECT * FROM ${esquema}.${t}`)).map((l) => JSON.stringify(l)).sort();
    assert.deepEqual(noPg, naD1, t);
  }
}
const [{ dados }] = await sql.unsafe("SELECT dados FROM equipa.equipas");
assert.equal(JSON.parse(dados).utilizadores[0].nome, difícil);

// o servidor continua a trabalhar sobre os dados copiados
const DB = d1SobrePostgresJs(sql);
const ren = (await pedir(DB, "POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.1" })).corpo;
assert.equal(ren.estado, "ACTIVA"); assert.equal(ren.id, pag.corpo.licencas[0].id, "a mesma licença: o segredo do computador e os pagamentos vieram");
assert.equal((await pedir(DB, "POST", "/admin/api/pagamento", { nif: NIF, plano: "COMERCIO", meses: 1, valor: 15000, metodo: "MULTICAIXA" }, ADMIN)).estado, 200);
assert.deepEqual((await sql.unsafe("SELECT id FROM licencas.pagamentos ORDER BY id")).map((l) => l.id), [1, 2], "os pagamentos continuam a numeração");
assert.equal((await pedir(DB, "GET", "/v1/planos")).corpo.planos.find((p) => p.codigo === "COMERCIO").preco_mes, 15000, "o preço da D1");
assert.equal((await verificarComServidor(sql)).clientes, 1);

// copiar outra vez: só com --substituir; e um erro desfaz tudo
await assert.rejects(importar(sql, copia), /já tem dados em .*--substituir/);
const comColunaVelha = structuredClone(copia);
comColunaVelha.tabelas.clientes[0].coluna_velha = "x";
await assert.rejects(importar(sql, comColunaVelha, { substituir: true }), /clientes: a cópia tem colunas que o Postgres não tem \(coluna_velha\)/);
assert.equal((await sql.unsafe("SELECT count(*) AS n FROM licencas.pagamentos"))[0].n, 2, "nada se perdeu com o erro");
const semEquipas = structuredClone(copia);
delete semEquipas.tabelas.equipas;
semEquipas.tabelas.d1_migrations = [{ id: 1 }];
const rel2 = await importar(sql, semEquipas, { substituir: true });
assert.deepEqual(rel2.ignoradas, ["d1_migrations"]);
assert.deepEqual(rel2.tabelas.find((l) => l.tabela === "equipa.equipas"), { tabela: "equipa.equipas", origem: null, destino: 0 });
assert.deepEqual((await sql.unsafe("SELECT id FROM licencas.pagamentos ORDER BY id")).map((l) => l.id), [1], "substituir recomeça do que a D1 tem");

await fechar();
console.log("Cópia da D1 para o Postgres: todos os testes passaram.");

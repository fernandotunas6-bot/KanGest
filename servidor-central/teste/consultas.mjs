// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// O mesmo SQL nos dois motores, sem rede: node servidor-central/teste/consultas.mjs (depois de «npm ci» em servidor-central)
// - cada consulta do worker.js compila no SQLite (schema.sql, como o D1) e no Postgres (migrações do Supabase, depois do
//   adaptador d1.js): apanha SQL que só um deles percebe, tabelas e colunas que faltam, tipos que não batem;
// - as tabelas e as colunas são as mesmas no schema.sql, nas migrações e em ESQUEMAS do d1.js;
// - a conversão do adaptador (marcadores e esquemas).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { ESQUEMAS, paraPostgres } from "../supabase/functions/kangest/d1.js";
import { pgliteComMigracoes } from "./pglite.mjs";

const raiz = new URL("../../servidor-licencas/", import.meta.url);
const falhas = [];

// ------------------------------------------------------------------ a conversão do adaptador

assert.equal(paraPostgres("SELECT * FROM clientes WHERE nif = ? AND email = ?"), "SELECT * FROM empresas.clientes WHERE nif = $1 AND email = $2");
assert.equal(paraPostgres("INSERT INTO limites (chave) VALUES (?1) ON CONFLICT(chave) DO UPDATE SET janela = ?2"), "INSERT INTO seguranca.limites (chave) VALUES ($1) ON CONFLICT(chave) DO UPDATE SET janela = $2");
assert.equal(paraPostgres("SELECT ?2, ?, ?1, ?"), "SELECT $2, $3, $1, $4", "«?» depois de «?N» segue a regra do SQLite");
assert.equal(paraPostgres("SELECT 'FROM clientes ?' AS x FROM codigos_conta k JOIN codigos c ON 1 = 1 WHERE a = ?"), "SELECT 'FROM clientes ?' AS x FROM seguranca.codigos_conta k JOIN licencas.codigos c ON 1 = 1 WHERE a = $1", "cadeias ficam como estão");
assert.equal(paraPostgres("UPDATE licencas SET fim = ? WHERE id IN (SELECT id FROM licencas.licencas)"), "UPDATE licencas.licencas SET fim = $1 WHERE id IN (SELECT id FROM licencas.licencas)", "não repete o esquema");
assert.equal(paraPostgres("DELETE FROM equipa_versoes; DELETE FROM equipas"), "DELETE FROM equipa.equipa_versoes; DELETE FROM equipa.equipas");

// ------------------------------------------------------------------ as consultas do worker.js

/** Cada `.prepare("…")` do worker.js, com a linha onde está. */
function consultasDo(fonte) {
  const lista = [];
  for (let i = fonte.indexOf(".prepare("); i >= 0; i = fonte.indexOf(".prepare(", i + 1)) {
    let j = i + ".prepare(".length;
    while (/\s/.test(fonte[j])) j++;
    const linha = fonte.slice(0, i).split("\n").length;
    const aspa = fonte[j];
    if (!["`", '"', "'"].includes(aspa)) {
      falhas.push(`worker.js:${linha}: a consulta não é um texto fixo (não se pode verificar)`);
      continue;
    }
    let k = j + 1;
    while (fonte[k] !== aspa) {
      if (fonte[k] === "\\") k++;
      else if (aspa === "`" && fonte[k] === "$" && fonte[k + 1] === "{") falhas.push(`worker.js:${linha}: a consulta tem \${…} (não se pode verificar)`);
      k++;
    }
    lista.push({ linha, sql: fonte.slice(j + 1, k) });
  }
  return lista;
}

const schema = readFileSync(new URL("schema.sql", raiz), "utf8");
const consultas = consultasDo(readFileSync(new URL("src/worker.js", raiz), "utf8"));
assert.ok(consultas.length > 80, `só ${consultas.length} consultas encontradas no worker.js`);

const TIPOS_ESTRANHOS = { 16: "booleano", 1082: "data", 1114: "data e hora", 1184: "data e hora", 114: "JSON", 3802: "JSON" };
const lite = new DatabaseSync(":memory:");
lite.exec(schema);
const pg = await pgliteComMigracoes();

for (const { linha, sql } of consultas) {
  try {
    lite.prepare(sql);
  } catch (e) {
    falhas.push(`worker.js:${linha} no SQLite: ${e.message}\n    ${sql.replace(/\s+/g, " ").trim()}`);
  }
  try {
    const { resultFields } = await pg.describeQuery(paraPostgres(sql));
    // o D1 devolve números e texto: um booleano, uma data ou um JSON do Postgres chegavam ao worker.js de outra forma
    for (const c of resultFields ?? []) {
      if (TIPOS_ESTRANHOS[c.dataTypeID]) falhas.push(`worker.js:${linha} no Postgres: a coluna «${c.name}» vem como ${TIPOS_ESTRANHOS[c.dataTypeID]} (o D1 dá números ou texto)`);
    }
  } catch (e) {
    falhas.push(`worker.js:${linha} no Postgres: ${e.message}\n    ${paraPostgres(sql).replace(/\s+/g, " ").trim()}`);
  }
}

// ------------------------------------------------------------------ as mesmas tabelas e colunas nos três sítios

const TIPOS = { INTEGER: ["integer", "bigint"], TEXT: ["text"] };
const naD1 = new Map(
  lite
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
    .all()
    .map(({ name }) => [name, lite.prepare(`PRAGMA table_info(${name})`).all()]),
);
const colunasPg = (
  await pg.query(
    `SELECT table_schema AS esquema, table_name AS tabela, column_name AS nome, data_type AS tipo, is_nullable = 'NO' AS obrigatoria
       FROM information_schema.columns WHERE table_schema = ANY($1) ORDER BY table_schema, table_name, ordinal_position`,
    [Object.keys(ESQUEMAS)],
  )
).rows;
const noPostgres = new Map();
for (const c of colunasPg) noPostgres.set(c.tabela, [...(noPostgres.get(c.tabela) ?? []), c]);
const noAdaptador = new Map(Object.entries(ESQUEMAS).flatMap(([e, ts]) => ts.map((t) => [t, e])));

for (const t of new Set([...naD1.keys(), ...noPostgres.keys(), ...noAdaptador.keys()])) {
  const d1 = naD1.get(t);
  const pgc = noPostgres.get(t);
  if (!d1) falhas.push(`tabela ${t}: falta no schema.sql`);
  if (!pgc) falhas.push(`tabela ${t}: falta nas migrações do Supabase`);
  if (!noAdaptador.has(t)) falhas.push(`tabela ${t}: falta em ESQUEMAS (d1.js)`);
  if (!d1 || !pgc) continue;
  if (noAdaptador.has(t) && pgc[0].esquema !== noAdaptador.get(t)) falhas.push(`tabela ${t}: no esquema ${pgc[0].esquema}, mas d1.js diz ${noAdaptador.get(t)}`);
  const nomesD1 = d1.map((c) => c.name).join(", ");
  const nomesPg = pgc.map((c) => c.nome).join(", ");
  if (nomesD1 !== nomesPg) {
    falhas.push(`tabela ${t}: colunas diferentes\n    D1:       ${nomesD1}\n    Postgres: ${nomesPg}`);
    continue;
  }
  d1.forEach((c, i) => {
    const p = pgc[i];
    if (!TIPOS[c.type]?.includes(p.tipo)) falhas.push(`${t}.${c.name}: ${c.type} na D1, ${p.tipo} no Postgres`);
    // as chaves primárias são sempre obrigatórias no Postgres; as outras colunas têm de bater
    if (!c.pk && (c.notnull === 1) !== p.obrigatoria) falhas.push(`${t}.${c.name}: NOT NULL na D1 ${c.notnull === 1}, no Postgres ${p.obrigatoria}`);
  });
}

await pg.close();
if (falhas.length) {
  console.error(`${falhas.length} problema(s):\n- ${falhas.join("\n- ")}`);
  process.exit(1);
}
console.log(`Consultas: as ${consultas.length} do worker.js compilam no SQLite e no Postgres; ${naD1.size} tabelas iguais nos dois.`);

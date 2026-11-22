// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Cópia de segurança da D1 do servidor de licenças para o computador do produtor (docs/ARQUITETURA.md, «Operação»).
// A D1 já guarda sozinha os últimos 7 dias (Time Travel, plano gratuito); esta cópia é para guardar mais tempo e fora
// da Cloudflare.
//
//   node servidor-licencas/copia-d1.mjs [pasta]
//       Lê todas as tabelas pela API da Cloudflare (só SELECT) e grava ~/.kangest-licencas/copias/d1-<data e hora>.sql,
//       só legível pelo produtor; guarda as 30 mais recentes. Token: CLOUDFLARE_API_TOKEN ou
//       ~/.kangest-licencas/cloudflare-token.txt (com «D1 · Read»); conta: CLOUDFLARE_ACCOUNT_ID ou a do token.
//   node servidor-licencas/copia-d1.mjs ensaiar <cópia.sql>
//       Carrega a cópia numa D1 local (o schema.sql + a cópia) e confirma que o servidor lê o resumo e cada ficha.
//
// Restaurar (apaga e volta a pôr as linhas de cada tabela; o esquema vem do schema.sql, que já está na D1):
//   npx wrangler d1 execute kangest-licencas --remote --file ~/.kangest-licencas/copias/d1-<data>.sql
// Para voltar a um momento dos últimos 7 dias, sem cópia: npx wrangler d1 time-travel restore kangest-licencas --timestamp=…
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const API = "https://api.cloudflare.com/client/v4";
export const NOME = "kangest-licencas";
export const POR_PAGINA = 500;
const GUARDAR = 30;
// a ordem em que se inserem (as que as chaves estrangeiras referem primeiro); o limite por IP não entra (é passageiro)
const PRIMEIRO = ["planos", "clientes"];

/** Uma ligação à D1 pela API da Cloudflare: consultar(sql) → linhas. `buscar` é o fetch (troca-se nos testes). */
export async function ligarD1({ token, conta, nome = NOME, buscar = fetch }) {
  const pedir = async (metodo, caminho, corpo) => {
    const r = await buscar(API + caminho, {
      method: metodo,
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: corpo ? JSON.stringify(corpo) : undefined,
    });
    const j = await r.json().catch(() => null);
    if (!r.ok || !j?.success) throw new Error(`Cloudflare (${r.status}): ${j?.errors?.map((e) => e.message).join("; ") || "sem resposta"}`);
    return j.result;
  };
  conta ||= (await pedir("GET", "/accounts"))[0]?.id;
  if (!conta) throw new Error("O token não dá acesso a nenhuma conta da Cloudflare.");
  const base = (await pedir("GET", `/accounts/${conta}/d1/database?name=${encodeURIComponent(nome)}`)).find((b) => b.name === nome);
  if (!base) throw new Error(`Não há nenhuma base D1 «${nome}» nesta conta da Cloudflare.`);
  return {
    conta,
    base: base.uuid,
    consultar: async (sql, params = []) => (await pedir("POST", `/accounts/${conta}/d1/database/${base.uuid}/query`, { sql, params }))[0].results,
  };
}

const literal = (v) => (v === null || v === undefined ? "NULL" : typeof v === "number" ? String(v) : `'${String(v).replace(/'/g, "''")}'`);

/** A cópia em SQL: para cada tabela, apaga o que lá estiver e volta a pôr as linhas. Só dados; o esquema é o schema.sql. */
export async function copiarD1(d1) {
  const nomes = (await d1.consultar("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '\\_cf\\_%' ESCAPE '\\' ORDER BY name"))
    .map((t) => t.name)
    .filter((t) => t !== "limites" && t !== "d1_migrations");
  const ordem = [...PRIMEIRO.filter((t) => nomes.includes(t)), ...nomes.filter((t) => !PRIMEIRO.includes(t))];
  const linhas = [`-- Cópia da D1 «${NOME}» em ${new Date().toISOString()}. Só dados: o esquema é o servidor-licencas/schema.sql.`, "PRAGMA defer_foreign_keys = true;"];
  const contagens = {};
  for (const t of [...ordem].reverse()) linhas.push(`DELETE FROM ${t};`);
  for (const t of ordem) {
    contagens[t] = 0;
    for (let desde = 0; ; desde += POR_PAGINA) {
      const pagina = await d1.consultar(`SELECT * FROM "${t}" ORDER BY rowid LIMIT ${POR_PAGINA} OFFSET ${desde}`);
      for (const l of pagina) linhas.push(`INSERT INTO ${t} (${Object.keys(l).join(", ")}) VALUES (${Object.values(l).map(literal).join(", ")});`);
      contagens[t] += pagina.length;
      if (pagina.length < POR_PAGINA) break;
    }
  }
  return { sql: linhas.join("\n") + "\n", contagens };
}

/** Carrega a cópia numa D1 local e confirma que o servidor (o mesmo worker.js) lê o resumo do produtor e cada ficha. */
export async function ensaiar(sqlCopia) {
  const { d1Local } = await import("./teste/d1-local.mjs");
  const { default: worker } = await import("./src/worker.js");
  const DB = d1Local(readFileSync(new URL("schema.sql", import.meta.url), "utf8"));
  await DB.exec(sqlCopia);
  const env = { DB, ADMIN_TOKEN: `ensaio-${crypto.randomUUID()}` };
  const pedir = async (caminho) => {
    const r = await worker.fetch(new Request(`https://ensaio.local${caminho}`, { headers: { authorization: `Bearer ${env.ADMIN_TOKEN}` } }), env);
    return { estado: r.status, corpo: await r.json() };
  };
  const resumo = await pedir("/admin/api/resumo");
  if (resumo.estado !== 200) throw new Error(`O resumo do produtor falhou: ${resumo.corpo.erro}`);
  for (const c of resumo.corpo.clientes) {
    const ficha = await pedir(`/admin/api/cliente?nif=${encodeURIComponent(c.nif)}`);
    if (ficha.estado !== 200) throw new Error(`A ficha do cliente ${c.nif} falhou: ${ficha.corpo.erro}`);
  }
  return { clientes: resumo.corpo.clientes.length, DB };
}

async function principal([accao, ...resto]) {
  if (accao === "ensaiar") {
    if (!resto[0]) throw new Error("Uso: copia-d1.mjs ensaiar <cópia.sql>");
    const { clientes } = await ensaiar(readFileSync(resto[0], "utf8"));
    console.log(`Ensaio feito: a cópia carrega e o servidor lê o resumo do produtor e a ficha de ${clientes === 1 ? "1 cliente" : `${clientes} clientes`}.`);
    return;
  }
  const pasta = accao ?? join(homedir(), ".kangest-licencas", "copias");
  const ficheiroToken = join(homedir(), ".kangest-licencas", "cloudflare-token.txt");
  const token = process.env.CLOUDFLARE_API_TOKEN || (existsSync(ficheiroToken) ? readFileSync(ficheiroToken, "utf8").trim() : "");
  if (!token) throw new Error(`Falta o token da Cloudflare (CLOUDFLARE_API_TOKEN ou ${ficheiroToken}).`);
  const { sql, contagens } = await copiarD1(await ligarD1({ token, conta: process.env.CLOUDFLARE_ACCOUNT_ID }));
  mkdirSync(pasta, { recursive: true, mode: 0o700 });
  const ficheiro = join(pasta, `d1-${new Date().toISOString().slice(0, 16).replace(/:/g, "")}.sql`);
  writeFileSync(ficheiro, sql, { mode: 0o600 });
  chmodSync(ficheiro, 0o600);
  const antigas = readdirSync(pasta).filter((f) => /^d1-.*\.sql$/.test(f)).sort().slice(0, -GUARDAR);
  for (const f of antigas) rmSync(join(pasta, f));
  const com = Object.entries(contagens).filter(([, n]) => n).map(([t, n]) => `${t} ${n}`);
  console.log(`Cópia gravada em ${ficheiro}: ${com.join(", ") || "sem linhas"}.${antigas.length ? ` Apagadas ${antigas.length} antigas.` : ""}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  principal(process.argv.slice(2)).catch((e) => {
    console.error(e.message);
    process.exitCode = 1;
  });
}

// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Fase 4 da passagem para o Supabase (docs/ARQUITETURA.md): copiar os dados da D1 (o Worker antigo) para o Postgres.
//
//   node servidor-central/copiar-d1.mjs exportar [ficheiro.json]
//       Lê todas as tabelas da D1 pela API da Cloudflare (CLOUDFLARE_API_TOKEN, o mesmo do publicar-licencas.py, e
//       CLOUDFLARE_ACCOUNT_ID) e grava-as em ~/.kangest-licencas/d1-<data>.json, que só o produtor lê.
//   node servidor-central/copiar-d1.mjs ensaiar <ficheiro.json|.sql>
//       Importa num Postgres local (PGlite, com as migrações) e confirma que o servidor lê tudo. Não toca no Supabase.
//   KANGEST_DB_URL=postgresql://… node servidor-central/copiar-d1.mjs importar <ficheiro.json|.sql> [--substituir]
//       Importa no Postgres do Supabase, numa só transacção. --substituir apaga antes o que lá estiver.
//
// Também aceita o .sql do «wrangler d1 export». Só mostra contagens, nunca os dados (têm emails, NIFs e resumos de senhas).
import { chmodSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { ESQUEMAS, OPCOES_POSTGRES, d1SobrePostgresJs } from "./supabase/functions/kangest/d1.js";

const API = "https://api.cloudflare.com/client/v4";
const ESQUEMA_DE = Object.fromEntries(Object.entries(ESQUEMAS).flatMap(([e, ts]) => ts.map((t) => [t, e])));
const qualificada = (t) => `${ESQUEMA_DE[t]}.${t}`;
/** Todas as tabelas, pela ordem em que se podem inserir: as que as chaves estrangeiras referem primeiro. */
export const ORDEM = ["planos", "clientes", ...Object.values(ESQUEMAS).flat().filter((t) => t !== "planos" && t !== "clientes")];
export const POR_PAGINA = 500;

/** Todas as tabelas da D1 «nome», pela API da Cloudflare. `buscar` é o fetch (troca-se nos testes). */
export async function exportarD1({ token, conta, nome = "kangest-licencas", buscar = fetch }) {
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
  const base = (await pedir("GET", `/accounts/${conta}/d1/database?name=${encodeURIComponent(nome)}`)).find((b) => b.name === nome);
  if (!base) throw new Error(`Não há nenhuma base D1 «${nome}» nesta conta da Cloudflare.`);
  const consultar = async (sql) => (await pedir("POST", `/accounts/${conta}/d1/database/${base.uuid}/query`, { sql, params: [] }))[0].results;
  const nomes = (await consultar("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '\\_cf\\_%' ESCAPE '\\' ORDER BY name")).map((t) => t.name);
  const tabelas = {};
  for (const t of nomes) {
    tabelas[t] = [];
    for (let desde = 0; ; desde += POR_PAGINA) {
      const linhas = await consultar(`SELECT * FROM "${t.replace(/"/g, '""')}" ORDER BY rowid LIMIT ${POR_PAGINA} OFFSET ${desde}`);
      tabelas[t].push(...linhas);
      if (linhas.length < POR_PAGINA) break;
    }
  }
  return { origem: "d1", base: nome, exportada_em: new Date().toISOString(), tabelas };
}

/** Uma cópia gravada: o .json do «exportar» ou o .sql do «wrangler d1 export» (lido pelo próprio SQLite). */
export async function lerCopia(texto, ficheiro) {
  if (/\.json$/i.test(ficheiro)) {
    const copia = JSON.parse(texto);
    if (!copia?.tabelas || typeof copia.tabelas !== "object") throw new Error(`${ficheiro} não é uma cópia da D1 (falta «tabelas»).`);
    return copia;
  }
  const { DatabaseSync } = await import("node:sqlite");
  const db = new DatabaseSync(":memory:", { enableForeignKeyConstraints: false });
  db.exec(texto);
  const tabelas = {};
  for (const { name } of db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all()) {
    tabelas[name] = db.prepare(`SELECT * FROM "${name.replace(/"/g, '""')}" ORDER BY rowid`).all().map((l) => ({ ...l }));
  }
  db.close();
  return { origem: "sql", tabelas };
}

const valor = (v) => (typeof v === "boolean" ? Number(v) : v === undefined ? null : v);

/**
 * Copia as tabelas para o Postgres (uma ligação do postgres.js), numa só transacção. Recusa se o destino já tiver dados,
 * a não ser com `substituir`. Os planos que a migração já pôs ficam com os valores da D1.
 */
export async function importar(sql, copia, { substituir = false } = {}) {
  const tabelas = copia.tabelas ?? {};
  const relatorio = { tabelas: [], ignoradas: Object.keys(tabelas).filter((t) => !ESQUEMA_DE[t]).sort() };
  await sql.begin(async (tx) => {
    const colunas = {};
    const esquemas = Object.keys(ESQUEMAS); // em lista de parâmetros: sem fetch_types, o postgres.js não envia arrays
    const lidas = await tx.unsafe(
      `SELECT table_name AS t, column_name AS c FROM information_schema.columns WHERE table_schema IN (${esquemas.map((_, i) => "$" + (i + 1)).join(", ")}) ORDER BY ordinal_position`,
      esquemas,
    );
    for (const { t, c } of lidas) (colunas[t] ??= []).push(c);
    const contar = async (t) => (await tx.unsafe(`SELECT count(*) AS n FROM ${qualificada(t)}`))[0].n;

    if (substituir) {
      await tx.unsafe(`TRUNCATE ${ORDEM.map(qualificada).join(", ")} RESTART IDENTITY CASCADE`);
    } else {
      for (const t of ORDEM.filter((t) => t !== "planos")) {
        const n = await contar(t);
        if (n > 0) throw new Error(`O Postgres já tem dados em ${qualificada(t)} (${n} linhas). Para os apagar e copiar de novo: --substituir.`);
      }
    }

    for (const t of ORDEM) {
      const linhas = tabelas[t];
      if (!Array.isArray(linhas)) {
        relatorio.tabelas.push({ tabela: qualificada(t), origem: null, destino: await contar(t) });
        continue;
      }
      const cols = [...new Set(linhas.flatMap((l) => Object.keys(l)))];
      const aMais = cols.filter((c) => !colunas[t].includes(c));
      if (aMais.length) throw new Error(`${t}: a cópia tem colunas que o Postgres não tem (${aMais.join(", ")}). Acrescente-as numa migração antes de copiar.`);
      const outras = cols.filter((c) => c !== "codigo");
      const conflito = t !== "planos" ? "" : outras.length ? ` ON CONFLICT (codigo) DO UPDATE SET ${outras.map((c) => `${c} = excluded.${c}`).join(", ")}` : " ON CONFLICT (codigo) DO NOTHING";
      for (let i = 0; i < linhas.length && cols.length; i += 200) {
        const args = [];
        const valores = linhas
          .slice(i, i + 200)
          .map((l) => `(${cols.map((c) => { args.push(valor(l[c])); return "$" + args.length; }).join(", ")})`)
          .join(", ");
        try {
          await tx.unsafe(`INSERT INTO ${qualificada(t)} (${cols.join(", ")}) VALUES ${valores}${conflito}`, args);
        } catch (e) {
          throw new Error(`${t}, linhas ${i + 1} a ${Math.min(i + 200, linhas.length)}: ${e.message}${e.detail ? ` (${e.detail})` : ""}`);
        }
      }
      relatorio.tabelas.push({ tabela: qualificada(t), origem: linhas.length, destino: await contar(t) });
    }
    // os pagamentos novos continuam a numeração da D1
    await tx.unsafe("SELECT setval(pg_get_serial_sequence('licencas.pagamentos', 'id'), COALESCE((SELECT max(id) FROM licencas.pagamentos), 0) + 1, false)");
  });
  return relatorio;
}

/** O servidor (o mesmo worker.js) lê os dados copiados: o resumo do produtor e a ficha de cada cliente. Só lê. */
export async function verificarComServidor(sql) {
  const { default: worker } = await import("../servidor-licencas/src/worker.js");
  const env = { DB: d1SobrePostgresJs(sql), ADMIN_TOKEN: `verificar-${crypto.randomUUID()}` };
  const pedir = async (caminho) => {
    const r = await worker.fetch(new Request(`https://verificar.local${caminho}`, { headers: { authorization: `Bearer ${env.ADMIN_TOKEN}` } }), env);
    return { estado: r.status, corpo: await r.json() };
  };
  const resumo = await pedir("/admin/api/resumo");
  if (resumo.estado !== 200) throw new Error(`O resumo do produtor falhou: ${resumo.corpo.erro}`);
  for (const c of resumo.corpo.clientes) {
    const ficha = await pedir(`/admin/api/cliente?nif=${encodeURIComponent(c.nif)}`);
    if (ficha.estado !== 200) throw new Error(`A ficha do cliente ${c.nif} falhou: ${ficha.corpo.erro}`);
  }
  return { clientes: resumo.corpo.clientes.length };
}

function mostrar(linhas, { titulo = false } = {}) {
  const largura = Math.max(6, ...linhas.map((l) => l.tabela.length));
  const destino = (v) => (v === undefined ? "" : `  →  ${String(v).padStart(8)}`);
  if (titulo) console.log(`  ${"Tabela".padEnd(largura)}  ${"D1".padStart(8)}${destino("Postgres")}`);
  for (const l of linhas) console.log(`  ${l.tabela.padEnd(largura)}  ${(l.origem === null ? "(não há)" : String(l.origem)).padStart(8)}${destino(l.destino)}`);
}

async function principal([accao, ficheiro, ...opcoes]) {
  if (accao === "exportar") {
    const { CLOUDFLARE_API_TOKEN: token, CLOUDFLARE_ACCOUNT_ID: conta } = process.env;
    if (!token || !conta) throw new Error("Defina CLOUDFLARE_API_TOKEN e CLOUDFLARE_ACCOUNT_ID (os mesmos do publicar-licencas.py).");
    const copia = await exportarD1({ token, conta });
    const destino = ficheiro ?? join(homedir(), ".kangest-licencas", `d1-${copia.exportada_em.slice(0, 10)}.json`);
    mkdirSync(dirname(destino), { recursive: true });
    writeFileSync(destino, JSON.stringify(copia), { mode: 0o600 });
    chmodSync(destino, 0o600);
    mostrar(Object.entries(copia.tabelas).map(([tabela, l]) => ({ tabela, origem: l.length })));
    console.log(`Cópia gravada em ${destino}. Tem dados dos clientes: fica fora do repositório e só o produtor a lê.`);
    return;
  }
  if ((accao !== "ensaiar" && accao !== "importar") || !ficheiro) {
    console.error("Uso: copiar-d1.mjs exportar [ficheiro.json] | ensaiar <ficheiro> | importar <ficheiro> [--substituir]");
    process.exitCode = 2;
    return;
  }
  const copia = await lerCopia(readFileSync(ficheiro, "utf8"), ficheiro);
  let sql;
  let fechar;
  if (accao === "ensaiar") {
    ({ sql, fechar } = await (await import("./teste/pglite.mjs")).postgresLocal());
  } else {
    const url = process.env.KANGEST_DB_URL;
    if (!url) throw new Error("Defina KANGEST_DB_URL com a ligação ao Postgres do Supabase (Project Settings → Database → Connection string, «Session pooler»).");
    const { default: postgres } = await import("postgres");
    sql = postgres(url, { ...OPCOES_POSTGRES, max: 1, ssl: /@(localhost|127\.0\.0\.1)[:/]/.test(url) ? false : "require", onnotice: () => {} });
    fechar = () => sql.end();
  }
  try {
    const r = await importar(sql, copia, { substituir: opcoes.includes("--substituir") });
    mostrar(r.tabelas, { titulo: true });
    if (r.ignoradas.length) console.log(`Na cópia mas não no Postgres (não copiadas): ${r.ignoradas.join(", ")}.`);
    const diferentes = r.tabelas.filter((l) => l.origem !== null && l.origem !== l.destino && l.tabela !== "empresas.planos");
    if (diferentes.length) throw new Error(`Contagens diferentes em: ${diferentes.map((l) => l.tabela).join(", ")}.`);
    const { clientes: n } = await verificarComServidor(sql);
    console.log(`${accao === "ensaiar" ? "Ensaio feito" : "Cópia feita"}: o servidor lê o resumo do produtor e a ficha de ${n === 1 ? "1 cliente" : `${n} clientes`}.`);
  } finally {
    await fechar();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  principal(process.argv.slice(2)).catch((e) => {
    console.error(e.message);
    process.exitCode = 1;
  });
}

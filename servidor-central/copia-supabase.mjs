// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Cópia de segurança do backend central (o Postgres do Supabase) para o computador do produtor:
//   node servidor-central/copia-supabase.mjs [pasta]
// O plano gratuito do Supabase não guarda cópias que se possam descarregar. Esta guarda todas as tabelas (menos o limite
// de pedidos por IP, que é passageiro) em ~/.kangest-licencas/copias/supabase-<data e hora>.json, só legível pelo
// produtor, e mantém as 30 mais recentes. Só lê (transacção só de leitura), com o token do Supabase
// (~/.kangest-licencas/supabase-token.txt ou SUPABASE_ACCESS_TOKEN).
//
// A cópia tem o formato do copiar-d1.mjs:
//   node servidor-central/copiar-d1.mjs ensaiar <cópia>                                  confirma que se restaura
//   KANGEST_DB_URL=… node servidor-central/copiar-d1.mjs importar <cópia> --substituir    restaura no Supabase
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { ESQUEMAS } from "./supabase/functions/kangest/d1.js";

export const PROJECTO = "fissktkxilseymadzppn";
const POR_PAGINA = 1000;
const GUARDAR = 30;

/** Todas as tabelas do backend (menos «limites»), pela API de gestão do Supabase. `buscar` é o fetch (troca-se nos testes). */
export async function copiarSupabase({ token, projecto = PROJECTO, buscar = fetch }) {
  const consultar = async (sql) => {
    const r = await buscar(`https://api.supabase.com/v1/projects/${projecto}/database/query`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ query: sql, read_only: true }),
    });
    const j = await r.json().catch(() => null);
    if (!r.ok || !Array.isArray(j)) throw new Error(`Supabase (${r.status}): ${j?.message ?? "sem resposta"}`);
    return j;
  };
  const tabelas = {};
  for (const [esquema, nomes] of Object.entries(ESQUEMAS)) {
    for (const t of nomes.filter((t) => t !== "limites")) {
      tabelas[t] = [];
      for (let desde = 0; ; desde += POR_PAGINA) {
        const linhas = await consultar(`select * from ${esquema}.${t} order by 1, 2 limit ${POR_PAGINA} offset ${desde}`);
        tabelas[t].push(...linhas);
        if (linhas.length < POR_PAGINA) break;
      }
    }
  }
  return { origem: "supabase", projecto, copiada_em: new Date().toISOString(), tabelas };
}

async function principal([pasta = join(homedir(), ".kangest-licencas", "copias")]) {
  const ficheiroToken = join(homedir(), ".kangest-licencas", "supabase-token.txt");
  const token = process.env.SUPABASE_ACCESS_TOKEN || (existsSync(ficheiroToken) ? readFileSync(ficheiroToken, "utf8").trim() : "");
  if (!token) throw new Error(`Falta o token do Supabase (${ficheiroToken} ou SUPABASE_ACCESS_TOKEN).`);
  const copia = await copiarSupabase({ token });
  mkdirSync(pasta, { recursive: true, mode: 0o700 });
  const ficheiro = join(pasta, `supabase-${copia.copiada_em.slice(0, 16).replace(/:/g, "")}.json`);
  writeFileSync(ficheiro, JSON.stringify(copia), { mode: 0o600 });
  chmodSync(ficheiro, 0o600);
  // só as mais recentes
  const antigas = readdirSync(pasta).filter((f) => /^supabase-.*\.json$/.test(f)).sort().slice(0, -GUARDAR);
  for (const f of antigas) rmSync(join(pasta, f));
  const linhas = Object.entries(copia.tabelas).filter(([, l]) => l.length).map(([t, l]) => `${t} ${l.length}`);
  console.log(`Cópia gravada em ${ficheiro}: ${linhas.join(", ") || "sem linhas"}.${antigas.length ? ` Apagadas ${antigas.length} antigas.` : ""}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  principal(process.argv.slice(2)).catch((e) => {
    console.error(e.message);
    process.exitCode = 1;
  });
}

// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// O Postgres local, sem Docker nem rede: PGlite (Postgres em memória) com as migrações do Supabase, servido num socket
// local e usado pelo postgres.js com as mesmas opções e o mesmo adaptador da função kangest. Serve os testes e o ensaio
// da cópia dos dados da D1 (importar-d1.mjs).
import { readFileSync, readdirSync } from "node:fs";
import { PGlite } from "@electric-sql/pglite";
import { PGLiteSocketServer } from "@electric-sql/pglite-socket";
import postgres from "postgres";
import { OPCOES_POSTGRES, d1SobrePostgresJs } from "../supabase/functions/kangest/d1.js";

const MIGRACOES = new URL("../supabase/migrations/", import.meta.url);

/** PGlite com os papéis da API do Supabase (as migrações tiram-lhes os privilégios) e todas as migrações aplicadas. */
export async function pgliteComMigracoes() {
  const pg = await PGlite.create({ parsers: { 20: Number, 1700: Number } });
  await pg.exec("CREATE ROLE anon NOLOGIN; CREATE ROLE authenticated NOLOGIN; CREATE ROLE service_role NOLOGIN;");
  for (const f of readdirSync(MIGRACOES).filter((f) => f.endsWith(".sql")).sort()) {
    await pg.exec(readFileSync(new URL(f, MIGRACOES), "utf8"));
  }
  return pg;
}

/** Uma ligação do postgres.js (com as opções da função) a um Postgres local novo, com as migrações. */
export async function postgresLocal() {
  const pg = await pgliteComMigracoes();
  const servidor = new PGLiteSocketServer({ db: pg, host: "127.0.0.1", port: 0 });
  await servidor.start();
  servidor.server?.unref?.(); // o processo acaba sozinho quando a ligação fica parada (idle_timeout)
  const sql = postgres(`postgres://postgres@${servidor.getServerConn()}/postgres`, {
    ...OPCOES_POSTGRES,
    max: 1,
    idle_timeout: 1,
    onnotice: () => {},
  });
  const fechar = async () => {
    await sql.end();
    await servidor.stop();
    await pg.close();
  };
  return { sql, fechar };
}

/** Uma base com a API do D1 pelo mesmo caminho da função: postgres.js → d1.js → Postgres. */
export async function basePostgres() {
  const { sql, fechar } = await postgresLocal();
  const base = d1SobrePostgresJs(sql);
  base.fechar = fechar;
  base.sql = sql;
  return base;
}

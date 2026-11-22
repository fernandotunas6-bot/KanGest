// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// A base dos testes do servidor: SQLite em memória, como o D1 (por omissão), ou, com KANGEST_BD=postgres, o Postgres da
// função kangest do Supabase (PGlite com as migrações, pelo postgres.js e pelo adaptador d1.js). O Postgres precisa de
// «npm ci» uma vez em servidor-central.
import { readFileSync } from "node:fs";

export const motor = process.env.KANGEST_BD === "postgres" ? "Postgres" : "SQLite";

export async function baseDeTeste() {
  if (motor === "Postgres") return (await import("../../servidor-central/teste/pglite.mjs")).basePostgres();
  const { d1Local } = await import("./d1-local.mjs");
  return d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8"));
}

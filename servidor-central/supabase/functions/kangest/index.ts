// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Backend central do KanGest no Supabase: o servidor de licenças (servidor-licencas/src/worker.js) com a base em
// Postgres, nas mesmas rotas, em https://<projecto>.supabase.co/functions/v1/kangest/… Ver docs/ARQUITETURA.md.
// Antes de publicar: node servidor-central/preparar.mjs (copia o núcleo para nucleo/).
import postgres from "npm:postgres@3.4.9";
import { OPCOES_POSTGRES, d1SobrePostgresJs } from "./d1.js";
import { criarEnv, tratar } from "./entrada.js";

const ligacao = Deno.env.get("SUPABASE_DB_URL");
if (!ligacao) throw new Error("Falta SUPABASE_DB_URL (o Supabase define-a sozinho nas funções).");

const env = criarEnv(d1SobrePostgresJs(postgres(ligacao, OPCOES_POSTGRES)), (nome: string) => Deno.env.get(nome));

Deno.serve((req: Request) => tratar(req, env));

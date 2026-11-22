// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Resumo da senha do painel do produtor, para o segredo ADMIN_SENHA do servidor de licenças. A senha lê-se da entrada
// (não fica no histórico do terminal) e só sai o resumo PBKDF2-SHA256 com sal aleatório:
//
//   node servidor-licencas/resumo-senha.mjs > ~/.kangest-licencas/servidor/admin-senha.txt
//
// O scripts/publicar-licencas.py envia-o (com o admin-email.txt) com --segredos ou --senha-painel.
import { pbkdf2Sync, randomBytes } from "node:crypto";

const ITERACOES = 100_000; // o máximo que o Cloudflare Workers aceita no PBKDF2
const b64url = (b) => Buffer.from(b).toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

let senha = "";
for await (const parte of process.stdin) senha += parte;
senha = senha.replace(/\r?\n$/, "");
if (senha.length < 10) {
  console.error("A senha tem de ter pelo menos 10 caracteres.");
  process.exit(1);
}
const sal = randomBytes(16);
const resumo = pbkdf2Sync(senha, sal, ITERACOES, 32, "sha256");
console.log(`pbkdf2-sha256$${ITERACOES}$${b64url(sal)}$${b64url(resumo)}`);

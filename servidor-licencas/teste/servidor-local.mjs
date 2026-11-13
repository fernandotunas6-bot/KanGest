// O Worker a correr no Node, com D1 em memória, para testar o KanGest sem Cloudflare:
//   KANGEST_CHAVE_SERVIDOR=~/.kangest-licencas/servidor/privada.pem ADMIN_TOKEN=… node teste/servidor-local.mjs 8787
import { createServer } from "node:http";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync } from "node:fs";
import { d1Local } from "./d1-local.mjs";
import worker from "../src/worker.js";

const porta = Number(process.argv[2] ?? 8787);
const env = {
  DB: d1Local(readFileSync(new URL("../schema.sql", import.meta.url), "utf8")),
  CHAVE_PRIVADA: process.env.KANGEST_CHAVE_SERVIDOR
    ? readFileSync(process.env.KANGEST_CHAVE_SERVIDOR, "utf8")
    : generateKeyPairSync("ed25519").privateKey.export({ format: "pem", type: "pkcs8" }),
  ADMIN_TOKEN: process.env.ADMIN_TOKEN ?? "teste-local-".padEnd(32, "x"),
};

createServer(async (req, res) => {
  const partes = [];
  for await (const p of req) partes.push(p);
  const corpo = Buffer.concat(partes);
  const r = await worker.fetch(
    new Request(`http://127.0.0.1:${porta}${req.url}`, { method: req.method, headers: req.headers, body: ["GET", "HEAD"].includes(req.method) ? undefined : corpo }),
    env,
  );
  res.writeHead(r.status, Object.fromEntries(r.headers));
  res.end(Buffer.from(await r.arrayBuffer()));
}).listen(porta, "127.0.0.1", () => console.log(`Servidor de licenças local em http://127.0.0.1:${porta}`));

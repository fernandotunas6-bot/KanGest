# Servidor de licenças do KanGest

Cloudflare Worker + D1, sem dependências. Regista os KanGest instalados (com confirmação por código), dá a
experiência uma vez por NIF, guarda os pagamentos e entrega as licenças (`KG1.…`, assinadas com Ed25519).

| Ficheiro | O quê |
|---|---|
| `src/worker.js` | API pública (`/v1/registo`, `/v1/confirmar`, `/v1/renovar`, `/v1/planos`) e do painel (`/admin/api/…`) |
| `src/painel.html` | Painel do produtor (`/admin`). **Depois de o alterar:** `node construir.mjs` gera `src/painel.js` |
| `schema.sql` | Tabelas (planos, clientes, maquinas, codigos, licencas, pagamentos). Só `CREATE … IF NOT EXISTS` |
| `teste/testar.mjs` | Teste completo sem rede (D1 imitado com `node:sqlite`) |
| `teste/servidor-local.mjs` | O Worker servido pelo Node, para testar o KanGest (`licencas_servidor_local` no Rust) |

Publicar: `CLOUDFLARE_API_TOKEN=… python3 ../scripts/publicar-licencas.py` (ver `docs/LICENCAS.md`).
Segredos no Cloudflare: `CHAVE_PRIVADA`, `ADMIN_TOKEN` e, opcionais, `RESEND_API_KEY` e `EMAIL_REMETENTE`.

# Servidor de licenças do KanGest

Cloudflare Worker + D1, sem dependências. Regista os KanGest instalados (com confirmação por código), dá a
experiência uma vez por NIF, guarda os pagamentos e entrega as licenças (`KG1.…`, assinadas com Ed25519).

| Ficheiro | O quê |
|---|---|
| `src/worker.js` | API pública (`/v1/registo`, `/v1/confirmar`, `/v1/renovar`, `/v1/planos`) e do painel (`/admin/api/…`) |
| `src/painel.html` | Painel do produtor (`/admin`). **Depois de o alterar:** `node construir.mjs` gera `src/painel.js` |
| `src/encaminhar.js` | O Worker depois da passagem para o Supabase (fase 4): encaminha tudo para a função `kangest` (`publicar-licencas.py --encaminhar`) |
| `schema.sql` | Tabelas (planos, clientes, maquinas, codigos, licencas, pagamentos). Só `CREATE … IF NOT EXISTS` |
| `teste/testar.mjs` | Teste completo sem rede (D1 imitado com `node:sqlite`) |
| `teste/contas.mjs` | Contas das empresas no site e equipa gerida |
| `teste/base.mjs` | A base dos testes: SQLite, como o D1, ou, com `KANGEST_BD=postgres`, o Postgres do Supabase (ver `servidor-central/`) |
| `teste/servidor-local.mjs` | O Worker servido pelo Node, para testar o KanGest (`licencas_servidor_local` no Rust) |

O mesmo `worker.js` corre também no Supabase ([`servidor-central/`](../servidor-central/LEIA-ME.md)), com a base em
Postgres. Por isso, o SQL tem de ser o que o SQLite e o Postgres percebem da mesma maneira (regras em
[`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md)). `node servidor-central/teste/consultas.mjs` confirma-o.
Uma tabela nova entra em três sítios:
- `schema.sql`;
- uma migração em `servidor-central/supabase/migrations`;
- `ESQUEMAS` no `d1.js`.

Publicar: `CLOUDFLARE_API_TOKEN=… python3 ../scripts/publicar-licencas.py` (ver `docs/LICENCAS.md`).
Segredos no Cloudflare: `CHAVE_PRIVADA`, `ADMIN_TOKEN` e, opcionais, `RESEND_API_KEY`, `EMAIL_REMETENTE`, `ACCESS_EQUIPA` e `ACCESS_AUD`.

Protecção: limite de pedidos por IP e por hora (tabela `limites`, constantes `LIMITES`), Cloudflare Access à frente do
painel (com `ACCESS_EQUIPA` e `ACCESS_AUD`) e cabeçalhos de segurança em todas as respostas. Ver `docs/CLOUDFLARE.md`.

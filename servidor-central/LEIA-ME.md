# Backend central do KanGest no Supabase

O servidor de licenças ([`servidor-licencas/src/worker.js`](../servidor-licencas/src/worker.js)) a correr como uma
Edge Function do Supabase, com a base em Postgres. As rotas e o código são os mesmos. O mapa geral está em
[`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md).

| Ficheiro | O quê |
|---|---|
| `supabase/migrations/` | as tabelas, por esquema (`empresas`, `licencas`, `equipa`, `seguranca`, `web`). Uma migração aplicada nunca se altera |
| `supabase/functions/kangest/index.ts` | a entrada no Deno: liga ao Postgres (`SUPABASE_DB_URL`) e serve os pedidos |
| `supabase/functions/kangest/entrada.js` | entrega cada pedido ao `worker.js` como o Worker o recebia: sem o prefixo `/kangest`, com o IP e o país. `/admin` vai para o site |
| `supabase/functions/kangest/d1.js` | a API do D1 sobre Postgres (marcadores, esquemas, tipos, transacções) |
| `supabase/functions/kangest/nucleo/` | cópia do `worker.js` e do painel, feita por `preparar.mjs`; não vai para o repositório |
| `supabase/config.toml` | `verify_jwt = false` para a função, que tem a sua própria autenticação |
| `teste/` | testes em Postgres local (PGlite), sem Docker nem rede |

## Testar

```bash
cd servidor-central
npm ci                                                        # uma vez
npm run testar                                                # consultas, função e contas em Postgres
KANGEST_BD=postgres node ../servidor-licencas/teste/testar.mjs  # o teste completo do servidor, em Postgres
```

- `teste/consultas.mjs` compila cada consulta do `worker.js` no SQLite e no Postgres. Também confirma que as tabelas e
  as colunas são as mesmas no `schema.sql`, nas migrações e em `ESQUEMAS` do `d1.js`.
- `teste/funcao.mjs` faz pedidos como o Supabase os entrega e percorre o caminho completo:
  - registo e licença assinada;
  - renovação e pagamentos;
  - bloqueio de um computador;
  - conta da empresa;
  - CORS e limite por IP.
- Com `KANGEST_BD=postgres`, os testes de `servidor-licencas/teste` (`contas.mjs`, `testar.mjs`) correm no Postgres.
  Sem a variável, correm no SQLite, como o D1.

## Publicar (quando houver acesso ao projecto)

```bash
cd servidor-central
supabase link --project-ref fissktkxilseymadzppn
supabase db push                                         # aplica as migrações
node preparar.mjs                                        # copia o núcleo para a função
supabase secrets set --env-file ~/.kangest-licencas/supabase.env   # os segredos de docs/ARQUITETURA.md, fora do repositório
supabase functions deploy kangest --no-verify-jwt
```

Depois de publicar:

1. `https://fissktkxilseymadzppn.supabase.co/functions/v1/kangest/v1/planos` responde com os planos.
2. Depois de um pedido, `select chave from seguranca.limites` mostra o IP público de quem pediu, não um IP interno. É
   assim que se confirma que o limite por IP vê o IP certo.
3. Os esquemas do KanGest **não** se juntam a *Exposed schemas* nas definições da API.

A cópia dos dados da D1 e o Worker antigo a encaminhar para aqui são a fase 4 ([`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md)).

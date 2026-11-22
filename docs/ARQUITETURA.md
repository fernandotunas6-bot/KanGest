# Arquitectura

Uma regra para tudo: **cada coisa vive num só sítio e há uma só forma de lhe chegar.** Este documento é a
referência. Quando uma peça muda de sítio, este documento muda no mesmo pull request.

## 1. Três camadas

| Camada | O que guarda | Onde vive | Tecnologia |
|---|---|---|---|
| **Dados da empresa** | vendas, stock, caixa, SAF-T, AGT, utilizadores e PINs | no computador servidor de cada loja | SQLite (`kigest.db`), funciona sem Internet |
| **Backend central** | empresas (NIF), licenças, pagamentos, contas, equipa gerida, sessões | um projecto Supabase por produto | Postgres + uma Edge Function |
| **Sites** | páginas, transferências, área do produtor (`/admin.html`), conta da empresa (`/conta.html`) | Cloudflare Pages | estático, sem base de dados |

O backend central só guarda o que serve para licenciar, cobrar e gerir a equipa. Os dados de negócio (vendas, clientes
da loja, stock) ficam no computador da empresa. Só saem nas cópias de segurança que ela própria configura.

## 2. Produtos

| Produto | Backend central | Site |
|---|---|---|
| KanGest | Supabase `fissktkxilseymadzppn` (em transição; hoje é o Worker `kangest-licencas` + D1) | kangest.pages.dev |
| Livraria | projecto Supabase próprio | livrariaadventista.ao |
| SIGA Plus | projecto Supabase próprio | portal-siga.com |

Os produtos não partilham bases de dados. A Livraria fala com o KanGest só pela API de integração do KanGest
([API.md](API.md)).

## 3. Backend central do KanGest

### Um só código, as mesmas rotas

O servidor é um ficheiro, [`servidor-licencas/src/worker.js`](../servidor-licencas/src/worker.js). Durante a
transição corre em dois sítios sem mudar uma linha:

| | Cloudflare (hoje) | Supabase (destino) |
|---|---|---|
| Entrada | o próprio `worker.js` (`export default { fetch }`) | [`servidor-central/supabase/functions/kangest/index.ts`](../servidor-central/supabase/functions/kangest/index.ts) |
| Base | D1 (SQLite), tabelas em [`schema.sql`](../servidor-licencas/schema.sql) | Postgres, tabelas nas [migrações](../servidor-central/supabase/migrations) |
| Ligação à base | `env.DB` do Worker | `env.DB` do adaptador [`d1.js`](../servidor-central/supabase/functions/kangest/d1.js) (a mesma API do D1) |
| Endereço | `https://kangest-licencas.valentinocanguele.workers.dev` | `https://fissktkxilseymadzppn.supabase.co/functions/v1/kangest` |

As rotas são as mesmas nos dois endereços:

| Rota | Quem a usa |
|---|---|
| `POST /v1/registo`, `/v1/confirmar`, `/v1/renovar` | o KanGest (registo, experiência, licença, estado, equipa gerida) |
| `GET /v1/planos` | o site (planos e preços) |
| `/v1/web/…` | o KanGest e o site (acesso pela Internet, verificação de humano) |
| `/v1/conta/…` | a conta da empresa no site (`/conta.html`) |
| `/admin/api/…` | a área do produtor no site (`/admin.html`) |

No Supabase, `GET /admin` (a página antiga do painel) encaminha para `/admin.html` no site. As funções do Supabase não
servem páginas HTML.

**O adaptador** (`d1.js`) faz o Postgres parecer o D1:
- `?` e `?N` passam a `$N`;
- cada tabela ganha o seu esquema (`clientes` → `empresas.clientes`), sem depender do `search_path`;
- verdadeiro e falso vão como 1 e 0, e `undefined` vai como `NULL`, como no D1;
- os inteiros grandes voltam como números;
- `batch` corre numa transacção.

**Regra para o SQL do `worker.js`:** só SQL que o SQLite e o Postgres percebem da mesma maneira.
- `COALESCE`, nunca `IFNULL`.
- `CASE … THEN 1 ELSE 0 END` em vez de comparações no `SELECT`, porque o Postgres devolve `true`/`false`.
- No `ON CONFLICT … DO UPDATE`, a linha antiga leva o nome da tabela (`limites.n`) e a nova leva `excluded.`.

Os testes confirmam-no, e o CI corre-os em cada push
([`servidor-central/LEIA-ME.md`](../servidor-central/LEIA-ME.md)):
- `servidor-central/teste/consultas.mjs` compila todas as consultas do `worker.js` nos dois motores;
- `servidor-central/teste/funcao.mjs` percorre a função de ponta a ponta sobre Postgres;
- com `KANGEST_BD=postgres`, os testes de `servidor-licencas/teste` correm no Postgres.

### Esquemas (Postgres)

| Esquema | Tabelas | Para quê |
|---|---|---|
| `empresas` | `clientes`, `contas`, `planos` | a empresa (pelo NIF), a conta do dono no site, os planos à venda |
| `licencas` | `maquinas`, `instalacoes`, `licencas`, `pagamentos`, `codigos` | computadores registados e o estado de cada um, licenças emitidas, pagamentos, códigos de registo |
| `equipa` | `funcionarios`, `lojas_geridas`, `postos_geridos`, `equipa_versoes`, `equipas` | a equipa gerida no site e a que cada KanGest comunica |
| `seguranca` | `sessoes_painel`, `sessoes_conta`, `codigos_conta`, `segredos_maquina`, `limites` | sessões, códigos da conta, segredos dos computadores, limite de pedidos por IP |
| `web` | `subdominios` | acesso pela Internet (um subdomínio por empresa) |

As tabelas e as colunas têm os mesmos nomes na D1 e no Postgres. Uma tabela nova entra em três sítios:
- `schema.sql`;
- uma migração nova;
- `ESQUEMAS` em `d1.js`.

### Quem chega aos dados

- **Só a função `kangest`**, pela ligação directa ao Postgres (`SUPABASE_DB_URL`).
- Os cinco esquemas **não se expõem** na API REST do Supabase: não se juntam a *Exposed schemas* nas definições da API.
  Todas as tabelas têm RLS ligado e nenhuma política, e `anon` e `authenticated` não têm privilégios nelas. Quem tiver a
  chave pública do projecto não lê nem escreve nada.
- A função não usa o Supabase Auth nem a verificação de JWT do Supabase (`verify_jwt = false`). Tem a sua própria
  autenticação:
  - o produtor: token ou sessão `s1.`;
  - o dono da empresa: sessão `c1.`;
  - cada computador: o seu segredo;
  - o registo e a conta: códigos de 6 algarismos.

### Segredos da função

Os mesmos nomes no Worker (`wrangler secret put`) e na função (`supabase secrets set`), nunca no repositório:

| Variável | Para quê | Obrigatória |
|---|---|---|
| `CHAVE_PRIVADA` | assina as licenças (Ed25519, PEM pkcs8) | sim |
| `ADMIN_TOKEN` | chave de emergência do produtor e chave das sessões | sim |
| `ADMIN_EMAIL`, `ADMIN_SENHA` | entrada do produtor com email e senha (a senha só como resumo PBKDF2) | não |
| `ADMIN_ORIGENS` | sites que podem chamar `/admin/api` e `/v1/conta` (por omissão `https://kangest.pages.dev`) | não |
| `RESEND_API_KEY`, `EMAIL_REMETENTE` | envio dos códigos por email (sem eles, o código aparece na área do produtor) | não |
| `TURNSTILE_SITE_KEY`, `TURNSTILE_SECRET_KEY` | verificação de humano | não |
| `WEB_DOMINIO`, `CF_API_TOKEN`, `CF_CONTA`, `CF_ZONA` | acesso pela Internet (túneis e DNS na Cloudflare) | não |
| `ACCESS_EQUIPA`, `ACCESS_AUD` | Cloudflare Access à frente do painel (só no Worker) | não |
| `SUPABASE_DB_URL` | ligação ao Postgres (o Supabase define-a sozinho) | — |
| `ENCAMINHADOR_SEGREDO` | o mesmo na função e no Worker antigo depois da fase 4: a função só aceita o IP e o país que o Worker lhe diz quando o segredo está certo | na fase 4 |
| `NOVO_SERVIDOR` | só no Worker antigo depois da fase 4: o endereço da função para onde encaminha | na fase 4 |

## 4. Convenções

- **Nomes:** português, `snake_case`, tabelas no plural.
- **Datas:** texto ISO 8601 em UTC (`2026-11-22T10:40:00Z`); os dias em `AAAA-MM-DD` (dia de Luanda). Os momentos
  acabam em `_em`.
- **Dinheiro:** cêntimos de kwanza, inteiros (`bigint` no Postgres), nunca decimais.
- **Indicadores:** inteiros 0/1 (`activo`, `bloqueada`), iguais nos dois motores.
- **Chaves:**
  - a empresa é sempre o NIF;
  - um computador é o NIF + o código da máquina;
  - o que se gere no site tem um `id` do site, que no KanGest fica em `conta_id`.
- **Segredos:** nunca no código nem em claro na base, só resumos:
  - SHA-256 nos códigos e nos segredos dos computadores;
  - PBKDF2 nas senhas do site;
  - Argon2 no KanGest.
- **Endereços:** um sítio por cliente da API:
  - no site, `site-fonte/config.json` (`web.licencas`);
  - no programa, `SERVIDOR` em `src-tauri/src/services/licenca.rs`.
- **Estrutura da base:** só por migrações numeradas, e uma migração aplicada nunca se altera (corrige-se com outra):
  - KanGest: `src-tauri/migrations/NNNN_….sql`;
  - backend: `servidor-central/supabase/migrations/<data>_….sql`.
- **Ambientes:** produção e testes separados (um branch do Supabase para testes).

## 5. Contas

| Quem | Onde entra | Onde fica a conta |
|---|---|---|
| Produtor | `/admin.html` no site | backend central (email e senha; sessões em `seguranca.sessoes_painel`) |
| Dono da empresa | `/conta.html` no site | backend central (`empresas.contas`; sessões em `seguranca.sessoes_conta`) |
| Funcionários | no KanGest (login e senha ou PIN, sem Internet) | `kigest.db`. São geridos na conta da empresa (`equipa.*`) e o KanGest aplica-os na verificação |

## 6. Onde está cada coisa

| Pasta | O quê |
|---|---|
| `src-tauri/` | o programa (Rust): base local, regras, API do KanGest |
| `src/` | a interface (React) |
| `servidor-licencas/` | o servidor central: `src/worker.js`, `schema.sql` (D1), testes |
| `servidor-central/` | o mesmo servidor no Supabase: migrações, função `kangest`, testes em Postgres ([LEIA-ME](../servidor-central/LEIA-ME.md)) |
| `site-fonte/` → `site/` | o site, gerado por `scripts/gerar-site.py` |
| `docs/` | documentação |

## 7. Passagem para o Supabase

| Fase | O quê | Estado |
|---|---|---|
| 1 | Este documento | feito |
| 2 | Esquemas e tabelas (migração em `servidor-central/supabase/migrations`) | preparada e testada em Postgres local; aplica-se quando houver acesso ao projecto |
| 3 | Função `kangest`: o mesmo `worker.js`, as mesmas rotas | preparada e testada em Postgres local; publica-se quando houver acesso ao projecto |
| 4 | Copiar os dados da D1 (`servidor-central/copiar-d1.mjs`) e pôr o Worker antigo a encaminhar os pedidos para o endereço novo (`servidor-licencas/src/encaminhar.js`, com `publicar-licencas.py --encaminhar`) | preparada e testada em Postgres local; faz-se depois da fase 3 |
| 5 | O site e uma versão nova do KanGest com o endereço novo | por fazer |

O roteiro das fases 3 a 5, comando a comando, está no [LEIA-ME do servidor-central](../servidor-central/LEIA-ME.md).

O Worker antigo continua de pé até todos os KanGest instalados terem a versão com o endereço novo. Até lá, só
encaminha os pedidos. Leva o IP e o país de quem pediu em cabeçalhos próprios, que a função só aceita com o
`ENCAMINHADOR_SEGREDO`.

A seguir: `seguranca.auditoria`, com o registo do que o produtor e os donos das empresas mudam.

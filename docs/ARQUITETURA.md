# Arquitectura

Uma regra para tudo: **cada coisa vive num só sítio e há uma só forma de lhe chegar.** Este documento é a
referência. Quando uma peça muda de sítio, este documento muda no mesmo pull request.

## 1. Três camadas

| Camada | O que guarda | Onde vive | Tecnologia |
|---|---|---|---|
| **Dados da empresa** | vendas, stock, caixa, SAF-T, AGT, utilizadores e PINs | no computador servidor de cada loja | SQLite (`kigest.db`), funciona sem Internet |
| **Servidor de licenças** | empresas (NIF), licenças, pagamentos, contas, equipa gerida, sessões | Cloudflare | um Worker + uma base D1 |
| **Site** | páginas, transferências, área do produtor (`/admin.html`), conta da empresa (`/conta.html`) | Cloudflare Pages | estático, sem base de dados |

O servidor de licenças só guarda o que serve para licenciar, cobrar e gerir a equipa. Os dados de negócio (vendas,
clientes da loja, stock) ficam no computador da empresa. Só saem nas cópias de segurança que ela própria configura.

Tudo o que o KanGest tem na Internet está numa só conta da Cloudflare: o servidor de licenças, o site, os túneis do
acesso pela Internet, a verificação de humano (Turnstile) e o DNS.

## 2. Produtos

| Produto | Backend | Site |
|---|---|---|
| KanGest | Cloudflare: Worker `kangest-licencas` + D1 `kangest-licencas` | kangest.pages.dev |
| Livraria | projecto Supabase próprio | livrariaadventista.ao |
| SIGA Plus | projecto Supabase próprio | portal-siga.com |

Os produtos não partilham bases de dados. A Livraria fala com o KanGest só pela API de integração do KanGest
([API.md](API.md)).

## 3. Servidor de licenças

O servidor é um ficheiro, [`servidor-licencas/src/worker.js`](../servidor-licencas/src/worker.js), com a base D1
descrita em [`schema.sql`](../servidor-licencas/schema.sql). Endereço: `https://kangest-licencas.valentinocanguele.workers.dev`.

| Rota | Quem a usa |
|---|---|
| `POST /v1/registo`, `/v1/confirmar`, `/v1/renovar` | o KanGest (registo, experiência, licença, estado, equipa gerida) |
| `GET /v1/planos` | o site (planos e preços) |
| `/v1/web/…` | o KanGest e o site (acesso pela Internet, verificação de humano) |
| `/v1/conta/…` | a conta da empresa no site (`/conta.html`) |
| `/admin/api/…` | a área do produtor no site (`/admin.html`) |

### Tabelas, por área

A D1 é SQLite e não tem esquemas: as áreas são as secções do `schema.sql`.

| Área | Tabelas | Para quê |
|---|---|---|
| empresas | `clientes`, `contas`, `planos` | a empresa (pelo NIF), a conta do dono no site, os planos à venda |
| licenças | `maquinas`, `instalacoes`, `licencas`, `pagamentos`, `codigos` | computadores registados e o estado de cada um, licenças emitidas, pagamentos, códigos de registo |
| equipa | `funcionarios`, `lojas_geridas`, `postos_geridos`, `equipa_versoes`, `equipas` | a equipa gerida no site e a que cada KanGest comunica |
| segurança | `sessoes_painel`, `sessoes_conta`, `pedidos_conta`, `codigos_conta`, `segredos_maquina`, `limites` | sessões, contas novas à espera do código, códigos da conta, segredos dos computadores, limite de pedidos por IP |
| web | `subdominios` | acesso pela Internet (um subdomínio por empresa) |

### Quem chega aos dados

- **Só o Worker**, pela ligação `DB` à D1. A D1 não tem API pública: fora do Worker, só com um token da conta da
  Cloudflare (o produtor).
- O servidor tem a sua própria autenticação:
  - o produtor: chave (`ADMIN_TOKEN`) ou sessão `s1.`, com email e senha;
  - o dono da empresa: sessão `c1.`;
  - cada computador: o seu segredo;
  - o registo e a conta: códigos de 6 algarismos.
- Limite de pedidos por IP e por hora em todas as rotas que enviam emails, testam códigos ou aceitam senhas.

### Operação

- **Publicar:** `python3 scripts/publicar-licencas.py`, que aplica o `schema.sql` e envia o código, mantendo os
  segredos ([LICENCAS.md](LICENCAS.md)).
- **Emails:** o Worker envia quatro, todos com o visual do site e uma versão em texto: o código de registo, o código
  da conta, o aviso ao produtor de cada registo e as boas-vindas no primeiro registo de uma empresa. Saem pela
  Cloudflare Email Service a partir de `kangest.livrariaadventista.com`; nunca para domínios de teste (`.invalid`,
  `.test`).
- **Prova de fumo** depois de cada publicação: `node servidor-licencas/teste/fumo.mjs`. Faz o percurso de um cliente em
  produção com uma empresa de teste, que se apaga no fim.
- **Vigia** ([`.github/workflows/vigia.yml`](../.github/workflows/vigia.yml)): de 6 em 6 horas confirma que o servidor e
  o site respondem. Se falharem, o GitHub avisa por email.
- **Cópias de segurança:**
  - a D1 guarda sozinha os últimos 7 dias, e volta-se a qualquer minuto com
    `npx wrangler d1 time-travel restore kangest-licencas --timestamp=…`;
  - para guardar mais tempo e fora da Cloudflare, `node servidor-licencas/copia-d1.mjs` grava uma cópia em
    `~/.kangest-licencas/copias/`, só legível pelo produtor;
  - `copia-d1.mjs ensaiar` confirma que a cópia se restaura, e `npx wrangler d1 execute kangest-licencas --remote --file
    <cópia>` restaura-a.

### Segredos do Worker

Definem-se com o `publicar-licencas.py` (ou `wrangler secret put`), nunca no repositório:

| Variável | Para quê | Obrigatória |
|---|---|---|
| `CHAVE_PRIVADA` | assina as licenças (Ed25519, PEM pkcs8) | sim |
| `ADMIN_TOKEN` | chave de emergência do produtor e chave das sessões | sim |
| `ADMIN_EMAIL`, `ADMIN_SENHA` | entrada do produtor com email e senha (a senha só como resumo PBKDF2) | não |
| `ADMIN_ORIGENS` | sites que podem chamar `/admin/api` e `/v1/conta` (por omissão `https://kangest.pages.dev`) | não |
| `EMAIL` (ligação `send_email`), `EMAIL_REMETENTE` | os emails pela Cloudflare Email Service, a partir de `kangest.livrariaadventista.com`: os códigos, o aviso ao produtor e as boas-vindas (sem eles, nenhum email sai e os códigos aparecem na área do produtor) | não |
| `RESEND_API_KEY` | alternativa à Cloudflare Email Service para os mesmos emails | não |
| `TURNSTILE_SITE_KEY`, `TURNSTILE_SECRET_KEY` | verificação de humano | não |
| `WEB_DOMINIO`, `CF_API_TOKEN`, `CF_CONTA`, `CF_ZONA` | acesso pela Internet (túneis e DNS na Cloudflare) | não |
| `ACCESS_EQUIPA`, `ACCESS_AUD` | Cloudflare Access à frente do painel | não |

## 4. Convenções

- **Nomes:** português, `snake_case`, tabelas no plural.
- **Datas:** texto ISO 8601 em UTC (`2026-11-22T10:40:00Z`); os dias em `AAAA-MM-DD` (dia de Luanda). Os momentos
  acabam em `_em`.
- **Dinheiro:** cêntimos de kwanza, inteiros, nunca decimais.
- **Indicadores:** inteiros 0/1 (`activo`, `bloqueada`).
- **Chaves:**
  - a empresa é sempre o NIF;
  - um computador é o NIF + o código da máquina;
  - o que se gere no site tem um `id` do site, que no KanGest fica em `conta_id`.
- **Segredos:** nunca no código nem em claro na base, só resumos:
  - SHA-256 nos códigos e nos segredos dos computadores;
  - PBKDF2 nas senhas do site;
  - Argon2 no KanGest.
- **Endereços:** um sítio por cliente da API:
  - no site, `site-fonte/config.json` (`web.licencas`), de onde o vigia e a prova de fumo também o lêem;
  - no programa, `SERVIDOR` em `src-tauri/src/services/licenca.rs`.
- **Estrutura da base:**
  - KanGest: só por migrações numeradas (`src-tauri/migrations/NNNN_….sql`), e uma migração aplicada nunca se altera
    (corrige-se com outra);
  - servidor de licenças: o `schema.sql` só tem `CREATE … IF NOT EXISTS` e aplica-se em cada publicação, por isso uma
    tabela nova entra sozinha. Uma coluna nova numa tabela que já existe pede também um `ALTER TABLE`, aplicado uma vez
    à D1 com `npx wrangler d1 execute`.

## 5. Contas

| Quem | Onde entra | Onde fica a conta |
|---|---|---|
| Produtor | `/admin.html` no site | servidor de licenças (email e senha; sessões em `sessoes_painel`) |
| Dono da empresa | `/conta.html` no site | servidor de licenças (`contas`; sessões em `sessoes_conta`) |
| Funcionários | no KanGest (login e senha ou PIN, sem Internet) | `kigest.db`. São geridos na conta da empresa (área equipa) e o KanGest aplica-os na verificação |

## 6. Onde está cada coisa

| Pasta | O quê |
|---|---|
| `src-tauri/` | o programa (Rust): base local, regras, API do KanGest |
| `src/` | a interface (React) |
| `servidor-licencas/` | o servidor de licenças: `src/worker.js`, `schema.sql`, testes, cópia de segurança, prova de fumo ([LEIA-ME](../servidor-licencas/LEIA-ME.md)) |
| `site-fonte/` → `site/` | o site, gerado por `scripts/gerar-site.py` |
| `docs/` | documentação |

## 7. O que fica do Supabase

Em 22/11/2026 o servidor de licenças passou por um projecto Supabase (`fissktkxilseymadzppn`) e voltou à Cloudflare no
mesmo dia. Nunca teve dados de clientes. Fica só um **encaminhamento**
([`servidor-licencas/encaminhamento-supabase`](../servidor-licencas/encaminhamento-supabase/LEIA-ME.md)). O KanGest
1.4.0 saiu com o endereço do Supabase, e o encaminhamento leva os pedidos dessas instalações ao Worker. A 1.4.1 volta a
falar directamente com o Worker.

O projecto Supabase apaga-se quando já não houver KanGest 1.4.0 instalado.

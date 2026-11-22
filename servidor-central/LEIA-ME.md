# Backend central do KanGest no Supabase

O servidor de licenças ([`servidor-licencas/src/worker.js`](../servidor-licencas/src/worker.js)) a correr como uma
Edge Function do Supabase, com a base em Postgres. As rotas e o código são os mesmos. O mapa geral está em
[`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md).

| Ficheiro | O quê |
|---|---|
| `supabase/migrations/` | as tabelas, por esquema (`empresas`, `licencas`, `equipa`, `seguranca`, `web`). Uma migração aplicada nunca se altera |
| `supabase/functions/kangest/index.ts` | a entrada no Deno: liga ao Postgres (`SUPABASE_DB_URL`) e serve os pedidos |
| `supabase/functions/kangest/entrada.js` | entrega cada pedido ao `worker.js` como o Worker o recebia: sem o prefixo `/kangest`, com o IP e o país (também os encaminhados pelo Worker antigo). `/admin` vai para o site |
| `supabase/functions/kangest/d1.js` | a API do D1 sobre Postgres (marcadores, esquemas, tipos, transacções) |
| `supabase/functions/kangest/nucleo/` | cópia do `worker.js` e do painel, feita por `preparar.mjs`; não vai para o repositório |
| `supabase/config.toml` | `verify_jwt = false` para a função, que tem a sua própria autenticação |
| `copiar-d1.mjs` | a cópia dos dados da D1 para o Postgres: exportar, ensaiar, importar (e restaurar uma cópia de segurança) |
| `copia-supabase.mjs` | a cópia de segurança do Postgres do Supabase para o computador do produtor |
| `teste/` | testes em Postgres local (PGlite), sem Docker nem rede; `teste/fumo.mjs` é a prova de fumo em produção |

## Testar

```bash
cd servidor-central
npm ci                                                        # uma vez
npm run testar                                                # consultas, função, cópia da D1 e contas em Postgres
KANGEST_BD=postgres node ../servidor-licencas/teste/testar.mjs  # o teste completo do servidor, em Postgres
```

- `teste/consultas.mjs` compila cada consulta do `worker.js` no SQLite e no Postgres. Também confirma que as tabelas e
  as colunas são as mesmas no `schema.sql`, nas migrações e em `ESQUEMAS` do `d1.js`.
- `teste/funcao.mjs` faz pedidos como o Supabase os entrega e percorre o caminho completo:
  - registo e licença assinada;
  - renovação e pagamentos;
  - bloqueio de um computador;
  - conta da empresa;
  - CORS e limite por IP;
  - pedidos encaminhados pelo Worker antigo (`encaminhar.js`).
- `teste/copia.mjs` exporta uma D1 imitada pela API da Cloudflare e importa-a no Postgres. Confirma que as linhas ficam
  iguais, valor a valor, e que o servidor continua a trabalhar sobre elas: a mesma licença, os pagamentos a continuar a
  numeração.
- Com `KANGEST_BD=postgres`, os testes de `servidor-licencas/teste` (`contas.mjs`, `testar.mjs`) correm no Postgres.
  Sem a variável, correm no SQLite, como o D1.

## Publicar a função (fase 3; feita em 22/11/2026)

O projecto está na organização «Kangest» do Supabase. O acesso do CLI é por um token pessoal dessa conta, guardado fora
do repositório em `~/.kangest-licencas/supabase-token.txt` (chmod 600). Assim, o login normal do CLI (Livraria, Sga)
fica como está. Não é preciso a senha da base, nem Docker.

```bash
cd servidor-central
export SUPABASE_ACCESS_TOKEN="$(cat ~/.kangest-licencas/supabase-token.txt)"
supabase link --project-ref fissktkxilseymadzppn
supabase db push --linked                                # aplica as migrações novas
node preparar.mjs                                        # copia o núcleo para a função
# os segredos, dos mesmos ficheiros que o Worker usa (fora do repositório); o do encaminhamento cria-se uma vez
S=~/.kangest-licencas/servidor
[ -f $S/encaminhador-segredo.txt ] || (python3 -c "import secrets; print(secrets.token_urlsafe(32))" > $S/encaminhador-segredo.txt && chmod 600 $S/encaminhador-segredo.txt)
supabase secrets set CHAVE_PRIVADA="$(tr -d '\n' < $S/privada.pem)" ADMIN_TOKEN="$(cat $S/admin-token.txt)" \
    ADMIN_EMAIL="$(cat $S/admin-email.txt)" ADMIN_SENHA="$(cat $S/admin-senha.txt)" \
    ENCAMINHADOR_SEGREDO="$(cat $S/encaminhador-segredo.txt)"
# e os opcionais que estiverem em uso no Worker (RESEND_API_KEY, TURNSTILE_*, WEB_DOMINIO…: docs/ARQUITETURA.md)
supabase functions deploy kangest --project-ref fissktkxilseymadzppn --no-verify-jwt --use-api
```

Para actualizar a função depois de mudar o `worker.js`: `node preparar.mjs` e o mesmo `functions deploy`.

Depois de publicar (confirmado na primeira publicação):

1. `https://fissktkxilseymadzppn.supabase.co/functions/v1/kangest/v1/planos` responde com os planos.
2. Depois de um pedido, `select chave from seguranca.limites` mostra o IP público de quem pediu, não um IP interno. É
   assim que se confirma que o limite por IP vê o IP certo.
3. Os esquemas do KanGest **não** se juntam a *Exposed schemas* nas definições da API.

## Passagem dos dados e do Worker antigo (fase 4)

Em 22/11/2026 a D1 não tinha dados de clientes: 0 clientes, 0 computadores, 0 licenças e 0 pagamentos, só os 3 planos
de origem, sem preço. Se ainda for assim no dia da passagem, a cópia não é precisa: basta publicar a função e correr o
`--encaminhar` (mais abaixo). O primeiro comando do ensaio mostra as contagens e diz se é o caso.

**Ensaio**, com tempo e sem tocar no Supabase. Usa o mesmo token do `publicar-licencas.py`:

```bash
cd servidor-central
CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… node copiar-d1.mjs exportar   # grava ~/.kangest-licencas/d1-<data>.json
node copiar-d1.mjs ensaiar ~/.kangest-licencas/d1-<data>.json
```

O ensaio importa a cópia num Postgres local e confirma que o servidor lê o resumo e a ficha de cada cliente. A cópia
tem os dados dos clientes: fica em `~/.kangest-licencas`, só legível pelo produtor, e nunca entra no repositório.

**A passagem**, fora de horas. Leva poucos minutos:

```bash
cd servidor-central
CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… node copiar-d1.mjs exportar
KANGEST_DB_URL='postgresql://…' node copiar-d1.mjs importar ~/.kangest-licencas/d1-<data>.json   # --substituir se já lá houver um ensaio
cd ..
CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… python3 scripts/publicar-licencas.py \
    --encaminhar https://fissktkxilseymadzppn.supabase.co/functions/v1/kangest
```

- `KANGEST_DB_URL` é a ligação ao Postgres do Supabase (*Project Settings → Database → Connection string*,
  «Session pooler»). Tem a senha da base: passe-a só na linha do comando, nunca num ficheiro do repositório.
- A importação corre numa só transacção: se algo falhar, nada fica a meio.
- O que a D1 receber entre a exportação e o `--encaminhar` perde-se. As renovações repetem-se sozinhas, mas um registo
  feito nesse intervalo tem de ser confirmado outra vez.
- Depois do `--encaminhar`, o Worker `kangest-licencas` só encaminha para a função
  ([`encaminhar.js`](../servidor-licencas/src/encaminhar.js)). Os KanGest instalados e o site continuam a funcionar
  pelo endereço antigo. O `publicar-licencas.py` normal passa a recusar-se, porque voltava a pôr o `worker.js` a
  escrever na D1.
- A D1 fica como está, só como cópia de segurança, até a fase 5 chegar a todos os clientes.

Confirmar: `https://kangest-licencas.valentinocanguele.workers.dev/v1/planos` responde (agora pelo Supabase), e
`select chave from seguranca.limites` mostra os IPs públicos de quem pede, não os da Cloudflare.

## Endereço novo no site e no programa (fase 5)

- Site: `web.licencas` em `site-fonte/config.json` passa a ser o endereço da função. A área do produtor (`/admin.html`)
  e a conta da empresa (`/conta.html`) passam a chamá-la directamente.
- Programa: `SERVIDOR` em `src-tauri/src/services/licenca.rs` passa a ser o endereço da função, numa versão nova.

## Verificar em produção

**Prova de fumo**, depois de cada publicação da função:

```bash
node servidor-central/teste/fumo.mjs
```

Faz o que um cliente faz, com uma empresa de teste (NIF «FUMO…»), e no fim apaga tudo o que criou:
- registo, código e confirmação;
- licença assinada, verificada com a chave pública do KanGest;
- renovação com o estado e a equipa;
- conta da empresa: código, senha, entrada, resumo e saída;
- equipa gerida, que chega ao KanGest e fica aplicada;
- ficha do cliente na área do produtor;
- a entrada do produtor recusa uma senha errada.

Diz também se o Worker antigo já encaminha para o Supabase. Usa a chave do produtor
(`~/.kangest-licencas/servidor/admin-token.txt`) e, para apagar, o token do Supabase.

**Vigia** ([`.github/workflows/vigia.yml`](../.github/workflows/vigia.yml)): de 6 em 6 horas confirma que a função, o
Worker antigo e o site respondem. Se algum falhar, o GitHub avisa por email.

## Plano gratuito do Supabase: pausa e cópias de segurança

O projecto está no plano gratuito, e isso traz dois cuidados:

- **Pausa ao fim de 7 dias sem actividade.** O vigia lê os planos da base de 6 em 6 horas, o que mantém o projecto
  activo. Se mesmo assim for pausado, o Supabase avisa por email e o projecto reactiva-se no painel.
- **Sem cópias de segurança que se possam descarregar.** Faça-as no computador do produtor:

  ```bash
  node servidor-central/copia-supabase.mjs        # ~/.kangest-licencas/copias/supabase-<data>.json (guarda as 30 últimas)
  node servidor-central/copiar-d1.mjs ensaiar ~/.kangest-licencas/copias/supabase-<data>.json   # confirma que se restaura
  ```

  Restaurar, só se for preciso:
  `KANGEST_DB_URL='postgresql://…' node servidor-central/copiar-d1.mjs importar <cópia> --substituir`.

  A cópia só lê a base. Tem os dados dos clientes, por isso fica fora do repositório e só o produtor a lê.

No plano Pro, o Supabase faz cópias diárias e não pausa os projectos.

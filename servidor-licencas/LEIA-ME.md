# Servidor de licenças do KanGest

Cloudflare Worker + D1, sem dependências. Faz quatro coisas:
- regista os KanGest instalados, com confirmação por código, e dá a experiência uma vez por NIF;
- guarda os pagamentos e entrega as licenças (`KG1.…`, assinadas com Ed25519);
- serve a conta da empresa e a equipa gerida no site, e cria contas novas antes de instalar o programa;
- serve a área do produtor.

O mapa geral está em [`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md).

| Ficheiro | O quê |
|---|---|
| `src/worker.js` | API pública (`/v1/registo`, `/v1/confirmar`, `/v1/renovar`, `/v1/planos`, `/v1/web/…`, `/v1/conta/…`) e do produtor (`/admin/api/…`) |
| `src/painel.html` | Painel do produtor (`/admin`). **Depois de o alterar:** `node construir.mjs` gera `src/painel.js` |
| `schema.sql` | As tabelas, arrumadas por área (empresas, licenças, equipa, segurança, web). Só `CREATE … IF NOT EXISTS` |
| `copia-d1.mjs` | Cópia de segurança da D1 para o computador do produtor, e o ensaio do restauro |
| `encaminhamento-supabase/` | Temporário: leva ao Worker os pedidos dos KanGest 1.4.0 ([LEIA-ME](encaminhamento-supabase/LEIA-ME.md)) |
| `teste/testar.mjs` | Teste completo sem rede (D1 imitada com `node:sqlite`) |
| `teste/contas.mjs` | Contas das empresas no site e equipa gerida |
| `teste/conta-nova.mjs` | A conta criada no site antes de instalar: código, «por registar», experiência no primeiro registo, NIF de outra empresa |
| `teste/emails.mjs` | Os emails: quais saem, para quem, HTML escapado, versão em texto, domínios de teste de fora |
| `teste/copia.mjs`, `teste/encaminhamento.mjs` | A cópia de segurança e o encaminhamento no Supabase |
| `teste/fumo.mjs` | Prova de fumo em produção, com uma empresa de teste que se apaga no fim |
| `teste/servidor-local.mjs` | O Worker servido pelo Node, para testar o KanGest (`licencas_servidor_local` no Rust) |

## Testar

```bash
node servidor-licencas/teste/testar.mjs
node servidor-licencas/teste/contas.mjs
node servidor-licencas/teste/conta-nova.mjs
node servidor-licencas/teste/copia.mjs
node servidor-licencas/teste/emails.mjs
node servidor-licencas/teste/encaminhamento.mjs
```

O CI corre-os em cada push (job `servidor`).

## Publicar e verificar

```bash
CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-licencas.py      # aplica o schema.sql e envia o código (docs/LICENCAS.md)
node servidor-licencas/teste/fumo.mjs                             # o percurso de um cliente em produção
```

- A prova de fumo usa a chave do produtor (`~/.kangest-licencas/servidor/admin-token.txt`). Para apagar a empresa de
  teste, usa o token da Cloudflare: `~/.kangest-licencas/cloudflare-token.txt` ou `CLOUDFLARE_API_TOKEN`.
- Segredos no Worker: `CHAVE_PRIVADA` e `ADMIN_TOKEN`; opcionais `ADMIN_EMAIL` e `ADMIN_SENHA` (`--senha-painel`),
  `RESEND_API_KEY` e `EMAIL_REMETENTE`, `ACCESS_EQUIPA` e `ACCESS_AUD` e o resto da lista em
  [`docs/ARQUITETURA.md`](../docs/ARQUITETURA.md).

## Emails

O Worker envia estes emails, todos com o visual do site (HTML) e uma versão em texto:
- o código de registo de um computador;
- o código da conta da empresa, e o código para criar uma conta nova no site;
- «este email já tem conta», a quem pede uma conta nova com um email que já tem (o site não o revela);
- o aviso ao produtor de cada pedido de registo (com o código, se não chegou à empresa) e de cada conta nova;
- as boas-vindas no primeiro registo de uma empresa.

Saem pela Cloudflare Email Service, a partir do subdomínio `kangest.livrariaadventista.com`. Para ligar, uma vez:
1. no painel da Cloudflare, o plano **Workers pago** (a Email Sending só envia para qualquer destinatário nesse plano);
2. **Compute → Email Service → Email Sending → Onboard Domain**: `kangest.livrariaadventista.com` (a Cloudflare põe
   os registos DNS sozinha);
3. publicar com o remetente:

   ```bash
   CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-licencas.py --email-cloudflare "KanGest <codigos@kangest.livrariaadventista.com>"
   ```

O remetente fica em `~/.kangest-licencas/servidor/email-remetente.txt`, e as publicações seguintes mantêm a ligação.
Sem a ligação nem o Resend, nenhum email sai e os códigos aparecem na área do produtor. Para domínios de teste
(`.invalid`, `.test`, `.example`) nunca sai nada: a prova de fumo não gera devoluções.

## Conta nova no site

Uma empresa pode criar a conta no site (`conta.html#criar`) antes de instalar o KanGest:
1. `POST /v1/conta/criar` com empresa, NIF, email e telefone: o código vai para o email (fica em `pedidos_conta`, só o
   resumo). Um NIF já registado é recusado; um email que já tem conta recebe o aviso, com a mesma resposta.
2. `POST /v1/conta/criar/confirmar` com o código e a senha: nascem a empresa e a conta, e a sessão abre-se logo.
3. A empresa fica **por registar** (`POR_REGISTAR`): a experiência de 30 dias só começa quando o programa se regista com
   esse NIF (`/v1/registo` e `/v1/confirmar`), e as boas-vindas saem nesse dia.

Se alguém criar a conta com o NIF de outra empresa, ela desfaz-se quando a empresa registar o programa com outro
email (o código do registo prova quem é dono dele): as sessões terminam e quem tem o email do registo cria a conta em
«Activar a conta». Na área do produtor, a data da experiência não se apaga depois de começar; para a terminar,
põe-se uma data passada.

## Cópias de segurança

A D1 guarda sozinha os últimos 7 dias (Time Travel, plano gratuito):

```bash
npx wrangler d1 time-travel restore kangest-licencas --timestamp=2026-11-22T10:00:00Z   # volta a esse minuto
```

Para guardar mais tempo e fora da Cloudflare:

```bash
node servidor-licencas/copia-d1.mjs                                     # ~/.kangest-licencas/copias/d1-<data>.sql (guarda as 30 últimas)
node servidor-licencas/copia-d1.mjs ensaiar ~/.kangest-licencas/copias/d1-<data>.sql   # confirma que se restaura
npx wrangler d1 execute kangest-licencas --remote --file ~/.kangest-licencas/copias/d1-<data>.sql   # restaura
```

A cópia só lê a D1. Tem os dados dos clientes: fica fora do repositório e só o produtor a lê.

## Protecção

- Limite de pedidos por IP e por hora (tabela `limites`, constantes `LIMITES`).
- Cloudflare Access à frente do painel (com `ACCESS_EQUIPA` e `ACCESS_AUD`).
- Cabeçalhos de segurança em todas as respostas.

Ver [`docs/CLOUDFLARE.md`](../docs/CLOUDFLARE.md).

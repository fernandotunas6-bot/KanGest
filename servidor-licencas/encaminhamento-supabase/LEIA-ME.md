# Encaminhamento no Supabase (temporário)

O KanGest 1.4.0 saiu com o servidor de licenças no Supabase (projecto `fissktkxilseymadzppn`), que no mesmo dia voltou
à Cloudflare ([`docs/ARQUITETURA.md`](../../docs/ARQUITETURA.md), secção 7). Esta função, `kangest`, fica no endereço
que a 1.4.0 conhece e leva cada pedido, tal como veio, ao Worker da Cloudflare. Não tem base de dados: os dados estão
todos na D1.

| Ficheiro | O quê |
|---|---|
| `supabase/functions/kangest/encaminhar.js` | o encaminhamento (testa-se no Node: `node servidor-licencas/teste/encaminhamento.mjs`) |
| `supabase/functions/kangest/index.ts` | a entrada no Deno |
| `supabase/config.toml` | `verify_jwt = false`: o KanGest não manda o JWT do Supabase |

O Worker vê o IP do Supabase em vez do da loja, por isso as instalações 1.4.0 partilham o limite de pedidos por hora.
Para as poucas que houver, chega.

## Publicar

```bash
export SUPABASE_ACCESS_TOKEN="$(cat ~/.kangest-licencas/supabase-token.txt)"
supabase functions deploy kangest --workdir servidor-licencas/encaminhamento-supabase \
    --project-ref fissktkxilseymadzppn --no-verify-jwt --use-api
```

## Quando apagar

Quando já não houver KanGest 1.4.0 instalado (actualizam-se para a 1.4.1 pelas actualizações automáticas). Nessa
altura apagam-se o projecto Supabase, esta pasta, o passo do vigia (`.github/workflows/vigia.yml`) e o teste
`teste/encaminhamento.mjs`.

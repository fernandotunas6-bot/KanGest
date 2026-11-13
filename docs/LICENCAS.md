# Licenças do KanGest

O KanGest confirma a licença no próprio computador, sem internet. A licença é um texto que começa por `KG1.`,
assinado pelo produtor; ninguém consegue alterar as datas, o plano ou os postos sem estragar a assinatura.

## Para o cliente

| Situação | O que acontece |
|---|---|
| Acabou de instalar | 30 dias de experiência com tudo. Aviso nos últimos 15 dias. |
| Licença activa | Usa normalmente. Aviso nos últimos 15 dias para renovar. |
| Assinatura terminada | Mais 15 dias de tolerância a facturar, com aviso. |
| Depois da tolerância | **Modo de consulta**: vê, imprime 2.ªs vias, relatórios e SAF-T; não emite documentos novos. |
| Relógio atrasado | Não emite até acertar a data e a hora (o KanGest guarda a última data vista). |
| Licença de outro NIF ou computador | É recusada; o ecrã explica porquê. |

Tudo se faz em **Sistema → Licença e Assinatura**: estado, código do pedido (`KG-<NIF>-<computador>`) para copiar,
botão para pedir a licença no site e o campo para colar a licença recebida.

## Para o produtor

A ferramenta está em `scripts/licencas/kangest-licencas.mjs` (Node, sem dependências).

```bash
# uma vez: cria o par de chaves em ~/.kangest-licencas/ (a pública já está no KanGest)
node scripts/licencas/kangest-licencas.mjs chaves

# emitir: NIF e empresa do código do pedido; --maquina prende a licença a um computador (opcional)
node scripts/licencas/kangest-licencas.mjs emitir --nif 5417189230 --empresa "Loja X, Lda" \
     --plano COMERCIO --postos 3 --meses 12
#   planos: ESSENCIAL (1 posto), COMERCIO (3 postos), EMPRESA (sem limite); --postos muda o número
#   --ate 2027-12-31 em vez de --meses; --tolerancia 15 (dias); --inicio AAAA-MM-DD

# ver o conteúdo de uma licença e confirmar a assinatura
node scripts/licencas/kangest-licencas.mjs ver KG1.…
```

Cada licença emitida fica registada em `~/.kangest-licencas/emitidas.csv`.

**A chave privada (`~/.kangest-licencas/privada.pem`) nunca vai para o repositório**, que é público. Guarde uma cópia
num sítio seguro (pen cifrada, gestor de senhas): sem ela não se emitem licenças que as instalações aceitem. Se for
roubada, gera-se outra, põe-se a pública nova em `CHAVES_PUBLICAS` (`src-tauri/src/services/licenca.rs`) e publica-se
uma versão nova do KanGest; as licenças antigas continuam válidas enquanto a chave antiga estiver na lista.

## Regras técnicas

- A verificação está em `documentos::emitir`: vale para o POS, a gestão, as compras, os recibos e a oficina.
- O limite de postos aplica-se ao criar um posto (`posto.guardar`).
- A experiência conta desde a criação da empresa; nas bases antigas, desde a primeira vez que a versão com licenças
  abre. Os 30 dias estão no programa (`DIAS_EXPERIENCIA`), não na configuração que o cliente pode alterar.
- Em rede, a licença é a do servidor: os postos pedem-lhe o estado e é ele que emite os documentos.

## Próximos passos

1. Servidor de licenças (Cloudflare Workers + D1): registo com confirmação de email, experiência emitida pelo
   servidor e renovação automática quando há internet.
2. Pagamentos por referência Multicaixa e painel de vendas.

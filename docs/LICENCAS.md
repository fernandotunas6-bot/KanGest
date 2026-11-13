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

Tudo se faz em **Sistema → Licença e Assinatura**: estado, registo online, código do pedido (`KG-<NIF>-<computador>`)
para copiar, botão para pedir a licença no site e o campo para colar a licença recebida.

### Com internet: registo online

1. O administrador escreve o email da empresa (e, se quiser, o telefone) e carrega em **Enviar código de confirmação**.
2. Recebe um código de 6 algarismos (por email; enquanto o envio de email não estiver configurado, o produtor vê o
   código no painel e dá-o por telefone ou WhatsApp). Escreve-o e confirma.
3. O KanGest fica registado: recebe a experiência associada ao NIF (uma só vez por NIF, mesmo que reinstale) e, depois
   de cada pagamento, a licença nova chega sozinha. Verifica no servidor no máximo a cada 12 horas, ou em
   **Verificar agora**. Sem internet, continua a funcionar com a licença que já tem.

### Sem internet: licença colada

Envia o código do pedido, paga e recebe a licença (`KG1.…`) por email ou WhatsApp para colar em
**Activar com o texto da licença**.

## Para o produtor

### Servidor de licenças e painel

- Servidor: `https://kangest-licencas.valentinocanguele.workers.dev` (Cloudflare Workers + D1, código em `servidor-licencas/`).
- Painel: `https://kangest-licencas.valentinocanguele.workers.dev/admin`, com a chave de
  `~/.kangest-licencas/servidor/admin-token.txt`.
  - **Clientes**: estado de cada NIF, computadores, licenças emitidas; abrir um cliente para **registar um pagamento**
    (o valor vem da tabela de preços; a assinatura soma a partir do fim já pago), bloquear um computador, mudar os
    postos ou o número de computadores permitidos (2 por omissão).
  - **Pedidos de registo**: os códigos por confirmar (visíveis enquanto o email não estiver configurado).
  - **Planos e preços**: preços mensal e anual em Kz; aparecem também em `GET /v1/planos` (para o site).
  - **Emitir licença**: licença manual para quem não tem internet (cole o código do pedido `KG-…` para preencher).
- Publicar ou actualizar: `CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-licencas.py` (cria a base D1, aplica o
  esquema, envia o código; os segredos só na primeira vez ou com `--segredos`). Email dos códigos (Resend):
  `--email-api re_… --remetente "KanGest <licencas@dominio>"`.
- Testes sem rede: `cd servidor-licencas && node teste/testar.mjs`.

O servidor assina com uma chave **própria** (`~/.kangest-licencas/servidor/privada.pem`, guardada também como segredo
no Cloudflare), diferente da chave do produtor. Se a conta Cloudflare for comprometida, tira-se só essa chave de
`CHAVES_PUBLICAS` numa versão nova; as licenças emitidas à mão continuam válidas.

### Licenças à mão (ferramenta de linha de comandos)

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

**As chaves privadas (`~/.kangest-licencas/privada.pem` e `~/.kangest-licencas/servidor/`) nunca vão para o repositório**, que é público. Guarde uma cópia
num sítio seguro (pen cifrada, gestor de senhas): sem ela não se emitem licenças que as instalações aceitem. Se for
roubada, gera-se outra, põe-se a pública nova em `CHAVES_PUBLICAS` (`src-tauri/src/services/licenca.rs`) e publica-se
uma versão nova do KanGest; as licenças antigas continuam válidas enquanto a chave antiga estiver na lista.

## Regras técnicas

- A verificação está em `documentos::emitir`: vale para o POS, a gestão, as compras, os recibos e a oficina.
- O limite de postos aplica-se ao criar um posto (`posto.guardar`).
- A experiência conta desde a criação da empresa; nas bases antigas, desde a primeira vez que a versão com licenças
  abre. Os 30 dias estão no programa (`DIAS_EXPERIENCIA`), não na configuração que o cliente pode alterar.
- Em rede, a licença é a do servidor: os postos pedem-lhe o estado e é ele que emite os documentos. O registo online
  faz-se também no servidor (é o computador dele que fica registado).
- Ao receber uma licença do servidor, o KanGest nunca troca uma licença válida que dure mais (por exemplo, emitida à
  mão) por uma mais curta.

## Próximos passos

1. Envio dos códigos por email (conta Resend com um domínio verificado) e um domínio próprio para o servidor.
2. Pagamentos por referência Multicaixa (ProxyPay ou outro intermediário): o pagamento confirmado renova sozinho.
3. Preços dos planos no site, lidos de `/v1/planos`.

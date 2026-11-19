# API de integração

A API deixa outros sistemas trabalharem com o KanGest: a loja online lê os artigos, os preços e o stock e emite as
facturas das encomendas; a contabilidade descarrega os documentos e o SAF-T; um painel de BI lê as vendas; uma app ou um
verificador de preços procura artigos pelo código de barras.

Cada sistema liga-se com uma **chave de acesso** própria, criada no KanGest. A chave age em nome de um utilizador e só
faz o que as suas permissões deixam. Os documentos emitidos pela API seguem exactamente as regras do programa: série do
posto, assinatura AGT, stock, conta corrente, licença e Facturação Electrónica.

A descrição técnica completa (OpenAPI 3, para importar no Postman ou no Swagger) está em [openapi.json](openapi.json)
e é servida pela própria API em `GET /api/v1/openapi.json`.

## 1. Ligar a API

A API faz parte do **plano Empresa** e está disponível durante os 30 dias de experiência. Nos planos Essencial e
Comércio acrescenta-se com o **módulo `api`** na licença. Sem ele, o ecrã avisa e os pedidos são recusados com
`403 plano_sem_api`.

**Sistema → API e Integrações** (permissão *API e integrações*, que por omissão só o administrador tem):

1. Marque **Ligar a API de integração** e escolha a porta (7800 por omissão; tem de ser diferente da dos postos em rede).
2. Escolha quem pode ligar-se:
   - **Só desta máquina** (127.0.0.1): para publicar a API na Internet através de um túnel com HTTPS (ver §3) ou para
     programas instalados no mesmo computador.
   - **Rede local**: outros computadores da rede ligam-se pelo IP desta máquina (o ecrã mostra o endereço).
3. **Guardar**. O ecrã mostra o endereço da API, por exemplo `http://192.168.1.10:7800/api/v1`. Se a firewall
   perguntar, autorize o KanGest.

A API corre na máquina que guarda a base de dados (modo *Sozinho* ou *Servidor*). Num *posto em rede*, o mesmo ecrã
configura a API do servidor. Depois de ligada, abre sozinha sempre que o KanGest arranca.

## 2. Chaves de acesso

Em **Chaves de acesso → Nova chave**:

| Campo | Para quê |
|---|---|
| Nome | O sistema que se liga: «Loja online», «Contabilidade»… |
| Age em nome de | O utilizador da chave. Os documentos ficam com este operador. Crie um utilizador próprio para cada integração (ex.: «Loja Online»): assim as vendas aparecem separadas no relatório «Por operador». |
| Posto | Série e armazém dos documentos. Um posto próprio com prefixo (ex.: `W`) dá documentos como `FR W2026/15` e o relatório «Por posto» mostra as vendas do canal. |
| Permissões | O que a chave pode fazer (tabela abaixo). Há modelos para *Loja online*, *Contabilidade*, *Só consulta (BI)* e *Verificador de preços*. |
| IPs permitidos | Vazio = qualquer. Ex.: `192.168.1.20, 10.0.0.0/8`. |
| Pedidos por minuto | 120 por omissão; 0 = sem limite. |
| Válida até | Vazio = sem validade. |

Ao criar, o KanGest mostra a chave **uma única vez** (`kg_` seguido de 48 letras e algarismos). Guarde-a no sistema
que se vai ligar. O KanGest guarda só uma impressão (SHA-256) e os primeiros 11 caracteres para a identificar; se a
chave se perder, **Renovar segredo** gera outra (a antiga deixa logo de funcionar).

- **Desactivar** (editar → «Chave activa»): suspende sem perder a configuração.
- **Revogar**: a chave deixa de funcionar de vez.
- **Desactivar o utilizador** da chave também a suspende.
- Uma permissão que o utilizador não tenha **não tem efeito** na chave (o ecrã avisa).

### Permissões das chaves

| Permissão | Dá acesso a |
|---|---|
| `api.artigos` | Artigos, preços e tabelas (famílias, IVA, armazéns, modos de pagamento…) |
| `api.stock` | Existências por armazém e movimentos de stock |
| `api.entidades` | Clientes e fornecedores, saldos e documentos em dívida |
| `api.documentos` | Documentos (vendas, compras, recibos, stock) com linhas e pagamentos |
| `api.custos` | Preços de custo, custo médio e lucro (sem ela, estes campos não saem da API) |
| `relatorios.ver` | Resumos de vendas |
| `sistema.saft` | SAF-T de facturação, de aquisição e do Estado |
| `artigos.criar`, `artigos.editar` | Criar e alterar artigos e preços |
| `clientes`, `fornecedores` | Criar e alterar clientes / fornecedores |
| `vendas.fatura_recibo` | Emitir FR |
| `vendas.fatura` | Emitir FT e GT |
| `vendas.proforma` | Emitir PP, OR e NE |
| `vendas.nota_credito`, `vendas.nota_debito` | Emitir NC / ND |
| `tesouraria.recibo` | Emitir recibos (RC) |
| `pos.alterar_preco`, `pos.desconto` | Vender com preço diferente do da ficha / com desconto |
| `vendas.limite_credito` | Vender acima do limite de crédito do cliente |
| `documentos.anular` | Anular documentos |
| `stock.entrada`, `stock.saida`, `stock.transferencia` | Documentos de stock (ES; SS e CI; TS) |
| `compras.fatura` | Documentos de compra (FC, VFR, VNC, VND, VNE, VOR, VGT, AF) |

## 3. Segurança

- A API fala **HTTP sem cifra** (ao contrário dos postos em rede, que vão cifrados). Na rede local fica protegida
  pelas chaves.
- **Para a Internet**, escolha «Só desta máquina» e publique a API com um túnel que dê HTTPS, por exemplo o
  [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) instalado no mesmo
  computador. Para experimentar: `cloudflared tunnel --url http://127.0.0.1:7800` (dá um endereço `https://….trycloudflare.com`);
  para uso permanente, um túnel com nome no domínio da empresa. **Não encaminhe a porta no router.**
- **Cloudflare Access** (opcional, recomendado na Internet): com o domínio da equipa e o AUD da aplicação em
  **API e Integrações**, os pedidos que chegam pela Cloudflare só passam com o JWT do Access (por exemplo, de um token
  de serviço de cada integração), além da chave de API; sem ele, `403 cloudflare_access`. Passo a passo em
  [CLOUDFLARE.md](CLOUDFLARE.md).
- Atrás de um túnel ou proxy **na própria máquina**, o KanGest usa o IP indicado em `CF-Connecting-IP`, `X-Real-IP`
  ou `X-Forwarded-For` (para os IPs permitidos e o registo). Vindos de outros computadores, estes cabeçalhos são ignorados.
- 20 chaves inválidas do mesmo IP em 10 minutos bloqueiam esse IP durante 10 minutos.
- Nunca envie a chave no endereço (`?chave=`): só nos cabeçalhos.
- O **registo de pedidos** (ecrã da API) guarda 90 dias: data, chave, IP, pedido, resposta, duração e erro. A criação,
  alteração, renovação e revogação de chaves e o ligar/desligar da API ficam na **Auditoria**.

## 4. Como fazer pedidos

```text
Endereço:   http://<máquina>:7800/api/v1
Chave:      Authorization: Bearer kg_…        (ou X-API-Key: kg_…)
Corpo:      Content-Type: application/json
```

- **Valores monetários em cêntimos de Kz** (inteiros): 1 140,00 Kz = `114000`. Os preços `preco1`…`preco4` são sem
  IVA; `preco1_iva`… são com IVA.
- Datas `AAAA-MM-DD`; data e hora `AAAA-MM-DDTHH:MM:SS` (hora local do KanGest).
- **Listas** por ordem de `id`, em páginas: `{"dados": [...], "mais": true, "seguinte": 1234}`. Para a página seguinte
  repita o pedido com `desde_id=1234`, até `mais` ser `false`. `limite` vai de 1 a 1000 (100 por omissão).
- **Sincronização:** `alterado_desde=2026-11-14T08:00:00` em artigos, clientes e fornecedores devolve só o que mudou
  desde essa hora (incluindo os desactivados, com `ativo: false`). Para o stock, peça `/stock/movimentos` desde o
  último `id` visto: cada movimento traz o `saldo` do armazém.
- **Erros:** `{"erro": {"codigo": "…", "mensagem": "…", "pedido": 1234}}`. As mensagens das regras de negócio são as
  do programa.
- **Número do pedido:** cada resposta a um pedido autenticado (ou recusado) leva `X-KanGest-Pedido: <n.º>` e, nos
  erros, o mesmo número em `erro.pedido`. É o número da linha no registo de pedidos (Sistema → API e Integrações,
  coluna «N.º»): cite-o ao pedir ajuda.
- **Rotas:** a lista completa está em `docs/openapi.json` e no código (`api::ROTAS`); um teste confirma que as duas
  descrevem as mesmas rotas e métodos.

| HTTP | Códigos | Quando |
|---|---|---|
| 400 | `pedido_invalido`, `campo_desconhecido` | JSON inválido, parâmetro mal escrito, campo que não existe |
| 401 | `nao_autenticado`, `chave_invalida`, `chave_revogada`, `chave_inactiva`, `chave_expirada` | Problema com a chave |
| 403 | `sem_permissao`, `ip_nao_autorizado`, `utilizador_inactivo`, `plano_sem_api`, `cloudflare_access` | A chave não pode fazer isto, a licença não inclui a API, ou falta a autorização do Cloudflare Access |
| 404 | `nao_encontrado` | Registo ou rota inexistente |
| 405 | `metodo_nao_permitido` | Ex.: `DELETE` numa rota que não o aceita |
| 413 | `pedido_grande` | Corpo acima de 2 MB |
| 422 | `validacao` | Recusado pelas regras do programa (stock insuficiente, valor pago insuficiente, licença terminada…) |
| 429 | `limite_pedidos`, `demasiadas_falhas` | Limite por minuto (cabeçalho `Retry-After`) ou demasiadas chaves erradas |
| 500 | `erro_interno` | Erro do KanGest (o pormenor fica no registo de pedidos) |

Com limite por minuto, cada resposta traz `X-RateLimit-Limit` e `X-RateLimit-Remaining`.

## 5. Recursos

| Método | Caminho | Permissão | O quê |
|---|---|---|---|
| GET | `/saude` | — | A API responde? (sem chave) |
| GET | `/openapi.json` | — | Especificação OpenAPI (sem chave) |
| GET | `/` | qualquer chave | Empresa, chave (permissões efectivas), utilizador e posto |
| GET | `/tabelas` | qualquer chave | Famílias, unidades, IVA, isenções, armazéns, modos de pagamento, tipos de documento, prazos, retenções, moedas |
| GET | `/artigos` | `api.artigos` | Lista (`q`, `familia_id`, `armazem_id`, `inativos`, `alterado_desde`, `desde_id`, `limite`) |
| GET | `/artigos/{id}` | `api.artigos` | Um artigo (stock no `armazem_id` pedido ou no do posto da chave) |
| GET | `/artigos/codigo/{código}` | `api.artigos` | Pelo código de barras, pela referência, por um código de embalagem ou por uma etiqueta da balança (`leitura.quantidade` diz quanto vale a leitura) |
| POST | `/artigos` | `artigos.criar` | Criar (`descricao`, `taxa_iva_id`, `preco1` ou `preco1_iva`…) |
| GET | `/artigos/{id}/foto` | `api.artigos` | A fotografia do artigo (WebP, JPEG ou PNG); `tem_foto` no artigo diz se existe |
| PUT/PATCH | `/artigos/{id}` | `artigos.editar` | Alterar só os campos enviados |

Cada artigo traz `atributos`, os dados do ramo de actividade da empresa ([RAMOS.md](RAMOS.md)) — por exemplo
`{"autor": "Pepetela", "editora": "Dom Quixote", "ano": 1980}` numa livraria ou `{"modelo": "CAM01", "tamanho": "M",
"cor": "Azul"}` numa loja de roupa — e aceita-os ao criar ou alterar (texto, número ou `true`; `{}` apaga-os).
A pesquisa `q` também os procura.

| Método | Caminho | Permissão | O quê |
|---|---|---|---|
| GET | `/stock` | `api.stock` | Existências por artigo, com o detalhe por armazém e o reservado (`artigo_id`, `armazem_id`, `q`, `abaixo_minimo`) |
| GET | `/stock/movimentos` | `api.stock` | Movimentos (`desde_id`, `artigo_id`, `armazem_id`, `de`) |
| GET | `/clientes`, `/fornecedores` | `api.entidades` | Lista (`q`, `nif`, `inativos`, `alterado_desde`, `desde_id`, `limite`) |
| GET | `/clientes/{id}`, `/fornecedores/{id}` | `api.entidades` | Um cliente/fornecedor com saldo e crédito |
| GET | `/clientes/{id}/pendentes` | `api.entidades` | Documentos em dívida |
| POST | `/clientes`, `/fornecedores` | `clientes` / `fornecedores` | Criar |
| PUT/PATCH | `/clientes/{id}`, `/fornecedores/{id}` | `clientes` / `fornecedores` | Alterar só os campos enviados |
| GET | `/documentos` | `api.documentos` | Lista (`tipo=FT,FR`, `grupo`, `de`, `ate`, `entidade_id`, `estado`, `desta_chave=1`, `linhas=1`, `desde_id`, `limite`) |
| GET | `/documentos/{id}` | `api.documentos` | Documento completo (linhas, pagamentos, IVA, AGT) |
| POST | `/documentos` | a do tipo (§2) | Emitir |
| POST | `/documentos/{id}/anular` | `documentos.anular` | Anular, com `{"motivo": "…"}` |
| GET | `/relatorios/vendas` | `relatorios.ver` | Resumo (`de`, `ate`, `agrupar` = dia, mes, artigo, familia, operador, cliente, pagamento, vendedor, posto, documento, iva…) |
| GET | `/saft` | `sistema.saft` | XML do SAF-T (`de`, `ate`, `tipo` = facturacao, aquisicao ou estado) |

## 6. Emitir documentos

```bash
curl -X POST http://192.168.1.10:7800/api/v1/documentos \
  -H "Authorization: Bearer kg_…" -H "Content-Type: application/json" \
  -H "Idempotency-Key: encomenda-1001" \
  -d '{
        "tipo": "FR",
        "referencia": "ENC-1001",
        "cliente": { "nif": "5000111222", "nome": "Empresa X, Lda", "email": "compras@x.ao" },
        "linhas": [ { "referencia": "ARZ5", "quantidade": 2 },
                    { "codigo_barras": "5601234567890", "quantidade": 1 } ],
        "pagamentos": [ { "modo": "TB", "valor": 342000 } ]
      }'
```

A resposta (201) é o documento emitido: `numero_doc` (ex.: `FR W2026/15`), totais, linhas, `hash_controlo` (os 4
caracteres da assinatura) e, com a Facturação Electrónica ligada, `fe.estado` e `fe.url_consulta` (o conteúdo do QR
code da AGT para mostrar ao cliente).

- **Artigo:** `artigo_id`, ou `referencia`, ou `codigo_barras` (o código de uma embalagem multiplica a quantidade pelas unidades da embalagem; uma etiqueta da balança dá o peso).
- **Preço:** omitido = o da ficha (com a promoção em vigor). `preco_unitario` é sem IVA; `preco_com_iva` é com IVA (o
  total pode diferir 1 cêntimo por arredondamento). Um preço diferente do da ficha exige `pos.alterar_preco`; um
  desconto acima da promoção exige `pos.desconto` (e o limite de desconto do utilizador). Pela API **não há PIN de
  supervisor**: conta só a permissão da chave.
- **Cliente:** `entidade_id`, ou `cliente` com o NIF (procura-o e, se não existir, cria-o — exige `clientes`). Sem NIF,
  a venda é a Consumidor Final. As facturas a crédito (FT) exigem um cliente identificado.
- **Pagamentos:** obrigatórios na FR e no RC; os modos estão em `GET /tabelas` (NU numerário, MB Multicaixa, TB
  transferência…). O troco só se dá em numerário.
- **`Idempotency-Key`** (até 100 caracteres, por exemplo o n.º da encomenda): se a ligação cair e o sistema repetir o
  pedido com a mesma chave, o KanGest devolve o documento já emitido (200, cabeçalho `Idempotent-Replayed: true`) em
  vez de emitir outro. Use-a sempre nas vendas.
- **Documento de origem** (`doc_origem_id`: nota de crédito, conversão de proforma ou de nota de encomenda): envie as
  linhas sem preço — o preço e o desconto vêm da origem. Sem `pos.alterar_preco` / `pos.desconto`, cada artigo tem de
  estar na origem e um preço ou desconto diferente é recusado.
- **Caixa:** os documentos da API não entram no caixa de nenhum posto (a venda da loja online não passa pela gaveta).
  Uma nota de crédito de uma venda paga fica como crédito do cliente; a devolução do dinheiro faz-se no programa
  (Contas a Receber → devolver) ou fora dele.
- **Outros tipos:** nota de crédito com `doc_origem_id` e `motivo`; recibo (`RC`) com `entidade_id`, `liquidar`
  (`[{"documento_id": …, "valor": …}]`) e `pagamentos`; proforma/encomenda (`PP`, `NE`) sem pagamentos; guia (`GT`) com
  `transporte`; compras com `entidade_id` do fornecedor e `doc_externo`.
- A data é a de hoje. Não se recuperam documentos manuais pela API.
- Com a licença terminada (modo de consulta), a emissão é recusada (422); as leituras continuam.

## 7. Avisos automáticos (webhooks)

Em vez de perguntar de minutos a minutos, o outro sistema pode ser **avisado**. Em **API e Integrações → Avisos
automáticos → Novo aviso**, indique o endereço que recebe os avisos e os eventos:

| Evento | Quando | `dados` |
|---|---|---|
| `documento.emitido` | Qualquer documento novo (vendas, compras, recibos, stock), no programa, no POS ou pela API | O documento com as linhas (sem custos) e `url` para o pedir à API |
| `documento.anulado` | Um documento é anulado | O mesmo, com `estado: "A"` e o motivo |
| `stock.alterado` | Entradas, saídas, vendas, transferências, inventário | O artigo, a `quantidade` total e o detalhe por armazém, no momento do envio |
| `artigo.alterado` | Artigo criado ou alterado (descrição, preços, promoção, activo) | O artigo com os preços sem e com IVA (sem custos) |

Cada aviso é um `POST` com JSON:

```json
{ "id": 381, "evento": "stock.alterado", "criado_em": "2026-11-15T10:42:07",
  "dados": { "id": 12, "referencia": "ARZ5", "quantidade": 47, "armazens": [{ "armazem_id": 1, "armazem": "Principal", "quantidade": 47 }] } }
```

- Cabeçalhos: `X-KanGest-Evento`, `X-KanGest-Entrega` (o `id`, igual nas novas tentativas: use-o para não tratar o
  mesmo aviso duas vezes) e `X-KanGest-Assinatura: t=<segundos>,v1=<hex>`.
- Responda com `2xx` em menos de 15 segundos. Sem resposta ou com erro, o KanGest volta a tentar ao fim de 1, 5, 15 e
  30 minutos e de 1, 3, 6 e 12 horas; depois a entrega fica «Falhada» e reenvia-se no ecrã.
- Vários movimentos do mesmo artigo seguidos dão **um** aviso de stock, com a quantidade do momento do envio.
- **Confirme sempre a assinatura**: `v1` é o HMAC-SHA256 de `"<t>.<corpo>"` com o segredo do aviso (botão da chave).
  Recuse avisos com `t` de há mais de 5 minutos.

```js
// Node.js (Express com express.raw({ type: "application/json" }))
import { createHmac, timingSafeEqual } from "node:crypto";
function avisoValido(corpo, cabecalho, segredo) {
  const { t, v1 } = Object.fromEntries(cabecalho.split(",").map((p) => p.split("=")));
  if (Math.abs(Date.now() / 1000 - Number(t)) > 300) return false;
  const esperado = createHmac("sha256", segredo).update(`${t}.${corpo}`).digest("hex");
  return v1.length === esperado.length && timingSafeEqual(Buffer.from(v1), Buffer.from(esperado));
}
```

```php
// PHP (WordPress/WooCommerce)
$corpo = file_get_contents('php://input');
parse_str(str_replace(',', '&', $_SERVER['HTTP_X_KANGEST_ASSINATURA'] ?? ''), $a);
$ok = abs(time() - (int)($a['t'] ?? 0)) <= 300
   && hash_equals(hash_hmac('sha256', ($a['t'] ?? '') . '.' . $corpo, $segredo), $a['v1'] ?? '');
```

```python
# Python
import hmac, hashlib, time
def aviso_valido(corpo: bytes, cabecalho: str, segredo: str) -> bool:
    a = dict(p.split("=", 1) for p in cabecalho.split(","))
    if abs(time.time() - int(a["t"])) > 300:
        return False
    esperado = hmac.new(segredo.encode(), f"{a['t']}.".encode() + corpo, hashlib.sha256).hexdigest()
    return hmac.compare_digest(esperado, a["v1"])
```

Os avisos fazem parte da API: só saem quando a licença a inclui. Com o KanGest na rede local e a loja na Internet, o
endereço da loja tem de ser alcançável a partir do computador do KanGest (é ele que liga para fora).

## 8. Exemplos de integração

**Loja online** (WooCommerce, Shopify ou loja própria, através de um pequeno programa de ligação):

1. Com **avisos automáticos** (§7) de `artigo.alterado` e `stock.alterado`, a loja actualiza-se no momento. Sem eles,
   de 10 em 10 minutos: `GET /artigos?alterado_desde=<última hora vista>` (página a página) para preços, descrições e
   artigos desactivados; `GET /stock/movimentos?desde_id=<último id visto>` para o stock.
2. Encomenda paga: `POST /documentos` (FR) com `Idempotency-Key` = n.º da encomenda e `referencia` = n.º da encomenda.
   Guarde `numero_doc` e mostre ao cliente o QR (`fe.url_consulta`).
3. Encomenda a pagar na entrega: `NE` ao receber (o stock fica reservado) e, ao entregar, `FR` com `doc_origem_id` da
   NE e as mesmas linhas (sem preços). A NE fica facturada.

**Contabilidade:** no início de cada mês, `GET /saft?de=2026-10-01&ate=2026-10-31` (e `tipo=aquisicao` para as
compras); ou `GET /documentos?de=…&ate=…&linhas=1` para importar documento a documento.

**Painel / BI:** `GET /relatorios/vendas?de=…&ate=…&agrupar=dia` (ou `artigo`, `familia`, `pagamento`, `posto`).

**Verificador de preços:** `GET /artigos/codigo/5601234567890` → `descricao`, `preco1_iva`, `promo_atual`.

Ler todas as páginas de uma lista (JavaScript):

```js
const BASE = "http://192.168.1.10:7800/api/v1";
const cab = { Authorization: `Bearer ${process.env.KANGEST_CHAVE}` };
let desde = 0, artigos = [];
for (;;) {
  const r = await fetch(`${BASE}/artigos?limite=500&desde_id=${desde}`, { headers: cab });
  if (!r.ok) throw new Error((await r.json()).erro.mensagem);
  const pagina = await r.json();
  artigos.push(...pagina.dados);
  if (!pagina.mais) break;
  desde = pagina.seguinte;
}
```

O mesmo em Python:

```python
import os, requests
BASE = "http://192.168.1.10:7800/api/v1"
cab = {"Authorization": f"Bearer {os.environ['KANGEST_CHAVE']}"}
desde, artigos = 0, []
while True:
    r = requests.get(f"{BASE}/artigos", params={"limite": 500, "desde_id": desde}, headers=cab, timeout=30)
    r.raise_for_status()
    pagina = r.json()
    artigos += pagina["dados"]
    if not pagina["mais"]:
        break
    desde = pagina["seguinte"]
```

## 9. Para quem programa o KanGest

- Código: `src-tauri/src/api.rs` (servidor HTTP, autenticação, rotas e os métodos `api.*` do ecrã),
  `src-tauri/src/webhooks.rs` (avisos: fila, entregas e assinatura; eventos pelos gatilhos da migração
  `0021_webhooks.sql`) e `src/pages/Integracoes.tsx`. Base de dados: migração `0018_api.sql` (`api_chaves`, `api_pedidos`, `api_idempotencia`,
  `documentos.api_chave_id` e `alterado_em` mantido por gatilhos em artigos e entidades).
- As rotas chamam os mesmos métodos do `rpc` (`services::dispatch`) dentro do contexto da chave (`state::CONTEXTO`):
  sessão com as permissões efectivas da chave e o posto da chave. As leituras próprias da API são consultas próprias
  (ordenadas por `id`, com os custos retirados sem `api.custos`).
- Rota nova: acrescentar em `rotear` e `rota_existe`, no gerador `scripts/gerar-openapi.py` (que escreve
  `docs/openapi.json`, incluído no executável) e neste documento. Permissão nova para chaves: `PERMISSOES_API`.
- Testes: `cargo test api` (unitários e o teste de integração `api_integracao`, que liga a API numa porta livre e faz
  os pedidos por HTTP). O servidor de demonstração da auditoria (`servidor_demo`) também liga a API na porta 7800.

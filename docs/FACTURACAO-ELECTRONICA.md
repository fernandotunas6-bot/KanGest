# Facturação Electrónica da AGT

Regime jurídico das facturas (Decreto Presidencial 71/25): as facturas e os documentos equivalentes são comunicados à AGT em tempo real. Obrigatório desde 1 de Janeiro de 2026 para grandes contribuintes e fornecedores do Estado; alargado aos restantes por fases.

O KanGest implementa os serviços REST `sigt/fe/v1` publicados no Quiosque AGT (quiosqueagt.minfin.gov.ao/doc-agt): **registarFactura**, **obterEstado**, **solicitarSerie** e **listarSeries**, com assinaturas JWS RS256 e autenticação HTTP Basic.

## O que é preciso (fora do KanGest)

| Quem | O quê | Onde |
|---|---|---|
| Produtor do software | Credenciais de acesso aos serviços (utilizador/senha) | Pedido por email a produtores.dfe.dcrr.agt@minfin.gov.ao com o nome e o NIF da empresa |
| Produtor do software | Certificação do software para a facturação electrónica → **n.º de validação** (ex.: `C_134`) | AGT |
| Produtor do software | Par de chaves RSA (≥ 2048 bits) — a pública registada no **Portal do Parceiro** | Gerada localmente: `openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out produtor.pem` |
| Contribuinte (empresa) | Adesão à facturação electrónica, estabelecimentos registados, **chave do contribuinte** | Portal do Contribuinte |

## Ligar no KanGest

**Sistema → Facturação Electrónica (AGT)**:

1. Ambiente **Homologação** para os testes; **Produção** depois de aprovado.
2. Utilizador, senha, n.º de validação, estabelecimento (`SEDE` por defeito), chave do produtor e chave do contribuinte (ficheiros PEM).
3. **Pedir série** para cada tipo (FT, FR, NC, ND, AF, RC). A série atribuída pela AGT passa a ser a série em uso; com a facturação electrónica ligada, estes documentos só se emitem em séries da AGT.
4. **Ligar facturação electrónica**.

## Funcionamento

- Cada documento emitido (FT, FR, NC, ND, AF, RC) entra numa fila e é enviado em segundo plano (de 2 em 2 minutos, lotes de até 30), ou logo com «Enviar e consultar agora».
- Sem ligação à AGT nada pára: os documentos ficam «Por enviar» e seguem quando houver rede (contingência).
- A AGT valida de forma assíncrona: o KanGest consulta o estado (`obterEstado`) e marca cada documento como **Válido** ou **Rejeitado** com os códigos de erro da AGT.
- Um documento rejeitado não se reenvia com o mesmo número: emite-se um documento novo e associa-se em **Corrigir** (enviado com `documentStatus = C` e o número rejeitado).
- O impresso (A4 e talão, incluindo ESC/POS) leva o **QR code** da AGT: `https://quiosqueagt.minfin.gov.ao/facturacao-eletronica/consultar-fe?emissor=<NIF>&document=<n.º com %20>`, correcção de erros M.

## Pontos a confirmar na homologação

A documentação da AGT tem inconsistências; o KanGest segue o texto e os exemplos e estes pontos devem ser confirmados nos testes com a AGT:

- `companyName` nos documentos: a tabela diz «contribuinte», o exemplo usa o nome do cliente. O KanGest envia o **nome do cliente**.
- Cabeçalho JWS: o exemplo usa `{"typ":"JOSE","alg":"RS256"}` (usado pelo KanGest); outra página indica `JWT`.
- O QR code oficial inclui o logótipo da AGT ao centro (menos de 20% da área); o KanGest imprime o QR sem logótipo.
- O serviço `anularFactura` consta da lista de serviços da AGT mas a sua especificação não está publicada. Por isso, com a facturação electrónica ligada, **um documento comunicado (ou em fila) não se anula no KanGest**: corrige-se com uma nota de crédito, que também é comunicada. Os documentos rejeitados pela AGT podem ser anulados. Quando a AGT publicar o `anularFactura`, liga-se aqui.
- O IVA de cada linha é enviado tal como foi calculado e impresso; a especificação pede arredondamento ao cêntimo superior no `taxContribution` — confirmar na homologação se a AGT recalcula este valor.

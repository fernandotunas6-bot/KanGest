# KanGest face aos grandes sistemas POS — o que falta implementar

Este estudo compara o KanGest 1.1 com o que os POS de grande difusão costumam oferecer. Serviram de referência as
funções comuns aos POS internacionais (Square, Shopify POS, Lightspeed, Toast, Odoo POS, Loyverse) e aos programas de
facturação e POS mais usados em Angola e Portugal (Primavera, PHC, Sage, Zone Soft, Vendus). Não é uma comparação produto
a produto: é a lista do que um cliente que já usou um destes sistemas espera encontrar.

O lado do KanGest foi verificado no código (serviços Rust, migrações da base de dados e ecrãs React), não só na documentação.

Legenda: ✅ tem · 🟡 parcial · ❌ não tem · **Esforço:** P pequeno (dias) · M médio (1–3 semanas) · G grande (mais de um mês)

## 1. Onde o KanGest já está ao nível (ou acima)

| Área | O que o KanGest tem |
|---|---|
| Fiscal de Angola | Assinatura RSA-SHA1 encadeada por série, SAF-T AO de facturação, de aquisição e do Estado validados contra o XSD, Facturação Electrónica da AGT com QR code, retenções na fonte, IRT Grupo C. Os POS internacionais não têm nada disto. |
| Venda no POS | Leitor de código de barras (`3*código`), atalhos F2–F12, suspender e retomar, proformas e notas, desconto de linha e global, alterar preço com PIN de supervisor, multipagamento com troco, FR e FT a crédito, kits, escolha de lote (FEFO). |
| Equipamento | Impressão ESC/POS directa (rede, porta, fila), gaveta, visor de cliente VFD ou segundo ecrã, terminal táctil com teclado no ecrã. |
| Stock | Vários armazéns, transferências, lotes e validades, rastreabilidade, inventário, recálculo, reservas por notas de encomenda, encomendas a fornecedores, custos de importação e moeda estrangeira. |
| Segurança | Permissões por operação e perfis, PIN, bloqueio por tentativas e por inactividade, registo de acessos e auditoria. |
| Gestão | Tesouraria, conta corrente de clientes e fornecedores, adiantamentos, RH e salários (IRT/INSS), oficina, relatórios e síntese mês a mês. |
| Vários postos | Um servidor na rede local com sessão e série por posto. |

## 2. O que falta, por área

### A. Integração com outros sistemas

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| **API para outros sistemas** (loja online, contabilidade, BI, apps) com chaves de acesso | Padrão | ✅ **feita neste trabalho** — ver [API.md](API.md) | — | — |
| Webhooks: avisar outro sistema quando há uma venda, uma anulação ou uma mudança de stock | Comum | ❌ (a API já permite sincronizar por consulta: `desde_id` e `alterado_desde`) | Alta | M |
| Ligação pronta a lojas online (WooCommerce, Shopify) | Comum | ❌ (agora possível através da API) | Média | M |
| Enviar a factura ao cliente por email ou WhatsApp (PDF) | Padrão | ❌ só impressão; o PDF faz-se pelo diálogo de impressão do sistema | Alta | M |
| Painel ou app do dono no telemóvel (vendas em tempo real, fora da loja) | Padrão | ❌ (a API é a base para isto) | Média | G |
| Cópia de segurança na nuvem | Padrão nos POS em nuvem | ❌ só cópias locais (14 dias na pasta de dados) | Alta | M |
| Importar extractos bancários e reconciliar | Comum nos ERP | ❌ | Baixa | M |

### B. Venda e pagamentos

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| TPA integrado (o valor vai sozinho para o terminal, sem o operador o escrever) | Padrão | ❌ escreve-se o valor no TPA e no KanGest | Média | G (depende da API do banco/EMIS) |
| Multicaixa Express / pagamento por QR ou referência | Crescente em Angola | ❌ | Média | M–G |
| Pagar uma venda com o saldo do cliente (nota de crédito, adiantamento, vale) | Comum | ✅ **feito:** a FR paga-se com o crédito do cliente (modo CS, como nos recibos); o POS mostra o crédito disponível. Antes, o POS oferecia a opção e o servidor recusava-a | — | — |
| Receber em moeda estrangeira no POS (USD/EUR com troco em Kz) | Comum em Angola | ❌ moeda estrangeira só nas compras | Média | M |
| Troca num só passo (devolução + venda nova) | Comum | 🟡 faz-se com nota de crédito e depois uma FR | Baixa | P |
| Gorjetas | Restauração | ❌ | Baixa | P |

### C. Catálogo de artigos

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| Vários códigos de barras por artigo e embalagens (caixa de 12 → 12 unidades) | Padrão | ✅ **feito:** outros códigos e embalagens na ficha do artigo; o POS e a API multiplicam pelas unidades | — | — |
| Artigos a peso (código EAN-13 da balança com peso ou preço, prefixos 20–29) | Padrão em mercearias, talhos, padarias | ✅ **feito:** formato configurável, código na balança na ficha, dígito de controlo confirmado | — | — |
| Fotografias dos artigos (grelha do POS e loja online) | Padrão | ❌ | Média | P–M |
| Variantes (tamanho e cor) com stock por variante | Padrão em vestuário e calçado | ❌ | Média | G |
| Números de série / IMEI por unidade vendida | Padrão em electrónica e telemóveis | ❌ (há lotes, não números de série) | Média | M |
| Preços por quantidade e tabelas de preços por cliente | Comum | 🟡 4 níveis de preço e desconto por cliente | Baixa | M |
| Modificadores e complementos («sem cebola», extras) | Restauração | ❌ | Baixa | M |

### D. Promoções e fidelização

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| Promoções: leve 3 pague 2, combos, desconto por família, por horário (happy hour), cupões | Padrão | 🟡 só desconto % por artigo entre datas | Alta | M |
| Fidelização: pontos ou cashback por cliente | Padrão | ❌ | Média | M |
| Vales e cartões oferta (venda, saldo, resgate) | Padrão | ❌ | Média | M |
| Campanhas (aniversários, SMS/WhatsApp/email aos clientes) | Comum | ❌ | Baixa | M |

### E. Caixa

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| Contagem do fecho por notas e moedas (Kz) | Padrão | ✅ **feito:** contagem por notas e moedas (configuráveis), guardada e impressa no relatório de fecho | — | — |
| Fecho cego (o operador conta sem ver o valor esperado) | Comum | ✅ **feito:** opção em Opções de Venda; quem não vê relatórios só vê o esperado depois de fechar | — | — |
| Aviso de excesso de numerário na gaveta (recolha para o cofre) | Comum | ❌ | Baixa | P |
| Leitura X a meio do turno | Comum | 🟡 o estado do caixa mostra o acumulado, sem talão próprio | Baixa | P |

### F. Operação

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| Continuar a vender sem rede (modo offline) e sincronizar depois | Padrão | ❌ no modo «posto em rede», sem rede o posto não vende | Alta (lojas com vários postos) | G |
| Várias lojas com sede central (preços, stock e relatórios consolidados pela Internet) | Padrão | 🟡 vários armazéns e postos, mas só na rede local | Média | G |
| POS em tablet ou telemóvel | Padrão | ❌ só Windows e macOS | Baixa | G |
| Verificador de preços e quiosque de auto-atendimento | Comum | ❌ (possível com a API: `GET /api/v1/artigos/codigo/{código}`) | Baixa | P com a API |

### G. Restauração (se for um mercado a seguir)

Mesas e planta da sala, comandas, envio para a cozinha e o bar (impressora de cozinha ou ecrã KDS), divisão da conta,
transferência de mesa, menus e modificadores: ❌. Hoje só existe o campo «Identificação (cliente, mesa, observação)» ao
suspender uma venda. É um módulo à parte (esforço G); a impressão ESC/POS já existente serve para a impressora de cozinha.

### H. Gestão

| Função | Nos grandes POS | KanGest | Prioridade | Esforço |
|---|---|---|---|---|
| Comissões de vendedores | Comum | 🟡 o vendedor fica no documento e há o relatório «por vendedor», sem cálculo de comissão | Média | P |
| Sugestão automática de encomendas a fornecedores (pelo mínimo e pelas vendas) | Comum | 🟡 relatórios de ruptura e de mínimo; a encomenda faz-se à mão | Média | M |
| Picagem de ponto dos operadores no POS | Comum | ❌ (o RH tem faltas e férias) | Baixa | P |
| Relatórios enviados por email todos os dias | Comum | ❌ | Baixa | M |

## 3. Prioridades recomendadas

**Feito neste trabalho:** API de integração com chaves de acesso, permissões por chave, IPs permitidos, validade,
limite de pedidos, registo de pedidos, idempotência na emissão e exportação do SAF-T ([API.md](API.md)).

**Fase seguinte (maior retorno para o retalho angolano, esforço pequeno a médio):**

1. ~~Corrigir a compensação de saldo no POS~~ — feito: a FR paga-se com o crédito do cliente.
2. ~~Artigos a peso (EAN-13 de balança) e vários códigos de barras/embalagens por artigo~~ — feito.
3. ~~Contagem do fecho de caixa por notas e moedas e fecho cego~~ — feito.
4. Webhooks para a API (venda emitida, documento anulado, stock alterado), assinados com HMAC.
5. Envio da factura por email/WhatsApp em PDF.
6. Fotografias dos artigos (úteis no POS táctil e na loja online ligada pela API).
7. Cópia de segurança na nuvem (cifrada).

**Depois:** promoções avançadas, fidelização e vales, números de série/IMEI, comissões, sugestão de encomendas,
receber em moeda estrangeira no POS, integração com TPA/Multicaixa Express.

**Projectos grandes:** modo offline nos postos em rede, várias lojas pela Internet com sede central, app do dono,
variantes (tamanho/cor) e o módulo de restauração.

## 4. Notas

- A API pode ser vendida como parte de um plano de licença (por exemplo, só no plano «Empresa»); hoje fica disponível
  em todos os planos e desligada por omissão.
- Os postos em rede falam HTTP sem cifra na rede local ([REDE.md](REDE.md)). A API segue a mesma regra e, para ser usada
  pela Internet, deve ficar atrás de um túnel ou proxy com HTTPS (ver [API.md](API.md)).

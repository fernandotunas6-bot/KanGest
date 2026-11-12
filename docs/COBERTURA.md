# Cobertura: KIGEST original vs KanGest novo

Fonte: menus, ecrãs, botões e permissões extraídos dos executáveis originais (descomprimidos do UPX, com os formulários Delphi `TPF0` lidos):
**KanGest GC** (gestão/administração: 216 ecrãs, 9 grupos de menu) e **KanGest POS** (vendas: 77 ecrãs).

Legenda: ✅ feito · 🟡 parcial · ❌ falta

## A. KanGest POS (vendas)

| Função no POS original | Novo | Notas |
|---|---|---|
| Ecrã de venda (grelha código, ref., descrição, un., preço, desc. %, taxa, total) | ✅ | Carrinho com grelha de artigos por família |
| Leitor de código de barras | ✅ | `3*código` para quantidades |
| F2 Lista de artigos | ✅ | |
| F3 Quantidade (teclado numérico) | ✅ | |
| F4 Cliente / F5 Alterar dados do cliente | ✅ | |
| F6 Desconto de linha | ✅ | Com limite de desconto por utilizador |
| F7 Alterar preço | ✅ | Exige permissão ou PIN de supervisor |
| F8 Desconto global (% ou numerário) | ✅ | O valor em Kz é distribuído pelas linhas |
| F9 Descrição de linha | ✅ | |
| Del Apagar linha | ✅ | Exige permissão ou PIN |
| Suspender documento / abrir documento temporário | ✅ | |
| Notas e proformas: abrir e facturar | ✅ | |
| Cancelar documento | ✅ | |
| Pagamento multi-modo com troco (numerário, Multicaixa, transferência…) | ✅ | |
| FR / FT a crédito | ✅ | |
| Impressão em talão (80 mm ou 58 mm, como o «Tamanho» do PosConfig.ini) ou A4, vias, opções de impressão ao finalizar | ✅ | Usa o diálogo de impressão do sistema; a largura escolhe-se por posto |
| Promoção do artigo (desconto entre datas) | ✅ | Aplicada automaticamente no POS e nos documentos; até esse valor não pede PIN |
| Equipamento POS: terminal táctil (1024 × 768, teclado no ecrã, ecrã inteiro), leitor de código de barras com o foco em qualquer sítio (Enter ou Tab), visor de cliente de coluna (VFD série, CD5220/Epson) ou segundo ecrã | ✅ | Ver [EQUIPAMENTO-POS.md](EQUIPAMENTO-POS.md) |
| Talão térmico com logótipo, fonte B nas 48 colunas, documento de origem, motivo e vencimento | ✅ | Os mesmos elementos fiscais do A4 |
| Desenho dos impressos igual ao KIGEST (Fatura.fr3, FaturaVia2, FaturaProformaTk, FechoCaixa no POS; FaturaVenda, FaturaRecibo, FaturaProforma, NotaCredito, ReciboClienteNew no GC) | ✅ | Talão HTML e ESC/POS: empresa ao centro, n.º/título, data e hora, linhas em duas filas, totais, modo de pagamento, resumo de imposto, certificação, operador e «OBRIGADO VOLTE SEMPRE». A4: logótipo e empresa à esquerda, «Exmo(s) Senhor(es)» à direita, faixa de dados, tabela e rodapé com resumo de imposto, retenções, coordenadas bancárias e TOTAL DO DOCUMENTO |
| Nota de entrega (NotaEntregaTk) | ✅ | Em qualquer documento de venda: artigos e quantidades sem preços, em talão ou A4, com assinaturas |
| Abertura e fecho de caixa (valor físico vs sistema, diferença, reimprimir) | ✅ | Inclui a lista de fechos anteriores |
| Autenticação de supervisor por código (PIN) | ✅ | |
| Utilizadores com PIN, permissões por operação e limite de desconto | ✅ | |
| Configuração do posto (n.º, armazém, repetir linhas, confirmar cliente) | ✅ | Série por posto |
| Configuração de impressora (A4/talão, vias, valor mínimo) | ✅ | |
| Documentos emitidos e reimpressão (padrão/A4, 2.ª via) | ✅ | |
| Nota de crédito com documento de origem | ✅ | |
| Entrada e saída de stock | ✅ | |
| Mapa de stock | ✅ | |
| Clientes | ✅ | |
| Cadastro da empresa (logótipo) | ✅ | |
| Séries de documentos | ✅ | Automáticas por posto e ano, com ecrã de gestão (série por defeito, validade, séries manuais, bloqueio) |
| Vendas por operador / artigos por operador | ✅ | Em Relatórios: «Por operador» e «Artigos por operador» |
| Exportar SAF-T | ✅ | Validado nos testes contra o XSD oficial SAF-T AO 1.01_01; falta passar no validador da AGT |
| Cópia de segurança (manual + automática diária) | ✅ | |
| Fluxo de caixa: inserir dinheiro, regularização, pagamentos com rubrica e centro de custo, contas | ✅ | Caixa do POS + Tesouraria (contas, centros de custo, depósitos) |
| Lotes de artigos (seleccionar lote na venda) | ✅ | Botão «Lote» em cada linha: escolha manual com validade e quantidade, ou automático (expira primeiro) |
| Artigos compostos (kits) | ✅ | A venda do kit baixa o stock de cada componente |
| Abertura de gaveta e impressão ESC/POS directa | ✅ | Por posto: fila local, impressora de rede (9100) ou porta; corta o papel, acentos (CP850), gaveta sempre/só numerário/nunca, talão de teste e botão Gaveta (Ctrl+G) |
| Rede com vários postos ligados ao mesmo servidor | ✅ | Modo servidor/cliente em Configurações → Rede: o servidor serve o mesmo `rpc` por HTTP na rede local com chave de rede, sessão e série por posto; a impressão e a gaveta ficam em cada posto |

## B. KanGest GC (gestão / administração)

### Gestão de Stock
| Menu original | Novo |
|---|---|
| Lista de Artigos | ✅ |
| Documento de Stock (entrada) / Saída Stock / Transferência Stock | ✅ |
| Movimentos de Stock / Extrato de Movimento | ✅ (extracto por artigo) |
| Mapa de Stock | ✅ (com reservado por notas de encomenda, disponível e encomendado a fornecedores) |
| Lista de Stock por armazém (ListaStock / ListaStockArmazem) | ✅ (Relatórios de Stock → Existências, por família, com CSV e impressão) |
| Lista de Preços / Previsão de Venda | ✅ (4 PVP com IVA, margem s/ custo e promoção; stock ao custo e ao PVP com lucro previsto) |
| Margens dos 4 preços (Margem1–4 / Lucro1–4) | ✅ (na ficha do artigo: a margem s/ custo calcula o PVP) |
| Inventário / Contagem | ✅ (folha de contagem + regularização IN) |
| Relatórios de stock (ruptura, excesso, alerta, validade) | ✅ (ruptura, abaixo do mínimo, excesso, validade por lote e artigos sem saída, com CSV e impressão) |
| Rastreabilidade | ✅ (relatório de rastreio por lote ou artigo: documento e fornecedor de entrada, clientes de saída) |
| Lotes de Artigo | ✅ (lote e validade nas entradas, FEFO nas saídas, transferência e inventário por lote, aviso de validade) |
| Recálculo de Stock | ✅ (compara existências e lotes com os movimentos, corrige e refaz o saldo do extracto; fica em auditoria) |
| Doc. Consumo Interno | ✅ |
| Clonar artigos / artigos descontinuados / etiquetas e códigos de barras | ✅ (clonar na ficha do artigo; etiquetas Code 128 em folha A4 3×8 ou rolo 50×30 / 40×25 mm) |

### Vendas
| Menu original | Novo |
|---|---|
| Faturas / Fatura Recibo / Fatura Proforma / Orçamento | ✅ |
| Nota de Crédito | ✅ |
| Nota de Débito | ✅ |
| Nota de Encomenda | ✅ (com conversão em factura) |
| Guia de Transporte (carga/descarga, viatura) | ✅ (SAF-T MovementOfGoods) |
| Recibo Cliente / Estorno de Recibo | ✅ (estorno = anular recibo) |
| Factura Recibo (autofacturação) | ✅ |
| Adiantamento de Cliente | ✅ (recibo de adiantamento RA; o crédito compensa-se nos recibos, devolve-se ou regulariza-se) |
| Retenção na fonte (IRT, II, IS, IPU, IAC, IVA cativo) | ✅ (SAF-T WithholdingTax) |
| Relatórios: por artigo, entidade, dia, operador, tipo de pagamento, documento | ✅ |
| Relatórios: por ponto (posto), por vendedor, artigos por entidade, detalhes de documentos | ✅ |
| Relatório gráfico | ✅ (painel) |
| Vendas por mês e Relatório Síntese (ano a ano, mês a mês) | ✅ (inclui o histórico importado do KIGEST) |

### Compras
| Menu original | Novo |
|---|---|
| V/Fatura | ✅ (FC) |
| V/Fatura Recibo, V/Nota Crédito, V/Nota Débito, V/Orçamento, V/Guia | ✅ (com n.º do documento do fornecedor) |
| Encomenda a fornecedor / Alterar encomenda | ✅ (conversão em V/Factura) |
| Pagamento a fornecedor / Estorno recibo fornecedor / conta corrente de fornecedor | ✅ |
| Moedas, câmbio e custos adicionais de importação | ✅ (compras em USD/EUR/ZAR/CNY…, câmbio por documento, frete/seguro/desalfandegamento repartidos no custo) |
| Relatórios de compras / Mapa de fornecedor | ✅ (mapa com compras, IVA dedutível, pagamentos, dívida e os documentos de cada fornecedor) |

### Tesouraria e Finanças
| Menu original | Novo |
|---|---|
| Emitir recibo / Recibos emitidos / Contas a receber (por cliente) | ✅ |
| Fechos de Caixa (POS) | ✅ |
| Fluxo de Caixa, Contas, Centros de custo, Pagamentos, Transferências, Resumo mensal | ✅ |
| Contas a Pagar | ✅ |
| Extracto de conta corrente / Aviso de vencimento / Documentos pendentes (lista global) | ✅ |
| Bancos e contas bancárias | ✅ (contas de tesouraria com banco, agência, IBAN e saldo) |
| Mapa de imposto (geral / por dia / por família) | ✅ |
| Exportar SAF-T (Vendas) | ✅ |
| Exportar SAF-T (Compras) | ✅ (TaxAccountingBasis A: fornecedores, PurchaseInvoices, IVA dedutível, moeda estrangeira, retenções; validado contra o XSD) |
| Exportar SAF-T (Estado) | ✅ (TaxAccountingBasis E: só documentos de entidades públicas; validado contra o XSD) |

### Recursos Humanos
| Menu original | Novo |
|---|---|
| Funcionários (ficha completa, departamentos, funções, categorias, agregado familiar) | ✅ |
| Subsídios fixos (alimentação, transporte, chefia, turno, falhas, renda, atavio, isenção de horário, deslocação) | ✅ |
| Alterações mensais (horas extra, faltas, prémios, subsídios de férias e Natal, retroactivos…) | ✅ |
| Adiantamentos em prestações (com saída na tesouraria) | ✅ |
| Processamento automático (IRT Grupo A por escalões, INSS 3% + 8%, isenções de subsídios) | ✅ (tabela da Lei 14/25 pré-carregada e editável, com confirmação) |
| Recibos de salário, mapa de remunerações IRT/INSS, folha bancária (CSV) | ✅ |
| Mapas por rubrica (Produtividade, Horas Extra, Abono de Família, Subsídios de Natal e Férias, Sub_Chefia/Turno/Falhas/Arrendamento/Atavio/Deslocação, Retroactivos, Faltas) | ✅ (Recibos, mapas e pagamento → Rubrica: qualquer rubrica do mês, com CSV e impressão) |
| Ficha do funcionário, lista de funcionários, mapa de férias e guia de férias | ✅ (impressão; listas também em CSV) |
| Lista de agregados familiares e folha de pagamento ao banco | ✅ (impressão e CSV; a folha do banco assinala quem não tem IBAN) |
| Pagamento de salários pela tesouraria (centro de custo Salários) e anulação | ✅ |
| Férias e faltas | ✅ |
| Trabalhadores por conta própria: honorários por mês, dia ou hora, retenção de IRT (Grupo B, taxa editável), pagamento e anulação pela tesouraria | ✅ |
| Prestações sociais (maternidade, doença, acidente…) com suspensão do salário no período | ✅ |
| IRT Grupo C (volume de vendas) | ✅ (Relatórios → «IRT Grupo C (por mês)»: base sem IVA × taxa editável, 6,5% por defeito) |
| Facturação Electrónica da AGT (registarFactura, obterEstado, séries, QR code) | ✅ (ver [FACTURACAO-ELECTRONICA.md](FACTURACAO-ELECTRONICA.md); falta testar na homologação da AGT com credenciais reais) |
| Envio da declaração de remunerações à Segurança Social / mapa de IRT ao Portal do Contribuinte | 🟡 (exporta-se o CSV; o formato de carregamento dos portais não está publicado de forma acessível) |

### Oficina
| Menu original | Novo |
|---|---|
| Viaturas (marca, modelo, cor, ano, chassi, matrícula, motor, combustível, km) | ✅ |
| Ordens de serviço (avaria, diagnóstico, técnico, peças e mão de obra, estados) | ✅ |
| Impressão da OS e facturação da OS (associa a factura e fecha a OS) | ✅ |

### Cadastros e Sistema
| Menu original | Novo |
|---|---|
| Clientes / Fornecedores / Armazém / Empresa | ✅ (listas de entidades com CSV e impressão; papel timbrado) |
| Utilizadores (permissões por módulo, PIN, bloquear) | ✅ (Sistema → Utilizadores e Permissões: perfis de acesso próprios, senhas provisórias, bloqueio por tentativas falhadas, bloqueio da sessão, entrada por PIN, registo de acessos e auditoria; ver [UTILIZADORES.md](UTILIZADORES.md)) |
| Alterar senha | ✅ |
| Cópia de segurança | ✅ |
| Parametrização de documentos (Fr_Parametrizacao) | ✅ (descrição, vias, validade em dias, duplicar linha, prefixo do valor por extenso, texto próprio, papel A4/A5; o comportamento fiscal fica fixo) |
| Opções de Venda (Fr_ConfigContas: forma de pagamento → conta) | ✅ (e activar/desactivar formas de pagamento) |
| Definição de Faturas e Recibos | ✅ (coordenadas bancárias, valor por extenso, assinaturas, operador) |
| Séries de documentos | ✅ (automáticas + ecrã de gestão em Sistema → Séries de Documentos) |
| Manual do utilizador / Sobre | ✅ (manual rápido, atalhos do POS, versão, pasta e tamanho da base) |
| Verificar actualização | ✅ (endereço ou pasta partilhada, SHA-256 e assinatura minisign opcional, cópia de segurança antes de instalar; ver [ATUALIZACOES.md](ATUALIZACOES.md)) |
| **Importação de dados do KIGEST** (SQL Server: empresa, artigos, stock, entidades, utilizadores, histórico de documentos e conta corrente, RH e salários, tesouraria, oficina, custos de importação) | ✅ (simulação e conferência KIGEST × KanGest; ver [MIGRACAO-KIGEST.md](MIGRACAO-KIGEST.md) e [COMPARACAO-KIGEST.md](COMPARACAO-KIGEST.md)) |

## Resumo

- **POS:** todo o fluxo de venda do original está coberto, incluindo kits, saída de lotes pelo que expira primeiro, impressão térmica directa em ESC/POS com abertura de gaveta e vários postos ligados ao mesmo servidor.
- **GC:** o menu é igual ao do KIGEST (mesmos grupos e itens) e já não há itens "brevemente". Vendas, compras (com moeda estrangeira e custos de importação), stock com lotes e kits, tesouraria, conta corrente, RH e salários, oficina, SAF-T de facturação e de aquisição, e importação de dados estão cobertos. Os três SAF-T (facturação, aquisição e Estado), os honorários com IRT Grupo B e as prestações sociais estão feitos. Falta só o carregamento directo dos mapas de remunerações nos portais do INSS e da AGT, cujo formato não está publicado.

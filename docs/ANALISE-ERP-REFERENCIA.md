# O KanGest face ao Primavera, ao Odoo e a outros sistemas

Análise de interface e de funções (Novembro de 2026): como os sistemas de referência apresentam e organizam o trabalho,
o que já foi trazido para o KanGest, o que falta e se é possível aproveitar o Odoo.

## Resumo

- **O KanGest já cobre bem o núcleo comercial angolano**: facturação certificada, POS com teclas F, stock com lotes,
  tesouraria, conta corrente, RH com IRT/INSS, oficina, os três SAF-T (AO) de facturação e a Facturação Electrónica da AGT.
  Nisto está ao nível do Primavera e do PHC para uma PME, e à frente do Odoo, que não tem localização angolana certificada
  pela AGT (só módulos pagos de terceiros).
- **A interface era limpa, mas organizada como o KIGEST antigo**: um menu lateral com 90 entradas em acordeão, sem
  pesquisa, sem favoritos e sem barra superior. Nos sistemas de referência a navegação faz-se cada vez mais pela pesquisa
  (Ctrl+K no Odoo, «Diga-me» no Business Central) e por atalhos pessoais.
- **Nesta ronda trouxe-se o que dava mais retorno em toda a aplicação** (secção 3): barra superior com localização,
  pesquisa global Ctrl+K, favoritos e recentes, menu recolhível, tema escuro, tabelas compactas, painel com comparações e
  alertas, barra de estado e documentos ligados em cada documento, e relatórios arrumados por assunto.
- **As maiores lacunas são funcionais** (secção 4). Duas são obrigações fiscais com prazo: a homologação real da
  Facturação Electrónica (obrigatória para todas as empresas em 2027) e o **SAF-T de inventários**, um ficheiro anual
  exigido a todos os contribuintes desde 2026 que o KanGest ainda não gera (próximo prazo legal: 15 de Fevereiro de 2027,
  para o inventário de 2026). Seguem-se o envio de documentos em PDF por e-mail/WhatsApp, a importação genérica de Excel,
  anexos e actividades, listas configuráveis e, a prazo, a contabilidade (o SAF-T de contabilidade passa a obrigatório a
  partir do exercício de 2026).
- **Copiar código do Odoo não é viável nem recomendável** (secção 5): linguagens e arquitectura diferentes, licença LGPL na
  edição Community e proprietária na Enterprise. O caminho é estudar o comportamento e **reimplementar as ideias**, que
  não estão protegidas — foi o que se fez com a paleta de comandos, a barra de estado e os documentos ligados.

## 1. Os sistemas de referência

| Sistema | O que é | Onde é forte | Como apresenta |
|---|---|---|---|
| **Cegid Primavera** (ERP v10, Evolution) | ERP de referência em Angola e Portugal; certificado pela AGT, incluindo a facturação electrónica | Contabilidade, RH, logística, tesouraria, fiscalidade; *Listas* configuráveis («provavelmente a funcionalidade mais popular», segundo a própria Cegid); painéis de informação relacionada; Business Analytics | Aplicação Windows com fita de opções por área; grelhas densas; listas e mapas personalizáveis |
| **Cegid PHC** (CS, Evolution, GO) | ERP modular | Módulos de Gestão, Contabilidade (certificada pela AGT, SAF-T AO), CRM, POS, Frota, Suporte, Projectos, Processos (BPM) e agentes de IA (Cegid Pulse) | Painéis de gestão e de tesouraria em tempo real |
| **Cegid Vendus** | POS e facturação na nuvem para pequenos negócios | Facturação electrónica AGT, PDF assinado, impressoras térmicas, conta corrente, envio do SAF-T, app móvel, 30 dias grátis | Web e app, muito simples |
| **Odoo 19 / 20** | Suite web modular (Community LGPL, Enterprise paga) | Vistas lista, kanban, pivot, gráfico, calendário; paleta de comandos; barra de estado e «smart buttons»; *chatter* com mensagens, anexos e actividades; painéis; modo escuro; regras de reposição; listas de preços; fidelização | Web, menu de aplicações, barra de pesquisa com filtros, agrupar e favoritos |
| **Microsoft Dynamics 365 Business Central** | ERP para PME | (referência de interface) | «Diga-me» (Alt+Q) para chegar a qualquer página; centro de função com contadores de trabalho pendente |

## 2. Padrões de interface: os outros, o KanGest antes e agora

| Padrão | Primavera / Odoo / outros | KanGest antes | KanGest agora |
|---|---|---|---|
| **Navegação** | Primavera: fita por área. Odoo: aplicações e menu superior | Menu lateral com 9 grupos e 90 entradas; vários grupos abertos obrigavam a percorrer uma lista longa | Um grupo aberto de cada vez, secção **Favoritos** no topo, menu **recolhível** para uma barra de ícones com menus ao lado |
| **Localização** | Odoo: trilho (breadcrumbs) no topo | Só o título da página | **Barra superior** com «Grupo › Ecrã» e estrela para marcar como favorito |
| **Pesquisa global** | Odoo: paleta de comandos (Ctrl+K). Business Central: «Diga-me» | Não existia; cada lista tinha a sua pesquisa | **Ctrl+K** em qualquer ecrã: ecrãs (sem acentos e na grafia antiga ou nova: «fatura» encontra «Factura»), documentos, clientes/fornecedores por nome ou NIF, artigos, e acções (abrir POS, tema, bloquear sessão…) |
| **Criar** | Odoo: botão «Novo» em cada lista | Só pelo menu ou dentro de Documentos | Botão **Novo** na barra superior: Factura, Factura/Recibo, Proforma, Orçamento, Recibo, V/Factura, Pagamento, Entrada de stock |
| **Conta e preferências** | Odoo: avatar no canto superior direito | No fundo do menu, com nome cortado | **Menu do utilizador** no canto superior direito: senha, bloquear, manual, tema, densidade, sair |
| **Estado do documento** | Odoo: barra de estado no topo de cada documento (ex.: Rascunho → Lançado) | Etiquetas soltas («Em dívida», «Anulado») | **Barra de estado** na ficha: Por pagar → Paga em parte → Paga; Emitida → Facturada; aviso de vencimento e estado na AGT |
| **Documentos ligados** | Odoo: «smart buttons». Primavera: painéis de informação relacionada | Só o n.º de origem no impresso | **Ligados**: origem, recibos e notas de crédito que abatem, facturas emitidas a partir de uma proforma; cada um abre-se com um clique, na mesma janela |
| **Painel** | Primavera: Business Analytics por perfil. Odoo: painéis com filtros. Business Central: contadores do que está pendente | 4 indicadores, gráfico de 30 dias, top de artigos | Indicadores com **variação** (vs ontem, vs mês anterior até ao mesmo dia), **«A sua atenção»** (vencidos, pagamentos em atraso, facturas rejeitadas ou por validar na AGT, stock no mínimo, lotes expirados ou a expirar, proformas a expirar), **antiguidade de saldos** e **recebimentos por meio de pagamento** |
| **Relatórios** | Primavera: mapas agrupados. Odoo: menu de relatórios e tabela dinâmica | 17 separadores em três linhas | **4 assuntos** (Vendas, Operadores e postos, Pagamentos e documentos, Impostos), cada um com os seus mapas |
| **Densidade** | Primavera: grelhas densas. Odoo: listas compactas | Uma só altura de linha | **Confortável / Compacta** (mais linhas no ecrã), por utilizador |
| **Tema** | Odoo: modo escuro | Só claro | **Claro / Escuro / Sistema**; a impressão sai sempre em claro |

## 3. O que mudou nesta ronda

| Mudança | Onde |
|---|---|
| Barra superior, menu com favoritos, um grupo aberto, menu recolhível, botão Novo, menu do utilizador | `src/components/layout/AppShell.tsx`, `src/lib/navegacao.ts` |
| Pesquisa global Ctrl+K (ecrãs, documentos, entidades, artigos, acções; recentes e favoritos quando vazia) | `src/components/Comandos.tsx`; `?id=` abre a ficha em `Entidades.tsx` e `Artigos.tsx` |
| Tema claro/escuro/sistema e densidade das tabelas, por utilizador neste computador | `src/lib/aparencia.ts`, `src/index.css` (cores por função: `accent-ink`, `on-accent`, `nav`, `scrim`), `Td`/`Th` em `src/components/ui` |
| Painel: variações, «A sua atenção», antiguidade de saldos, recebimentos por meio de pagamento | `src/pages/Dashboard.tsx`; `dashboard` em `src-tauri/src/services/relatorios.rs` |
| Barra de estado e documentos ligados | `src/components/EstadoDocumento.tsx`, `DocumentoModal.tsx`; `recibos` e `derivados` em `documentos.obter` |
| Relatórios por assunto | `src/pages/Relatorios.tsx` |
| Atalhos e navegação no manual | `src/pages/Sobre.tsx` |

Os testes do backend cobrem as novidades (`fluxo_completo`: recibos e documentos derivados, faixas de antiguidade que
somam o «A receber», recebimentos por meio). Um servidor de rede de uma versão anterior continua a funcionar com o
frontend novo: os campos novos são opcionais.

## 4. O que falta: lacunas por prioridade

Esforço: **P** pequeno (dias), **M** médio (1–3 semanas), **G** grande (meses).

### Prioridade 1 — necessário em Angola a curto prazo

| # | Função | Como fazem os outros | Proposta para o KanGest | Esforço |
|---|---|---|---|---|
| 1 | **Facturação Electrónica homologada** | Primavera, PHC e Vendus anunciam a certificação AGT; obrigatória desde 1/1/2026 para grandes contribuintes e para todas as empresas em 2027 | O módulo existe; falta passar na homologação da AGT com credenciais reais (ver `COBERTURA.md`). É o maior risco comercial | P (técnico) |
| 2 | **SAF-T de inventários** | Obrigação nova da AGT (2026), comum a todos os programas de gestão | Ficheiro anual com o inventário no fim do exercício, exigido a **todos** os contribuintes (mesmo sem stock: ficheiro «sem operações»). Estrutura na Circular n.º 05/GACA/GJ/DSF/AGT/2026: cabeçalho (NIF, período, data, software e certificação) e, por produto, método de custeio, tipo, identificador, descrição, quantidade, unidade e valor; entrega em XML, JSON ou Excel no Portal do Contribuinte. O KanGest já tem todos estes dados (existências por armazém ao custo médio); falta gerar o ficheiro no formato da circular. Prazo legal 15 de Fevereiro (o de 2025 foi prorrogado até 15/04/2026) | P |
| 3 | **PDF directo e envio por e-mail / WhatsApp** | Odoo: «Imprimir e enviar». Vendus: PDF assinado | Gerar o PDF sem o diálogo de impressão e partilhar: e-mail (SMTP da empresa) e WhatsApp (abrir a conversa com o ficheiro). Em Angola o WhatsApp é o canal habitual | M |
| 4 | **Importação genérica de Excel/CSV** (artigos, clientes, fornecedores, saldos iniciais, stock) | Odoo: assistente de importação com modelos. Vendus: migração pelo próprio utilizador | Hoje só se importa do KIGEST, mas o site promete «migração de qualquer sistema». Modelos .xlsx, pré-visualização e verificação como na importação do KIGEST | M |
| 5 | **Sugestão de encomenda a fornecedor** | Odoo: regras de reposição (mínimo/máximo) que geram pedidos de compra | Os artigos já têm mínimo e máximo; gerar a Encomenda a Fornecedor com o que está abaixo do mínimo até ao máximo, por fornecedor habitual | P/M |
| 6 | **Exportação para Excel (.xlsx)** | Primavera e Odoo exportam qualquer lista | Hoje é CSV; um .xlsx com formatos de número e data evita erros ao abrir no Excel em português | P |

### Prioridade 2 — diferenciação e produtividade

| # | Função | Como fazem os outros | Proposta para o KanGest | Esforço |
|---|---|---|---|---|
| 7 | **Integração contabilística** | Primavera e PHC têm contabilidade certificada e SAF-T de contabilidade | O SAF-T de contabilidade é obrigatório a partir do exercício de 2026 (entrega em 2027). Primeiro passo: gerar os lançamentos dos documentos (vendas, compras, recibos, pagamentos, salários) com um plano de contas configurável e exportá-los para o programa do contabilista | M |
| 8 | **Anexos e notas** nos documentos, clientes e artigos | Odoo: *chatter* com anexos e histórico | Anexar a V/Factura digitalizada, o comprovativo da transferência ao recibo, o contrato ao cliente | M |
| 9 | **Actividades e lembretes** | Odoo: actividades com data e responsável | «Ligar ao cliente», «cobrar factura» com data e utilizador; aparecem em «A sua atenção» | M |
| 10 | **Vista kanban** onde há estados | Odoo: kanban arrastável | Ordens de serviço (Aberta → Em curso → Concluída → Facturada), proformas e encomendas | P/M |
| 11 | **Listas configuráveis** | Primavera: Listas. Odoo: filtros, agrupar por e favoritos | Escolher e ordenar colunas, ordenar por clique no cabeçalho, agrupar, guardar filtros | M/G |
| 12 | **Tabela dinâmica** sobre vendas e compras | Odoo: vista pivot. Primavera: Business Analytics | Linhas × colunas × medida (ex.: família × mês × total) com exportação | M |
| 13 | **Conciliação bancária** | Odoo: importa extractos e concilia. Cegid PHC Bancos: importação de extractos e reconciliação | Importar o extracto do banco (CSV/Excel) e marcar os movimentos de tesouraria correspondentes | M |
| 14 | **Listas de preços** | Odoo: tabelas por cliente, quantidade, período e família | Já há 4 PVP, desconto por cliente e promoções por artigo; faltam preços por quantidade, por família e por período | M |
| 15 | **Unidades alternativas e variantes** | Odoo: unidades com conversão e variantes (tamanho/cor) | Caixa de 12 = 12 unidades na compra e na venda; variantes para vestuário e calçado | M/G |
| 16 | **Fidelização e vales** | Odoo: pontos, cartões oferta e códigos de desconto | Pontos por compra e vales no POS | M |
| 17 | **Selector de data próprio** | — | Os campos de data seguem o idioma do Windows (num Windows em inglês aparece mm/dd/aaaa ao lado de datas em dd/mm/aaaa); um selector próprio mostra sempre dd/mm/aaaa | P |

### Prioridade 3 — médio e longo prazo

| # | Função | Como fazem os outros | Proposta | Esforço |
|---|---|---|---|---|
| 18 | **Contabilidade geral** (PGC angolano) e SAF-T de contabilidade | O ponto forte do Primavera e do PHC | Depois da integração (7): diário, razão, balancete, balanço, demonstração de resultados e o SAF-T de contabilidade | G |
| 19 | **Consulta no telemóvel** | Odoo é web; Cegid lançou app móvel; Vendus tem app | Painel e contas correntes em modo de consulta, servidos pelo servidor de rede do KanGest | G |
| 20 | **Várias empresas na mesma instalação** | Primavera e Odoo | Escolher a empresa ao entrar, com bases separadas | G |
| 21 | **CRM simples** | PHC CRM, Odoo CRM | Oportunidades em kanban ligadas às proformas | M |
| 22 | **Imobilizado** | PHC Ativos, Primavera | Fichas de activos e amortizações | G |
| 23 | **Assistente com IA** | Odoo 20 (agentes que criam e alteram registos), Cegid Pulse | Perguntas sobre os dados («quanto vendi de arroz este mês?») a partir da pesquisa global | exploratório |

## 5. Podemos aproveitar ou copiar o Odoo?

**Código: não.**

- **Licenças.** O Odoo Community é LGPL-3: código copiado para o KanGest arrastaria as obrigações da LGPL (publicar o
  código dessas partes e das alterações), o que não combina com um produto vendido com licença própria. O Odoo
  Enterprise é proprietário: só se usa com subscrição e a licença proíbe distribuir ou vender cópias. Os módulos da loja
  Odoo Apps têm normalmente uma licença proprietária que proíbe copiar código.
- **Tecnologia.** O Odoo é Python, PostgreSQL e um framework JavaScript próprio (OWL); o KanGest é Rust, SQLite e React,
  a funcionar sem internet. Nada se transplanta: teria de ser reescrito na mesma, por isso copiar não poupa trabalho.
- **Marca e aspecto.** Também não se devem copiar ícones, textos, ecrãs ao pixel nem o nome.

**Ideias: sim.** O comportamento de um programa — uma paleta de comandos, uma barra de estado, botões com os documentos
ligados, regras de reposição de stock — não está protegido. Estuda-se como funciona e implementa-se de raiz, no estilo do
KanGest. Foi o que se fez nesta ronda, e é o caminho para as lacunas da secção 4 (kanban, actividades, anexos, listas de
preços, pivot).

*Isto não é parecer jurídico; num caso concreto de reutilização de código, confirme com um advogado.*

## 6. Roteiro proposto

| Fase | Quando | O quê |
|---|---|---|
| 1 | Até 15 de Fevereiro de 2027 | SAF-T de inventários (2), homologação da Facturação Electrónica (1), exportação .xlsx (6), selector de data (17) |
| 2 | Até ao fim do 1.º trimestre de 2027 | PDF e envio por e-mail/WhatsApp (3), importação de Excel (4), sugestão de encomenda (5), integração contabilística (7) |
| 3 | 2.º e 3.º trimestres de 2027 | Anexos e actividades (8, 9), kanban (10), listas configuráveis (11), tabela dinâmica (12), conciliação bancária (13), listas de preços (14) |
| 4 | A partir do 4.º trimestre de 2027 | Contabilidade (18), consulta no telemóvel (19), unidades e variantes (15), fidelização (16), multi-empresa (20) |

## Como ver os ecrãs

Siga [scripts/auditoria/README.md](../scripts/auditoria/README.md): o servidor de demonstração e o frontend no browser
mostram todos os ecrãs com dados de exemplo. Para o tema escuro ou as tabelas compactas, escolha-os no menu do utilizador
(canto superior direito).

## Fontes

- Odoo 19 — notas de versão: https://www.odoo.com/odoo-19-release-notes
- Odoo 20 — notas de versão: https://www.odoo.com/odoo-20-release-notes
- Licenças do Odoo (Community LGPL-3, Enterprise): https://www.odoo.com/documentation/master/legal/licenses.html
- Cegid Angola — facturação electrónica obrigatória: https://www.cegid.com/ao/lp/transverse-e-invoice-facturacao-electronica/
- Cegid — software preparado para a facturação electrónica em Angola: https://www.cegid.com/ao/imprensa/cegid-software-preparado-para-facturacao-electronica-angola/
- Cegid PHC — módulos (Angola): https://www.cegid.com/ao/produtos/cegid-phc-software-gestao/modulos/
- Cegid PHC Bancos: https://www.cegid.com/ib/pt/produtos/cegid-phc-software-gestao/modulo-bancos/
- KPMG Angola — SAF-T de inventários: https://kpmg.com/ao/pt/insights/tax-news/saf-t-inventarios.html
- KPMG Angola — SAF-T de contabilidade (2025 facultativo, obrigatório a partir de 2026): https://kpmg.com/ao/pt/insights/tax-news/saf-t-contabilidade.html
- EY Angola — submissão do SAF-T de inventários até 15 de Abril de 2026: https://www.ey.com/pt_ao/technical/tax-alerts/submissao-do-saf-t-de-inventarios-ate-15-de-abril-de-2026
- Módulos de terceiros do Odoo para Angola (ex.: SAF-T de salários): https://apps.odoo.com/apps/modules/19.0/l10n_ao_saft_payroll
- Cegid Primavera v10 — guias de programação (listas, painéis de informação relacionada): https://developers.ila.cegid.com/en/v10/resources/guides/article/how-to-create-related-information-panels-with-visual-studio
- Vendus — facturação electrónica em Angola: https://www.vendus.pt/blog/faturacao-eletronica-obrigatoria-angola/

# Comparação: KIGEST (GC + POS) × KanGest 1.0

Este documento compara o sistema antigo com o novo em três planos: a tecnologia, as funções e os dados.
A lista detalhada, menu a menu, está em [COBERTURA.md](COBERTURA.md); a migração passo a passo em [MIGRACAO-KIGEST.md](MIGRACAO-KIGEST.md) e [BUILD.md](BUILD.md).

## 1. Tecnologia

| | KIGEST | KanGest 1.0 |
|---|---|---|
| Programas | Dois: `KanGest GC.exe` (gestão) e `KanGestPos.exe` (venda) | Um só programa com a gestão e o POS, com as mesmas permissões por utilizador |
| Sistemas | Windows (Delphi 32-bit) | Windows 10/11 e macOS |
| Base de dados | SQL Server instalado à parte, com a senha do `sa` em texto simples em cada posto | SQLite dentro do programa, sem servidor de base de dados para instalar |
| Vários postos | Todos ligados ao SQL Server | Um PC é o servidor e os outros ligam-se por rede com chave (Configurações → Rede) |
| Relatórios | FastReport (`.fr3`) | Impressão do próprio programa (A4, A5, talão de 80 mm) e exportação CSV; os relatórios de vendas incluem o histórico importado do KIGEST |
| Impressora térmica | Pelo controlador do Windows | Controlador do sistema ou ESC/POS directo, com corte de papel e gaveta |
| Cópias de segurança | `.bak` do SQL Server | Cópia automática diária e manual, e uma cópia antes de cada importação ou actualização |
| Actualizações | Manual | Sobre → Actualizações (endereço ou pasta partilhada, verificação SHA-256) |
| Facturação electrónica AGT | Não tem | Envio por API com JWS, séries electrónicas e QR code (falta testar com credenciais reais na homologação) |

## 2. Funções

| Área | KIGEST | KanGest 1.0 |
|---|---|---|
| POS | Venda com grelha, leitor de código de barras, desconto, alterar preço, suspender, notas e proformas, multi-pagamento, abertura e fecho de caixa | Tudo igual, mais kits, escolha de lote, gaveta e impressão ESC/POS directa |
| Vendas | FT, FR, VD, PP, OR, NE, NC, ND, GT, RC, autofacturação, adiantamentos, retenções | Igual |
| Compras | V/Factura, V/FR, V/NC, V/ND, encomendas, moeda estrangeira e custos de importação, pagamentos a fornecedores | Igual |
| Stock | Entradas, saídas, transferências, inventário, lotes, rastreabilidade, recálculo, etiquetas | Igual |
| Tesouraria | Fluxo de caixa, contas e bancos, centros de custo, contas a pagar e a receber, extractos e avisos | Igual |
| Fiscal | SAF-T de facturação, mapa de IVA | SAF-T de facturação, de aquisição e do Estado (validados contra o XSD oficial), mapa de IVA, IRT Grupo C |
| Análise de vendas | Relatórios por artigo, cliente, dia, operador…; «Relatório Síntese» feito à parte (Word/PowerPoint) | Os mesmos relatórios, mais «Por mês» e o **Relatório Síntese (mês a mês)**: cada mês do ano ao lado do ano anterior, variação, acumulados, média e melhor mês |
| RH e salários | Funcionários, subsídios, alterações mensais, adiantamentos, IRT e INSS, recibos, mapas, honorários | Igual, com a tabela de IRT da Lei 14/25 editável e prestações sociais |
| Oficina | Viaturas e ordens de serviço | Igual, com facturação da OS |
| Sistema | Utilizadores, empresa, séries, parametrização de documentos | Igual, mais auditoria de operações e importação do KIGEST |

**Única falta conhecida:** o carregamento directo dos mapas de remunerações nos portais do INSS e da AGT. O KanGest exporta o CSV; o formato de carregamento dos portais não está publicado.

**Diferenças de uso:**

- O visual é novo; os menus seguem os mesmos grupos do GC.
- Os atalhos de teclado do POS (F2, F3, F4…) continuam a funcionar, mas já não aparecem escritos nos botões. A lista completa está em Sistema → Manual e Atalhos.
- Os utilizadores importados entram com **senhas provisórias** e permissões a rever.
- As vendas novas usam **séries novas** (por exemplo `FR N2026/1`). Os documentos do KIGEST ficam como histórico, com as assinaturas originais, e não se anulam: usa-se nota de crédito.
- O SAF-T dos meses anteriores à mudança continua a ser exportado no KIGEST.

## 3. Dados

O importador lê a base `KigestGC` directamente. Cada tabela do KIGEST, e o que lhe acontece:

### Importadas

| Dados | Tabelas do KIGEST | No KanGest |
|---|---|---|
| Empresa | `tbzconfigCompany` | Dados da empresa, bancos e logótipo |
| IVA, isenções, unidades, famílias, subfamílias, armazéns | `tbzconfigTaxasIva`, `tbzconfigTaxaMotivoIsencao`, `tbArtigosUnidade`, `tbArtigosFamilia`, `tbArtigosSubFamilia`, `tbzconfigArmazem` | Tabelas do sistema |
| Artigos | `tbArtigos` | Artigos com os 4 preços, custo, IVA, mínimo e máximo |
| Stock | `tbArtigosStock` | Existências por armazém (saldo inicial) |
| Lotes | `tbArtigosLotes` | Lotes com validade e saldo |
| Clientes e fornecedores | `tbClientes`, `tbFornecedores` | Entidades |
| Utilizadores | `tbUsuario` | Utilizadores com PIN e senha provisória |
| Documentos | `tbDocumentosCabessalho`, `tbDocumentosDetalhe`, `tbDocsModoPagamentos`, `tbzconfigTiposDocumentos` | Histórico com linhas, pagamentos e assinaturas |
| Conta corrente | `tbContasCorrente` | Valores em dívida nas facturas importadas |
| Recibos | `vsSAFT_REC_Cabecalho`, `vsSAFT_ReciboLinha`, `vsTotalporPagtoRecibos` | Recibos ligados às facturas |
| Retenções | `tbDocsRetencaoFonte` | Retenções de cada documento |
| Custos de importação | `tbDocsDetalhesCustoMecadorias`, `tbzconfigTiposCustosMercadorias` | Frete, seguro, despacho… de cada compra, em Kz |
| Proformas e orçamentos | `vsDocsProformaCabessalho`, `vsDocsProformaDetalhes` | Histórico |
| Tesouraria | `tbFluxoCaixaContas`, `tbBancos`, `tbBancosContas`, `tbzconfigCentroCusto`, fluxo de caixa | Contas, centros de custo e movimentos (mesmo saldo) |
| Caixa do POS | `tbCaixaDiaria` | Fechos de caixa |
| Moedas e prazos | `tbzconfigMoeda`, `tbzconfigTiposPagamento` | Câmbios e prazos |
| RH | `tbRhFuncionarios`, departamentos, funções, categorias, `tbRhAgregadoFamiliar`, `tbRhMapaFerias` | Funcionários e tabelas do RH |
| Salários | `tbRhProcessamentoCabessalho`, `tbRhProcessamentoRecibo`, honorários, `tbRhAlteracoesMensais`, `tbRhAdiantamentoSalario` | Meses processados, recibos e o que está por processar |
| Oficina | `vsOficinaFichaAutomovel`, `vsOrdemServicoCabessalho`, `vsOrdemServicoDetalhes` | Viaturas e ordens de serviço |

### Não importadas, e porquê

| Tabela do KIGEST | Motivo |
|---|---|
| `tbDocsCabessalhoSuspenso` (vendas suspensas) | Temporárias: devem ser fechadas no KIGEST antes de migrar |
| `tbUsuarioPrivilegio` (permissões) | Os códigos não correspondem às permissões novas; revêem-se em Configurações → Utilizadores |
| `tbArtigosCategoria`, `tbArtigosTipoInventario` | O KanGest organiza os artigos por família e subfamília |
| `tbRhFuncSubsidioSociasDetalhes` (subsídio de maternidade e outras prestações pagas) | Histórico de prestações já pagas; as prestações em curso registam-se em Recursos Humanos → Prestações Sociais |
| `tbzconfigModosPagamento`, `tbZconfigRegimeIva`, `tbZconfigMeses` | Tabelas fixas que já existem no KanGest |
| Séries antigas e configurações internas | As séries novas são criadas por posto e por ano |

Qualquer outra tabela com dados que o importador não use aparece na análise («Ligar e analisar» → «Não importadas», com as colunas) e pode ser exportada para o suporte rever.

### Conferência depois de importar

No fim de cada simulação ou importação, o ecrã mostra a **Conferência KIGEST × KanGest**, com o total de cada lado e a diferença:

- artigos, clientes, fornecedores e funcionários;
- quantidade total em stock;
- documentos importados;
- valores em dívida (conta corrente);
- saldo das contas de tesouraria.

O lado do KanGest conta só o que veio do KIGEST. Diferenças normais: artigos com a mesma referência juntam-se num só, e os registos sem artigo, conta ou documento conhecido ficam de fora. Os documentos cujo número já exista no KanGest também não entram. Cada caso aparece nos avisos.

## 4. Como importar todos os dados

Os dados estão no SQL Server do computador **DESKTOP-G26LDJM**:

1. No KanGest: Sistema → Importar dados do KIGEST → **Guardar o ExportarKIGEST** e copiar `ExportarKIGEST.bat` e `ExportarKIGEST.ps1` para uma pen.
2. No DESKTOP-G26LDJM: fechar o GC e o POS e fazer duplo clique em `ExportarKIGEST.bat`. Fica um ficheiro `.kigest` no Ambiente de Trabalho.
3. No KanGest: **Ficheiro exportado** → escolher o ficheiro → **Analisar** → **Simular** → rever a conferência → **Importar dados**.

Se o KanGest for instalado no próprio DESKTOP-G26LDJM, também pode ligar-se directamente ao SQL Server (ver [BUILD.md](BUILD.md)).

# Migração de dados: KIGEST → KanGest novo

O ecrã está em **Sistema → Importar dados do KIGEST** e só aparece para utilizadores com perfil **Administrador**. Há duas fontes:

- **Ficheiro exportado (.kigest)** — no computador do KIGEST corre-se o `ExportarKIGEST.bat` (botão «Guardar o ExportarKIGEST», ou a pasta `ferramentas/exportar-kigest/`). Usa o PowerShell do Windows: lê o `Conexao.ini`, liga-se ao SQL Server, só lê a base e grava todas as tabelas e as vistas usadas num ficheiro `.kigest` (JSON por linhas, gzip). As colunas de senhas ficam de fora. O ficheiro importa-se em qualquer computador, sem rede entre os dois.
- **Ligação directa ao SQL Server** — quando o KanGest está no mesmo computador ou na mesma rede que o SQL Server.

A importação é igual nos dois casos (mesmas tabelas, simulação, conferência e avisos).

## O que é importado

| Conjunto | Tabelas do KIGEST | Notas |
|---|---|---|
| Empresa | `tbzconfigCompany` | Nome, NIF, morada, telefones, email, rodapé, bancos/IBAN e logótipo (PNG/JPEG). O n.º do certificado do KIGEST (ex.: 123/AGT/2020, lido do texto «Processado por programa validado nº…») fica como referência para as 2.ªs vias do histórico |
| Tabelas | `tbzconfigTaxasIva`, `tbzconfigTaxaMotivoIsencao`, `tbArtigosUnidade`, `tbArtigosFamilia`, `tbArtigosSubFamilia`, `tbzconfigArmazem` | Reaproveita o que já existir (mesma taxa, unidade, família ou armazém) |
| Artigos | `tbArtigos` | Referência, código de barras, 4 preços, custo, IVA e isenção, stock mínimo e máximo, promoção (`Promocao`, `PromoData_ini`, `PromoData_Fim`, `Desconto`/`ValDesconto`). Os serviços são reconhecidos pela família. Todos os produtos movimentam stock (no KIGEST as vendas abatem o stock mesmo com `ControloStock = N`); `ControloStock = N` ou `StockZerado = S` passa a «permite vender sem stock» |
| Stock | `tbArtigosStock` (ou `tbArtigos.QtdStock`) | Existência por armazém, custo médio e as datas de última entrada e saída. Fica registado o movimento "Saldo inicial importado do KIGEST". |
| Clientes / Fornecedores | `tbClientes` / `tbFornecedores` (ou as vistas `vsClientes` / `vsFornecedores`) | O "Consumidor Final" é associado ao do sistema novo. Os códigos de cliente podem ser números ou texto (ex.: «Filial centro», «000001234») e são usados tal como estão |
| Utilizadores | `tbUsuario` | **Senhas provisórias** (as do KIGEST não são reaproveitáveis): cada utilizador tem de escolher uma senha nova no primeiro acesso. O PIN é mantido. O perfil fica CAIXA, excepto contas "admin". |
| Documentos | `tbDocumentosCabessalho`, `tbDocumentosDetalhe`, `tbDocsModoPagamentos`, `tbzconfigTiposDocumentos` | Histórico só de leitura, com os números, as datas, as linhas e as **assinaturas originais**. Anulado = `EstadoDocsTotal` ≠ 0 (o critério dos relatórios do KIGEST); o `EstadoDocumento` é ignorado, porque há bases com «A» em todos os documentos. Totais: sem imposto (`ValSemTaxa`), descontos de linha (`ValDescontoLinha`) e global (`ValDesconto`) |
| Conta corrente | `tbContasCorrente` | Os valores em dívida passam para as facturas importadas: podem emitir-se recibos sobre elas no sistema novo |
| Recursos humanos | `tbRhFuncionarios`, `tbZconfigRhDepartamento`, `tbZconfigRhFuncao`, `tbZconfigRhCategoria`, `tbRhAgregadoFamiliar`, `tbRhMapaFerias` | Ficha completa, salário base e subsídios fixos, IBAN, descontos INSS/IRT, contrato, admissão e saída; trabalhadores por conta própria marcados como independentes; agregado familiar; férias (um período por funcionário, o mapa mensal do KIGEST não se repete). Confirme a tabela de IRT antes do primeiro processamento. |
| Lotes | `tbArtigosLotes` | Lote, validade, fabrico, custo e quantidade como **saldo inicial** do lote. O artigo só passa a ser controlado por lote se, em todos os armazéns, a soma dos lotes for igual ao stock (aviso nos restantes). |
| Oficina | `vsOficinaFichaAutomovel`, `vsOrdemServicoCabessalho`, `vsOrdemServicoDetalhes` | Viaturas (com o cliente), ordens de serviço com diagnóstico, km, peças e serviços; a OS fica ligada à factura histórica (`DocVenvaAssociado`). OS sem viatura conhecida não entram (aviso). |
| Recibos | `vsSAFT_REC_Cabecalho` (ou `tbReciboCabessalho`), `vsSAFT_ReciboLinha`, `vsTotalporPagtoRecibos` | Recibos de clientes e pagamentos a fornecedores, ligados às facturas importadas, com os meios de pagamento. Não alteram os valores em dívida (que vêm da conta corrente), mas completam o extracto. |
| Retenções na fonte | `tbDocsRetencaoFonte` | IRT, II, IS, IPU… de cada documento importado |
| Custos de importação | `tbDocsDetalhesCustoMecadorias`, `tbzconfigTiposCustosMercadorias` | Frete, seguro, despacho… de cada compra importada, pelo contravalor em Kz |
| Proformas e orçamentos | `vsDocsProformaCabessalho` (ou `tbProformas`), `vsDocsProformaDetalhes` | Histórico com as linhas e a validade |
| Tesouraria | `tbFluxoCaixaContas`, `tbBancos`, `tbBancosContas`, `tbzconfigCentroCusto`, `tbFlulxoaixaCabessalho` (ou `tbFluxoCaixa`) | Contas de caixa e banco (com IBAN), centros de custo e todos os movimentos: o saldo de cada conta fica igual ao do KIGEST |
| Fechos de caixa | `tbCaixaDiaria` | Sessões de caixa do POS (abertura, fecho, saldo inicial e contado) |
| Moedas e prazos | `tbzconfigMoeda`, `tbzconfigTiposPagamento` | Câmbios em vigor e prazos de pagamento |
| Salários (histórico) | `tbRhProcessamentoCabessalho`, `tbRhProcessamentoRecibo`, `vsRhProcesamentoHonorarios`, `tbRhAlteracoesMensais`, `tbRhAdiantamentoSalario` | Meses processados com os recibos e as linhas (pagos), honorários, e as alterações e parcelas de adiantamento ainda por processar |

A importação pode ser repetida: artigos, entidades, utilizadores e documentos já importados não são duplicados. Repetir também corrige o que versões anteriores do importador leram mal (estado anulado, descontos, cliente com código em texto, dívida — esta só se o documento ainda não teve recibos no KanGest — e o controlo de stock dos artigos).
Antes de importar, o programa faz uma cópia de segurança da base actual na pasta Transferências.

## Certificado AGT e assinaturas

- Os documentos do KIGEST ficam como **histórico**, com o hash e os 4 caracteres de controlo originais. As 2.ªs vias impressas indicam o certificado do KIGEST.
- O **n.º de certificado do KIGEST** fica guardado só como referência. Não é activado no sistema novo, porque o certificado AGT pertence ao produtor e ao programa certificado.
- A **chave privada** de assinatura do KIGEST não é extraída.
  - Se for o titular do certificado do programa, carregue a sua chave em **Configurações → Certificação AGT → Carregar chave do produtor (PEM)**.
  - Caso contrário, peça a certificação do KanGest novo à AGT, entregando a chave pública mostrada nesse ecrã.
- As vendas novas continuam em **séries próprias**. Se não houver prefixo definido, o Posto#1 passa a usar "N", por exemplo `FR N2026/1`.
- Os documentos importados entram em todos os relatórios de vendas (por dia, mês, artigo, família, cliente, operador, IVA, Relatório Síntese) e no painel, mas não no SAF-T, no fluxo de caixa nem no IRT Grupo C do sistema novo. O SAF-T dos períodos anteriores à mudança deve ser exportado no KIGEST.
- Documentos importados não se anulam no sistema novo. Para devoluções, emita uma nota de crédito que refere a factura antiga.

## Como fazer a migração

O mais simples é instalar o KanGest no próprio PC do SQL Server (`DESKTOP-G26LDJM`) e usar o servidor `localhost`: não é preciso mexer no TCP/IP nem na Firewall (ver [BUILD.md](BUILD.md)). Se o KanGest ficar noutro computador:

1. **No PC onde está o SQL Server do KIGEST** (por exemplo `DESKTOP-G26LDJM`):
   - Abra o *SQL Server Configuration Manager* e active o **TCP/IP** em *Protocolos para MSSQLSERVER* (ou para SQLEXPRESS). Reinicie o serviço.
   - Numa instância com nome (`SERVIDOR\SQLEXPRESS`), confirme que o serviço **SQL Server Browser** está a correr.
   - No Firewall do Windows, permita a porta **1433/TCP** (e **1434/UDP** se usar instância com nome).
2. **No KIGEST**, exporte o SAF-T do período actual (Sistema → Exportar SAF-T) e feche o programa em todos os postos.
3. **No KanGest novo** (no mesmo PC ou noutro da mesma rede):
   1. Entre como Administrador e abra **Sistema → Importar dados do KIGEST**.
   2. Use **Ler Conexao.ini** e escolha o ficheiro `Conexao.ini` da pasta do KIGEST. Preenche servidor, base, utilizador e senha. Se o servidor for `localhost`, troque pelo nome ou IP do PC do SQL Server.
   3. Carregue em **Ligar e analisar** e confira as tabelas encontradas e o n.º de registos.
   4. Escolha o que importar e carregue em **Simular**: confira a **Conferência KIGEST × KanGest** (artigos, clientes, fornecedores, stock, documentos, dívidas, saldos de tesouraria e funcionários, de um lado e do outro) e os avisos.
   5. Carregue em **Importar dados**.
   6. Guarde ou imprima o relatório final, que inclui a conferência e as senhas provisórias dos utilizadores.
4. Reveja **Configurações** (empresa, postos e impressoras, certificação AGT) e **Sistema → Utilizadores e Permissões** (perfis e permissões de cada pessoa).
5. Confira o **stock** (Gestão de Stock → Mapa) e as **contas a receber** antes de começar a vender.

## Segurança

O `Conexao.ini` do KIGEST guarda a senha do utilizador `sa` em texto simples.
Depois da migração, recomenda-se mudar essa senha no SQL Server e, se a base antiga ficar só para consulta, criar um utilizador SQL só de leitura.


## O que não é importado

- Vendas suspensas no POS (`tbDocsCabessalhoSuspenso`) — são temporárias: feche-as no KIGEST antes da migração.
- Permissões dos utilizadores (`tbUsuarioPrivilegio`) — os códigos do KIGEST não correspondem às permissões novas; reveja-as em Sistema → Utilizadores e Permissões.
- Categorias e tipos de inventário dos artigos (`tbArtigosCategoria`, `tbArtigosTipoInventario`) — o KanGest usa família e subfamília.
- Histórico de prestações sociais pagas (`tbRhFuncSubsidioSociasDetalhes`) — as prestações em curso registam-se em Recursos Humanos → Prestações Sociais.
- Séries antigas, locais de entrega e configurações internas do KIGEST.

A comparação completa, tabela a tabela, está em [COMPARACAO-KIGEST.md](COMPARACAO-KIGEST.md).

A análise («Ligar e analisar») lista estas tabelas com o número de registos e permite **exportar a análise** (tabelas e colunas) para o suporte confirmar a correspondência dos campos antes da importação definitiva.

## Formato do ficheiro .kigest

Texto UTF-8, uma linha por objecto JSON, comprimido em gzip:

1. cabeçalho: `{"formato":"kigest-exportacao","versao":1,"origem":"SERVIDOR/KigestGC","data":"…","computador":"…"}`;
2. por tabela: `{"tabela":"tbArtigos","tipo":"TABELA","colunas":[…]}`, depois um registo por linha (`[valor, …]` pela ordem das colunas; datas em `AAAA-MM-DD HH:MM:SS`, binários em `{"b64":"…"}`), e `{"fim":"tbArtigos","registos":N}` (com `"erro"` se a tabela não pôde ser lida);
3. no fim: `{"concluido":true,…}`. Sem esta linha o ficheiro é considerado incompleto e não se importa.

## Ligar à base do KIGEST

Segundo o `Conexao.ini` do KIGEST GC, a base `KigestGC` está no SQL Server do computador **DESKTOP-G26LDJM** (utilizador `sa`). Para o KanGest novo chegar a ela a partir de outro computador:

1. No DESKTOP-G26LDJM, em *SQL Server Configuration Manager → Protocolos*, active **TCP/IP** e reinicie o serviço SQL Server.
2. Abra a porta **1433/TCP** na Firewall do Windows (e **1434/UDP** se usar uma instância com nome, ex.: `SQLEXPRESS`).
3. No KanGest, em Importar dados do KIGEST, use o nome do computador ou o seu endereço IP (ex.: `192.168.8.20`) — o IP é mais fiável quando os computadores são Windows e Mac na mesma rede.
4. Faça **Ligar e analisar** e confira as tabelas encontradas.
5. Faça **Simular**: a importação corre toda e é desfeita no fim, mostrando o que seria importado e os avisos, sem gravar nada.
6. Só depois **Importar dados** (é feita antes uma cópia de segurança). Pode repetir a importação: não duplica registos.

Referência de desempenho: 10 000 facturas com 30 000 linhas, 2 000 clientes e 3 000 recibos importam em cerca de 8 s numa compilação de testes (mais rápido na versão instalada).

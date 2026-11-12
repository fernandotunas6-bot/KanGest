# Análise do KIGEST (GC + POS) e plano do KanGest novo

## 1. O sistema actual

| Item | KIGEST actual |
|---|---|
| Tecnologia | Delphi (VCL) 32-bit, executáveis comprimidos com UPX |
| Componentes | TMS (AdvGrid, AdvEdit…), Raize, FastReport 2022 (`.fr3`), TurboPower LockBox (RSA/SHA1), Indy/OpenSSL |
| Base de dados | **SQL Server** (`SQLNCLI11`), base `KigestGC`, partilhada pelo GC e pelo POS na rede local |
| Executáveis | `KanGest GC.exe` (gestão, ~200 ecrãs) e `KanGestPos.exe` (ponto de venda) |
| Postos | Cada posto tem `PosConfig.ini` (n.º de posto, armazém, impressora de talão 80 mm, impressora A4) |
| Relatórios | 186 modelos FastReport no GC e 54 no POS (facturas A4/A5/talão, 2.ª via, mapas, RH) |
| Certificação | Assinatura RSA-SHA1 encadeada (`Dados.Txt`: `2019-07-29;2019-07-29T13:05:47;VD 1/4;81.23;<hash>`), exportação SAF-T AO (`UExportarSAFT`, `XSDSAFTAO101_01`) |
| Backup | `.bak` do SQL Server, diário, em `D:\BACKUP_KIGEST` |

### Módulos encontrados (unidades Delphi do `KanGest.map`)

- **Vendas:** Factura, Factura/Recibo, Factura Proforma, Orçamento, Encomenda de cliente, Nota de Crédito, Nota de Débito, Guia de Transporte, Venda a Dinheiro, Autofacturação, documentos suspensos, séries e numeração automática.
- **Compras:** Factura de compra, encomenda a fornecedor, custos adicionais de importação (moeda e câmbio), consumo interno.
- **Stock:** Entradas, saídas, transferências, lotes e validade, rastreabilidade, recálculo de stock, inventário, ruptura e excesso, etiquetas de preço e códigos de barras.
- **Conta corrente:** Recibos, estorno de recibos, adiantamentos de clientes, regularização, extractos, avisos de vencimento.
- **Tesouraria:** Fluxo de caixa, contas bancárias e caixas, depósitos, contas a pagar e a receber, centros de custo, abertura e fecho de caixa, devolução de dinheiro.
- **Retenções:** IRT, Imposto Industrial, Imposto de Selo, IPU, IAC (`tbDocsRetencaoFonte`).
- **RH / Salários:** Funcionários, agregado familiar, processamento salarial (IRT e INSS de Angola), subsídios (férias, Natal, alimentação, transporte, chefia, turno, falhas, arrendamento, atavio), horas extra, faltas, férias, adiantamentos, honorários de independentes, mapas INSS/IRT, recibos de salário e folha de pagamento bancária.
- **Oficina:** Viaturas (marca, modelo, cor, chassi, matrícula), ordens de serviço, peças e diagnóstico.
- **Sistema:** Utilizadores e permissões, empresa, moedas e câmbios, regimes de IVA, motivos de isenção, cópia de segurança, exportação para PDF e email, gráficos.

### Tabelas principais (reconstruídas a partir do SQL dos relatórios)

`tbArtigos` (4 preços c/ e s/ IVA, margens, custo, lotes, promoções), `tbArtigosStock` (por armazém), `tbArtigosLotes`, `tbArtigosFamilia` / `SubFamilia` / `Categoria` / `Unidade`, `tbDocumentosCabessalho` / `tbDocumentosDetalhe` (todos os documentos, incluindo dados de carga e descarga da guia de transporte), `tbDocsModoPagamentos`, `tbDocsRetencaoFonte`, `tbDocsDetalhesCustoMecadorias`, `tbContasCorrente`, `tbReciboCabessalho`, `tbFluxoCaixaContas`, `tbRh*` (funcionários, processamento, recibos, alterações mensais, adiantamentos, subsídios), `tbzconfig*` (tipos de documento, taxas de IVA, modos de pagamento, armazéns, moedas, empresa, meses, departamentos, funções e categorias).

## 2. O KanGest novo

| | Novo |
|---|---|
| Desktop | **Tauri 2** (Windows 10/11 com WebView2, macOS 10.15+), instalador `.exe` (NSIS) e `.dmg` (ver [BUILD.md](BUILD.md)) |
| Backend | Rust: `sqlx` (SQLite em modo WAL), `rsa` + `sha1` (assinatura AGT), `argon2` (senhas) |
| Frontend | React 19, TypeScript, Tailwind 4, TanStack Query, Zustand |
| API interna | Um único comando `rpc(method, params)`. O mesmo despachante pode ser servido por HTTP para postos em rede (fase 5). |
| Dinheiro | Inteiros em cêntimos, sem erros de vírgula flutuante |

### Mapeamento antigo → novo

| KIGEST | KanGest novo |
|---|---|
| `tbArtigos` + `tbArtigosStock` | `artigos` + `stock` (por armazém) + `movimentos_stock` (extracto com saldo) |
| `tbDocumentosCabessalho` / `Detalhe` | `documentos` / `documento_linhas` |
| `tbDocsModoPagamentos` | `documento_pagamentos` |
| `tbContasCorrente` / `tbReciboCabessalho` | `documentos.valor_pendente` + `recibo_documentos` (liquidações por recibo ou NC) |
| `tbzconfigTiposDocumentos` | `tipos_documento` (efeito em stock, conta corrente, pagamento no acto e assinatura) |
| Séries / `UAutoNumeracao` | `series` (por tipo e ano, com o último hash da cadeia) |
| `UAberturaCaixa` / fecho | `caixa_sessoes` + `caixa_movimentos` |
| `tbUsuario` | `utilizadores` (perfis ADMIN, GESTOR e CAIXA) |
| `PosConfig.ini` | `postos` + configuração local (fase 5) |

### Fase 1 (feita): núcleo comercial

- [x] Configuração inicial (empresa e administrador), login e perfis
- [x] Artigos: 4 níveis de preço, IVA e motivos de isenção, código de barras, stock mínimo e máximo
- [x] Clientes e fornecedores com saldo em dívida
- [x] **POS**: leitor de código de barras (`3*código` para quantidades), grelha por família, cliente (F4), pagamento multi-modo com troco (F2), FR ou FT a crédito, talão de 80 mm
- [x] Documentos: FT, FR, NC (com origem), PP, OR (conversão em factura), FC, ES, SS, TS, RC
- [x] Assinatura RSA-SHA1 encadeada por série, 4 caracteres de controlo e n.º do certificado no documento
- [x] Stock com custo médio ponderado, extracto de movimentos e stock negativo controlado
- [x] Conta corrente: facturas em dívida, recibos e notas de crédito
- [x] Caixa: abertura, entradas e saídas, fecho com contagem e relatório
- [x] Anulação com motivo (repõe stock e pendentes) e auditoria
- [x] Painel e relatórios: diária, por artigo (com margem), por família, cliente, operador, forma de pagamento e mapa de IVA
- [x] Impressão A4 e talão (2.ª via incluída)
- [x] Exportação **SAF-T (AO)** 1.01_01 por período (facturas, NC, proformas e orçamentos, recibos; clientes, produtos, tabela de IVA). Tem de ser validada no validador oficial da AGT antes da certificação.

### Fase 2: fiscal e documentos em falta
- Validação do SAF-T contra o XSD oficial e ajustes pedidos pela AGT
- Guia de Transporte com dados de carga e descarga, Nota de Débito e Autofacturação
- Retenção na fonte (IRT, Imposto Industrial, Selo, IPU, IAC) nos documentos
- Lotes e validade, etiquetas e códigos de barras, inventário
- Impressão **ESC/POS directa** (crate `escpos` ou `tauri-plugin-thermal-printer`) com abertura de gaveta
- Compras com moeda estrangeira, câmbio e custos adicionais

### Fase 3: tesouraria e operações
- Fluxo de caixa, contas bancárias, depósitos, contas a pagar e a receber, centros de custo
- Conta corrente de fornecedores, encomendas de clientes e fornecedores
- Oficina: viaturas e ordens de serviço

### Fase 4: RH e salários
- Funcionários, agregado, processamento com IRT e INSS (tabelas de Angola), subsídios, faltas, férias, adiantamentos, recibos de salário, mapas e folha bancária

### Fase 5: rede, migração e distribuição
- **Modo servidor** para vários postos na LAN (o despachante RPC exposto por HTTP com token)
- **Migração** a partir do SQL Server `KigestGC` (artigos, clientes, fornecedores, saldos e stock), lendo directamente com a crate `tiberius`
- Cópias de segurança automáticas, actualizações automáticas (`tauri-plugin-updater`), assinatura de código para Windows e macOS
- **Facturação electrónica AGT** (envio por API com JWS), que está a ser introduzida em 2025/2026

## 3. Recursos no GitHub avaliados

| Projecto | Para quê |
|---|---|
| [Mahalli (AbdelilahOu/mahalli-tauri)](https://github.com/AbdelilahOu/Mahalli-tauri) | Inventário e facturação em Tauri e SQLite: referência de domínio |
| [Quick Invoice](https://github.com/chinmaygarg/Quick-Invoice) | Tauri, sqlx e React: padrão de facturação |
| [abilityapps/tauri-template](https://awesome.ecosyste.ms/projects/github.com%2Fabilityapps%2Ftauri-template) | Modelo React, shadcn e sqlx com migrações |
| [`escpos` (crate)](https://docs.rs/escpos) / [`tauri-plugin-thermal-printer`](https://docs.rs/crate/tauri-plugin-thermal-printer/2.0.1) | Impressão térmica directa (fase 2) |
| [onsoft-agt (Packagist)](https://packagist.org/packages/productiononschool/onsoft-agt) | Referência do envio de facturas à AGT (RSA-SHA1 e JWS) |

## 4. Avisos importantes

1. **Certificação AGT:** o software novo tem de ser certificado pela AGT com uma chave do produtor. A chave gerada no arranque serve para desenvolvimento. Antes de usar em produção, entregue a chave pública (Configurações → Certificação AGT) e registe o n.º do certificado.
2. **Segurança do sistema antigo:** o `Conexao.ini` guarda a senha do utilizador `sa` do SQL Server em texto simples, em todos os postos. Recomenda-se mudar essa senha e criar um utilizador SQL só de leitura para a migração.
3. **Recursos do sistema antigo:** os modelos `.fr3`, as imagens e o código do KIGEST não foram copiados. Serviram apenas para perceber as funcionalidades e as regras de negócio.

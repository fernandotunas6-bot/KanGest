/** Permissões por operação, agrupadas como no ecrã de utilizadores do KIGEST. */
export const GRUPOS_PERMISSOES: { grupo: string; itens: { id: string; label: string }[] }[] = [
  {
    grupo: "Stock",
    itens: [
      { id: "artigos.criar", label: "Adicionar artigo" },
      { id: "artigos.editar", label: "Editar artigo" },
      { id: "stock.entrada", label: "Entrada de stock" },
      { id: "stock.saida", label: "Saída de stock" },
      { id: "stock.transferencia", label: "Transferência" },
      { id: "stock.inventario", label: "Inventário" },
    ],
  },
  {
    grupo: "Vendas",
    itens: [
      { id: "vendas.fatura", label: "Factura" },
      { id: "vendas.fatura_recibo", label: "Factura/Recibo" },
      { id: "vendas.proforma", label: "Proforma / Orçamento" },
      { id: "vendas.nota_credito", label: "Nota de crédito" },
      { id: "vendas.nota_debito", label: "Nota de débito" },
      { id: "vendas.limite_credito", label: "Vender acima do limite de crédito" },
      { id: "vendas.documento_manual", label: "Recuperar documentos manuais" },
      { id: "clientes", label: "Clientes" },
      { id: "documentos.anular", label: "Anular documentos" },
    ],
  },
  {
    grupo: "Compras e tesouraria",
    itens: [
      { id: "compras.fatura", label: "Facturas de compra" },
      { id: "fornecedores", label: "Fornecedores" },
      { id: "tesouraria.recibo", label: "Recibo de cliente" },
      { id: "tesouraria.pagamento", label: "Pagamento a fornecedor" },
      { id: "tesouraria.fluxo", label: "Fluxo de caixa e contas" },
      { id: "tesouraria.regularizar", label: "Regularizar conta corrente" },
      { id: "caixa.movimentos", label: "Inserir/retirar dinheiro do caixa" },
    ],
  },
  {
    grupo: "POS (retalho)",
    itens: [
      { id: "pos.alterar_preco", label: "Alterar preço" },
      { id: "pos.desconto", label: "Desconto" },
      { id: "pos.quantidade", label: "Alterar quantidade" },
      { id: "pos.eliminar_linha", label: "Eliminar linha" },
      { id: "pos.suspenso", label: "Documentos suspensos" },
      { id: "pos.fecho_caixa", label: "Fecho de caixa" },
      { id: "pos.relatorio", label: "Documentos emitidos" },
      { id: "pos.config_posto", label: "Configurar posto e impressora" },
    ],
  },
  {
    grupo: "Recursos humanos e oficina",
    itens: [
      { id: "rh.funcionarios", label: "Fichas de funcionários e férias" },
      { id: "rh.processamento", label: "Processamento salarial e mapas" },
      { id: "rh.configuracao", label: "Configuração salarial (IRT, INSS, rubricas)" },
      { id: "oficina", label: "Oficina (viaturas e ordens de serviço)" },
    ],
  },
  {
    grupo: "Relatórios e fiscal",
    itens: [
      { id: "relatorios.ver", label: "Relatórios" },
      { id: "sistema.saft", label: "Exportar SAF-T" },
      { id: "sistema.series", label: "Séries de documentos" },
      { id: "sistema.fe", label: "Facturação Electrónica (AGT)" },
      { id: "sistema.certificacao", label: "Certificação AGT" },
    ],
  },
  {
    grupo: "Administração",
    itens: [
      { id: "sistema.utilizadores", label: "Utilizadores, perfis e segurança" },
      { id: "sistema.auditoria", label: "Registo de acessos e auditoria" },
      { id: "sistema.configuracoes", label: "Empresa e rede" },
      { id: "sistema.tabelas", label: "Tabelas (famílias, unidades, armazéns, IVA)" },
      { id: "sistema.parametrizacao", label: "Parametrização de documentos" },
      { id: "sistema.backup", label: "Cópia de segurança" },
      { id: "sistema.atualizacoes", label: "Actualizações" },
      { id: "sistema.importar", label: "Importar dados do KIGEST" },
    ],
  },
];

export const TODAS_PERMISSOES = GRUPOS_PERMISSOES.flatMap((g) => g.itens.map((i) => i.id));

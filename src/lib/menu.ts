import {
  Boxes,
  Contact,
  Landmark,
  Settings,
  ShoppingBag,
  Truck,
  UsersRound,
  Wallet,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { Perfil } from "./types";

export interface ItemMenu {
  label: string;
  to?: string;
  perm?: string;
  minPerfil?: Perfil;
  emBreve?: boolean;
}

export interface GrupoMenu {
  id: string;
  titulo: string;
  icon: LucideIcon;
  itens: ItemMenu[];
}

/** Menu igual à barra de navegação do KIGEST GC (grupos e itens pela mesma ordem). */
export const MENU: GrupoMenu[] = [
  {
    id: "stock",
    titulo: "Gestão de Stock",
    icon: Boxes,
    itens: [
      { label: "Lista de Artigos", to: "/artigos" },
      { label: "Documento de Stock", to: "/documentos/novo/ES", perm: "stock.entrada" },
      { label: "Saída de Stock", to: "/documentos/novo/SS", perm: "stock.saida" },
      { label: "Inventário", to: "/inventario", perm: "stock.inventario" },
      { label: "Transferência de Stock", to: "/documentos/novo/TS", perm: "stock.transferencia" },
      { label: "Consumo Interno", to: "/documentos/novo/CI", perm: "stock.saida" },
      { label: "Mapa e Movimentos de Stock", to: "/stock" },
      { label: "Documentos de Stock", to: "/documentos?grupo=STOCK" },
      { label: "Lotes de Artigo", to: "/lotes" },
      { label: "Relatórios de Stock", to: "/relatorios/stock", perm: "relatorios.ver" },
      { label: "Lista de Preços", to: "/relatorios/stock?situacao=precos", perm: "relatorios.ver" },
      { label: "Previsão de Venda", to: "/relatorios/stock?situacao=previsao", perm: "relatorios.ver" },
      { label: "Rastreabilidade de Lotes", to: "/relatorios/rastreio", perm: "relatorios.ver" },
      { label: "Etiquetas e Códigos de Barras", to: "/etiquetas" },
      { label: "Recálculo de Stock", to: "/stock/recalculo", perm: "stock.inventario" },
    ],
  },
  {
    id: "vendas",
    titulo: "Vendas",
    icon: ShoppingBag,
    itens: [
      { label: "Facturas", to: "/documentos/novo/FT", perm: "vendas.fatura" },
      { label: "Factura/Recibo", to: "/documentos/novo/FR", perm: "vendas.fatura_recibo" },
      { label: "Factura Proforma", to: "/documentos/novo/PP", perm: "vendas.proforma" },
      { label: "Nota de Crédito", to: "/documentos/novo/NC", perm: "vendas.nota_credito" },
      { label: "Nota de Débito", to: "/documentos/novo/ND", perm: "vendas.nota_debito" },
      { label: "Nota de Encomenda", to: "/documentos/novo/NE", perm: "vendas.proforma" },
      { label: "Orçamento", to: "/documentos/novo/OR", perm: "vendas.proforma" },
      { label: "Guia de Transporte", to: "/documentos/novo/GT", perm: "vendas.fatura" },
      { label: "Recibo de Cliente", to: "/recibos/novo", perm: "tesouraria.recibo" },
      { label: "Adiantamento de Cliente", to: "/adiantamentos/novo", perm: "tesouraria.recibo" },
      { label: "Estorno de Recibo", to: "/recibos/estorno", perm: "tesouraria.recibo" },
      { label: "Documentos Emitidos", to: "/documentos?grupo=VENDAS" },
    ],
  },
  {
    id: "compras",
    titulo: "Compras",
    icon: Truck,
    itens: [
      { label: "V/Factura", to: "/documentos/novo/FC", perm: "compras.fatura" },
      { label: "V/Factura Recibo", to: "/documentos/novo/VFR", perm: "compras.fatura" },
      { label: "V/Nota de Crédito", to: "/documentos/novo/VNC", perm: "compras.fatura" },
      { label: "V/Nota de Débito", to: "/documentos/novo/VND", perm: "compras.fatura" },
      { label: "Encomenda a Fornecedor", to: "/documentos/novo/VNE", perm: "compras.fatura" },
      { label: "V/Orçamento", to: "/documentos/novo/VOR", perm: "compras.fatura" },
      { label: "V/Guia de Transporte", to: "/documentos/novo/VGT", perm: "compras.fatura" },
      { label: "Factura/Recibo (Autofacturação)", to: "/documentos/novo/AF", perm: "compras.fatura" },
      { label: "Pagamento a Fornecedor", to: "/pagamentos/novo", perm: "tesouraria.pagamento" },
      { label: "Estorno de Pagamento", to: "/pagamentos/estorno", perm: "tesouraria.pagamento" },
      { label: "Mapa de Fornecedor", to: "/relatorios/fornecedores", perm: "relatorios.ver" },
      { label: "Moedas e Câmbios", to: "/moedas", perm: "compras.fatura" },
      { label: "Documentos Emitidos", to: "/documentos?grupo=COMPRAS" },
    ],
  },
  {
    id: "tesouraria",
    titulo: "Tesouraria",
    icon: Wallet,
    itens: [
      { label: "Fluxo de Caixa", to: "/tesouraria", perm: "tesouraria.fluxo" },
      { label: "Contas a Receber", to: "/contas/receber" },
      { label: "Contas a Pagar", to: "/contas/pagar", perm: "tesouraria.pagamento" },
      { label: "Aviso de Vencimento", to: "/avisos" },
      { label: "Caixa e Fechos (POS)", to: "/caixa" },
    ],
  },
  {
    id: "financas",
    titulo: "Gestão de Finanças",
    icon: Landmark,
    itens: [
      { label: "Contas e Centros de Custo", to: "/tesouraria/configuracao", perm: "tesouraria.fluxo" },
      { label: "Relatórios e Mapas de Imposto", to: "/relatorios", perm: "relatorios.ver" },
      { label: "Relatório Síntese (mês a mês)", to: "/relatorios?agrupar=sintese", perm: "relatorios.ver" },
      { label: "Exportar SAF-T (AO)", to: "/relatorios?saft=1", perm: "sistema.saft" },
      { label: "SAF-T de Aquisição (compras)", to: "/relatorios?saft=compras", perm: "sistema.saft" },
      { label: "SAF-T de Facturação ao Estado", to: "/relatorios?saft=estado", perm: "sistema.saft" },
    ],
  },
  {
    id: "rh",
    titulo: "Recursos Humanos",
    icon: UsersRound,
    itens: [
      { label: "Funcionários", to: "/rh/funcionarios", perm: "rh.funcionarios" },
      { label: "Alterações do Mês", to: "/rh/salarios?vista=alteracoes", perm: "rh.processamento" },
      { label: "Processamento Salarial", to: "/rh/salarios?vista=processar", perm: "rh.processamento" },
      { label: "Recibos, Mapas IRT / INSS", to: "/rh/salarios?vista=processamentos", perm: "rh.processamento" },
      { label: "Férias e Faltas", to: "/rh/ferias", perm: "rh.funcionarios" },
      { label: "Honorários (Conta Própria)", to: "/rh/honorarios?vista=honorarios", perm: "rh.processamento" },
      { label: "Prestações Sociais", to: "/rh/honorarios?vista=prestacoes", perm: "rh.funcionarios" },
      { label: "Configuração Salarial", to: "/rh/configuracao", perm: "rh.processamento" },
    ],
  },
  {
    id: "oficina",
    titulo: "Oficina",
    icon: Wrench,
    itens: [
      { label: "Ordens de Serviço", to: "/oficina/ordens", perm: "oficina" },
      { label: "Viaturas", to: "/oficina/viaturas", perm: "oficina" },
    ],
  },
  {
    id: "cadastros",
    titulo: "Cadastros",
    icon: Contact,
    itens: [
      { label: "Clientes", to: "/clientes", perm: "clientes" },
      { label: "Fornecedores", to: "/fornecedores", perm: "fornecedores" },
      { label: "Artigos", to: "/artigos" },
    ],
  },
  {
    id: "sistema",
    titulo: "Sistema",
    icon: Settings,
    itens: [
      { label: "Configurações", to: "/configuracoes" },
      { label: "Séries de Documentos", to: "/series", perm: "sistema.series" },
      { label: "Utilizadores e Permissões", to: "/utilizadores", perm: "sistema.utilizadores" },
      { label: "Registo de Acessos e Auditoria", to: "/utilizadores?vista=acessos", perm: "sistema.auditoria" },
      { label: "Facturação Electrónica (AGT)", to: "/facturacao-electronica", perm: "sistema.fe" },
      { label: "Documentos (Parametrização)", to: "/parametrizacao?vista=documentos", perm: "sistema.parametrizacao" },
      { label: "Opções de Venda", to: "/parametrizacao?vista=venda", perm: "sistema.parametrizacao" },
      { label: "Definição de Faturas e Recibos", to: "/parametrizacao?vista=impresso", perm: "sistema.parametrizacao" },
      { label: "Importar dados do KIGEST", to: "/importar", perm: "sistema.importar" },
      { label: "Manual e Atalhos", to: "/sobre?vista=manual" },
      { label: "Sobre o KanGest", to: "/sobre" },
    ],
  },
];

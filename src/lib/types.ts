export type Perfil = "ADMIN" | "GESTOR" | "CAIXA";

export interface Sessao {
  id: number;
  nome: string;
  login: string;
  /** código do perfil: ADMIN, GESTOR, CAIXA ou um perfil criado pela empresa */
  perfil: string;
  permissoes: string[];
  limite_desconto: number;
  /** nome do perfil para mostrar */
  perfil_descricao?: string | null;
  /** escalão do perfil: 1 caixa · 2 gestor · 3 administrador */
  nivel?: number;
  /** senha provisória: tem de a mudar antes de continuar */
  mudar_senha?: boolean;
  /** sessão bloqueada (inactividade ou pelo operador) */
  bloqueada?: boolean;
}

/** Impressão térmica directa: ESC/POS em vez do diálogo do sistema (corta o papel e abre a gaveta). */
export interface EscPos {
  ativo: boolean;
  /** fila de impressão local, impressora de rede (9100) ou porta/dispositivo */
  ligacao: "fila" | "rede" | "ficheiro";
  destino: string;
  colunas: number;
  cortar: boolean;
  acentos: boolean;
  gaveta: "nunca" | "numerario" | "sempre";
  /** pino da gaveta no conector RJ11 */
  pino: 2 | 5;
  /** imprimir o logótipo da empresa no topo do talão (convertido para 1 bit ao gravar o posto) */
  logo_ativo?: boolean;
  logo?: { bytes_linha: number; altura: number; dados: string } | null;
}

/** Visor de cliente: coluna VFD numa porta série ou segundo ecrã (terminais de ecrã duplo). */
export interface VisorOpcoes {
  modo: "nenhum" | "serie" | "ecra";
  porta: string;
  velocidade: number;
  colunas: number;
  protocolo: "cd5220" | "escpos";
  mensagem1: string;
  mensagem2: string;
}

export interface PostoOpcoes {
  formato: "ticket" | "a4";
  vias: number;
  imprimir_auto: boolean;
  mostrar_opcoes_impressao: boolean;
  valor_minimo_impressao: number;
  repetir_linhas: boolean;
  confirmar_cliente: boolean;
  escpos?: EscPos | null;
  /** Largura do papel do talão (PosConfig.ini «Tamanho» do KIGEST POS): 80 ou 58 mm. */
  talao_mm?: 80 | 58;
  visor?: VisorOpcoes;
  /** abrir o POS em ecrã inteiro (terminais dedicados) */
  pos_ecra_inteiro?: boolean;
}

export interface Posto {
  id: number;
  descricao: string;
  armazem_id: number;
  armazem?: string | null;
  serie_prefixo: string;
  opcoes: PostoOpcoes | null;
}

export interface Artigo {
  id: number;
  referencia: string;
  codigo_barras: string | null;
  descricao: string;
  descricao_curta: string | null;
  tipo: "P" | "S";
  familia_id: number | null;
  subfamilia_id: number | null;
  unidade_id: number | null;
  taxa_iva_id: number;
  motivo_isencao: string | null;
  custo: number;
  custo_medio: number;
  preco1: number;
  preco2: number;
  preco3: number;
  preco4: number;
  qtd_minima: number;
  qtd_maxima: number;
  controla_stock: number;
  permite_negativo: number;
  ativo: number;
  taxa_pct: number;
  unidade: string | null;
  familia: string | null;
  subfamilia: string | null;
  stock: number;
  preco1_iva: number;
  /** Controla lotes e datas de validade. */
  controla_lote: number;
  aviso_validade_dias: number;
  /** Artigo composto (kit): a venda baixa o stock dos componentes. */
  composto: number;
  /** Promoção: desconto % entre as datas (vazias = sem limite) e o desconto em vigor hoje (0 = sem promoção). */
  promo_desconto?: number;
  promo_inicio?: string | null;
  promo_fim?: string | null;
  promo_atual?: number;
}

export interface Componente {
  componente_id: number;
  quantidade: number;
  referencia?: string;
  descricao?: string;
  custo_medio?: number;
  unidade?: string | null;
}

export interface Lote {
  id: number;
  artigo_id: number;
  armazem_id: number;
  lote: string;
  validade: string | null;
  quantidade: number;
  custo: number;
  referencia: string;
  descricao: string;
  armazem: string;
  aviso_validade_dias: number;
  dias_validade: number | null;
  ultima_entrada: string | null;
  ultima_saida: string | null;
}

export interface Moeda {
  codigo: string;
  descricao: string;
  simbolo: string;
  cambio: number;
  ativo: number;
}

export interface Entidade {
  id: number;
  tipo: "C" | "F" | "A";
  nome: string;
  nif: string;
  endereco: string | null;
  localidade: string | null;
  provincia: string | null;
  telefone: string | null;
  email: string | null;
  pais?: string;
  limite_credito: number;
  desconto: number;
  nivel_preco: number;
  saldo?: number;
}

export interface Tabelas {
  familias: { id: number; descricao: string }[];
  subfamilias: { id: number; familia_id: number; descricao: string }[];
  unidades: { id: number; descricao: string; abreviatura: string }[];
  taxas: { id: number; codigo: string; descricao: string; percentagem: number }[];
  motivos: { codigo: string; descricao: string }[];
  armazens: { id: number; nome: string }[];
  modos_pagamento: { codigo: string; descricao: string }[];
  tipos_documento: TipoDocumento[];
  postos: { id: number; descricao: string; armazem_id: number; serie_prefixo: string }[];
  posto_id: number;
  armazem_id: number;
  posto: Posto;
}

export type Grupo = "VENDAS" | "COMPRAS" | "STOCK";

export interface TipoDocumento {
  codigo: string;
  descricao: string;
  classe: string;
  grupo: Grupo;
  entidade: "C" | "F" | null;
  stock: number;
  conta_corrente: number;
  pago_no_acto: number;
  assinado: number;
  credito: number;
  externo: number;
  ordem: number;
  vias: number;
  validade_dias: number;
  repetir_linhas: number;
  prefixo_extenso: string | null;
  texto_rodape: string | null;
  formato: "A4" | "A5";
}

/** Definição de Faturas e Recibos: o que aparece no impresso. */
export interface OpcoesImpresso {
  mostrar_bancos: boolean;
  valor_extenso: boolean;
  assinaturas: boolean;
  mostrar_operador: boolean;
}

export interface Transporte {
  carga_local?: string;
  carga_data?: string;
  descarga_local?: string;
  descarga_data?: string;
  matricula?: string;
  motorista?: string;
}

export interface DocResumo {
  id: number;
  numero_doc: string;
  tipo_codigo: string;
  tipo_desc: string;
  classe: string;
  grupo: Grupo;
  doc_externo: string | null;
  total_retencao: number;
  importado: number;
  data_emissao: string;
  data_hora: string;
  data_vencimento: string | null;
  entidade_id: number | null;
  entidade_nome: string;
  entidade_nif: string;
  total_liquido: number;
  total_iva: number;
  total: number;
  valor_pendente: number;
  estado: "N" | "A" | "F";
  hash_controlo: string;
  utilizador: string;
}

export interface DocLinha {
  id: number;
  linha: number;
  artigo_id: number;
  referencia: string;
  descricao: string;
  quantidade: number;
  unidade: string | null;
  preco_unitario: number;
  desconto_pct: number;
  valor_desconto: number;
  taxa_iva_pct: number;
  motivo_isencao: string | null;
  valor_liquido: number;
  valor_iva: number;
  total: number;
  /** Lote(s) consumidos ou recebidos, ex.: "L1 (2) · L2 (5)". */
  lote: string | null;
  validade: string | null;
}

export interface DocCompleto {
  documento: DocResumo & {
    entidade_endereco: string | null;
    total_bruto: number;
    total_desconto: number;
    valor_entregue: number;
    troco: number;
    hash: string;
    observacoes: string | null;
    motivo_anulacao: string | null;
    doc_origem: string | null;
    armazem: string | null;
    armazem_destino: string | null;
    vias: number;
    autorizado_por_nome: string | null;
    credito: number;
    externo: number;
    motivo: string | null;
    referencia: string | null;
    prazo: string | null;
    prazo_desc: string | null;
    transporte: Transporte | null;
    entidade_localidade: string | null;
    importado: number;
    moeda: string;
    cambio: number;
    total_moeda: number | null;
    doc_externo_hash: string | null;
    doc_externo_data: string | null;
    deducao_tipo: string | null;
    validade_dias?: number;
    prefixo_extenso?: string | null;
    texto_rodape?: string | null;
    formato?: "A4" | "A5";
  };
  opcoes?: OpcoesImpresso;
  /** Facturação electrónica: documento comunicado à AGT (impresso com o QR code da AGT). */
  fe?: { estado: "PENDENTE" | "ENVIADO" | "VALIDO" | "INVALIDO"; nif: string } | null;
  custos?: { id: number; tipo: string; descricao: string | null; valor: number }[];
  retencoes: { codigo: string; descricao: string; percentagem: number; base: number; valor: number }[];
  linhas: DocLinha[];
  pagamentos: { modo_codigo: string; descricao: string; valor: number }[];
  liquidacoes: { id: number; numero_doc: string; data_emissao: string; total: number; valor: number }[];
  resumo_iva: { taxa: number; motivo_isencao: string | null; base: number; iva: number }[];
  empresa: Empresa;
}

export interface Empresa {
  nome: string;
  designacao_comercial: string | null;
  nif: string;
  endereco: string | null;
  localidade: string | null;
  provincia: string | null;
  telefone: string | null;
  email: string | null;
  regime_iva: string;
  rodape: string | null;
  bancos: { banco: string; conta?: string; iban: string }[] | null;
  certificado: string;
  certificado_anterior: string | null;
  logo: string | null;
}

export interface CaixaResumo {
  sessao: {
    id: number;
    aberta_em: string;
    fechada_em: string | null;
    valor_inicial: number;
    valor_contado: number | null;
    estado: string;
    utilizador: string;
    posto: string | null;
  };
  por_modo: { codigo: string; descricao: string; total: number }[];
  documentos: number;
  total_vendido: number;
  entradas: number;
  saidas: number;
  movimentos: { id: number; data_hora: string; tipo: string; valor: number; descricao: string; rubrica: string | null; modo_codigo: string | null }[];
  artigos: { referencia: string; descricao: string; quantidade: number; total: number }[];
  numerario_esperado: number;
}

// ---------------- Oficina ----------------
export interface Viatura {
  id: number;
  entidade_id: number | null;
  matricula: string | null;
  marca: string;
  modelo: string | null;
  cor: string | null;
  ano: number | null;
  chassi: string | null;
  motor: string | null;
  combustivel: string | null;
  categoria: string | null;
  km: number;
  observacoes: string | null;
  ativo: number;
  cliente?: string | null;
  cliente_nif?: string | null;
  ordens?: number;
}

export type EstadoOS = "ABERTA" | "EM_CURSO" | "CONCLUIDA" | "FACTURADA" | "CANCELADA";

export interface OrdemLinha {
  id?: number;
  artigo_id: number | null;
  descricao: string;
  quantidade: number;
  mecanico: string | null;
  referencia?: string | null;
  artigo_tipo?: string | null;
}

export interface Ordem {
  id: number;
  numero: string;
  data_entrada: string;
  viatura_id: number;
  entidade_id: number | null;
  km_entrada: number | null;
  km_saida: number | null;
  referencia: string | null;
  avaria: string | null;
  diagnostico: string | null;
  tecnico: string | null;
  estado: EstadoOS;
  data_conclusao: string | null;
  documento_id: number | null;
  documento: string | null;
  observacoes: string | null;
  matricula: string | null;
  marca: string;
  modelo: string | null;
  cor?: string | null;
  chassi?: string | null;
  ano?: number | null;
  combustivel?: string | null;
  categoria?: string | null;
  cliente: string | null;
  cliente_nif?: string | null;
  cliente_morada?: string | null;
  cliente_localidade?: string | null;
  utilizador?: string;
  linhas?: OrdemLinha[];
  n_linhas?: number;
}

// ---------------- Recursos humanos ----------------
export interface Funcionario {
  id: number;
  numero: string | null;
  nome: string;
  sexo: string | null;
  data_nascimento: string | null;
  estado_civil: string | null;
  nacionalidade: string | null;
  naturalidade: string | null;
  bi_numero: string | null;
  bi_local_emissao: string | null;
  bi_validade: string | null;
  nif: string | null;
  inss_numero: string | null;
  morada: string | null;
  localidade: string | null;
  municipio: string | null;
  provincia: string | null;
  telefone: string | null;
  email: string | null;
  departamento_id: number | null;
  funcao_id: number | null;
  categoria_id: number | null;
  tipo_contrato: string | null;
  data_admissao: string | null;
  data_saida: string | null;
  motivo_saida: string | null;
  salario_base: number;
  sub_alimentacao: number;
  sub_transporte: number;
  sub_chefia: number;
  sub_turno: number;
  abono_falhas: number;
  sub_arrendamento: number;
  sub_atavio: number;
  sub_isencao_horario: number;
  sub_deslocacao: number;
  desconta_inss: number;
  desconta_irt: number;
  desconto_sindicato: number;
  dependentes: number;
  banco: string | null;
  iban: string | null;
  conta_empresa_id: number | null;
  ativo: number;
  independente: number;
  honorario: number;
  tipo_honorario: "MENSAL" | "DIA" | "HORA";
  local_trabalho: string | null;
  departamento?: string | null;
  funcao?: string | null;
  categoria?: string | null;
  agregado?: { nome: string; parentesco: string | null; data_nascimento: string | null }[];
}

export interface Rubrica {
  codigo: string;
  descricao: string;
  tipo: "REMUNERACAO" | "DESCONTO";
  incide_inss: number;
  incide_irt: number;
  isento_irt_ate: number;
  mapa: string | null;
  ordem: number;
}

export interface Escalao {
  id?: number;
  de: number;
  ate: number | null;
  parcela_fixa: number;
  taxa: number;
  excesso: number;
}

export interface TabelasRH {
  departamentos: { id: number; descricao: string }[];
  funcoes: { id: number; descricao: string }[];
  categorias: { id: number; descricao: string }[];
  rubricas: Rubrica[];
  escaloes: Escalao[];
  parametros: {
    inss_trabalhador: string | null;
    inss_empregador: string | null;
    dias_uteis: string | null;
    horas_dia: string | null;
    acrescimo_horas_extra: string | null;
    irt_independentes: string | null;
    tabela_confirmada: boolean;
  };
}

export interface LinhaRecibo {
  rubrica: string;
  descricao: string;
  tipo: string;
  quantidade: number | null;
  valor: number;
}

export interface ResumoSalario {
  funcionario_id: number;
  nome: string;
  departamento: string | null;
  funcao: string | null;
  salario_base: number;
  total_remuneracoes: number;
  base_inss: number;
  inss_trabalhador: number;
  inss_empregador: number;
  base_irt: number;
  irt: number;
  total_descontos: number;
  liquido: number;
  linhas: LinhaRecibo[];
}

export interface Processamento {
  id: number;
  ano: number;
  mes: number;
  data: string;
  estado: "PROCESSADO" | "ANULADO";
  utilizador: string;
  total_liquido: number;
  pago: number;
  motivo_anulacao: string | null;
  recibos: number;
  total_irt: number;
  total_inss: number;
}

export interface ReciboSalario {
  id: number;
  funcionario_id: number;
  numero: string | null;
  nome: string;
  nif: string | null;
  inss_numero: string | null;
  departamento: string | null;
  funcao: string | null;
  categoria: string | null;
  banco: string | null;
  iban: string | null;
  salario_base: number;
  total_remuneracoes: number;
  base_inss: number;
  inss_trabalhador: number;
  inss_empregador: number;
  base_irt: number;
  irt: number;
  total_descontos: number;
  liquido: number;
  linhas: LinhaRecibo[];
}

export interface Alteracao {
  id: number;
  funcionario_id: number;
  nome: string;
  ano: number;
  mes: number;
  rubrica: string;
  rubrica_desc: string;
  tipo: string;
  quantidade: number;
  valor: number;
  descricao: string | null;
  processado: number;
  adiantamento_id: number | null;
}

export const MESES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];

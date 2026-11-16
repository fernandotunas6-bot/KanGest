import {
  BookOpen,
  Car,
  Cpu,
  Hammer,
  HandPlatter,
  Pill,
  Scissors,
  Shirt,
  ShoppingBasket,
  Store,
  Truck,
  Briefcase,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import type { Artigo } from "./types";

/**
 * Ramos de actividade: o KanGest adapta-se ao negócio — que dados se pedem em cada artigo (ISBN e autor
 * numa livraria, princípio activo numa farmácia, tamanho e cor numa loja de roupa, n.º de série e
 * garantia em electrónica…), o que aparece no POS e o que se sugere ao criar artigos. A escolha fica em
 * Configurações → Empresa (ou no primeiro arranque) e vale para todos os postos.
 */
export type RamoId =
  | "comercio"
  | "supermercado"
  | "livraria"
  | "farmacia"
  | "moda"
  | "electronica"
  | "ferragens"
  | "autopecas"
  | "restauracao"
  | "servicos"
  | "beleza"
  | "oficina"
  | "grossista";

export interface Campo {
  /** nome do dado (minúsculas e _), igual em todos os artigos do ramo */
  id: string;
  label: string;
  tipo?: "texto" | "numero" | "sim";
  dica?: string;
  /** ocupa a linha inteira no formulário */
  largo?: boolean;
}

export interface Ramo {
  id: RamoId;
  nome: string;
  descricao: string;
  icone: LucideIcon;
  campos: Campo[];
  /** dados mostrados por baixo do nome do artigo (POS e lista) */
  resumo: string[];
  /** valores por omissão de um artigo novo */
  novoArtigo?: { tipo?: "P" | "S"; controla_stock?: boolean; controla_lote?: boolean; unidade?: string };
  /** nome do campo do código de barras (ex.: ISBN) */
  rotuloCodigo?: string;
  /** gerar variantes (um artigo por combinação) */
  variantes?: { nome: string; label: string; sugestoes: string[] }[];
  /** as vendas em paralelo do POS têm nome (ex.: «Mesa 4») */
  vendasComNome?: string;
  /** famílias sugeridas para começar */
  familias: string[];
  /** o que o KanGest já faz por este ramo (mostrado ao escolher) */
  dicas: string[];
}

const marca: Campo = { id: "marca", label: "Marca" };
const modelo: Campo = { id: "modelo", label: "Modelo" };

export const RAMOS: Ramo[] = [
  {
    id: "comercio",
    nome: "Comércio geral / loja",
    descricao: "Loja de bairro, cantina, bazar, comércio misto.",
    icone: Store,
    campos: [marca],
    resumo: ["marca"],
    familias: ["Geral"],
    dicas: ["POS com leitor de códigos de barras", "Stock, compras e contas correntes", "Promoções e fidelização"],
  },
  {
    id: "supermercado",
    nome: "Supermercado, mercearia, minimercado",
    descricao: "Muitos artigos, leitor de códigos, balança e validades.",
    icone: ShoppingBasket,
    campos: [marca, { id: "origem", label: "Origem / país" }],
    resumo: ["marca"],
    familias: ["Mercearia", "Bebidas", "Laticínios", "Talho", "Peixaria", "Padaria", "Frutas e legumes", "Congelados", "Higiene", "Limpeza"],
    dicas: [
      "Etiquetas da balança (peso ou preço no código EAN-13)",
      "Vários códigos de barras por artigo (unidade, pack, caixa)",
      "Lotes e validades com aviso",
      "Vendas em paralelo e várias caixas em rede",
    ],
  },
  {
    id: "livraria",
    nome: "Livraria e papelaria",
    descricao: "Livros com ISBN, autor e editora; material escolar e de escritório.",
    icone: BookOpen,
    campos: [
      { id: "autor", label: "Autor(es)", largo: true },
      { id: "editora", label: "Editora" },
      { id: "ano", label: "Ano de edição", tipo: "numero" },
      { id: "edicao", label: "Edição", dica: "Ex.: 3.ª edição" },
      { id: "coleccao", label: "Colecção" },
      { id: "idioma", label: "Idioma" },
      { id: "paginas", label: "Páginas", tipo: "numero" },
    ],
    resumo: ["autor", "editora"],
    rotuloCodigo: "ISBN / código de barras",
    familias: ["Literatura", "Escolar", "Infantil", "Técnico e universitário", "Religião", "Jornais e revistas", "Papelaria", "Material escolar"],
    dicas: [
      "O ISBN é o código de barras do livro: é conferido e o ISBN-10 passa a ISBN-13",
      "Pesquisa no POS por título, autor, editora ou ISBN",
      "Encomendas a fornecedores e devoluções (notas de crédito)",
    ],
  },
  {
    id: "farmacia",
    nome: "Farmácia e parafarmácia",
    descricao: "Medicamentos com lote e validade, princípio activo e receita.",
    icone: Pill,
    campos: [
      { id: "dci", label: "Princípio activo (DCI)", largo: true },
      { id: "dosagem", label: "Dosagem", dica: "Ex.: 500 mg" },
      { id: "forma", label: "Forma farmacêutica", dica: "Comprimidos, xarope…" },
      { id: "laboratorio", label: "Laboratório" },
      { id: "receita", label: "Sujeito a receita médica", tipo: "sim", dica: "O POS avisa para confirmar a receita" },
    ],
    resumo: ["dci", "dosagem"],
    novoArtigo: { controla_lote: true },
    familias: ["Medicamentos", "Genéricos", "Parafarmácia", "Higiene e cosmética", "Bebé e mamã", "Ortopedia", "Dispositivos médicos"],
    dicas: ["Lotes e validades (sai primeiro o que expira primeiro)", "Pesquisa pelo princípio activo", "Aviso de receita médica no POS"],
  },
  {
    id: "moda",
    nome: "Moda, vestuário e calçado",
    descricao: "Tamanhos e cores, marcas e colecções.",
    icone: Shirt,
    campos: [marca, modelo, { id: "tamanho", label: "Tamanho" }, { id: "cor", label: "Cor" }, { id: "coleccao", label: "Colecção / estação" }, { id: "material", label: "Material" }],
    resumo: ["tamanho", "cor"],
    variantes: [
      { nome: "tamanho", label: "Tamanhos", sugestoes: ["XS", "S", "M", "L", "XL", "XXL", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45"] },
      { nome: "cor", label: "Cores", sugestoes: ["Preto", "Branco", "Azul", "Vermelho", "Verde", "Cinzento", "Bege", "Castanho", "Rosa", "Amarelo"] },
    ],
    familias: ["Homem", "Senhora", "Criança", "Calçado", "Acessórios", "Desporto"],
    dicas: ["Gerar variantes: um artigo por tamanho e cor, cada um com o seu stock", "Pesquisa no POS pelo modelo, tamanho ou cor", "Trocas com nota de crédito"],
  },
  {
    id: "electronica",
    nome: "Electrónica, informática e telemóveis",
    descricao: "Marca e modelo, n.º de série ou IMEI e garantia na factura.",
    icone: Cpu,
    campos: [
      marca,
      modelo,
      { id: "garantia_meses", label: "Garantia (meses)", tipo: "numero", dica: "Fica escrita na linha da factura" },
      { id: "pede_serie", label: "Pedir o n.º de série / IMEI na venda", tipo: "sim" },
    ],
    resumo: ["marca", "modelo"],
    familias: ["Telemóveis", "Informática", "Acessórios", "Electrodomésticos", "Áudio e vídeo", "Reparações"],
    dicas: ["N.º de série / IMEI pedido no POS e escrito na factura", "Garantia impressa na linha", "Reparações como serviços (ordens de trabalho)"],
  },
  {
    id: "ferragens",
    nome: "Ferragens e materiais de construção",
    descricao: "Medidas, marcas, venda ao metro, ao kg e à unidade.",
    icone: Hammer,
    campos: [marca, { id: "medida", label: "Medida / dimensões" }, { id: "material", label: "Material" }],
    resumo: ["medida", "marca"],
    familias: ["Cimento e inertes", "Ferro e aço", "Canalização", "Electricidade", "Tintas", "Ferramentas", "Fixação", "Madeiras"],
    dicas: ["Quantidades com casas decimais (metro, kg, m²)", "Preços por nível (balcão, revendedor, obra)", "Guias de transporte para entregas"],
  },
  {
    id: "autopecas",
    nome: "Peças auto e acessórios",
    descricao: "Referência do fabricante e viaturas a que serve.",
    icone: Car,
    campos: [marca, { id: "ref_fabricante", label: "Ref. do fabricante (OEM)" }, { id: "aplicacao", label: "Aplicação (viaturas)", largo: true, dica: "Ex.: Toyota Hilux 2015-2022" }],
    resumo: ["ref_fabricante", "aplicacao"],
    familias: ["Motor", "Travões", "Suspensão", "Filtros", "Óleos e lubrificantes", "Eléctrica", "Pneus", "Carroçaria"],
    dicas: ["Pesquisa pela ref. do fabricante ou pela viatura", "Vários códigos por artigo (equivalências)", "Oficina: viaturas e ordens de reparação"],
  },
  {
    id: "restauracao",
    nome: "Restauração, café, bar e pastelaria",
    descricao: "Mesas e pedidos em paralelo, pratos sem stock próprio.",
    icone: HandPlatter,
    campos: [{ id: "alergenos", label: "Alergénios", largo: true, dica: "Ex.: glúten, leite, frutos secos" }],
    resumo: [],
    novoArtigo: { controla_stock: false },
    vendasComNome: "Mesa",
    familias: ["Pratos", "Entradas", "Sobremesas", "Bebidas", "Cafetaria", "Pastelaria", "Vinhos"],
    dicas: ["Cada mesa ou pedido é uma venda em paralelo, com nome («Mesa 4»)", "Botões grandes por família no POS táctil", "Telemóveis e tablets dos empregados pelo browser"],
  },
  {
    id: "servicos",
    nome: "Prestação de serviços",
    descricao: "Consultoria, contabilidade, advocacia, limpeza, segurança, manutenção, formação…",
    icone: Briefcase,
    campos: [{ id: "duracao_min", label: "Duração habitual (min)", tipo: "numero" }],
    resumo: [],
    novoArtigo: { tipo: "S", unidade: "H" },
    familias: ["Consultoria", "Avenças", "Manutenção", "Formação", "Deslocações", "Projectos"],
    dicas: [
      "Avenças: contratos que se facturam todos os meses (ou trimestre, ou ano) de uma vez",
      "Retenção na fonte nas facturas",
      "Contas correntes e avisos de vencimento",
    ],
  },
  {
    id: "beleza",
    nome: "Salão de beleza, barbearia, estética e clínica",
    descricao: "Serviços com duração, profissional e produtos.",
    icone: Scissors,
    campos: [{ id: "duracao_min", label: "Duração (min)", tipo: "numero" }],
    resumo: [],
    novoArtigo: { tipo: "S" },
    vendasComNome: "Cliente",
    familias: ["Cabelo", "Barbearia", "Unhas", "Estética", "Massagens", "Produtos"],
    dicas: ["Cada cliente em atendimento é uma venda em paralelo, com nome", "Vendedor por documento e comissões de cada profissional", "Fidelização com pontos"],
  },
  {
    id: "oficina",
    nome: "Oficina auto",
    descricao: "Viaturas, ordens de reparação, peças e mão de obra.",
    icone: Wrench,
    campos: [marca, { id: "ref_fabricante", label: "Ref. do fabricante (OEM)" }, { id: "aplicacao", label: "Aplicação (viaturas)", largo: true }],
    resumo: ["ref_fabricante"],
    familias: ["Mão de obra", "Peças", "Óleos e lubrificantes", "Pneus", "Diagnóstico"],
    dicas: ["Módulo Oficina: viaturas e ordens de reparação", "Mecânico por linha", "Facturar a ordem de reparação"],
  },
  {
    id: "grossista",
    nome: "Distribuição e venda por grosso",
    descricao: "Embalagens, níveis de preço por cliente e encomendas.",
    icone: Truck,
    campos: [marca, { id: "embalagem", label: "Embalagem", dica: "Ex.: cx 12 × 1 L" }],
    resumo: ["embalagem", "marca"],
    familias: ["Bebidas", "Mercearia", "Higiene", "Limpeza"],
    dicas: ["4 níveis de preço por cliente", "Código de barras por embalagem (vende a caixa)", "Encomendas, guias de transporte e sugestão de compras"],
  },
];

export const ramoPorId = (id: string | null | undefined): Ramo => RAMOS.find((r) => r.id === id) ?? RAMOS[0];

/** Atributos guardados no artigo (texto JSON) → objecto. */
export function atributosDe(a: Pick<Artigo, "atributos"> | null | undefined): Record<string, string | number | boolean> {
  const v = a?.atributos;
  if (!v) return {};
  if (typeof v === "object") return v as Record<string, string | number | boolean>;
  try {
    const o = JSON.parse(v);
    return o && typeof o === "object" && !Array.isArray(o) ? o : {};
  } catch {
    return {};
  }
}

/** Linha curta com os dados principais do ramo (ex.: «Pepetela · Dom Quixote», «M · Azul»). */
export function resumoAtributos(a: Pick<Artigo, "atributos">, ramo: Ramo): string {
  const at = atributosDe(a);
  return ramo.resumo
    .map((k) => at[k])
    .filter((x) => x !== undefined && x !== null && x !== "" && typeof x !== "boolean")
    .join(" · ");
}

/** Texto acrescentado à descrição da linha na venda (n.º de série, garantia). */
export function notaDaLinha(a: Pick<Artigo, "atributos">, serie?: string): string {
  const at = atributosDe(a);
  const partes: string[] = [];
  if (serie?.trim()) partes.push(`S/N ${serie.trim()}`);
  const g = Number(at.garantia_meses);
  if (g > 0) partes.push(`Garantia ${g} ${g === 1 ? "mês" : "meses"}`);
  return partes.join(" · ");
}

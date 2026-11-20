// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { GraduationCap, HeartPulse, Store, type LucideIcon } from "lucide-react";

/**
 * Tipo de gestão, escolhido na instalação (um nível acima do ramo de actividade): decide os menus e o que
 * se prepara para começar. A escola usa o mesmo motor de facturação, recibos e contas correntes — as
 * propinas são avenças mensais e os artigos do ensino são isentos de IVA (art. 12.º, n.º 1, al. l) do CIVA,
 * código M21 da AGT). Fica em config('empresa.modo') — ver src-tauri/src/services/modos.rs.
 */
export type ModoId = "loja" | "escola" | "hospital";

export interface Modo {
  id: ModoId;
  nome: string;
  /** subtítulo no menu lateral */
  subtitulo: string;
  descricao: string;
  icone: LucideIcon;
  /** ainda não se pode escolher */
  emBreve?: boolean;
  /** o que o KanGest faz neste modo (mostrado ao escolher) */
  dicas: string[];
}

export const MODOS: Modo[] = [
  {
    id: "loja",
    nome: "Lojas e comércio",
    subtitulo: "Gestão Comercial",
    descricao: "Lojas, supermercados, farmácias, livrarias, restauração, serviços e oficinas.",
    icone: Store,
    dicas: ["POS com leitor de códigos de barras", "Stock, compras e inventário", "Facturação certificada e SAF-T", "Contas correntes e tesouraria"],
  },
  {
    id: "escola",
    nome: "Escola e colégio",
    subtitulo: "Gestão Escolar",
    descricao: "Propinas mensais, matrículas, pagamentos e devedores.",
    icone: GraduationCap,
    dicas: [
      "Propinas facturadas todos os meses, sem repetir",
      "Pagamentos com recibo e conta corrente de cada aluno",
      "Devedores por turma",
      "Ensino isento de IVA (art. 12.º, n.º 1, al. l) do CIVA — M21)",
    ],
  },
  {
    id: "hospital",
    nome: "Hospital e clínica",
    subtitulo: "Gestão Hospitalar",
    descricao: "Consultas, exames, internamentos e farmácia hospitalar.",
    icone: HeartPulse,
    emBreve: true,
    dicas: [],
  },
];

export function modoPorId(id?: string | null): Modo {
  return MODOS.find((m) => m.id === id && !m.emBreve) ?? MODOS[0];
}

/** Motivo de isenção do ensino (art. 12.º, n.º 1, al. l) do CIVA). */
export const MOTIVO_ENSINO = "M21";
export const FAMILIA_ENSINO = "Propinas e emolumentos";

/** Artigo para começar, revisto (e com o preço posto) antes de o criar. */
export interface ArtigoBase {
  referencia: string;
  descricao: string;
  familia?: string;
  /** cêntimos (nos isentos, o preço é o PVP) */
  preco: number;
  tipo?: "P" | "S";
  unidade?: string;
  motivo_isencao?: string;
}

/** O essencial para cobrar: a propina mensal e a matrícula (outros emolumentos criam-se em Artigos). */
export function artigosEscola(propina = 0, matricula = 0): ArtigoBase[] {
  return [
    { referencia: "PROP", descricao: "Propina mensal", unidade: "MES", preco: propina },
    { referencia: "MATR", descricao: "Matrícula", preco: matricula },
  ].map((a) => ({ ...a, familia: FAMILIA_ENSINO, tipo: "S" as const, motivo_isencao: MOTIVO_ENSINO }));
}

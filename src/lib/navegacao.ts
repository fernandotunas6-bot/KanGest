// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { create } from "zustand";
import { MENU, type GrupoMenu, type ItemMenu } from "./menu";

/** Um ecrã do menu (ou o Painel), com o grupo a que pertence. */
export interface Destino {
  to: string;
  label: string;
  grupo?: GrupoMenu;
  item?: ItemMenu;
}

export const PAINEL: Destino = { to: "/", label: "Painel" };

/** Ecrãs abertos por endereços que não estão no menu tal e qual. */
const FORA_DO_MENU: Record<string, string> = {
  "/documentos": "Documentos Emitidos",
};

/** Todos os destinos do menu, pela ordem do menu (o primeiro com cada endereço). */
export const DESTINOS: Destino[] = (() => {
  const vistos = new Set<string>();
  const out: Destino[] = [];
  for (const g of MENU)
    for (const i of g.itens)
      if (i.to && !i.emBreve && !vistos.has(i.to)) {
        vistos.add(i.to);
        out.push({ to: i.to, label: i.label, grupo: g, item: i });
      }
  return out;
})();

export function destinoPorEndereco(to: string): Destino | undefined {
  return to === "/" ? PAINEL : DESTINOS.find((d) => d.to === to);
}

/**
 * O destino do menu que corresponde ao endereço actual. Um item com parâmetros (ex.: Lista de Preços =
 * /relatorios/stock?situacao=precos) que coincide tem prioridade sobre o item simples do mesmo ecrã;
 * um endereço com parâmetros a mais (ex.: ?grupo=VENDAS&tipo=RC) fica no item cujos parâmetros contém.
 */
export function destinoActual(pathname: string, search: string): Destino | null {
  if (pathname === "/") return PAINEL;
  const atuais = new URLSearchParams(search);
  let simples: Destino | null = null;
  let parcial: Destino | null = null;
  // o primeiro item deste ecrã: quando todos levam parâmetros e o endereço não traz nenhum (ex.: /rh/salarios abre no
  // separador do primeiro item), o menu e o caminho no topo ficam nesse item em vez de mostrar só «KanGest»
  let primeiro: Destino | null = null;
  for (const d of DESTINOS) {
    const [caminho, query] = d.to.split("?");
    if (caminho !== pathname) continue;
    primeiro ??= d;
    if (!query) {
      simples ??= d;
      continue;
    }
    const pedidos = [...new URLSearchParams(query)];
    if (pedidos.length === [...atuais].length && pedidos.every(([k, v]) => atuais.get(k) === v)) return d;
    if (!parcial && pedidos.every(([k, v]) => atuais.get(k) === v)) parcial = d;
  }
  const r = parcial ?? simples ?? primeiro;
  if (r) return r;
  return FORA_DO_MENU[pathname] ? { to: pathname, label: FORA_DO_MENU[pathname] } : null;
}

// ------------------------------------------------------------------ favoritos e recentes (por utilizador)

function lerLista(chave: string): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(chave) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function gravarLista(chave: string, lista: string[]) {
  try {
    localStorage.setItem(chave, JSON.stringify(lista));
  } catch {
    /* armazenamento indisponível */
  }
}

interface Navegacao {
  login: string | null;
  favoritos: string[];
  recentes: string[];
  /** Carrega os favoritos e recentes do utilizador que entrou. */
  iniciar: (login: string | null) => void;
  alternarFavorito: (to: string) => void;
  registarVisita: (to: string) => void;
}

const MAX_RECENTES = 8;

export const useNavegacao = create<Navegacao>((set, get) => ({
  login: null,
  favoritos: [],
  recentes: [],
  iniciar: (login) => {
    if (login === get().login) return;
    set({
      login,
      favoritos: login ? lerLista(`kangest.favoritos.${login}`) : [],
      recentes: login ? lerLista(`kangest.recentes.${login}`) : [],
    });
  },
  alternarFavorito: (to) => {
    const { login, favoritos } = get();
    const nova = favoritos.includes(to) ? favoritos.filter((f) => f !== to) : [...favoritos, to];
    if (login) gravarLista(`kangest.favoritos.${login}`, nova);
    set({ favoritos: nova });
  },
  registarVisita: (to) => {
    const { login, recentes } = get();
    if (recentes[0] === to) return;
    const nova = [to, ...recentes.filter((r) => r !== to)].slice(0, MAX_RECENTES);
    if (login) gravarLista(`kangest.recentes.${login}`, nova);
    set({ recentes: nova });
  },
}));

// ------------------------------------------------------------------ pesquisa sem acentos

/**
 * Texto para comparar: sem acentos, minúsculas e com a grafia antiga e a nova iguais
 * («factura» = «fatura», «electrónica» = «eletrónica», «acção» = «ação»).
 */
export function normalizar(s: string): string {
  return s
    .toLowerCase()
    .replace(/ct/g, "t")
    .replace(/[cp]ç/g, "ç")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "");
}

/** Pontuação de um texto para as palavras pesquisadas (0 = não corresponde a todas). */
export function pontuar(palavras: string[], texto: string): number {
  const t = normalizar(texto);
  let total = 0;
  for (const w of palavras) {
    const i = t.indexOf(w);
    if (i < 0) return 0;
    total += i === 0 ? 4 : /[\s/(-]/.test(t[i - 1]) ? 2 : 1;
  }
  return total;
}

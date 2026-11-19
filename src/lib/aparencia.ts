import { create } from "zustand";
import { aplicarCores, coresGuardadas, guardarCores, type Cores } from "@/lib/cores";

/** Aparência escolhida por cada utilizador neste computador (tema e densidade das tabelas). */
export type Tema = "claro" | "escuro" | "sistema";
export type Densidade = "confortavel" | "compacta";
/** Ecrã táctil: alvos maiores para os dedos. "auto" liga-se em telemóveis, tablets e terminais POS. */
export type Toque = "auto" | "sim" | "nao";

const CHAVE_TEMA = "kangest.tema";
const CHAVE_DENSIDADE = "kangest.densidade";
const CHAVE_TOQUE = "kangest.toque";

const ecraTactil = () => !!window.matchMedia?.("(pointer: coarse)").matches;
export const toqueEfectivo = (t: Toque) => t === "sim" || (t === "auto" && ecraTactil());

function ler<T extends string>(chave: string, validos: readonly T[], omissao: T): T {
  try {
    const v = localStorage.getItem(chave) as T | null;
    return v && validos.includes(v) ? v : omissao;
  } catch {
    return omissao;
  }
}

function gravar(chave: string, valor: string) {
  try {
    localStorage.setItem(chave, valor);
  } catch {
    /* armazenamento indisponível: fica só nesta sessão */
  }
}

const escuroNoSistema = () => !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
const efectivo = (t: Tema): "claro" | "escuro" => (t === "sistema" ? (escuroNoSistema() ? "escuro" : "claro") : t);

/** O jogo de cores da empresa (Configurações → Aparência), igual para todos; o tema claro ou escuro é de cada um. */
let coresActuais: Cores | null = coresGuardadas();

function aplicar(tema: Tema, densidade: Densidade, toque: Toque) {
  const r = document.documentElement;
  r.dataset.tema = efectivo(tema);
  r.dataset.densidade = densidade;
  r.dataset.toque = toqueEfectivo(toque) ? "1" : "0";
  aplicarCores(coresActuais, efectivo(tema) === "escuro");
}

interface Aparencia {
  tema: Tema;
  densidade: Densidade;
  /** O tema que está à vista ("sistema" resolvido). */
  temaEfectivo: "claro" | "escuro";
  toque: Toque;
  /** Modo táctil ligado agora ("auto" resolvido). */
  tactil: boolean;
  /** Jogo de cores da empresa (vem do servidor; `null` = o do KanGest). */
  cores: Cores | null;
  setTema: (t: Tema) => void;
  setDensidade: (d: Densidade) => void;
  setToque: (t: Toque) => void;
  setCores: (c: Cores | null) => void;
}

const temaInicial = ler<Tema>(CHAVE_TEMA, ["claro", "escuro", "sistema"], "claro");
const densidadeInicial = ler<Densidade>(CHAVE_DENSIDADE, ["confortavel", "compacta"], "confortavel");
const toqueInicial = ler<Toque>(CHAVE_TOQUE, ["auto", "sim", "nao"], "auto");
// aplica-se antes do primeiro desenho (sem "piscar" de claro para escuro)
aplicar(temaInicial, densidadeInicial, toqueInicial);

export const useAparencia = create<Aparencia>((set, get) => ({
  tema: temaInicial,
  densidade: densidadeInicial,
  temaEfectivo: efectivo(temaInicial),
  toque: toqueInicial,
  tactil: toqueEfectivo(toqueInicial),
  cores: coresActuais,
  setCores: (cores) => {
    coresActuais = cores;
    guardarCores(cores);
    aplicar(get().tema, get().densidade, get().toque);
    set({ cores });
  },
  setTema: (tema) => {
    gravar(CHAVE_TEMA, tema);
    aplicar(tema, get().densidade, get().toque);
    set({ tema, temaEfectivo: efectivo(tema) });
  },
  setDensidade: (densidade) => {
    gravar(CHAVE_DENSIDADE, densidade);
    aplicar(get().tema, densidade, get().toque);
    set({ densidade });
  },
  setToque: (toque) => {
    gravar(CHAVE_TOQUE, toque);
    aplicar(get().tema, get().densidade, toque);
    set({ toque, tactil: toqueEfectivo(toque) });
  },
}));

// "Sistema": acompanha a mudança de tema do Windows / macOS sem reiniciar
window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
  const { tema, densidade, toque } = useAparencia.getState();
  if (tema !== "sistema") return;
  aplicar(tema, densidade, toque);
  useAparencia.setState({ temaEfectivo: efectivo(tema) });
});
// "Automático": um portátil com ecrã táctil que se liga ou desliga de um rato muda de modo sozinho
window.matchMedia?.("(pointer: coarse)").addEventListener?.("change", () => {
  const { tema, densidade, toque } = useAparencia.getState();
  aplicar(tema, densidade, toque);
  useAparencia.setState({ tactil: toqueEfectivo(toque) });
});

import { create } from "zustand";

/** Aparência escolhida por cada utilizador neste computador (tema e densidade das tabelas). */
export type Tema = "claro" | "escuro" | "sistema";
export type Densidade = "confortavel" | "compacta";

const CHAVE_TEMA = "kangest.tema";
const CHAVE_DENSIDADE = "kangest.densidade";

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

function aplicar(tema: Tema, densidade: Densidade) {
  const r = document.documentElement;
  r.dataset.tema = efectivo(tema);
  r.dataset.densidade = densidade;
}

interface Aparencia {
  tema: Tema;
  densidade: Densidade;
  /** O tema que está à vista ("sistema" resolvido). */
  temaEfectivo: "claro" | "escuro";
  setTema: (t: Tema) => void;
  setDensidade: (d: Densidade) => void;
}

const temaInicial = ler<Tema>(CHAVE_TEMA, ["claro", "escuro", "sistema"], "claro");
const densidadeInicial = ler<Densidade>(CHAVE_DENSIDADE, ["confortavel", "compacta"], "confortavel");
// aplica-se antes do primeiro desenho (sem "piscar" de claro para escuro)
aplicar(temaInicial, densidadeInicial);

export const useAparencia = create<Aparencia>((set, get) => ({
  tema: temaInicial,
  densidade: densidadeInicial,
  temaEfectivo: efectivo(temaInicial),
  setTema: (tema) => {
    gravar(CHAVE_TEMA, tema);
    aplicar(tema, get().densidade);
    set({ tema, temaEfectivo: efectivo(tema) });
  },
  setDensidade: (densidade) => {
    gravar(CHAVE_DENSIDADE, densidade);
    aplicar(get().tema, densidade);
    set({ densidade });
  },
}));

// "Sistema": acompanha a mudança de tema do Windows / macOS sem reiniciar
window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
  const { tema, densidade } = useAparencia.getState();
  if (tema !== "sistema") return;
  aplicar(tema, densidade);
  useAparencia.setState({ temaEfectivo: efectivo(tema) });
});

/**
 * Jogos de cores do sistema (Configurações → Aparência e a instalação): a cor principal (menu, botões, ligações) e a
 * de destaque (Abrir POS, Pagar, avisos). Os tons de cada uma — claros, escuros e os do tema escuro — calculam-se
 * aqui e entram como variáveis CSS (as mesmas do index.css). O «petroleo» é o do KanGest e usa o index.css tal como
 * está. A lista de ids tem de ser igual à de src-tauri/src/services/aparencia.rs (PALETAS).
 */
export interface Cores {
  paleta: string;
  marca?: string;
  destaque?: string;
}

export interface Paleta {
  id: string;
  nome: string;
  marca: string;
  destaque: string;
}

export const PALETAS: Paleta[] = [
  { id: "petroleo", nome: "Verde-petróleo (KanGest)", marca: "#146b63", destaque: "#e09a2b" },
  { id: "azul", nome: "Azul-marinho", marca: "#1f4e8c", destaque: "#e8a33d" },
  { id: "oceano", nome: "Azul petróleo e ouro", marca: "#0a748f", destaque: "#c9963a" },
  { id: "vinho", nome: "Vinho e dourado", marca: "#7d2541", destaque: "#d8a24a" },
  { id: "floresta", nome: "Verde-floresta", marca: "#2f6b3a", destaque: "#e3a23a" },
  { id: "grafite", nome: "Grafite e laranja", marca: "#3a4652", destaque: "#ea7a2a" },
  { id: "violeta", nome: "Violeta", marca: "#5a3d99", destaque: "#f0b43e" },
];

type RGB = [number, number, number];
const deHex = (h: string): RGB => {
  const n = parseInt(h.replace("#", ""), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};
const paraHex = (c: RGB) => "#" + c.map((x) => Math.round(Math.min(255, Math.max(0, x))).toString(16).padStart(2, "0")).join("");
/** `a` misturado com `b` na proporção `t` (0 = só `a`). */
const mistura = (a: string, b: string, t: number) => {
  const x = deHex(a);
  const y = deHex(b);
  return paraHex([0, 1, 2].map((i) => x[i] * (1 - t) + y[i] * t) as RGB);
};
const luminancia = (h: string) => {
  const [r, g, b] = deHex(h).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
export const contraste = (a: string, b: string) => {
  const [x, y] = [luminancia(a), luminancia(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
};
/** Escurece (ou clareia) `cor` até ter o contraste pedido com `contra`. */
function ate(cor: string, contra: string, minimo: number, direccao: "#000000" | "#ffffff") {
  let c = cor;
  for (let i = 0; i < 40 && contraste(c, contra) < minimo; i++) c = mistura(c, direccao, 0.06);
  return c;
}

const ESCURO_FUNDO = "#101513";
const TINTA = "#1d2422"; // texto sobre os botões de destaque (--color-on-accent)

/** As duas cores de um jogo, prontas a usar (texto branco legível na principal, texto escuro no destaque). */
export function coresDe(c: Cores | null | undefined): { marca: string; destaque: string } {
  const p = PALETAS.find((x) => x.id === c?.paleta);
  const base = p ?? { marca: c?.marca || PALETAS[0].marca, destaque: c?.destaque || PALETAS[0].destaque };
  return { marca: ate(base.marca, "#ffffff", 4.5, "#000000"), destaque: ate(base.destaque, TINTA, 4.5, "#ffffff") };
}

/** As variáveis CSS de um jogo, no tema claro ou no escuro. */
export function variaveis(c: Cores, escuro: boolean): Record<string, string> {
  const { marca, destaque } = coresDe(c);
  if (!escuro) {
    return {
      "--color-brand": marca,
      "--color-brand-600": mistura(marca, "#000000", 0.16),
      "--color-brand-700": mistura(marca, "#000000", 0.32),
      "--color-brand-50": mistura(marca, "#ffffff", 0.9),
      "--color-brand-100": mistura(marca, "#ffffff", 0.8),
      "--color-chart": mistura(marca, "#ffffff", 0.1),
      "--color-nav": ate(mistura(marca, "#000000", 0.32), "#ffffff", 7, "#000000"),
      "--color-accent": destaque,
      "--color-accent-50": mistura(destaque, "#ffffff", 0.88),
      "--color-accent-ink": ate(mistura(destaque, "#000000", 0.45), "#ffffff", 4.5, "#000000"),
    };
  }
  return {
    "--color-brand": mistura(marca, "#ffffff", 0.1),
    "--color-brand-600": mistura(marca, "#ffffff", 0.2),
    "--color-brand-700": mistura(marca, "#000000", 0.55),
    "--color-brand-50": mistura(marca, ESCURO_FUNDO, 0.78),
    "--color-brand-100": mistura(marca, ESCURO_FUNDO, 0.68),
    "--color-chart": mistura(marca, "#ffffff", 0.25),
    "--color-nav": mistura(marca, "#000000", 0.78),
    "--color-accent": destaque,
    "--color-accent-50": mistura(destaque, ESCURO_FUNDO, 0.85),
    "--color-accent-ink": ate(mistura(destaque, "#ffffff", 0.25), ESCURO_FUNDO, 4.5, "#ffffff"),
    "--cor-texto-marca": ate(mistura(marca, "#ffffff", 0.45), ESCURO_FUNDO, 4.5, "#ffffff"),
    "--cor-texto-marca-700": ate(mistura(marca, "#ffffff", 0.6), ESCURO_FUNDO, 6, "#ffffff"),
  };
}

const TODAS = [
  "--color-brand", "--color-brand-600", "--color-brand-700", "--color-brand-50", "--color-brand-100", "--color-chart", "--color-nav",
  "--color-accent", "--color-accent-50", "--color-accent-ink", "--cor-texto-marca", "--cor-texto-marca-700",
];

/** Põe o jogo de cores no documento (sem jogo, ou o do KanGest: as cores do index.css). */
export function aplicarCores(c: Cores | null | undefined, escuro: boolean) {
  const r = document.documentElement.style;
  for (const v of TODAS) r.removeProperty(v);
  if (!c || c.paleta === "petroleo") return;
  for (const [k, v] of Object.entries(variaveis(c, escuro))) r.setProperty(k, v);
}

const CHAVE = "kangest.cores";
/** As últimas cores da empresa neste aparelho (para o primeiro desenho já sair com elas). */
export function coresGuardadas(): Cores | null {
  try {
    const v = JSON.parse(localStorage.getItem(CHAVE) || "null");
    return v && typeof v.paleta === "string" ? v : null;
  } catch {
    return null;
  }
}
export function guardarCores(c: Cores | null) {
  try {
    if (c) localStorage.setItem(CHAVE, JSON.stringify(c));
    else localStorage.removeItem(CHAVE);
  } catch {
    /* sem armazenamento: aplica-se só nesta sessão */
  }
}

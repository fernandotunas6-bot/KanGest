/**
 * Código de barras Code 128 (conjuntos B e C) em SVG, sem dependências.
 * Usa o conjunto C para sequências pares de dígitos (códigos mais curtos) e o B para o resto.
 */
const PADROES = [
  "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213",
  "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132",
  "221231", "213212", "223112", "312131", "311222", "321122", "321221", "312212", "322112", "322211",
  "212123", "212321", "232121", "111323", "131123", "131321", "112313", "132113", "132311", "211313",
  "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121", "313121", "211331",
  "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
  "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214",
  "112412", "122114", "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111",
  "111242", "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
  "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311", "113141",
  "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];
const START_B = 104;
const START_C = 105;
const CODE_B = 100;
const CODE_C = 99;
const STOP = 106;

/** Valores (símbolos) Code 128 do texto, com início, trocas de conjunto e dígito de controlo. */
export function code128(texto: string): number[] {
  const s = [...texto].filter((c) => c.charCodeAt(0) >= 32 && c.charCodeAt(0) <= 126).join("");
  if (!s) return [];
  const digitos = (i: number) => {
    let n = 0;
    while (i + n < s.length && /\d/.test(s[i + n])) n++;
    return n;
  };
  const out: number[] = [];
  let set: "B" | "C";
  let i = 0;
  if (digitos(0) >= 4) {
    set = "C";
    out.push(START_C);
  } else {
    set = "B";
    out.push(START_B);
  }
  while (i < s.length) {
    const d = digitos(i);
    if (set === "B" && d >= 6 && d % 2 === 0) {
      out.push(CODE_C);
      set = "C";
    } else if (set === "B" && d >= 6) {
      // número ímpar de dígitos: o primeiro vai em B, o resto em C
      out.push(s.charCodeAt(i) - 32);
      i++;
      continue;
    }
    if (set === "C") {
      if (d >= 2) {
        out.push(Number(s.slice(i, i + 2)));
        i += 2;
        continue;
      }
      out.push(CODE_B);
      set = "B";
    }
    out.push(s.charCodeAt(i) - 32);
    i++;
  }
  const soma = out.reduce((acc, v, k) => acc + v * (k === 0 ? 1 : k), 0);
  out.push(soma % 103, STOP);
  return out;
}

/** Larguras alternadas barra/espaço (em módulos), incluindo as zonas de silêncio. */
export function barras(texto: string): string {
  const v = code128(texto);
  return v.map((x) => PADROES[x]).join("");
}

/** Caminho SVG das barras e largura total em módulos (com 10 módulos de silêncio de cada lado). */
export function svgBarras(texto: string, altura = 40): { d: string; largura: number } {
  const w = barras(texto);
  let x = 10;
  let d = "";
  for (let k = 0; k < w.length; k++) {
    const n = Number(w[k]);
    if (k % 2 === 0) d += `M${x} 0h${n}v${altura}h-${n}z`;
    x += n;
  }
  return { d, largura: x + 10 };
}

/** Valores monetários circulam em cêntimos (inteiros). */

/**
 * Número à portuguesa com separador de milhares SEMPRE (o pt-PT do browser não agrupa
 * números de 4 dígitos: "8208,00" ao lado de "82 992,00"). O separador é um espaço que não
 * parte a linha.
 */
const fmtCache = new Map<string, Intl.NumberFormat>();
export function numero(v: number, maxDec = 3, minDec = 0): string {
  const k = `${minDec}:${maxDec}`;
  let f = fmtCache.get(k);
  if (!f) {
    f = new Intl.NumberFormat("en-US", { minimumFractionDigits: minDec, maximumFractionDigits: maxDec, useGrouping: true });
    fmtCache.set(k, f);
  }
  // sem "−0": um valor que arredonda para zero mostra-se como 0
  const arred = Math.round(v * 10 ** maxDec) / 10 ** maxDec;
  return f.format(arred === 0 ? 0 : v).replace(/,/g, "\u00a0").replace(".", ",").replace("-", "\u2212");
}

export const kz = (c: number | null | undefined) => `${numero((c ?? 0) / 100, 2, 2)}\u00a0Kz`;
export const kzNum = (c: number | null | undefined) => numero((c ?? 0) / 100, 2, 2);
export const qtd = (n: number | null | undefined) => numero(n ?? 0, 3);
export const pct = (n: number | null | undefined) => `${numero(n ?? 0, 3)}%`;

/** "1 234,50" | "1234.5" -> 123450 */
export function parseKz(s: string): number {
  const limpo = s.replace(/\s|kz/gi, "");
  const normal = limpo.includes(",") ? limpo.replace(/\./g, "").replace(",", ".") : limpo;
  const v = Number(normal);
  return Number.isFinite(v) ? Math.round(v * 100) : 0;
}

export const toInputKz = (c: number) => (c / 100).toFixed(2).replace(".", ",");

export function data(d?: string | null) {
  if (!d) return "—";
  const [y, m, dd] = d.slice(0, 10).split("-");
  return `${dd}/${m}/${y}`;
}

export function dataHora(d?: string | null) {
  if (!d) return "—";
  return `${data(d)} ${d.slice(11, 16)}`;
}

export const hojeISO = () => {
  const d = new Date();
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
};

export const inicioMesISO = () => hojeISO().slice(0, 8) + "01";

/** Preço sem IVA a partir do PVP com IVA (cêntimos). */
export const semIva = (comIva: number, taxa: number) => Math.round(comIva / (1 + taxa / 100));
export const comIva = (semIvaC: number, taxa: number) => Math.round(semIvaC * (1 + taxa / 100));

/** Valor por extenso simples (Kwanzas), usado nos recibos. */
export function extenso(centimos: number): string {
  const u = ["zero", "um", "dois", "três", "quatro", "cinco", "seis", "sete", "oito", "nove", "dez", "onze", "doze", "treze", "catorze", "quinze", "dezasseis", "dezassete", "dezoito", "dezanove"];
  const d = ["", "", "vinte", "trinta", "quarenta", "cinquenta", "sessenta", "setenta", "oitenta", "noventa"];
  const c = ["", "cento", "duzentos", "trezentos", "quatrocentos", "quinhentos", "seiscentos", "setecentos", "oitocentos", "novecentos"];
  const ate999 = (n: number): string => {
    if (n === 100) return "cem";
    const partes: string[] = [];
    if (n >= 100) partes.push(c[Math.floor(n / 100)]);
    const r = n % 100;
    if (r >= 20) partes.push(d[Math.floor(r / 10)] + (r % 10 ? " e " + u[r % 10] : ""));
    else if (r > 0) partes.push(u[r]);
    return partes.join(" e ");
  };
  const grupo = (n: number): string => {
    if (n === 0) return "zero";
    const escalas: [number, string, string][] = [[1e9, "mil milhões", "mil milhões"], [1e6, "milhão", "milhões"], [1e3, "mil", "mil"]];
    const partes: string[] = [];
    let resto = n;
    for (const [v, s, p] of escalas) {
      const q = Math.floor(resto / v);
      if (q) {
        partes.push(v === 1e3 && q === 1 ? "mil" : `${ate999(q)} ${q === 1 ? s : p}`);
        resto %= v;
      }
    }
    if (resto) partes.push(ate999(resto));
    return partes.join(partes.length > 1 && resto && resto < 100 ? " e " : " ");
  };
  const kzInt = Math.floor(Math.abs(centimos) / 100);
  const cents = Math.abs(centimos) % 100;
  let s = `${grupo(kzInt)} ${kzInt === 1 ? "kwanza" : "kwanzas"}`;
  if (cents) s += ` e ${ate999(cents)} ${cents === 1 ? "cêntimo" : "cêntimos"}`;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

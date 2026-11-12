/**
 * QR Code (modelo 2) em modo byte UTF-8, sem dependências. Segue o algoritmo de referência
 * (ISO/IEC 18004): codificação, Reed-Solomon em GF(256), entrelaçamento por blocos, padrões
 * de função, informação de formato e de versão, escolha da máscara com menor penalização.
 *
 * Usado no QR code da Facturação Electrónica da AGT (correcção de erros M).
 */

export type Nivel = "L" | "M" | "Q" | "H";

const ECC_POR_BLOCO: Record<Nivel, number[]> = {
  L: [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  M: [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
  Q: [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  H: [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
};
const BLOCOS: Record<Nivel, number[]> = {
  L: [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  M: [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
  Q: [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
  H: [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
};
const FORMATO: Record<Nivel, number> = { L: 1, M: 0, Q: 3, H: 2 };

function modulosDados(v: number): number {
  let r = (16 * v + 128) * v + 64;
  if (v >= 2) {
    const n = Math.floor(v / 7) + 2;
    r -= (25 * n - 10) * n - 55;
    if (v >= 7) r -= 36;
  }
  return r;
}
const palavrasDados = (v: number, n: Nivel) => Math.floor(modulosDados(v) / 8) - ECC_POR_BLOCO[n][v] * BLOCOS[n][v];

function mult(x: number, y: number): number {
  let z = 0;
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d);
    z ^= ((y >>> i) & 1) * x;
  }
  return z & 0xff;
}
function divisor(grau: number): number[] {
  const r = new Array(grau - 1).fill(0).concat([1]);
  let raiz = 1;
  for (let i = 0; i < grau; i++) {
    for (let j = 0; j < r.length; j++) {
      r[j] = mult(r[j], raiz);
      if (j + 1 < r.length) r[j] ^= r[j + 1];
    }
    raiz = mult(raiz, 0x02);
  }
  return r;
}
function resto(dados: number[], div: number[]): number[] {
  const r = div.map(() => 0);
  for (const b of dados) {
    const f = b ^ (r.shift() as number);
    r.push(0);
    div.forEach((c, i) => (r[i] ^= mult(c, f)));
  }
  return r;
}

export interface QR {
  tamanho: number;
  modulos: boolean[][];
  versao: number;
}

/** Codifica o texto; escolhe a menor versão (≥ minVersao) que o comporta. */
export function qrcode(texto: string, nivel: Nivel = "M", minVersao = 1): QR {
  const bytes = Array.from(new TextEncoder().encode(texto));
  let versao = minVersao;
  for (; ; versao++) {
    if (versao > 40) throw new Error("Texto demasiado longo para um QR code.");
    const cc = versao < 10 ? 8 : 16;
    if (4 + cc + bytes.length * 8 <= palavrasDados(versao, nivel) * 8) break;
  }
  // ---- fluxo de bits
  const bits: number[] = [];
  const junta = (v: number, n: number) => {
    for (let i = n - 1; i >= 0; i--) bits.push((v >>> i) & 1);
  };
  junta(0b0100, 4);
  junta(bytes.length, versao < 10 ? 8 : 16);
  bytes.forEach((b) => junta(b, 8));
  const capacidade = palavrasDados(versao, nivel) * 8;
  junta(0, Math.min(4, capacidade - bits.length));
  junta(0, (8 - (bits.length % 8)) % 8);
  for (let p = 0xec; bits.length < capacidade; p ^= 0xec ^ 0x11) junta(p, 8);
  const dados: number[] = [];
  for (let i = 0; i < bits.length; i += 8) dados.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0));

  // ---- blocos + Reed-Solomon, entrelaçados
  const nb = BLOCOS[nivel][versao];
  const ecc = ECC_POR_BLOCO[nivel][versao];
  const total = Math.floor(modulosDados(versao) / 8);
  const curtos = nb - (total % nb);
  const tamCurto = Math.floor(total / nb);
  const div = divisor(ecc);
  const blocos: number[][] = [];
  for (let i = 0, k = 0; i < nb; i++) {
    const dat = dados.slice(k, k + tamCurto - ecc + (i < curtos ? 0 : 1));
    k += dat.length;
    const b = dat.concat(resto(dat, div));
    if (i < curtos) b.splice(tamCurto - ecc, 0, -1); // alinha blocos curtos
    blocos.push(b);
  }
  const palavras: number[] = [];
  for (let i = 0; i < blocos[0].length; i++) blocos.forEach((b, j) => (i !== tamCurto - ecc || j >= curtos) && palavras.push(b[i]));

  // ---- matriz
  const n = versao * 4 + 17;
  const m: boolean[][] = Array.from({ length: n }, () => new Array(n).fill(false));
  const funcao: boolean[][] = Array.from({ length: n }, () => new Array(n).fill(false));
  const pinta = (x: number, y: number, escuro: boolean) => {
    m[y][x] = escuro;
    funcao[y][x] = true;
  };
  for (let i = 0; i < n; i++) {
    pinta(6, i, i % 2 === 0);
    pinta(i, 6, i % 2 === 0);
  }
  const localizador = (x: number, y: number) => {
    for (let dy = -4; dy <= 4; dy++)
      for (let dx = -4; dx <= 4; dx++) {
        const d = Math.max(Math.abs(dx), Math.abs(dy));
        const xx = x + dx;
        const yy = y + dy;
        if (xx >= 0 && xx < n && yy >= 0 && yy < n) pinta(xx, yy, d !== 2 && d !== 4);
      }
  };
  localizador(3, 3);
  localizador(n - 4, 3);
  localizador(3, n - 4);
  if (versao > 1) {
    const na = Math.floor(versao / 7) + 2;
    const passo = versao === 32 ? 26 : Math.ceil((versao * 4 + 4) / (na * 2 - 2)) * 2;
    const pos = [6];
    for (let p = n - 7; pos.length < na; p -= passo) pos.splice(1, 0, p);
    pos.forEach((a, i) =>
      pos.forEach((b, j) => {
        if ((i === 0 && j === 0) || (i === 0 && j === na - 1) || (i === na - 1 && j === 0)) return;
        for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) pinta(a + dx, b + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
      }),
    );
  }
  const formato = (mascara: number) => {
    const d = (FORMATO[nivel] << 3) | mascara;
    let r = d;
    for (let i = 0; i < 10; i++) r = (r << 1) ^ ((r >>> 9) * 0x537);
    const b = ((d << 10) | r) ^ 0x5412;
    const bit = (i: number) => ((b >>> i) & 1) !== 0;
    for (let i = 0; i <= 5; i++) pinta(8, i, bit(i));
    pinta(8, 7, bit(6));
    pinta(8, 8, bit(7));
    pinta(7, 8, bit(8));
    for (let i = 9; i < 15; i++) pinta(14 - i, 8, bit(i));
    for (let i = 0; i < 8; i++) pinta(n - 1 - i, 8, bit(i));
    for (let i = 8; i < 15; i++) pinta(8, n - 15 + i, bit(i));
    pinta(8, n - 8, true);
  };
  formato(0); // reserva as áreas
  if (versao >= 7) {
    let r = versao;
    for (let i = 0; i < 12; i++) r = (r << 1) ^ ((r >>> 11) * 0x1f25);
    const b = (versao << 12) | r;
    for (let i = 0; i < 18; i++) {
      const bit = ((b >>> i) & 1) !== 0;
      const a = n - 11 + (i % 3);
      const c = Math.floor(i / 3);
      pinta(a, c, bit);
      pinta(c, a, bit);
    }
  }
  // ---- dados em zigue-zague
  let i = 0;
  for (let dir = n - 1; dir >= 1; dir -= 2) {
    if (dir === 6) dir = 5;
    for (let v = 0; v < n; v++)
      for (let j = 0; j < 2; j++) {
        const x = dir - j;
        const cima = ((dir + 1) & 2) === 0;
        const y = cima ? n - 1 - v : v;
        if (!funcao[y][x] && i < palavras.length * 8) {
          m[y][x] = ((palavras[i >>> 3] >>> (7 - (i & 7))) & 1) !== 0;
          i++;
        }
      }
  }
  // ---- máscara com menor penalização
  const aplica = (k: number) => {
    for (let y = 0; y < n; y++)
      for (let x = 0; x < n; x++) {
        if (funcao[y][x]) continue;
        const inv = [(x + y) % 2 === 0, y % 2 === 0, x % 3 === 0, (x + y) % 3 === 0, (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0, ((x * y) % 2) + ((x * y) % 3) === 0, (((x * y) % 2) + ((x * y) % 3)) % 2 === 0, (((x + y) % 2) + ((x * y) % 3)) % 2 === 0][k];
        if (inv) m[y][x] = !m[y][x];
      }
  };
  let melhor = 0;
  let menor = Infinity;
  for (let k = 0; k < 8; k++) {
    aplica(k);
    formato(k);
    const p = penalizacao(m);
    if (p < menor) {
      menor = p;
      melhor = k;
    }
    aplica(k);
  }
  aplica(melhor);
  formato(melhor);
  return { tamanho: n, modulos: m, versao };
}

function penalizacao(m: boolean[][]): number {
  const n = m.length;
  let p = 0;
  const linha = (get: (i: number, j: number) => boolean) => {
    for (let i = 0; i < n; i++) {
      let corrida = 1;
      for (let j = 1; j <= n; j++) {
        if (j < n && get(i, j) === get(i, j - 1)) corrida++;
        else {
          if (corrida >= 5) p += 3 + (corrida - 5);
          corrida = 1;
        }
      }
      // padrões semelhantes ao localizador (1:1:3:1:1 com 4 claros)
      for (let j = 0; j + 11 <= n; j++) {
        const s = Array.from({ length: 11 }, (_, k) => (get(i, j + k) ? "1" : "0")).join("");
        if (s === "10111010000" || s === "00001011101") p += 40;
      }
    }
  };
  linha((i, j) => m[i][j]);
  linha((i, j) => m[j][i]);
  for (let y = 0; y < n - 1; y++) for (let x = 0; x < n - 1; x++) if (m[y][x] === m[y][x + 1] && m[y][x] === m[y + 1][x] && m[y][x] === m[y + 1][x + 1]) p += 3;
  const escuros = m.reduce((s, r) => s + r.filter(Boolean).length, 0);
  const k = Math.ceil(Math.abs(escuros * 20 - n * n * 10) / (n * n)) - 1;
  return p + Math.max(0, k) * 10;
}

/** Caminho SVG dos módulos escuros (uma unidade por módulo, com margem de 4). */
export function qrSvgPath(q: QR): string {
  let d = "";
  q.modulos.forEach((r, y) =>
    r.forEach((v, x) => {
      if (v) d += `M${x + 4} ${y + 4}h1v1h-1z`;
    }),
  );
  return d;
}

/** Endereço do QR code da Facturação Electrónica da AGT (espaços do n.º do documento como %20). */
export function urlFacturaAgt(nifEmissor: string, documentNo: string): string {
  return `https://quiosqueagt.minfin.gov.ao/facturacao-eletronica/consultar-fe?emissor=${encodeURIComponent(nifEmissor)}&document=${documentNo.replace(/ /g, "%20")}`;
}

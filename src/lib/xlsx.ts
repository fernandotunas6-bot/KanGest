/**
 * Livro Excel (.xlsx) escrito de raiz, sem bibliotecas: um .zip («stored», sem compressão) com o XML mínimo do
 * formato Office Open XML. Os números saem como números (o Excel soma-os), os valores em kwanzas com
 * «#,##0.00» e as datas como datas (dd/mm/aaaa).
 */

/** Célula: texto, número, valor em cêntimos ({ kz }), data ISO ({ data }) ou vazia. */
export type CelulaXL = string | number | null | undefined | { kz: number } | { data: string | null | undefined };

export interface FolhaXL {
  nome: string;
  linhas: CelulaXL[][];
  /** linhas do topo a negrito (cabeçalho da tabela); por omissão 1 */
  cabecalho?: number;
  /** filtro automático e painel fixo a partir desta linha de cabeçalho (1 = primeira); por omissão = cabecalho */
  linhaTabela?: number;
}

/** Valores monetários em cêntimos (como circulam no KanGest) → célula em kwanzas. */
export const kzXL = (centimos: number | null | undefined): CelulaXL => ({ kz: centimos ?? 0 });
export const dataXL = (iso: string | null | undefined): CelulaXL => ({ data: iso });

// estilos (índices em cellXfs): 0 normal · 1 kwanzas · 2 data · 3 cabeçalho a negrito
const ESTILO_KZ = 1;
const ESTILO_DATA = 2;
const ESTILO_CAB = 3;

const esc = (s: string) =>
  s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    // caracteres de controlo não são XML válido
    // eslint-disable-next-line no-control-regex
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "");

function coluna(i: number): string {
  let s = "";
  for (let n = i + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}

/** Dias desde 30/12/1899 (o «zero» das datas do Excel). */
function serieExcel(iso: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2}))?/.exec(iso);
  if (!m) return null;
  const dias = Date.UTC(+m[1], +m[2] - 1, +m[3]) / 86_400_000 + 25569;
  return m[4] ? dias + (+m[4] * 60 + +m[5]) / 1440 : dias;
}

function texto(c: CelulaXL): string {
  if (c === null || c === undefined) return "";
  if (typeof c === "object") return "kz" in c ? (c.kz / 100).toFixed(2) : (c.data ?? "");
  return String(c);
}

function folhaXML(f: FolhaXL): string {
  const cab = f.cabecalho ?? 1;
  const tabela = f.linhaTabela ?? cab;
  const ncols = Math.max(1, ...f.linhas.map((l) => l.length));
  // largura de cada coluna pelo texto mais comprido (entre 8 e 60 caracteres)
  const larguras = Array.from({ length: ncols }, (_, j) => Math.min(60, Math.max(8, ...f.linhas.map((l) => texto(l[j]).length + 2))));
  const linhas = f.linhas
    .map((l, i) => {
      const r = i + 1;
      const cel = l
        .map((c, j) => {
          const ref = `${coluna(j)}${r}`;
          const negrito = r <= cab ? ` s="${ESTILO_CAB}"` : "";
          if (c === null || c === undefined || c === "") return "";
          if (typeof c === "number") return Number.isFinite(c) ? `<c r="${ref}"${negrito}><v>${c}</v></c>` : "";
          if (typeof c === "object") {
            if ("kz" in c) return `<c r="${ref}" s="${ESTILO_KZ}"><v>${(c.kz / 100).toFixed(2)}</v></c>`;
            const n = c.data ? serieExcel(c.data) : null;
            return n === null ? "" : `<c r="${ref}" s="${ESTILO_DATA}"><v>${n}</v></c>`;
          }
          return `<c r="${ref}" t="inlineStr"${negrito}><is><t xml:space="preserve">${esc(c)}</t></is></c>`;
        })
        .join("");
      return `<row r="${r}">${cel}</row>`;
    })
    .join("");
  const fim = `${coluna(ncols - 1)}${Math.max(tabela, f.linhas.length)}`;
  const painel =
    tabela > 0
      ? `<sheetViews><sheetView workbookViewId="0"><pane ySplit="${tabela}" topLeftCell="A${tabela + 1}" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>`
      : "";
  return (
    `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` +
    `<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">` +
    painel +
    `<cols>${larguras.map((w, j) => `<col min="${j + 1}" max="${j + 1}" width="${w}" customWidth="1"/>`).join("")}</cols>` +
    `<sheetData>${linhas}</sheetData>` +
    (tabela > 0 && f.linhas.length > tabela ? `<autoFilter ref="A${tabela}:${fim}"/>` : "") +
    `</worksheet>`
  );
}

const ESTILOS =
  `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` +
  `<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">` +
  `<numFmts count="1"><numFmt numFmtId="164" formatCode="dd/mm/yyyy"/></numFmts>` +
  `<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts>` +
  `<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>` +
  `<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>` +
  `<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>` +
  `<cellXfs count="4">` +
  `<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>` +
  `<xf numFmtId="4" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>` +
  `<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>` +
  `<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/>` +
  `</cellXfs>` +
  `<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>` +
  `</styleSheet>`;

/** Nome de folha válido no Excel: até 31 caracteres, sem []:*?/\ e sem repetir. */
function nomesFolhas(folhas: FolhaXL[]): string[] {
  const usados = new Set<string>();
  return folhas.map((f, i) => {
    let n = (f.nome || `Folha${i + 1}`).replace(/[[\]:*?/\\]/g, " ").trim().slice(0, 31) || `Folha${i + 1}`;
    for (let k = 2; usados.has(n.toLowerCase()); k++) n = `${n.slice(0, 28)} ${k}`;
    usados.add(n.toLowerCase());
    return n;
  });
}

/** O livro inteiro, em bytes, pronto a gravar como .xlsx. */
export function livroXLSX(folhas: FolhaXL[]): Uint8Array {
  const nomes = nomesFolhas(folhas);
  const ficheiros: [string, string][] = [
    [
      "[Content_Types].xml",
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">` +
        `<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>` +
        `<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>` +
        `<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>` +
        folhas.map((_, i) => `<Override PartName="/xl/worksheets/sheet${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`).join("") +
        `</Types>`,
    ],
    [
      "_rels/.rels",
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` +
        `<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>`,
    ],
    [
      "xl/workbook.xml",
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>` +
        nomes.map((n, i) => `<sheet name="${esc(n)}" sheetId="${i + 1}" r:id="rId${i + 1}"/>`).join("") +
        `</sheets></workbook>`,
    ],
    [
      "xl/_rels/workbook.xml.rels",
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` +
        folhas.map((_, i) => `<Relationship Id="rId${i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`).join("") +
        `<Relationship Id="rId${folhas.length + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>`,
    ],
    ["xl/styles.xml", ESTILOS],
    ...folhas.map((f, i): [string, string] => [`xl/worksheets/sheet${i + 1}.xml`, folhaXML(f)]),
  ];
  return zip(ficheiros.map(([n, c]) => [n, new TextEncoder().encode(c)]));
}

// ------------------------------------------------------------------ zip («stored») com CRC-32

const TABELA_CRC = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(b: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < b.length; i++) c = TABELA_CRC[(c ^ b[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function zip(ficheiros: [string, Uint8Array][]): Uint8Array {
  const agora = new Date();
  const hora = (agora.getHours() << 11) | (agora.getMinutes() << 5) | (agora.getSeconds() >> 1);
  const dia = ((agora.getFullYear() - 1980) << 9) | ((agora.getMonth() + 1) << 5) | agora.getDate();
  const partes: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let pos = 0;
  for (const [nome, dados] of ficheiros) {
    const n = new TextEncoder().encode(nome);
    const crc = crc32(dados);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint16(6, 0x0800, true); // nomes em UTF-8
    local.setUint16(8, 0, true); // sem compressão
    local.setUint16(10, hora, true);
    local.setUint16(12, dia, true);
    local.setUint32(14, crc, true);
    local.setUint32(18, dados.length, true);
    local.setUint32(22, dados.length, true);
    local.setUint16(26, n.length, true);
    local.setUint16(28, 0, true);
    const cd = new DataView(new ArrayBuffer(46));
    cd.setUint32(0, 0x02014b50, true);
    cd.setUint16(4, 20, true);
    cd.setUint16(6, 20, true);
    cd.setUint16(8, 0x0800, true);
    cd.setUint16(10, 0, true);
    cd.setUint16(12, hora, true);
    cd.setUint16(14, dia, true);
    cd.setUint32(16, crc, true);
    cd.setUint32(20, dados.length, true);
    cd.setUint32(24, dados.length, true);
    cd.setUint16(28, n.length, true);
    cd.setUint32(42, pos, true);
    partes.push(new Uint8Array(local.buffer), n, dados);
    central.push(new Uint8Array(cd.buffer), n);
    pos += 30 + n.length + dados.length;
  }
  const tamanhoCentral = central.reduce((s, b) => s + b.length, 0);
  const fim = new DataView(new ArrayBuffer(22));
  fim.setUint32(0, 0x06054b50, true);
  fim.setUint16(8, ficheiros.length, true);
  fim.setUint16(10, ficheiros.length, true);
  fim.setUint32(12, tamanhoCentral, true);
  fim.setUint32(16, pos, true);
  const tudo = [...partes, ...central, new Uint8Array(fim.buffer)];
  const out = new Uint8Array(tudo.reduce((s, b) => s + b.length, 0));
  let o = 0;
  for (const b of tudo) {
    out.set(b, o);
    o += b.length;
  }
  return out;
}

/** Bytes → base64 (para atravessar o RPC, que só leva texto). */
export function base64(b: Uint8Array): string {
  let s = "";
  for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
  return btoa(s);
}

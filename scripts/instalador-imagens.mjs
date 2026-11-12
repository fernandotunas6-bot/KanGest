// Imagens do instalador de Windows (NSIS) com o logótipo do KanGest:
//   src-tauri/icons/nsis-cabecalho.bmp  150 × 57  (cabeçalho das páginas interiores)
//   src-tauri/icons/nsis-lateral.bmp    164 × 314 (páginas de boas-vindas e de fim)
// O NSIS só aceita BMP de 24 bits: o PNG da captura do Chrome é descodificado e regravado aqui.
// Precisa de um Chrome sem janela aberto com --remote-debugging-port=9333 (ver scripts/auditoria/README.md).
import { readFileSync, writeFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

const svg = readFileSync(new URL("../src/assets/kigest-icone.svg", import.meta.url), "utf8").replace(/<\?xml[^>]*>/, "");
const logo = (px) => svg.replace("<svg ", `<svg width="${px}" height="${px}" `);
const fonte = `font-family:'Segoe UI','Helvetica Neue',Arial,sans-serif`;

const cabecalho = `<div style="width:150px;height:57px;background:#fff;display:flex;align-items:center;justify-content:flex-end;gap:7px;padding-right:10px;box-sizing:border-box;${fonte}">
  <div style="text-align:right;line-height:1.05"><div style="font-size:17px;font-weight:700;color:#0b433e">KanGest</div><div style="font-size:9px;color:#5b6b69">Gestão e POS</div></div>
  ${logo(40)}
</div>`;

const lateral = `<div style="width:164px;height:314px;background:linear-gradient(165deg,#1c8c80 0%,#0f5c55 55%,#0b433e 100%);color:#fff;${fonte};display:flex;flex-direction:column;align-items:center;box-sizing:border-box;padding-top:46px;position:relative;overflow:hidden">
  <div style="position:absolute;right:-48px;top:-48px;width:150px;height:150px;border-radius:50%;background:rgba(255,255,255,.06)"></div>
  <div style="position:absolute;left:-60px;bottom:-70px;width:190px;height:190px;border-radius:50%;background:rgba(240,165,49,.10)"></div>
  ${logo(84)}
  <div style="margin-top:14px;font-size:24px;font-weight:700;letter-spacing:.2px">KanGest</div>
  <div style="margin-top:4px;font-size:11px;opacity:.85;text-align:center;line-height:1.35">Gestão Comercial<br>e Ponto de Venda</div>
  <div style="position:absolute;bottom:16px;font-size:10px;opacity:.7">Versão ${JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version}</div>
</div>`;

const alvo = (await (await fetch("http://127.0.0.1:9333/json/list")).json()).find((t) => t.type === "page");
const ws = new WebSocket(alvo.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pend = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pend.has(d.id)) pend.get(d.id)(d), pend.delete(d.id);
};
const cmd = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await cmd("Page.enable");

async function captura(html, w, h) {
  await cmd("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
  const pagina = `<!doctype html><html><head><meta charset="utf-8"></head><body style="margin:0">${html}</body></html>`;
  await cmd("Page.navigate", { url: "data:text/html;charset=utf-8;base64," + Buffer.from(pagina).toString("base64") });
  await new Promise((r) => setTimeout(r, 700));
  const shot = await cmd("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: w, height: h, scale: 1 } });
  return Buffer.from(shot.result.data, "base64");
}

/** PNG de 8 bits (RGB ou RGBA, sem entrelaçamento) → píxeis RGB. */
function lerPng(buf) {
  let p = 8, w = 0, h = 0, tipo = 0;
  const idat = [];
  while (p < buf.length) {
    const len = buf.readUInt32BE(p), nome = buf.toString("ascii", p + 4, p + 8), dados = buf.subarray(p + 8, p + 8 + len);
    if (nome === "IHDR") {
      w = dados.readUInt32BE(0); h = dados.readUInt32BE(4); tipo = dados[9];
      if (dados[8] !== 8 || dados[12] !== 0 || (tipo !== 2 && tipo !== 6)) throw new Error("PNG não suportado");
    } else if (nome === "IDAT") idat.push(dados);
    p += 12 + len;
  }
  const bpp = tipo === 6 ? 4 : 3, linha = w * bpp, raw = inflateSync(Buffer.concat(idat));
  const px = Buffer.alloc(h * linha);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (linha + 1)], src = raw.subarray(y * (linha + 1) + 1, (y + 1) * (linha + 1));
    for (let x = 0; x < linha; x++) {
      const a = x >= bpp ? px[y * linha + x - bpp] : 0, b = y ? px[(y - 1) * linha + x] : 0, c = x >= bpp && y ? px[(y - 1) * linha + x - bpp] : 0;
      const pa = Math.abs(b - c), pb = Math.abs(a - c), pc = Math.abs(a + b - 2 * c);
      const pred = [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][f];
      px[y * linha + x] = (src[x] + pred) & 255;
    }
  }
  const rgb = Buffer.alloc(w * h * 3);
  for (let i = 0; i < w * h; i++) {
    const al = bpp === 4 ? px[i * 4 + 3] / 255 : 1; // fundo branco onde houver transparência
    for (let k = 0; k < 3; k++) rgb[i * 3 + k] = Math.round(px[i * bpp + k] * al + 255 * (1 - al));
  }
  return { w, h, rgb };
}

/** BMP de 24 bits (linhas de baixo para cima, BGR, alinhadas a 4 bytes). */
function bmp({ w, h, rgb }) {
  const linha = Math.ceil((w * 3) / 4) * 4, tam = 54 + linha * h, b = Buffer.alloc(tam);
  b.write("BM", 0); b.writeUInt32LE(tam, 2); b.writeUInt32LE(54, 10);
  b.writeUInt32LE(40, 14); b.writeInt32LE(w, 18); b.writeInt32LE(h, 22); b.writeUInt16LE(1, 26); b.writeUInt16LE(24, 28);
  b.writeUInt32LE(linha * h, 34); b.writeInt32LE(2835, 38); b.writeInt32LE(2835, 42);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = 54 + (h - 1 - y) * linha + x * 3, i = (y * w + x) * 3;
      b[o] = rgb[i + 2]; b[o + 1] = rgb[i + 1]; b[o + 2] = rgb[i];
    }
  return b;
}

const pasta = new URL("../src-tauri/icons/", import.meta.url);
for (const [nome, html, w, h] of [["nsis-cabecalho", cabecalho, 150, 57], ["nsis-lateral", lateral, 164, 314]]) {
  const png = await captura(html, w, h);
  writeFileSync(new URL(`${nome}.bmp`, pasta), bmp(lerPng(png)));
  console.log("gravado:", `src-tauri/icons/${nome}.bmp`);
}
await cmd("Emulation.clearDeviceMetricsOverride");
ws.close();

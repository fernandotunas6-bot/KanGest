// Gera src-tauri/icons/app-icon.png (1024 × 1024, fundo transparente) a partir de src/assets/kigest-icone.svg,
// desenhando o SVG num Chrome sem janela já aberto com --remote-debugging-port=9333.
// Depois: npx tauri icon src-tauri/icons/app-icon.png
import { readFileSync, writeFileSync } from "node:fs";

const svg = readFileSync(new URL("../src/assets/kigest-icone.svg", import.meta.url), "utf8");
const html = `<!doctype html><html><body style="margin:0;background:transparent">${svg.replace("<svg ", '<svg width="1024" height="1024" ')}</body></html>`;
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
await cmd("Emulation.setDeviceMetricsOverride", { width: 1024, height: 1024, deviceScaleFactor: 1, mobile: false });
await cmd("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });
await cmd("Page.navigate", { url: "data:text/html;base64," + Buffer.from(html).toString("base64") });
await new Promise((r) => setTimeout(r, 800));
const shot = await cmd("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: 1024, height: 1024, scale: 1 } });
const destino = process.argv[2] ?? new URL("../src-tauri/icons/app-icon.png", import.meta.url).pathname;
writeFileSync(destino, Buffer.from(shot.result.data, "base64"));
await cmd("Emulation.clearDeviceMetricsOverride");
await cmd("Emulation.setDefaultBackgroundColorOverride", {});
ws.close();
console.log("ícone gravado:", destino);

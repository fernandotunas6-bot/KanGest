// Auditoria visual: controla um Chrome sem janela (porta de depuração 9333) e tira capturas de ecrã.
//
//   node scripts/auditoria/capturas.mjs <pasta> "rota=nome[@accao:arg...]" ...
//
// rota: caminho depois do "#" (ex.: /pos, /configuracoes?aba=rede). O nome vem depois do ÚLTIMO "=".
// Acções: @texto:<início do texto> (clica no elemento mais interior com esse texto),
//         @foco:<início do placeholder> ou @campo:<início do rótulo> e @escrever:<texto> (preencher caixas; %40 = @),
//         @clique:<texto> (botões/links), @titulo:<início do title> (botões só com ícone),
//         @tecla:<F12|Enter…>, @rolar:<px>,
//         @impressao (mostra a página como sai na impressora: só o conteúdo de impressão).
// Tamanho da janela: variáveis W e H (por omissão 1366 × 820).
// Para cada ecrã escreve os erros e avisos da consola (ou "sem erros").
import { writeFileSync, mkdirSync } from "node:fs";

const [saida, ...pedidos] = process.argv.slice(2);
mkdirSync(saida, { recursive: true });
const base = process.env.URL ?? "http://localhost:1422/#";
const alvo = (await (await fetch("http://127.0.0.1:9333/json/list")).json()).find((t) => t.type === "page");
const ws = new WebSocket(alvo.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));

let id = 0;
const pend = new Map();
const erros = [];
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pend.has(d.id)) {
    pend.get(d.id)(d);
    pend.delete(d.id);
  }
  if (d.method === "Runtime.exceptionThrown") erros.push("EXC " + (d.params.exceptionDetails.exception?.description ?? d.params.exceptionDetails.text).split("\n")[0]);
  if (d.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(d.params.type))
    erros.push(d.params.type.toUpperCase() + " " + d.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 300));
};
const cmd = (method, params = {}) =>
  new Promise((r) => {
    const i = ++id;
    pend.set(i, r);
    ws.send(JSON.stringify({ id: i, method, params }));
  });
const espera = (ms) => new Promise((r) => setTimeout(r, ms));

await cmd("Runtime.enable");
await cmd("Page.enable");
await cmd("Emulation.setDeviceMetricsOverride", { width: Number(process.env.W ?? 1366), height: Number(process.env.H ?? 820), deviceScaleFactor: 1, mobile: false });

for (const p of pedidos) {
  const corte = p.lastIndexOf("=");
  const rota = p.slice(0, corte);
  const [nome, ...accoes] = p.slice(corte + 1).split("@");
  erros.length = 0;
  await cmd("Emulation.setEmulatedMedia", { media: "" });
  await cmd("Page.navigate", { url: "about:blank" });
  await espera(300);
  await cmd("Page.navigate", { url: base + rota });
  await espera(1800);
  for (const a of accoes) {
    const [tipo, ...arg] = a.split(":");
    const texto = arg.join(":");
    if (tipo === "clique") {
      await cmd("Runtime.evaluate", {
        expression: `(() => { const el = [...document.querySelectorAll('button,a,[role=button],label,summary')].find(e => e.textContent.trim().includes(${JSON.stringify(texto)})); if (el) { el.click(); return true } return false })()`,
      });
    } else if (tipo === "titulo") {
      await cmd("Runtime.evaluate", {
        expression: `(() => { const el = [...document.querySelectorAll('[title]')].find(e => e.title.startsWith(${JSON.stringify(texto)})); if (el) { el.click(); return true } return false })()`,
      });
    } else if (tipo === "texto") {
      await cmd("Runtime.evaluate", {
        expression: `(() => { const t = ${JSON.stringify(texto)}; const els = [...document.querySelectorAll('body *')].filter(e => e.textContent.trim().startsWith(t)); const el = els.find(e => ![...e.children].some(c => c.textContent.trim().startsWith(t))); if (el) { el.click(); return true } return false })()`,
      });
    } else if (tipo === "foco") {
      await cmd("Runtime.evaluate", {
        expression: `(() => { const el = [...document.querySelectorAll('input,textarea')].find(e => (e.placeholder || '').startsWith(${JSON.stringify(texto)})); if (el) { el.focus(); return true } return false })()`,
      });
    } else if (tipo === "campo") {
      await cmd("Runtime.evaluate", {
        expression: `(() => { const l = [...document.querySelectorAll('label')].find(e => e.textContent.trim().toLowerCase().startsWith(${JSON.stringify(texto.toLowerCase())})); const el = l?.querySelector('input,textarea,select') ?? (l?.htmlFor ? document.getElementById(l.htmlFor) : null); if (el) { el.focus(); return true } return false })()`,
      });
    } else if (tipo === "escrever") {
      await cmd("Input.insertText", { text: texto.replaceAll("%40", "@") }); // %40 escreve «@» (o @ separa acções)
    } else if (tipo === "tecla") {
      await cmd("Input.dispatchKeyEvent", { type: "keyDown", key: texto, code: texto });
      await cmd("Input.dispatchKeyEvent", { type: "keyUp", key: texto, code: texto });
    } else if (tipo === "impressao") {
      await cmd("Emulation.setEmulatedMedia", { media: "print" });
    } else if (tipo === "rolar") {
      const y = texto.trim() !== "" && Number.isFinite(Number(texto)) ? Number(texto) : 99999;
      await cmd("Runtime.evaluate", { expression: `document.querySelector('main')?.scrollTo(0, ${y})` });
    }
    await espera(900);
  }
  const shot = await cmd("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${saida}/${nome}.png`, Buffer.from(shot.result.data, "base64"));
  console.log(`${nome}: ${erros.length ? erros.join(" | ") : "sem erros"}`);
}
ws.close();

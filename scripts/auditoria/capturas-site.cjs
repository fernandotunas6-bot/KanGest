// Capturas do site (site-fonte/assets/ecras) com a interface actual, a partir do servidor de demonstração
// (ver README.md desta pasta: servidor na porta 7799 e frontend na 1422). Usa o Playwright com o Chromium.
//
//   node scripts/auditoria/capturas-site.cjs                 → grava os .jpg 1440×900 em site-fonte/assets/ecras
//   python3 scripts/auditoria/miniaturas-site.py             → gera as versões -720 a partir deles
//
// Só se publicam no site com uma versão nova (o site mostra o que se descarrega): gerar e publicar com
// scripts/gerar-site.py e scripts/publicar-site-cloudflare.py.
const path = require("node:path");
const { chromium } = require("playwright");

const SAIDA = path.resolve(__dirname, "../../site-fonte/assets/ecras");
const BASE = process.env.URL ?? "http://localhost:1422/#";

/** nome → rota e o que fazer antes da captura */
const ECRAS = [
  ["painel", "/"],
  ["documentos", "/documentos?grupo=VENDAS"],
  ["stock", "/stock"],
  ["relatorios", "/relatorios?agrupar=artigo"],
  ["tesouraria", "/tesouraria"],
  ["contas", "/contas/receber"],
  ["importar", "/importar"],
  ["salarios", "/rh/salarios?vista=processamentos"],
  ["utilizadores", "/utilizadores"],
  [
    "pos",
    "/pos",
    async (p) => {
      for (const t of ["Arroz Tio Lucas 5kg", "Coca-Cola 33cl", "Coca-Cola 33cl", "Leite Nido 400g", "Óleo Fula 1L"]) {
        await p.getByText(t, { exact: true }).first().click();
        await p.waitForTimeout(200);
      }
    },
  ],
];

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  // aparência por omissão (tema claro, menu aberto, tabelas confortáveis), como num computador novo
  await page.addInitScript(() => {
    try {
      localStorage.clear();
    } catch {
      /* sem armazenamento */
    }
  });
  for (const [nome, rota, antes] of ECRAS) {
    await page.goto("about:blank");
    await page.goto(BASE + rota);
    await page.waitForTimeout(1800);
    if (antes) await antes(page);
    await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(SAIDA, `${nome}.jpg`), type: "jpeg", quality: 86 });
    console.log(`${nome}.jpg`);
  }
  await browser.close();
})();

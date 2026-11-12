/**
 * Equipamento do posto de venda: visor de cliente (coluna VFD ou segundo ecrã), logótipo para a
 * impressora térmica e ecrã inteiro.
 */
import { numero } from "./format";

const TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// ------------------------------------------------------------------ visor de cliente

export interface LinhaVisor {
  descricao: string;
  quantidade: number;
  /** total da linha com IVA, em cêntimos */
  total: number;
}

/** O que o cliente vê: «livre» (à espera), «venda» (a registar), «pagar» (no pagamento) e «fim» (troco). */
export interface EstadoVisor {
  fase: "livre" | "venda" | "pagar" | "fim";
  empresa: { nome: string; logo: string | null };
  mensagem1: string;
  mensagem2: string;
  linhas: LinhaVisor[];
  /** linha que acabou de mudar (o artigo repetido soma à linha que já existe) */
  ultima: LinhaVisor | null;
  ativa?: number | null;
  total: number;
  itens: number;
  desconto: number;
  pago?: number;
  troco?: number;
  documento?: string;
}

const CANAL = "kigest-visor-cliente";
let canal: BroadcastChannel | null = null;

/** Envia o estado para o segundo ecrã (janela «visor» na aplicação; BroadcastChannel no browser). */
export async function publicarVisor(estado: EstadoVisor) {
  try {
    canal ??= new BroadcastChannel(CANAL);
    canal.postMessage(estado);
  } catch {
    /* sem BroadcastChannel */
  }
  if (TAURI) {
    try {
      const { emitTo } = await import("@tauri-apps/api/event");
      await emitTo("visor", "visor-cliente", estado);
    } catch {
      /* a janela do visor ainda não existe */
    }
  }
}

/** Recebe o estado no ecrã do cliente. Devolve a função para deixar de ouvir. */
export function ouvirVisor(fn: (e: EstadoVisor) => void): () => void {
  let fim: (() => void) | undefined;
  let ativo = true;
  let bc: BroadcastChannel | null = null;
  try {
    bc = new BroadcastChannel(CANAL);
    bc.onmessage = (m) => fn(m.data as EstadoVisor);
  } catch {
    /* sem BroadcastChannel */
  }
  if (TAURI) {
    import("@tauri-apps/api/event")
      .then(({ listen, emitTo }) =>
        listen<EstadoVisor>("visor-cliente", (ev) => fn(ev.payload)).then((u) => {
          if (ativo) fim = u;
          else u();
          // pede o estado actual ao POS (a janela pode abrir a meio de uma venda)
          emitTo("main", "visor-pronto", null).catch(() => {});
        }),
      )
      .catch(() => {});
  }
  return () => {
    ativo = false;
    fim?.();
    bc?.close();
  };
}

/** No POS: chama `fn` quando o ecrã do cliente acaba de abrir e precisa do estado actual. */
export function ouvirVisorPronto(fn: () => void): () => void {
  if (!TAURI) return () => {};
  let fim: (() => void) | undefined;
  let ativo = true;
  import("@tauri-apps/api/event")
    .then(({ listen }) => listen("visor-pronto", () => fn()))
    .then((u) => (ativo ? (fim = u) : u()))
    .catch(() => {});
  return () => {
    ativo = false;
    fim?.();
  };
}

/**
 * Abre o ecrã do cliente: no segundo monitor em ecrã inteiro (terminais de ecrã duplo) ou, havendo
 * um só monitor, numa janela normal. No browser abre um separador.
 */
export async function abrirVisorEcra(): Promise<"segundo-monitor" | "janela" | "separador" | "ja-aberto"> {
  if (!TAURI) {
    window.open(`${location.pathname}#/visor-cliente`, "kigest-visor");
    return "separador";
  }
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const { availableMonitors, currentMonitor } = await import("@tauri-apps/api/window");
  const existente = await WebviewWindow.getByLabel("visor");
  if (existente) {
    await existente.show().catch(() => {});
    return "ja-aberto";
  }
  const [monitores, atual] = await Promise.all([availableMonitors(), currentMonitor()]);
  const outro = monitores.find((m) => !atual || m.position.x !== atual.position.x || m.position.y !== atual.position.y) ?? null;
  const escala = outro?.scaleFactor ?? 1;
  const w = new WebviewWindow("visor", {
    url: "index.html#/visor-cliente",
    title: "KanGest — Ecrã do cliente",
    width: outro ? outro.size.width / escala : 800,
    height: outro ? outro.size.height / escala : 600,
    x: outro ? outro.position.x / escala : undefined,
    y: outro ? outro.position.y / escala : undefined,
    decorations: !outro,
    focus: false,
    skipTaskbar: !!outro,
  });
  await new Promise<void>((ok, falha) => {
    w.once("tauri://created", () => ok());
    w.once("tauri://error", (e) => falha(new Error(String(e.payload))));
  });
  if (outro) await w.setFullscreenOnMonitor(outro.position).catch(() => w.setFullscreen(true));
  return outro ? "segundo-monitor" : "janela";
}

/** Formato dos valores para o visor de coluna (só ASCII: espaços normais e «-»). */
const kzVisor = (c: number) => numero(c / 100, 2, 2).replace(/ /g, " ").replace("−", "-");

/** Linha com texto à esquerda e valor à direita, na largura do visor. */
function par(esq: string, dir: string, cols: number) {
  const d = dir.slice(0, cols);
  const e = esq.slice(0, Math.max(0, cols - d.length - 1));
  return e + " ".repeat(Math.max(1, cols - e.length - d.length)) + d;
}

/** As duas linhas do visor de coluna (VFD 2 × 20) para cada fase da venda. */
export function linhasVisorColuna(e: EstadoVisor, cols = 20): [string, string] {
  switch (e.fase) {
    case "venda": {
      const u = e.ultima;
      const l1 = u ? par(u.quantidade !== 1 ? `${numero(u.quantidade, 3).replace(/ /g, " ")}x ${u.descricao}` : u.descricao, kzVisor(u.total), cols) : "";
      return [l1, par("TOTAL", kzVisor(e.total), cols)];
    }
    case "pagar":
      return ["TOTAL A PAGAR", par("Kz", kzVisor(e.total), cols)];
    case "fim":
      return e.troco && e.troco > 0 ? [par("TROCO", kzVisor(e.troco), cols), "Obrigado! Volte sempre"] : ["Obrigado!", "Volte sempre"];
    default:
      return [e.mensagem1 || e.empresa.nome, e.mensagem2 || "Bem-vindo!"];
  }
}

// ------------------------------------------------------------------ logótipo na impressora térmica

export interface RasterLogo {
  bytes_linha: number;
  altura: number;
  dados: string;
}

/**
 * Converte o logótipo (PNG/JPEG em data URL) para a imagem de 1 bit que a impressora térmica imprime
 * (`GS v 0`): largura total do papel (576 pontos em 80 mm, 384 em 58 mm), logótipo centrado a 60%,
 * no máximo 160 pontos de altura, cinzentos por difusão de erro (Floyd–Steinberg).
 */
export async function logoParaRaster(src: string, colunas: number): Promise<{ raster: RasterLogo; previa: string }> {
  const img = new Image();
  img.src = src;
  await img.decode();
  const papel = colunas <= 32 ? 384 : 576;
  let w = Math.round(papel * 0.6);
  let h = Math.max(1, Math.round((w * img.naturalHeight) / Math.max(1, img.naturalWidth)));
  if (h > 160) {
    h = 160;
    w = Math.max(1, Math.round((h * img.naturalWidth) / Math.max(1, img.naturalHeight)));
  }
  const cv = document.createElement("canvas");
  cv.width = papel;
  cv.height = h;
  const ctx = cv.getContext("2d")!;
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, papel, h);
  ctx.drawImage(img, Math.round((papel - w) / 2), 0, w, h);
  const px = ctx.getImageData(0, 0, papel, h).data;
  const g = new Float32Array(papel * h);
  for (let i = 0; i < papel * h; i++) {
    g[i] = 0.299 * px[i * 4] + 0.587 * px[i * 4 + 1] + 0.114 * px[i * 4 + 2];
  }
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < papel; x++) {
      const i = y * papel + x;
      const novo = g[i] < 128 ? 0 : 255;
      const erro = g[i] - novo;
      g[i] = novo;
      if (x + 1 < papel) g[i + 1] += (erro * 7) / 16;
      if (y + 1 < h) {
        if (x > 0) g[i + papel - 1] += (erro * 3) / 16;
        g[i + papel] += (erro * 5) / 16;
        if (x + 1 < papel) g[i + papel + 1] += erro / 16;
      }
    }
  }
  const bl = papel / 8;
  const dados = new Uint8Array(bl * h);
  const out = ctx.createImageData(papel, h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < papel; x++) {
      const preto = g[y * papel + x] === 0;
      if (preto) dados[y * bl + (x >> 3)] |= 0x80 >> (x & 7);
      const o = (y * papel + x) * 4;
      out.data[o] = out.data[o + 1] = out.data[o + 2] = preto ? 0 : 255;
      out.data[o + 3] = 255;
    }
  }
  ctx.putImageData(out, 0, 0);
  let bin = "";
  for (let i = 0; i < dados.length; i += 0x8000) bin += String.fromCharCode(...dados.subarray(i, i + 0x8000));
  return { raster: { bytes_linha: bl, altura: h, dados: btoa(bin) }, previa: cv.toDataURL("image/png") };
}

// ------------------------------------------------------------------ ecrã inteiro

export async function ecraInteiro(ligar?: boolean): Promise<boolean> {
  if (TAURI) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const j = getCurrentWindow();
    const alvo = ligar ?? !(await j.isFullscreen());
    await j.setFullscreen(alvo);
    return alvo;
  }
  const alvo = ligar ?? !document.fullscreenElement;
  if (alvo && !document.fullscreenElement) await document.documentElement.requestFullscreen().catch(() => {});
  if (!alvo && document.fullscreenElement) await document.exitFullscreen().catch(() => {});
  return alvo;
}

import { useQuery } from "@tanstack/react-query";
import { call } from "./api";
import type { Artigo } from "./types";

/**
 * Fotografias dos artigos: reduzidas aqui (o servidor só guarda) — miniatura quadrada de 160 px para a grelha do POS
 * e imagem até 800 px para a ficha e a loja online (API). WebP quando o browser sabe gerar; senão JPEG.
 */

async function carregar(ficheiro: Blob): Promise<HTMLImageElement> {
  const url = URL.createObjectURL(ficheiro);
  try {
    const img = new Image();
    img.decoding = "async";
    await new Promise<void>((ok, erro) => {
      img.onload = () => ok();
      img.onerror = () => erro(new Error("Não foi possível ler a imagem."));
      img.src = url;
    });
    return img;
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
}

function paraDataUrl(c: HTMLCanvasElement, qualidade: number): string {
  const w = c.toDataURL("image/webp", qualidade);
  // o Safari antigo devolve PNG quando não sabe WebP: aí vai JPEG (bem mais pequeno)
  return w.startsWith("data:image/webp") ? w : c.toDataURL("image/jpeg", qualidade);
}

/** Lê um ficheiro de imagem (ou foto da câmara) e devolve a miniatura e a imagem prontas a guardar. */
export async function prepararFoto(ficheiro: File): Promise<{ miniatura: string; imagem: string }> {
  if (!ficheiro.type.startsWith("image/")) throw new Error("Escolha uma imagem (JPEG, PNG, WebP…).");
  const img = await carregar(ficheiro);
  const { naturalWidth: w, naturalHeight: h } = img;
  if (!w || !h) throw new Error("Imagem vazia.");
  // imagem: cabe em 800×800, sem deformar
  const k = Math.min(1, 800 / Math.max(w, h));
  const grande = document.createElement("canvas");
  grande.width = Math.round(w * k);
  grande.height = Math.round(h * k);
  const g = grande.getContext("2d")!;
  g.fillStyle = "#fff"; // PNG com transparência: fundo branco (o JPEG não tem transparência)
  g.fillRect(0, 0, grande.width, grande.height);
  g.drawImage(img, 0, 0, grande.width, grande.height);
  // miniatura: quadrado de 160 px, recortado ao centro
  const lado = Math.min(w, h);
  const mini = document.createElement("canvas");
  mini.width = mini.height = 160;
  const m = mini.getContext("2d")!;
  m.fillStyle = "#fff";
  m.fillRect(0, 0, 160, 160);
  m.drawImage(img, (w - lado) / 2, (h - lado) / 2, lado, lado, 0, 0, 160, 160);
  let imagem = paraDataUrl(grande, 0.82);
  if (imagem.length > 780_000) imagem = paraDataUrl(grande, 0.6);
  return { miniatura: paraDataUrl(mini, 0.75), imagem };
}

// miniaturas já recebidas, por artigo e versão da foto (a versão muda quando a foto muda)
const cache = new Map<string, string>();
const chave = (a: Pick<Artigo, "id" | "foto">) => `${a.id}@${a.foto}`;

/** Miniaturas dos artigos com foto (pede ao servidor só as que ainda não tem). */
export function useMiniaturas(artigos: Pick<Artigo, "id" | "foto">[] | undefined): Record<number, string> {
  const comFoto = (artigos ?? []).filter((a) => !!a.foto);
  const q = useQuery({
    queryKey: ["miniaturas", comFoto.map(chave).join(",")],
    enabled: comFoto.length > 0,
    staleTime: Infinity,
    queryFn: async () => {
      const faltam = comFoto.filter((a) => !cache.has(chave(a)));
      if (faltam.length) {
        const r = await call<Record<string, string>>("fotos.miniaturas", { ids: faltam.map((a) => a.id) });
        for (const a of faltam) if (r[a.id]) cache.set(chave(a), r[a.id]);
      }
      const out: Record<number, string> = {};
      for (const a of comFoto) {
        const v = cache.get(chave(a));
        if (v) out[a.id] = v;
      }
      return out;
    },
  });
  return q.data ?? {};
}

import { useEffect, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ImagePlus, Palette, RotateCcw, Image as Imagem } from "lucide-react";
import { call } from "@/lib/api";
import { useAparencia } from "@/lib/aparencia";
import type { Cores } from "@/lib/cores";
import { Button, Card } from "@/components/ui";
import { EscolherCores } from "@/components/EscolherCores";
import entradaImg from "@/assets/acesso/entrada.jpg";

/** Lê uma imagem e reduz para no máximo 1600 px (JPEG), para o ecrã de entrada. */
function lerImagem(ficheiro: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(new Error("Não foi possível ler a imagem."));
    r.onload = () => {
      const img = new Image();
      img.onerror = () => reject(new Error("Formato de imagem não suportado."));
      img.onload = () => {
        const escala = Math.min(1, 1600 / Math.max(img.width, img.height));
        const c = document.createElement("canvas");
        c.width = Math.round(img.width * escala);
        c.height = Math.round(img.height * escala);
        c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
        resolve(c.toDataURL("image/jpeg", 0.85));
      };
      img.src = r.result as string;
    };
    r.readAsDataURL(ficheiro);
  });
}

/**
 * Configurações → Aparência: o jogo de cores do sistema (para todos os postos e browsers da empresa; o tema claro ou
 * escuro continua a ser de cada um) e a imagem do ecrã de entrada.
 */
export default function AparenciaConfig() {
  const qc = useQueryClient();
  const cores = useAparencia((a) => a.cores) ?? { paleta: "petroleo" };
  const imagem = useQuery({
    queryKey: ["aparencia.imagemEntrada"],
    queryFn: () => call<{ imagem: string | null }>("aparencia.imagemEntrada"),
  });
  const anteriores = useRef<Cores>(cores);
  const espera = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(espera.current), []);

  // vê-se logo; grava-se quando se pára de mexer (as cores personalizadas mudam enquanto se arrasta)
  function escolher(c: Cores) {
    useAparencia.getState().setCores(c);
    window.clearTimeout(espera.current);
    espera.current = window.setTimeout(async () => {
      try {
        await call("aparencia.guardar", { cores: c });
        anteriores.current = c;
        qc.invalidateQueries({ queryKey: ["setup"] });
        toast.success("Cores guardadas para todos os postos.");
      } catch (e) {
        useAparencia.getState().setCores(anteriores.current);
        toast.error((e as Error).message);
      }
    }, 600);
  }

  async function guardarImagem(valor: string) {
    try {
      await call("aparencia.guardar", { imagem_entrada: valor });
      qc.invalidateQueries({ queryKey: ["aparencia.imagemEntrada"] });
      qc.invalidateQueries({ queryKey: ["setup"] });
      toast.success(valor ? "Imagem do ecrã de entrada guardada." : "Voltou a imagem do KanGest.");
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const propria = imagem.data?.imagem;
  return (
    <div className="max-w-4xl space-y-5">
      <Card className="p-6">
        <div className="mb-1 flex items-center gap-2 font-semibold">
          <Palette className="size-5 text-brand" /> Cores do sistema
        </div>
        <p className="mb-4 text-[13px] text-ink-3">
          O jogo de cores do menu, dos botões e do ecrã de entrada, igual em todos os postos e browsers da empresa. O tema claro ou escuro
          continua a ser de cada um (no menu do utilizador).
        </p>
        <EscolherCores valor={cores} onChange={escolher} />
      </Card>

      <Card className="p-6">
        <div className="mb-1 flex items-center gap-2 font-semibold">
          <Imagem className="size-5 text-brand" /> Imagem do ecrã de entrada
        </div>
        <p className="mb-4 text-[13px] text-ink-3">
          Aparece à esquerda do ecrã de entrada, nos computadores e nos browsers. Use uma fotografia vossa (com autorização das pessoas que
          aparecem) ou uma de uso livre; sem ela, fica a do KanGest.
        </p>
        <div className="grid gap-5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
          <div className="aspect-[16/10] overflow-hidden rounded-lg bg-sunken ring-1 ring-line">
            <img src={propria || entradaImg} alt="Imagem do ecrã de entrada" className="size-full object-cover" style={{ objectPosition: propria ? "50% 35%" : "68% 100%" }} />
          </div>
          <div className="flex flex-col gap-2">
            <label className="kg-btn inline-flex cursor-pointer items-center justify-center gap-2 rounded-lg border border-line-strong bg-surface px-4 py-2 font-medium hover:bg-sunken">
              <ImagePlus className="size-4" /> Carregar imagem…
              <input
                type="file"
                accept="image/jpeg,image/png,image/webp"
                className="hidden"
                onChange={async (e) => {
                  const f = e.target.files?.[0];
                  e.target.value = "";
                  if (!f) return;
                  try {
                    await guardarImagem(await lerImagem(f));
                  } catch (err) {
                    toast.error((err as Error).message);
                  }
                }}
              />
            </label>
            {propria && (
              <Button variant="secondary" onClick={() => guardarImagem("")}>
                <RotateCcw className="size-4" /> Usar a do KanGest
              </Button>
            )}
          </div>
        </div>
      </Card>
    </div>
  );
}

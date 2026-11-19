import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Palette } from "lucide-react";
import { call } from "@/lib/api";
import { useAparencia } from "@/lib/aparencia";
import type { Cores } from "@/lib/cores";
import { Card } from "@/components/ui";
import { EscolherCores } from "@/components/EscolherCores";

/**
 * Configurações → Aparência: o jogo de cores do sistema, para todos os postos e browsers da empresa (o tema claro ou
 * escuro continua a ser de cada um). A imagem do ecrã de entrada é a do KanGest e não se troca.
 */
export default function AparenciaConfig() {
  const qc = useQueryClient();
  const cores = useAparencia((a) => a.cores) ?? { paleta: "petroleo" };
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

  return (
    <Card className="max-w-4xl p-6">
      <div className="mb-1 flex items-center gap-2 font-semibold">
        <Palette className="size-5 text-brand" /> Cores do sistema
      </div>
      <p className="mb-4 text-[13px] text-ink-3">
        O jogo de cores do menu, dos botões e do ecrã de entrada, igual em todos os postos e browsers da empresa. O tema claro ou escuro continua a
        ser de cada um (no menu do utilizador).
      </p>
      <EscolherCores valor={cores} onChange={escolher} />
    </Card>
  );
}

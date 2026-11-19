import { Check } from "lucide-react";
import { cn } from "@/components/ui";
import { PALETAS, coresDe, type Cores } from "@/lib/cores";

/** Amostra de um jogo: o menu na cor principal, um botão nela e outro no destaque. */
function Amostra({ marca, destaque }: { marca: string; destaque: string }) {
  return (
    <div className="flex h-14 overflow-hidden rounded-md ring-1 ring-black/10">
      <div className="w-1/3" style={{ background: `color-mix(in srgb, ${marca} 68%, black)` }} />
      <div className="flex flex-1 flex-col justify-center gap-1.5 bg-[#f6f4ef] px-2">
        <span className="h-2.5 w-3/4 rounded-sm" style={{ background: marca }} />
        <span className="h-2.5 w-1/2 rounded-sm" style={{ background: destaque }} />
      </div>
    </div>
  );
}

/**
 * Os jogos de cores do sistema (instalação e Configurações → Aparência): os prontos e o personalizado (duas cores à
 * escolha). As cores muito claras escurecem-se sozinhas o necessário para o texto se ler (src/lib/cores.ts).
 */
export function EscolherCores({ valor, onChange }: { valor: Cores; onChange: (c: Cores) => void }) {
  const personal = valor.paleta === "personalizada";
  const atuais = coresDe(valor);
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {PALETAS.map((p) => {
          const sel = valor.paleta === p.id;
          return (
            <button
              key={p.id}
              type="button"
              onClick={() => onChange({ paleta: p.id })}
              aria-pressed={sel}
              className={cn("rounded-lg border p-2 text-left transition", sel ? "border-brand ring-2 ring-brand/25" : "border-line hover:border-line-strong")}
            >
              <Amostra marca={p.marca} destaque={p.destaque} />
              <span className="mt-2 flex items-center justify-between gap-1 text-[12.5px] font-medium">
                {p.nome}
                {sel && <Check className="size-4 shrink-0 text-brand" />}
              </span>
            </button>
          );
        })}
        <button
          type="button"
          onClick={() => onChange({ paleta: "personalizada", marca: atuais.marca, destaque: atuais.destaque })}
          aria-pressed={personal}
          className={cn("rounded-lg border p-2 text-left transition", personal ? "border-brand ring-2 ring-brand/25" : "border-line hover:border-line-strong")}
        >
          <Amostra marca={personal ? valor.marca || atuais.marca : "#8a9490"} destaque={personal ? valor.destaque || atuais.destaque : "#c9c3b5"} />
          <span className="mt-2 flex items-center justify-between gap-1 text-[12.5px] font-medium">
            Personalizado
            {personal && <Check className="size-4 shrink-0 text-brand" />}
          </span>
        </button>
      </div>
      {personal && (
        <div className="flex flex-wrap gap-6 rounded-lg bg-sunken p-4 text-[13px]">
          <label className="flex items-center gap-3">
            <input
              type="color"
              value={valor.marca || atuais.marca}
              onChange={(e) => onChange({ ...valor, marca: e.target.value })}
              className="h-9 w-14 cursor-pointer rounded border border-line bg-surface"
            />
            <span>
              <b className="block font-medium">Cor principal</b>
              <span className="text-ink-3">menu, botões e ligações</span>
            </span>
          </label>
          <label className="flex items-center gap-3">
            <input
              type="color"
              value={valor.destaque || atuais.destaque}
              onChange={(e) => onChange({ ...valor, destaque: e.target.value })}
              className="h-9 w-14 cursor-pointer rounded border border-line bg-surface"
            />
            <span>
              <b className="block font-medium">Cor de destaque</b>
              <span className="text-ink-3">Abrir POS, Pagar e avisos</span>
            </span>
          </label>
        </div>
      )}
    </div>
  );
}

import { CheckCircle2 } from "lucide-react";
import { RAMOS, ramoPorId, type RamoId } from "@/lib/ramos";
import { cn } from "@/components/ui";

/** Grelha dos ramos de actividade; o escolhido mostra o que o KanGest já faz por esse ramo. */
export function EscolherRamo({ valor, onChange }: { valor: string | null | undefined; onChange: (r: RamoId) => void }) {
  const atual = ramoPorId(valor);
  return (
    <div>
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {RAMOS.map((r) => (
          <button
            key={r.id}
            type="button"
            onClick={() => onChange(r.id)}
            aria-pressed={atual.id === r.id}
            className={cn(
              "flex items-start gap-3 rounded-xl border p-3 text-left transition",
              atual.id === r.id ? "border-brand bg-brand-50/60 ring-2 ring-brand/20" : "border-line hover:border-brand/40 hover:bg-sunken/60",
            )}
          >
            <span className={cn("grid size-9 shrink-0 place-items-center rounded-lg", atual.id === r.id ? "bg-brand text-white" : "bg-sunken text-ink-2")}>
              <r.icone className="size-[18px]" />
            </span>
            <span className="min-w-0">
              <span className="block text-[13.5px] font-medium leading-snug">{r.nome}</span>
              <span className="block text-[12px] leading-snug text-ink-3">{r.descricao}</span>
            </span>
          </button>
        ))}
      </div>
      <ul className="mt-3 grid gap-1 rounded-xl bg-sunken/70 p-3 text-[12.5px] text-ink-2 sm:grid-cols-2">
        {atual.dicas.map((d) => (
          <li key={d} className="flex items-start gap-1.5">
            <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-ok" /> {d}
          </li>
        ))}
        {atual.campos.length > 0 && (
          <li className="flex items-start gap-1.5">
            <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-ok" /> Nos artigos: {atual.campos.map((c) => c.label.toLowerCase()).join(", ")}
          </li>
        )}
      </ul>
    </div>
  );
}

import { Link } from "react-router-dom";
import { AlertTriangle, Clock, KeyRound } from "lucide-react";
import { useLicenca, useSincronizarLicenca } from "@/lib/licenca";
import { cn } from "@/components/ui";

/**
 * Faixa de aviso da licença. Só aparece quando a experiência ou a assinatura está a acabar, já acabou
 * (modo de consulta) ou há um problema (relógio atrasado, licença de outra empresa).
 */
export function AvisoLicenca({ compacto = false, className }: { compacto?: boolean; className?: string }) {
  const { data: e } = useLicenca();
  useSincronizarLicenca();
  if (!e || (e.pode_emitir && !e.aviso)) return null;
  const grave = !e.pode_emitir;
  const Icone = grave ? AlertTriangle : e.modo === "EXPERIENCIA" ? Clock : KeyRound;
  return (
    <div
      role={grave ? "alert" : "status"}
      className={cn(
        "flex flex-wrap items-center gap-x-4 gap-y-1 rounded-xl border px-4 py-2.5 text-[13.5px]",
        grave ? "border-danger/30 bg-danger-50 text-danger" : "border-accent/40 bg-accent-50 text-[#7a4f0a]",
        !compacto && "mb-5",
        className,
      )}
    >
      <Icone className="size-4 shrink-0" />
      <span className="min-w-0 flex-1">{e.mensagem}</span>
      <Link to="/licenca" className="shrink-0 font-semibold underline underline-offset-2">
        {e.modo === "EXPERIENCIA" ? "Activar licença" : grave ? "Resolver" : "Renovar"}
      </Link>
    </div>
  );
}

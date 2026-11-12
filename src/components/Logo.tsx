import { useId } from "react";
import { cn } from "@/components/ui";

/**
 * Logótipo do KanGest: o mesmo desenho do ícone da aplicação (src/assets/kigest-icone.svg),
 * sem a margem e a sombra do ícone, para usar em qualquer tamanho na interface.
 * «K» geométrico em branco com a perna inferior em laranja (a cor do POS) sobre o verde da marca.
 */
export function Logo({ className, titulo = "KanGest" }: { className?: string; titulo?: string }) {
  const id = useId().replace(/:/g, "");
  return (
    <svg viewBox="100 100 824 824" className={cn("shrink-0", className)} role="img" aria-label={titulo}>
      <defs>
        <linearGradient id={`f${id}`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#1c8c80" />
          <stop offset="1" stopColor="#0b433e" />
        </linearGradient>
        <linearGradient id={`b${id}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.16" />
          <stop offset="0.55" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
      </defs>
      <rect x="100" y="100" width="824" height="824" rx="188" fill={`url(#f${id})`} />
      <rect x="100" y="100" width="824" height="824" rx="188" fill={`url(#b${id})`} />
      <g fill="none" strokeLinecap="round" strokeWidth="112" transform="translate(-6 0)">
        <line x1="374" y1="318" x2="374" y2="706" stroke="#f7f4ee" />
        <line x1="500" y1="473" x2="668" y2="706" stroke="#f0a531" />
        <line x1="382" y1="590" x2="662" y2="318" stroke="#f7f4ee" />
      </g>
    </svg>
  );
}

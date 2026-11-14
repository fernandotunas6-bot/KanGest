import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { ArrowDownRight, ArrowUpRight, X, type LucideIcon } from "lucide-react";
import { forwardRef, useEffect, useRef, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TdHTMLAttributes } from "react";
import { numero } from "@/lib/format";

export const cn = (...c: ClassValue[]) => twMerge(clsx(c));

// ---------------- Button ----------------
type Variant = "primary" | "secondary" | "ghost" | "danger" | "accent";
const variants: Record<Variant, string> = {
  primary: "bg-brand text-white hover:bg-brand-600 active:bg-brand-700 shadow-sm",
  secondary: "bg-surface text-ink border border-line-strong hover:bg-sunken",
  ghost: "text-ink-2 hover:bg-sunken hover:text-ink",
  danger: "bg-danger text-white hover:brightness-95",
  accent: "bg-accent text-on-accent hover:brightness-95 shadow-sm",
};
const sizes = { sm: "h-8 px-3 text-[13px] gap-1.5", md: "h-10 px-4 gap-2", lg: "h-12 px-5 text-base gap-2", xl: "h-16 px-6 text-lg gap-3" };

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: keyof typeof sizes; loading?: boolean }
>(({ className, variant = "primary", size = "md", loading, disabled, children, ...p }, ref) => (
  <button
    ref={ref}
    disabled={disabled || loading}
    className={cn(
      "inline-flex items-center justify-center rounded-lg font-medium transition-colors disabled:opacity-50 disabled:pointer-events-none whitespace-nowrap",
      variants[variant],
      sizes[size],
      className,
    )}
    {...p}
  >
    {loading && <span className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" />}
    {children}
  </button>
));

// ---------------- Inputs ----------------
const inputBase =
  "h-10 w-full rounded-lg border border-line-strong bg-surface px-3 text-ink placeholder:text-ink-3 outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/15 disabled:bg-sunken";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) => (
  <input ref={ref} className={cn(inputBase, className)} {...p} />
));

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(({ className, children, ...p }, ref) => (
  <select ref={ref} className={cn(inputBase, "select-kg pr-8", className)} {...p}>
    {children}
  </select>
));

export function Field({ label, hint, children, className }: { label: string; hint?: string; children: ReactNode; className?: string }) {
  return (
    <label className={cn("flex flex-col gap-1.5", className)}>
      <span className="text-[12px] font-medium uppercase tracking-wide text-ink-2">{label}</span>
      {children}
      {hint && <span className="text-[12px] text-ink-3">{hint}</span>}
    </label>
  );
}

// ---------------- Card / Badge ----------------
export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn("rounded-xl border border-line bg-surface shadow-card", className)}>{children}</div>;
}

const tones = {
  neutral: "bg-sunken text-ink-2",
  brand: "bg-brand-50 text-brand-700",
  ok: "bg-ok-50 text-ok",
  danger: "bg-danger-50 text-danger",
  accent: "bg-accent-50 text-accent-ink",
};
export function Badge({ tone = "neutral", children, className }: { tone?: keyof typeof tones; children: ReactNode; className?: string }) {
  return <span className={cn("inline-flex items-center whitespace-nowrap rounded-md px-2 py-0.5 text-[12px] font-medium", tones[tone], className)}>{children}</span>;
}

// ---------------- Page header ----------------
export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4 pb-5">
      <div>
        <h1 className="text-[22px] font-semibold tracking-tight text-ink">{title}</h1>
        {subtitle && <p className="mt-0.5 text-ink-3">{subtitle}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}

// ---------------- Modal ----------------
export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  width = "max-w-2xl",
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-scrim p-6 pt-[6vh] backdrop-blur-[2px]" onMouseDown={onClose}>
      <div role="dialog" aria-modal="true" aria-label={title} className={cn("w-full rounded-2xl border border-line bg-surface shadow-pop", width)} onMouseDown={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-line px-6 py-4">
          <h2 className="text-[17px] font-semibold">{title}</h2>
          <button onClick={onClose} className="rounded-md p-1 text-ink-3 hover:bg-sunken hover:text-ink" aria-label="Fechar">
            <X className="size-5" />
          </button>
        </div>
        <div className="px-6 py-5">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-line bg-bg/60 px-6 py-4 rounded-b-2xl">{footer}</div>}
      </div>
    </div>
  );
}

// ---------------- Table ----------------
export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("overflow-auto", className)}>
      <table className="w-full border-collapse text-left">{children}</table>
    </div>
  );
}
export function Th({ children, className }: { children?: ReactNode; className?: string }) {
  return (
    <th className={cn("sticky top-0 z-[1] border-b border-line bg-surface px-4 py-[var(--kg-cab-y)] text-[12px] font-medium uppercase tracking-wide text-ink-3", className)}>
      {children}
    </th>
  );
}
export function Td({ children, className, ...rest }: TdHTMLAttributes<HTMLTableCellElement>) {
  return (
    <td {...rest} className={cn("border-b border-line/70 px-4 py-[var(--kg-celula-y)]", className)}>
      {children}
    </td>
  );
}

export function Empty({ icon, title, text, action }: { icon: ReactNode; title: string; text?: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center">
      <div className="mb-1 rounded-2xl bg-brand-50 p-3 text-brand">{icon}</div>
      <p className="font-medium text-ink">{title}</p>
      {text && <p className="max-w-sm text-ink-3">{text}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function Stat({
  label,
  value,
  hint,
  icon,
  tone = "brand",
  delta,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  icon?: ReactNode;
  tone?: "brand" | "accent" | "danger" | "ok";
  /** variação face ao período anterior (ver Delta) */
  delta?: ReactNode;
}) {
  const t = { brand: "bg-brand-50 text-brand", accent: "bg-accent-50 text-accent-ink", danger: "bg-danger-50 text-danger", ok: "bg-ok-50 text-ok" }[tone];
  return (
    <Card className="p-5">
      <div className="flex items-start justify-between">
        <span className="text-[13px] text-ink-2">{label}</span>
        {icon && <span className={cn("rounded-lg p-1.5", t)}>{icon}</span>}
      </div>
      <div className="mt-2 flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="text-[24px] font-semibold tracking-tight">{value}</span>
        {delta}
      </div>
      {hint && <div className="mt-1 text-[12px] text-ink-3">{hint}</div>}
    </Card>
  );
}

/**
 * Variação de um valor face a um período anterior com nome («vs ontem»): seta e percentagem,
 * verde quando sobe e vermelho quando desce (nunca só a cor). Sem valor anterior não se mostra.
 */
export function Delta({ atual, anterior, periodo }: { atual: number; anterior: number; periodo: string }) {
  if (!anterior) return null;
  const v = ((atual - anterior) / Math.abs(anterior)) * 100;
  const sobe = v >= 0;
  const Icon = sobe ? ArrowUpRight : ArrowDownRight;
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className={cn("num inline-flex items-center gap-0.5 rounded-md px-1.5 py-0.5 text-[11.5px] font-medium", sobe ? "bg-ok-50 text-ok" : "bg-danger-50 text-danger")}>
        <Icon className="size-3.5" aria-label={sobe ? "subiu" : "desceu"} />
        {numero(Math.abs(v), 1)}%
      </span>
      <span className="text-[12px] text-ink-3">{periodo}</span>
    </span>
  );
}

// ---------------- Menus suspensos ----------------
/** Botão com um menu suspenso; fecha ao clicar fora ou com Esc. */
export function Popover({
  botao,
  children,
  alinhar = "direita",
  className,
}: {
  botao: (p: { aberto: boolean; alternar: () => void }) => ReactNode;
  children: (fechar: () => void) => ReactNode;
  alinhar?: "direita" | "esquerda";
  className?: string;
}) {
  const [aberto, setAberto] = useState(false);
  const raiz = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!aberto) return;
    const fora = (e: MouseEvent) => {
      if (!raiz.current?.contains(e.target as Node)) setAberto(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setAberto(false);
    document.addEventListener("mousedown", fora);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", fora);
      document.removeEventListener("keydown", esc);
    };
  }, [aberto]);
  return (
    <div ref={raiz} className="relative">
      {botao({ aberto, alternar: () => setAberto((a) => !a) })}
      {aberto && (
        <div className={cn("absolute top-full z-40 mt-1.5 rounded-xl border border-line bg-surface p-1.5 text-ink shadow-pop", alinhar === "direita" ? "right-0" : "left-0", className)}>
          {children(() => setAberto(false))}
        </div>
      )}
    </div>
  );
}

export function MenuItem({ icon: Icon, children, onClick, perigo, atalho }: { icon?: LucideIcon; children: ReactNode; onClick: () => void; perigo?: boolean; atalho?: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn("flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[13.5px] transition hover:bg-sunken", perigo ? "text-danger" : "text-ink")}
    >
      {Icon && <Icon className={cn("size-4 shrink-0", perigo ? "" : "text-ink-3")} />}
      <span className="flex-1">{children}</span>
      {atalho && <kbd>{atalho}</kbd>}
    </button>
  );
}

/** Escolha entre poucas opções lado a lado (ex.: tema Claro / Escuro / Sistema). */
export function Segmentado<T extends string>({
  valor,
  opcoes,
  onChange,
  className,
}: {
  valor: T;
  opcoes: { id: T; label: string; icon?: LucideIcon }[];
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <div className={cn("flex rounded-lg bg-sunken p-0.5", className)}>
      {opcoes.map((o) => (
        <button
          key={o.id}
          type="button"
          aria-pressed={valor === o.id}
          onClick={() => onChange(o.id)}
          className={cn(
            "inline-flex flex-1 items-center justify-center gap-1.5 rounded-md px-2.5 py-1 text-[12.5px] transition",
            valor === o.id ? "bg-surface font-medium text-ink shadow-sm ring-1 ring-line" : "text-ink-2 hover:text-ink",
          )}
        >
          {o.icon && <o.icon className="size-3.5" />}
          {o.label}
        </button>
      ))}
    </div>
  );
}

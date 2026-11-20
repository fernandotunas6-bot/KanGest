// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { ArrowDown, ArrowDownRight, ArrowUp, ArrowUpRight, ChevronsUpDown, X, type LucideIcon } from "lucide-react";
import { createContext, forwardRef, useCallback, useContext, useEffect, useMemo, useRef, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TdHTMLAttributes } from "react";
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
      // os ícones não encolhem quando o botão é estreito (o texto é que se ajusta)
      "kg-btn inline-flex items-center justify-center rounded-lg font-medium transition-colors disabled:opacity-50 disabled:pointer-events-none whitespace-nowrap [&>svg]:shrink-0",
      size === "sm" && "kg-btn-sm",
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
  "kg-campo h-10 w-full rounded-lg border border-line-strong bg-surface px-3 text-ink placeholder:text-ink-3 outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/15 disabled:bg-sunken";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...p }, ref) => (
  <input ref={ref} className={cn(inputBase, className)} {...p} />
));

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(({ className, children, ...p }, ref) => (
  <select ref={ref} className={cn(inputBase, "select-kg pr-8", className)} {...p}>
    {children}
  </select>
));

/** Campo com rótulo. `erro` aparece a vermelho no lugar da dica (e o campo fica com a borda vermelha). */
export function Field({ label, hint, erro, aviso, children, className }: { label: string; hint?: string; erro?: string | null; aviso?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cn("flex flex-col gap-1.5", erro && "[&_.kg-campo]:border-danger [&_.kg-campo]:focus:ring-danger/15", className)}>
      <span className="text-[13px] font-medium text-ink-2">{label}</span>
      {children}
      {erro ? (
        <span role="alert" className="text-[12px] font-medium text-danger">
          {erro}
        </span>
      ) : aviso ? (
        <span className="text-[12px] font-medium text-accent-ink">{aviso}</span>
      ) : (
        hint && <span className="text-[12px] text-ink-3">{hint}</span>
      )}
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
        <h1 className="text-[20px] font-semibold tracking-tight text-ink sm:text-[22px]">{title}</h1>
        {subtitle && <p className="mt-0.5 text-ink-3">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

// ---------------- Modal ----------------
/** Janelas abertas, da mais antiga para a mais recente: só a de cima responde ao Esc e ao Ctrl+S. */
const pilhaModais: number[] = [];
let proximoModal = 1;

/** Dentro de um Modal: fechar como o «X» (com o aviso das alterações por guardar). */
const FecharModal = createContext<() => void>(() => {});
export const usePedirFecho = () => useContext(FecharModal);

/** «Cancelar» de um Modal: avisa se houver alterações por guardar. */
export function BotaoCancelar({ children = "Cancelar" }: { children?: ReactNode }) {
  const fechar = usePedirFecho();
  return (
    <Button variant="secondary" onClick={fechar}>
      {children}
    </Button>
  );
}

/**
 * Janela por cima do ecrã. Com `sujo`, fechar (Esc, «X», fora da janela ou BotaoCancelar) pergunta antes de deitar fora
 * as alterações. Com `onGuardar`, Ctrl+S (⌘S no Mac) guarda.
 */
export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  width = "max-w-2xl",
  sujo,
  onGuardar,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
  /** há alterações por guardar */
  sujo?: boolean;
  onGuardar?: () => void;
}) {
  const [aSair, setASair] = useState(false);
  const pedirFecho = useCallback(() => (sujo ? setASair(true) : onClose()), [sujo, onClose]);
  const guardarRef = useRef(onGuardar);
  guardarRef.current = onGuardar;
  const [eu] = useState(() => proximoModal++);
  useEffect(() => {
    if (!open) return;
    pilhaModais.push(eu);
    return () => {
      const i = pilhaModais.lastIndexOf(eu);
      if (i >= 0) pilhaModais.splice(i, 1);
    };
  }, [open, eu]);
  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => {
      // uma janela por cima desta (ex.: uma confirmação) trata as suas teclas
      if (pilhaModais[pilhaModais.length - 1] !== eu) return;
      if (e.key === "Escape") {
        e.preventDefault();
        if (aSair) setASair(false);
        else pedirFecho();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s" && guardarRef.current) {
        e.preventDefault();
        setASair(false);
        guardarRef.current();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [open, pedirFecho, aSair, eu]);
  useEffect(() => {
    if (!sujo) setASair(false);
  }, [sujo]);
  if (!open) return null;
  return (
    <FecharModal.Provider value={pedirFecho}>
      <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-scrim p-2 pt-3 backdrop-blur-[2px] sm:p-6 sm:pt-[6vh]" onMouseDown={pedirFecho}>
        <div role="dialog" aria-modal="true" aria-label={title} className={cn("w-full rounded-2xl border border-line bg-surface shadow-pop", width)} onMouseDown={(e) => e.stopPropagation()}>
          <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3.5 sm:px-6 sm:py-4">
            <h2 className="min-w-0 text-[17px] font-semibold">{title}</h2>
            <button onClick={pedirFecho} className="rounded-md p-1 text-ink-3 hover:bg-sunken hover:text-ink" aria-label="Fechar">
              <X className="size-5" />
            </button>
          </div>
          <div className="px-4 py-4 sm:px-6 sm:py-5">{children}</div>
          {aSair ? (
            // fica no fundo do ecrã mesmo numa ficha comprida (o resto da janela pode estar fora de vista)
            <div
              role="alertdialog"
              aria-label="Alterações por guardar"
              className="sticky bottom-0 z-10 flex flex-wrap items-center justify-end gap-2 rounded-b-2xl border-t border-accent/40 bg-accent-50 px-4 py-3.5 shadow-pop sm:px-6 sm:py-4"
            >
              <span className="mr-auto text-[13.5px] font-medium text-accent-ink">Há alterações por guardar. Sair sem as guardar?</span>
              <Button variant="secondary" autoFocus onClick={() => setASair(false)}>
                Continuar a editar
              </Button>
              <Button variant="danger" onClick={onClose}>
                Sair sem guardar
              </Button>
              {onGuardar && (
                <Button
                  onClick={() => {
                    setASair(false);
                    onGuardar();
                  }}
                >
                  Guardar
                </Button>
              )}
            </div>
          ) : (
            footer && <div className="flex flex-wrap justify-end gap-2 rounded-b-2xl border-t border-line bg-bg/60 px-4 py-3.5 sm:px-6 sm:py-4">{footer}</div>
          )}
        </div>
      </div>
    </FecharModal.Provider>
  );
}

/** Pergunta antes de uma acção que não se desfaz (eliminar, desactivar…). */
export function Confirmar({
  titulo,
  children,
  confirmar,
  perigo = true,
  onConfirmar,
  onClose,
}: {
  titulo: string;
  children: ReactNode;
  confirmar: string;
  perigo?: boolean;
  onConfirmar: () => Promise<unknown> | void;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      onClose={onClose}
      title={titulo}
      width="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Voltar
          </Button>
          <Button
            variant={perigo ? "danger" : "primary"}
            loading={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await onConfirmar();
              } finally {
                setBusy(false);
              }
            }}
          >
            {confirmar}
          </Button>
        </>
      }
    >
      <div className="space-y-2 text-[14px] text-ink-2">{children}</div>
    </Modal>
  );
}

/** Atalho de teclado mostrado num botão («Ctrl S» / «⌘S»). */
export function Atalho({ tecla }: { tecla: string }) {
  const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/i.test(navigator.userAgent);
  return <kbd className="ml-1 hidden rounded border border-white/30 px-1 font-sans text-[11px] font-normal opacity-80 sm:inline">{mac ? `⌘${tecla}` : `Ctrl ${tecla}`}</kbd>;
}

// ---------------- Table ----------------
export function Table({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className={cn("overflow-auto", className)}>
      <table className="w-full border-collapse text-left">{children}</table>
    </div>
  );
}
export function Th({
  children,
  className,
  direccao,
  onOrdenar,
}: {
  children?: ReactNode;
  className?: string;
  /** coluna ordenável (ver useOrdenacao): sentido actual e o clique que muda a ordem */
  direccao?: Direccao | null;
  onOrdenar?: () => void;
}) {
  const Seta = direccao === "asc" ? ArrowUp : direccao === "desc" ? ArrowDown : ChevronsUpDown;
  return (
    <th
      aria-sort={direccao === "asc" ? "ascending" : direccao === "desc" ? "descending" : undefined}
      className={cn("sticky top-0 z-[1] border-b border-line bg-surface px-4 py-[var(--kg-cab-y)] text-[12px] font-medium uppercase tracking-wide text-ink-3", className)}
    >
      {onOrdenar ? (
        <button
          type="button"
          onClick={onOrdenar}
          title="Ordenar por esta coluna"
          className={cn("group/ord inline-flex items-center gap-1 uppercase tracking-wide transition hover:text-ink", direccao && "text-ink")}
        >
          {children}
          <Seta className={cn("size-3.5 shrink-0 print:hidden", direccao ? "text-brand" : "opacity-0 group-hover/ord:opacity-60")} />
        </button>
      ) : (
        children
      )}
    </th>
  );
}

export type Direccao = "asc" | "desc";
const colador = new Intl.Collator("pt", { sensitivity: "base", numeric: true });

/**
 * Ordenar uma lista pelo clique nos cabeçalhos (1.º clique crescente, 2.º decrescente, 3.º volta à ordem original).
 * Os vazios ficam sempre no fim; o texto compara sem acentos e com os números no sítio («FT 2026/9» antes de «/10»).
 *   const { ordenados, cab } = useOrdenacao(lista, (x, k) => …);   <Th {...cab("total")}>Total</Th>
 */
export function useOrdenacao<T, K extends string>(lista: T[] | undefined, valor: (x: T, chave: K) => string | number | null | undefined) {
  const [ordem, setOrdem] = useState<{ chave: K; dir: Direccao } | null>(null);
  const ordenados = useMemo(() => {
    if (!lista || !ordem) return lista;
    const f = ordem.dir === "asc" ? 1 : -1;
    const vazio = (v: unknown) => v === null || v === undefined || v === "";
    return [...lista].sort((a, b) => {
      const va = valor(a, ordem.chave);
      const vb = valor(b, ordem.chave);
      if (vazio(va) || vazio(vb)) return vazio(va) === vazio(vb) ? 0 : vazio(va) ? 1 : -1;
      return (typeof va === "number" && typeof vb === "number" ? va - vb : colador.compare(String(va), String(vb))) * f;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lista, ordem]);
  const cab = (chave: K) => ({
    direccao: ordem?.chave === chave ? ordem.dir : null,
    onOrdenar: () => setOrdem((o) => (!o || o.chave !== chave ? { chave, dir: "asc" } : o.dir === "asc" ? { chave, dir: "desc" } : null)),
  });
  return { ordenados, cab };
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
    <Card className="min-w-0 p-3.5 sm:p-5">
      <div className="flex items-start justify-between gap-2">
        <span className="text-[13px] text-ink-2">{label}</span>
        {icon && <span className={cn("rounded-lg p-1.5", t)}>{icon}</span>}
      </div>
      <div className="mt-2 flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="break-words text-[18px] font-semibold tracking-tight sm:text-[24px]">{value}</span>
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

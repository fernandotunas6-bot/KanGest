import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { hojeISO } from "@/lib/format";
import { cn } from "./ui";

/** O mesmo formato de evento de uma caixa de texto: os ecrãs continuam a ler `e.target.value`. */
type Evento = { target: { value: string } };

const MESES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
const DIAS_SEMANA = ["S", "T", "Q", "Q", "S", "S", "D"]; // a semana começa à segunda-feira

const dois = (n: number) => String(n).padStart(2, "0");
const iso = (a: number, m: number, d: number) => `${a}-${dois(m)}-${dois(d)}`;
const diasNoMes = (a: number, m: number) => new Date(a, m, 0).getDate();

/** «2026-11-14» (ou «2026-11-14T09:30») → «14/11/2026» (ou «14/11/2026 09:30»). */
function paraTexto(v: string | null | undefined, comHora: boolean): string {
  if (!v || !/^\d{4}-\d{2}-\d{2}/.test(v)) return "";
  const t = `${v.slice(8, 10)}/${v.slice(5, 7)}/${v.slice(0, 4)}`;
  return comHora ? `${t} ${v.slice(11, 16) || "00:00"}` : t;
}

/** Algarismos escritos → «dd/mm/aaaa hh:mm» à medida que se escreve. */
function mascara(texto: string, comHora: boolean): string {
  const n = texto.replace(/\D/g, "").slice(0, comHora ? 12 : 8);
  let s = n.slice(0, 2);
  if (n.length > 2) s += "/" + n.slice(2, 4);
  if (n.length > 4) s += "/" + n.slice(4, 8);
  if (comHora && n.length > 8) s += " " + n.slice(8, 10);
  if (comHora && n.length > 10) s += ":" + n.slice(10, 12);
  return s;
}

/** Texto completo e válido → valor ISO; senão null. */
function paraIso(texto: string, comHora: boolean): string | null {
  const n = texto.replace(/\D/g, "");
  if (n.length !== (comHora ? 12 : 8)) return null;
  const d = +n.slice(0, 2), m = +n.slice(2, 4), a = +n.slice(4, 8);
  if (a < 1900 || m < 1 || m > 12 || d < 1 || d > diasNoMes(a, m)) return null;
  if (!comHora) return iso(a, m, d);
  const h = +n.slice(8, 10), mi = +n.slice(10, 12);
  if (h > 23 || mi > 59) return null;
  return `${iso(a, m, d)}T${dois(h)}:${dois(mi)}`;
}

const horaAgora = () => {
  const d = new Date();
  return `${dois(d.getHours())}:${dois(d.getMinutes())}`;
};

function somarDias(v: string, dias: number): string {
  const d = new Date(+v.slice(0, 4), +v.slice(5, 7) - 1, +v.slice(8, 10) + dias);
  return iso(d.getFullYear(), d.getMonth() + 1, d.getDate());
}

/**
 * Campo de data sempre em dd/mm/aaaa (o campo nativo do browser segue o idioma do Windows e mostrava
 * mm/dd/aaaa num Windows em inglês). Escreve-se a data só com algarismos, as setas ↑/↓ mudam o dia e o botão
 * abre um calendário em português. O valor é o mesmo dos campos nativos: «AAAA-MM-DD» (ou «AAAA-MM-DDTHH:MM»
 * com `comHora`) e «» quando vazio.
 */
export function InputData({
  value,
  onChange,
  min,
  max,
  disabled,
  className,
  autoFocus,
  comHora = false,
  tamanho = "md",
  title,
  placeholder,
}: {
  value: string | null | undefined;
  onChange: (e: Evento) => void;
  min?: string;
  max?: string;
  disabled?: boolean;
  className?: string;
  autoFocus?: boolean;
  /** data e hora (em vez do datetime-local) */
  comHora?: boolean;
  /** "sm": a altura das linhas das tabelas */
  tamanho?: "md" | "sm";
  title?: string;
  placeholder?: string;
}) {
  const [texto, setTexto] = useState(() => paraTexto(value, comHora));
  // calendário aberto: posição no ecrã (fica fora das tabelas e janelas com scroll, para não ser cortado)
  const [aberto, setAberto] = useState<{ top: number; left: number } | null>(null);
  const raiz = useRef<HTMLDivElement>(null);
  const painel = useRef<HTMLDivElement>(null);

  // o valor mudou de fora (outro filtro, «Limpar»…): mostra-o
  useEffect(() => {
    if ((paraIso(texto, comHora) ?? "") !== (value ?? "")) setTexto(paraTexto(value, comHora));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, comHora]);

  useEffect(() => {
    if (!aberto) return;
    const fora = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!raiz.current?.contains(t) && !painel.current?.contains(t)) setAberto(null);
    };
    const fechar = (e: Event) => !painel.current?.contains(e.target as Node) && setAberto(null);
    document.addEventListener("mousedown", fora);
    window.addEventListener("scroll", fechar, true);
    window.addEventListener("resize", fechar);
    return () => {
      document.removeEventListener("mousedown", fora);
      window.removeEventListener("scroll", fechar, true);
      window.removeEventListener("resize", fechar);
    };
  }, [aberto]);

  function abrirCalendario() {
    if (aberto) return setAberto(null);
    const r = raiz.current!.getBoundingClientRect();
    const [largura, altura] = [264, 330];
    const left = Math.max(8, Math.min(r.left, window.innerWidth - largura - 8));
    const top = r.bottom + 6 + altura > window.innerHeight ? Math.max(8, r.top - 6 - altura) : r.bottom + 6;
    setAberto({ top, left });
  }

  const dentro = (v: string) => (!min || v.slice(0, 10) >= min.slice(0, 10)) && (!max || v.slice(0, 10) <= max.slice(0, 10));
  const emitir = (v: string) => onChange({ target: { value: v } });

  function escrever(t: string) {
    const m = mascara(t, comHora);
    setTexto(m);
    if (!m) return emitir("");
    const v = paraIso(m, comHora);
    if (v && dentro(v)) emitir(v);
  }

  function escolher(dia: string) {
    const v = comHora ? `${dia}T${(value ?? "").slice(11, 16) || horaAgora()}` : dia;
    setTexto(paraTexto(v, comHora));
    emitir(v);
    setAberto(null);
  }

  function limpar() {
    escrever("");
    setAberto(null);
  }

  function tecla(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const base = value || hojeISO();
      const dia = somarDias(base.slice(0, 10), e.key === "ArrowUp" ? 1 : -1);
      const v = comHora ? `${dia}T${base.slice(11, 16) || "00:00"}` : dia;
      if (!dentro(v)) return;
      setTexto(paraTexto(v, comHora));
      emitir(v);
    } else if (e.key === "Escape" && aberto) {
      e.stopPropagation();
      setAberto(null);
    }
  }

  const sm = tamanho === "sm";
  // escrito por inteiro mas inválido ou fora dos limites: a moldura fica vermelha até se corrigir
  const completo = texto.replace(/\D/g, "").length === (comHora ? 12 : 8);
  const valido = paraIso(texto, comHora);
  const invalido = completo && !(valido && dentro(valido));
  return (
    <div ref={raiz} className={cn("relative", sm ? "w-36" : "w-full", className)}>
      <input
        type="text"
        inputMode="numeric"
        autoComplete="off"
        autoFocus={autoFocus}
        disabled={disabled}
        title={title}
        value={texto}
        placeholder={placeholder ?? (comHora ? "dd/mm/aaaa hh:mm" : "dd/mm/aaaa")}
        onChange={(e) => escrever(e.target.value)}
        onBlur={() => setTexto(paraTexto(value, comHora))}
        onKeyDown={tecla}
        aria-invalid={invalido || undefined}
        className={cn(
          "num w-full rounded-lg border border-line-strong bg-surface text-ink placeholder:text-ink-3 outline-none transition focus:border-brand focus:ring-3 focus:ring-brand/15 disabled:bg-sunken",
          sm ? "h-7 rounded-md pl-2 pr-7 text-[12.5px]" : "h-10 pl-3 pr-9",
          invalido && "border-danger focus:border-danger focus:ring-danger/15",
        )}
      />
      <button
        type="button"
        tabIndex={-1}
        disabled={disabled}
        onClick={abrirCalendario}
        title="Escolher no calendário"
        className={cn("absolute top-1/2 -translate-y-1/2 rounded-md text-ink-3 transition hover:bg-sunken hover:text-ink disabled:pointer-events-none", sm ? "right-1 p-0.5" : "right-1.5 p-1")}
      >
        <CalendarDays className={sm ? "size-3.5" : "size-4"} />
      </button>
      {aberto &&
        createPortal(
          <div ref={painel} className="fixed z-[60]" style={aberto}>
            <Calendario valor={(value ?? "").slice(0, 10)} dentro={dentro} onEscolher={escolher} onLimpar={limpar} />
          </div>,
          document.body,
        )}
    </div>
  );
}

function Calendario({ valor, dentro, onEscolher, onLimpar }: { valor: string; dentro: (v: string) => boolean; onEscolher: (v: string) => void; onLimpar: () => void }) {
  const hoje = hojeISO();
  const inicial = valor || hoje;
  const [mes, setMes] = useState({ a: +inicial.slice(0, 4), m: +inicial.slice(5, 7) });
  const mover = (n: number) => setMes(({ a, m }) => ({ a: m + n > 12 ? a + 1 : m + n < 1 ? a - 1 : a, m: ((m + n + 11) % 12) + 1 }));
  // primeira célula: a segunda-feira da semana do dia 1
  const desvio = (new Date(mes.a, mes.m - 1, 1).getDay() + 6) % 7;
  const total = diasNoMes(mes.a, mes.m);
  const celulas = Array.from({ length: Math.ceil((desvio + total) / 7) * 7 }, (_, i) => i - desvio + 1);
  return (
    <div className="w-[264px] rounded-xl border border-line bg-surface p-3 text-ink shadow-pop" onMouseDown={(e) => e.preventDefault()}>
      <div className="mb-2 flex items-center justify-between">
        <button type="button" onClick={() => mover(-1)} className="rounded-md p-1 text-ink-2 hover:bg-sunken" aria-label="Mês anterior">
          <ChevronLeft className="size-4" />
        </button>
        <span className="text-[13.5px] font-medium">
          {MESES[mes.m - 1]} {mes.a}
        </span>
        <button type="button" onClick={() => mover(1)} className="rounded-md p-1 text-ink-2 hover:bg-sunken" aria-label="Mês seguinte">
          <ChevronRight className="size-4" />
        </button>
      </div>
      <div className="grid grid-cols-7 gap-0.5 text-center">
        {DIAS_SEMANA.map((d, i) => (
          <span key={i} className="py-1 text-[11px] font-medium text-ink-3">
            {d}
          </span>
        ))}
        {celulas.map((d, i) => {
          if (d < 1 || d > total) return <span key={i} />;
          const v = iso(mes.a, mes.m, d);
          const ok = dentro(v);
          return (
            <button
              key={i}
              type="button"
              disabled={!ok}
              onClick={() => onEscolher(v)}
              className={cn(
                "num h-8 rounded-md text-[13px] transition disabled:opacity-30",
                v === valor ? "bg-brand font-semibold text-white" : v === hoje ? "font-semibold text-brand ring-1 ring-brand/40" : "hover:bg-sunken",
              )}
            >
              {d}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex justify-between border-t border-line pt-2 text-[12.5px]">
        <button type="button" onClick={onLimpar} className="rounded-md px-2 py-1 text-ink-2 hover:bg-sunken">
          Limpar
        </button>
        <button type="button" disabled={!dentro(hoje)} onClick={() => onEscolher(hoje)} className="rounded-md px-2 py-1 font-medium text-brand hover:bg-brand-50 disabled:opacity-40">
          Hoje
        </button>
      </div>
    </div>
  );
}

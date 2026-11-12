import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Delete, KeyRound, LockKeyhole, LogOut } from "lucide-react";
import { call } from "@/lib/api";
import type { Sessao } from "@/lib/types";
import { useSession } from "@/store/session";
import { Button, Field, Input, cn } from "@/components/ui";
import { Logo } from "@/components/Logo";

export interface PoliticaPublica {
  login_pin: boolean;
  senha_min: number;
  inatividade_min: number;
}

/** Política de segurança que o ecrã precisa de conhecer (entrada por PIN, mínimo da senha, inactividade). */
export function usePolitica() {
  return useQuery({ queryKey: ["politica"], queryFn: () => call<PoliticaPublica>("seguranca.politica"), staleTime: 60_000 });
}

/** Ecrã inicial do perfil: quem só tem acesso de caixa vai directamente para o POS. */
export const inicioDe = (s: Sessao) => ((s.nivel ?? (s.perfil === "ADMIN" ? 3 : s.perfil === "GESTOR" ? 2 : 1)) >= 2 ? "/" : "/pos");

/** Bloqueia a sessão: o ecrã fica tapado e o servidor recusa pedidos até se introduzir a senha ou o PIN. */
export async function bloquearSessao() {
  try {
    const s = await call<Sessao>("auth.bloquear");
    useSession.getState().setSessao(s);
  } catch (e) {
    toast.error((e as Error).message);
  }
}

export function useTerminarSessao() {
  const qc = useQueryClient();
  const setSessao = useSession((s) => s.setSessao);
  return async () => {
    try {
      await call("auth.logout");
    } finally {
      qc.clear();
      setSessao(null);
    }
  };
}

/** Bloqueia a sessão ao fim de N minutos sem teclado, rato nem toque. */
export function useInatividade(minutos: number, ativo: boolean) {
  const ultimo = useRef(Date.now());
  useEffect(() => {
    if (!ativo || minutos <= 0) return;
    ultimo.current = Date.now();
    const mexeu = () => (ultimo.current = Date.now());
    const eventos = ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"] as const;
    eventos.forEach((e) => window.addEventListener(e, mexeu, { passive: true, capture: true }));
    const t = setInterval(() => {
      if (Date.now() - ultimo.current >= minutos * 60_000) {
        ultimo.current = Date.now();
        bloquearSessao();
      }
    }, 10_000);
    return () => {
      eventos.forEach((e) => window.removeEventListener(e, mexeu, { capture: true }));
      clearInterval(t);
    };
  }, [minutos, ativo]);
}

// ------------------------------------------------------------------ teclado de PIN

/** Teclado numérico para o PIN (ecrãs tácteis dos terminais POS); também aceita o teclado físico. */
export function TecladoPin({ valor, onChange, onConfirmar, ocupado }: { valor: string; onChange: (v: string) => void; onConfirmar: () => void; ocupado?: boolean }) {
  const tecla = (k: string) => onChange((valor + k).slice(0, 8));
  const base = "h-14 rounded-xl border border-line bg-surface text-[20px] font-semibold shadow-sm transition hover:bg-sunken active:scale-[0.97]";
  return (
    <div>
      <div className="mb-4 flex h-14 items-center justify-center gap-3 rounded-xl border border-line bg-sunken">
        {valor.length === 0 ? (
          <span className="text-ink-3">Introduza o PIN</span>
        ) : (
          Array.from({ length: valor.length }, (_, i) => <span key={i} className="size-3 rounded-full bg-ink" />)
        )}
      </div>
      <div className="grid grid-cols-3 gap-2">
        {["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((k) => (
          <button key={k} type="button" className={base} onClick={() => tecla(k)}>
            {k}
          </button>
        ))}
        <button type="button" className={cn(base, "text-[14px] font-medium text-ink-2")} onClick={() => onChange("")}>
          Limpar
        </button>
        <button type="button" className={base} onClick={() => tecla("0")}>
          0
        </button>
        <button type="button" className={cn(base, "grid place-items-center text-ink-2")} onClick={() => onChange(valor.slice(0, -1))} aria-label="Apagar">
          <Delete className="size-5" />
        </button>
      </div>
      <Button type="button" size="lg" className="mt-3 w-full" loading={ocupado} disabled={valor.length < 4} onClick={onConfirmar}>
        Confirmar
      </Button>
    </div>
  );
}

/** Captura os dígitos do teclado físico enquanto o teclado de PIN está à vista. */
export function useTecladoFisicoPin(ativo: boolean, valor: string, onChange: (v: string) => void, onConfirmar: () => void) {
  useEffect(() => {
    if (!ativo) return;
    const h = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      if (/^\d$/.test(e.key)) onChange((valor + e.key).slice(0, 8));
      else if (e.key === "Backspace") onChange(valor.slice(0, -1));
      else if (e.key === "Enter" && valor.length >= 4) onConfirmar();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [ativo, valor, onChange, onConfirmar]);
}

// ------------------------------------------------------------------ sessão bloqueada

/**
 * Tapa o ecrã sem o desmontar: a venda em curso no POS, os formulários a meio e os filtros ficam
 * como estavam e voltam a estar à vista quando o mesmo utilizador retoma a sessão.
 */
export function EcraBloqueado() {
  const sessao = useSession((s) => s.sessao)!;
  const setSessao = useSession((s) => s.setSessao);
  const terminar = useTerminarSessao();
  const qc = useQueryClient();
  const [modo, setModo] = useState<"senha" | "pin">("senha");
  const [senha, setSenha] = useState("");
  const [pin, setPin] = useState("");
  const [erro, setErro] = useState("");
  const [ocupado, setOcupado] = useState(false);
  const raiz = useRef<HTMLDivElement>(null);
  const campo = useRef<HTMLInputElement>(null);

  // Nada do que está por baixo recebe teclas (atalhos do POS, leitor de códigos, Esc dos diálogos…).
  // Corre antes de todos os outros (fase de captura na janela); no modo PIN é ele que lê os dígitos.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (modo === "senha" && raiz.current?.contains(e.target as Node)) return;
      e.stopImmediatePropagation();
      if (e.key === "Tab") return;
      e.preventDefault();
      if (modo === "senha") return campo.current?.focus();
      if (ocupado) return;
      if (/^\d$/.test(e.key)) setPin((v) => (v + e.key).slice(0, 8));
      else if (e.key === "Backspace") setPin((v) => v.slice(0, -1));
      else if (e.key === "Escape") setPin("");
      else if (e.key === "Enter" && pin.length >= 4) retomar({ pin });
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  });

  async function retomar(params: { senha?: string; pin?: string }) {
    setOcupado(true);
    setErro("");
    try {
      const s = await call<Sessao>("auth.desbloquear", params);
      setSessao(s);
      // o que se tentou carregar com a sessão bloqueada foi recusado: volta a pedir
      qc.invalidateQueries();
    } catch (e) {
      setErro((e as Error).message);
      setSenha("");
      setPin("");
      // muitas tentativas falhadas bloqueiam a conta e terminam a sessão
      const atual = await call<Sessao | null>("auth.sessao").catch(() => sessao);
      if (!atual) setSessao(null);
    } finally {
      setOcupado(false);
    }
  }

  return (
    <div ref={raiz} onKeyDown={(e) => e.stopPropagation()} className="fixed inset-0 z-[200] grid place-items-center bg-brand-700/95 p-6 backdrop-blur-md">
      <div className="w-full max-w-sm rounded-2xl bg-surface p-7 shadow-pop">
        <div className="mb-6 flex items-center gap-3">
          <div className="grid size-12 place-items-center rounded-full bg-brand text-[18px] font-semibold text-white">{sessao.nome.slice(0, 1).toUpperCase()}</div>
          <div className="min-w-0">
            <div className="truncate text-[17px] font-semibold">{sessao.nome}</div>
            <div className="flex items-center gap-1.5 text-[13px] text-ink-3">
              <LockKeyhole className="size-3.5" /> Sessão bloqueada
            </div>
          </div>
        </div>

        <div className="mb-4 grid grid-cols-2 gap-1 rounded-lg bg-sunken p-1 text-[13px]">
          {(["senha", "pin"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => {
                setModo(m);
                setErro("");
              }}
              className={cn("rounded-md py-1.5", modo === m ? "bg-surface font-medium shadow-sm" : "text-ink-2")}
            >
              {m === "senha" ? "Senha" : "PIN"}
            </button>
          ))}
        </div>

        {modo === "senha" ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (senha) retomar({ senha });
            }}
          >
            <Field label="Senha">
              <Input ref={campo} autoFocus type="password" value={senha} onChange={(e) => setSenha(e.target.value)} className="h-11" />
            </Field>
            <Button size="lg" className="mt-4 w-full" loading={ocupado} disabled={!senha}>
              Continuar
            </Button>
          </form>
        ) : (
          <TecladoPin valor={pin} onChange={setPin} ocupado={ocupado} onConfirmar={() => retomar({ pin })} />
        )}

        {erro && <p className="mt-3 rounded-lg bg-danger-50 px-3 py-2 text-[13px] text-danger">{erro}</p>}

        <button type="button" onClick={terminar} className="mx-auto mt-5 flex items-center gap-1.5 text-[13px] text-ink-2 hover:text-ink">
          <LogOut className="size-4" /> Trocar de utilizador
        </button>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ senha provisória

/** Primeiro acesso com uma senha provisória (criada ou redefinida pelo administrador, ou importada). */
export function MudarSenhaObrigatoria() {
  const sessao = useSession((s) => s.sessao)!;
  const setSessao = useSession((s) => s.setSessao);
  const terminar = useTerminarSessao();
  const politica = usePolitica();
  const nav = useNavigate();
  const [f, setF] = useState({ atual: "", nova: "", confirmar: "" });
  const [erro, setErro] = useState("");
  const [ocupado, setOcupado] = useState(false);
  const minimo = politica.data?.senha_min ?? 6;

  async function guardar(e: FormEvent) {
    e.preventDefault();
    setErro("");
    if (f.nova !== f.confirmar) return setErro("A confirmação não coincide com a nova senha.");
    setOcupado(true);
    try {
      const s = await call<Sessao>("auth.alterarSenha", { atual: f.atual, nova: f.nova });
      setSessao(s);
      toast.success("Senha alterada. Bem-vindo!");
      nav(inicioDe(s), { replace: true });
    } catch (err) {
      setErro((err as Error).message);
    } finally {
      setOcupado(false);
    }
  }

  return (
    <Moldura>
      <form onSubmit={guardar} className="w-full max-w-sm">
        <div className="mb-7">
          <div className="mb-4 inline-flex rounded-xl bg-accent-50 p-2.5 text-[#9a6514]">
            <KeyRound className="size-5" />
          </div>
          <h2 className="text-[24px] font-semibold tracking-tight">Escolha a sua senha</h2>
          <p className="mt-1 text-ink-3">
            Olá, {sessao.nome.split(" ")[0]}. Está a usar uma senha provisória: defina uma senha só sua para continuar.
          </p>
        </div>
        <div className="space-y-4">
          <Field label="Senha provisória">
            <Input autoFocus type="password" required value={f.atual} onChange={(e) => setF({ ...f, atual: e.target.value })} className="h-11" />
          </Field>
          <Field label="Nova senha" hint={`Pelo menos ${minimo} caracteres`}>
            <Input type="password" autoComplete="new-password" required value={f.nova} onChange={(e) => setF({ ...f, nova: e.target.value })} className="h-11" />
          </Field>
          <Field label="Confirmar nova senha">
            <Input type="password" autoComplete="new-password" required value={f.confirmar} onChange={(e) => setF({ ...f, confirmar: e.target.value })} className="h-11" />
          </Field>
          {erro && <p className="rounded-lg bg-danger-50 px-3 py-2 text-[13px] text-danger">{erro}</p>}
          <Button size="lg" className="w-full" loading={ocupado}>
            Guardar e entrar
          </Button>
          <button type="button" onClick={terminar} className="mx-auto flex items-center gap-1.5 text-[13px] text-ink-2 hover:text-ink">
            <LogOut className="size-4" /> Sair
          </button>
        </div>
      </form>
    </Moldura>
  );
}

/** Painel de marca à esquerda e o formulário à direita (entrada, senha provisória). */
export function Moldura({ children, empresa }: { children: ReactNode; empresa?: string | null }) {
  return (
    <div className="grid h-full grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
      <div className="relative flex flex-col justify-between overflow-hidden bg-brand-700 p-12 text-white">
        <div className="absolute -right-32 -top-32 size-[480px] rounded-full bg-brand/60 blur-3xl" />
        <div className="absolute -bottom-40 -left-20 size-[420px] rounded-full bg-accent/25 blur-3xl" />
        <div className="relative flex items-center gap-3">
          <Logo className="size-11 rounded-[22%] ring-1 ring-white/15" />
          <span className="text-lg font-semibold">KanGest</span>
        </div>
        <div className="relative max-w-md">
          <h1 className="text-[38px] font-semibold leading-[1.1] tracking-tight">Facturação, stock e caixa num só lugar.</h1>
          <p className="mt-4 text-white/70">Documentos certificáveis pela AGT, POS rápido com leitor de código de barras e relatórios em tempo real.</p>
        </div>
        <div className="relative text-[13px] text-white/50">{empresa ?? ""}</div>
      </div>
      <div className="flex items-center justify-center overflow-y-auto p-10">{children}</div>
    </div>
  );
}

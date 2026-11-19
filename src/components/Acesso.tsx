import { Fragment, forwardRef, useState, type InputHTMLAttributes, type ReactNode } from "react";
import { ArrowLeft, Building2, Check, Eye, EyeOff } from "lucide-react";
import { abrirExterno } from "@/lib/api";
import { cn } from "@/components/ui";
import { Logo } from "@/components/Logo";
import painelImg from "@/assets/acesso/painel.jpg";
import posImg from "@/assets/acesso/pos.jpg";
import documentosImg from "@/assets/acesso/documentos.jpg";
import relatoriosImg from "@/assets/acesso/relatorios.jpg";

/**
 * Ecrãs de acesso (entrar, ligar um aparelho, senha provisória, recuperar a senha, perguntas frequentes): à esquerda a
 * montra do KanGest (o programa e três novidades), à direita o formulário, e em baixo a faixa com as ligações de ajuda.
 */

const SITE = "https://kangest.pages.dev";

export type SeccaoAjuda = "entrar" | "aparelhos" | "seguranca";

const NOVIDADES = [
  { imagem: documentosImg, etiqueta: "Integrações", titulo: "Loja online ligada", texto: "As encomendas do site entram como notas de encomenda e facturas." },
  { imagem: posImg, etiqueta: "Ponto de venda", titulo: "Venda em qualquer aparelho", texto: "Telemóvel, tablet ou terminal POS, pela rede da loja." },
  { imagem: relatoriosImg, etiqueta: "Gestão", titulo: "Números em tempo real", texto: "Vendas por loja, por mês e por artigo, prontas a imprimir." },
];

/** Montra: o painel do KanGest numa janela, o POS por cima e as novidades em cartões (só em ecrãs largos). */
function Vitrine() {
  return (
    <aside className="relative hidden min-h-0 flex-col overflow-hidden bg-[#eef3f2] md:flex">
      {/* fachada clara: faixas verticais de luz, como painéis de vidro */}
      <div className="absolute inset-0 bg-[repeating-linear-gradient(90deg,rgba(255,255,255,0.9)_0_46px,rgba(205,221,217,0.55)_46px_62px,rgba(255,255,255,0.55)_62px_104px)]" />
      <div className="absolute inset-0 bg-[radial-gradient(80%_60%_at_15%_10%,rgba(255,255,255,0.95),transparent_70%)]" />
      <div className="relative flex items-center gap-3 px-10 pt-9">
        <Logo className="size-12 rounded-[22%] shadow-md ring-1 ring-black/5" />
        <span className="text-[24px] font-semibold tracking-tight text-[#0c4a44]">KanGest</span>
      </div>
      <div className="relative flex min-h-0 flex-1 items-center justify-center px-[7%] py-6">
        <figure className="relative w-full max-w-[min(100%,calc((100vh-360px)*1.6))] overflow-hidden rounded-xl bg-white shadow-[0_34px_80px_-24px_rgba(12,74,68,0.55)] ring-1 ring-black/5">
          <div className="flex items-center gap-1.5 border-b border-black/5 bg-[#f6f6f3] px-3 py-2">
            <span className="size-2.5 rounded-full bg-[#ec6a5e]" />
            <span className="size-2.5 rounded-full bg-[#f4bf4f]" />
            <span className="size-2.5 rounded-full bg-[#61c554]" />
            <span className="ml-3 text-[11px] text-[#7c8582]">KanGest — Painel</span>
          </div>
          <img src={painelImg} alt="O painel do KanGest" className="block w-full" draggable={false} />
        </figure>
        <img
          src={posImg}
          alt="O ponto de venda do KanGest"
          draggable={false}
          className="absolute bottom-[3%] right-[3%] w-[36%] rounded-lg shadow-[0_24px_50px_-16px_rgba(12,74,68,0.6)] ring-1 ring-black/10"
        />
      </div>
      <div className="relative grid grid-cols-3 gap-4 px-6 pb-6">
        {NOVIDADES.map((n) => (
          <article key={n.titulo} className="relative aspect-[16/10] overflow-hidden rounded-lg shadow-lg ring-1 ring-black/10">
            <img src={n.imagem} alt="" draggable={false} className="absolute inset-0 size-full object-cover object-top" />
            <div className="absolute inset-0 bg-gradient-to-b from-[#0c4a44]/95 via-[#0c4a44]/70 to-[#0c4a44]/15" />
            <div className="relative p-3.5 text-white">
              <div className="text-[10.5px] font-semibold uppercase tracking-wider text-[#f2b955]">{n.etiqueta}</div>
              <div className="mt-1 text-[15px] font-semibold leading-tight">{n.titulo}</div>
              <div className="mt-1 text-[12px] leading-snug text-white/85">{n.texto}</div>
            </div>
          </article>
        ))}
      </div>
    </aside>
  );
}

/** Faixa de baixo com as ligações de ajuda (perguntas, segurança, contacto, site). */
export function RodapeAcesso({ onAjuda }: { onAjuda?: (s?: SeccaoAjuda) => void }) {
  const ligacoes: { rotulo: string; acao: () => void }[] = [
    ...(onAjuda
      ? [
          { rotulo: "Perguntas frequentes", acao: () => onAjuda() },
          { rotulo: "Segurança", acao: () => onAjuda("seguranca") },
        ]
      : []),
    { rotulo: "Contacte-nos", acao: () => void abrirExterno(`${SITE}/contacto`) },
    { rotulo: "Site do KanGest", acao: () => void abrirExterno(SITE) },
  ];
  return (
    <footer className="flex shrink-0 flex-wrap items-center justify-center gap-x-8 gap-y-1.5 bg-[#0b2724] px-6 py-3.5 text-[13px] text-white/85">
      {ligacoes.map((l) => (
        <button key={l.rotulo} type="button" onClick={l.acao} className="hover:text-white hover:underline">
          {l.rotulo}
        </button>
      ))}
    </footer>
  );
}

/** Painel de marca à esquerda e o formulário à direita (entrada, ligar o aparelho, senha provisória). */
export function Moldura({ children, empresa, topo, onAjuda }: { children: ReactNode; empresa?: string | null; topo?: ReactNode; onAjuda?: (s?: SeccaoAjuda) => void }) {
  const canto = topo ?? (empresa ? <EmpresaNoCanto nome={empresa} /> : null);
  return (
    <div className="flex h-full min-h-0 flex-col bg-bg">
      <div className="grid min-h-0 flex-1 md:grid-cols-[minmax(0,1.05fr)_minmax(0,1fr)]">
        <Vitrine />
        <section className="relative flex min-h-0 flex-col overflow-y-auto">
          {/* luz quente no canto, como num fim de tarde, e o verde da marca em baixo */}
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(75%_60%_at_100%_0%,rgba(224,154,43,0.13),transparent_70%),radial-gradient(60%_50%_at_0%_100%,rgba(20,107,99,0.09),transparent_70%)]" />
          <div className="pointer-events-none absolute inset-0 bg-[repeating-linear-gradient(115deg,transparent_0_140px,rgba(255,255,255,0.22)_140px_143px)]" />
          {/* ecrãs pequenos (telemóvel, terminal POS): só uma faixa com a marca */}
          <div className="relative flex items-center gap-2.5 bg-[#0c4a44] px-5 py-3 text-white md:hidden">
            <Logo className="size-8 rounded-[22%] ring-1 ring-white/15" />
            <span className="font-semibold">KanGest</span>
            {empresa && <span className="ml-auto truncate text-[12px] text-white/70">{empresa}</span>}
          </div>
          {canto && <div className="relative hidden justify-end px-10 pt-8 md:flex">{canto}</div>}
          <div className="relative flex flex-1 items-center justify-center px-6 py-10 md:px-12">{children}</div>
        </section>
      </div>
      <RodapeAcesso onAjuda={onAjuda} />
    </div>
  );
}

function EmpresaNoCanto({ nome }: { nome: string }) {
  return (
    <span className="inline-flex max-w-[22rem] items-center gap-2 text-[14px] text-ink-2" title={nome}>
      <Building2 className="size-4 shrink-0 text-brand" />
      <span className="truncate">{nome}</span>
    </span>
  );
}

/** «Bem-vindo ao KanGest» com a linha por baixo, como cabeçalho dos formulários de acesso. */
export function BoasVindas({ antes = "Bem-vindo ao", titulo = "KanGest", texto }: { antes?: string; titulo?: string; texto?: ReactNode }) {
  return (
    <div className="mb-7">
      <p className="text-[17px] text-ink">{antes}</p>
      <h1 className="text-[42px] font-bold leading-[1.05] tracking-tight text-brand">{titulo}</h1>
      <div className="mt-4 h-px w-full bg-line-strong" />
      {texto && <p className="mt-4 text-[14px] leading-relaxed text-ink-3">{texto}</p>}
    </div>
  );
}

/** Rótulo por cima, o campo, e por baixo «Este campo é obrigatório» (ou outra mensagem) quando falta. */
export function CampoAcesso({ label, erro, dica, children }: { label: string; erro?: string | false; dica?: ReactNode; children: ReactNode }) {
  return (
    <div>
      <div className="mb-2 text-[14px] font-medium text-ink">{label}</div>
      {children}
      {erro ? <p className="mt-1.5 text-[13px] text-danger">{erro}</p> : dica ? <p className="mt-1.5 text-[12.5px] text-ink-3">{dica}</p> : null}
    </div>
  );
}

const campoBase =
  "h-14 w-full rounded-md border bg-surface px-4 text-[16px] text-ink outline-none transition placeholder:text-ink-3 focus:ring-3 disabled:bg-sunken";

export const InputAcesso = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { invalido?: boolean }>(
  ({ className, invalido, ...p }, ref) => (
    <input
      ref={ref}
      aria-invalid={invalido || undefined}
      className={cn(campoBase, invalido ? "border-danger focus:border-danger focus:ring-danger/15" : "border-line-strong focus:border-brand focus:ring-brand/15", className)}
      {...p}
    />
  ),
);
InputAcesso.displayName = "InputAcesso";

/** Campo de senha com o olho para mostrar ou esconder o que se escreveu. */
export const InputSenha = forwardRef<HTMLInputElement, Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & { invalido?: boolean }>(
  ({ className, ...p }, ref) => {
    const [ver, setVer] = useState(false);
    return (
      <div className="relative">
        <InputAcesso ref={ref} type={ver ? "text" : "password"} className={cn("pr-12", className)} {...p} />
        <button
          type="button"
          tabIndex={-1}
          onClick={() => setVer((v) => !v)}
          className="absolute inset-y-0 right-0 grid w-12 place-items-center text-ink-2 hover:text-ink"
          aria-label={ver ? "Esconder" : "Mostrar"}
          title={ver ? "Esconder" : "Mostrar"}
        >
          {ver ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
        </button>
      </div>
    );
  },
);
InputSenha.displayName = "InputSenha";

/** Interruptor pequeno com o rótulo à esquerda (ex.: «Ocultar valores»). */
export function Interruptor({ ligado, onChange, rotulo, dica }: { ligado: boolean; onChange: (v: boolean) => void; rotulo: string; dica?: string }) {
  return (
    <button type="button" role="switch" aria-checked={ligado} onClick={() => onChange(!ligado)} title={dica} className="inline-flex items-center gap-2.5 text-[14px] text-ink">
      {rotulo}
      <span className={cn("relative h-5 w-9 rounded-full transition-colors", ligado ? "bg-brand" : "bg-line-strong")}>
        <span className={cn("absolute top-0.5 size-4 rounded-full bg-white shadow transition-transform", ligado ? "translate-x-[18px]" : "translate-x-0.5")} />
      </span>
    </button>
  );
}

/** Campo com o rótulo dentro da linha (sobe quando se escreve), como nos formulários de recuperação. */
export function CampoFlutuante({
  label,
  senha,
  invalido,
  className,
  ...p
}: Omit<InputHTMLAttributes<HTMLInputElement>, "placeholder"> & { label: string; senha?: boolean; invalido?: boolean }) {
  const [ver, setVer] = useState(false);
  return (
    <label className={cn("relative block", className)}>
      <input
        placeholder=" "
        type={senha ? (ver ? "text" : "password") : p.type}
        aria-invalid={invalido || undefined}
        className={cn(
          "peer h-14 w-full rounded-md border bg-surface px-4 text-[16px] text-ink outline-none transition",
          senha && "pr-12",
          invalido ? "border-danger focus:border-danger" : "border-line-strong focus:border-accent",
        )}
        {...p}
      />
      <span
        className={cn(
          "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 bg-surface px-1 text-[15px] text-ink-3 transition-all",
          "peer-focus:top-0 peer-focus:text-[12px] peer-[:not(:placeholder-shown)]:top-0 peer-[:not(:placeholder-shown)]:text-[12px]",
          invalido ? "peer-focus:text-danger" : "peer-focus:text-accent-ink",
        )}
      >
        {label} *
      </span>
      {senha && (
        <button
          type="button"
          tabIndex={-1}
          onClick={() => setVer((v) => !v)}
          className="absolute right-0 top-0 grid h-14 w-12 place-items-center text-ink-2 hover:text-ink"
          aria-label={ver ? "Esconder" : "Mostrar"}
        >
          {ver ? <EyeOff className="size-5" /> : <Eye className="size-5" />}
        </button>
      )}
    </label>
  );
}

/** Os passos de um processo (1 — 2 — 3), com o actual em destaque e os feitos com um visto. */
export function Passos({ passos, atual }: { passos: string[]; atual: number }) {
  return (
    <ol className="flex items-center gap-3">
      {passos.map((p, i) => (
        <Fragment key={p}>
          <li className="flex shrink-0 items-center gap-2.5" aria-current={i === atual ? "step" : undefined}>
            <span className={cn("grid size-6 place-items-center rounded-full text-[12px] font-semibold text-white", i <= atual ? "bg-accent" : "bg-ink-3")}>
              {i < atual ? <Check className="size-3.5" /> : i + 1}
            </span>
            <span className={cn("text-[15px]", i === atual ? "font-medium text-ink" : "text-ink-3", i !== atual && "hidden sm:inline")}>{p}</span>
          </li>
          {i < passos.length - 1 && <li aria-hidden className="h-px min-w-6 flex-1 bg-line" />}
        </Fragment>
      ))}
    </ol>
  );
}

/**
 * Página inteira de ajuda (perguntas frequentes, recuperar a senha): uma faixa com «←» e o título, o conteúdo ao
 * centro e a faixa das ligações em baixo. `variante`: «escura» (faixa verde-escura, título dourado) ou «clara».
 */
export function PaginaAcesso({
  titulo,
  onVoltar,
  variante = "clara",
  onAjuda,
  children,
}: {
  titulo: string;
  onVoltar: () => void;
  variante?: "clara" | "escura";
  onAjuda?: (s?: SeccaoAjuda) => void;
  children: ReactNode;
}) {
  const escura = variante === "escura";
  return (
    <div className="flex h-full min-h-0 flex-col bg-bg">
      <div className="flex shrink-0 items-center gap-2.5 bg-[#0b2724] px-6 py-2.5 text-white md:px-[12%]">
        <Logo className="size-7 rounded-[22%] ring-1 ring-white/15" />
        <span className="font-semibold">KanGest</span>
      </div>
      <header
        className={cn(
          "shrink-0 px-6 md:px-[12%]",
          escura ? "bg-[linear-gradient(100deg,#0b2724_0%,#0f4a44_45%,#3a8a7e_100%)] py-14" : "bg-sunken py-10",
        )}
      >
        <button
          type="button"
          onClick={onVoltar}
          aria-label="Voltar"
          title="Voltar"
          className={cn("-ml-1 rounded p-1", escura ? "text-white/80 hover:text-white" : "text-ink-3 hover:text-ink")}
        >
          <ArrowLeft className="size-5" />
        </button>
        <h1 className={cn("mt-2 text-[28px] font-semibold tracking-tight", escura ? "text-[#f2b955]" : "text-accent-ink")}>{titulo}</h1>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto px-6 py-10 md:px-[12%]">{children}</main>
      <RodapeAcesso onAjuda={onAjuda} />
    </div>
  );
}

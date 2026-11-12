import { useEffect, useRef, useState } from "react";
import { ouvirVisor, type EstadoVisor } from "@/lib/equipamento";
import { kz, qtd } from "@/lib/format";
import { Logo } from "@/components/Logo";

const INICIAL: EstadoVisor = {
  fase: "livre",
  empresa: { nome: "", logo: null },
  mensagem1: "",
  mensagem2: "",
  linhas: [],
  ultima: null,
  total: 0,
  itens: 0,
  desconto: 0,
};

/**
 * Ecrã do cliente (segundo monitor dos terminais de ecrã duplo). Não tem sessão nem chama o servidor:
 * recebe tudo do POS desta máquina.
 */
export default function VisorCliente() {
  const [e, setE] = useState<EstadoVisor>(INICIAL);
  const [agora, setAgora] = useState(() => new Date());
  const ativaRef = useRef<HTMLDivElement>(null);

  useEffect(() => ouvirVisor(setE), []);
  useEffect(() => {
    const t = setInterval(() => setAgora(new Date()), 30_000);
    return () => clearInterval(t);
  }, []);
  const ativa = e.ativa ?? e.linhas.length - 1;
  // a linha que mudou fica sempre à vista
  useEffect(() => {
    ativaRef.current?.scrollIntoView({ block: "nearest" });
  }, [ativa, e.linhas.length, e.ultima]);

  const hora = agora.toLocaleTimeString("pt-PT", { hour: "2-digit", minute: "2-digit" });
  const diaTxt = agora.toLocaleDateString("pt-PT", { weekday: "long", day: "numeric", month: "long" });
  const dia = diaTxt.charAt(0).toUpperCase() + diaTxt.slice(1);
  const marca = e.empresa.logo ? (
    <img src={e.empresa.logo} alt="" className="max-h-full max-w-full object-contain" />
  ) : (
    <Logo className="size-full rounded-[22%]" />
  );

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-bg text-ink select-none">
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-line bg-surface px-6">
        <div className="grid size-10 place-items-center">{marca}</div>
        <div className="text-[18px] font-semibold">{e.empresa.nome || "KanGest"}</div>
        <div className="ml-auto text-right leading-tight">
          <div className="num text-[18px] font-semibold">{hora}</div>
          <div className="text-[12.5px] text-ink-3">{dia}</div>
        </div>
      </header>

      {e.fase === "livre" && (
        <main className="grid flex-1 place-items-center p-10 text-center">
          <div>
            <div className="mx-auto mb-8 grid size-40 place-items-center">{marca}</div>
            <div className="text-[44px] font-semibold leading-tight tracking-tight">{e.mensagem1 || e.empresa.nome || "Bem-vindo"}</div>
            <div className="mt-3 text-[24px] text-ink-2">{e.mensagem2 || "Bem-vindo!"}</div>
          </div>
        </main>
      )}

      {(e.fase === "venda" || e.fase === "pagar") && (
        <main className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_minmax(320px,38%)]">
          <section className="min-h-0 overflow-y-auto px-6 py-4">
            {e.linhas.map((l, i) => {
              const destaque = i === ativa;
              return (
                <div
                  key={i}
                  ref={destaque ? ativaRef : undefined}
                  className={`flex items-baseline justify-between gap-4 border-b border-line py-3 ${destaque ? "rounded-lg bg-brand-50 px-3 text-brand-700" : ""}`}
                >
                  <div className="min-w-0">
                    <div className="truncate text-[20px] font-medium">{l.descricao}</div>
                    <div className="num text-[14px] text-ink-3">{qtd(l.quantidade)} un.</div>
                  </div>
                  <div className="num shrink-0 text-[20px] font-semibold">{kz(l.total)}</div>
                </div>
              );
            })}
          </section>
          <aside className="flex flex-col justify-center gap-6 bg-brand px-8 text-white">
            <div>
              <div className="text-[18px] uppercase tracking-wide text-white/75">{e.fase === "pagar" ? "Total a pagar" : "Total"}</div>
              <div className="num mt-1 text-[52px] font-bold leading-none tracking-tight">{kz(e.total)}</div>
            </div>
            <div className="grid gap-1 text-[17px] text-white/85">
              <span>
                {e.itens} {e.itens === 1 ? "artigo" : "artigos"}
              </span>
              {e.desconto > 0 && <span>Poupou {kz(e.desconto)}</span>}
            </div>
          </aside>
        </main>
      )}

      {e.fase === "fim" && (
        <main className="grid flex-1 place-items-center p-10 text-center">
          <div>
            <div className="text-[48px] font-semibold tracking-tight">Obrigado pela preferência!</div>
            {e.documento && <div className="mt-2 text-[20px] text-ink-3">{e.documento}</div>}
            <div className="mx-auto mt-10 grid w-fit grid-cols-2 gap-x-12 gap-y-3 text-left text-[26px]">
              <span className="text-ink-2">Total</span>
              <span className="num text-right font-semibold">{kz(e.total)}</span>
              {e.pago !== undefined && e.pago > 0 && (
                <>
                  <span className="text-ink-2">Recebido</span>
                  <span className="num text-right font-semibold">{kz(e.pago)}</span>
                </>
              )}
              {!!e.troco && (
                <>
                  <span className="font-semibold text-[#9a6514]">Troco</span>
                  <span className="num text-right text-[34px] font-bold text-[#9a6514]">{kz(e.troco)}</span>
                </>
              )}
            </div>
          </div>
        </main>
      )}
    </div>
  );
}

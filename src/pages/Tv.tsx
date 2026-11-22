// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Maximize, Minimize, Target, X } from "lucide-react";
import { call } from "@/lib/api";
import { useKz } from "@/lib/privacidade";
import { useSession } from "@/store/session";
import { Button, Modal, cn } from "@/components/ui";
import { BarChart } from "@/components/BarChart";

interface Loja {
  id: number;
  nome: string;
  hoje: number;
  mes: number;
  meta: number;
  percentagem: number | null;
}
interface PainelTv {
  agora: string;
  empresa: string | null;
  loja_unica: boolean;
  dia: number;
  dias_mes: number;
  hoje: { total: number; documentos: number; semana_passada_ate_agora: number };
  mes: { total: number; documentos: number; meta: number; projeccao: number; percentagem: number | null; ritmo_ideal: number };
  horas: { hora: number; hoje: number; semana_passada: number }[];
  lojas: Loja[];
  ultimas: { numero: string; quando: string; total: number; loja: string | null; artigo: string | null; artigos: number }[];
  top: { descricao: string; quantidade: number; total: number }[];
}

const MEDALHAS = ["#d4a73f", "#c0c7d1", "#c9875a"];

/** Cor da percentagem da meta, contra o ritmo ideal do mês (dia / dias do mês). */
function tom(pct: number | null, ideal: number) {
  if (pct == null) return "text-white/60";
  if (pct >= 100) return "text-emerald-300";
  return pct >= ideal * 0.9 ? "text-emerald-300" : pct >= ideal * 0.7 ? "text-amber-300" : "text-rose-300";
}

/**
 * Modo TV: o ecrã das vendas para uma televisão na loja ou na sede (rota /tv, fora do menu). Actualiza de 30 em 30
 * segundos, mantém o ecrã ligado e mostra só valores (sem nomes de clientes). Com «Ocultar valores», os kwanzas
 * aparecem escondidos como no painel.
 */
export default function Tv() {
  const navigate = useNavigate();
  const kz = useKz();
  const tem = useSession((s) => s.tem);
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["tv"], queryFn: () => call<PainelTv>("tv.painel"), refetchInterval: 30_000 });
  const d = q.data;
  const [relogio, setRelogio] = useState(() => new Date());
  const [ecraInteiro, setEcraInteiro] = useState(() => !!document.fullscreenElement);
  const [metas, setMetas] = useState(false);

  useEffect(() => {
    const t = setInterval(() => setRelogio(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  // o ecrã não se apaga (onde o sistema deixa); volta a pedir ao regressar ao separador
  useEffect(() => {
    let bloqueio: { release: () => Promise<void> } | null = null;
    const pedir = async () => {
      try {
        bloqueio = await (navigator as Navigator & { wakeLock?: { request: (t: string) => Promise<{ release: () => Promise<void> }> } }).wakeLock?.request("screen") ?? null;
      } catch {
        /* sem permissão: fica como o sistema decidir */
      }
    };
    void pedir();
    const f = () => document.visibilityState === "visible" && void pedir();
    const fs = () => setEcraInteiro(!!document.fullscreenElement);
    document.addEventListener("visibilitychange", f);
    document.addEventListener("fullscreenchange", fs);
    return () => {
      document.removeEventListener("visibilitychange", f);
      document.removeEventListener("fullscreenchange", fs);
      void bloqueio?.release().catch(() => undefined);
    };
  }, []);

  const ideal = d ? (d.dia / d.dias_mes) * 100 : 0;
  const variacao = d && d.hoje.semana_passada_ate_agora > 0 ? ((d.hoje.total - d.hoje.semana_passada_ate_agora) / d.hoje.semana_passada_ate_agora) * 100 : null;
  const projPct = d && d.mes.meta > 0 ? Math.round((d.mes.projeccao / d.mes.meta) * 100) : null;
  const horas = useMemo(
    () =>
      (d?.horas ?? [])
        .filter((h) => (h.hora >= 7 && h.hora <= 21) || h.hoje !== 0)
        .map((h) => ({ chave: `${String(h.hora).padStart(2, "0")}:00`, rotulo: `${h.hora}h`, valor: h.hoje / 100 })),
    [d],
  );

  const alternarEcra = async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await document.documentElement.requestFullscreen();
    } catch {
      /* o sistema não deixa */
    }
  };

  return (
    <div className="dark fixed inset-0 flex flex-col overflow-hidden bg-[#06302c] text-white" style={{ backgroundImage: "radial-gradient(80rem 40rem at 0% 0%, #146b6355, transparent 60%)" }}>
      <header className="flex items-center gap-4 border-b border-white/10 bg-[#0c4a44]/70 px-6 py-3">
        <span className="text-xl font-extrabold tracking-tight">KanGest</span>
        <span className="h-7 w-px bg-white/20" />
        <span className="text-sm font-bold uppercase tracking-[0.2em] text-[#fcefc7]">Painel de vendas</span>
        {d?.empresa && <span className="truncate text-sm text-white/70">{d.empresa}</span>}
        <span className={cn("ml-2 inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-bold", q.isError ? "bg-amber-400/15 text-amber-200" : "bg-emerald-400/15 text-emerald-200")}>
          <span className={cn("size-2 rounded-full", q.isError ? "bg-amber-300" : "animate-pulse bg-emerald-300")} />
          {q.isError ? "SEM LIGAÇÃO" : "AO VIVO"}
        </span>
        <span className="ml-auto font-mono text-3xl font-bold tabular-nums">{relogio.toLocaleTimeString("pt-PT")}</span>
        <div className="flex items-center gap-1">
          {tem("sistema.configuracoes") && (
            <Button variant="ghost" size="sm" className="text-white hover:bg-white/10" onClick={() => setMetas(true)} title="Metas do mês por loja">
              <Target className="size-4" /> Metas
            </Button>
          )}
          <Button variant="ghost" size="sm" className="text-white hover:bg-white/10" onClick={alternarEcra} aria-label={ecraInteiro ? "Sair do ecrã inteiro" : "Ecrã inteiro"}>
            {ecraInteiro ? <Minimize className="size-4" /> : <Maximize className="size-4" />}
          </Button>
          <Button variant="ghost" size="sm" className="text-white hover:bg-white/10" onClick={() => navigate("/")} aria-label="Sair do modo TV">
            <X className="size-4" />
          </Button>
        </div>
      </header>

      {!d ? (
        <div className="flex flex-1 items-center justify-center text-white/70">{q.isError ? "Não foi possível ler as vendas. A tentar outra vez…" : "A abrir…"}</div>
      ) : (
        <main className="grid min-h-0 flex-1 grid-cols-12 grid-rows-[auto_minmax(0,1fr)_minmax(0,0.8fr)] gap-4 p-5">
          {/* cartões */}
          <Cartao className="col-span-3" titulo="Vendas de hoje">
            <p className="text-5xl font-extrabold tabular-nums tracking-tight">{kz(d.hoje.total)}</p>
            <p className="mt-2 text-sm text-white/70">
              {d.hoje.documentos} {d.hoje.documentos === 1 ? "venda" : "vendas"}
              {variacao != null && (
                <span className={cn("ml-2 rounded-full px-2 py-0.5 font-semibold", variacao >= 0 ? "bg-emerald-400/15 text-emerald-200" : "bg-rose-400/15 text-rose-200")}>
                  {variacao >= 0 ? "▲" : "▼"} {Math.abs(variacao).toFixed(1).replace(".", ",")} % vs. semana passada
                </span>
              )}
            </p>
          </Cartao>
          <Cartao className="col-span-3" titulo="Vendas do mês">
            <p className="text-5xl font-extrabold tabular-nums tracking-tight">{kz(d.mes.total)}</p>
            <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/10">
              <div className="h-full rounded-full bg-[#7fd3c8]" style={{ width: `${ideal}%` }} />
            </div>
            <p className="mt-1.5 text-sm text-white/70">dia {d.dia} de {d.dias_mes}</p>
          </Cartao>
          <Cartao className="col-span-3" titulo="Meta do mês">
            {d.mes.meta > 0 ? (
              <div className="flex items-center gap-4">
                <Anel pct={d.mes.percentagem ?? 0} ideal={ideal} />
                <div className="min-w-0 space-y-1 whitespace-nowrap text-sm">
                  <p className="font-semibold tabular-nums">{kz(d.mes.meta)}</p>
                  <p className="text-white/70">Falta <b className="text-white">{kz(Math.max(d.mes.meta - d.mes.total, 0))}</b></p>
                  <p className={cn("font-semibold", tom(d.mes.percentagem, ideal))}>
                    {(d.mes.percentagem ?? 0) >= 100 ? "Meta atingida" : (d.mes.percentagem ?? 0) >= ideal * 0.9 ? "Em dia" : "Abaixo do ritmo"}
                  </p>
                </div>
              </div>
            ) : (
              <p className="text-sm text-white/70">{tem("sistema.configuracoes") ? "Defina as metas do mês em «Metas»." : "Sem metas definidas para este mês."}</p>
            )}
          </Cartao>
          <Cartao className="col-span-3" titulo="Projecção do mês">
            <p className="text-5xl font-extrabold tabular-nums tracking-tight">{kz(d.mes.projeccao)}</p>
            <p className="mt-2 text-sm text-white/70">
              {projPct != null ? (
                <span className={cn("rounded-full px-2 py-0.5 font-semibold", projPct >= 100 ? "bg-emerald-400/15 text-emerald-200" : "bg-amber-400/15 text-amber-200")}>
                  {projPct >= 100 ? "▲" : "▼"} {projPct} % da meta
                </span>
              ) : (
                "ao ritmo actual"
              )}
            </p>
          </Cartao>

          {/* hora a hora e lojas */}
          <Cartao className={cn("flex flex-col", d.loja_unica ? "col-span-12" : "col-span-8")} titulo="Hoje, hora a hora">
            <div className="min-h-0 flex-1 rounded-xl bg-white p-3 text-[#1d2422]">
              <BarChart dados={horas} formatar={(v) => kz(Math.round(v * 100))} altura={220} />
            </div>
          </Cartao>
          {!d.loja_unica && (
            <Cartao className="col-span-4 overflow-hidden" titulo="Ranking das lojas · % da meta">
              <ol className="space-y-3">
                {d.lojas.map((l, i) => (
                  <li key={l.id} className="flex items-center gap-3">
                    <span className="grid size-8 shrink-0 place-items-center rounded-full text-sm font-bold text-[#06302c]" style={{ background: MEDALHAS[i] ?? "#ffffff33" }}>
                      {i + 1}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-baseline gap-2">
                        <span className="truncate font-semibold">{l.nome}</span>
                        <span className="ml-auto whitespace-nowrap text-sm tabular-nums text-white/80">{kz(l.mes)}</span>
                        <span className={cn("w-14 text-right text-sm font-bold tabular-nums", tom(l.percentagem, ideal))}>
                          {l.percentagem != null ? `${l.percentagem.toFixed(0)} %` : "—"}
                        </span>
                      </div>
                      <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-white/10">
                        <div className="h-full rounded-full bg-[#7fd3c8]" style={{ width: `${Math.min(l.percentagem ?? 0, 100)}%` }} />
                      </div>
                      <p className="mt-0.5 text-xs text-white/60">hoje {kz(l.hoje)}</p>
                    </div>
                  </li>
                ))}
              </ol>
            </Cartao>
          )}

          {/* últimas vendas e mais vendidos */}
          <Cartao className="col-span-7 overflow-hidden" titulo="Últimas vendas">
            <ul className="divide-y divide-white/10">
              {d.ultimas.map((u) => (
                <li key={u.numero} className="flex items-center gap-3 py-1.5 text-sm">
                  <span className="w-12 font-mono tabular-nums text-white/70">{u.quando.slice(11, 16)}</span>
                  {u.loja && !d.loja_unica && <span className="w-32 truncate font-semibold">{u.loja}</span>}
                  <span className="min-w-0 flex-1 truncate text-white/80">
                    {u.artigo ?? u.numero}
                    {u.artigos > 1 && <span className="text-white/50"> +{u.artigos - 1}</span>}
                  </span>
                  <span className={cn("whitespace-nowrap font-bold tabular-nums", u.total < 0 ? "text-rose-300" : "text-emerald-200")}>{kz(u.total)}</span>
                </li>
              ))}
              {d.ultimas.length === 0 && <li className="py-6 text-center text-white/60">Ainda não há vendas.</li>}
            </ul>
          </Cartao>
          <Cartao className="col-span-5 overflow-hidden" titulo="Mais vendidos hoje">
            <ol className="space-y-2">
              {d.top.map((t, i) => (
                <li key={t.descricao} className="flex items-center gap-3 text-sm">
                  <span className="grid size-7 shrink-0 place-items-center rounded-lg bg-white/10 text-xs font-bold">{i + 1}</span>
                  <span className="min-w-0 flex-1 truncate">{t.descricao}</span>
                  <span className="text-white/60 tabular-nums">{String(t.quantidade).replace(".", ",")} un.</span>
                  <span className="w-32 text-right font-semibold tabular-nums">{kz(t.total)}</span>
                </li>
              ))}
              {d.top.length === 0 && <li className="py-6 text-center text-white/60">Ainda não há vendas hoje.</li>}
            </ol>
          </Cartao>
        </main>
      )}

      {metas && (
        <MetasModal
          onClose={() => setMetas(false)}
          onGuardado={() => {
            setMetas(false);
            void qc.invalidateQueries({ queryKey: ["tv"] });
          }}
        />
      )}
    </div>
  );
}

function Cartao({ titulo, className, children }: { titulo: string; className?: string; children: React.ReactNode }) {
  return (
    <section className={cn("rounded-2xl border border-white/10 bg-white/[0.06] p-4 shadow-xl shadow-black/20", className)}>
      <h2 className="mb-2 text-[11px] font-bold uppercase tracking-[0.16em] text-white/60">{titulo}</h2>
      {children}
    </section>
  );
}

/** Anel da meta, com um traço onde devia estar hoje ao ritmo da meta. */
function Anel({ pct, ideal }: { pct: number; ideal: number }) {
  const t = 96;
  const e = 10;
  const r = (t - e) / 2;
  const c = 2 * Math.PI * r;
  const v = Math.min(Math.max(pct, 0), 100);
  const ang = (Math.min(Math.max(ideal, 0), 100) / 100) * 2 * Math.PI - Math.PI / 2;
  return (
    <div className="relative shrink-0" style={{ width: t, height: t }}>
      <svg width={t} height={t}>
        <circle cx={t / 2} cy={t / 2} r={r} fill="none" stroke="#ffffff22" strokeWidth={e} />
        <circle cx={t / 2} cy={t / 2} r={r} fill="none" stroke="#7fd3c8" strokeWidth={e} strokeLinecap="round" strokeDasharray={`${(v / 100) * c} ${c}`} transform={`rotate(-90 ${t / 2} ${t / 2})`} />
        <line x1={t / 2 + (r - e) * Math.cos(ang)} y1={t / 2 + (r - e) * Math.sin(ang)} x2={t / 2 + (r + e) * Math.cos(ang)} y2={t / 2 + (r + e) * Math.sin(ang)} stroke="#fcefc7" strokeWidth={2} />
      </svg>
      <span className="absolute inset-0 grid place-items-center text-2xl font-extrabold tabular-nums">{Math.round(pct)}%</span>
    </div>
  );
}

function MetasModal({ onClose, onGuardado }: { onClose: () => void; onGuardado: () => void }) {
  const q = useQuery({ queryKey: ["tv.metas"], queryFn: () => call<{ lojas: { id: number; nome: string; meta: number }[] }>("tv.metas") });
  const [valores, setValores] = useState<Record<number, string>>({});
  const [erro, setErro] = useState("");
  const [aGuardar, setAGuardar] = useState(false);
  useEffect(() => {
    if (q.data) setValores(Object.fromEntries(q.data.lojas.map((l) => [l.id, l.meta ? String(l.meta / 100) : ""])));
  }, [q.data]);
  const guardar = async () => {
    setErro("");
    const metas: Record<string, number> = {};
    for (const [id, v] of Object.entries(valores)) {
      const n = Number(String(v).replace(/\s/g, "").replace(",", ".") || 0);
      if (!Number.isFinite(n) || n < 0) return setErro("Escreva as metas em kwanzas (0 ou vazio tira a meta).");
      metas[id] = Math.round(n * 100);
    }
    setAGuardar(true);
    try {
      await call("tv.metasGuardar", { metas });
      onGuardado();
    } catch (e) {
      setErro(e instanceof Error ? e.message : String(e));
    } finally {
      setAGuardar(false);
    }
  };
  return (
    <Modal
      open
      onClose={onClose}
      title="Metas do mês por loja"
      width="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>Cancelar</Button>
          <Button onClick={guardar} loading={aGuardar}>Guardar</Button>
        </>
      }
    >
      <p className="mb-3 text-sm text-ink-3">Em kwanzas, com IVA. A meta da empresa é a soma das lojas. Vazio ou 0 tira a meta.</p>
      <div className="space-y-2">
        {q.data?.lojas.map((l) => (
          <label key={l.id} className="flex items-center gap-3 text-sm">
            <span className="min-w-0 flex-1 truncate">{l.nome}</span>
            <input
              inputMode="decimal"
              className="w-44 rounded-lg border border-line bg-surface px-3 py-1.5 text-right tabular-nums"
              value={valores[l.id] ?? ""}
              onChange={(e) => setValores((v) => ({ ...v, [l.id]: e.target.value }))}
              placeholder="0"
            />
          </label>
        ))}
      </div>
      {erro && <p className="mt-3 text-sm text-danger">{erro}</p>}
    </Modal>
  );
}

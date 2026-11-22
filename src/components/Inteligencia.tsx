// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// KanGest inteligente no painel: o resumo de ontem, as previsões e os alertas. Tudo é calculado neste computador (ou no
// servidor da rede) a partir dos dados da empresa (services/inteligencia.rs); nada sai para fora.
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  ArrowDownRight, Calculator, CalendarRange, CheckCircle2, ChevronRight, FileX2, HandCoins, PackageX, Percent, Snowflake, Sparkles, Tag,
  TrendingDown, UserX, type LucideIcon,
} from "lucide-react";
import { call } from "@/lib/api";
import { data, pct, qtd } from "@/lib/format";
import { useKz } from "@/lib/privacidade";
import { Badge, Card, Delta, cn } from "@/components/ui";

interface Resumo {
  dia: string;
  dia_semana: string;
  vendas: number;
  docs: number;
  ticket: number;
  habitual: number | null;
  variacao: number | null;
  margem: number | null;
  lucro: number | null;
  top_artigo: { descricao: string; quantidade: number; total: number } | null;
  melhor_cliente: { id: number; nome: string; total: number; docs: number } | null;
}
interface Previsoes {
  fecho_mes: { vendas_ate_hoje: number; previsto: number; dias_restantes: number; mes_anterior: number | null; variacao: number | null } | null;
  ruptura: { n: number; artigos: { artigo_id: number; descricao: string; stock: number; media_diaria: number; dias: number }[] };
  tesouraria: { entra: number; sai: number; saldo: number; entra_atraso: number; sai_atraso: number };
}
interface Item {
  id?: number | null;
  nome: string;
  valor?: number;
  n?: number;
  dias?: number;
  intervalo?: number;
  custo?: number;
  quantidade?: number;
}
interface Alerta {
  tipo: string;
  tom: "danger" | "accent" | "neutral";
  n: number;
  valor: number;
  ligacao: string;
  itens: Item[];
  extra: { habitual?: number; variacao?: number; actual?: number; anterior?: number } | null;
}
interface Painel {
  hoje: string;
  resumo: Resumo;
  previsoes: Previsoes;
  alertas: Alerta[];
}

/** «sexta-feira» → «sextas-feiras», «sábado» → «sábados». */
const nosDias = (d: string) => (d.endsWith("-feira") ? d.replace(/-feira$/, "s-feiras") : d + "s");
const plural = (n: number, um: string, varios: string) => (n === 1 ? um : varios);

function Esqueleto() {
  return <div className="h-[190px] animate-pulse rounded-lg bg-sunken/60" />;
}

function Titulo({ icon: Icon, titulo, sub, extra }: { icon: LucideIcon; titulo: string; sub: string; extra?: React.ReactNode }) {
  return (
    <div className="mb-3 flex items-start justify-between gap-2">
      <div>
        <div className="flex items-center gap-2 font-semibold">
          <Icon className="size-4 text-brand" /> {titulo}
        </div>
        <div className="text-[13px] text-ink-3">{sub}</div>
      </div>
      {extra}
    </div>
  );
}

function Linha({ rotulo, children, onClick }: { rotulo: string; children: React.ReactNode; onClick?: () => void }) {
  return (
    <li className="flex items-baseline justify-between gap-3">
      <span className="shrink-0 text-ink-3">{rotulo}</span>
      {onClick ? (
        <button className="min-w-0 text-right underline-offset-2 hover:underline" onClick={onClick}>
          {children}
        </button>
      ) : (
        <span className="min-w-0 text-right">{children}</span>
      )}
    </li>
  );
}

export default function Inteligencia() {
  const { data: d, isError } = useQuery({
    queryKey: ["inteligencia"],
    queryFn: () => call<Painel>("inteligencia.painel"),
    refetchInterval: 300_000,
    retry: false,
  });
  // sem a permissão de relatórios, ou num servidor de rede com uma versão anterior: o painel fica como estava
  if (isError) return null;
  return (
    <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-3">
      <ResumoDeOntem r={d?.resumo} />
      <CartaoPrevisoes p={d?.previsoes} />
      <CartaoAlertas a={d?.alertas} />
    </div>
  );
}

function ResumoDeOntem({ r }: { r?: Resumo }) {
  const nav = useNavigate();
  const kz = useKz();
  return (
    <Card className="flex flex-col p-5">
      <Titulo icon={Sparkles} titulo="Resumo de ontem" sub={r ? `${r.dia_semana[0].toUpperCase()}${r.dia_semana.slice(1)}, ${data(r.dia)}` : "…"} />
      {!r ? (
        <Esqueleto />
      ) : r.docs === 0 ? (
        <p className="text-[13.5px] text-ink-2">
          Ontem não houve vendas.
          {!!r.habitual && ` Às ${nosDias(r.dia_semana)}, costuma vender ${kz(r.habitual)}.`}
        </p>
      ) : (
        <>
          <div className="num text-[24px] font-semibold tracking-tight">{kz(r.vendas)}</div>
          {r.variacao !== null && r.habitual !== null && (
            <p className="text-[13px] text-ink-2">
              <span className={cn("num font-medium", r.variacao >= 0 ? "text-ok" : "text-danger")}>
                {r.variacao > 0 ? "+" : ""}
                {r.variacao}%
              </span>{" "}
              face ao habitual às {nosDias(r.dia_semana)} ({kz(r.habitual)})
            </p>
          )}
          <ul className="mt-3 space-y-1.5 text-[13.5px]">
            <Linha rotulo="Documentos">
              <span className="num">{r.docs}</span> · ticket médio <span className="num">{kz(r.ticket)}</span>
            </Linha>
            {r.margem !== null && (
              <Linha rotulo="Margem">
                <span className="num">{pct(r.margem)}</span>
                {r.lucro !== null && <> · lucro <span className="num">{kz(r.lucro)}</span></>}
              </Linha>
            )}
            {r.top_artigo && (
              <Linha rotulo="Mais vendido">
                {r.top_artigo.descricao} (<span className="num">{qtd(r.top_artigo.quantidade)}</span>)
              </Linha>
            )}
            {r.melhor_cliente && (
              <Linha rotulo="Melhor cliente" onClick={() => nav(`/clientes?id=${r.melhor_cliente!.id}`)}>
                {r.melhor_cliente.nome} · <span className="num">{kz(r.melhor_cliente.total)}</span>
              </Linha>
            )}
          </ul>
        </>
      )}
    </Card>
  );
}

function CartaoPrevisoes({ p }: { p?: Previsoes }) {
  const nav = useNavigate();
  const kz = useKz();
  const f = p?.fecho_mes;
  const t = p?.tesouraria;
  return (
    <Card className="flex flex-col p-5">
      <Titulo icon={CalendarRange} titulo="Previsões" sub="Ao ritmo das últimas semanas" />
      {!p ? (
        <Esqueleto />
      ) : (
        <div className="space-y-4 text-[13.5px]">
          <button className="block w-full text-left" onClick={() => nav("/relatorios?agrupar=mes")}>
            <div className="text-[12.5px] text-ink-3">Fecho do mês</div>
            {f ? (
              <>
                <div className="num text-[20px] font-semibold tracking-tight">≈ {kz(f.previsto)}</div>
                <div className="flex flex-wrap items-center gap-x-1.5 text-[12.5px] text-ink-3">
                  <span>
                    <span className="num">{kz(f.vendas_ate_hoje)}</span> até hoje · {f.dias_restantes === 0 ? "último dia" : `faltam ${f.dias_restantes} ${plural(f.dias_restantes, "dia", "dias")}`}
                  </span>
                  {f.mes_anterior !== null && <Delta atual={f.previsto} anterior={f.mes_anterior} periodo="vs mês anterior" />}
                </div>
              </>
            ) : (
              <div className="text-ink-2">A previsão aparece com 14 dias de vendas.</div>
            )}
          </button>

          {t && (
            <button className="block w-full text-left" onClick={() => nav("/contas/receber")}>
              <div className="text-[12.5px] text-ink-3">Próximos 30 dias (vencimentos)</div>
              <div>
                Entram <span className="num font-medium">{kz(t.entra)}</span>, saem <span className="num font-medium">{kz(t.sai)}</span>
              </div>
              {(t.entra_atraso > 0 || t.sai_atraso > 0) && (
                <div className="text-[12.5px] text-ink-3">
                  Já em atraso:{" "}
                  {[t.entra_atraso > 0 && `${kz(t.entra_atraso)} a receber`, t.sai_atraso > 0 && `${kz(t.sai_atraso)} a pagar`].filter(Boolean).join(", ")}
                </div>
              )}
            </button>
          )}

          <div>
            <div className="flex items-center justify-between gap-2 text-[12.5px] text-ink-3">
              <span>Vão esgotar em 7 dias</span>
              {p.ruptura.n > 0 && (
                <button className="underline-offset-2 hover:text-ink hover:underline" onClick={() => nav("/compras/sugestao")}>
                  Sugestão de encomendas
                </button>
              )}
            </div>
            {p.ruptura.n === 0 ? (
              <div className="text-ink-2">Nenhum artigo, ao ritmo actual.</div>
            ) : (
              <ul className="mt-1 space-y-1">
                {p.ruptura.artigos.slice(0, 3).map((a) => (
                  <li key={a.artigo_id} className="flex flex-wrap items-baseline justify-between gap-x-3">
                    <span className="min-w-0">{a.descricao}</span>
                    <span className="num shrink-0 text-[12.5px] text-ink-3">
                      {qtd(a.stock)} em stock · {a.dias === 0 ? "esgota hoje" : `~${a.dias} ${plural(a.dias, "dia", "dias")}`}
                    </span>
                  </li>
                ))}
                {p.ruptura.n > 3 && <li className="text-[12.5px] text-ink-3">e mais {p.ruptura.n - 3}</li>}
              </ul>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}

const ICONES: Record<string, LucideIcon> = {
  abaixo_custo: TrendingDown,
  caixa_diferenca: Calculator,
  anulacoes: FileX2,
  vendas_quebra: ArrowDownRight,
  margem_queda: Percent,
  preco_abaixo_custo: Tag,
  devedores: HandCoins,
  clientes_parados: UserX,
  stock_parado: Snowflake,
};

/** A frase de cada alerta e a linha de detalhe (os valores respeitam «Ocultar valores»). */
function descrever(a: Alerta, kz: (c: number | null | undefined) => string): { titulo: string; detalhe: string } {
  const nomes = (k = 3) => a.itens.slice(0, k).map((i) => i.nome).join(", ");
  switch (a.tipo) {
    case "abaixo_custo":
      return { titulo: `${a.n} ${plural(a.n, "venda", "vendas")} abaixo do custo`, detalhe: `Últimos 7 dias · perda de ${kz(a.valor)} · ${nomes()}` };
    case "caixa_diferenca":
      return {
        titulo: `${a.n} ${plural(a.n, "fecho de caixa", "fechos de caixa")} com diferença`,
        detalhe: `Últimos 7 dias · ${a.itens.map((i) => `${i.nome}: ${(i.valor ?? 0) < 0 ? "falta" : "sobra"} ${kz(Math.abs(i.valor ?? 0))}`).join(" · ")}`,
      };
    case "anulacoes":
      return {
        titulo: `${a.n} vendas anuladas esta semana`,
        detalhe: `O habitual é ${String(a.extra?.habitual ?? 0).replace(".", ",")} por semana · ${a.itens.map((i) => `${i.nome}: ${i.n}`).join(", ")}`,
      };
    case "vendas_quebra":
      return {
        titulo: `Vendas da semana ${Math.abs(a.extra?.variacao ?? 0)}% abaixo do habitual`,
        detalhe: `${kz(a.valor)} face a ${kz(a.extra?.habitual)} por semana`,
      };
    case "margem_queda":
      return { titulo: `Margem a cair para ${pct(a.extra?.actual)}`, detalhe: `Últimos 30 dias · nos 3 meses anteriores era ${pct(a.extra?.anterior)}` };
    case "preco_abaixo_custo":
      return {
        titulo: `${a.n} ${plural(a.n, "preço", "preços")} de venda abaixo do custo`,
        detalhe: a.itens.slice(0, 2).map((i) => `${i.nome}: ${kz(i.valor)} (custo ${kz(i.custo)})`).join(" · "),
      };
    case "devedores": {
      const i = a.itens[0];
      return {
        titulo: `${a.n} ${plural(a.n, "cliente", "clientes")} em atraso há mais de 30 dias`,
        detalhe: `${kz(a.valor)} no total${i ? ` · ${i.nome}: ${kz(i.valor)}, ${i.dias} dias` : ""}`,
      };
    }
    case "clientes_parados":
      return {
        titulo: `${a.n} ${plural(a.n, "bom cliente deixou", "bons clientes deixaram")} de comprar`,
        detalhe: a.itens.slice(0, 2).map((i) => `${i.nome}: há ${i.dias} dias (comprava de ${i.intervalo} em ${i.intervalo})`).join(" · "),
      };
    case "stock_parado":
      return { titulo: `${a.n} ${plural(a.n, "artigo parado", "artigos parados")} há 90 dias`, detalhe: `${kz(a.valor)} ao custo · ${nomes()}` };
    default:
      return { titulo: a.tipo, detalhe: "" };
  }
}

function CartaoAlertas({ a }: { a?: Alerta[] }) {
  const nav = useNavigate();
  const kz = useKz();
  return (
    <Card className="flex flex-col p-5">
      <Titulo
        icon={Sparkles}
        titulo="Alertas inteligentes"
        sub="O que foge ao habitual nos seus dados"
        extra={a && a.length > 0 ? <Badge tone={a.some((x) => x.tom === "danger") ? "danger" : "accent"}>{a.length}</Badge> : undefined}
      />
      {!a ? (
        <Esqueleto />
      ) : a.length ? (
        <ul className="-mx-2 space-y-0.5">
          {a.map((x) => {
            const Icon = ICONES[x.tipo] ?? PackageX;
            const { titulo, detalhe } = descrever(x, kz);
            return (
              <li key={x.tipo}>
                <button onClick={() => nav(x.ligacao)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition hover:bg-sunken">
                  <span
                    className={cn(
                      "grid size-8 shrink-0 place-items-center rounded-lg",
                      x.tom === "danger" ? "bg-danger-50 text-danger" : x.tom === "accent" ? "bg-accent-50 text-accent-ink" : "bg-sunken text-ink-2",
                    )}
                  >
                    <Icon className="size-4" />
                  </span>
                  <span className="min-w-0 flex-1 text-[13.5px] leading-snug">
                    {titulo}
                    {detalhe && <span className="line-clamp-2 text-[12px] text-ink-3">{detalhe}</span>}
                  </span>
                  <ChevronRight className="size-4 shrink-0 text-ink-3" />
                </button>
              </li>
            );
          })}
        </ul>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 py-6 text-center">
          <span className="rounded-2xl bg-ok-50 p-3 text-ok">
            <CheckCircle2 className="size-6" />
          </span>
          <p className="font-medium">Nada fora do habitual</p>
          <p className="max-w-[260px] text-[13px] text-ink-3">Vendas, margens, fechos de caixa, preços e clientes dentro do normal.</p>
        </div>
      )}
    </Card>
  );
}

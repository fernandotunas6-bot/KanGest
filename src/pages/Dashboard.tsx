// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { BarChart3, CalendarClock, CalendarX2, CheckCircle2, ChevronRight, CircleDollarSign, Eye, EyeOff, FileClock, FilePlus2, GraduationCap, HandCoins, Hourglass, MonitorPlay, PackageMinus, Receipt, Repeat, Send, ShieldAlert, ShoppingCart, Table2, TrendingUp, Truck, Unplug, type LucideIcon } from "lucide-react";
import { call } from "@/lib/api";
import { data, dataHora, hojeISO, qtd } from "@/lib/format";
import { useKz, useOcultarValores } from "@/lib/privacidade";
import { linkPronto } from "@/lib/construtor";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Delta, PageHeader, Stat, Table, Td, Th, cn } from "@/components/ui";
import { BarChart } from "@/components/BarChart";
import DocumentoModal from "@/components/DocumentoModal";
import Inteligencia from "@/components/Inteligencia";

interface Antiguidade {
  por_vencer: number;
  d1_30: number;
  d31_60: number;
  d61_90: number;
  mais_90: number;
  docs_vencidos: number;
}

interface Alertas {
  proformas_a_expirar: number;
  lotes_expirados: number;
  lotes_a_expirar: number;
  fe_pendentes: number;
  fe_rejeitadas: number;
  compras_vencidas: number;
  valor_compras_vencidas: number;
  avencas_por_facturar?: number;
  /** alunos com propinas por facturar (modo escola) */
  propinas_por_facturar?: number;
  /** chaves da API sem contacto há mais de 3 horas e webhooks com 3 falhas seguidas (a loja online parou?) */
  integracoes_paradas?: number;
}

interface Dash {
  vendas_hoje: number;
  docs_hoje: number;
  vendas_mes: number;
  docs_mes: number;
  iva_mes: number;
  ticket_medio: number;
  a_receber: number;
  a_pagar: number;
  stock_baixo: number;
  // os campos seguintes não existem num servidor de rede com uma versão anterior
  vendas_ontem?: number;
  vendas_mes_anterior?: number;
  antiguidade?: Antiguidade;
  alertas?: Alertas;
  por_modo?: { descricao: string; total: number }[];
  serie: { dia: string; total: number; docs: number }[];
  top_artigos: { referencia: string; descricao: string; quantidade: number; total: number }[];
  ultimos: { id: number; numero_doc: string; entidade_nome: string; total: number; data_hora: string; estado: string }[];
}

export default function Dashboard() {
  const nav = useNavigate();
  // com a permissão de relatórios, cada indicador abre o relatório pronto que o explica (construtor de relatórios)
  const analisa = useSession((s) => s.tem)("relatorios.ver");
  const analisar = (id: string, periodo?: Parameters<typeof linkPronto>[1]) => (analisa ? () => nav(linkPronto(id, periodo)) : undefined);
  const [doc, setDoc] = useState<number | null>(null);
  const [tabela, setTabela] = useState(false);
  const { data: d } = useQuery({ queryKey: ["dashboard"], queryFn: () => call<Dash>("dashboard"), refetchInterval: 60_000 });
  // «Ocultar valores» (escolhido aqui ou no ecrã de entrada): os kwanzas aparecem como ••••••
  const kz = useKz();
  const [ocultar, setOcultar] = useOcultarValores();
  // enquanto carrega não se mostram zeros nem "sem documentos" (pareceria que não há dados)
  const v = (x: number | undefined) => (d ? kz(x) : "…");
  // o valor grande dos cartões: enquanto carrega, uma barra a pulsar (em vez de «…»)
  const valor = (x: number | undefined) => (d ? kz(x) : <span className="inline-block h-7 w-32 max-w-full animate-pulse rounded-md bg-sunken align-middle" aria-label="A carregar" />);
  const ant = d?.antiguidade;
  const vencido = ant ? ant.d1_30 + ant.d31_60 + ant.d61_90 + ant.mais_90 : 0;
  const al = d?.alertas;

  // 30 dias completos (dias sem vendas = 0) para o eixo não "saltar".
  const serie = useMemo(() => {
    const mapa = new Map<string, number>(d?.serie.map((s) => [s.dia, s.total] as [string, number]));
    const out = [];
    const fim = new Date(hojeISO() + "T12:00:00");
    for (let i = 29; i >= 0; i--) {
      const dt = new Date(fim.getTime() - i * 86400000);
      const iso = dt.toISOString().slice(0, 10);
      out.push({ chave: data(iso), rotulo: iso.slice(8, 10) + "/" + iso.slice(5, 7), valor: mapa.get(iso) ?? 0 });
    }
    return out;
  }, [d]);

  // «A sua atenção»: só o que tem alguma coisa para fazer, do mais grave para o menos grave
  const atencao: { n: number; texto: [string, string]; valor?: number; to: string; tom: "danger" | "accent" | "neutral"; icon: LucideIcon }[] = d
    ? (
        [
          { n: ant?.docs_vencidos ?? 0, texto: ["documento de cliente vencido", "documentos de clientes vencidos"], valor: vencido, to: "/avisos", tom: "danger", icon: CalendarClock },
          { n: al?.compras_vencidas ?? 0, texto: ["pagamento a fornecedor em atraso", "pagamentos a fornecedores em atraso"], valor: al?.valor_compras_vencidas, to: "/contas/pagar", tom: "danger", icon: Truck },
          { n: al?.integracoes_paradas ?? 0, texto: ["integração sem sincronizar há mais de 3 horas", "integrações sem sincronizar há mais de 3 horas"], to: "/integracoes", tom: "danger", icon: Unplug },
          { n: al?.fe_rejeitadas ?? 0, texto: ["factura rejeitada pela AGT", "facturas rejeitadas pela AGT"], to: "/facturacao-electronica", tom: "danger", icon: ShieldAlert },
          { n: al?.lotes_expirados ?? 0, texto: ["lote expirado ainda com stock", "lotes expirados ainda com stock"], to: "/relatorios/stock?situacao=validade", tom: "danger", icon: CalendarX2 },
          { n: al?.fe_pendentes ?? 0, texto: ["factura por validar na AGT", "facturas por validar na AGT"], to: "/facturacao-electronica", tom: "accent", icon: Send },
          { n: al?.propinas_por_facturar ?? 0, texto: ["aluno com propinas por facturar", "alunos com propinas por facturar"], to: "/escola", tom: "accent", icon: GraduationCap },
          { n: al?.avencas_por_facturar ?? 0, texto: ["avença por facturar", "avenças por facturar"], to: "/avencas", tom: "accent", icon: Repeat },
          { n: d.stock_baixo, texto: ["artigo no mínimo ou abaixo", "artigos no mínimo ou abaixo"], to: "/stock", tom: "accent", icon: PackageMinus },
          { n: al?.lotes_a_expirar ?? 0, texto: ["lote perto da validade", "lotes perto da validade"], to: "/relatorios/stock?situacao=validade", tom: "accent", icon: Hourglass },
          { n: al?.proformas_a_expirar ?? 0, texto: ["proforma ou orçamento expira em 7 dias", "proformas e orçamentos expiram em 7 dias"], to: "/documentos?grupo=VENDAS", tom: "neutral", icon: FileClock },
        ] as const
      )
        .filter((a) => a.n > 0)
        .map((a) => ({ ...a, texto: [...a.texto] as [string, string] }))
    : [];

  const faixas = ant
    ? [
        { nome: "Por vencer", valor: ant.por_vencer },
        { nome: "1–30 dias", valor: ant.d1_30 },
        { nome: "31–60 dias", valor: ant.d31_60 },
        { nome: "61–90 dias", valor: ant.d61_90 },
        { nome: "Mais de 90 dias", valor: ant.mais_90 },
      ]
    : [];
  const modos = d?.por_modo ?? [];

  return (
    <>
      <PageHeader
        title="Painel"
        subtitle={new Date().toLocaleDateString("pt-PT", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}
        actions={
          <>
            <Button variant="secondary" onClick={() => nav("/tv")} title="O painel de vendas em ecrã inteiro, para uma televisão">
              <MonitorPlay className="size-4" /> Modo TV
            </Button>
            <Button variant="secondary" onClick={() => setOcultar(!ocultar)} title={ocultar ? "Mostrar os valores do painel" : "Ocultar os valores do painel"}>
              {ocultar ? <Eye className="size-4" /> : <EyeOff className="size-4" />} {ocultar ? "Mostrar valores" : "Ocultar valores"}
            </Button>
            <Button variant="secondary" onClick={() => nav("/documentos/novo/FT")}>
              <FilePlus2 className="size-4" /> Nova factura
            </Button>
            <Button onClick={() => nav("/pos")}>
              <ShoppingCart className="size-4" /> Abrir POS
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 sm:gap-4 lg:grid-cols-4">
        <Stat
          label="Vendas de hoje"
          value={valor(d?.vendas_hoje)}
          delta={d && <Delta atual={d.vendas_hoje} anterior={d.vendas_ontem ?? 0} periodo="vs ontem" />}
          hint={d ? `${d.docs_hoje} ${d.docs_hoje === 1 ? "documento" : "documentos"}${d.vendas_ontem !== undefined ? ` · ontem ${kz(d.vendas_ontem)}` : ""}` : "…"}
          icon={<CircleDollarSign className="size-4" />}
          onAnalisar={analisar("vendas_hoje")}
        />
        <Stat
          label="Vendas do mês"
          value={valor(d?.vendas_mes)}
          delta={d && <Delta atual={d.vendas_mes} anterior={d.vendas_mes_anterior ?? 0} periodo="vs mês anterior" />}
          hint={`IVA liquidado ${v(d?.iva_mes)} · ticket médio ${v(d?.ticket_medio)}`}
          icon={<TrendingUp className="size-4" />}
          onAnalisar={analisar("vendas_dia", "mes")}
        />
        <Stat
          label="A receber de clientes"
          value={valor(d?.a_receber)}
          hint={
            vencido > 0 ? (
              <button className="text-left font-medium text-danger underline-offset-2 hover:underline" onClick={() => nav("/avisos")}>
                Vencido: {kz(vencido)}
              </button>
            ) : d ? (
              "Nada vencido"
            ) : (
              "…"
            )
          }
          icon={<HandCoins className="size-4" />}
          tone="accent"
          onAnalisar={analisar("antiguidade")}
        />
        <Stat
          label="A pagar a fornecedores"
          value={valor(d?.a_pagar)}
          hint={
            al?.valor_compras_vencidas ? (
              <button className="text-left font-medium text-danger underline-offset-2 hover:underline" onClick={() => nav("/contas/pagar")}>
                Em atraso: {kz(al.valor_compras_vencidas)}
              </button>
            ) : (
              <button className="text-left underline-offset-2 hover:underline" onClick={() => nav("/contas/pagar")}>
                Ver contas a pagar
              </button>
            )
          }
          icon={<Truck className="size-4" />}
          tone="brand"
        />
      </div>

      {/* KanGest inteligente: resumo de ontem, previsões e alertas, calculados neste computador */}
      <Inteligencia />

      <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1.7fr)_minmax(0,1fr)]">
        <Card className="p-5">
          <div className="mb-3 flex items-center justify-between">
            <div>
              <div className="font-semibold">Vendas — últimos 30 dias</div>
              <div className="text-[13px] text-ink-3">Valores com IVA, líquidos de notas de crédito</div>
            </div>
            <div className="flex items-center gap-1">
              {analisa && (
                <Button variant="ghost" size="sm" onClick={() => nav(linkPronto("vendas_dia", "30dias"))} title="Abrir no construtor de relatórios: agrupar, comparar, exportar">
                  <BarChart3 className="size-4" /> Analisar
                </Button>
              )}
              <Button variant="ghost" size="sm" onClick={() => setTabela(!tabela)}>
                <Table2 className="size-4" /> {tabela ? "Gráfico" : "Tabela"}
              </Button>
            </div>
          </div>
          {tabela ? (
            <Table className="max-h-[230px]">
              <thead>
                <tr>
                  <Th>Dia</Th>
                  <Th className="text-right">Vendas</Th>
                </tr>
              </thead>
              <tbody>
                {[...serie].reverse().map((s) => (
                  <tr key={s.chave}>
                    <Td>{s.chave}</Td>
                    <Td className="num text-right">{kz(s.valor)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          ) : d ? (
            <BarChart dados={serie} formatar={kz} ocultarEixo={ocultar} />
          ) : (
            <div className="h-[230px] animate-pulse rounded-lg bg-sunken/60" />
          )}
        </Card>

        <Card className="flex flex-col p-5">
          <div className="mb-2 flex items-center justify-between">
            <div className="font-semibold">A sua atenção</div>
            {atencao.length > 0 && <Badge tone="danger">{atencao.length}</Badge>}
          </div>
          {!d ? (
            <div className="h-[200px] animate-pulse rounded-lg bg-sunken/60" />
          ) : atencao.length ? (
            <ul className="-mx-2 space-y-0.5">
              {atencao.map((a) => (
                <li key={a.texto[1]}>
                  <button onClick={() => nav(a.to)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition hover:bg-sunken">
                    <span
                      className={cn(
                        "grid size-8 shrink-0 place-items-center rounded-lg",
                        a.tom === "danger" ? "bg-danger-50 text-danger" : a.tom === "accent" ? "bg-accent-50 text-accent-ink" : "bg-sunken text-ink-2",
                      )}
                    >
                      <a.icon className="size-4" />
                    </span>
                    <span className="min-w-0 flex-1 text-[13.5px] leading-snug">
                      <b className="num font-semibold">{a.n}</b> {a.n === 1 ? a.texto[0] : a.texto[1]}
                      {!!a.valor && <span className="num block text-[12px] text-ink-3">{kz(a.valor)}</span>}
                    </span>
                    <ChevronRight className="size-4 shrink-0 text-ink-3" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 py-8 text-center">
              <span className="rounded-2xl bg-ok-50 p-3 text-ok">
                <CheckCircle2 className="size-6" />
              </span>
              <p className="font-medium">Está tudo em dia</p>
              <p className="max-w-[240px] text-[13px] text-ink-3">Sem documentos vencidos, stock em falta, lotes a expirar nem facturas pendentes na AGT.</p>
            </div>
          )}
        </Card>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-3">
        <Card className="p-5">
          <div className="mb-1 flex items-baseline justify-between gap-3">
            <div className="font-semibold">Antiguidade de saldos</div>
            <button className="whitespace-nowrap text-[12.5px] text-ink-3 underline-offset-2 hover:text-ink hover:underline" onClick={() => nav("/avisos")}>
              Ver avisos
            </button>
          </div>
          <div className="mb-3 text-[13px] text-ink-3">A receber, por dias desde o vencimento</div>
          {!d ? <Esqueleto /> : ant ? <ListaBarras linhas={faixas} total={d.a_receber} vazio="Nenhum valor por receber." /> : <SoComServidorNovo />}
        </Card>

        <Card className="p-5">
          <div className="mb-1 font-semibold">Recebimentos do mês</div>
          <div className="mb-3 text-[13px] text-ink-3">Por meio de pagamento (FR, recibos e adiantamentos)</div>
          {!d ? (
            <Esqueleto />
          ) : d.por_modo ? (
            <ListaBarras linhas={modos.map((m) => ({ nome: m.descricao, valor: m.total }))} vazio="Ainda sem recebimentos este mês." />
          ) : (
            <SoComServidorNovo />
          )}
        </Card>

        <TopPainel />
      </div>

      <Card className="mt-4">
        <div className="flex items-center justify-between px-5 pb-2 pt-4">
          <div className="font-semibold">Últimos documentos</div>
          <Button variant="ghost" size="sm" onClick={() => nav("/documentos")}>
            Ver todos
          </Button>
        </div>
        <Table>
          <thead>
            <tr>
              <Th>Documento</Th>
              <Th>Cliente</Th>
              <Th>Data</Th>
              <Th className="text-right">Total</Th>
            </tr>
          </thead>
          <tbody>
            {d?.ultimos.map((u) => (
              <tr key={u.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setDoc(u.id)}>
                <Td className="font-medium">
                  <span className="inline-flex items-center gap-2">
                    <Receipt className="size-4 text-ink-3" /> {u.numero_doc}
                    {u.estado === "A" && <Badge tone="danger">Anulado</Badge>}
                  </span>
                </Td>
                <Td>{u.entidade_nome}</Td>
                <Td className="text-ink-2">{dataHora(u.data_hora)}</Td>
                <Td className="num text-right font-medium">{kz(u.total)}</Td>
              </tr>
            ))}
          </tbody>
        </Table>
        {d && !d.ultimos.length && <p className="py-8 text-center text-ink-3">Ainda não foram emitidos documentos.</p>}
      </Card>

      <DocumentoModal id={doc} onClose={() => setDoc(null)} />
    </>
  );
}

function Esqueleto() {
  return <div className="h-[150px] animate-pulse rounded-lg bg-sunken/60" />;
}

/** Posto ligado a um servidor de rede com uma versão anterior do KanGest (sem estes dados). */
function SoComServidorNovo() {
  return <p className="py-8 text-center text-[13px] text-ink-3">Disponível depois de actualizar o KanGest no servidor.</p>;
}

/**
 * Lista de barras: nome, barra e valor em cada linha, com as barras alinhadas numa só grelha (uma só cor;
 * a ordem é a dos dados — faixas de antiguidade pela ordem natural, meios de pagamento do maior para o menor).
 */
function ListaBarras({ linhas, total, vazio }: { linhas: { nome: string; valor: number }[]; total?: number; vazio: string }) {
  const kz = useKz();
  const max = Math.max(0, ...linhas.map((l) => l.valor));
  const soma = total ?? linhas.reduce((s, l) => s + l.valor, 0);
  if (!max) return <p className="py-8 text-center text-[13px] text-ink-3">{vazio}</p>;
  return (
    <div className="grid grid-cols-[minmax(0,6.75rem)_minmax(40px,1fr)_auto_auto] items-center gap-x-3 gap-y-2.5 text-[13px]">
      {linhas.map((l) => (
        <div key={l.nome} className="contents" title={`${l.nome}: ${kz(l.valor)}`}>
          <span className="truncate text-ink-2">{l.nome}</span>
          <span className="h-2 rounded-full bg-brand-50">
            {l.valor > 0 && <span className="block h-2 rounded-full bg-chart" style={{ width: `${Math.max(3, (l.valor / max) * 100)}%` }} />}
          </span>
          <span className="num text-right font-medium">{kz(l.valor)}</span>
          <span className="num w-9 text-right text-[12px] text-ink-3">{soma > 0 ? `${Math.round((l.valor / soma) * 100)}%` : ""}</span>
        </div>
      ))}
    </div>
  );
}

type TipoTop = "artigos" | "clientes" | "familias" | "vendedores" | "operadores";
type PeriodoTop = "hoje" | "semana" | "mes" | "30dias" | "ano";
interface PrefTop {
  tipo: TipoTop;
  periodo: PeriodoTop;
  ordem: "valor" | "quantidade";
}
const TIPOS_TOP: Record<TipoTop, { titulo: string; unidade: string }> = {
  artigos: { titulo: "Artigos mais vendidos", unidade: "unid." },
  clientes: { titulo: "Melhores clientes", unidade: "documentos" },
  familias: { titulo: "Famílias que mais vendem", unidade: "unid." },
  vendedores: { titulo: "Vendedores", unidade: "documentos" },
  operadores: { titulo: "Quem mais vendeu", unidade: "documentos" },
};
const PERIODOS_TOP: Record<PeriodoTop, string> = { hoje: "hoje", semana: "7 dias", mes: "este mês", "30dias": "30 dias", ano: "este ano" };
const CHAVE_TOP = "kangest.painel.top";

/** Os «tops» do painel, à escolha de cada utilizador (fica neste aparelho): o quê, o período e a ordem. */
function TopPainel() {
  const kz = useKz();
  const [pref, setPref] = useState<PrefTop>(() => {
    try {
      return { tipo: "artigos", periodo: "mes", ordem: "valor", ...JSON.parse(localStorage.getItem(CHAVE_TOP) ?? "{}") };
    } catch {
      return { tipo: "artigos", periodo: "mes", ordem: "valor" };
    }
  });
  const mudar = (p: Partial<PrefTop>) => {
    const n = { ...pref, ...p };
    setPref(n);
    try {
      localStorage.setItem(CHAVE_TOP, JSON.stringify(n));
    } catch {
      /* sem armazenamento: vale até fechar */
    }
  };
  const q = useQuery({
    queryKey: ["painel-top", pref],
    queryFn: () => call<{ total_periodo: number; linhas: { chave: string | number; nome: string; quantidade: number; total: number }[] }>("relatorios.top", { ...pref, n: 6 }),
    refetchInterval: 60_000,
    placeholderData: (x) => x,
  });
  const linhas = q.data?.linhas ?? [];
  const maior = Math.max(1, ...linhas.map((l) => (pref.ordem === "quantidade" ? l.quantidade : l.total)));
  const sel = "rounded-md border border-line bg-surface px-1.5 py-0.5 text-[12.5px] font-normal text-ink-2 outline-none hover:border-line-strong focus:border-brand";
  return (
    <Card className={cn("p-5 transition-opacity", q.isFetching && "opacity-80")}>
      <div className="mb-3 flex flex-wrap items-center gap-x-2 gap-y-1.5">
        <select aria-label="O quê" className={cn(sel, "font-semibold text-ink")} value={pref.tipo} onChange={(e) => mudar({ tipo: e.target.value as TipoTop })}>
          {Object.entries(TIPOS_TOP).map(([k, v]) => (
            <option key={k} value={k}>
              {v.titulo}
            </option>
          ))}
        </select>
        <select aria-label="Período" className={sel} value={pref.periodo} onChange={(e) => mudar({ periodo: e.target.value as PeriodoTop })}>
          {Object.entries(PERIODOS_TOP).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
        <select aria-label="Ordenar por" className={sel} value={pref.ordem} onChange={(e) => mudar({ ordem: e.target.value as PrefTop["ordem"] })}>
          <option value="valor">por valor</option>
          <option value="quantidade">por {TIPOS_TOP[pref.tipo].unidade === "unid." ? "quantidade" : "n.º de documentos"}</option>
        </select>
      </div>
      {linhas.length ? (
        <ol className="space-y-2.5">
          {linhas.map((a, i) => {
            const v = pref.ordem === "quantidade" ? a.quantidade : a.total;
            return (
              <li key={`${a.chave}-${i}`} className="flex items-center gap-3">
                <span className="num grid size-6 shrink-0 place-items-center rounded-md bg-sunken text-[12px] text-ink-2">{i + 1}</span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13.5px]">{a.nome}</div>
                  <div className="mt-1 h-1 overflow-hidden rounded-full bg-sunken">
                    <div className="h-full rounded-full bg-brand/70" style={{ width: `${Math.max(4, (v / maior) * 100)}%` }} />
                  </div>
                  <div className="mt-0.5 text-[12px] text-ink-3">
                    {qtd(a.quantidade)} {TIPOS_TOP[pref.tipo].unidade === "documentos" && a.quantidade === 1 ? "documento" : TIPOS_TOP[pref.tipo].unidade}
                    {q.data && q.data.total_periodo > 0 && ` · ${Math.round((a.total / q.data.total_periodo) * 100)}% das vendas`}
                  </div>
                </div>
                <span className="num text-[13.5px] font-medium">{kz(a.total)}</span>
              </li>
            );
          })}
        </ol>
      ) : q.data ? (
        <p className="py-8 text-center text-ink-3">Sem vendas {PERIODOS_TOP[pref.periodo]}.</p>
      ) : (
        <Esqueleto />
      )}
    </Card>
  );
}

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  CalendarClock,
  CalendarX2,
  CheckCircle2,
  ChevronRight,
  CircleDollarSign,
  FileClock,
  FilePlus2,
  HandCoins,
  Hourglass,
  PackageMinus,
  Receipt,
  Send,
  ShieldAlert,
  ShoppingCart,
  Table2,
  TrendingUp,
  Truck,
  type LucideIcon,
} from "lucide-react";
import { call } from "@/lib/api";
import { data, dataHora, hojeISO, kz, qtd } from "@/lib/format";
import { Badge, Button, Card, Delta, PageHeader, Stat, Table, Td, Th, cn } from "@/components/ui";
import { BarChart } from "@/components/BarChart";
import DocumentoModal from "@/components/DocumentoModal";

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
  const [doc, setDoc] = useState<number | null>(null);
  const [tabela, setTabela] = useState(false);
  const { data: d } = useQuery({ queryKey: ["dashboard"], queryFn: () => call<Dash>("dashboard"), refetchInterval: 60_000 });
  // enquanto carrega não se mostram zeros nem "sem documentos" (pareceria que não há dados)
  const v = (x: number | undefined) => (d ? kz(x) : "…");
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
          { n: al?.fe_rejeitadas ?? 0, texto: ["factura rejeitada pela AGT", "facturas rejeitadas pela AGT"], to: "/facturacao-electronica", tom: "danger", icon: ShieldAlert },
          { n: al?.lotes_expirados ?? 0, texto: ["lote expirado ainda com stock", "lotes expirados ainda com stock"], to: "/relatorios/stock?situacao=validade", tom: "danger", icon: CalendarX2 },
          { n: al?.fe_pendentes ?? 0, texto: ["factura por validar na AGT", "facturas por validar na AGT"], to: "/facturacao-electronica", tom: "accent", icon: Send },
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
            <Button variant="secondary" onClick={() => nav("/documentos/novo/FT")}>
              <FilePlus2 className="size-4" /> Nova factura
            </Button>
            <Button onClick={() => nav("/pos")}>
              <ShoppingCart className="size-4" /> Abrir POS
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-4 gap-4">
        <Stat
          label="Vendas de hoje"
          value={v(d?.vendas_hoje)}
          delta={d && <Delta atual={d.vendas_hoje} anterior={d.vendas_ontem ?? 0} periodo="vs ontem" />}
          hint={d ? `${d.docs_hoje} ${d.docs_hoje === 1 ? "documento" : "documentos"}${d.vendas_ontem !== undefined ? ` · ontem ${kz(d.vendas_ontem)}` : ""}` : "…"}
          icon={<CircleDollarSign className="size-4" />}
        />
        <Stat
          label="Vendas do mês"
          value={v(d?.vendas_mes)}
          delta={d && <Delta atual={d.vendas_mes} anterior={d.vendas_mes_anterior ?? 0} periodo="vs mês anterior" />}
          hint={`IVA liquidado ${v(d?.iva_mes)} · ticket médio ${v(d?.ticket_medio)}`}
          icon={<TrendingUp className="size-4" />}
        />
        <Stat
          label="A receber de clientes"
          value={v(d?.a_receber)}
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
        />
        <Stat
          label="A pagar a fornecedores"
          value={v(d?.a_pagar)}
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

      <div className="mt-4 grid grid-cols-[minmax(0,1.7fr)_minmax(0,1fr)] gap-4">
        <Card className="p-5">
          <div className="mb-3 flex items-center justify-between">
            <div>
              <div className="font-semibold">Vendas — últimos 30 dias</div>
              <div className="text-[13px] text-ink-3">Valores com IVA, líquidos de notas de crédito</div>
            </div>
            <Button variant="ghost" size="sm" onClick={() => setTabela(!tabela)}>
              <Table2 className="size-4" /> {tabela ? "Gráfico" : "Tabela"}
            </Button>
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
            <BarChart dados={serie} formatar={kz} />
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

      <div className="mt-4 grid grid-cols-3 gap-4">
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

        <Card className="p-5">
          <div className="mb-3 font-semibold">Artigos mais vendidos (mês)</div>
          {d?.top_artigos.length ? (
            <ol className="space-y-2.5">
              {d.top_artigos.slice(0, 6).map((a, i) => (
                <li key={a.referencia} className="flex items-center gap-3">
                  <span className="num grid size-6 shrink-0 place-items-center rounded-md bg-sunken text-[12px] text-ink-2">{i + 1}</span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13.5px]">{a.descricao}</div>
                    <div className="text-[12px] text-ink-3">{qtd(a.quantidade)} unid.</div>
                  </div>
                  <span className="num text-[13.5px] font-medium">{kz(a.total)}</span>
                </li>
              ))}
            </ol>
          ) : d ? (
            <p className="py-8 text-center text-ink-3">Sem vendas este mês.</p>
          ) : (
            <Esqueleto />
          )}
        </Card>
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

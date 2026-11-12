import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { AlertTriangle, CircleDollarSign, FilePlus2, HandCoins, Receipt, ShoppingCart, Table2, TrendingUp } from "lucide-react";
import { call } from "@/lib/api";
import { data, dataHora, hojeISO, kz, qtd } from "@/lib/format";
import { Badge, Button, Card, PageHeader, Stat, Table, Td, Th } from "@/components/ui";
import { BarChart } from "@/components/BarChart";
import DocumentoModal from "@/components/DocumentoModal";

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
        <Stat label="Vendas de hoje" value={v(d?.vendas_hoje)} hint={d ? `${d.docs_hoje} ${d.docs_hoje === 1 ? "documento" : "documentos"}` : "…"} icon={<CircleDollarSign className="size-4" />} />
        <Stat label="Vendas do mês" value={v(d?.vendas_mes)} hint={`IVA liquidado ${v(d?.iva_mes)}`} icon={<TrendingUp className="size-4" />} />
        <Stat
          label="A receber de clientes"
          value={v(d?.a_receber)}
          hint={
            <button className="text-left underline-offset-2 hover:underline" onClick={() => nav("/contas/pagar")}>
              A pagar a fornecedores: {v(d?.a_pagar)}
            </button>
          }
          icon={<HandCoins className="size-4" />}
          tone="accent"
        />
        <Stat
          label="Artigos abaixo do mínimo"
          value={d ? d.stock_baixo : "…"}
          hint={<button className="text-left underline-offset-2 hover:underline" onClick={() => nav("/stock")}>Ver stock</button>}
          icon={<AlertTriangle className="size-4" />}
          tone={d?.stock_baixo ? "danger" : "ok"}
        />
      </div>

      <div className="mt-4 grid grid-cols-[minmax(0,1.7fr)_minmax(0,1fr)] gap-4">
        <Card className="p-5">
          <div className="mb-3 flex items-center justify-between">
            <div>
              <div className="font-semibold">Vendas — últimos 30 dias</div>
              <div className="text-[13px] text-ink-3">Valores com IVA, líquidos de notas de crédito · ticket médio {v(d?.ticket_medio)}</div>
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

        <Card className="p-5">
          <div className="mb-3 font-semibold">Artigos mais vendidos (mês)</div>
          {d?.top_artigos.length ? (
            <ol className="space-y-2.5">
              {d.top_artigos.map((a, i) => (
                <li key={a.referencia} className="flex items-center gap-3">
                  <span className="grid size-6 shrink-0 place-items-center rounded-md bg-sunken text-[12px] text-ink-2">{i + 1}</span>
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
            <div className="h-[200px] animate-pulse rounded-lg bg-sunken/60" />
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

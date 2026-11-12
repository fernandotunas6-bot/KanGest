import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CalendarClock, Layers, PackageCheck, Printer, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data, hojeISO, kz, qtd } from "@/lib/format";
import type { Lote } from "@/lib/types";
import { imprimir } from "@/lib/print";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Input, PageHeader, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";

const FILTROS = [
  { id: "todos", label: "Todos" },
  { id: "aviso", label: "A expirar" },
  { id: "expirados", label: "Expirados" },
] as const;

/** Estado de validade de um lote face ao aviso definido no artigo. */
function estado(l: Lote): "expirado" | "aviso" | "ok" | "sem" {
  if (l.dias_validade === null || l.dias_validade === undefined) return "sem";
  if (l.dias_validade < 0) return "expirado";
  if (l.dias_validade <= (l.aviso_validade_dias ?? 30)) return "aviso";
  return "ok";
}

export default function Lotes() {
  const t = useTabelas().data;
  const sessao = useSession((s) => s.sessao);
  const [q, setQ] = useState("");
  const [armazem, setArmazem] = useState("");
  const [filtro, setFiltro] = useState<(typeof FILTROS)[number]["id"]>("todos");
  const [comStock, setComStock] = useState(true);
  const lista = useQuery({
    queryKey: ["lotes", armazem, comStock],
    queryFn: () => call<Lote[]>("lotes.listar", { armazem_id: armazem ? Number(armazem) : null, com_stock: comStock }),
  });
  const todos = lista.data ?? [];
  const linhas = useMemo(() => {
    const termo = q.trim().toLowerCase();
    return todos.filter((l) => {
      const e = estado(l);
      if (filtro === "aviso" && e !== "aviso") return false;
      if (filtro === "expirados" && e !== "expirado") return false;
      return !termo || [l.descricao, l.referencia, l.lote].some((x) => x?.toLowerCase().includes(termo));
    });
  }, [todos, q, filtro]);
  const expirados = todos.filter((l) => estado(l) === "expirado" && l.quantidade > 0);
  const aviso = todos.filter((l) => estado(l) === "aviso" && l.quantidade > 0);
  const valor = todos.reduce((s, l) => s + Math.round(l.quantidade * l.custo), 0);

  const corpo = (
    <>
      <thead>
        <tr>
          <Th>Artigo</Th>
          <Th>Lote</Th>
          <Th>Armazém</Th>
          <Th>Validade</Th>
          <Th className="text-right">Quantidade</Th>
          <Th className="text-right">Custo</Th>
          <Th>Última entrada</Th>
          <Th>Última saída</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => {
          const e = estado(l);
          return (
            <tr key={l.id} className={cn(l.quantidade <= 0 && "opacity-55")}>
              <Td>
                <div className="font-medium">{l.descricao}</div>
                <div className="font-mono text-[11.5px] text-ink-3">{l.referencia}</div>
              </Td>
              <Td className="font-mono text-[12.5px]">{l.lote}</Td>
              <Td className="text-ink-2">{l.armazem}</Td>
              <Td>
                {l.validade ? (
                  <div className="flex items-center gap-2">
                    <span className="num">{data(l.validade)}</span>
                    {e === "expirado" && <Badge tone="danger">Expirado há {Math.abs(l.dias_validade ?? 0)} d</Badge>}
                    {e === "aviso" && <Badge tone="accent">{l.dias_validade === 0 ? "Expira hoje" : `Faltam ${l.dias_validade} d`}</Badge>}
                  </div>
                ) : (
                  <span className="text-ink-3">—</span>
                )}
              </Td>
              <Td className="num text-right font-medium">{qtd(l.quantidade)}</Td>
              <Td className="num text-right text-ink-2">{kz(l.custo)}</Td>
              <Td className="text-ink-2">{data(l.ultima_entrada)}</Td>
              <Td className="text-ink-2">{data(l.ultima_saida)}</Td>
            </tr>
          );
        })}
      </tbody>
    </>
  );

  return (
    <>
      <PageHeader
        title="Lotes de Artigo"
        subtitle="Existências por lote e controlo de datas de validade"
        actions={
          <Button variant="secondary" onClick={imprimir} disabled={!linhas.length}>
            <Printer className="size-4" /> Imprimir
          </Button>
        }
      />
      <div className="mb-4 grid grid-cols-4 gap-4">
        <Stat label="Lotes com stock" value={todos.filter((l) => l.quantidade > 0).length} icon={<Layers className="size-4" />} />
        <Stat label="A expirar (aviso do artigo)" value={aviso.length} tone="accent" icon={<CalendarClock className="size-4" />} />
        <Stat label="Expirados com stock" value={expirados.length} tone="danger" icon={<AlertTriangle className="size-4" />} />
        <Stat label="Valor em lotes (custo)" value={kz(valor)} tone="ok" icon={<PackageCheck className="size-4" />} />
      </div>
      <Card>
        <div className="flex flex-wrap items-center gap-3 border-b border-line p-4">
          <div className="relative min-w-64 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Artigo, referência ou lote…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Select className="w-48" value={armazem} onChange={(e) => setArmazem(e.target.value)}>
            <option value="">Todos os armazéns</option>
            {t?.armazens.map((a) => (
              <option key={a.id} value={a.id}>
                {a.nome}
              </option>
            ))}
          </Select>
          <div className="flex gap-1 rounded-lg bg-sunken p-1">
            {FILTROS.map((f) => (
              <button
                key={f.id}
                onClick={() => setFiltro(f.id)}
                className={cn("rounded-md px-3 py-1.5 text-[13px]", filtro === f.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
              >
                {f.label}
              </button>
            ))}
          </div>
          <label className="flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={comStock} onChange={(e) => setComStock(e.target.checked)} /> Só com stock
          </label>
        </div>
        {!linhas.length ? (
          <Empty
            icon={<Layers className="size-6" />}
            title={lista.isLoading ? "A carregar…" : "Sem lotes"}
            text="Os lotes são criados nas entradas de artigos com controlo de lotes (compras, documentos de stock e inventário)."
          />
        ) : (
          <Table className="max-h-[calc(100vh-330px)]">{corpo}</Table>
        )}
      </Card>
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <div className="mb-1 text-[16px] font-bold">Lotes de artigo</div>
          <div className="mb-3">
            {FILTROS.find((f) => f.id === filtro)?.label} · emitido por {sessao?.nome} em {data(hojeISO())}
          </div>
          <table className="w-full border-collapse text-left">{corpo}</table>
        </div>
      </PrintPortal>
    </>
  );
}

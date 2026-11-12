import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { ChevronDown, FileText, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { data, hojeISO, inicioMesISO, kz } from "@/lib/format";
import { MENU } from "@/lib/menu";
import type { DocResumo, Grupo } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Input, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import DocumentoModal from "@/components/DocumentoModal";

const GRUPOS: { id: "" | Grupo; label: string; menu?: string }[] = [
  { id: "", label: "Todos" },
  { id: "VENDAS", label: "Vendas", menu: "vendas" },
  { id: "COMPRAS", label: "Compras", menu: "compras" },
  { id: "STOCK", label: "Stock", menu: "stock" },
];

export default function Documentos() {
  const [sp, setSp] = useSearchParams();
  const grupo = (sp.get("grupo") ?? "") as "" | Grupo;
  const [f, setF] = useState({ tipo: "", de: inicioMesISO(), ate: hojeISO(), q: "" });
  const [aberto, setAberto] = useState<number | null>(null);
  // ?tipo=RC: «Recibos Emitidos», «Recibos Detalhes»… abrem a lista já filtrada
  useEffect(() => {
    setF((x) => ({ ...x, tipo: sp.get("tipo") ?? "" }));
  }, [sp]);
  const [menu, setMenu] = useState(false);
  const dq = useDebounce(f.q);
  const tabelas = useTabelas();
  const tem = useSession((s) => s.tem);
  const nav = useNavigate();
  const lista = useQuery({
    queryKey: ["documentos", grupo, f.tipo, f.de, f.ate, dq],
    queryFn: () => call<DocResumo[]>("documentos.listar", { ...f, q: dq, grupo: grupo || null }),
  });
  // O total só soma o que se pode somar: com um tipo escolhido, esse tipo; senão as vendas (ou as compras),
  // sem proformas, guias nem movimentos de stock. As notas de crédito subtraem.
  const classeTotal = f.tipo ? null : grupo === "COMPRAS" ? "COMPRA" : grupo === "STOCK" ? "STOCK" : "VENDA";
  const total =
    lista.data
      ?.filter((d) => d.estado !== "A" && (!classeTotal || d.classe === classeTotal))
      .reduce((s, d) => s + (["NC", "VNC"].includes(d.tipo_codigo) ? -d.total : d.total), 0) ?? 0;
  const rotuloTotal = f.tipo ? `Total de ${f.tipo} no período` : classeTotal === "COMPRA" ? "Total de compras no período" : classeTotal === "STOCK" ? "Total movimentado no período" : "Total facturado no período";
  const hoje = hojeISO();

  // "Novo documento": os mesmos itens do menu do grupo (como na barra do KIGEST GC)
  const novos = MENU.filter((m) => !grupo || GRUPOS.find((g) => g.id === grupo)?.menu === m.id)
    .filter((m) => ["vendas", "compras", "stock"].includes(m.id))
    .flatMap((m) => m.itens.filter((i) => !!i.to && (i.to.startsWith("/documentos/novo/") || ["/recibos/novo", "/pagamentos/novo", "/adiantamentos/novo"].includes(i.to)) && !i.emBreve && (!i.perm || tem(i.perm))).map((i) => ({ ...i, grupo: m.titulo })));
  const tiposGrupo = tabelas.data?.tipos_documento.filter((t) => !grupo || t.grupo === grupo);

  return (
    <>
      <PageHeader
        title="Documentos Emitidos"
        subtitle={grupo ? `${GRUPOS.find((g) => g.id === grupo)?.label}` : "Vendas, compras, recibos, pagamentos e movimentos de stock"}
        actions={
          <div className="relative">
            <Button onClick={() => setMenu(!menu)}>
              Novo documento <ChevronDown className="size-4" />
            </Button>
            {menu && (
              <div className="absolute right-0 z-20 mt-2 max-h-[70vh] w-72 overflow-y-auto rounded-xl border border-line bg-surface p-1.5 shadow-pop" onMouseLeave={() => setMenu(false)}>
                {novos.map((n, k) => (
                  <div key={n.label + n.grupo}>
                    {(k === 0 || novos[k - 1].grupo !== n.grupo) && <div className="px-3 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-ink-3">{n.grupo}</div>}
                    <button onClick={() => nav(n.to!)} className="flex w-full items-center rounded-lg px-3 py-2 text-left hover:bg-sunken">
                      {n.label}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        }
      />
      <div className="mb-4 inline-flex rounded-lg bg-sunken p-1">
        {GRUPOS.map((g) => (
          <button
            key={g.id}
            onClick={() => {
              setF({ ...f, tipo: "" });
              setSp(g.id ? { grupo: g.id } : {});
            }}
            className={cn("rounded-md px-4 py-1.5 text-[13px]", grupo === g.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
          >
            {g.label}
          </button>
        ))}
      </div>
      <Card>
        <div className="flex flex-wrap gap-3 border-b border-line p-4">
          <div className="relative min-w-64 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Número, entidade, NIF ou n.º do fornecedor…" value={f.q} onChange={(e) => setF({ ...f, q: e.target.value })} />
          </div>
          <Select className="w-64" value={f.tipo} onChange={(e) => setF({ ...f, tipo: e.target.value })}>
            <option value="">Todos os tipos</option>
            {tiposGrupo?.map((t) => (
              <option key={t.codigo} value={t.codigo}>
                {t.codigo} — {t.descricao}
              </option>
            ))}
          </Select>
          <Input type="date" className="w-40" value={f.de} onChange={(e) => setF({ ...f, de: e.target.value })} />
          <Input type="date" className="w-40" value={f.ate} onChange={(e) => setF({ ...f, ate: e.target.value })} />
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<FileText className="size-6" />} title="Sem documentos no período" />
        ) : (
          <Table className="max-h-[calc(100vh-330px)]">
            <thead>
              <tr>
                <Th>Documento</Th>
                <Th>Data</Th>
                <Th>Entidade</Th>
                <Th>Operador</Th>
                <Th>Estado</Th>
                <Th className="text-right">Total</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((d) => (
                <tr key={d.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setAberto(d.id)}>
                  <Td>
                    <div className="font-medium">
                      {d.numero_doc}
                      {!!d.importado && <Badge className="ml-2">Histórico KIGEST</Badge>}
                    </div>
                    <div className="text-[12px] text-ink-3">
                      {d.tipo_desc}
                      {d.doc_externo ? ` · ${d.doc_externo}` : ""}
                    </div>
                  </Td>
                  <Td className="text-ink-2">{data(d.data_emissao)}</Td>
                  <Td>
                    <div>{d.entidade_nome}</div>
                    <div className="font-mono text-[11.5px] text-ink-3">{d.entidade_nif}</div>
                  </Td>
                  <Td className="text-ink-2">{d.utilizador}</Td>
                  <Td>
                    {d.estado === "A" ? (
                      <Badge tone="danger">Anulado</Badge>
                    ) : d.estado === "F" ? (
                      <Badge tone="brand">{d.classe === "PROFORMA" ? "Facturada" : "Facturado"}</Badge>
                    ) : d.classe === "PROFORMA" ? (
                      d.data_vencimento && d.data_vencimento.slice(0, 10) < hoje ? (
                        <Badge>Expirada {data(d.data_vencimento)}</Badge>
                      ) : (
                        <Badge tone="accent">Válida até {data(d.data_vencimento ?? d.data_emissao)}</Badge>
                      )
                    ) : d.valor_pendente > 0 ? (
                      <Badge tone="accent">Em dívida {kz(d.valor_pendente)}</Badge>
                    ) : (
                      <Badge tone="ok">Normal</Badge>
                    )}
                  </Td>
                  <Td className="num text-right font-medium">
                    {["NC", "VNC"].includes(d.tipo_codigo) ? "−" : ""}
                    {kz(d.total)}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        <div className="flex justify-between border-t border-line px-5 py-3 text-[13px] text-ink-2">
          <span>{lista.data?.length ?? 0} documentos</span>
          <span>
            {rotuloTotal}: <b className="num text-ink">{kz(total)}</b>
          </span>
        </div>
      </Card>
      <DocumentoModal id={aberto} onClose={() => setAberto(null)} />
    </>
  );
}

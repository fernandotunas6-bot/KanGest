import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Search, Undo2 } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, kz } from "@/lib/format";
import type { DocCompleto, DocResumo } from "@/lib/types";
import { Button, Card, Empty, Field, Input, PageHeader, Table, Td, Th, cn } from "@/components/ui";

/** Estorno de Recibo (Fr_EstornoRecibo) de cliente (RC) ou de fornecedor (VRC). */
export default function Estorno({ tipo }: { tipo: "RC" | "VRC" }) {
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const [sel, setSel] = useState<number | null>(null);
  const [motivo, setMotivo] = useState("");
  const [busy, setBusy] = useState(false);
  const lista = useQuery({ queryKey: ["estorno", tipo, dq], queryFn: () => call<DocResumo[]>("documentos.listar", { tipo, q: dq, estado: "N", limite: 200 }) });
  const det = useQuery({ queryKey: ["doc", sel], queryFn: () => call<DocCompleto>("documentos.obter", { id: sel }), enabled: !!sel }).data;
  const fornecedor = tipo === "VRC";

  async function estornar() {
    if (!sel) return;
    setBusy(true);
    try {
      await call("documentos.anular", { id: sel, motivo });
      toast.success("Recibo estornado: os documentos voltaram a ficar em dívida.");
      qc.invalidateQueries();
      setSel(null);
      setMotivo("");
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title={fornecedor ? "Estorno de Pagamento a Fornecedor" : "Estorno de Recibo"}
        subtitle="Anula o recibo e repõe em dívida os documentos que ele liquidou"
      />
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4">
        <Card>
          <div className="border-b border-line p-4">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
              <Input className="pl-9" placeholder="N.º do recibo ou entidade…" value={q} onChange={(e) => setQ(e.target.value)} />
            </div>
          </div>
          {!lista.data?.length ? (
            <Empty icon={<Undo2 className="size-6" />} title="Sem recibos válidos" />
          ) : (
            <Table className="max-h-[calc(100vh-280px)]">
              <thead>
                <tr>
                  <Th>Recibo</Th>
                  <Th>Data</Th>
                  <Th>Entidade</Th>
                  <Th className="text-right">Valor</Th>
                </tr>
              </thead>
              <tbody>
                {lista.data.map((d) => (
                  <tr key={d.id} className={cn("cursor-pointer hover:bg-sunken/60", sel === d.id && "bg-brand-50")} onClick={() => setSel(d.id)}>
                    <Td className="font-medium">{d.numero_doc}</Td>
                    <Td className="text-ink-2">{data(d.data_emissao)}</Td>
                    <Td className="text-ink-2">{d.entidade_nome}</Td>
                    <Td className="num text-right">{kz(d.total)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>
        <Card className="h-fit p-5">
          {!det ? (
            <p className="py-10 text-center text-ink-3">Seleccione o recibo a estornar.</p>
          ) : (
            <>
              <div className="mb-1 text-[17px] font-semibold">{det.documento.numero_doc}</div>
              <div className="mb-4 text-ink-2">
                {det.documento.entidade_nome} · {data(det.documento.data_emissao)} · {kz(det.documento.total)}
              </div>
              <div className="mb-1 text-[12px] font-medium uppercase tracking-wide text-ink-2">Documentos liquidados</div>
              <table className="mb-4 w-full text-[13px]">
                <tbody>
                  {det.liquidacoes.map((l) => (
                    <tr key={l.id} className="border-b border-line/70">
                      <td className="py-1.5">{l.numero_doc}</td>
                      <td className="py-1.5 text-ink-3">{data(l.data_emissao)}</td>
                      <td className="num py-1.5 text-right">{kz(l.valor)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="mb-4 text-[13px] text-ink-2">
                Pagamento: {det.pagamentos.map((p) => `${p.descricao} ${kz(p.valor)}`).join(" · ") || "—"}
              </div>
              <Field label="Motivo do estorno">
                <Input value={motivo} placeholder="Ex.: cheque devolvido, recibo emitido por engano" onChange={(e) => setMotivo(e.target.value)} />
              </Field>
              <Button variant="danger" className="mt-4 w-full" loading={busy} disabled={motivo.trim().length < 5} onClick={estornar}>
                <Undo2 className="size-4" /> Estornar recibo
              </Button>
              <p className="mt-3 text-[12px] text-ink-3">O recibo fica anulado (estado A no SAF-T) e o dinheiro sai do fluxo de caixa. Os créditos compensados voltam a ficar disponíveis.</p>
            </>
          )}
        </Card>
      </div>
    </>
  );
}

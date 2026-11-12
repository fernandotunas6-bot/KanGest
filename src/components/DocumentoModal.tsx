import { Fragment, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Ban, Printer, Receipt, FileText, Truck, Undo2 } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas, useTalao58 } from "@/lib/hooks";
import { imprimir, NOMES_VIAS } from "@/lib/print";
import type { DocCompleto } from "@/lib/types";
import { useSession } from "@/store/session";
import { A4, NotaEntrega, PrintPortal, Ticket } from "./DocumentoPrint";
import { Badge, Button, Field, Input, Modal, Select, cn } from "./ui";

export { imprimir };

export function useDocumento(id: number | null) {
  return useQuery({ queryKey: ["documento", id], queryFn: () => call<DocCompleto>("documentos.obter", { id }), enabled: !!id });
}

type Formato = "a4" | "ticket";

/** N cópias do documento, cada uma com a sua designação (Original, Duplicado…). */
export function Copias({ d, formato, vias, segundaVia }: { d: DocCompleto; formato: Formato; vias: number; segundaVia?: boolean }) {
  return (
    <>
      {formato === "a4" && d.documento.formato === "A5" && <style>{"@media print { @page { size: A5; margin: 7mm; } }"}</style>}
      {Array.from({ length: Math.max(1, vias) }, (_, i) => {
        const nome = segundaVia ? `2.ª Via — ${NOMES_VIAS[i] ?? `Cópia ${i + 1}`}` : (NOMES_VIAS[i] ?? `Cópia ${i + 1}`);
        return (
          <Fragment key={i}>
            {formato === "a4" ? <A4 d={d} via={nome} /> : <Ticket d={d} via={nome} />}
            {i < vias - 1 && <div className={formato === "a4" ? "break-after-page" : "h-10"} />}
          </Fragment>
        );
      })}
    </>
  );
}

/** Impressão directa (sem pré-visualização), usada pelo POS quando "imprimir automaticamente" está activo. */
export function ImpressaoAutomatica({ id, formato, vias, onDone }: { id: number; formato: Formato; vias: number; onDone: () => void }) {
  const { data: d } = useDocumento(id);
  useEffect(() => {
    if (!d) return;
    imprimir();
    const t = setTimeout(onDone, 1500);
    return () => clearTimeout(t);
  }, [d, onDone]);
  if (!d) return null;
  return (
    <PrintPortal>
      <Copias d={d} formato={formato} vias={vias} />
    </PrintPortal>
  );
}

export default function DocumentoModal({
  id,
  onClose,
  formatoInicial,
}: {
  id: number | null;
  onClose: () => void;
  formatoInicial?: Formato;
}) {
  const { data: d } = useDocumento(id);
  const opcoes = useTabelas().data?.posto?.opcoes;
  const talao58 = useTalao58();
  // impressão da nota de entrega (sem preços) em vez do documento
  const [notaEntrega, setNotaEntrega] = useState(false);
  const [formato, setFormato] = useState<Formato>(formatoInicial ?? opcoes?.formato ?? "a4");
  const [vias, setVias] = useState(1);
  const [segundaVia, setSegundaVia] = useState(false);
  const [anular, setAnular] = useState(false);
  const [motivo, setMotivo] = useState("");
  const tem = useSession((s) => s.tem);
  const qc = useQueryClient();
  const nav = useNavigate();

  // n.º de vias: o do posto (POS) ou o do tipo de documento
  useEffect(() => {
    if (d) setVias(formatoInicial === "ticket" ? (opcoes?.vias ?? 1) : d.documento.vias || 1);
  }, [d, formatoInicial, opcoes?.vias]);

  // Com impressão térmica directa configurada, o talão vai em ESC/POS (corta o papel e abre a gaveta).
  const termica = !!opcoes?.escpos?.ativo && formato === "ticket";
  async function imprimirDoc() {
    setNotaEntrega(false);
    if (!termica) return imprimir();
    try {
      await call("impressao.talao", { id, vias, segunda_via: segundaVia });
      toast.success("Talão enviado à impressora térmica.");
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function confirmarAnulacao() {
    try {
      await call("documentos.anular", { id, motivo });
      toast.success("Documento anulado.");
      setAnular(false);
      qc.invalidateQueries();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <Modal
      open={!!id}
      onClose={onClose}
      title={d ? `${d.documento.tipo_desc} ${d.documento.numero_doc}` : "Documento"}
      width="max-w-4xl"
      footer={
        d && (
          <>
            {tem("documentos.anular") && d.documento.estado === "N" && !d.documento.importado && (
              <Button variant="ghost" className="mr-auto text-danger" onClick={() => setAnular(true)}>
                <Ban className="size-4" /> Anular
              </Button>
            )}
            {d.documento.estado === "N" && ["PROFORMA", "TRANSPORTE"].includes(d.documento.classe) && tem("vendas.fatura") && (
              <Button variant="secondary" onClick={() => nav(`/documentos/novo/FT?origem=${d.documento.id}`)}>
                <FileText className="size-4" /> Converter em factura
              </Button>
            )}
            {d.documento.estado === "N" && d.documento.classe === "COMPRA_DOC" && tem("compras.fatura") && (
              <Button variant="secondary" onClick={() => nav(`/documentos/novo/FC?origem=${d.documento.id}`)}>
                <FileText className="size-4" /> Converter em V/Factura
              </Button>
            )}
            {tem("vendas.nota_credito") && d.documento.estado === "N" && ["FT", "FR"].includes(d.documento.tipo_codigo) && (
              <Button variant="secondary" onClick={() => nav(`/documentos/novo/NC?origem=${d.documento.id}`)}>
                <Undo2 className="size-4" /> Nota de crédito
              </Button>
            )}
            {tem("compras.fatura") && d.documento.estado === "N" && ["FC", "VFR"].includes(d.documento.tipo_codigo) && (
              <Button variant="secondary" onClick={() => nav(`/documentos/novo/VNC?origem=${d.documento.id}`)}>
                <Undo2 className="size-4" /> V/Nota de crédito
              </Button>
            )}
            {d.documento.grupo === "VENDAS" && d.documento.estado !== "A" && d.linhas.length > 0 && !["RC", "RA"].includes(d.documento.tipo_codigo) && (
              <Button
                variant="secondary"
                title="Artigos e quantidades, sem preços, com assinaturas de entrega (no formato escolhido: A4 ou talão)"
                onClick={() => {
                  setNotaEntrega(true);
                  imprimir();
                }}
              >
                <Truck className="size-4" /> Nota de entrega
              </Button>
            )}
            <label className="flex items-center gap-2 text-[13px] text-ink-2">
              Vias
              <Select className="h-8 w-16" value={vias} onChange={(e) => setVias(Number(e.target.value))}>
                {[1, 2, 3, 4].map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </Select>
            </label>
            <label className="mr-2 flex items-center gap-2 text-[13px] text-ink-2">
              <input type="checkbox" checked={segundaVia} onChange={(e) => setSegundaVia(e.target.checked)} /> 2.ª via
            </label>
            <Button onClick={imprimirDoc}>
              <Printer className="size-4" /> {termica ? "Imprimir na térmica" : "Imprimir"}
            </Button>
          </>
        )
      }
    >
      {!d ? (
        <div className="py-10 text-center text-ink-3">A carregar…</div>
      ) : (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <div className="inline-flex rounded-lg bg-sunken p-1">
              {(["a4", "ticket"] as const).map((f) => (
                <button
                  key={f}
                  onClick={() => setFormato(f)}
                  className={cn("flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[13px]", formato === f ? "bg-surface font-medium shadow-sm" : "text-ink-2")}
                >
                  {f === "a4" ? <FileText className="size-4" /> : <Receipt className="size-4" />}
                  {f === "a4" ? "A4" : `Talão ${talao58 ? 58 : 80} mm`}
                </button>
              ))}
            </div>
            {termica && <Badge tone="accent">Impressão directa (ESC/POS)</Badge>}
            {d.documento.estado === "A" && <Badge tone="danger">Anulado: {d.documento.motivo_anulacao}</Badge>}
            {d.documento.hash_controlo && <Badge tone="brand">Assinado · {d.documento.hash_controlo}</Badge>}
            {d.documento.autorizado_por_nome && <Badge tone="accent">Autorizado por {d.documento.autorizado_por_nome}</Badge>}
          </div>
          <div className="max-h-[60vh] overflow-auto rounded-xl border border-line bg-sunken p-6">
            <div className="mx-auto w-fit bg-white shadow-card">
              {formato === "a4" ? <A4 d={d} via={segundaVia ? "2.ª Via" : "Original"} /> : <Ticket d={d} via={segundaVia ? "2.ª Via" : "Original"} />}
            </div>
          </div>
          <PrintPortal>
            {notaEntrega ? <NotaEntrega d={d} talao={formato === "ticket"} /> : <Copias d={d} formato={formato} vias={vias} segundaVia={segundaVia} />}
          </PrintPortal>

          <Modal
            open={anular}
            onClose={() => setAnular(false)}
            title="Anular documento"
            width="max-w-md"
            footer={
              <>
                <Button variant="secondary" onClick={() => setAnular(false)}>
                  Cancelar
                </Button>
                <Button variant="danger" onClick={confirmarAnulacao}>
                  Anular
                </Button>
              </>
            }
          >
            <p className="mb-4 text-ink-2">
              A anulação repõe o stock e fica registada na auditoria e no SAF-T. Para devoluções parciais use uma Nota de Crédito.
            </p>
            <Field label="Motivo">
              <Input autoFocus value={motivo} onChange={(e) => setMotivo(e.target.value)} />
            </Field>
          </Modal>
        </>
      )}
    </Modal>
  );
}

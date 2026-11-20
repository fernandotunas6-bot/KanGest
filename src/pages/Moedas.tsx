import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Coins, Plus } from "lucide-react";
import { call } from "@/lib/api";
import type { Moeda } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Table, Td, Th } from "@/components/ui";
import { numero } from "@/lib/format";

const fmtCambio = { format: (v: number) => numero(v, 6) };

/** Moedas e câmbios (frmMoeda / frmAlterarCambio do KIGEST). */
export default function Moedas() {
  const tem = useSession((s) => s.tem);
  const lista = useQuery({ queryKey: ["moedas"], queryFn: () => call<Moeda[]>("moedas.listar") });
  const [editar, setEditar] = useState<Partial<Moeda> | null>(null);
  const qc = useQueryClient();
  async function alternarPos(m: Moeda) {
    try {
      await call("moedas.guardar", { codigo: m.codigo, descricao: m.descricao, simbolo: m.simbolo, cambio: m.cambio, ativo: m.ativo !== 0, pos: !m.pos });
      toast.success(m.pos ? `${m.descricao} deixa de aparecer no pagamento do POS.` : `${m.descricao} passa a aparecer no pagamento do POS.`);
      qc.invalidateQueries({ queryKey: ["moedas"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <>
      <PageHeader
        title="Moedas e Câmbios"
        subtitle="Câmbio das compras e vendas em moeda estrangeira (Kz por 1 unidade) e moedas aceites no POS"
        actions={
          tem("compras.fatura") && (
            <Button onClick={() => setEditar({ cambio: 0, ativo: 1 })}>
              <Plus className="size-4" /> Nova moeda
            </Button>
          )
        }
      />
      <Card>
        <Table>
          <thead>
            <tr>
              <Th>Código</Th>
              <Th>Descrição</Th>
              <Th>Símbolo</Th>
              <Th className="text-right">Câmbio (Kz)</Th>
              <Th className="text-right">1 Kz =</Th>
              <Th>Estado</Th>
              <Th>Aceite no POS</Th>
            </tr>
          </thead>
          <tbody>
            {lista.data?.map((m) => (
              <tr key={m.codigo} className="cursor-pointer hover:bg-sunken/60" onClick={() => m.codigo !== "AOA" && tem("compras.fatura") && setEditar(m)}>
                <Td className="font-mono font-medium">{m.codigo}</Td>
                <Td>{m.descricao}</Td>
                <Td className="text-ink-2">{m.simbolo}</Td>
                <Td className="num text-right font-medium">
                  {m.cambio > 0 ? fmtCambio.format(m.cambio) : <Badge tone="accent">Sem câmbio</Badge>}
                </Td>
                <Td className="num text-right text-ink-2">{m.cambio > 0 && m.codigo !== "AOA" ? `${fmtCambio.format(1 / m.cambio)} ${m.simbolo}` : "—"}</Td>
                <Td>{m.codigo === "AOA" ? <Badge tone="brand">Moeda base</Badge> : m.ativo ? <Badge tone="ok">Activa</Badge> : <Badge>Inactiva</Badge>}</Td>
                <Td onClick={(e) => e.stopPropagation()}>
                  {m.codigo === "AOA" ? (
                    <span className="text-[12.5px] text-ink-3">sempre</span>
                  ) : (
                    <label className="flex items-center gap-2 text-[13px]" title="Aparece no pagamento do POS (o cliente paga nesta moeda e o troco sai em kwanzas)">
                      <input type="checkbox" disabled={!tem("compras.fatura")} checked={!!m.pos} onChange={() => alternarPos(m)} />
                      {m.pos ? "Sim" : <span className="text-ink-3">Não</span>}
                    </label>
                  )}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      </Card>
      {editar && <MoedaForm inicial={editar} onClose={() => setEditar(null)} />}
    </>
  );
}

function MoedaForm({ inicial, onClose }: { inicial: Partial<Moeda>; onClose: () => void }) {
  const qc = useQueryClient();
  const [m, setM] = useState(inicial);
  const [cambio, setCambio] = useState(String(inicial.cambio ?? 0).replace(".", ","));
  const [busy, setBusy] = useState(false);
  const novo = !inicial.codigo;
  async function guardar() {
    setBusy(true);
    try {
      await call("moedas.guardar", { ...m, cambio: Number(cambio.replace(/\s/g, "").replace(",", ".")) || 0, ativo: m.ativo !== 0, pos: !!m.pos });
      toast.success("Moeda guardada.");
      qc.invalidateQueries({ queryKey: ["moedas"] });
      qc.invalidateQueries({ queryKey: ["doc-auxiliares"] });
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={novo ? "Nova moeda" : `${m.codigo} — ${m.descricao}`}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={guardar}>
            Guardar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-3 gap-4">
        <Field label="Código ISO">
          <Input className="font-mono uppercase" maxLength={3} disabled={!novo} value={m.codigo ?? ""} onChange={(e) => setM({ ...m, codigo: e.target.value.toUpperCase() })} />
        </Field>
        <Field label="Descrição" className="col-span-2">
          <Input value={m.descricao ?? ""} onChange={(e) => setM({ ...m, descricao: e.target.value })} />
        </Field>
        <Field label="Símbolo">
          <Input value={m.simbolo ?? ""} onChange={(e) => setM({ ...m, simbolo: e.target.value })} />
        </Field>
        <Field label="Câmbio do dia (Kz)" className="col-span-2" hint="Quantos kwanzas vale 1 unidade desta moeda">
          <Input autoFocus={!novo} className="num text-right" value={cambio} onChange={(e) => setCambio(e.target.value)} />
        </Field>
      </div>
      <label className="mt-4 flex items-center gap-2 text-ink-2">
        <input type="checkbox" checked={m.ativo !== 0} onChange={(e) => setM({ ...m, ativo: e.target.checked ? 1 : 0 })} /> Moeda activa
      </label>
      <label className="mt-2 flex items-start gap-2 text-ink-2">
        <input type="checkbox" className="mt-1" checked={!!m.pos} onChange={(e) => setM({ ...m, pos: e.target.checked ? 1 : 0 })} />
        <span>
          Aceitar em numerário no POS
          <span className="block text-[12.5px] text-ink-3">Aparece no pagamento: o cliente paga nesta moeda ao câmbio do dia e o troco sai em kwanzas.</span>
        </span>
      </label>
      <p className="mt-4 flex items-start gap-2 rounded-lg bg-sunken px-3 py-2 text-[12.5px] text-ink-2">
        <Coins className="mt-0.5 size-4 shrink-0" /> O câmbio fica gravado em cada documento de compra; alterá-lo aqui não muda documentos já emitidos.
      </p>
    </Modal>
  );
}

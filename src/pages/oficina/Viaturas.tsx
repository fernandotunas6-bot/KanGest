import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Car, ClipboardPlus, Plus, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import type { Entidade, Viatura } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th } from "@/components/ui";
import { EntidadePicker } from "@/components/Pickers";
import { qtd } from "@/lib/format";

export const COMBUSTIVEIS = ["Gasolina", "Gasóleo", "Híbrido", "Eléctrico", "GPL"];
export const CATEGORIAS = ["Ligeiro de passageiros", "Ligeiro de mercadorias", "Pesado", "Motociclo", "Máquina / equipamento"];

export interface Sugestoes {
  marcas: string[];
  modelos: string[];
  cores: string[];
  tecnicos: string[];
}

export function useSugestoes() {
  return useQuery({ queryKey: ["oficina-sugestoes"], queryFn: () => call<Sugestoes>("oficina.sugestoes"), staleTime: 60_000 }).data;
}

export default function Viaturas() {
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const nav = useNavigate();
  const [editar, setEditar] = useState<Partial<Viatura> | null>(null);
  const lista = useQuery({ queryKey: ["viaturas", dq], queryFn: () => call<Viatura[]>("oficina.viaturas", { q: dq }) });
  return (
    <>
      <PageHeader
        title="Viaturas"
        subtitle="Registo de viaturas dos clientes da oficina"
        actions={
          <Button onClick={() => setEditar({ ativo: 1, km: 0 })}>
            <Plus className="size-4" /> Nova viatura
          </Button>
        }
      />
      <Card>
        <div className="border-b border-line p-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Matrícula, marca, modelo, chassi ou cliente…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<Car className="size-6" />} title="Sem viaturas" text="Registe a viatura do cliente para abrir ordens de serviço." />
        ) : (
          <Table className="max-h-[calc(100vh-260px)]">
            <thead>
              <tr>
                <Th>Matrícula</Th>
                <Th>Marca / Modelo</Th>
                <Th>Cor</Th>
                <Th>Ano</Th>
                <Th>Cliente</Th>
                <Th className="text-right">Km</Th>
                <Th className="text-right">Ordens</Th>
                <Th className="w-10" />
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((v) => (
                <tr key={v.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setEditar(v)}>
                  <Td className="font-mono font-medium">{v.matricula ?? "—"}</Td>
                  <Td>
                    <span className="font-medium">{v.marca}</span> <span className="text-ink-2">{v.modelo}</span>
                  </Td>
                  <Td className="text-ink-2">{v.cor ?? "—"}</Td>
                  <Td className="num text-ink-2">{v.ano ?? "—"}</Td>
                  <Td className="text-ink-2">{v.cliente ?? "—"}</Td>
                  <Td className="num text-right">{qtd(v.km ?? 0)}</Td>
                  <Td className="num text-right">{v.ordens ? <Badge tone="brand">{v.ordens}</Badge> : "—"}</Td>
                  <Td>
                    <button
                      title="Nova ordem de serviço"
                      className="rounded-md p-1.5 text-ink-3 hover:bg-brand-50 hover:text-brand"
                      onClick={(e) => {
                        e.stopPropagation();
                        nav(`/oficina/ordens?nova=${v.id}`);
                      }}
                    >
                      <ClipboardPlus className="size-4" />
                    </button>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {editar && <ViaturaForm inicial={editar} onClose={() => setEditar(null)} />}
    </>
  );
}

export function ViaturaForm({ inicial, onClose, onGuardada }: { inicial: Partial<Viatura>; onClose: () => void; onGuardada?: (id: number) => void }) {
  const qc = useQueryClient();
  const sug = useSugestoes();
  const [v, setV] = useState(inicial);
  const [cliente, setCliente] = useState<Entidade | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (k: keyof Viatura, x: unknown) => setV((o) => ({ ...o, [k]: x }));
  const clienteAtual = cliente ?? (v.entidade_id ? ({ id: v.entidade_id, nome: v.cliente ?? "", nif: v.cliente_nif ?? "" } as Entidade) : null);

  async function guardar() {
    setBusy(true);
    try {
      const r = await call<{ id: number }>("oficina.viaturaGuardar", {
        ...v,
        marca: v.marca ?? "",
        entidade_id: clienteAtual?.id ?? null,
        ano: v.ano ? Number(v.ano) : null,
        km: Number(v.km) || 0,
        ativo: v.ativo !== 0,
      });
      toast.success("Viatura guardada.");
      qc.invalidateQueries({ queryKey: ["viaturas"] });
      qc.invalidateQueries({ queryKey: ["oficina-sugestoes"] });
      onGuardada?.(r.id);
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
      title={v.id ? `${v.marca} ${v.modelo ?? ""} · ${v.matricula ?? "sem matrícula"}` : "Nova viatura"}
      width="max-w-3xl"
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
      <div className="grid grid-cols-4 gap-4">
        <div className="col-span-4">
          <EntidadePicker tipo="C" label="Proprietário (cliente)" valor={clienteAtual} onChange={setCliente} />
        </div>
        <Field label="Matrícula">
          <Input className="font-mono uppercase" autoFocus value={v.matricula ?? ""} placeholder="LD-00-00-AA" onChange={(e) => set("matricula", e.target.value.toUpperCase())} />
        </Field>
        <Field label="Marca">
          <Input list="of-marcas" value={v.marca ?? ""} onChange={(e) => set("marca", e.target.value)} />
        </Field>
        <Field label="Modelo">
          <Input list="of-modelos" value={v.modelo ?? ""} onChange={(e) => set("modelo", e.target.value)} />
        </Field>
        <Field label="Cor">
          <Input list="of-cores" value={v.cor ?? ""} onChange={(e) => set("cor", e.target.value)} />
        </Field>
        <Field label="Ano">
          <Input className="num" type="number" min={1950} max={2100} value={v.ano ?? ""} onChange={(e) => set("ano", e.target.value)} />
        </Field>
        <Field label="N.º de chassi (VIN)" className="col-span-2">
          <Input className="font-mono uppercase" value={v.chassi ?? ""} onChange={(e) => set("chassi", e.target.value.toUpperCase())} />
        </Field>
        <Field label="Motor">
          <Input value={v.motor ?? ""} placeholder="Ex.: 2.4 D-4D" onChange={(e) => set("motor", e.target.value)} />
        </Field>
        <Field label="Combustível">
          <Select value={v.combustivel ?? ""} onChange={(e) => set("combustivel", e.target.value)}>
            <option value="">—</option>
            {COMBUSTIVEIS.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </Select>
        </Field>
        <Field label="Categoria" className="col-span-2">
          <Select value={v.categoria ?? ""} onChange={(e) => set("categoria", e.target.value)}>
            <option value="">—</option>
            {CATEGORIAS.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </Select>
        </Field>
        <Field label="Quilometragem">
          <Input className="num text-right" type="number" min={0} value={v.km ?? 0} onChange={(e) => set("km", e.target.value)} />
        </Field>
        <Field label="Observações" className="col-span-4">
          <Input value={v.observacoes ?? ""} onChange={(e) => set("observacoes", e.target.value)} />
        </Field>
      </div>
      {v.id && (
        <label className="mt-4 flex items-center gap-2 text-ink-2">
          <input type="checkbox" checked={v.ativo !== 0} onChange={(e) => set("ativo", e.target.checked ? 1 : 0)} /> Viatura activa
        </label>
      )}
      <datalist id="of-marcas">{sug?.marcas.map((m) => <option key={m} value={m} />)}</datalist>
      <datalist id="of-modelos">{sug?.modelos.map((m) => <option key={m} value={m} />)}</datalist>
      <datalist id="of-cores">{sug?.cores.map((m) => <option key={m} value={m} />)}</datalist>
    </Modal>
  );
}

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Lock, LockOpen, Plus, Star, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data } from "@/lib/format";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

export interface Serie {
  id: number;
  tipo_codigo: string;
  tipo_desc: string;
  grupo: string;
  serie: string;
  descricao: string | null;
  data_inicio: string | null;
  data_fim: string | null;
  padrao: number;
  manual: number;
  ativa: number;
  posto_id: number | null;
  posto: string | null;
  ultimo_numero: number;
  data_ultimo: string | null;
  documentos: number;
}

/** Séries de documentos (Fr_SeriesDocumento do KIGEST). */
export default function Series() {
  const qc = useQueryClient();
  const t = useTabelas().data;
  const tem = useSession((s) => s.tem);
  const [tipo, setTipo] = useState("");
  const [editar, setEditar] = useState<Partial<Serie> | null>(null);
  const lista = useQuery({ queryKey: ["series", tipo], queryFn: () => call<Serie[]>("series.listar", { tipo: tipo || null }) });
  const pode = tem("sistema.series");

  async function estado(s: Serie, patch: { ativa?: boolean; padrao?: boolean }) {
    try {
      await call("series.estado", { id: s.id, ...patch });
      qc.invalidateQueries({ queryKey: ["series"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  async function eliminar(s: Serie) {
    try {
      await call("series.eliminar", { id: s.id });
      toast.success(`Série ${s.serie} eliminada.`);
      qc.invalidateQueries({ queryKey: ["series"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <PageHeader
        title="Séries de Documentos"
        subtitle="Numeração por tipo de documento: série por defeito, período de validade, séries manuais e bloqueio"
        actions={
          pode && (
            <Button onClick={() => setEditar({ tipo_codigo: tipo || "FT", padrao: 1, manual: 0, ativa: 1 })}>
              <Plus className="size-4" /> Nova série
            </Button>
          )
        }
      />
      <Card>
        <div className="flex items-center gap-3 border-b border-line p-4">
          <Select className="w-72" value={tipo} onChange={(e) => setTipo(e.target.value)}>
            <option value="">Todos os tipos de documento</option>
            {t?.tipos_documento.map((x) => (
              <option key={x.codigo} value={x.codigo}>
                {x.codigo} — {x.descricao}
              </option>
            ))}
          </Select>
          <p className="text-[12.5px] text-ink-3">
            Sem série por defeito, cada posto usa a série automática do ano (ex.: {new Date().getFullYear()}). As séries manuais servem para recuperar documentos emitidos à mão.
          </p>
        </div>
        <Table className="max-h-[calc(100vh-270px)]">
          <thead>
            <tr>
              <Th>Documento</Th>
              <Th>Série</Th>
              <Th>Descrição</Th>
              <Th>Validade</Th>
              <Th>Posto</Th>
              <Th className="text-right">Último n.º</Th>
              <Th>Último documento</Th>
              <Th>Estado</Th>
              <Th className="text-right">Acções</Th>
            </tr>
          </thead>
          <tbody>
            {lista.data?.map((s) => (
              <tr key={s.id} className={cn(!s.ativa && "opacity-55", pode && "cursor-pointer hover:bg-sunken/60")} onClick={() => pode && setEditar(s)}>
                <Td>
                  <span className="font-mono text-[12.5px] text-ink-3">{s.tipo_codigo}</span> {s.tipo_desc}
                </Td>
                <Td className="font-mono font-medium">{s.serie}</Td>
                <Td className="text-ink-2">{s.descricao ?? "—"}</Td>
                <Td className="text-ink-2">{s.data_inicio || s.data_fim ? `${data(s.data_inicio)} a ${data(s.data_fim)}` : "—"}</Td>
                <Td className="text-ink-2">{s.posto ?? "Todos"}</Td>
                <Td className="num text-right">{s.ultimo_numero}</Td>
                <Td className="text-ink-2">{data(s.data_ultimo)}</Td>
                <Td>
                  <div className="flex flex-wrap gap-1">
                    {!!s.padrao && <Badge tone="brand">Por defeito</Badge>}
                    {!!s.manual && <Badge tone="accent">Manual</Badge>}
                    {!s.ativa && <Badge tone="danger">Bloqueada</Badge>}
                  </div>
                </Td>
                <Td onClick={(e) => e.stopPropagation()}>
                  {pode && (
                    <div className="flex justify-end gap-1">
                      {!s.padrao && !s.manual && (
                        <Button size="sm" variant="ghost" title="Definir como série por defeito" onClick={() => estado(s, { padrao: true })}>
                          <Star className="size-4" />
                        </Button>
                      )}
                      <Button size="sm" variant="ghost" title={s.ativa ? "Bloquear série" : "Desbloquear série"} onClick={() => estado(s, { ativa: !s.ativa })}>
                        {s.ativa ? <Lock className="size-4" /> : <LockOpen className="size-4" />}
                      </Button>
                      {s.documentos === 0 && (
                        <Button size="sm" variant="ghost" className="text-danger" title="Eliminar (só séries sem documentos)" onClick={() => eliminar(s)}>
                          <Trash2 className="size-4" />
                        </Button>
                      )}
                    </div>
                  )}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      </Card>
      {editar && <SerieForm inicial={editar} onClose={() => setEditar(null)} />}
    </>
  );
}

function SerieForm({ inicial, onClose }: { inicial: Partial<Serie>; onClose: () => void }) {
  const qc = useQueryClient();
  const t = useTabelas().data;
  const postos = t?.postos ?? [];
  const [s, setS] = useState(inicial);
  const [busy, setBusy] = useState(false);
  const comDocs = (inicial.ultimo_numero ?? 0) > 0;
  async function guardar() {
    setBusy(true);
    try {
      await call("series.guardar", {
        id: s.id,
        tipo_codigo: s.tipo_codigo,
        serie: s.serie ?? "",
        descricao: s.descricao ?? null,
        data_inicio: s.data_inicio || null,
        data_fim: s.data_fim || null,
        padrao: !!s.padrao,
        manual: !!s.manual,
        posto_id: s.posto_id ?? null,
        ativa: s.ativa !== 0,
      });
      toast.success("Série guardada.");
      qc.invalidateQueries({ queryKey: ["series"] });
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
      title={s.id ? `Série ${s.serie}` : "Nova série"}
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={guardar}>
            Gravar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Documento" className="col-span-2">
          <Select disabled={comDocs} value={s.tipo_codigo} onChange={(e) => setS({ ...s, tipo_codigo: e.target.value })}>
            {t?.tipos_documento.map((x) => (
              <option key={x.codigo} value={x.codigo}>
                {x.codigo} — {x.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Série *" hint="Letras, números e hífen (ex.: A2026, LJ2)">
          <Input autoFocus disabled={comDocs} className="font-mono uppercase" value={s.serie ?? ""} onChange={(e) => setS({ ...s, serie: e.target.value.toUpperCase() })} />
        </Field>
        <Field label="Descrição da série *">
          <Input value={s.descricao ?? ""} onChange={(e) => setS({ ...s, descricao: e.target.value })} />
        </Field>
        <Field label="Data inicial">
          <Input type="date" value={s.data_inicio ?? ""} onChange={(e) => setS({ ...s, data_inicio: e.target.value })} />
        </Field>
        <Field label="Data final">
          <Input type="date" value={s.data_fim ?? ""} onChange={(e) => setS({ ...s, data_fim: e.target.value })} />
        </Field>
        {postos.length > 1 && (
          <Field label="Posto" className="col-span-2">
            <Select value={s.posto_id ?? ""} onChange={(e) => setS({ ...s, posto_id: e.target.value ? Number(e.target.value) : null })}>
              <option value="">Todos os postos</option>
              {postos.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.descricao}
                </option>
              ))}
            </Select>
          </Field>
        )}
      </div>
      <div className="mt-4 space-y-2">
        <label className="flex items-center gap-2">
          <input type="checkbox" disabled={!!s.manual} checked={!!s.padrao} onChange={(e) => setS({ ...s, padrao: e.target.checked ? 1 : 0 })} /> Definir como série por defeito
        </label>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={!!s.manual} onChange={(e) => setS({ ...s, manual: e.target.checked ? 1 : 0, padrao: e.target.checked ? 0 : s.padrao })} /> Série de documentos manuais
          <span className="text-[12px] text-ink-3">(recuperação de documentos emitidos à mão)</span>
        </label>
        {s.id && (
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={s.ativa !== 0} onChange={(e) => setS({ ...s, ativa: e.target.checked ? 1 : 0 })} /> Série activa
          </label>
        )}
      </div>
    </Modal>
  );
}

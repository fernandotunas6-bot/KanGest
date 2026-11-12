import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { toast } from "sonner";
import { CalendarDays, Plus, Printer } from "lucide-react";
import { imprimir } from "@/lib/print";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { call } from "@/lib/api";
import { data, hojeISO, qtd } from "@/lib/format";
import { MESES, type Alteracao, type Funcionario } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface Ferias {
  id: number;
  funcionario_id: number;
  nome: string;
  ano: number;
  data_inicio: string;
  data_fim: string;
  dias: number;
  estado: "MARCADA" | "GOZADA" | "CANCELADA";
  observacoes: string | null;
}

const ESTADOS = { MARCADA: { label: "Marcada", tone: "accent" }, GOZADA: { label: "Gozada", tone: "ok" }, CANCELADA: { label: "Cancelada", tone: "neutral" } } as const;

/** Dias úteis (segunda a sexta) entre duas datas, inclusive. */
function diasUteis(de: string, ate: string) {
  if (!de || !ate || ate < de) return 0;
  let n = 0;
  const d = new Date(de + "T12:00:00");
  const fim = new Date(ate + "T12:00:00");
  while (d <= fim) {
    const w = d.getDay();
    if (w !== 0 && w !== 6) n++;
    d.setDate(d.getDate() + 1);
  }
  return n;
}

export default function FeriasFaltas() {
  const hoje = hojeISO();
  const [ano, setAno] = useState(Number(hoje.slice(0, 4)));
  const [mes, setMes] = useState(Number(hoje.slice(5, 7)));
  const [editar, setEditar] = useState<Partial<Ferias> | null>(null);
  const funcs = useQuery({ queryKey: ["funcionarios", "", false], queryFn: () => call<Funcionario[]>("rh.funcionarios", { q: "" }) }).data ?? [];
  const ferias = useQuery({ queryKey: ["rh-ferias", ano], queryFn: () => call<Ferias[]>("rh.ferias", { ano }) });
  const faltas = useQuery({ queryKey: ["rh-alteracoes", ano, mes], queryFn: () => call<Alteracao[]>("rh.alteracoes", { ano, mes }) }).data?.filter((a) => a.rubrica === "101");
  const empresa = useEmpresa();
  const gozados = (fid: number) => (ferias.data ?? []).filter((f) => f.funcionario_id === fid && f.estado !== "CANCELADA").reduce((s, f) => s + f.dias, 0);

  return (
    <>
      <PageHeader
        title="Férias e Faltas"
        subtitle="Plano de férias anual e faltas lançadas no processamento"
        actions={
          <>
            <Input className="num w-24" type="number" value={ano} onChange={(e) => setAno(Number(e.target.value))} />
            <Button variant="secondary" disabled={!ferias.data?.length} onClick={imprimir}>
              <Printer className="size-4" /> Mapa de férias
            </Button>
            <Button onClick={() => setEditar({ estado: "MARCADA" })}>
              <Plus className="size-4" /> Marcar férias
            </Button>
          </>
        }
      />
      <div className="grid grid-cols-[minmax(0,1fr)_360px] gap-4">
        <Card>
          <div className="border-b border-line px-5 py-3 font-medium">Férias de {ano}</div>
          {!ferias.data?.length ? (
            <Empty icon={<CalendarDays className="size-6" />} title="Sem férias marcadas" text="Marque os períodos de férias do pessoal. São 22 dias úteis por ano, regra geral (confirme a Lei Geral do Trabalho)." />
          ) : (
            <Table className="max-h-[calc(100vh-280px)]">
              <thead>
                <tr>
                  <Th>Funcionário</Th>
                  <Th>Início</Th>
                  <Th>Fim</Th>
                  <Th className="text-right">Dias úteis</Th>
                  <Th className="text-right">Total no ano</Th>
                  <Th>Estado</Th>
                  <Th>Observações</Th>
                </tr>
              </thead>
              <tbody>
                {ferias.data.map((f) => (
                  <tr key={f.id} className={cn("cursor-pointer hover:bg-sunken/60", f.estado === "CANCELADA" && "opacity-55")} onClick={() => setEditar(f)}>
                    <Td className="font-medium">{f.nome}</Td>
                    <Td>{data(f.data_inicio)}</Td>
                    <Td>{data(f.data_fim)}</Td>
                    <Td className="num text-right">{f.dias}</Td>
                    <Td className="num text-right text-ink-2">{gozados(f.funcionario_id)}</Td>
                    <Td>
                      <Badge tone={ESTADOS[f.estado].tone}>{ESTADOS[f.estado].label}</Badge>
                    </Td>
                    <Td className="text-ink-2">{f.observacoes ?? "—"}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>
        <Card className="h-fit">
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <span className="font-medium">Faltas</span>
            <Select className="h-8 w-36" value={mes} onChange={(e) => setMes(Number(e.target.value))}>
              {MESES.map((m, i) => (
                <option key={m} value={i + 1}>
                  {m}
                </option>
              ))}
            </Select>
          </div>
          {!faltas?.length ? (
            <p className="px-5 py-8 text-center text-ink-3">Sem faltas lançadas neste mês.</p>
          ) : (
            <ul className="divide-y divide-line">
              {faltas.map((a) => (
                <li key={a.id} className="flex items-center justify-between px-5 py-2.5">
                  <div>
                    <div className="font-medium">{a.nome}</div>
                    {a.descricao && <div className="text-[12px] text-ink-3">{a.descricao}</div>}
                  </div>
                  <span className="num">{qtd(a.quantidade)} dia(s)</span>
                </li>
              ))}
            </ul>
          )}
          <div className="border-t border-line px-5 py-3 text-[12.5px] text-ink-3">
            As faltas injustificadas lançam-se em{" "}
            <Link to="/rh/salarios?vista=alteracoes" className="font-medium text-brand underline">
              Alterações do mês
            </Link>{" "}
            (rubrica 101) e descontam no recibo.
          </div>
        </Card>
      </div>
      {editar && <FeriasForm inicial={editar} funcs={funcs} onClose={() => setEditar(null)} />}
      {!editar && (
        <PrintPortal>
          <div className="p-2 text-[11px] text-black">
            <CabecalhoEmpresa empresa={empresa} titulo="Mapa de Férias" numero={String(ano)} />
            <table className="w-full border-collapse text-left">
              <thead>
                <tr className="border-b border-black">
                  <th className="py-1">Funcionário</th>
                  <th className="py-1">Início</th>
                  <th className="py-1">Fim</th>
                  <th className="py-1 text-right">Dias úteis</th>
                  <th className="py-1 text-right">Total no ano</th>
                  <th className="py-1">Estado</th>
                </tr>
              </thead>
              <tbody>
                {(ferias.data ?? []).map((f) => (
                  <tr key={f.id} className="border-b border-dotted border-gray-300">
                    <td className="py-0.5">{f.nome}</td>
                    <td className="py-0.5">{data(f.data_inicio)}</td>
                    <td className="py-0.5">{data(f.data_fim)}</td>
                    <td className="py-0.5 text-right">{f.dias}</td>
                    <td className="py-0.5 text-right">{gozados(f.funcionario_id)}</td>
                    <td className="py-0.5">{ESTADOS[f.estado].label}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </PrintPortal>
      )}
    </>
  );
}

/** Guia de férias de um funcionário (GuiaFerias do KIGEST), para assinar. */
function GuiaFerias({ f, empresa }: { f: Partial<Ferias>; empresa: ReturnType<typeof useEmpresa> }) {
  return (
    <div className="p-2 text-[12px] text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Guia de Férias" numero={f.ano ? String(f.ano) : undefined} />
      <div className="mt-4 space-y-2 text-[13px] leading-relaxed">
        <p>
          Comunica-se que o(a) funcionário(a) <b>{f.nome}</b> gozará <b>{f.dias}</b> dia(s) úteis de férias, de <b>{data(f.data_inicio)}</b> a{" "}
          <b>{data(f.data_fim)}</b>, devendo retomar o serviço no primeiro dia útil seguinte.
        </p>
        {f.observacoes && <p>Observações: {f.observacoes}</p>}
      </div>
      <div className="mt-20 grid grid-cols-3 gap-10 text-center">
        <div className="border-t border-black pt-1">O Funcionário</div>
        <div className="border-t border-black pt-1">O Responsável</div>
        <div className="border-t border-black pt-1">Recursos Humanos</div>
      </div>
    </div>
  );
}

function FeriasForm({ inicial, funcs, onClose }: { inicial: Partial<Ferias>; funcs: Funcionario[]; onClose: () => void }) {
  const qc = useQueryClient();
  const [f, setF] = useState(inicial);
  const [busy, setBusy] = useState(false);
  const sugeridos = diasUteis(f.data_inicio ?? "", f.data_fim ?? "");
  const empresa = useEmpresa();
  async function guardar() {
    setBusy(true);
    try {
      await call("rh.feriasGuardar", { ...f, funcionario_id: Number(f.funcionario_id), dias: Number(f.dias ?? sugeridos) });
      toast.success("Férias guardadas.");
      qc.invalidateQueries({ queryKey: ["rh-ferias"] });
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
      title={f.id ? `Férias — ${f.nome}` : "Marcar férias"}
      width="max-w-xl"
      footer={
        <>
          {f.id && (
            <Button variant="ghost" className="mr-auto" onClick={imprimir}>
              <Printer className="size-4" /> Guia de férias
            </Button>
          )}
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!f.funcionario_id} onClick={guardar}>
            Guardar
          </Button>
        </>
      }
    >
      {f.id && (
        <PrintPortal>
          <GuiaFerias f={inicial} empresa={empresa} />
        </PrintPortal>
      )}
      <div className="grid grid-cols-2 gap-4">
        <Field label="Funcionário" className="col-span-2">
          <Select value={f.funcionario_id ?? ""} onChange={(e) => setF({ ...f, funcionario_id: Number(e.target.value) })}>
            <option value="">Seleccione…</option>
            {funcs.map((x) => (
              <option key={x.id} value={x.id}>
                {x.nome}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Início">
          <Input type="date" value={f.data_inicio ?? ""} onChange={(e) => setF({ ...f, data_inicio: e.target.value, dias: undefined })} />
        </Field>
        <Field label="Fim">
          <Input type="date" min={f.data_inicio} value={f.data_fim ?? ""} onChange={(e) => setF({ ...f, data_fim: e.target.value, dias: undefined })} />
        </Field>
        <Field label="Dias úteis" hint="Calculado sem sábados e domingos; ajuste os feriados.">
          <Input className="num text-right" type="number" min={1} value={f.dias ?? sugeridos} onChange={(e) => setF({ ...f, dias: Number(e.target.value) })} />
        </Field>
        <Field label="Estado">
          <Select value={f.estado ?? "MARCADA"} onChange={(e) => setF({ ...f, estado: e.target.value as Ferias["estado"] })}>
            {Object.entries(ESTADOS).map(([k, v]) => (
              <option key={k} value={k}>
                {v.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Observações" className="col-span-2">
          <Input value={f.observacoes ?? ""} onChange={(e) => setF({ ...f, observacoes: e.target.value })} />
        </Field>
      </div>
    </Modal>
  );
}

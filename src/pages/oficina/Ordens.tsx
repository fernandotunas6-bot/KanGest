import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { ClipboardList, FileText, Plus, Printer, Search, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, dataHora, hojeISO, qtd } from "@/lib/format";
import { imprimir } from "@/lib/print";
import type { Artigo, EstadoOS, Ordem, OrdemLinha, Viatura } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { ArtigoPicker } from "@/components/Pickers";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { useSugestoes, ViaturaForm } from "./Viaturas";

export const ESTADOS: { id: EstadoOS; label: string; tone: "neutral" | "brand" | "accent" | "ok" | "danger" }[] = [
  { id: "ABERTA", label: "Aberta", tone: "accent" },
  { id: "EM_CURSO", label: "Em curso", tone: "brand" },
  { id: "CONCLUIDA", label: "Concluída", tone: "ok" },
  { id: "FACTURADA", label: "Facturada", tone: "neutral" },
  { id: "CANCELADA", label: "Cancelada", tone: "danger" },
];
const estadoDe = (e: EstadoOS) => ESTADOS.find((x) => x.id === e) ?? ESTADOS[0];

export default function Ordens() {
  const [sp, setSp] = useSearchParams();
  const [q, setQ] = useState("");
  const [estado, setEstado] = useState<EstadoOS | "">("");
  const dq = useDebounce(q);
  const [aberta, setAberta] = useState<number | "nova" | null>(null);
  const [viaturaNova, setViaturaNova] = useState<number | null>(null);
  const lista = useQuery({ queryKey: ["ordens", dq, estado], queryFn: () => call<Ordem[]>("oficina.ordens", { q: dq, estado: estado || null }) });

  // /oficina/ordens?nova=<viatura>: abre logo uma OS nova para a viatura
  useEffect(() => {
    const v = sp.get("nova");
    if (v !== null) {
      setViaturaNova(v ? Number(v) : null);
      setAberta("nova");
      setSp({}, { replace: true });
    }
  }, [sp, setSp]);

  return (
    <>
      <PageHeader
        title="Ordens de Serviço"
        subtitle="Entrada de viaturas, diagnóstico, trabalhos e facturação"
        actions={
          <Button
            onClick={() => {
              setViaturaNova(null);
              setAberta("nova");
            }}
          >
            <Plus className="size-4" /> Nova OS
          </Button>
        }
      />
      <Card>
        <div className="flex flex-wrap items-center gap-3 border-b border-line p-4">
          <div className="relative min-w-64 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="N.º da OS, matrícula, cliente ou avaria…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <div className="flex flex-wrap gap-1 rounded-lg bg-sunken p-1">
            {[{ id: "" as const, label: "Todas" }, ...ESTADOS].map((e) => (
              <button
                key={e.id}
                onClick={() => setEstado(e.id)}
                className={cn("rounded-md px-3 py-1.5 text-[13px]", estado === e.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
              >
                {e.label}
              </button>
            ))}
          </div>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<ClipboardList className="size-6" />} title="Sem ordens de serviço" text="Abra uma OS quando a viatura dá entrada na oficina." />
        ) : (
          <Table className="max-h-[calc(100vh-260px)]">
            <thead>
              <tr>
                <Th>N.º</Th>
                <Th>Entrada</Th>
                <Th>Viatura</Th>
                <Th>Cliente</Th>
                <Th>Avaria</Th>
                <Th>Técnico</Th>
                <Th>Estado</Th>
                <Th>Factura</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((o) => {
                const e = estadoDe(o.estado);
                return (
                  <tr key={o.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setAberta(o.id)}>
                    <Td className="font-medium">{o.numero}</Td>
                    <Td className="text-ink-2">{dataHora(o.data_entrada)}</Td>
                    <Td>
                      <span className="font-mono">{o.matricula ?? "—"}</span> <span className="text-ink-2">{[o.marca, o.modelo].filter(Boolean).join(" ")}</span>
                    </Td>
                    <Td className="text-ink-2">{o.cliente ?? "—"}</Td>
                    <Td className="max-w-64 truncate text-ink-2">{o.avaria ?? "—"}</Td>
                    <Td className="text-ink-2">{o.tecnico ?? "—"}</Td>
                    <Td>
                      <Badge tone={e.tone}>{e.label}</Badge>
                    </Td>
                    <Td className="text-ink-2">{o.documento ?? "—"}</Td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
        )}
      </Card>
      {aberta !== null && <OrdemForm id={aberta === "nova" ? null : aberta} viaturaInicial={viaturaNova} onClose={() => setAberta(null)} onCriada={(id) => setAberta(id)} />}
    </>
  );
}

interface LinhaForm extends OrdemLinha {
  key: string;
}

function OrdemForm({ id, viaturaInicial, onClose, onCriada }: { id: number | null; viaturaInicial: number | null; onClose: () => void; onCriada: (id: number) => void }) {
  const qc = useQueryClient();
  const nav = useNavigate();
  const sug = useSugestoes();
  const empresa = useEmpresa();
  const viaturas = useQuery({ queryKey: ["viaturas", ""], queryFn: () => call<Viatura[]>("oficina.viaturas", { q: "" }) }).data;
  const atual = useQuery({ queryKey: ["ordem", id], queryFn: () => call<Ordem>("oficina.ordem", { id }), enabled: !!id }).data;
  const [o, setO] = useState<Partial<Ordem>>({ estado: "ABERTA", viatura_id: viaturaInicial ?? undefined });
  const [linhas, setLinhas] = useState<LinhaForm[]>([]);
  const [novaViatura, setNovaViatura] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!atual) return;
    setO(atual);
    setLinhas((atual.linhas ?? []).map((l) => ({ ...l, key: crypto.randomUUID() })));
  }, [atual]);

  const viatura = viaturas?.find((v) => v.id === o.viatura_id);
  const bloqueada = o.estado === "FACTURADA";
  const set = (k: keyof Ordem, v: unknown) => setO((x) => ({ ...x, [k]: v }));
  const num = (v: unknown) => (v === "" || v === null || v === undefined ? null : Number(v));

  async function guardar(): Promise<number | null> {
    if (!o.viatura_id) {
      toast.error("Seleccione a viatura.");
      return null;
    }
    setBusy(true);
    try {
      const r = await call<{ id: number }>("oficina.ordemGuardar", {
        id: id ?? undefined,
        viatura_id: o.viatura_id,
        entidade_id: o.entidade_id ?? viatura?.entidade_id ?? null,
        km_entrada: num(o.km_entrada),
        km_saida: num(o.km_saida),
        referencia: o.referencia ?? null,
        avaria: o.avaria ?? null,
        diagnostico: o.diagnostico ?? null,
        tecnico: o.tecnico ?? null,
        estado: o.estado,
        observacoes: o.observacoes ?? null,
        linhas: linhas.map((l) => ({ artigo_id: l.artigo_id, descricao: l.descricao, quantidade: Number(l.quantidade) || 0, mecanico: l.mecanico || null })),
      });
      toast.success("Ordem de serviço guardada.");
      qc.invalidateQueries({ queryKey: ["ordens"] });
      qc.invalidateQueries({ queryKey: ["ordem", r.id] });
      qc.invalidateQueries({ queryKey: ["viaturas"] });
      qc.invalidateQueries({ queryKey: ["oficina-sugestoes"] });
      if (!id) onCriada(r.id);
      return r.id;
    } catch (e) {
      toast.error((e as Error).message);
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function facturar() {
    const oid = await guardar();
    if (!oid) return;
    if (!linhas.some((l) => l.artigo_id)) {
      toast.error("Associe artigos (peças ou mão de obra) às linhas para poder facturar.");
      return;
    }
    nav(`/documentos/novo/FT?ordem=${oid}`);
  }

  function adicionar(a: Artigo) {
    setLinhas((ls) => [...ls, { key: crypto.randomUUID(), artigo_id: a.id, descricao: a.descricao, quantidade: 1, mecanico: null, referencia: a.referencia, artigo_tipo: a.tipo }]);
  }
  const upd = (k: string, p: Partial<LinhaForm>) => setLinhas((ls) => ls.map((l) => (l.key === k ? { ...l, ...p } : l)));

  return (
    <>
      <Modal
        open
        onClose={onClose}
        title={id ? `${o.numero ?? "Ordem de serviço"} · ${estadoDe((o.estado as EstadoOS) ?? "ABERTA").label}` : "Nova ordem de serviço"}
        width="max-w-5xl"
        footer={
          <>
            {id && (
              <Button variant="secondary" className="mr-auto" onClick={imprimir}>
                <Printer className="size-4" /> Imprimir OS
              </Button>
            )}
            <Button variant="secondary" onClick={onClose}>
              Fechar
            </Button>
            {!bloqueada && (
              <>
                <Button variant="secondary" loading={busy} onClick={guardar}>
                  Guardar
                </Button>
                {o.estado !== "CANCELADA" && (
                  <Button loading={busy} onClick={facturar}>
                    <FileText className="size-4" /> Facturar
                  </Button>
                )}
              </>
            )}
          </>
        }
      >
        {bloqueada && (
          <div className="mb-4 rounded-lg bg-sunken px-4 py-2.5 text-ink-2">
            Facturada em {o.documento ?? "—"}{o.data_conclusao ? ` · concluída em ${dataHora(o.data_conclusao)}` : ""}. A OS já não pode ser alterada.
          </div>
        )}
        <fieldset disabled={bloqueada} className="grid grid-cols-4 gap-4">
          <Field label="Viatura" className="col-span-2">
            <div className="flex gap-2">
              <Select value={o.viatura_id ?? ""} onChange={(e) => set("viatura_id", e.target.value ? Number(e.target.value) : undefined)}>
                <option value="">Seleccione…</option>
                {viaturas?.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.matricula ?? "s/ matrícula"} · {v.marca} {v.modelo ?? ""} {v.cliente ? `— ${v.cliente}` : ""}
                  </option>
                ))}
              </Select>
              <Button variant="secondary" title="Registar viatura" onClick={() => setNovaViatura(true)}>
                <Plus className="size-4" />
              </Button>
            </div>
          </Field>
          <Field label="Estado">
            <Select value={o.estado} onChange={(e) => set("estado", e.target.value)}>
              {ESTADOS.filter((e) => e.id !== "FACTURADA" || o.estado === "FACTURADA").map((e) => (
                <option key={e.id} value={e.id}>
                  {e.label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Técnico responsável">
            <Input list="of-tecnicos" value={o.tecnico ?? ""} onChange={(e) => set("tecnico", e.target.value)} />
          </Field>
          <Field label="Km à entrada" hint={viatura ? `Último registo: ${qtd(viatura.km)} km` : undefined}>
            <Input className="num text-right" type="number" min={0} value={o.km_entrada ?? ""} onChange={(e) => set("km_entrada", e.target.value)} />
          </Field>
          <Field label="Km à saída">
            <Input className="num text-right" type="number" min={0} value={o.km_saida ?? ""} onChange={(e) => set("km_saida", e.target.value)} />
          </Field>
          <Field label="Referência do cliente" className="col-span-2">
            <Input value={o.referencia ?? ""} placeholder="Requisição, apólice de seguro…" onChange={(e) => set("referencia", e.target.value)} />
          </Field>
          <Field label="Avaria / pedido do cliente" className="col-span-2">
            <textarea
              className="min-h-20 w-full rounded-lg border border-line-strong bg-surface px-3 py-2 outline-none focus:border-brand focus:ring-3 focus:ring-brand/15"
              value={o.avaria ?? ""}
              onChange={(e) => set("avaria", e.target.value)}
            />
          </Field>
          <Field label="Diagnóstico" className="col-span-2">
            <textarea
              className="min-h-20 w-full rounded-lg border border-line-strong bg-surface px-3 py-2 outline-none focus:border-brand focus:ring-3 focus:ring-brand/15"
              value={o.diagnostico ?? ""}
              onChange={(e) => set("diagnostico", e.target.value)}
            />
          </Field>
        </fieldset>

        <div className="mt-5 rounded-xl border border-line">
          <div className="flex items-center justify-between gap-3 border-b border-line p-3">
            <span className="font-medium">Peças e trabalhos</span>
            {!bloqueada && <ArtigoPicker className="max-w-lg flex-1" placeholder="Adicionar peça ou mão de obra…" onPick={adicionar} />}
          </div>
          {linhas.length === 0 ? (
            <p className="py-6 text-center text-ink-3">Sem linhas. Adicione as peças aplicadas e as horas de mão de obra.</p>
          ) : (
            <table className="w-full text-left">
              <thead>
                <tr className="text-[12px] uppercase tracking-wide text-ink-3">
                  <th className="px-3 py-2 font-medium">Descrição</th>
                  <th className="w-28 px-3 py-2 text-right font-medium">Quantidade</th>
                  <th className="w-48 px-3 py-2 font-medium">Mecânico</th>
                  <th className="w-10" />
                </tr>
              </thead>
              <tbody>
                {linhas.map((l) => (
                  <tr key={l.key} className="border-t border-line/70">
                    <td className="px-3 py-1.5">
                      <input disabled={bloqueada} className="w-full bg-transparent font-medium outline-none" value={l.descricao} onChange={(e) => upd(l.key, { descricao: e.target.value })} />
                      <div className="font-mono text-[11.5px] text-ink-3">
                        {l.referencia ?? "sem artigo"} {l.artigo_tipo === "S" && "· serviço"}
                      </div>
                    </td>
                    <td className="px-3 py-1.5">
                      <Input disabled={bloqueada} className="num h-8 text-right" type="number" min={0} step="any" value={l.quantidade} onChange={(e) => upd(l.key, { quantidade: Number(e.target.value) })} />
                    </td>
                    <td className="px-3 py-1.5">
                      <Input disabled={bloqueada} className="h-8" list="of-tecnicos" value={l.mecanico ?? ""} onChange={(e) => upd(l.key, { mecanico: e.target.value })} />
                    </td>
                    <td className="px-2 py-1.5">
                      {!bloqueada && (
                        <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setLinhas((ls) => ls.filter((x) => x.key !== l.key))}>
                          <Trash2 className="size-4" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        <Field label="Observações" className="mt-4">
          <Input disabled={bloqueada} value={o.observacoes ?? ""} onChange={(e) => set("observacoes", e.target.value)} />
        </Field>
        <datalist id="of-tecnicos">{sug?.tecnicos.map((t) => <option key={t} value={t} />)}</datalist>
      </Modal>

      {novaViatura && <ViaturaForm inicial={{ ativo: 1, km: 0 }} onClose={() => setNovaViatura(false)} onGuardada={(vid) => set("viatura_id", vid)} />}

      {id && atual && (
        <PrintPortal>
          <OrdemPrint o={{ ...atual, ...o } as Ordem} linhas={linhas} empresa={empresa} />
        </PrintPortal>
      )}
    </>
  );
}

function OrdemPrint({ o, linhas, empresa }: { o: Ordem; linhas: OrdemLinha[]; empresa: ReturnType<typeof useEmpresa> }) {
  return (
    <div className="mx-auto w-[190mm] bg-white p-2 text-[11.5px] leading-snug text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Ordem de Serviço" numero={o.numero}>
        <div className="mt-1">Entrada: {dataHora(o.data_entrada)}</div>
        <div>{estadoDe(o.estado).label}</div>
      </CabecalhoEmpresa>
      <div className="mt-4 grid grid-cols-2 gap-6">
        <div className="rounded border border-black/40 p-3">
          <div className="text-[10px] uppercase text-black/60">Cliente</div>
          <div className="font-semibold">{o.cliente ?? "—"}</div>
          {o.cliente_nif && <div>NIF: {o.cliente_nif}</div>}
          {(o.cliente_morada || o.cliente_localidade) && <div>{[o.cliente_morada, o.cliente_localidade].filter(Boolean).join(", ")}</div>}
        </div>
        <div className="rounded border border-black/40 p-3">
          <div className="text-[10px] uppercase text-black/60">Viatura</div>
          <div className="font-semibold">
            {o.marca} {o.modelo ?? ""} · {o.matricula ?? "sem matrícula"}
          </div>
          <div>{[o.cor, o.ano, o.combustivel].filter(Boolean).join(" · ")}</div>
          {o.chassi && <div>Chassi: {o.chassi}</div>}
          <div>
            Km entrada: {(o.km_entrada != null ? qtd(o.km_entrada) : "—")} · Km saída: {(o.km_saida != null ? qtd(o.km_saida) : "—")}
          </div>
        </div>
      </div>
      <div className="mt-4 grid grid-cols-2 gap-6">
        <div>
          <div className="text-[10px] uppercase text-black/60">Avaria / pedido do cliente</div>
          <div className="min-h-12 whitespace-pre-wrap">{o.avaria ?? "—"}</div>
        </div>
        <div>
          <div className="text-[10px] uppercase text-black/60">Diagnóstico</div>
          <div className="min-h-12 whitespace-pre-wrap">{o.diagnostico ?? "—"}</div>
        </div>
      </div>
      <table className="mt-4 w-full border-collapse text-left">
        <thead>
          <tr className="border-y border-black">
            <th className="py-1">Referência</th>
            <th className="py-1">Descrição</th>
            <th className="py-1 text-right">Qtd.</th>
            <th className="py-1 pl-4">Mecânico</th>
          </tr>
        </thead>
        <tbody>
          {linhas.map((l, i) => (
            <tr key={i} className="border-b border-black/20">
              <td className="py-1 font-mono">{l.referencia ?? ""}</td>
              <td className="py-1">{l.descricao}</td>
              <td className="py-1 text-right">{l.quantidade}</td>
              <td className="py-1 pl-4">{l.mecanico ?? ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="mt-3 grid grid-cols-3 gap-6">
        <div>Técnico: {o.tecnico ?? "—"}</div>
        <div>Referência: {o.referencia ?? "—"}</div>
        <div>Factura: {o.documento ?? "—"}</div>
      </div>
      {o.observacoes && <div className="mt-2">Obs.: {o.observacoes}</div>}
      <div className="mt-16 grid grid-cols-2 gap-16 text-center">
        <div className="border-t border-black pt-1">O cliente</div>
        <div className="border-t border-black pt-1">A oficina</div>
      </div>
      <div className="mt-6 text-center text-[10px] text-black/60">Documento interno sem valor fiscal · impresso em {data(hojeISO())}</div>
    </div>
  );
}

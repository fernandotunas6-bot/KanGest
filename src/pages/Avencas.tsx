// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CalendarClock, FileCheck2, Pencil, Plus, Repeat, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { data, kz, parseKz, toInputKz } from "@/lib/format";
import type { Artigo, Entidade } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { ArtigoPicker, EntidadePicker } from "@/components/Pickers";

type Periodicidade = "MENSAL" | "TRIMESTRAL" | "SEMESTRAL" | "ANUAL";

interface LinhaAvenca {
  artigo_id: number;
  artigo?: string;
  referencia?: string;
  descricao: string | null;
  quantidade: number;
  preco: number | null;
  desconto: number;
}

interface Avenca {
  id: number;
  entidade_id: number;
  cliente: string;
  cliente_nif: string;
  descricao: string;
  periodicidade: Periodicidade;
  proxima: string;
  fim: string | null;
  prazo: string | null;
  observacoes: string | null;
  vendedor_id: number | null;
  ativo: number;
  valor: number;
  periodo: string;
  por_facturar: boolean;
  terminada: boolean;
  emitidas: number;
  ultimo_documento: string | null;
  linhas: LinhaAvenca[];
}

const PERIODOS: { id: Periodicidade; label: string }[] = [
  { id: "MENSAL", label: "Mensal" },
  { id: "TRIMESTRAL", label: "Trimestral" },
  { id: "SEMESTRAL", label: "Semestral" },
  { id: "ANUAL", label: "Anual" },
];

const hoje = () => new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);

/**
 * Avenças (prestação de serviços): contratos que se facturam todos os meses, trimestres, semestres ou anos —
 * contabilidade, segurança, limpeza, manutenção, mensalidades. «Facturar» emite de uma vez as facturas dos
 * períodos que chegaram (um período nunca se factura duas vezes).
 */
export default function Avencas() {
  const qc = useQueryClient();
  const lista = useQuery({ queryKey: ["avencas"], queryFn: () => call<Avenca[]>("avencas.listar") });
  const [editar, setEditar] = useState<Partial<Avenca> | null>(null);
  const [facturar, setFacturar] = useState(false);
  const [inactivas, setInactivas] = useState(false);
  const pendentes = (lista.data ?? []).filter((a) => a.por_facturar);
  const visiveis = (lista.data ?? []).filter((a) => inactivas || a.ativo);
  const mensal = (lista.data ?? [])
    .filter((a) => a.ativo && !a.terminada)
    .reduce((s, a) => s + a.valor / { MENSAL: 1, TRIMESTRAL: 3, SEMESTRAL: 6, ANUAL: 12 }[a.periodicidade], 0);

  async function apagar(a: Avenca) {
    if (!confirm(a.emitidas ? `Desactivar a avença «${a.descricao}»? As facturas emitidas ficam.` : `Apagar a avença «${a.descricao}»?`)) return;
    try {
      const r = await call<{ desactivada?: boolean }>("avencas.apagar", { id: a.id });
      toast.success(r.desactivada ? "Avença desactivada." : "Avença apagada.");
      qc.invalidateQueries({ queryKey: ["avencas"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <PageHeader
        title="Avenças"
        subtitle="Serviços facturados todos os meses, trimestres ou anos (contabilidade, segurança, limpeza, manutenção, mensalidades)"
        actions={
          <>
            <Button variant="secondary" disabled={!pendentes.length} onClick={() => setFacturar(true)}>
              <FileCheck2 className="size-4" /> Facturar{pendentes.length ? ` (${pendentes.length})` : ""}
            </Button>
            <Button onClick={() => setEditar({ periodicidade: "MENSAL", proxima: hoje(), ativo: 1, linhas: [] })}>
              <Plus className="size-4" /> Nova avença
            </Button>
          </>
        }
      />

      <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-3">
        <Card className="p-4">
          <div className="text-[13px] text-ink-2">Avenças activas</div>
          <div className="mt-1 text-[22px] font-semibold">{(lista.data ?? []).filter((a) => a.ativo && !a.terminada).length}</div>
        </Card>
        <Card className="p-4">
          <div className="text-[13px] text-ink-2">Por facturar hoje</div>
          <div className={cn("mt-1 text-[22px] font-semibold", pendentes.length && "text-accent-ink")}>{pendentes.length}</div>
        </Card>
        <Card className="col-span-2 p-4 lg:col-span-1">
          <div className="text-[13px] text-ink-2">Receita recorrente (por mês, com IVA)</div>
          <div className="num mt-1 text-[22px] font-semibold">{kz(Math.round(mensal))}</div>
        </Card>
      </div>

      <Card>
        <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
          <span className="flex items-center gap-2 font-medium">
            <Repeat className="size-4 text-brand" /> Contratos
          </span>
          <label className="flex items-center gap-2 text-[13px] text-ink-2">
            <input type="checkbox" checked={inactivas} onChange={(e) => setInactivas(e.target.checked)} /> Mostrar desactivadas
          </label>
        </div>
        {visiveis.length === 0 ? (
          <Empty
            icon={<Repeat className="size-6" />}
            title="Ainda não há avenças"
            text="Crie uma avença para cada cliente com serviço contínuo: o KanGest lembra-o no painel quando chega a altura de facturar."
          />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Cliente</Th>
                <Th>Avença</Th>
                <Th>Periodicidade</Th>
                <Th>Próxima factura</Th>
                <Th className="text-right">Valor (c/ IVA)</Th>
                <Th>Última</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {visiveis.map((a) => (
                <tr key={a.id} className={cn(!a.ativo && "opacity-50")}>
                  <Td>
                    <div className="font-medium">{a.cliente}</div>
                    <div className="font-mono text-[11.5px] text-ink-3">{a.cliente_nif}</div>
                  </Td>
                  <Td>
                    <div>{a.descricao}</div>
                    <div className="text-[12px] text-ink-3">{a.linhas.map((l) => l.artigo).join(", ")}</div>
                  </Td>
                  <Td>{PERIODOS.find((p) => p.id === a.periodicidade)?.label}</Td>
                  <Td>
                    {a.terminada ? (
                      <Badge>Terminada</Badge>
                    ) : (
                      <>
                        <div className={cn("flex items-center gap-1.5", a.por_facturar && "font-medium text-accent-ink")}>
                          {a.por_facturar && <CalendarClock className="size-3.5" />}
                          {data(a.proxima)}
                        </div>
                        <div className="text-[12px] text-ink-3">{a.periodo}</div>
                      </>
                    )}
                  </Td>
                  <Td className="num text-right font-medium">{kz(a.valor)}</Td>
                  <Td className="text-[12.5px] text-ink-2">{a.ultimo_documento ?? "—"}</Td>
                  <Td className="whitespace-nowrap text-right">
                    <button className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-ink" title="Alterar" aria-label="Alterar" onClick={() => setEditar(a)}>
                      <Pencil className="size-4" />
                    </button>
                    {!!a.ativo && (
                      <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" title="Apagar / desactivar" aria-label="Apagar" onClick={() => apagar(a)}>
                        <Trash2 className="size-4" />
                      </button>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>

      {editar && <AvencaForm inicial={editar} onClose={() => setEditar(null)} />}
      {facturar && <Facturar pendentes={pendentes} onClose={() => setFacturar(false)} />}
    </>
  );
}

function AvencaForm({ inicial, onClose }: { inicial: Partial<Avenca>; onClose: () => void }) {
  const qc = useQueryClient();
  const prazos = useQuery({
    queryKey: ["doc-auxiliares"],
    queryFn: () => call<{ prazos: { codigo: string; descricao: string }[] }>("documentos.auxiliares"),
    staleTime: 300_000,
  }).data?.prazos;
  const [a, setA] = useState<Partial<Avenca>>(inicial);
  const [cliente, setCliente] = useState<Entidade | null>(inicial.entidade_id ? ({ id: inicial.entidade_id, nome: inicial.cliente, nif: inicial.cliente_nif } as Entidade) : null);
  const [linhas, setLinhas] = useState<(LinhaAvenca & { precoTxt: string })[]>(
    (inicial.linhas ?? []).map((l) => ({ ...l, precoTxt: l.preco === null ? "" : toInputKz(l.preco) })),
  );
  const [busy, setBusy] = useState(false);
  const set = (k: keyof Avenca, v: unknown) => setA((x) => ({ ...x, [k]: v }));

  function juntar(art: Artigo) {
    setLinhas((ls) => [...ls, { artigo_id: art.id, artigo: art.descricao, referencia: art.referencia, descricao: `${art.descricao} — {periodo}`, quantidade: 1, preco: null, precoTxt: "", desconto: 0 }]);
  }

  async function guardar() {
    if (!cliente) return toast.error("Escolha o cliente.");
    setBusy(true);
    try {
      await call("avencas.guardar", {
        id: a.id,
        entidade_id: cliente.id,
        descricao: a.descricao ?? "",
        periodicidade: a.periodicidade ?? "MENSAL",
        proxima: a.proxima,
        fim: a.fim || null,
        prazo: a.prazo || null,
        observacoes: a.observacoes || null,
        ativo: a.ativo === undefined ? true : !!a.ativo,
        linhas: linhas.map((l) => ({
          artigo_id: l.artigo_id,
          descricao: l.descricao,
          quantidade: Number(l.quantidade) || 1,
          preco: l.precoTxt.trim() ? parseKz(l.precoTxt) : null,
          desconto: Number(l.desconto) || 0,
        })),
      });
      toast.success("Avença guardada.");
      qc.invalidateQueries({ queryKey: ["avencas"] });
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
      title={a.id ? `Avença — ${a.descricao}` : "Nova avença"}
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
          <EntidadePicker tipo="C" valor={cliente} onChange={setCliente} />
        </div>
        <Field label="Descrição" className="col-span-2" hint="Ex.: Contabilidade mensal, Segurança do armazém">
          <Input value={a.descricao ?? ""} onChange={(e) => set("descricao", e.target.value)} />
        </Field>
        <Field label="Periodicidade">
          <Select value={a.periodicidade} onChange={(e) => set("periodicidade", e.target.value)}>
            {PERIODOS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Prazo de pagamento">
          <Select value={a.prazo ?? ""} onChange={(e) => set("prazo", e.target.value)}>
            <option value="">O do cliente</option>
            {prazos?.map((p) => (
              <option key={p.codigo} value={p.codigo}>
                {p.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Próxima factura" hint="Data a partir da qual se factura o próximo período">
          <Input type="date" value={a.proxima ?? ""} onChange={(e) => set("proxima", e.target.value)} />
        </Field>
        <Field label="Termina em" hint="Vazio = sem fim">
          <Input type="date" value={a.fim ?? ""} onChange={(e) => set("fim", e.target.value)} />
        </Field>
        <Field label="Observações na factura" className="col-span-2" hint="{periodo} = período facturado (ex.: Novembro 2026)">
          <Input value={a.observacoes ?? ""} placeholder="Avença: … — {periodo}" onChange={(e) => set("observacoes", e.target.value)} />
        </Field>
      </div>

      <div className="mt-5 rounded-xl border border-line p-4">
        <div className="mb-3 font-medium">Serviços facturados em cada período</div>
        <div className="space-y-2">
          {linhas.map((l, i) => (
            <div key={i} className="grid grid-cols-[minmax(0,1fr)_70px_110px_70px_36px] items-center gap-2 max-sm:grid-cols-2">
              <Input
                value={l.descricao ?? ""}
                title={`${l.referencia ?? ""} ${l.artigo ?? ""}`}
                onChange={(e) => setLinhas(linhas.map((x, j) => (j === i ? { ...x, descricao: e.target.value } : x)))}
                className="max-sm:col-span-2"
              />
              <Input className="num text-right" inputMode="decimal" title="Quantidade" value={String(l.quantidade)} onChange={(e) => setLinhas(linhas.map((x, j) => (j === i ? { ...x, quantidade: Number(e.target.value.replace(",", ".")) || 0 } : x)))} />
              <Input className="num text-right" inputMode="decimal" placeholder="Preço do artigo" title="Preço sem IVA (vazio = o do artigo)" value={l.precoTxt} onChange={(e) => setLinhas(linhas.map((x, j) => (j === i ? { ...x, precoTxt: e.target.value } : x)))} />
              <Input className="num text-right" inputMode="decimal" title="Desconto %" value={String(l.desconto)} onChange={(e) => setLinhas(linhas.map((x, j) => (j === i ? { ...x, desconto: Number(e.target.value.replace(",", ".")) || 0 } : x)))} />
              <button className="grid size-9 place-items-center rounded-md text-ink-3 hover:bg-danger-50 hover:text-danger" aria-label="Retirar" onClick={() => setLinhas(linhas.filter((_, j) => j !== i))}>
                <Trash2 className="size-4" />
              </button>
            </div>
          ))}
          {linhas.length > 0 && <div className="text-[12px] text-ink-3">Quantidade · preço sem IVA (vazio = o do artigo) · desconto %. Na descrição, {"{periodo}"} é o período facturado.</div>}
          <ArtigoPicker onPick={juntar} placeholder="Juntar serviço (pesquisar artigo)…" />
        </div>
      </div>
    </Modal>
  );
}

function Facturar({ pendentes, onClose }: { pendentes: Avenca[]; onClose: () => void }) {
  const qc = useQueryClient();
  const [sel, setSel] = useState<number[]>(pendentes.map((a) => a.id));
  const [busy, setBusy] = useState(false);
  const [resultado, setResultado] = useState<{ emitidos: { avenca: string; periodo: string; numero: string; total: number }[]; erros: { avenca: string; periodo: string; erro: string }[] } | null>(null);

  async function emitir() {
    setBusy(true);
    try {
      const r = await call<NonNullable<typeof resultado>>("avencas.emitir", { ids: sel });
      setResultado(r);
      if (r.emitidos.length) toast.success(`${r.emitidos.length} ${r.emitidos.length === 1 ? "factura emitida" : "facturas emitidas"}.`);
      if (r.erros.length) toast.error(`${r.erros.length} ${r.erros.length === 1 ? "avença não foi facturada" : "avenças não foram facturadas"}.`);
      qc.invalidateQueries({ queryKey: ["avencas"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
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
      title="Facturar avenças"
      width="max-w-2xl"
      footer={
        resultado ? (
          <Button onClick={onClose}>Fechar</Button>
        ) : (
          <>
            <Button variant="secondary" onClick={onClose}>
              Cancelar
            </Button>
            <Button loading={busy} disabled={!sel.length} onClick={emitir}>
              <FileCheck2 className="size-4" /> Emitir {sel.length} {sel.length === 1 ? "avença" : "avenças"}
            </Button>
          </>
        )
      }
    >
      {resultado ? (
        <div className="space-y-2 text-[13px]">
          {resultado.emitidos.map((e, i) => (
            <div key={i} className="flex justify-between gap-3 rounded-lg bg-ok-50 px-3 py-2">
              <span>
                <b>{e.numero}</b> · {e.avenca} · {e.periodo}
              </span>
              <span className="num">{kz(e.total)}</span>
            </div>
          ))}
          {resultado.erros.map((e, i) => (
            <div key={i} className="rounded-lg bg-danger-50 px-3 py-2 text-danger">
              {e.avenca} · {e.periodo}: {e.erro}
            </div>
          ))}
        </div>
      ) : (
        <>
          <p className="mb-3 text-[13px] text-ink-2">
            Sai uma factura (FT) por período chegado, com a data de hoje e o período na descrição. Os períodos em atraso saem todos, um a um.
          </p>
          <div className="space-y-1.5">
            {pendentes.map((a) => (
              <label key={a.id} className="flex cursor-pointer items-center gap-3 rounded-lg border border-line px-3 py-2 hover:bg-sunken/60">
                <input type="checkbox" checked={sel.includes(a.id)} onChange={(e) => setSel(e.target.checked ? [...sel, a.id] : sel.filter((x) => x !== a.id))} />
                <span className="min-w-0 flex-1">
                  <span className="block font-medium">{a.cliente}</span>
                  <span className="block text-[12px] text-ink-3">
                    {a.descricao} · a partir de {a.periodo}
                  </span>
                </span>
                <span className="num text-[13px]">{kz(a.valor)}</span>
              </label>
            ))}
          </div>
        </>
      )}
    </Modal>
  );
}

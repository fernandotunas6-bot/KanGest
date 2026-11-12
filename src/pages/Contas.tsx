import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { AlertTriangle, FileText, HandCoins, Printer, Scale, Undo2 } from "lucide-react";
import { call } from "@/lib/api";
import { data, kz, parseKz } from "@/lib/format";
import { imprimir } from "@/lib/print";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";

interface DocPend {
  id: number;
  numero_doc: string;
  tipo_desc: string;
  data_emissao: string;
  data_vencimento: string | null;
  entidade_id: number;
  entidade_nome: string;
  entidade_nif: string;
  total: number;
  valor_pendente: number;
  doc_externo: string | null;
  dias_atraso: number;
}
interface PorEntidade {
  entidade_id: number;
  entidade_nome: string;
  entidade_nif: string;
  documentos: number;
  total: number;
  vencido: number;
}

/** Contas a Receber (clientes) e Contas a Pagar (fornecedores), com aviso de vencimento e extracto. */
export default function Contas() {
  const { tipo = "receber" } = useParams();
  const pagar = tipo === "pagar";
  const nav = useNavigate();
  const [sp] = useSearchParams();
  const [vista, setVista] = useState<"entidades" | "documentos">("entidades");
  useEffect(() => {
    setVista(sp.get("vista") === "documentos" ? "documentos" : "entidades");
  }, [sp, tipo]);
  const [vencidos, setVencidos] = useState(false);
  const [extrato, setExtrato] = useState<number | null>(null);
  const { data: r, isFetching } = useQuery({
    queryKey: ["contas", tipo, vencidos],
    queryFn: () => call<{ documentos: DocPend[]; entidades: PorEntidade[] }>("contas.pendentes", { tipo: pagar ? "F" : "C", vencidos }),
  });
  const total = r?.entidades.reduce((s, e) => s + e.total, 0) ?? 0;
  const vencido = r?.entidades.reduce((s, e) => s + e.vencido, 0) ?? 0;
  const titulo = pagar ? "Contas a Pagar" : "Contas a Receber";

  return (
    <>
      <PageHeader
        title={titulo}
        subtitle={pagar ? "Facturas de fornecedores em dívida" : "Facturas de clientes em dívida, avisos de vencimento e extractos"}
        actions={
          <>
            <Button variant="secondary" onClick={imprimir}>
              <Printer className="size-4" /> Imprimir
            </Button>
            <Button onClick={() => nav(pagar ? "/pagamentos/novo" : "/recibos/novo")}>
              <HandCoins className="size-4" /> {pagar ? "Pagamento a fornecedor" : "Emitir recibo"}
            </Button>
          </>
        }
      />
      <div className="grid grid-cols-3 gap-4">
        <Stat label={pagar ? "Total a pagar" : "Total a receber"} value={kz(total)} hint={`${r?.entidades.length ?? 0} ${(r?.entidades.length ?? 0) === 1 ? "entidade" : "entidades"}`} />
        <Stat label="Vencido" value={kz(vencido)} tone="danger" icon={<AlertTriangle className="size-4" />} hint="Documentos com a data de vencimento ultrapassada" />
        <Stat label="Por vencer" value={kz(total - vencido)} tone="ok" />
      </div>

      <div className="mt-4 mb-3 flex items-center gap-3">
        <div className="inline-flex rounded-lg bg-sunken p-1">
          {(["entidades", "documentos"] as const).map((v) => (
            <button key={v} onClick={() => setVista(v)} className={cn("rounded-md px-3 py-1.5 text-[13px]", vista === v ? "bg-surface font-medium shadow-sm" : "text-ink-2")}>
              {v === "entidades" ? "Por entidade" : "Documentos pendentes"}
            </button>
          ))}
        </div>
        <label className="flex items-center gap-2 text-ink-2">
          <input type="checkbox" checked={vencidos} onChange={(e) => setVencidos(e.target.checked)} /> Só vencidos (aviso de vencimento)
        </label>
      </div>

      <Card className={cn("transition-opacity", isFetching && "opacity-60")}>
        {vista === "entidades" ? (
          r?.entidades.length ? (
            <Table className="max-h-[calc(100vh-380px)]">
              <thead>
                <tr>
                  <Th>Entidade</Th>
                  <Th>NIF</Th>
                  <Th className="text-right">Documentos</Th>
                  <Th className="text-right">Vencido</Th>
                  <Th className="text-right">Total em dívida</Th>
                  <Th />
                </tr>
              </thead>
              <tbody>
                {r.entidades.map((e) => (
                  <tr key={e.entidade_id ?? e.entidade_nome} className="hover:bg-sunken/60">
                    <Td className="font-medium">{e.entidade_nome}</Td>
                    <Td className="font-mono text-[12.5px]">{e.entidade_nif}</Td>
                    <Td className="num text-right">{e.documentos}</Td>
                    <Td className={cn("num text-right", e.vencido > 0 && "font-medium text-danger")}>{e.vencido ? kz(e.vencido) : "—"}</Td>
                    <Td className="num text-right font-semibold">{kz(e.total)}</Td>
                    <Td className="text-right">
                      <Button size="sm" variant="ghost" onClick={() => setExtrato(e.entidade_id)}>
                        <FileText className="size-4" /> Extracto
                      </Button>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          ) : (
            <Empty icon={<HandCoins className="size-6" />} title="Nada em dívida" text={pagar ? "Não há facturas de fornecedores por pagar." : "Não há facturas de clientes por receber."} />
          )
        ) : (
          <Table className="max-h-[calc(100vh-380px)]">
            <thead>
              <tr>
                <Th>Documento</Th>
                <Th>Entidade</Th>
                <Th>Emissão</Th>
                <Th>Vencimento</Th>
                <Th className="text-right">Total</Th>
                <Th className="text-right">Pendente</Th>
              </tr>
            </thead>
            <tbody>
              {r?.documentos.map((d) => (
                <tr key={d.id}>
                  <Td>
                    <div className="font-medium">{d.numero_doc}</div>
                    <div className="text-[12px] text-ink-3">
                      {d.tipo_desc}
                      {d.doc_externo ? ` · ${d.doc_externo}` : ""}
                    </div>
                  </Td>
                  <Td>{d.entidade_nome}</Td>
                  <Td>{data(d.data_emissao)}</Td>
                  <Td>
                    {data(d.data_vencimento)} {d.dias_atraso > 0 && <Badge tone="danger" className="ml-1">{d.dias_atraso} dias</Badge>}
                  </Td>
                  <Td className="num text-right">{kz(d.total)}</Td>
                  <Td className="num text-right font-semibold">{kz(d.valor_pendente)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>

      {extrato && <ExtratoModal entidadeId={extrato} onClose={() => setExtrato(null)} />}

      {!extrato && (
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <div className="text-[16px] font-bold">
            {titulo}
            {vencidos ? " — vencidos" : ""}
          </div>
          <div className="mb-3">Total {kz(total)} · Vencido {kz(vencido)}</div>
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th>Documento</th>
                <th>Entidade</th>
                <th>Emissão</th>
                <th>Vencimento</th>
                <th className="text-right">Total</th>
                <th className="text-right">Pendente</th>
              </tr>
            </thead>
            <tbody>
              {r?.documentos.map((d) => (
                <tr key={d.id} className="border-b border-black/20">
                  <td>{d.numero_doc}</td>
                  <td>{d.entidade_nome}</td>
                  <td>{data(d.data_emissao)}</td>
                  <td>{data(d.data_vencimento)}</td>
                  <td className="text-right">{kz(d.total)}</td>
                  <td className="text-right">{kz(d.valor_pendente)}</td>
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

interface Extrato {
  entidade: { nome: string; nif: string; tipo: string };
  grupo: "VENDAS" | "COMPRAS";
  movimentos: {
    id: number;
    data_emissao: string;
    numero_doc: string;
    tipo_desc: string;
    doc_externo: string | null;
    debito: number;
    credito: number;
    saldo: number;
    origem: "DOCUMENTO" | "COMPENSACAO" | "DEVOLUCAO" | "REGULARIZACAO";
  }[];
  saldo: number;
  pendente: number;
  credito: number;
}

interface Credito {
  id: number;
  numero_doc: string;
  tipo_desc: string;
  data_emissao: string;
  credito_disponivel: number;
}

export function ExtratoModal({ entidadeId, onClose }: { entidadeId: number; onClose: () => void }) {
  const { data: e } = useQuery({ queryKey: ["extrato", entidadeId], queryFn: () => call<Extrato>("contas.extrato", { entidade_id: entidadeId }) });
  const [acao, setAcao] = useState<"regularizar" | "devolver" | null>(null);
  return (
    <Modal
      open
      onClose={onClose}
      title={e ? `Extracto de conta corrente — ${e.entidade.nome}` : "Extracto"}
      width="max-w-5xl"
      footer={
        <>
          <Button variant="secondary" className="mr-auto" disabled={!e} onClick={() => setAcao("regularizar")}>
            <Scale className="size-4" /> Regularização…
          </Button>
          {!!e?.credito && (
            <Button variant="secondary" onClick={() => setAcao("devolver")}>
              <Undo2 className="size-4" /> {e.grupo === "COMPRAS" ? "Receber reembolso…" : "Devolver crédito…"}
            </Button>
          )}
          <Button onClick={imprimir}>
            <Printer className="size-4" /> Imprimir
          </Button>
        </>
      }
    >
      {e && (
        <>
          <div className="mb-4 grid grid-cols-3 gap-4">
            <Stat label="Em dívida" value={kz(e.pendente)} tone="accent" />
            <Stat label={e.grupo === "COMPRAS" ? "Crédito junto do fornecedor" : "Crédito a favor do cliente"} value={kz(e.credito)} tone="ok" />
            <Stat label="Saldo da conta corrente" value={kz(e.saldo)} />
          </div>
          <Table className="max-h-[52vh]">
            <thead>
              <tr>
                <Th>Data</Th>
                <Th>Documento</Th>
                <Th>Descrição</Th>
                <Th className="text-right">Débito</Th>
                <Th className="text-right">Crédito</Th>
                <Th className="text-right">Saldo</Th>
              </tr>
            </thead>
            <tbody>
              {e.movimentos.map((m, i) => (
                <tr key={`${m.origem}-${m.id}-${i}`} className={cn(m.origem !== "DOCUMENTO" && "bg-sunken/40")}>
                  <Td className="text-ink-2">{data(m.data_emissao)}</Td>
                  <Td className="font-medium">{m.numero_doc}</Td>
                  <Td className="text-ink-2">
                    {m.tipo_desc}
                    {m.doc_externo ? ` · ${m.doc_externo}` : ""}
                  </Td>
                  <Td className="num text-right">{m.debito ? kz(m.debito) : ""}</Td>
                  <Td className="num text-right">{m.credito ? kz(m.credito) : ""}</Td>
                  <Td className="num text-right font-medium">{kz(m.saldo)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          {!e.movimentos.length && <p className="py-8 text-center text-ink-3">Sem movimentos de conta corrente.</p>}
          <PrintPortal>
            <div className="p-2 text-[11px] text-black">
              <div className="text-[16px] font-bold">Extracto de conta corrente</div>
              <div className="mb-3">
                {e.entidade.nome} · NIF {e.entidade.nif}
              </div>
              <table className="w-full border-collapse text-left">
                <thead>
                  <tr className="border-b border-black">
                    <th>Data</th>
                    <th>Documento</th>
                    <th>Descrição</th>
                    <th className="text-right">Débito</th>
                    <th className="text-right">Crédito</th>
                    <th className="text-right">Saldo</th>
                  </tr>
                </thead>
                <tbody>
                  {e.movimentos.map((m, i) => (
                    <tr key={`${m.origem}-${m.id}-${i}`} className="border-b border-black/20">
                      <td>{data(m.data_emissao)}</td>
                      <td>{m.numero_doc}</td>
                      <td>{m.tipo_desc}</td>
                      <td className="text-right">{m.debito ? kz(m.debito) : ""}</td>
                      <td className="text-right">{m.credito ? kz(m.credito) : ""}</td>
                      <td className="text-right">{kz(m.saldo)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="mt-2 text-right font-bold">
                Em dívida {kz(e.pendente)} · Crédito {kz(e.credito)} · Saldo {kz(e.saldo)}
              </div>
            </div>
          </PrintPortal>
          {acao === "regularizar" && <RegularizarModal entidadeId={entidadeId} grupo={e.grupo} onClose={() => setAcao(null)} />}
          {acao === "devolver" && <DevolverModal entidadeId={entidadeId} grupo={e.grupo} onClose={() => setAcao(null)} />}
        </>
      )}
    </Modal>
  );
}

/** Regularização de conta corrente (Fr_RegularizacaoContaCorrente): não é documento fiscal. */
function RegularizarModal({ entidadeId, grupo, onClose }: { entidadeId: number; grupo: string; onClose: () => void }) {
  const qc = useQueryClient();
  const [tipo, setTipo] = useState<"CREDITO" | "DEBITO">("CREDITO");
  const [valor, setValor] = useState("");
  const [just, setJust] = useState("");
  const [busy, setBusy] = useState(false);
  async function gravar() {
    setBusy(true);
    try {
      await call("contas.regularizar", { entidade_id: entidadeId, grupo, tipo, valor: parseKz(valor), justificacao: just });
      toast.success("Regularização registada.");
      qc.invalidateQueries();
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title="Regularização de conta corrente"
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={parseKz(valor) <= 0 || just.trim().length < 5} onClick={gravar}>
            Gravar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Tipo">
          <Select value={tipo} onChange={(x) => setTipo(x.target.value as "CREDITO" | "DEBITO")}>
            <option value="CREDITO">Crédito — abate à dívida</option>
            <option value="DEBITO">Débito — anula crédito a favor</option>
          </Select>
        </Field>
        <Field label="Valor (Kz)">
          <Input className="num text-right" autoFocus value={valor} onChange={(x) => setValor(x.target.value)} />
        </Field>
        <Field label="Justificação" className="col-span-2">
          <Input value={just} placeholder="Ex.: acerto de arredondamentos acordado com o cliente" onChange={(x) => setJust(x.target.value)} />
        </Field>
      </div>
      <p className="mt-3 text-[12.5px] text-ink-3">
        {tipo === "CREDITO" ? "Abate a dívida dos documentos mais antigos." : "Consome créditos por utilizar (adiantamentos, notas de crédito)."} Fica registada no extracto e na auditoria; pode ser anulada.
      </p>
    </Modal>
  );
}

/** Devolução de dinheiro de um crédito (Fr_DevolucaoDinheiro). */
function DevolverModal({ entidadeId, grupo, onClose }: { entidadeId: number; grupo: string; onClose: () => void }) {
  const qc = useQueryClient();
  const creditos = useQuery({ queryKey: ["creditos", entidadeId, grupo], queryFn: () => call<Credito[]>("contas.creditos", { entidade_id: entidadeId, grupo }) }).data ?? [];
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<{ id: number; descricao: string; saldo: number }[]>("tesouraria.contas") }).data;
  const [credito, setCredito] = useState<number | null>(null);
  const [valor, setValor] = useState("");
  const [conta, setConta] = useState("");
  const [desc, setDesc] = useState("");
  const [busy, setBusy] = useState(false);
  const sel = creditos.find((c) => c.id === credito) ?? creditos[0];
  async function gravar() {
    if (!sel) return;
    setBusy(true);
    try {
      await call("contas.devolver", { credito_id: sel.id, valor: parseKz(valor), conta_id: Number(conta), descricao: desc || null });
      toast.success(grupo === "COMPRAS" ? "Reembolso registado na tesouraria." : "Devolução registada na tesouraria.");
      qc.invalidateQueries();
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={grupo === "COMPRAS" ? "Reembolso do fornecedor" : "Devolução de dinheiro"}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!sel || parseKz(valor) <= 0 || !conta} onClick={gravar}>
            Gravar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Documento de origem" className="col-span-2">
          <Select value={sel?.id ?? ""} onChange={(x) => setCredito(Number(x.target.value))}>
            {creditos.map((c) => (
              <option key={c.id} value={c.id}>
                {c.numero_doc} · {c.tipo_desc} · disponível {kz(c.credito_disponivel)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Valor (Kz)">
          <Input className="num text-right" autoFocus value={valor} onChange={(x) => setValor(x.target.value)} />
        </Field>
        <Field label={grupo === "COMPRAS" ? "Entra na conta" : "Sai da conta"}>
          <Select value={conta} onChange={(x) => setConta(x.target.value)}>
            <option value="">Seleccione…</option>
            {contas?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Descrição" className="col-span-2">
          <Input value={desc} placeholder={sel ? `Devolução ${sel.numero_doc}` : ""} onChange={(x) => setDesc(x.target.value)} />
        </Field>
      </div>
    </Modal>
  );
}

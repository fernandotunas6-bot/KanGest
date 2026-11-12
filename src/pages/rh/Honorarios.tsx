import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Banknote, FileSpreadsheet, HeartPulse, Pencil, Plus, Trash2, Undo2, UserRoundCog } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, kz, parseKz, qtd, toInputKz } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { MESES, type Funcionario } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { periodo } from "./Salarios";

interface Independente {
  id: number;
  nome: string;
  numero: string | null;
  nif: string | null;
  honorario: number;
  tipo_honorario: "MENSAL" | "DIA" | "HORA";
}
interface Honorario {
  id: number;
  funcionario_id: number;
  nome: string;
  nif: string | null;
  data: string;
  tipo: "MENSAL" | "DIA" | "HORA";
  valor_base: number;
  quantidade: number;
  iliquido: number;
  irt_pct: number;
  irt: number;
  liquido: number;
  descricao: string | null;
  estado: "PROCESSADO" | "ANULADO";
  motivo_anulacao: string | null;
  pago: number;
}
interface Prestacao {
  id: number;
  funcionario_id: number;
  nome: string;
  tipo: string;
  data_inicio: string;
  data_fim: string;
  liquido_mensal: number;
  meses: number;
  suspensao: number;
  observacoes: string | null;
}

const BASE: Record<string, string> = { MENSAL: "por mês", DIA: "por dia", HORA: "por hora" };
const TIPOS_PRESTACAO = ["Maternidade", "Paternidade", "Doença", "Acidente de trabalho", "Doença profissional", "Licença sem vencimento", "Outra"];

const VISTAS = [
  { id: "honorarios", label: "Honorários (conta própria)" },
  { id: "prestacoes", label: "Prestações sociais" },
] as const;

/** Trabalhadores por conta própria (IRT Grupo B) e prestações sociais (Fr_PrestacoesSociais). */
export default function Honorarios() {
  const [sp, setSp] = useSearchParams();
  const vista = sp.get("vista") === "prestacoes" ? "prestacoes" : "honorarios";
  const hoje = hojeISO();
  const [ano, setAno] = useState(Number(hoje.slice(0, 4)));
  const [mes, setMes] = useState(Number(hoje.slice(5, 7)));

  return (
    <>
      <PageHeader
        title="Honorários e Prestações Sociais"
        subtitle="Trabalhadores por conta própria com retenção de IRT e períodos de prestação da Segurança Social"
        actions={
          <div className="flex items-center gap-2">
            {vista === "honorarios" && (
              <Select className="w-36" value={mes} onChange={(e) => setMes(Number(e.target.value))}>
                {MESES.map((m, i) => (
                  <option key={m} value={i + 1}>
                    {m}
                  </option>
                ))}
              </Select>
            )}
            <Input className="num w-24" type="number" min={2000} max={2100} value={ano} onChange={(e) => setAno(Number(e.target.value))} />
          </div>
        }
      />
      <div className="mb-4 flex w-fit gap-1 rounded-lg bg-sunken p-1">
        {VISTAS.map((v) => (
          <button
            key={v.id}
            onClick={() => setSp({ vista: v.id })}
            className={cn("rounded-md px-3 py-1.5 text-[13px]", vista === v.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
          >
            {v.label}
          </button>
        ))}
      </div>
      {vista === "honorarios" ? <ListaHonorarios ano={ano} mes={mes} /> : <ListaPrestacoes ano={ano} />}
    </>
  );
}

// ======================================================================= honorários
function ListaHonorarios({ ano, mes }: { ano: number; mes: number }) {
  const q = useQuery({
    queryKey: ["rh-honorarios", ano, mes],
    queryFn: () => call<{ honorarios: Honorario[]; independentes: Independente[]; irt_pct: number }>("rh.honorarios", { ano, mes }),
  });
  const [novo, setNovo] = useState(false);
  const [pagar, setPagar] = useState<Honorario | null>(null);
  const [anular, setAnular] = useState<Honorario | null>(null);
  const lista = q.data?.honorarios ?? [];
  const validos = lista.filter((h) => h.estado === "PROCESSADO");
  const tot = (k: "iliquido" | "irt" | "liquido") => validos.reduce((s, h) => s + h[k], 0);

  async function exportar() {
    await exportarFicheiro(
      `Honorarios_${ano}-${String(mes).padStart(2, "0")}.csv`,
      csv([
        ["Nome", "NIF", "Data", "Base", "Quantidade", "Ilíquido", "Taxa IRT %", "IRT retido", "Líquido", "Pago"],
        ...validos.map((h) => [h.nome, h.nif, h.data, BASE[h.tipo], String(h.quantidade).replace(".", ","), csvKz(h.iliquido), String(h.irt_pct).replace(".", ","), csvKz(h.irt), csvKz(h.liquido), h.pago ? "Sim" : "Não"]),
      ]),
    );
  }

  return (
    <>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <p className="text-[13px] text-ink-3">
          {periodo(ano, mes)} · IRT retido à taxa de {qtd(q.data?.irt_pct ?? 6.5)}% (editável em Configuração Salarial)
        </p>
        <div className="flex gap-2">
          <Button variant="secondary" disabled={!validos.length} onClick={exportar}>
            <FileSpreadsheet className="size-4" /> Exportar
          </Button>
          <Button disabled={!q.data?.independentes.length} onClick={() => setNovo(true)}>
            <Plus className="size-4" /> Processar honorário
          </Button>
        </div>
      </div>
      <Card>
        {!lista.length ? (
          <Empty
            icon={<UserRoundCog className="size-6" />}
            title="Sem honorários neste mês"
            text={q.data?.independentes.length ? "Use «Processar honorário» para lançar o valor do mês." : "Marque um funcionário como trabalhador por conta própria na ficha (separador Remuneração)."}
          />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Trabalhador</Th>
                <Th>Base</Th>
                <Th className="text-right">Quant.</Th>
                <Th className="text-right">Ilíquido</Th>
                <Th className="text-right">IRT</Th>
                <Th className="text-right">Líquido</Th>
                <Th>Estado</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {lista.map((h) => (
                <tr key={h.id} className={cn(h.estado === "ANULADO" && "text-ink-3 line-through decoration-ink-3/40")}>
                  <Td>
                    <div className="font-medium">{h.nome}</div>
                    <div className="text-[12px] text-ink-3">{h.descricao ?? data(h.data)}</div>
                  </Td>
                  <Td>
                    {kz(h.valor_base)} {BASE[h.tipo]}
                  </Td>
                  <Td className="num text-right">{qtd(h.quantidade)}</Td>
                  <Td className="num text-right">{kz(h.iliquido)}</Td>
                  <Td className="num text-right">{kz(h.irt)}</Td>
                  <Td className="num text-right font-medium">{kz(h.liquido)}</Td>
                  <Td>
                    {h.estado === "ANULADO" ? (
                      <Badge tone="danger">Anulado</Badge>
                    ) : h.pago ? (
                      <Badge tone="ok">Pago</Badge>
                    ) : (
                      <Badge>Por pagar</Badge>
                    )}
                  </Td>
                  <Td className="text-right">
                    {h.estado === "PROCESSADO" && (
                      <div className="flex justify-end gap-1">
                        {!h.pago && (
                          <Button variant="secondary" onClick={() => setPagar(h)}>
                            <Banknote className="size-4" /> Pagar
                          </Button>
                        )}
                        <Button variant="ghost" title="Anular" onClick={() => setAnular(h)}>
                          <Undo2 className="size-4" />
                        </Button>
                      </div>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr className="font-semibold">
                <Td colSpan={3}>Total ({validos.length})</Td>
                <Td className="num text-right">{kz(tot("iliquido"))}</Td>
                <Td className="num text-right">{kz(tot("irt"))}</Td>
                <Td className="num text-right">{kz(tot("liquido"))}</Td>
                <Td colSpan={2} />
              </tr>
            </tfoot>
          </Table>
        )}
      </Card>
      {novo && q.data && <NovoHonorario ano={ano} mes={mes} independentes={q.data.independentes} taxa={q.data.irt_pct} onClose={() => setNovo(false)} />}
      {pagar && <PagarHonorario h={pagar} onClose={() => setPagar(null)} />}
      {anular && <AnularHonorario h={anular} onClose={() => setAnular(null)} />}
    </>
  );
}

function NovoHonorario({ ano, mes, independentes, taxa, onClose }: { ano: number; mes: number; independentes: Independente[]; taxa: number; onClose: () => void }) {
  const qc = useQueryClient();
  const [fid, setFid] = useState(String(independentes[0]?.id ?? ""));
  const ind = independentes.find((i) => String(i.id) === fid);
  const [base, setBase] = useState(toInputKz(independentes[0]?.honorario ?? 0));
  const [quant, setQuant] = useState(independentes[0]?.tipo_honorario === "MENSAL" ? "1" : "");
  const [desc, setDesc] = useState("");
  const [busy, setBusy] = useState(false);
  const q = Number(quant.replace(",", ".")) || 0;
  const iliquido = Math.round(parseKz(base) * (ind?.tipo_honorario === "MENSAL" && q <= 0 ? 1 : q));
  const irt = Math.round((iliquido * taxa) / 100);

  function escolher(id: string) {
    setFid(id);
    const i = independentes.find((x) => String(x.id) === id);
    setBase(toInputKz(i?.honorario ?? 0));
    setQuant(i?.tipo_honorario === "MENSAL" ? "1" : "");
  }

  async function gravar() {
    setBusy(true);
    try {
      await call("rh.honorarioProcessar", { funcionario_id: Number(fid), ano, mes, quantidade: q, valor_base: parseKz(base), descricao: desc || null });
      toast.success("Honorário processado.");
      qc.invalidateQueries({ queryKey: ["rh-honorarios"] });
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
      title={`Honorário de ${periodo(ano, mes)}`}
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!fid || iliquido <= 0} onClick={gravar}>
            Processar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Trabalhador por conta própria" className="col-span-2">
          <Select value={fid} onChange={(e) => escolher(e.target.value)}>
            {independentes.map((i) => (
              <option key={i.id} value={i.id}>
                {i.nome}
                {i.nif ? ` · NIF ${i.nif}` : ""}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={`Valor ${BASE[ind?.tipo_honorario ?? "MENSAL"]} (Kz)`}>
          <Input className="num text-right" value={base} onChange={(e) => setBase(e.target.value)} />
        </Field>
        <Field label={ind?.tipo_honorario === "HORA" ? "N.º de horas" : ind?.tipo_honorario === "DIA" ? "N.º de dias" : "Quantidade (meses)"}>
          <Input className="num text-right" inputMode="decimal" value={quant} onChange={(e) => setQuant(e.target.value)} autoFocus={ind?.tipo_honorario !== "MENSAL"} />
        </Field>
        <Field label="Descrição (opcional)" className="col-span-2">
          <Input value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="Serviço prestado" />
        </Field>
      </div>
      <div className="mt-5 grid grid-cols-3 gap-3 rounded-xl bg-sunken p-4 text-center">
        <div>
          <div className="text-[12px] uppercase tracking-wide text-ink-3">Ilíquido</div>
          <div className="num font-semibold">{kz(iliquido)}</div>
        </div>
        <div>
          <div className="text-[12px] uppercase tracking-wide text-ink-3">IRT {qtd(taxa)}%</div>
          <div className="num font-semibold text-danger">−{kz(irt)}</div>
        </div>
        <div>
          <div className="text-[12px] uppercase tracking-wide text-ink-3">Líquido</div>
          <div className="num text-[18px] font-semibold">{kz(iliquido - irt)}</div>
        </div>
      </div>
    </Modal>
  );
}

function PagarHonorario({ h, onClose }: { h: Honorario; onClose: () => void }) {
  const qc = useQueryClient();
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<{ id: number; descricao: string; saldo: number }[]>("tesouraria.contas") }).data;
  const [conta, setConta] = useState("");
  const [busy, setBusy] = useState(false);
  async function pagar() {
    setBusy(true);
    try {
      await call("rh.honorarioPagar", { id: h.id, conta_id: Number(conta) });
      toast.success("Pagamento registado na tesouraria.");
      qc.invalidateQueries();
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
      title={`Pagar honorário de ${h.nome}`}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!conta} onClick={pagar}>
            Registar pagamento
          </Button>
        </>
      }
    >
      <p className="mb-4 text-ink-2">
        Sai da tesouraria o líquido de <b className="text-ink">{kz(h.liquido)}</b>, no centro de custo «Salários». O IRT retido ({kz(h.irt)}) fica para entregar à AGT.
      </p>
      <Field label="Conta">
        <Select value={conta} onChange={(e) => setConta(e.target.value)}>
          <option value="">Seleccione…</option>
          {contas?.map((c) => (
            <option key={c.id} value={c.id}>
              {c.descricao} · saldo {kz(c.saldo)}
            </option>
          ))}
        </Select>
      </Field>
    </Modal>
  );
}

function AnularHonorario({ h, onClose }: { h: Honorario; onClose: () => void }) {
  const qc = useQueryClient();
  const [motivo, setMotivo] = useState("");
  const [busy, setBusy] = useState(false);
  async function anular() {
    setBusy(true);
    try {
      await call("rh.honorarioAnular", { id: h.id, motivo });
      toast.success("Honorário anulado.");
      qc.invalidateQueries({ queryKey: ["rh-honorarios"] });
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
      title={`Anular honorário de ${h.nome}`}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="danger" loading={busy} disabled={motivo.trim().length < 3 || !!h.pago} onClick={anular}>
            Anular
          </Button>
        </>
      }
    >
      {h.pago ? (
        <p className="mb-4 text-danger">Este honorário já foi pago. Anule primeiro o movimento na tesouraria (Fluxo de caixa).</p>
      ) : (
        <p className="mb-4 text-ink-2">O honorário fica anulado e pode voltar a processá-lo com os valores certos.</p>
      )}
      <Field label="Motivo">
        <Input autoFocus value={motivo} onChange={(e) => setMotivo(e.target.value)} />
      </Field>
    </Modal>
  );
}

// ======================================================================= prestações sociais
function ListaPrestacoes({ ano }: { ano: number }) {
  const qc = useQueryClient();
  const lista = useQuery({ queryKey: ["rh-prestacoes", ano], queryFn: () => call<Prestacao[]>("rh.prestacoes", { ano }) });
  const [edit, setEdit] = useState<Partial<Prestacao> | null>(null);

  async function eliminar(p: Prestacao) {
    try {
      await call("rh.prestacaoEliminar", { id: p.id });
      toast.success("Prestação eliminada.");
      qc.invalidateQueries({ queryKey: ["rh-prestacoes"] });
      qc.invalidateQueries({ queryKey: ["rh-simular"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <div className="mb-4 flex items-center justify-between gap-3">
        <p className="text-[13px] text-ink-3">Com «suspende o salário», o funcionário não entra no processamento dos meses do período (a prestação é paga pela Segurança Social).</p>
        <Button onClick={() => setEdit({ tipo: "Maternidade", data_inicio: hojeISO(), data_fim: hojeISO(), meses: 1, suspensao: 1, liquido_mensal: 0 })}>
          <Plus className="size-4" /> Nova prestação
        </Button>
      </div>
      <Card>
        {!lista.data?.length ? (
          <Empty icon={<HeartPulse className="size-6" />} title={`Sem prestações em ${ano}`} text="Registe licenças de maternidade, doença, acidentes de trabalho e outras prestações." />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Funcionário</Th>
                <Th>Tipo</Th>
                <Th>Período</Th>
                <Th className="text-right">Meses</Th>
                <Th className="text-right">Líquido mensal</Th>
                <Th>Salário</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {lista.data.map((p) => (
                <tr key={p.id}>
                  <Td>
                    <div className="font-medium">{p.nome}</div>
                    {p.observacoes && <div className="text-[12px] text-ink-3">{p.observacoes}</div>}
                  </Td>
                  <Td>{p.tipo}</Td>
                  <Td>
                    {data(p.data_inicio)} a {data(p.data_fim)}
                  </Td>
                  <Td className="num text-right">{p.meses}</Td>
                  <Td className="num text-right">{kz(p.liquido_mensal)}</Td>
                  <Td>{p.suspensao ? <Badge tone="danger">Suspenso</Badge> : <Badge>Mantém</Badge>}</Td>
                  <Td className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button variant="ghost" title="Editar" onClick={() => setEdit(p)}>
                        <Pencil className="size-4" />
                      </Button>
                      <Button variant="ghost" title="Eliminar" onClick={() => eliminar(p)}>
                        <Trash2 className="size-4" />
                      </Button>
                    </div>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {edit && <PrestacaoModal p={edit} onClose={() => setEdit(null)} />}
    </>
  );
}

function PrestacaoModal({ p, onClose }: { p: Partial<Prestacao>; onClose: () => void }) {
  const qc = useQueryClient();
  const funcs = useQuery({ queryKey: ["funcionarios", "", false], queryFn: () => call<Funcionario[]>("rh.funcionarios", { q: "" }) }).data?.filter((f) => !f.independente);
  const [f, setF] = useState<Partial<Prestacao>>(p);
  const [liquido, setLiquido] = useState(toInputKz(p.liquido_mensal ?? 0));
  const [busy, setBusy] = useState(false);
  const set = (k: keyof Prestacao, v: unknown) => setF((x) => ({ ...x, [k]: v }));

  async function gravar() {
    setBusy(true);
    try {
      await call("rh.prestacaoGuardar", {
        id: f.id ?? null,
        funcionario_id: Number(f.funcionario_id),
        tipo: f.tipo,
        data_inicio: f.data_inicio,
        data_fim: f.data_fim,
        liquido_mensal: parseKz(liquido),
        meses: Number(f.meses) || 1,
        suspensao: !!f.suspensao,
        observacoes: f.observacoes || null,
      });
      toast.success("Prestação gravada.");
      qc.invalidateQueries({ queryKey: ["rh-prestacoes"] });
      qc.invalidateQueries({ queryKey: ["rh-simular"] });
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
      title={f.id ? "Editar prestação social" : "Nova prestação social"}
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!f.funcionario_id || !f.tipo} onClick={gravar}>
            Gravar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Funcionário" className="col-span-2">
          <Select value={f.funcionario_id ?? ""} onChange={(e) => set("funcionario_id", e.target.value ? Number(e.target.value) : undefined)}>
            <option value="">Seleccione…</option>
            {funcs?.map((x) => (
              <option key={x.id} value={x.id}>
                {x.nome}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Tipo">
          <Select value={f.tipo ?? ""} onChange={(e) => set("tipo", e.target.value)}>
            {TIPOS_PRESTACAO.map((t) => (
              <option key={t}>{t}</option>
            ))}
          </Select>
        </Field>
        <Field label="N.º de meses">
          <Input className="num text-right" type="number" min={1} value={f.meses ?? 1} onChange={(e) => set("meses", Number(e.target.value))} />
        </Field>
        <Field label="Início">
          <Input type="date" value={f.data_inicio ?? ""} onChange={(e) => set("data_inicio", e.target.value)} />
        </Field>
        <Field label="Fim">
          <Input type="date" value={f.data_fim ?? ""} onChange={(e) => set("data_fim", e.target.value)} />
        </Field>
        <Field label="Líquido mensal da prestação (Kz)">
          <Input className="num text-right" value={liquido} onChange={(e) => setLiquido(e.target.value)} />
        </Field>
        <label className="flex h-10 items-center gap-2 self-end">
          <input type="checkbox" checked={!!f.suspensao} onChange={(e) => set("suspensao", e.target.checked ? 1 : 0)} /> Suspende o salário no período
        </label>
        <Field label="Observações" className="col-span-2">
          <Input value={f.observacoes ?? ""} onChange={(e) => set("observacoes", e.target.value)} />
        </Field>
      </div>
    </Modal>
  );
}

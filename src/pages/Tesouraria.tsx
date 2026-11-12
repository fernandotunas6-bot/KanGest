import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { ArrowDownLeft, ArrowRightLeft, ArrowUpRight, Ban, Banknote, Building2, Plus, Printer, Settings2 } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, inicioMesISO, kz, parseKz } from "@/lib/format";
import { imprimir } from "@/lib/print";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";

export interface Conta {
  id: number;
  descricao: string;
  tipo: "CAIXA" | "BANCO";
  banco: string | null;
  agencia: string | null;
  numero: string | null;
  iban: string | null;
  saldo_inicial: number;
  ativo: number;
  saldo: number;
}
export interface Centro {
  id: number;
  descricao: string;
  tipo: "DESPESA" | "RECEITA";
  ativo: number;
}
interface Mov {
  chave: string;
  data: string;
  data_hora: string;
  conta: string;
  tipo: "ENTRADA" | "SAIDA";
  valor: number;
  documento: string | null;
  descricao: string;
  rubrica: string | null;
  origem: "DOCUMENTO" | "CAIXA" | "TESOURARIA";
  origem_id: number;
  anulado: number;
}
interface Fluxo {
  movimentos: Mov[];
  saldo_inicial: number;
  entradas: number;
  saidas: number;
  saldo_final: number;
  a_receber: number;
  a_pagar: number;
  previsao: number;
}

const ABAS = ["Fluxo de Caixa", "Resumo Mensal", "Pagamentos por Centro de Custo", "Pagamentos por Contas"] as const;
const MESES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];

export default function Tesouraria() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const [conta, setConta] = useState<string>("");
  const [aba, setAba] = useState<(typeof ABAS)[number]>("Fluxo de Caixa");
  const [sp] = useSearchParams();
  useEffect(() => {
    const m: Record<string, (typeof ABAS)[number]> = { fluxo: "Fluxo de Caixa", mensal: "Resumo Mensal", centros: "Pagamentos por Centro de Custo", contas: "Pagamentos por Contas" };
    setAba(m[sp.get("aba") ?? ""] ?? "Fluxo de Caixa");
  }, [sp]);
  const [acao, setAcao] = useState<"" | "ENTRADA" | "SAIDA" | "DEPOSITO">("");
  const [anular, setAnular] = useState<Mov | null>(null);
  const [sel, setSel] = useState<string | null>(null);

  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<Conta[]>("tesouraria.contas") });
  const fluxo = useQuery({
    queryKey: ["tes-fluxo", de, ate, conta],
    queryFn: () => call<Fluxo>("tesouraria.fluxo", { de, ate, conta_id: conta ? Number(conta) : null }),
    placeholderData: (p) => p,
  });
  const mensal = useQuery({
    queryKey: ["tes-mensal", de.slice(0, 4)],
    queryFn: () => call<{ mes: string; entradas: number; saidas: number }[]>("tesouraria.resumoMensal", { ano: de.slice(0, 4) }),
    enabled: aba === "Resumo Mensal",
  });
  const porCentro = useQuery({
    queryKey: ["tes-centro", de, ate],
    queryFn: () =>
      call<{ centros: { rubrica: string; tipo: string; total: number; movimentos: number }[]; contas: { conta: string; entradas: number; saidas: number }[] }>(
        "tesouraria.porCentro",
        { de, ate },
      ),
    enabled: aba !== "Fluxo de Caixa" && aba !== "Resumo Mensal",
  });

  const f = fluxo.data;
  const atualizar = () => {
    qc.invalidateQueries({ queryKey: ["tes-fluxo"] });
    qc.invalidateQueries({ queryKey: ["tes-contas"] });
    qc.invalidateQueries({ queryKey: ["tes-mensal"] });
    qc.invalidateQueries({ queryKey: ["tes-centro"] });
  };
  const movSel = f?.movimentos.find((m) => m.chave === sel) ?? null;

  let saldo = f?.saldo_inicial ?? 0;
  const linhas = (f?.movimentos ?? []).map((m) => {
    if (!m.anulado) saldo += m.tipo === "ENTRADA" ? m.valor : -m.valor;
    return { ...m, saldo };
  });

  return (
    <>
      <PageHeader
        title="Fluxo de Caixa"
        subtitle="Tesouraria — entradas e saídas de todas as contas (caixa e bancos)"
        actions={
          <>
            <Button variant="secondary" onClick={() => nav("/tesouraria/configuracao")}>
              <Settings2 className="size-4" /> Contas e centros de custo
            </Button>
            <Button variant="secondary" onClick={imprimir}>
              <Printer className="size-4" /> Imprimir
            </Button>
          </>
        }
      />

      <div className="mb-4 flex flex-wrap items-end gap-3">
        <Field label="Data inicial">
          <Input type="date" className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        </Field>
        <Field label="Data final">
          <Input type="date" className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
        </Field>
        <Field label="Conta">
          <Select className="w-52" value={conta} onChange={(e) => setConta(e.target.value)}>
            <option value="">Todas as contas</option>
            {contas.data?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <div className="ml-auto flex gap-2">
          <Button variant="secondary" onClick={() => setAcao("ENTRADA")}>
            <ArrowDownLeft className="size-4 text-ok" /> Inserir
          </Button>
          <Button variant="secondary" onClick={() => setAcao("SAIDA")}>
            <ArrowUpRight className="size-4 text-danger" /> Pagamento
          </Button>
          <Button variant="secondary" onClick={() => setAcao("DEPOSITO")}>
            <ArrowRightLeft className="size-4" /> Depósito
          </Button>
          <Button variant="ghost" className="text-danger" disabled={!movSel || movSel.origem !== "TESOURARIA" || !!movSel.anulado} onClick={() => setAnular(movSel)}>
            <Ban className="size-4" /> Anular movimento
          </Button>
        </div>
      </div>

      <div className={cn("grid grid-cols-4 gap-4 transition-opacity", fluxo.isFetching && "opacity-60")}>
        <Stat label="Saldo inicial" value={kz(f?.saldo_inicial)} />
        <Stat label="Entradas" value={kz(f?.entradas)} tone="ok" icon={<ArrowDownLeft className="size-4" />} />
        <Stat label="Saídas" value={kz(f?.saidas)} tone="danger" icon={<ArrowUpRight className="size-4" />} />
        <Stat label="Saldo final" value={kz(f?.saldo_final)} hint={`Previsão com contas a receber e a pagar: ${kz(f?.previsao)}`} />
      </div>

      {/* em ecrãs até ~1536px os resumos passam para baixo da tabela, que fica com a largura toda */}
      <div className="mt-4 grid grid-cols-1 gap-4 2xl:grid-cols-[minmax(0,1fr)_320px]">
        <Card>
          <div className="flex gap-1 border-b border-line px-3 pt-2">
            {ABAS.map((a) => (
              <button
                key={a}
                onClick={() => setAba(a)}
                className={cn("-mb-px border-b-2 px-3 py-2 text-[13px]", aba === a ? "border-brand font-medium text-ink" : "border-transparent text-ink-2 hover:text-ink")}
              >
                {a}
              </button>
            ))}
          </div>
          {aba === "Fluxo de Caixa" && (
            <Table className="max-h-[calc(100vh-430px)]">
              <thead>
                <tr>
                  <Th>Data</Th>
                  <Th>Documento</Th>
                  <Th>Descrição</Th>
                  <Th>Conta</Th>
                  <Th className="text-right">Entrada</Th>
                  <Th className="text-right">Saída</Th>
                  <Th className="text-right">Saldo</Th>
                </tr>
              </thead>
              <tbody>
                {linhas.map((m) => (
                  <tr
                    key={m.chave}
                    onClick={() => setSel(m.chave)}
                    className={cn("cursor-pointer hover:bg-sunken/60", sel === m.chave && "bg-brand-50/60", m.anulado && "text-ink-3 line-through")}
                  >
                    <Td className="text-ink-2">{data(m.data)}</Td>
                    <Td className="font-mono text-[12px]">{m.documento ?? (m.origem === "CAIXA" ? "Caixa POS" : "—")}</Td>
                    <Td>
                      {m.descricao}
                      {m.rubrica && <span className="ml-2 text-[12px] text-ink-3">{m.rubrica}</span>}
                      {!!m.anulado && <Badge tone="danger" className="ml-2 no-underline">Anulado</Badge>}
                    </Td>
                    <Td className="text-ink-2">{m.conta}</Td>
                    <Td className="num text-right text-ok">{m.tipo === "ENTRADA" ? kz(m.valor) : ""}</Td>
                    <Td className="num text-right text-danger">{m.tipo === "SAIDA" ? kz(m.valor) : ""}</Td>
                    <Td className="num text-right font-medium">{kz(m.saldo)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
          {aba === "Fluxo de Caixa" && !linhas.length && <p className="py-10 text-center text-ink-3">Sem movimentos no período.</p>}

          {aba === "Resumo Mensal" && (
            <Table>
              <thead>
                <tr>
                  <Th>Mês ({de.slice(0, 4)})</Th>
                  <Th className="text-right">Entradas</Th>
                  <Th className="text-right">Saídas</Th>
                  <Th className="text-right">Resultado</Th>
                </tr>
              </thead>
              <tbody>
                {mensal.data?.map((m) => (
                  <tr key={m.mes}>
                    <Td>{MESES[Number(m.mes) - 1]}</Td>
                    <Td className="num text-right text-ok">{kz(m.entradas)}</Td>
                    <Td className="num text-right text-danger">{kz(m.saidas)}</Td>
                    <Td className={cn("num text-right font-medium", m.entradas - m.saidas < 0 && "text-danger")}>{kz(m.entradas - m.saidas)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}

          {aba === "Pagamentos por Centro de Custo" && (
            <Table>
              <thead>
                <tr>
                  <Th>Centro de custo / rubrica</Th>
                  <Th>Tipo</Th>
                  <Th className="text-right">Movimentos</Th>
                  <Th className="text-right">Total</Th>
                </tr>
              </thead>
              <tbody>
                {porCentro.data?.centros.map((c) => (
                  <tr key={c.rubrica + c.tipo}>
                    <Td>{c.rubrica}</Td>
                    <Td>{c.tipo === "SAIDA" ? <Badge tone="danger">Saída</Badge> : <Badge tone="ok">Entrada</Badge>}</Td>
                    <Td className="num text-right">{c.movimentos}</Td>
                    <Td className="num text-right font-medium">{kz(c.total)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}

          {aba === "Pagamentos por Contas" && (
            <Table>
              <thead>
                <tr>
                  <Th>Conta</Th>
                  <Th className="text-right">Entradas</Th>
                  <Th className="text-right">Saídas</Th>
                </tr>
              </thead>
              <tbody>
                {porCentro.data?.contas.map((c) => (
                  <tr key={c.conta}>
                    <Td>{c.conta}</Td>
                    <Td className="num text-right text-ok">{kz(c.entradas)}</Td>
                    <Td className="num text-right text-danger">{kz(c.saidas)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>

        <div className="grid grid-cols-2 content-start gap-4 2xl:grid-cols-1">
          <Card className="p-5">
            <div className="mb-3 font-semibold">Resumo de contas</div>
            <div className="space-y-2 text-ink-2">
              <div className="flex justify-between">
                <button className="hover:underline" onClick={() => nav("/contas/receber")}>A receber</button>
                <span className="num font-medium text-ink">{kz(f?.a_receber)}</span>
              </div>
              <div className="flex justify-between">
                <button className="hover:underline" onClick={() => nav("/contas/pagar")}>A pagar</button>
                <span className="num font-medium text-ink">{kz(f?.a_pagar)}</span>
              </div>
              <div className="flex justify-between border-t border-line pt-2">
                <span>Previsão</span>
                <span className="num font-semibold text-ink">{kz(f?.previsao)}</span>
              </div>
            </div>
          </Card>
          <Card className="p-5">
            <div className="mb-3 font-semibold">Saldo disponível</div>
            <ul className="space-y-2.5">
              {contas.data?.map((c) => (
                <li key={c.id} className="flex items-center gap-3">
                  <span className={cn("rounded-lg p-1.5", c.tipo === "CAIXA" ? "bg-accent-50 text-[#9a6514]" : "bg-brand-50 text-brand")}>
                    {c.tipo === "CAIXA" ? <Banknote className="size-4" /> : <Building2 className="size-4" />}
                  </span>
                  <span className="flex-1 text-[13.5px]">{c.descricao}</span>
                  <span className={cn("num font-medium", c.saldo < 0 && "text-danger")}>{kz(c.saldo)}</span>
                </li>
              ))}
            </ul>
          </Card>
        </div>
      </div>

      {acao && contas.data && (
        <MovimentoModal
          acao={acao}
          contas={contas.data}
          onClose={() => setAcao("")}
          onDone={() => {
            setAcao("");
            atualizar();
          }}
        />
      )}
      {anular && (
        <AnularModal
          mov={anular}
          onClose={() => setAnular(null)}
          onDone={() => {
            setAnular(null);
            setSel(null);
            atualizar();
          }}
        />
      )}

      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <div className="text-[16px] font-bold">Fluxo de Caixa</div>
          <div className="mb-3">
            {data(de)} a {data(ate)} · Saldo inicial {kz(f?.saldo_inicial)} · Entradas {kz(f?.entradas)} · Saídas {kz(f?.saidas)} · Saldo final {kz(f?.saldo_final)}
          </div>
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th>Data</th>
                <th>Documento</th>
                <th>Descrição</th>
                <th>Conta</th>
                <th className="text-right">Entrada</th>
                <th className="text-right">Saída</th>
                <th className="text-right">Saldo</th>
              </tr>
            </thead>
            <tbody>
              {linhas.map((m) => (
                <tr key={m.chave} className="border-b border-black/20">
                  <td>{data(m.data)}</td>
                  <td>{m.documento ?? ""}</td>
                  <td>
                    {m.descricao}
                    {m.anulado ? " (anulado)" : ""}
                  </td>
                  <td>{m.conta}</td>
                  <td className="text-right">{m.tipo === "ENTRADA" ? kz(m.valor) : ""}</td>
                  <td className="text-right">{m.tipo === "SAIDA" ? kz(m.valor) : ""}</td>
                  <td className="text-right">{kz(m.saldo)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PrintPortal>
    </>
  );
}

function MovimentoModal({ acao, contas, onClose, onDone }: { acao: "ENTRADA" | "SAIDA" | "DEPOSITO"; contas: Conta[]; onClose: () => void; onDone: () => void }) {
  const centros = useQuery({ queryKey: ["tes-centros"], queryFn: () => call<Centro[]>("tesouraria.centros") }).data ?? [];
  const [contaId, setContaId] = useState(contas[0]?.id ?? 1);
  const [para, setPara] = useState(contas.find((c) => c.tipo === "BANCO")?.id ?? contas[1]?.id ?? 2);
  const [valor, setValor] = useState("");
  const [descricao, setDescricao] = useState("");
  const [centro, setCentro] = useState<string>("");
  const [documento, setDocumento] = useState("");
  const [dataMov, setDataMov] = useState(hojeISO());
  const titulo = { ENTRADA: "Inserir dinheiro", SAIDA: "Pagamento (despesa)", DEPOSITO: "Depósito / transferência entre contas" }[acao];

  async function gravar() {
    try {
      if (acao === "DEPOSITO") {
        await call("tesouraria.transferencia", { de_conta: contaId, para_conta: para, valor: parseKz(valor), descricao });
      } else {
        await call("tesouraria.movimento", {
          tipo: acao,
          conta_id: contaId,
          valor: parseKz(valor),
          descricao,
          centro_custo_id: centro ? Number(centro) : null,
          documento,
          data: dataMov,
        });
      }
      toast.success("Movimento registado.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const tipoCentro = acao === "ENTRADA" ? "RECEITA" : "DESPESA";
  return (
    <Modal open onClose={onClose} title={titulo} width="max-w-lg" footer={<Button onClick={gravar}><Plus className="size-4" /> Gravar</Button>}>
      <div className="grid grid-cols-2 gap-4">
        <Field label={acao === "DEPOSITO" ? "Da conta" : "Conta"}>
          <Select value={contaId} onChange={(e) => setContaId(Number(e.target.value))}>
            {contas.map((c) => (
              <option key={c.id} value={c.id}>
                {c.descricao} ({kz(c.saldo)})
              </option>
            ))}
          </Select>
        </Field>
        {acao === "DEPOSITO" ? (
          <Field label="Para a conta">
            <Select value={para} onChange={(e) => setPara(Number(e.target.value))}>
              {contas.filter((c) => c.id !== contaId).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.descricao}
                </option>
              ))}
            </Select>
          </Field>
        ) : (
          <Field label="Data">
            <Input type="date" value={dataMov} onChange={(e) => setDataMov(e.target.value)} />
          </Field>
        )}
        <Field label="Valor (Kz)">
          <Input autoFocus className="num text-right" value={valor} onChange={(e) => setValor(e.target.value)} />
        </Field>
        {acao !== "DEPOSITO" && (
          <Field label="Centro de custo / rubrica">
            <Select value={centro} onChange={(e) => setCentro(e.target.value)}>
              <option value="">—</option>
              {centros.filter((c) => c.tipo === tipoCentro && c.ativo).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.descricao}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field label="Descrição" className="col-span-2">
          <Input value={descricao} onChange={(e) => setDescricao(e.target.value)} />
        </Field>
        {acao !== "DEPOSITO" && (
          <Field label="Documento (opcional)" className="col-span-2" hint="N.º de factura do fornecedor, talão, etc.">
            <Input value={documento} onChange={(e) => setDocumento(e.target.value)} />
          </Field>
        )}
      </div>
    </Modal>
  );
}

function AnularModal({ mov, onClose, onDone }: { mov: Mov; onClose: () => void; onDone: () => void }) {
  const [motivo, setMotivo] = useState("");
  async function anular() {
    try {
      await call("tesouraria.anular", { id: mov.origem_id, motivo });
      toast.success("Movimento anulado.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title="Anular movimento"
      width="max-w-md"
      footer={
        <Button variant="danger" onClick={anular}>
          Anular
        </Button>
      }
    >
      <p className="mb-4 text-ink-2">
        {mov.descricao} · <b>{kz(mov.valor)}</b>. As transferências são anuladas nos dois lados.
      </p>
      <Field label="Motivo">
        <Input autoFocus value={motivo} onChange={(e) => setMotivo(e.target.value)} />
      </Field>
    </Modal>
  );
}

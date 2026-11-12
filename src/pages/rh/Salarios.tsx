import { Fragment, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AlertTriangle, Banknote, ChevronDown, ChevronRight, FileSpreadsheet, HandCoins, Landmark, ListOrdered, Plus, Printer, Trash2, Undo2 } from "lucide-react";
import { call } from "@/lib/api";
import { data, dataHora, hojeISO, kz, parseKz, qtd } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { imprimir } from "@/lib/print";
import { MESES, type Alteracao, type Funcionario, type Processamento, type ReciboSalario, type ResumoSalario } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { useTabelasRH } from "./Funcionarios";

const VISTAS = [
  { id: "alteracoes", label: "Alterações do mês" },
  { id: "processar", label: "Simulação e processamento" },
  { id: "processamentos", label: "Recibos, mapas e pagamento" },
] as const;
type Vista = (typeof VISTAS)[number]["id"];

const DICAS: Record<string, string> = {
  "006": "Quantidade = n.º de horas; o valor é calculado pelo valor/hora e o acréscimo configurado.",
  "101": "Quantidade = n.º de dias de falta; desconta o valor/dia do salário base.",
  "004": "Quantidade = % do salário base (vazio = 50%) ou indique um valor fixo.",
  "005": "Quantidade = % do salário base (vazio = 50%) ou indique um valor fixo.",
};

export const periodo = (ano: number, mes: number) => `${MESES[mes - 1]} de ${ano}`;

type Impressao =
  | { tipo: "recibos"; p: Processamento; recibos: ReciboSalario[] }
  | { tipo: "mapa"; p: Processamento; mapa: Mapa }
  | { tipo: "rubrica"; p: Processamento; rubrica: RubricaMapa; recibos: ReciboSalario[] }
  | { tipo: "banco"; p: Processamento; mapa: Mapa }
  | null;

/** Rubrica escolhida para o mapa por rubrica (Mapa de Produtividade, Horas Extra, Abono de Família, Sub_*… do KIGEST). */
interface RubricaMapa {
  codigo: string;
  descricao: string;
  tipo: string;
}

/** Funcionários com a rubrica no processamento, com quantidade e valor. */
function linhasRubrica(recibos: ReciboSalario[], codigo: string) {
  return recibos
    .map((r) => {
      const ls = r.linhas.filter((l) => l.rubrica === codigo);
      return {
        r,
        quantidade: ls.some((l) => l.quantidade !== null) ? ls.reduce((s, l) => s + (l.quantidade ?? 0), 0) : null,
        valor: ls.reduce((s, l) => s + l.valor, 0),
        tem: ls.length > 0,
      };
    })
    .filter((x) => x.tem)
    .sort((a, b) => a.r.nome.localeCompare(b.r.nome));
}

export default function Salarios() {
  const [sp, setSp] = useSearchParams();
  const vista = (sp.get("vista") as Vista) || "alteracoes";
  const hoje = hojeISO();
  const [ano, setAno] = useState(Number(hoje.slice(0, 4)));
  const [mes, setMes] = useState(Number(hoje.slice(5, 7)));
  const [impressao, setImpressao] = useState<Impressao>(null);
  const empresa = useEmpresa();

  return (
    <>
      <PageHeader
        title="Processamento Salarial"
        subtitle="Salários, subsídios, INSS e IRT (Grupo A)"
        actions={
          <div className="flex items-center gap-2">
            <Select className="w-36" value={mes} onChange={(e) => setMes(Number(e.target.value))}>
              {MESES.map((m, i) => (
                <option key={m} value={i + 1}>
                  {m}
                </option>
              ))}
            </Select>
            <Input className="num w-24" type="number" min={2000} max={2100} value={ano} onChange={(e) => setAno(Number(e.target.value))} />
          </div>
        }
      />
      <div className="mb-4 flex gap-1 rounded-lg bg-sunken p-1 w-fit">
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
      {vista === "alteracoes" && <Alteracoes ano={ano} mes={mes} />}
      {vista === "processar" && <Simulacao ano={ano} mes={mes} onProcessado={() => setSp({ vista: "processamentos" })} />}
      {vista === "processamentos" && <Processamentos imprimirComo={(i) => { setImpressao(i); imprimir(); }} />}

      {impressao && (
        <PrintPortal>
          {impressao.tipo === "recibos" ? (
            impressao.recibos.map((r, i) => (
              <div key={r.id} style={{ breakAfter: i < impressao.recibos.length - 1 ? "page" : "auto" }}>
                <ReciboPrint r={r} p={impressao.p} empresa={empresa} />
              </div>
            ))
          ) : impressao.tipo === "banco" ? (
            <FolhaBanco p={impressao.p} mapa={impressao.mapa} empresa={empresa} />
          ) : impressao.tipo === "rubrica" ? (
            <div className="p-2 text-[11px] text-black">
              <CabecalhoEmpresa empresa={empresa} titulo={`Mapa — ${impressao.rubrica.descricao}`} numero={periodo(impressao.p.ano, impressao.p.mes)} />
              <TabelaRubrica recibos={impressao.recibos} rubrica={impressao.rubrica} />
            </div>
          ) : (
            <MapaPrint p={impressao.p} mapa={impressao.mapa} empresa={empresa} />
          )}
        </PrintPortal>
      )}
    </>
  );
}

// ======================================================================= alterações do mês
function Alteracoes({ ano, mes }: { ano: number; mes: number }) {
  const qc = useQueryClient();
  const tabs = useTabelasRH().data;
  const funcs = useQuery({ queryKey: ["funcionarios", "", false], queryFn: () => call<Funcionario[]>("rh.funcionarios", { q: "" }) }).data;
  const lista = useQuery({ queryKey: ["rh-alteracoes", ano, mes], queryFn: () => call<Alteracao[]>("rh.alteracoes", { ano, mes }) });
  const [form, setForm] = useState({ funcionario_id: "", rubrica: "006", quantidade: "", valor: "", descricao: "" });
  const [adiant, setAdiant] = useState(false);
  const [busy, setBusy] = useState(false);
  const rubricas = (tabs?.rubricas ?? []).filter((r) => r.codigo !== "001" && r.codigo !== "102");

  async function lancar() {
    setBusy(true);
    try {
      await call("rh.alteracaoGuardar", {
        funcionario_id: Number(form.funcionario_id),
        ano,
        mes,
        rubrica: form.rubrica,
        quantidade: form.quantidade ? Number(form.quantidade.replace(",", ".")) : null,
        valor: form.valor ? parseKz(form.valor) : null,
        descricao: form.descricao || null,
      });
      setForm((f) => ({ ...f, quantidade: "", valor: "", descricao: "" }));
      qc.invalidateQueries({ queryKey: ["rh-alteracoes"] });
      qc.invalidateQueries({ queryKey: ["rh-simular"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function eliminar(id: number) {
    try {
      await call("rh.alteracaoEliminar", { id });
      qc.invalidateQueries({ queryKey: ["rh-alteracoes"] });
      qc.invalidateQueries({ queryKey: ["rh-simular"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <Card className="mb-4 p-5">
        <div className="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1.3fr)_110px_150px_minmax(0,1fr)_auto] items-end gap-3">
          <Field label="Funcionário">
            <Select value={form.funcionario_id} onChange={(e) => setForm({ ...form, funcionario_id: e.target.value })}>
              <option value="">Seleccione…</option>
              {funcs?.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.nome}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Rubrica">
            <Select value={form.rubrica} onChange={(e) => setForm({ ...form, rubrica: e.target.value })}>
              {(["REMUNERACAO", "DESCONTO"] as const).map((t) => (
                <optgroup key={t} label={t === "REMUNERACAO" ? "Remunerações" : "Descontos"}>
                  {rubricas
                    .filter((r) => r.tipo === t)
                    .map((r) => (
                      <option key={r.codigo} value={r.codigo}>
                        {r.codigo} — {r.descricao}
                      </option>
                    ))}
                </optgroup>
              ))}
            </Select>
          </Field>
          <Field label="Quantidade">
            <Input className="num text-right" value={form.quantidade} onChange={(e) => setForm({ ...form, quantidade: e.target.value })} />
          </Field>
          <Field label="Valor (Kz)">
            <Input className="num text-right" value={form.valor} onChange={(e) => setForm({ ...form, valor: e.target.value })} />
          </Field>
          <Field label="Descrição">
            <Input value={form.descricao} onChange={(e) => setForm({ ...form, descricao: e.target.value })} onKeyDown={(e) => e.key === "Enter" && form.funcionario_id && lancar()} />
          </Field>
          <Button loading={busy} disabled={!form.funcionario_id} onClick={lancar}>
            <Plus className="size-4" /> Lançar
          </Button>
        </div>
        <div className="mt-2 flex items-center justify-between text-[12.5px] text-ink-3">
          <span>{DICAS[form.rubrica] ?? "Indique o valor do mês para esta rubrica."}</span>
          <Button size="sm" variant="secondary" onClick={() => setAdiant(true)}>
            <HandCoins className="size-4" /> Adiantamento salarial…
          </Button>
        </div>
      </Card>
      <Card>
        {!lista.data?.length ? (
          <Empty icon={<Plus className="size-6" />} title={`Sem alterações em ${periodo(ano, mes)}`} text="O salário base e os subsídios fixos da ficha entram automaticamente; aqui lançam-se só as variações do mês." />
        ) : (
          <Table className="tabela-compacta max-h-[calc(100vh-380px)]">
            <thead>
              <tr>
                <Th>Funcionário</Th>
                <Th>Rubrica</Th>
                <Th className="text-right">Quantidade</Th>
                <Th className="text-right">Valor</Th>
                <Th>Descrição</Th>
                <Th>Estado</Th>
                <Th className="w-10" />
              </tr>
            </thead>
            <tbody>
              {lista.data.map((a) => (
                <tr key={a.id}>
                  <Td className="font-medium">{a.nome}</Td>
                  <Td>
                    <span className="font-mono text-[12px] text-ink-3">{a.rubrica}</span> {a.rubrica_desc}
                  </Td>
                  <Td className="num text-right">{a.quantidade ? qtd(a.quantidade) : "—"}</Td>
                  <Td className={cn("num text-right", a.tipo === "DESCONTO" && "text-danger")}>{a.valor ? kz(a.valor) : "calculado"}</Td>
                  <Td className="text-ink-2">{a.descricao ?? "—"}</Td>
                  <Td>{a.processado ? <Badge tone="ok">Processada</Badge> : a.adiantamento_id ? <Badge tone="accent">Adiantamento</Badge> : <Badge>Por processar</Badge>}</Td>
                  <Td>
                    {!a.processado && (
                      <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => eliminar(a.id)}>
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
      {adiant && <AdiantamentoModal ano={ano} mes={mes} funcs={funcs ?? []} onClose={() => setAdiant(false)} />}
    </>
  );
}

function AdiantamentoModal({ ano, mes, funcs, onClose }: { ano: number; mes: number; funcs: Funcionario[]; onClose: () => void }) {
  const qc = useQueryClient();
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<{ id: number; descricao: string; saldo: number }[]>("tesouraria.contas") }).data;
  const [f, setF] = useState({ funcionario_id: "", valor: "", parcelas: "1", conta_id: "", descricao: "" });
  const [busy, setBusy] = useState(false);
  const valor = parseKz(f.valor);
  const parcelas = Math.max(1, Number(f.parcelas) || 1);
  async function gravar() {
    setBusy(true);
    try {
      await call("rh.adiantamento", {
        funcionario_id: Number(f.funcionario_id),
        valor,
        parcelas,
        ano,
        mes,
        conta_id: f.conta_id ? Number(f.conta_id) : null,
        descricao: f.descricao || null,
      });
      toast.success("Adiantamento registado.");
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
      title="Adiantamento salarial"
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!f.funcionario_id || valor <= 0} onClick={gravar}>
            Registar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Funcionário" className="col-span-2">
          <Select value={f.funcionario_id} onChange={(e) => setF({ ...f, funcionario_id: e.target.value })}>
            <option value="">Seleccione…</option>
            {funcs.map((x) => (
              <option key={x.id} value={x.id}>
                {x.nome}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Valor (Kz)">
          <Input className="num text-right" autoFocus value={f.valor} onChange={(e) => setF({ ...f, valor: e.target.value })} />
        </Field>
        <Field label="Parcelas (meses)" hint={valor > 0 ? `${kz(Math.floor(valor / parcelas))} por mês a partir de ${periodo(ano, mes)}` : undefined}>
          <Input className="num text-right" type="number" min={1} max={24} value={f.parcelas} onChange={(e) => setF({ ...f, parcelas: e.target.value })} />
        </Field>
        <Field label="Sai da conta" className="col-span-2" hint="Regista a saída de dinheiro na tesouraria">
          <Select value={f.conta_id} onChange={(e) => setF({ ...f, conta_id: e.target.value })}>
            <option value="">Não movimentar a tesouraria</option>
            {contas?.map((c) => (
              <option key={c.id} value={c.id}>
                {c.descricao} · saldo {kz(c.saldo)}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Descrição" className="col-span-2">
          <Input value={f.descricao} onChange={(e) => setF({ ...f, descricao: e.target.value })} />
        </Field>
      </div>
    </Modal>
  );
}

// ======================================================================= simulação e processamento
function Simulacao({ ano, mes, onProcessado }: { ano: number; mes: number; onProcessado: () => void }) {
  const qc = useQueryClient();
  const tabs = useTabelasRH().data;
  const sim = useQuery({ queryKey: ["rh-simular", ano, mes], queryFn: () => call<ResumoSalario[]>("rh.simular", { ano, mes }) });
  const procs = useQuery({ queryKey: ["rh-processamentos"], queryFn: () => call<Processamento[]>("rh.processamentos") }).data;
  const feito = procs?.find((p) => p.ano === ano && p.mes === mes && p.estado === "PROCESSADO");
  const [aberto, setAberto] = useState<number | null>(null);
  const [confirmar, setConfirmar] = useState(false);
  const [busy, setBusy] = useState(false);
  const linhas = sim.data ?? [];
  const soma = (k: keyof ResumoSalario) => linhas.reduce((s, l) => s + (Number(l[k]) || 0), 0);
  const outros = (l: ResumoSalario) => l.total_descontos - l.inss_trabalhador - l.irt;

  async function processar() {
    setBusy(true);
    try {
      const r = await call<{ funcionarios: number; total_liquido: number }>("rh.processar", { ano, mes });
      toast.success(`${periodo(ano, mes)} processado: ${r.funcionarios} recibos, ${kz(r.total_liquido)} líquidos.`);
      qc.invalidateQueries({ queryKey: ["rh-processamentos"] });
      qc.invalidateQueries({ queryKey: ["rh-alteracoes"] });
      setConfirmar(false);
      onProcessado();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      {tabs && !tabs.parametros.tabela_confirmada && (
        <div className="mb-4 flex items-start gap-3 rounded-xl border border-accent/40 bg-accent-50 px-4 py-3 text-[13px]">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-[#9a6514]" />
          <div>
            A tabela de IRT (Grupo A, Lei n.º 14/25) e as taxas de INSS (3% + 8%) vêm pré-carregadas e ainda não foram confirmadas.{" "}
            <Link to="/rh/configuracao" className="font-medium text-brand underline">
              Rever em Configuração salarial
            </Link>{" "}
            antes de processar.
          </div>
        </div>
      )}
      <Card>
        <div className="flex items-center justify-between border-b border-line px-5 py-3">
          <div>
            <span className="font-medium">{periodo(ano, mes)}</span>
            <span className="ml-2 text-ink-3">{feito ? `processado em ${dataHora(feito.data)}` : "pré-visualização — nada é gravado"}</span>
          </div>
          {feito ? (
            <Badge tone="ok">Processado</Badge>
          ) : (
            <Button disabled={!linhas.length} onClick={() => setConfirmar(true)}>
              Processar {MESES[mes - 1].toLowerCase()}
            </Button>
          )}
        </div>
        {!linhas.length ? (
          <Empty icon={<Banknote className="size-6" />} title="Sem funcionários activos" text="Crie as fichas dos funcionários com o salário base e os subsídios fixos." />
        ) : (
          <Table className="tabela-compacta max-h-[calc(100vh-330px)]">
            <thead>
              <tr>
                <Th className="w-8" />
                <Th>Funcionário</Th>
                <Th className="text-right">Salário base</Th>
                <Th className="text-right">Remunerações</Th>
                <Th className="text-right">INSS (3%)</Th>
                <Th className="text-right">IRT</Th>
                <Th className="text-right">Outros descontos</Th>
                <Th className="text-right">Líquido</Th>
                <Th className="text-right">INSS empresa</Th>
              </tr>
            </thead>
            <tbody>
              {linhas.map((l) => (
                <Fragment key={l.funcionario_id}>
                  <tr className="cursor-pointer hover:bg-sunken/60" onClick={() => setAberto(aberto === l.funcionario_id ? null : l.funcionario_id)}>
                    <Td className="text-ink-3">{aberto === l.funcionario_id ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}</Td>
                    <Td>
                      <div className="font-medium">{l.nome}</div>
                      <div className="text-[12px] text-ink-3">{[l.departamento, l.funcao].filter(Boolean).join(" · ")}</div>
                    </Td>
                    <Td className="num text-right text-ink-2">{kz(l.salario_base)}</Td>
                    <Td className="num text-right">{kz(l.total_remuneracoes)}</Td>
                    <Td className="num text-right text-ink-2">{kz(l.inss_trabalhador)}</Td>
                    <Td className="num text-right text-ink-2">{kz(l.irt)}</Td>
                    <Td className="num text-right text-ink-2">{kz(outros(l))}</Td>
                    <Td className="num text-right font-semibold">{kz(l.liquido)}</Td>
                    <Td className="num text-right text-ink-3">{kz(l.inss_empregador)}</Td>
                  </tr>
                  {aberto === l.funcionario_id && (
                    <tr>
                      <td colSpan={9} className="bg-sunken/50 px-12 py-3">
                        <LinhasRecibo linhas={l.linhas} />
                        <div className="mt-2 text-[12px] text-ink-3">
                          Base de incidência INSS {kz(l.base_inss)} · matéria colectável IRT {kz(l.base_irt)}
                        </div>
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
            <tfoot>
              <tr className="font-semibold">
                <Td />
                <Td>Total ({linhas.length})</Td>
                <Td className="num text-right">{kz(soma("salario_base"))}</Td>
                <Td className="num text-right">{kz(soma("total_remuneracoes"))}</Td>
                <Td className="num text-right">{kz(soma("inss_trabalhador"))}</Td>
                <Td className="num text-right">{kz(soma("irt"))}</Td>
                <Td className="num text-right">{kz(linhas.reduce((s, l) => s + outros(l), 0))}</Td>
                <Td className="num text-right">{kz(soma("liquido"))}</Td>
                <Td className="num text-right">{kz(soma("inss_empregador"))}</Td>
              </tr>
            </tfoot>
          </Table>
        )}
      </Card>
      {confirmar && (
        <Modal
          open
          onClose={() => setConfirmar(false)}
          title={`Processar ${periodo(ano, mes)}`}
          width="max-w-lg"
          footer={
            <>
              <Button variant="secondary" onClick={() => setConfirmar(false)}>
                Cancelar
              </Button>
              <Button loading={busy} onClick={processar}>
                Processar
              </Button>
            </>
          }
        >
          <p className="text-ink-2">
            São gravados {linhas.length} recibos, com um total líquido de <b className="text-ink">{kz(soma("liquido"))}</b>. As alterações do mês ficam fechadas; para corrigir, anule o processamento.
          </p>
        </Modal>
      )}
    </>
  );
}

function LinhasRecibo({ linhas }: { linhas: ResumoSalario["linhas"] }) {
  return (
    <table className="w-full max-w-3xl text-[13px]">
      <tbody>
        {linhas.map((x, i) => (
          <tr key={i}>
            <td className="w-14 py-0.5 font-mono text-[12px] text-ink-3">{x.rubrica}</td>
            <td className="py-0.5">{x.descricao}</td>
            <td className="num w-20 py-0.5 text-right text-ink-3">{x.quantidade ? qtd(x.quantidade) : ""}</td>
            <td className={cn("num w-36 py-0.5 text-right", x.tipo === "DESCONTO" && "text-danger")}>{x.tipo === "DESCONTO" ? `− ${kz(x.valor)}` : kz(x.valor)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ======================================================================= processamentos: recibos, mapas, pagamento
interface Mapa {
  linhas: {
    nome: string;
    nif: string | null;
    inss_numero: string | null;
    banco: string | null;
    iban: string | null;
    colunas: Record<string, number>;
    excesso_subsidios: number;
    salario_iliquido: number;
    base_inss: number;
    inss: number;
    inss_empregador: number;
    base_irt: number;
    irt: number;
    liquido: number;
  }[];
}

function Processamentos({ imprimirComo }: { imprimirComo: (i: Impressao) => void }) {
  const qc = useQueryClient();
  const lista = useQuery({ queryKey: ["rh-processamentos"], queryFn: () => call<Processamento[]>("rh.processamentos") });
  const [pagar, setPagar] = useState<Processamento | null>(null);
  const [anular, setAnular] = useState<Processamento | null>(null);
  const [mapa, setMapa] = useState<{ p: Processamento; m: Mapa } | null>(null);
  const [porRubrica, setPorRubrica] = useState<{ p: Processamento; recibos: ReciboSalario[] } | null>(null);
  async function abrirRubrica(p: Processamento) {
    try {
      const r = await call<{ recibos: ReciboSalario[] }>("rh.recibos", { id: p.id });
      setPorRubrica({ p, recibos: r.recibos });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function recibos(p: Processamento) {
    try {
      const r = await call<{ recibos: ReciboSalario[] }>("rh.recibos", { id: p.id });
      imprimirComo({ tipo: "recibos", p, recibos: r.recibos });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  async function abrirMapa(p: Processamento) {
    try {
      setMapa({ p, m: await call<Mapa>("rh.mapa", { id: p.id }) });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  const [banco, setBanco] = useState<{ p: Processamento; m: Mapa } | null>(null);
  async function folhaBancaria(p: Processamento) {
    try {
      const m = await call<Mapa>("rh.mapa", { id: p.id });
      setBanco({ p, m });
      const semIban = m.linhas.filter((l) => !l.iban).length;
      if (semIban) toast.warning(`${semIban} funcionário(s) sem IBAN na ficha.`);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <Card>
        {!lista.data?.length ? (
          <Empty icon={<Landmark className="size-6" />} title="Ainda sem processamentos" text="Processe o mês no separador Simulação e processamento." />
        ) : (
          <Table className="tabela-compacta">
            <thead>
              <tr>
                <Th>Período</Th>
                <Th>Processado em</Th>
                <Th className="text-right">Recibos</Th>
                <Th className="text-right">Total líquido</Th>
                <Th className="text-right">IRT</Th>
                <Th className="text-right">INSS (3% + 8%)</Th>
                <Th>Estado</Th>
                <Th className="text-right">Acções</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data.map((p) => {
                const ativo = p.estado === "PROCESSADO";
                return (
                  <tr key={p.id} className={cn(!ativo && "opacity-55")}>
                    <Td className="font-medium">{periodo(p.ano, p.mes)}</Td>
                    <Td className="text-ink-2">
                      {dataHora(p.data)} · {p.utilizador}
                    </Td>
                    <Td className="num text-right">{p.recibos}</Td>
                    <Td className="num text-right font-medium">{kz(p.total_liquido)}</Td>
                    <Td className="num text-right text-ink-2">{kz(p.total_irt)}</Td>
                    <Td className="num text-right text-ink-2">{kz(p.total_inss)}</Td>
                    <Td>
                      {!ativo ? (
                        <Badge tone="danger" className="max-w-56 truncate" >
                          Anulado{p.motivo_anulacao ? `: ${p.motivo_anulacao}` : ""}
                        </Badge>
                      ) : p.pago ? (
                        <Badge tone="ok">Pago</Badge>
                      ) : (
                        <Badge tone="accent">Por pagar</Badge>
                      )}
                    </Td>
                    <Td>
                      <div className="flex min-w-[210px] flex-wrap justify-end gap-1">
                        <Button size="sm" variant="ghost" onClick={() => recibos(p)} title="Imprimir os recibos de vencimento">
                          <Printer className="size-4" /> Recibos
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => abrirMapa(p)} title="Mapa de remunerações (IRT / INSS)">
                          <FileSpreadsheet className="size-4" /> Mapa
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => abrirRubrica(p)} title="Mapa de uma rubrica: horas extra, produtividade, abono de família, subsídios…">
                          <ListOrdered className="size-4" /> Rubrica
                        </Button>
                        {ativo && (
                          <Button size="sm" variant="ghost" onClick={() => folhaBancaria(p)} title="Ficheiro CSV com IBAN e valores para o banco">
                            <Landmark className="size-4" /> Banco
                          </Button>
                        )}
                        {ativo && !p.pago && (
                          <Button size="sm" variant="secondary" onClick={() => setPagar(p)}>
                            <Banknote className="size-4" /> Pagar
                          </Button>
                        )}
                        {ativo && (
                          <Button size="sm" variant="ghost" className="text-danger" onClick={() => setAnular(p)} title="Anular o processamento">
                            <Undo2 className="size-4" />
                          </Button>
                        )}
                      </div>
                    </Td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
        )}
      </Card>
      {pagar && <PagarModal p={pagar} onClose={() => setPagar(null)} />}
      {anular && (
        <AnularModal
          p={anular}
          onClose={() => setAnular(null)}
          onFeito={() => {
            qc.invalidateQueries({ queryKey: ["rh-processamentos"] });
            qc.invalidateQueries({ queryKey: ["rh-alteracoes"] });
          }}
        />
      )}
      {mapa && <MapaModal p={mapa.p} mapa={mapa.m} onClose={() => setMapa(null)} onImprimir={() => imprimirComo({ tipo: "mapa", p: mapa.p, mapa: mapa.m })} />}
      {banco && (
        <Modal
          open
          onClose={() => setBanco(null)}
          title={`Folha de pagamento ao banco — ${periodo(banco.p.ano, banco.p.mes)}`}
          width="max-w-4xl"
          footer={
            <>
              <Button
                variant="secondary"
                onClick={() =>
                  exportarFicheiro(
                    `Folha_bancaria_${banco.p.ano}-${String(banco.p.mes).padStart(2, "0")}.csv`,
                    csv([["Nome", "NIF", "Banco", "IBAN", "Valor"], ...banco.m.linhas.map((l) => [l.nome, l.nif, l.banco, l.iban, csvKz(l.liquido)])]),
                  )
                }
              >
                <FileSpreadsheet className="size-4" /> CSV
              </Button>
              <Button onClick={() => imprimirComo({ tipo: "banco", p: banco.p, mapa: banco.m })}>
                <Printer className="size-4" /> Imprimir
              </Button>
            </>
          }
        >
          <TabelaBanco mapa={banco.m} />
        </Modal>
      )}
      {porRubrica && (
        <RubricaModal
          p={porRubrica.p}
          recibos={porRubrica.recibos}
          onClose={() => setPorRubrica(null)}
          onImprimir={(rubrica) => imprimirComo({ tipo: "rubrica", p: porRubrica.p, rubrica, recibos: porRubrica.recibos })}
        />
      )}
    </>
  );
}

/** Linhas da folha de pagamento ao banco: só quem recebe algo. */
function TabelaBanco({ mapa }: { mapa: Mapa }) {
  const ls = mapa.linhas.filter((l) => l.liquido > 0);
  return (
    <Table className="tabela-compacta">
      <thead>
        <tr>
          <Th>Nome</Th>
          <Th>NIF</Th>
          <Th>Banco</Th>
          <Th>IBAN</Th>
          <Th className="text-right">Valor</Th>
        </tr>
      </thead>
      <tbody>
        {ls.map((l, i) => (
          <tr key={i}>
            <Td className="font-medium">{l.nome}</Td>
            <Td className="text-ink-2">{l.nif ?? "—"}</Td>
            <Td className="text-ink-2">{l.banco || "—"}</Td>
            <Td className={cn("font-mono text-[12px]", !l.iban && "text-danger")}>{l.iban || "sem IBAN"}</Td>
            <Td className="num text-right font-medium">{kz(l.liquido)}</Td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td colSpan={4}>
            Total · {ls.length} {ls.length === 1 ? "transferência" : "transferências"}
          </Td>
          <Td className="num text-right">{kz(ls.reduce((s, l) => s + l.liquido, 0))}</Td>
        </tr>
      </tfoot>
    </Table>
  );
}

/** Folha de pagamento ao banco para entregar assinada (FolhapagamentoBanco do KIGEST). */
function FolhaBanco({ p, mapa, empresa }: { p: Processamento; mapa: Mapa; empresa: ReturnType<typeof useEmpresa> }) {
  const total = mapa.linhas.filter((l) => l.liquido > 0).reduce((s, l) => s + l.liquido, 0);
  return (
    <div className="p-2 text-[11px] text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Folha de Pagamento" numero={periodo(p.ano, p.mes)} />
      <p className="my-4 text-[12px] leading-relaxed">
        Solicitamos a transferência dos salários de {periodo(p.ano, p.mes).toLowerCase()} para as contas abaixo indicadas, no valor total de <b>{kz(total)}</b>,
        com débito na nossa conta.
      </p>
      <TabelaBanco mapa={mapa} />
      <div className="mt-16 grid grid-cols-2 gap-16 text-center">
        <div className="border-t border-black pt-1">A Gerência</div>
        <div className="border-t border-black pt-1">Data e carimbo do banco</div>
      </div>
    </div>
  );
}

/** Mapa de uma rubrica no processamento (KIGEST: MapaProdutividade, MapaHorasExtras, MapaAbonoFamiliar, Sub_Chefia…). */
function RubricaModal({ p, recibos, onClose, onImprimir }: { p: Processamento; recibos: ReciboSalario[]; onClose: () => void; onImprimir: (r: RubricaMapa) => void }) {
  // rubricas presentes no mês, remunerações primeiro
  const rubricas = Array.from(
    new Map(recibos.flatMap((r) => r.linhas.map((l) => [l.rubrica, { codigo: l.rubrica, descricao: l.descricao, tipo: l.tipo }] as const))).values(),
  ).sort((a, b) => (a.tipo === b.tipo ? a.codigo.localeCompare(b.codigo) : a.tipo === "REMUNERACAO" ? -1 : b.tipo === "REMUNERACAO" ? 1 : a.tipo.localeCompare(b.tipo)));
  const [codigo, setCodigo] = useState(rubricas.find((r) => r.codigo !== "001")?.codigo ?? rubricas[0]?.codigo ?? "");
  const rubrica = rubricas.find((r) => r.codigo === codigo);
  function exportar() {
    if (!rubrica) return;
    const ls = linhasRubrica(recibos, rubrica.codigo);
    exportarFicheiro(
      `Mapa_${rubrica.descricao.replace(/[^\p{L}\p{N}]+/gu, "_")}_${p.ano}-${String(p.mes).padStart(2, "0")}.csv`,
      csv([["N.º", "Nome", "Departamento", "Função", "Quantidade", "Valor"], ...ls.map((x) => [x.r.numero ?? "", x.r.nome, x.r.departamento ?? "", x.r.funcao ?? "", x.quantidade ?? "", csvKz(x.valor)])]),
    );
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={`Mapa por rubrica — ${periodo(p.ano, p.mes)}`}
      width="max-w-4xl"
      footer={
        <>
          <Button variant="secondary" onClick={exportar} disabled={!rubrica}>
            <FileSpreadsheet className="size-4" /> CSV
          </Button>
          <Button onClick={() => rubrica && onImprimir(rubrica)} disabled={!rubrica}>
            <Printer className="size-4" /> Imprimir
          </Button>
        </>
      }
    >
      <Field label="Rubrica" className="mb-4 max-w-md">
        <Select value={codigo} onChange={(e) => setCodigo(e.target.value)}>
          {rubricas.map((r) => (
            <option key={r.codigo} value={r.codigo}>
              {r.codigo} — {r.descricao}
            </option>
          ))}
        </Select>
      </Field>
      {rubrica && <TabelaRubrica recibos={recibos} rubrica={rubrica} />}
    </Modal>
  );
}

function TabelaRubrica({ recibos, rubrica }: { recibos: ReciboSalario[]; rubrica: RubricaMapa }) {
  const ls = linhasRubrica(recibos, rubrica.codigo);
  const comQtd = ls.some((x) => x.quantidade !== null);
  return (
    <Table className="tabela-compacta">
      <thead>
        <tr>
          <Th>N.º</Th>
          <Th>Nome</Th>
          <Th>Departamento</Th>
          <Th>Função</Th>
          {comQtd && <Th className="text-right">Quantidade</Th>}
          <Th className="text-right">Valor</Th>
        </tr>
      </thead>
      <tbody>
        {ls.map((x) => (
          <tr key={x.r.id}>
            <Td className="font-mono text-[12px] text-ink-3">{x.r.numero ?? "—"}</Td>
            <Td className="font-medium">{x.r.nome}</Td>
            <Td className="text-ink-2">{x.r.departamento ?? "—"}</Td>
            <Td className="text-ink-2">{x.r.funcao ?? "—"}</Td>
            {comQtd && <Td className="num text-right">{x.quantidade === null ? "—" : qtd(x.quantidade)}</Td>}
            <Td className="num text-right font-medium">{kz(x.valor)}</Td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td colSpan={comQtd ? 5 : 4}>
            Total · {ls.length} {ls.length === 1 ? "funcionário" : "funcionários"}
          </Td>
          <Td className="num text-right">{kz(ls.reduce((s, x) => s + x.valor, 0))}</Td>
        </tr>
      </tfoot>
    </Table>
  );
}

function PagarModal({ p, onClose }: { p: Processamento; onClose: () => void }) {
  const qc = useQueryClient();
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<{ id: number; descricao: string; saldo: number }[]>("tesouraria.contas") }).data;
  const [conta, setConta] = useState("");
  const [busy, setBusy] = useState(false);
  async function pagar() {
    setBusy(true);
    try {
      await call("rh.pagar", { id: p.id, conta_id: Number(conta) });
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
      title={`Pagar salários de ${periodo(p.ano, p.mes)}`}
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
        Sai da tesouraria o total líquido de <b className="text-ink">{kz(p.total_liquido)}</b>, no centro de custo «Salários». Para pagar por banco use também o ficheiro «Banco».
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

function AnularModal({ p, onClose, onFeito }: { p: Processamento; onClose: () => void; onFeito: () => void }) {
  const [motivo, setMotivo] = useState("");
  const [busy, setBusy] = useState(false);
  async function anular() {
    setBusy(true);
    try {
      await call("rh.anular", { id: p.id, motivo });
      toast.success("Processamento anulado. As alterações do mês voltaram a ficar abertas.");
      onFeito();
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
      title={`Anular ${periodo(p.ano, p.mes)}`}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button variant="danger" loading={busy} disabled={motivo.trim().length < 3} onClick={anular}>
            Anular
          </Button>
        </>
      }
    >
      {p.pago ? (
        <p className="mb-4 text-danger">Os salários deste mês já foram pagos. Anule primeiro o movimento «Salários» na tesouraria (Fluxo de caixa).</p>
      ) : (
        <p className="mb-4 text-ink-2">Os recibos ficam anulados e pode corrigir as alterações e voltar a processar o mês.</p>
      )}
      <Field label="Motivo">
        <Input autoFocus value={motivo} onChange={(e) => setMotivo(e.target.value)} />
      </Field>
    </Modal>
  );
}

function MapaModal({ p, mapa, onClose, onImprimir }: { p: Processamento; mapa: Mapa; onClose: () => void; onImprimir: () => void }) {
  const ls = mapa.linhas;
  const soma = (k: "salario_iliquido" | "excesso_subsidios" | "base_inss" | "inss" | "inss_empregador" | "base_irt" | "irt" | "liquido") => ls.reduce((s, l) => s + l[k], 0);
  function exportar() {
    exportarFicheiro(
      `Mapa_remuneracoes_${p.ano}-${String(p.mes).padStart(2, "0")}.csv`,
      csv([
        ["Nome", "NIF", "N.º INSS", "Remuneração ilíquida", "Excesso de subsídios", "Base INSS", "INSS 3%", "INSS 8%", "Matéria colectável IRT", "IRT", "Líquido"],
        ...ls.map((l) => [l.nome, l.nif, l.inss_numero, csvKz(l.salario_iliquido), csvKz(l.excesso_subsidios), csvKz(l.base_inss), csvKz(l.inss), csvKz(l.inss_empregador), csvKz(l.base_irt), csvKz(l.irt), csvKz(l.liquido)]),
        ["Total", "", "", csvKz(soma("salario_iliquido")), csvKz(soma("excesso_subsidios")), csvKz(soma("base_inss")), csvKz(soma("inss")), csvKz(soma("inss_empregador")), csvKz(soma("base_irt")), csvKz(soma("irt")), csvKz(soma("liquido"))],
      ]),
    );
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={`Mapa de remunerações — ${periodo(p.ano, p.mes)}`}
      width="max-w-6xl"
      footer={
        <>
          <Button variant="secondary" onClick={exportar}>
            <FileSpreadsheet className="size-4" /> Exportar CSV
          </Button>
          <Button variant="secondary" onClick={onImprimir}>
            <Printer className="size-4" /> Imprimir
          </Button>
          <Button onClick={onClose}>Fechar</Button>
        </>
      }
    >
      <MapaTabela mapa={mapa} />
      <p className="mt-3 text-[12px] text-ink-3">
        IRT: declarar e pagar à AGT até ao fim do mês seguinte. INSS: 3% do trabalhador + 8% da entidade empregadora, sobre a base de incidência. Confirme os prazos em vigor.
      </p>
    </Modal>
  );
}

function MapaTabela({ mapa, impressao }: { mapa: Mapa; impressao?: boolean }) {
  const ls = mapa.linhas;
  const soma = (k: "salario_iliquido" | "excesso_subsidios" | "base_inss" | "inss" | "inss_empregador" | "base_irt" | "irt" | "liquido") => ls.reduce((s, l) => s + l[k], 0);
  const C = impressao ? "td" : Td;
  const H = impressao ? "th" : Th;
  const num = impressao ? "py-0.5 text-right" : "num text-right";
  return (
    <table className={cn("w-full border-collapse text-left", impressao && "text-[10px]")}>
      <thead>
        <tr className={cn(impressao && "border-y border-black")}>
          <H>Nome</H>
          <H>NIF</H>
          <H>N.º INSS</H>
          <H className="text-right">Ilíquido</H>
          <H className="text-right">Excesso subs.</H>
          <H className="text-right">Base INSS</H>
          <H className="text-right">INSS 3%</H>
          <H className="text-right">INSS 8%</H>
          <H className="text-right">Base IRT</H>
          <H className="text-right">IRT</H>
          <H className="text-right">Líquido</H>
        </tr>
      </thead>
      <tbody>
        {ls.map((l, i) => (
          <tr key={i} className={cn(impressao && "border-b border-black/20")}>
            <C className={cn(!impressao && "font-medium")}>{l.nome}</C>
            <C>{l.nif ?? "—"}</C>
            <C>{l.inss_numero ?? "—"}</C>
            <C className={num}>{kz(l.salario_iliquido)}</C>
            <C className={num}>{kz(l.excesso_subsidios)}</C>
            <C className={num}>{kz(l.base_inss)}</C>
            <C className={num}>{kz(l.inss)}</C>
            <C className={num}>{kz(l.inss_empregador)}</C>
            <C className={num}>{kz(l.base_irt)}</C>
            <C className={num}>{kz(l.irt)}</C>
            <C className={cn(num, "font-semibold")}>{kz(l.liquido)}</C>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className={cn("font-semibold", impressao && "border-t border-black")}>
          <C>Total ({ls.length})</C>
          <C />
          <C />
          <C className={num}>{kz(soma("salario_iliquido"))}</C>
          <C className={num}>{kz(soma("excesso_subsidios"))}</C>
          <C className={num}>{kz(soma("base_inss"))}</C>
          <C className={num}>{kz(soma("inss"))}</C>
          <C className={num}>{kz(soma("inss_empregador"))}</C>
          <C className={num}>{kz(soma("base_irt"))}</C>
          <C className={num}>{kz(soma("irt"))}</C>
          <C className={num}>{kz(soma("liquido"))}</C>
        </tr>
      </tfoot>
    </table>
  );
}

function MapaPrint({ p, mapa, empresa }: { p: Processamento; mapa: Mapa; empresa: ReturnType<typeof useEmpresa> }) {
  return (
    <div className="bg-white p-2 text-[11px] text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Mapa de Remunerações" numero={periodo(p.ano, p.mes)}>
        <div className="mt-1">IRT (Grupo A) e Segurança Social</div>
      </CabecalhoEmpresa>
      <div className="mt-4">
        <MapaTabela mapa={mapa} impressao />
      </div>
      <div className="mt-6 text-[10px]">Emitido em {data(hojeISO())}</div>
    </div>
  );
}

function ReciboPrint({ r, p, empresa }: { r: ReciboSalario; p: Processamento; empresa: ReturnType<typeof useEmpresa> }) {
  const rem = r.linhas.filter((l) => l.tipo !== "DESCONTO");
  const desc = r.linhas.filter((l) => l.tipo === "DESCONTO");
  return (
    <div className="mx-auto w-[190mm] bg-white p-2 text-[11.5px] leading-snug text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Recibo de Vencimento" numero={periodo(p.ano, p.mes)} />
      <div className="mt-4 grid grid-cols-3 gap-x-6 gap-y-1 rounded border border-black/40 p-3">
        <div className="col-span-2">
          <span className="text-black/60">Nome: </span>
          <b>{r.nome}</b>
        </div>
        <div>
          <span className="text-black/60">N.º: </span>
          {r.numero ?? "—"}
        </div>
        <div>
          <span className="text-black/60">NIF: </span>
          {r.nif ?? "—"}
        </div>
        <div>
          <span className="text-black/60">N.º INSS: </span>
          {r.inss_numero ?? "—"}
        </div>
        <div>
          <span className="text-black/60">Categoria: </span>
          {r.categoria ?? "—"}
        </div>
        <div>
          <span className="text-black/60">Função: </span>
          {r.funcao ?? "—"}
        </div>
        <div className="col-span-2">
          <span className="text-black/60">Departamento: </span>
          {r.departamento ?? "—"}
        </div>
      </div>
      <table className="mt-4 w-full border-collapse text-left">
        <thead>
          <tr className="border-y border-black">
            <th className="py-1">Cód.</th>
            <th className="py-1">Descrição</th>
            <th className="py-1 text-right">Qtd.</th>
            <th className="py-1 text-right">Remunerações</th>
            <th className="py-1 text-right">Descontos</th>
          </tr>
        </thead>
        <tbody>
          {[...rem, ...desc].map((l, i) => (
            <tr key={i} className="border-b border-black/15">
              <td className="py-0.5 font-mono">{l.rubrica}</td>
              <td className="py-0.5">{l.descricao}</td>
              <td className="py-0.5 text-right">{l.quantidade ? qtd(l.quantidade) : ""}</td>
              <td className="py-0.5 text-right">{l.tipo !== "DESCONTO" ? kz(l.valor) : ""}</td>
              <td className="py-0.5 text-right">{l.tipo === "DESCONTO" ? kz(l.valor) : ""}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr className="border-t border-black font-semibold">
            <td colSpan={3} className="py-1">
              Totais
            </td>
            <td className="py-1 text-right">{kz(r.total_remuneracoes)}</td>
            <td className="py-1 text-right">{kz(r.total_descontos)}</td>
          </tr>
        </tfoot>
      </table>
      <div className="mt-3 flex items-end justify-between">
        <div className="text-[10.5px] text-black/70">
          <div>Base de incidência INSS: {kz(r.base_inss)} · Matéria colectável IRT: {kz(r.base_irt)}</div>
          <div>Contribuição da entidade empregadora (INSS 8%): {kz(r.inss_empregador)}</div>
          {r.iban && (
            <div>
              Pagamento por transferência: {r.banco ?? ""} · IBAN {r.iban}
            </div>
          )}
        </div>
        <div className="rounded border-2 border-black px-4 py-2 text-right">
          <div className="text-[10px] uppercase">Líquido a receber</div>
          <div className="text-[16px] font-bold">{kz(r.liquido)}</div>
        </div>
      </div>
      <div className="mt-14 grid grid-cols-2 gap-16 text-center">
        <div className="border-t border-black pt-1">A entidade empregadora</div>
        <div className="border-t border-black pt-1">Recebi (o trabalhador)</div>
      </div>
    </div>
  );
}

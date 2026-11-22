// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useRef, useState, type FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { ArrowDownLeft, ArrowUpRight, History, Lock, Plus, Printer, ShoppingCart, Trash2, Wallet } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas, useTalao58 } from "@/lib/hooks";
import { dataHora, extenso, kz, kzNum, parseKz, qtd } from "@/lib/format";
import { imprimir } from "@/lib/print";
import type { CaixaResumo } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Segmentado, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { useEmpresa } from "@/components/PrintCabecalho";

interface SessaoLista {
  id: number;
  posto: string | null;
  utilizador: string;
  aberta_em: string;
  fechada_em: string | null;
  valor_inicial: number;
  valor_contado: number | null;
  estado: string;
  total_vendido: number;
  numero_z?: number | null;
}

export default function Caixa() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const tem = useSession((s) => s.tem);
  const { data: c, refetch } = useQuery({ queryKey: ["caixa"], queryFn: () => call<CaixaResumo | null>("caixa.estado") });
  const [mov, setMov] = useState(false);
  // talão de um movimento (vale de caixa): logo a seguir a registar ou para reimprimir
  const [vale, setVale] = useState<{ id: number; imprimir: boolean } | null>(null);
  const [fechar, setFechar] = useState(false);
  const [relatorio, setRelatorio] = useState<CaixaResumo | null>(null);
  const [historico, setHistorico] = useState(false);
  const [inicial, setInicial] = useState("0,00");

  async function abrir() {
    try {
      await call("caixa.abrir", { valor_inicial: parseKz(inicial) });
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const acoesTopo = (
    <Button variant="secondary" onClick={() => setHistorico(true)}>
      <History className="size-4" /> Fechos anteriores
    </Button>
  );

  return (
    <>
      {!c ? (
        <>
          <PageHeader title="Caixa" subtitle="Abertura, movimentos e fecho de caixa" actions={acoesTopo} />
          <Card className="mx-auto max-w-lg">
            <Empty
              icon={<Wallet className="size-6" />}
              title="Caixa fechado"
              text="Abra o caixa com o fundo de maneio para começar a vender."
              action={
                <div className="flex items-end gap-2">
                  <Field label="Fundo de maneio">
                    <Input className="num w-40 text-right" value={inicial} onChange={(e) => setInicial(e.target.value)} />
                  </Field>
                  <Button onClick={abrir}>Abrir caixa</Button>
                </div>
              }
            />
          </Card>
        </>
      ) : (
        <>
          <PageHeader
            title="Caixa"
            subtitle={`${c.sessao.posto ?? "Posto"} · aberto por ${c.sessao.utilizador} em ${dataHora(c.sessao.aberta_em)}`}
            actions={
              <>
                {acoesTopo}
                {tem("caixa.movimentos") && (
                  <Button variant="secondary" onClick={() => setMov(true)}>
                    Entrada / Saída de dinheiro
                  </Button>
                )}
                <Button variant="secondary" onClick={() => nav("/pos")}>
                  <ShoppingCart className="size-4" /> POS
                </Button>
                {tem("pos.fecho_caixa") && (
                  <Button variant="danger" onClick={() => setFechar(true)}>
                    <Lock className="size-4" /> Fechar caixa
                  </Button>
                )}
              </>
            }
          />
          <div className="grid grid-cols-4 gap-4">
            <Stat label="Fundo de maneio" value={kz(c.sessao.valor_inicial)} />
            <Stat label="Total recebido" value={c.cego ? "—" : kz(c.total_vendido)} hint={`${c.documentos} documentos`} />
            <Stat label="Entradas / Saídas" value={kz(c.entradas - c.saidas)} hint={`+${kz(c.entradas)} · −${kz(c.saidas)}`} tone="accent" />
            <Stat
              label="Numerário esperado na gaveta"
              value={c.cego ? "Fecho cego" : kz(c.numerario_esperado)}
              hint={
                c.cego
                  ? "Os totais aparecem no relatório depois de fechar"
                  : c.moedas_estrangeiras?.length
                    ? `E ainda: ${c.moedas_estrangeiras.map((m) => `${kzNum(m.valor)} ${m.moeda}`).join(" · ")}`
                    : undefined
              }
              tone="ok"
              icon={<Wallet className="size-4" />}
            />
          </div>
          <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2">
            <Card>
              <div className="px-5 pt-4 font-semibold">Recebimentos por modo de pagamento</div>
              <Table>
                <tbody>
                  {c.por_modo.map((m) => (
                    <tr key={m.codigo}>
                      <Td>{m.descricao}</Td>
                      <Td className="num text-right font-medium">{m.total === null ? "—" : kz(m.total)}</Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </Card>
            <Card>
              <div className="px-5 pt-4 font-semibold">Movimentos de caixa</div>
              <Table>
                <tbody>
                  {c.movimentos.map((m) => (
                    <tr key={m.id}>
                      <Td className="text-ink-2">{dataHora(m.data_hora)}</Td>
                      <Td>
                        <span className="inline-flex items-center gap-1.5">
                          {m.tipo === "ENTRADA" ? <ArrowDownLeft className="size-4 text-ok" /> : <ArrowUpRight className="size-4 text-danger" />}
                          {m.descricao}
                        </span>
                        {m.rubrica && <span className="ml-2 text-[12px] text-ink-3">{m.rubrica}</span>}
                        {m.beneficiario && <span className="block text-[12px] text-ink-3">{m.tipo === "ENTRADA" ? "Recebido de" : "Entregue a"} {m.beneficiario}</span>}
                      </Td>
                      <Td className={cn("num text-right", m.tipo === "ENTRADA" ? "text-ok" : "text-danger")}>
                        {m.tipo === "ENTRADA" ? "+" : "−"}
                        {kz(m.valor)}
                      </Td>
                      <Td className="w-8 text-right">
                        <button title="Imprimir o talão (vale de caixa)" onClick={() => setVale({ id: m.id, imprimir: false })} className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-ink">
                          <Printer className="size-4" />
                        </button>
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
              {!c.movimentos.length && <p className="py-8 text-center text-ink-3">Sem movimentos.</p>}
            </Card>
          </div>
        </>
      )}

      {mov && (
        <Movimento
          onClose={() => setMov(false)}
          onDone={(id, comTalao) => {
            setMov(false);
            refetch();
            if (id && comTalao) setVale({ id, imprimir: true });
          }}
        />
      )}
      {vale && <TalaoVale id={vale.id} imprimirLogo={vale.imprimir} onClose={() => setVale(null)} />}
      {fechar && c && (
        <Fechar
          resumo={c}
          onClose={() => setFechar(false)}
          onDone={(r) => {
            setFechar(false);
            setRelatorio(r);
            qc.setQueryData(["caixa"], null);
          }}
        />
      )}
      {historico && (
        <Historico
          onClose={() => setHistorico(false)}
          onAbrir={async (id) => {
            try {
              setRelatorio(await call<CaixaResumo>("caixa.obter", { id }));
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
        />
      )}
      {relatorio && <RelatorioFecho r={relatorio} onClose={() => setRelatorio(null)} />}
    </>
  );
}

interface LinhaVale {
  chave: number;
  descricao: string;
  rubrica: string;
  valor: string;
}

/** Entrada ou saída de dinheiro: uma ou várias linhas (ex.: combustível, água), a quem se entregou e o talão. */
function Movimento({ onClose, onDone }: { onClose: () => void; onDone: (movimentoId: number | null, comTalao: boolean) => void }) {
  const modos = useTabelas().data?.modos_pagamento ?? [];
  const [tipo, setTipo] = useState<"SAIDA" | "ENTRADA">("SAIDA");
  const [modo, setModo] = useState("NU");
  const [beneficiario, setBeneficiario] = useState("");
  const [linhas, setLinhas] = useState<LinhaVale[]>([{ chave: 1, descricao: "", rubrica: "", valor: "" }]);
  const [comTalao, setComTalao] = useState(true);
  const [busy, setBusy] = useState(false);
  const total = linhas.reduce((t, l) => t + parseKz(l.valor || "0"), 0);
  const mudar = (chave: number, m: Partial<LinhaVale>) => setLinhas((ls) => ls.map((l) => (l.chave === chave ? { ...l, ...m } : l)));

  async function guardar(e: FormEvent) {
    e.preventDefault();
    const usadas = linhas.filter((l) => l.descricao.trim() || l.valor.trim());
    if (!usadas.length) return toast.error("Indique a descrição e o valor.");
    setBusy(true);
    try {
      const r = await call<{ movimento_id?: number }>("caixa.movimento", {
        tipo,
        modo,
        beneficiario,
        linhas: usadas.map((l) => ({ descricao: l.descricao, rubrica: l.rubrica, valor: parseKz(l.valor || "0") })),
      });
      toast.success(tipo === "SAIDA" ? `Saída de ${kz(total)} registada.` : `Entrada de ${kz(total)} registada.`);
      onDone(r.movimento_id ?? null, comTalao);
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal open onClose={onClose} title="Entrada / saída de dinheiro" width="max-w-xl">
      <form onSubmit={guardar} className="space-y-4">
        <Segmentado
          valor={tipo}
          onChange={setTipo}
          opcoes={[
            { id: "SAIDA", label: "Saída (pagamento, sangria)", icon: ArrowUpRight },
            { id: "ENTRADA", label: "Entrada (reforço)", icon: ArrowDownLeft },
          ]}
        />
        <div className="grid grid-cols-2 gap-3">
          <Field label="Método">
            <Select value={modo} onChange={(e) => setModo(e.target.value)}>
              {modos.map((m) => (
                <option key={m.codigo} value={m.codigo}>
                  {m.descricao}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={tipo === "SAIDA" ? "Entregue a" : "Recebido de"}>
            <Input value={beneficiario} placeholder="Nome (opcional)" onChange={(e) => setBeneficiario(e.target.value)} />
          </Field>
        </div>
        <div>
          <div className="mb-1.5 grid grid-cols-[minmax(0,1fr)_76px_92px_28px] sm:grid-cols-[minmax(0,1fr)_120px_120px_32px] gap-2 text-[12px] font-medium text-ink-3">
            <span>Descrição</span>
            <span>Rubrica</span>
            <span className="text-right">Valor (Kz)</span>
            <span />
          </div>
          <div className="space-y-2">
            {linhas.map((l, n) => (
              <div key={l.chave} className="grid grid-cols-[minmax(0,1fr)_76px_92px_28px] sm:grid-cols-[minmax(0,1fr)_120px_120px_32px] items-center gap-2">
                <Input
                  autoFocus={n === 0}
                  aria-label="Descrição"
                  required={n === 0}
                  placeholder={n === 0 ? "Ex.: Combustível" : "Outra despesa"}
                  value={l.descricao}
                  onChange={(e) => mudar(l.chave, { descricao: e.target.value })}
                />
                <Input aria-label="Rubrica" placeholder="Opcional" value={l.rubrica} onChange={(e) => mudar(l.chave, { rubrica: e.target.value })} />
                <Input aria-label="Valor" required={n === 0} className="num text-right" inputMode="decimal" value={l.valor} onChange={(e) => mudar(l.chave, { valor: e.target.value })} />
                {linhas.length > 1 ? (
                  <button type="button" title="Tirar esta linha" onClick={() => setLinhas((ls) => ls.filter((x) => x.chave !== l.chave))} className="grid size-8 place-items-center rounded-md text-ink-3 hover:bg-sunken hover:text-danger">
                    <Trash2 className="size-4" />
                  </button>
                ) : (
                  <span />
                )}
              </div>
            ))}
          </div>
          <div className="mt-2 flex items-center justify-between">
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={linhas.length >= 30}
              onClick={() => setLinhas((ls) => [...ls, { chave: Math.max(...ls.map((x) => x.chave)) + 1, descricao: "", rubrica: "", valor: "" }])}
            >
              <Plus className="size-4" /> Acrescentar linha
            </Button>
            <span className="text-[14px]">
              Total <b className="num">{kz(total)}</b>
            </span>
          </div>
        </div>
        <label className="flex items-center gap-2 text-[13.5px]">
          <input type="checkbox" checked={comTalao} onChange={(e) => setComTalao(e.target.checked)} /> Imprimir o talão (vale de caixa) para assinar
        </label>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy}>Registar</Button>
        </div>
      </form>
    </Modal>
  );
}

interface Vale {
  id: number;
  tipo: "ENTRADA" | "SAIDA";
  data_hora: string;
  valor: number;
  descricao: string;
  rubrica: string | null;
  beneficiario: string | null;
  utilizador: string | null;
  posto: string | null;
  modo: string | null;
  linhas: { descricao: string; rubrica: string | null; valor: number }[];
}

/** Talão do movimento (vale de caixa): o que saiu ou entrou, linha a linha, com assinaturas. */
function TalaoVale({ id, imprimirLogo, onClose }: { id: number; imprimirLogo: boolean; onClose: () => void }) {
  const v = useQuery({ queryKey: ["caixa-vale", id], queryFn: () => call<Vale>("caixa.movimentoObter", { id }) }).data;
  const t58 = useTalao58();
  const e = useEmpresa();
  const impresso = useRef(false);
  // logo a seguir a registar: imprime sozinho, uma vez, quando já há os dados
  useEffect(() => {
    if (imprimirLogo && v && e && !impresso.current) {
      impresso.current = true;
      imprimir();
    }
  }, [imprimirLogo, v, e]);
  const linha = (k: string, val: string, forte = false) => (
    <div className={cn("flex justify-between gap-2", forte && "font-bold")}>
      <span>{k}</span>
      <span className="text-right">{val}</span>
    </div>
  );
  const saida = v?.tipo === "SAIDA";
  const linhas = v ? (v.linhas.length ? v.linhas : [{ descricao: v.descricao, rubrica: v.rubrica, valor: v.valor }]) : [];
  const conteudo = v && (
    <div
      className={cn("print-ticket mx-auto bg-white text-black", t58 ? "talao-58 w-[220px] p-2 text-[9.5px] leading-[1.3]" : "w-[300px] p-3 text-[11px] leading-[1.35]")}
      style={{ fontFamily: "Arial, Helvetica, sans-serif" }}
    >
      {e && (
        <div className="text-center">
          <div className="text-[13px] font-bold">{e.nome}</div>
          {e.endereco && <div>{e.endereco}</div>}
          {e.nif && <div>NIF: {e.nif}</div>}
        </div>
      )}
      <div className="mt-2 text-center text-[12.5px] font-bold">
        VALE DE CAIXA — {saida ? "SAÍDA" : "ENTRADA"} Nº {v.id}
      </div>
      {linha("Data:", dataHora(v.data_hora))}
      {v.posto && linha("Posto:", v.posto)}
      {v.utilizador && linha("Operador:", v.utilizador)}
      {v.beneficiario && linha(saida ? "Entregue a:" : "Recebido de:", v.beneficiario)}
      <div className="mt-2 flex justify-between border-y border-black py-0.5 font-bold">
        <span>Descrição</span>
        <span>Valor</span>
      </div>
      {linhas.map((l, n) => (
        <div key={n} className="flex justify-between gap-2">
          <span className="min-w-0">
            {l.descricao}
            {l.rubrica && <span> ({l.rubrica})</span>}
          </span>
          <span className="shrink-0">{kz(l.valor)}</span>
        </div>
      ))}
      <div className="mt-1 border-t border-black pt-1">
        {linha("TOTAL:", kz(v.valor), true)}
        {v.modo && linha("Método:", v.modo)}
      </div>
      <div className="mt-1">São: {extenso(v.valor)}</div>
      <div className="mt-6 grid grid-cols-2 gap-3 text-center">
        <div className="border-t border-black pt-0.5">O responsável</div>
        <div className="border-t border-black pt-0.5">{saida ? "Recebi" : "Entreguei"}</div>
      </div>
    </div>
  );
  return (
    <Modal open onClose={onClose} title="Talão do movimento de caixa" width="max-w-md" footer={<Button onClick={imprimir} disabled={!v}><Printer className="size-4" /> Imprimir</Button>}>
      <div className="flex max-h-[60vh] justify-center overflow-y-auto rounded-xl bg-sunken p-4">{conteudo ?? <span className="text-ink-3">A carregar…</span>}</div>
      {conteudo && <PrintPortal>{conteudo}</PrintPortal>}
    </Modal>
  );
}

/** Notas e moedas de kwanza (iguais às aceites pelo servidor). */
const NOTAS = [5000, 2000, 1000, 500, 200, 100, 50];
const MOEDAS = [20, 10, 5, 2, 1];

function Fechar({ resumo, onClose, onDone }: { resumo: CaixaResumo; onClose: () => void; onDone: (r: CaixaResumo) => void }) {
  const modos = useTabelas().data?.modos_pagamento ?? [];
  const [porNotas, setPorNotas] = useState(true);
  const [qtds, setQtds] = useState<Record<number, string>>({});
  const [contado, setContado] = useState("");
  const [outros, setOutros] = useState<Record<string, string>>({});
  const [obs, setObs] = useState("");
  const [busy, setBusy] = useState(false);
  const totalNotas = [...NOTAS, ...MOEDAS].reduce((s, d) => s + d * 100 * (Number(qtds[d]) || 0), 0);
  const valor = porNotas ? totalNotas : parseKz(contado || "0");
  const esperado = resumo.numerario_esperado;
  const dif = esperado === null ? null : valor - esperado;
  // outras formas de pagamento a conferir (talão do TPA, transferências…)
  const outrosModos = modos.filter((m) => m.codigo !== "NU" && m.codigo !== "CS" && (resumo.cego || (resumo.por_modo.find((p) => p.codigo === m.codigo)?.total ?? 0) !== 0));

  async function fechar() {
    setBusy(true);
    try {
      const contagem = porNotas ? Object.fromEntries(Object.entries(qtds).filter(([, q]) => Number(q) > 0).map(([d, q]) => [d, Number(q)])) : undefined;
      const contado_modos = Object.fromEntries(Object.entries(outros).filter(([, v]) => v.trim() !== "").map(([k, v]) => [k, parseKz(v)]));
      const r = await call<CaixaResumo>("caixa.fechar", {
        contagem: porNotas ? contagem : undefined,
        valor_contado: porNotas ? undefined : parseKz(contado || "0"),
        contado_modos,
        observacoes: obs || undefined,
      });
      toast.success(`Caixa fechado (Z n.º ${r.sessao.numero_z ?? r.sessao.id}).`);
      onDone(r);
    } catch (e) {
      toast.error((e as Error).message);
      setBusy(false);
    }
  }
  const celula = (d: number) => (
    <label key={d} className="flex items-center gap-2">
      <span className="num w-14 text-right text-ink-2">{d.toLocaleString("pt-PT")}</span>
      <span className="text-ink-3">×</span>
      <Input className="num h-8 w-16 text-right" inputMode="numeric" value={qtds[d] ?? ""} onChange={(e) => setQtds({ ...qtds, [d]: e.target.value.replace(/\D/g, "") })} />
    </label>
  );
  return (
    <Modal
      open
      onClose={onClose}
      title="Fecho de caixa"
      width="max-w-2xl"
      footer={
        <Button variant="danger" loading={busy} onClick={fechar}>
          Finalizar fecho
        </Button>
      }
    >
      <p className="mb-3 text-ink-2">
        {resumo.cego
          ? "Fecho cego: conte o dinheiro e os talões sem ver o valor do sistema. A diferença aparece no relatório, depois de fechar."
          : "Conte o numerário na gaveta. O sistema compara com o total esperado."}
      </p>
      <div className="mb-3 flex gap-2">
        <Button size="sm" variant={porNotas ? "primary" : "secondary"} onClick={() => setPorNotas(true)}>
          Contar notas e moedas
        </Button>
        <Button size="sm" variant={porNotas ? "secondary" : "primary"} onClick={() => setPorNotas(false)}>
          Indicar o total
        </Button>
      </div>
      {porNotas ? (
        <div className="grid grid-cols-2 gap-x-6 gap-y-2 rounded-xl border border-line p-4">
          <div className="space-y-2">
            <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Notas</div>
            {NOTAS.map(celula)}
          </div>
          <div className="space-y-2">
            <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Moedas</div>
            {MOEDAS.map(celula)}
          </div>
        </div>
      ) : (
        <Field label="Numerário contado (Kz)">
          <Input autoFocus className="num text-right text-lg" value={contado} onChange={(e) => setContado(e.target.value)} />
        </Field>
      )}
      {outrosModos.length > 0 && (
        <div className="mt-4 grid grid-cols-2 gap-3">
          {outrosModos.map((m) => (
            <Field key={m.codigo} label={`${m.descricao} (talão / extracto)`} hint="Deixe vazio se não confere esta forma">
              <Input className="num text-right" value={outros[m.codigo] ?? ""} onChange={(e) => setOutros({ ...outros, [m.codigo]: e.target.value })} />
            </Field>
          ))}
        </div>
      )}
      <Field label="Observações" className="mt-3">
        <Input value={obs} onChange={(e) => setObs(e.target.value)} placeholder="Ex.: nota de 1000 Kz rasgada; reforço de trocos" />
      </Field>
      <div className="mt-4 space-y-1 rounded-xl bg-sunken p-4">
        <div className="flex justify-between font-medium">
          <span>Numerário contado</span>
          <span className="num">{kz(valor)}</span>
        </div>
        {esperado !== null && dif !== null && (
          <>
            <div className="flex justify-between">
              <span>Total do sistema (numerário)</span>
              <span className="num">{kz(esperado)}</span>
            </div>
            <div className={cn("flex justify-between font-medium", dif < 0 ? "text-danger" : dif > 0 ? "text-accent-ink" : "text-ok")}>
              <span>Diferença</span>
              <span className="num">{kz(dif)}</span>
            </div>
          </>
        )}
      </div>
    </Modal>
  );
}

function Historico({ onClose, onAbrir }: { onClose: () => void; onAbrir: (id: number) => void }) {
  const { data } = useQuery({ queryKey: ["caixa-sessoes"], queryFn: () => call<SessaoLista[]>("caixa.sessoes") });
  return (
    <Modal open onClose={onClose} title="Fechos de caixa" width="max-w-4xl">
      <Table className="max-h-[60vh]">
        <thead>
          <tr>
            <Th>Z</Th>
            <Th>Posto</Th>
            <Th>Operador</Th>
            <Th>Abertura</Th>
            <Th>Fecho</Th>
            <Th className="text-right">Vendido</Th>
            <Th className="text-right">Contado</Th>
            <Th />
          </tr>
        </thead>
        <tbody>
          {data?.map((s) => (
            <tr key={s.id}>
              <Td className="num text-ink-2">{s.numero_z ?? "—"}</Td>
              <Td>{s.posto ?? "—"}</Td>
              <Td>{s.utilizador}</Td>
              <Td className="text-ink-2">{dataHora(s.aberta_em)}</Td>
              <Td className="text-ink-2">{s.estado === "ABERTA" ? <Badge tone="ok">Aberto</Badge> : dataHora(s.fechada_em)}</Td>
              <Td className="num text-right">{kz(s.total_vendido)}</Td>
              <Td className="num text-right">{s.valor_contado === null ? "—" : kz(s.valor_contado)}</Td>
              <Td>
                <Button size="sm" variant="ghost" onClick={() => onAbrir(s.id)}>
                  <Printer className="size-4" /> Ver
                </Button>
              </Td>
            </tr>
          ))}
        </tbody>
      </Table>
      {!data?.length && <p className="py-8 text-center text-ink-3">Ainda não há fechos de caixa.</p>}
    </Modal>
  );
}

function RelatorioFecho({ r, onClose }: { r: CaixaResumo; onClose: () => void }) {
  const dif = (r.sessao.valor_contado ?? 0) - (r.numerario_esperado ?? 0);
  const t58 = useTalao58();
  const e = useEmpresa();
  const tabelas = useTabelas();
  const armazem = tabelas.data?.armazens.find((a) => a.id === tabelas.data?.armazem_id);
  const linha = (k: string, v: string, forte = false) => (
    <div className={cn("flex justify-between gap-2", forte && "font-bold")}>
      <span>{k}</span>
      <span>{v}</span>
    </div>
  );
  // desenho do FechoCaixa.fr3 do KIGEST POS: empresa, n.º do fecho, datas, armazém, posto e vendedor,
  // valores por forma de pagamento, total, valor físico e diferença
  const conteudo = (
    <div
      className={cn("print-ticket mx-auto bg-white text-black", t58 ? "talao-58 w-[220px] p-2 text-[9.5px] leading-[1.3]" : "w-[300px] p-3 text-[11px] leading-[1.35]")}
      style={{ fontFamily: "Arial, Helvetica, sans-serif" }}
    >
      {e && (
        <div className="text-center">
          <div className="text-[13px] font-bold">{e.nome}</div>
          {e.designacao_comercial && e.designacao_comercial !== e.nome && <div>{e.designacao_comercial}</div>}
          {e.endereco && <div>{e.endereco}</div>}
          {e.telefone && <div>Telf.: {e.telefone}</div>}
          <div>{e.localidade || e.provincia || "Luanda"} - Angola</div>
        </div>
      )}
      <div className="mt-2 text-[12.5px] font-bold">
        {r.sessao.estado === "ABERTA" ? "CAIXA EM ABERTO (X)" : `FECHO DE CAIXA (Z) Nº ${r.sessao.numero_z ?? r.sessao.id}`}
      </div>
      {linha("Data Abertura:", dataHora(r.sessao.aberta_em))}
      {linha("Data de Fecho:", r.sessao.fechada_em ? dataHora(r.sessao.fechada_em) : "(em aberto)")}
      {armazem && linha("Armazém:", armazem.nome)}
      {linha("Posto:", r.sessao.posto ?? "—")}
      {linha("Vendedor:", r.sessao.utilizador)}
      <div className="mt-2 flex justify-between border-y border-black py-0.5 font-bold">
        <span>Descrição Pagamento</span>
        <span>Valor</span>
      </div>
      {r.por_modo.map((m) => (
        <div key={m.codigo}>{linha(m.descricao, m.total === null ? "—" : kz(m.total))}</div>
      ))}
      <div className="mt-1 border-t border-black pt-1">
        {linha(`Total de ${r.documentos} documento(s):`, kz(r.total_vendido), true)}
        {linha("Fundo de maneio", kz(r.sessao.valor_inicial))}
        {linha("Entradas", kz(r.entradas))}
        {linha("Saídas", kz(r.saidas))}
        {linha("Numerário esperado:", kz(r.numerario_esperado), true)}
        {(r.moedas_estrangeiras ?? []).map((m) => linha(`Em ${m.moeda} (na gaveta):`, `${kzNum(m.valor)} ${m.moeda}`))}
        {linha("Valor Físico:", r.sessao.valor_contado === null ? "—" : kz(r.sessao.valor_contado), true)}
        {r.sessao.valor_contado !== null && linha(dif < 0 ? "Diferença (falta):" : dif > 0 ? "Diferença (sobra):" : "Diferença:", kz(dif), true)}
      </div>
      {r.sessao.contagem && Object.keys(r.sessao.contagem).length > 0 && (
        <>
          <div className="mt-2 border-y border-black py-0.5 text-center">Contagem do numerário</div>
          {Object.entries(r.sessao.contagem)
            .sort(([a], [b]) => Number(b) - Number(a))
            .map(([d, q]) => (
              <div key={d}>{linha(`${Number(d).toLocaleString("pt-PT")} Kz × ${q}`, kz(Number(d) * 100 * q))}</div>
            ))}
        </>
      )}
      {(r.conferencia ?? []).some((c) => c.codigo !== "NU" && c.contado !== null) && (
        <>
          <div className="mt-2 border-y border-black py-0.5 text-center">Conferência (contado − sistema)</div>
          {(r.conferencia ?? [])
            .filter((c) => c.codigo !== "NU" && c.contado !== null)
            .map((c) => (
              <div key={c.codigo}>{linha(`${c.descricao}: ${kz(c.contado)}`, kz(c.diferenca), c.diferenca !== 0)}</div>
            ))}
        </>
      )}
      {r.sessao.observacoes && <div className="mt-2">Obs.: {r.sessao.observacoes}</div>}
      {r.artigos.length > 0 && (
        <>
          <div className="mt-2 border-y border-black py-0.5 text-center">Artigos vendidos</div>
          {r.artigos.map((a) => (
            <div key={a.referencia} className="flex justify-between gap-2">
              <span className="truncate">
                {qtd(a.quantidade)} x {a.descricao}
              </span>
              <span>{kz(a.total)}</span>
            </div>
          ))}
        </>
      )}
      {e?.rodape && <div className="mt-2 text-center">{e.rodape}</div>}
    </div>
  );
  return (
    <Modal open onClose={onClose} title="Relatório de fecho" width="max-w-md" footer={<Button onClick={imprimir}><Printer className="size-4" /> Imprimir</Button>}>
      <div className="flex max-h-[60vh] justify-center overflow-y-auto rounded-xl bg-sunken p-4">{conteudo}</div>
      {r.sessao.valor_contado !== null && dif !== 0 && <Badge tone={dif < 0 ? "danger" : "accent"} className="mt-3">Diferença de {kz(dif)}</Badge>}
      <PrintPortal>{conteudo}</PrintPortal>
    </Modal>
  );
}

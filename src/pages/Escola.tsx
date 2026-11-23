// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useMemo, useState, type FormEvent } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ArrowRightLeft, Ban, FileCheck2, GraduationCap, HandCoins, Plus, Printer, Search, Trash2, Wallet } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, kz, parseKz, toInputKz } from "@/lib/format";
import { imprimir } from "@/lib/print";
import type { Entidade } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Confirmar, Empty, Field, Input, Modal, PageHeader, Segmentado, Select, Stat, Table, Td, Th, cn, BlocoCarregando } from "@/components/ui";
import { EntidadePicker } from "@/components/Pickers";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";

/**
 * Escola, só a parte financeira: o aluno é o cliente que paga; a turma diz a propina; a matrícula gera a
 * propina de cada mês (uma factura, sem repetir). Recebe-se com o recibo normal do KanGest.
 */
interface Ano {
  id: number;
  designacao: string;
  inicio: string;
  fim: string;
  corrente: number;
  alunos: number;
}
interface Turma {
  id: number;
  ano_lectivo_id: number;
  classe: string;
  turma: string;
  propina: number;
  ativo: number;
  alunos: number;
}
type Estado = "PAGA" | "PARCIAL" | "DIVIDA" | "ANULADA" | "POR_FACTURAR" | "FUTURO" | "FORA" | "BOLSA";
interface Celula {
  mes: string;
  estado: Estado;
  total?: number;
  pendente?: number;
  numero_doc?: string | null;
}
interface LinhaPropina {
  matricula_id: number;
  entidade_id: number;
  avenca_id: number | null;
  nome: string;
  nif: string;
  telefone: string | null;
  turma_id: number;
  turma: string;
  estado: "ACTIVA" | "ANULADA";
  desconto: number;
  valor_mes: number;
  celulas: Celula[];
  em_divida: number;
  pago: number;
}
interface Propinas {
  ano_lectivo_id: number | null;
  meses: string[];
  linhas: LinhaPropina[];
  totais?: { em_divida: number; pago: number; meses_por_facturar: number };
  avencas_por_facturar?: number[];
}
interface Devedores {
  total: number;
  turmas: { turma_id: number; turma: string; total: number; alunos: { entidade_id: number; nome: string; telefone: string | null; meses: number; desde: string; valor: number }[] }[];
}

type Vista = "propinas" | "devedores" | "turmas";
const MESES = ["Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez"];
const nomeMes = (m: string) => `${MESES[Number(m.slice(5, 7)) - 1]} ${m.slice(2, 4)}`;

const CELULA: Record<Estado, { classe: string; texto: string; titulo: string }> = {
  PAGA: { classe: "bg-ok-50 text-ok", texto: "✓", titulo: "Paga" },
  PARCIAL: { classe: "bg-accent-50 text-accent-ink", texto: "½", titulo: "Paga em parte" },
  DIVIDA: { classe: "bg-danger-50 text-danger", texto: "!", titulo: "Em dívida" },
  ANULADA: { classe: "bg-sunken text-ink-3 line-through", texto: "×", titulo: "Factura anulada" },
  POR_FACTURAR: { classe: "border border-dashed border-line-strong text-ink-3", texto: "•", titulo: "Por facturar" },
  FUTURO: { classe: "text-ink-3/50", texto: "·", titulo: "Ainda não" },
  FORA: { classe: "text-ink-3/50", texto: "—", titulo: "Fora da matrícula" },
  BOLSA: { classe: "bg-brand-50 text-brand", texto: "B", titulo: "Bolsa integral" },
};

export default function Escola() {
  const [sp, setSp] = useSearchParams();
  const vista = (sp.get("vista") as Vista) || "propinas";
  const [anoId, setAnoId] = useState<number | null>(null);
  const anos = useQuery({ queryKey: ["escola-anos"], queryFn: () => call<Ano[]>("escola.anos") });
  const ano = anos.data?.find((a) => a.id === anoId) ?? anos.data?.find((a) => a.corrente) ?? anos.data?.[0] ?? null;

  return (
    <div>
      <PageHeader
        title="Escola"
        subtitle={ano ? `Ano lectivo ${ano.designacao}` : "Propinas, pagamentos e devedores"}
        actions={
          anos.data && anos.data.length > 1 ? (
            <Select className="w-40" aria-label="Ano lectivo" value={ano?.id ?? ""} onChange={(e) => setAnoId(Number(e.target.value))}>
              {anos.data.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.designacao}
                </option>
              ))}
            </Select>
          ) : undefined
        }
      />
      <Segmentado
        className="mb-4 w-fit"
        valor={vista}
        onChange={(v) => setSp(v === "propinas" ? {} : { vista: v })}
        opcoes={[
          { id: "propinas", label: "Propinas", icon: Wallet },
          { id: "devedores", label: "Devedores", icon: HandCoins },
          { id: "turmas", label: "Turmas", icon: GraduationCap },
        ]}
      />
      {anos.isLoading ? null : !ano || vista === "turmas" ? (
        <Turmas
          ano={ano}
          anos={anos.data ?? []}
          onAno={(id) => {
            // ano novo: a seguir criam-se as turmas
            setAnoId(id);
            setSp({ vista: "turmas" });
          }}
        />
      ) : vista === "devedores" ? (
        <ListaDevedores ano={ano} />
      ) : (
        <MapaPropinas ano={ano} irTurmas={() => setSp({ vista: "turmas" })} />
      )}
    </div>
  );
}

// ------------------------------------------------------------------ propinas

function MapaPropinas({ ano, irTurmas }: { ano: Ano; irTurmas: () => void }) {
  const qc = useQueryClient();
  const nav = useNavigate();
  const podeFacturar = useSession((s) => s.tem)("vendas.fatura");
  const [turma, setTurma] = useState("");
  const [q, setQ] = useState("");
  const [matricular, setMatricular] = useState(false);
  const [mudar, setMudar] = useState<LinhaPropina | null>(null);
  const [anular, setAnular] = useState<LinhaPropina | null>(null);
  const [facturar, setFacturar] = useState(false);
  const turmas = useQuery({ queryKey: ["escola-turmas", ano.id], queryFn: () => call<Turma[]>("escola.turmas", { ano_lectivo_id: ano.id }) });
  const p = useQuery({ queryKey: ["escola-propinas", ano.id, turma], queryFn: () => call<Propinas>("escola.propinas", { ano_lectivo_id: ano.id, turma_id: turma ? Number(turma) : null }) });
  const linhas = useMemo(() => {
    const t = q.trim().toLowerCase();
    return (p.data?.linhas ?? []).filter((l) => !t || l.nome.toLowerCase().includes(t) || l.nif.includes(t));
  }, [p.data, q]);
  const actualizar = () => qc.invalidateQueries({ predicate: (x) => String(x.queryKey[0]).startsWith("escola-") });
  const porFacturar = p.data?.totais?.meses_por_facturar ?? 0;

  async function emitir() {
    const r = await call<{ emitidos: unknown[]; erros: { avenca: string; erro: string }[] }>("avencas.emitir", { ids: p.data?.avencas_por_facturar ?? [] });
    if (r.emitidos.length) toast.success(`${r.emitidos.length} ${r.emitidos.length === 1 ? "factura de propina emitida" : "facturas de propina emitidas"}.`);
    for (const e of r.erros) toast.error(`${e.avenca}: ${e.erro}`);
    setFacturar(false);
    actualizar();
  }

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-3 gap-3">
        <Stat label="Em dívida" value={kz(p.data?.totais?.em_divida)} tone="danger" />
        <Stat label="Recebido" value={kz(p.data?.totais?.pago)} tone="ok" />
        <Stat label="Meses por facturar" value={porFacturar} tone="accent" />
      </div>
      <Card>
        <div className="flex flex-wrap items-center gap-2 border-b border-line p-3">
          <div className="relative min-w-[180px] flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Procurar aluno ou NIF" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Select className="w-44" aria-label="Turma" value={turma} onChange={(e) => setTurma(e.target.value)}>
            <option value="">Todas as turmas</option>
            {turmas.data?.map((t) => (
              <option key={t.id} value={t.id}>
                {t.classe} {t.turma}
              </option>
            ))}
          </Select>
          {podeFacturar && (
            <Button variant="accent" disabled={!porFacturar} onClick={() => setFacturar(true)}>
              <FileCheck2 className="size-4" /> Facturar propinas{porFacturar ? ` (${porFacturar})` : ""}
            </Button>
          )}
          <Button onClick={() => setMatricular(true)} disabled={!turmas.data?.some((t) => t.ativo)}>
            <Plus className="size-4" /> Matricular aluno
          </Button>
        </div>
        {!linhas.length ? (
          <Empty
            icon={<GraduationCap className="size-6" />}
            title={p.data?.linhas.length ? "Nenhum aluno encontrado" : "Ainda não há alunos matriculados"}
            text={turmas.data?.length ? "Matricule os alunos: cada um passa a ter a propina de cada mês." : "Crie primeiro as turmas, com o valor da propina de cada uma."}
            action={
              turmas.data && !turmas.data.length && (
                <Button onClick={irTurmas}>
                  <Plus className="size-4" /> Criar turmas
                </Button>
              )
            }
          />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Aluno</Th>
                <Th>Turma</Th>
                <Th className="text-right">Propina</Th>
                {p.data?.meses.map((m) => (
                  <Th key={m} className="px-1 text-center text-[11px]">
                    {nomeMes(m)}
                  </Th>
                ))}
                <Th className="text-right">Em dívida</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {linhas.map((l) => (
                <tr key={l.matricula_id} className={cn("border-t border-line", l.estado === "ANULADA" && "opacity-60")}>
                  <Td>
                    <div className="font-medium">{l.nome}</div>
                    <div className="text-[12px] text-ink-3">
                      {l.estado === "ANULADA" ? "Matrícula anulada" : l.desconto > 0 ? `Bolsa ${l.desconto}%` : l.nif !== "999999999" ? `NIF ${l.nif}` : ""}
                    </div>
                  </Td>
                  <Td className="whitespace-nowrap">{l.turma}</Td>
                  <Td className="num whitespace-nowrap text-right">{l.avenca_id ? kz(l.valor_mes) : "—"}</Td>
                  {l.celulas.map((c) => {
                    const v = CELULA[c.estado];
                    return (
                      <Td key={c.mes} className="px-1 text-center">
                        <span
                          title={`${nomeMes(c.mes)}: ${v.titulo}${c.numero_doc ? ` — ${c.numero_doc}` : ""}${c.pendente ? ` (falta ${kz(c.pendente)})` : c.total ? ` (${kz(c.total)})` : ""}`}
                          className={cn("inline-grid size-6 place-items-center rounded-md text-[12px] font-semibold", v.classe)}
                        >
                          {v.texto}
                        </span>
                      </Td>
                    );
                  })}
                  <Td className={cn("num whitespace-nowrap text-right", l.em_divida > 0 && "font-semibold text-danger")}>{kz(l.em_divida)}</Td>
                  <Td className="whitespace-nowrap text-right">
                    {l.em_divida > 0 && (
                      <Button size="sm" variant="secondary" onClick={() => nav(`/recibos/novo?entidade=${l.entidade_id}`)}>
                        Receber
                      </Button>
                    )}
                    {l.estado === "ACTIVA" && (
                      <>
                        <button title="Mudar de turma" onClick={() => setMudar(l)} className="ml-1 rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-ink">
                          <ArrowRightLeft className="size-4" />
                        </button>
                        <button title="Anular a matrícula" onClick={() => setAnular(l)} className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-danger">
                          <Ban className="size-4" />
                        </button>
                      </>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        <div className="flex flex-wrap gap-3 border-t border-line p-3 text-[12px] text-ink-3">
          {(["PAGA", "PARCIAL", "DIVIDA", "POR_FACTURAR", "BOLSA"] as Estado[]).map((e) => (
            <span key={e} className="flex items-center gap-1.5">
              <span className={cn("inline-grid size-5 place-items-center rounded text-[11px] font-semibold", CELULA[e].classe)}>{CELULA[e].texto}</span>
              {CELULA[e].titulo}
            </span>
          ))}
        </div>
      </Card>

      {matricular && <Matricular ano={ano} turmas={(turmas.data ?? []).filter((t) => t.ativo)} onClose={() => setMatricular(false)} onFeito={actualizar} />}
      {mudar && <MudarTurma linha={mudar} turmas={(turmas.data ?? []).filter((t) => t.ativo && t.id !== mudar.turma_id)} onClose={() => setMudar(null)} onFeito={actualizar} />}
      {anular && (
        <Confirmar
          titulo={`Anular a matrícula de ${anular.nome}?`}
          confirmar="Anular matrícula"
          onClose={() => setAnular(null)}
          onConfirmar={async () => {
            await call("escola.matriculaAnular", { id: anular.matricula_id });
            toast.success("Matrícula anulada: as propinas deixam de se facturar.");
            setAnular(null);
            actualizar();
          }}
        >
          As propinas dos próximos meses deixam de se facturar. As facturas já emitidas ficam na conta corrente (anule-as com uma nota de crédito, se for o caso).
        </Confirmar>
      )}
      {facturar && (
        <Confirmar titulo="Facturar as propinas?" confirmar="Facturar" perigo={false} onClose={() => setFacturar(false)} onConfirmar={emitir}>
          Emite {porFacturar} {porFacturar === 1 ? "factura" : "facturas"} de propina (uma por aluno e por mês, até ao mês actual), isentas de IVA. Nenhum mês se factura duas vezes.
        </Confirmar>
      )}
    </div>
  );
}

function Matricular({ ano, turmas, onClose, onFeito }: { ano: Ano; turmas: Turma[]; onClose: () => void; onFeito: () => void }) {
  const [novo, setNovo] = useState(true);
  const [aluno, setAluno] = useState({ nome: "", nif: "", telefone: "" });
  const [cliente, setCliente] = useState<Entidade | null>(null);
  const [turma, setTurma] = useState(String(turmas[0]?.id ?? ""));
  const [desconto, setDesconto] = useState("");
  // mês local (o UTC daria o mês anterior perto da meia-noite do dia 1)
  const hoje = hojeISO().slice(0, 7);
  const [primeiro, setPrimeiro] = useState(hoje < ano.inicio.slice(0, 7) ? ano.inicio.slice(0, 7) : hoje > ano.fim.slice(0, 7) ? ano.fim.slice(0, 7) : hoje);
  const [busy, setBusy] = useState(false);

  async function guardar(e: FormEvent) {
    e.preventDefault();
    if (!novo && !cliente) return toast.error("Escolha o aluno.");
    setBusy(true);
    try {
      await call("escola.matricular", {
        aluno: novo ? aluno : undefined,
        entidade_id: novo ? undefined : cliente?.id,
        turma_id: Number(turma),
        desconto: Number(desconto.replace(",", ".")) || 0,
        primeiro_mes: primeiro,
      });
      toast.success("Aluno matriculado.");
      onFeito();
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal open onClose={onClose} title={`Matricular aluno — ${ano.designacao}`} width="max-w-lg">
      <form onSubmit={guardar} className="space-y-4">
        <Segmentado
          valor={novo ? "novo" : "cliente"}
          onChange={(v) => setNovo(v === "novo")}
          opcoes={[
            { id: "novo", label: "Aluno novo" },
            { id: "cliente", label: "Já é cliente" },
          ]}
        />
        {novo ? (
          <div className="grid grid-cols-2 gap-3">
            <Field label="Nome do aluno" className="col-span-2">
              <Input required autoFocus value={aluno.nome} onChange={(e) => setAluno({ ...aluno, nome: e.target.value })} />
            </Field>
            <Field label="NIF (opcional)">
              <Input value={aluno.nif} onChange={(e) => setAluno({ ...aluno, nif: e.target.value })} />
            </Field>
            <Field label="Telefone (opcional)">
              <Input value={aluno.telefone} onChange={(e) => setAluno({ ...aluno, telefone: e.target.value })} />
            </Field>
          </div>
        ) : (
          <EntidadePicker tipo="C" valor={cliente} onChange={setCliente} label="Aluno" />
        )}
        <div className="grid grid-cols-2 gap-3">
          <Field label="Turma" className="col-span-2">
            <Select required value={turma} onChange={(e) => setTurma(e.target.value)}>
              {turmas.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.classe} {t.turma} — {t.propina ? `${kz(t.propina)}/mês` : "propina do artigo"}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Primeira propina">
            <Input type="month" required min={ano.inicio.slice(0, 7)} max={ano.fim.slice(0, 7)} value={primeiro} onChange={(e) => setPrimeiro(e.target.value)} />
          </Field>
          <Field label="Bolsa / desconto (%)" hint="100% = bolsa integral, sem propina">
            <Input inputMode="decimal" placeholder="0" value={desconto} onChange={(e) => setDesconto(e.target.value)} />
          </Field>
        </div>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy}>Matricular</Button>
        </div>
      </form>
    </Modal>
  );
}

function MudarTurma({ linha, turmas, onClose, onFeito }: { linha: LinhaPropina; turmas: Turma[]; onClose: () => void; onFeito: () => void }) {
  const [turma, setTurma] = useState(String(turmas[0]?.id ?? ""));
  const [busy, setBusy] = useState(false);
  async function guardar(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      await call("escola.mudarTurma", { id: linha.matricula_id, turma_id: Number(turma) });
      toast.success(`${linha.nome} mudou de turma: as próximas propinas são as da turma nova.`);
      onFeito();
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={onClose} title={`Mudar ${linha.nome} de turma`} width="max-w-md">
      <form onSubmit={guardar} className="space-y-4">
        <p className="text-[13.5px] text-ink-2">Agora em {linha.turma}. As facturas já emitidas ficam como estão.</p>
        <Field label="Turma nova">
          <Select required value={turma} onChange={(e) => setTurma(e.target.value)}>
            {turmas.map((t) => (
              <option key={t.id} value={t.id}>
                {t.classe} {t.turma} — {t.propina ? `${kz(t.propina)}/mês` : "propina do artigo"}
              </option>
            ))}
          </Select>
        </Field>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!turmas.length}>
            Mudar
          </Button>
        </div>
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------------ devedores

function ListaDevedores({ ano }: { ano: Ano }) {
  const nav = useNavigate();
  const empresa = useEmpresa();
  const d = useQuery({ queryKey: ["escola-devedores", ano.id], queryFn: () => call<Devedores>("escola.devedores", { ano_lectivo_id: ano.id }) });
  if (!d.data) return null;
  if (!d.data.turmas.length) return <Card><Empty icon={<HandCoins className="size-6" />} title="Não há propinas em dívida" text="Todas as propinas facturadas estão pagas." /></Card>;
  const alunos = d.data.turmas.reduce((s, t) => s + t.alunos.length, 0);
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-end gap-3">
        <Stat label="Total em dívida" value={kz(d.data.total)} tone="danger" hint={`${alunos} ${alunos === 1 ? "aluno" : "alunos"} em ${d.data.turmas.length} ${d.data.turmas.length === 1 ? "turma" : "turmas"}`} />
        <Button variant="secondary" onClick={imprimir}>
          <Printer className="size-4" /> Imprimir lista
        </Button>
      </div>
      {/* lista para imprimir: uma turma a seguir à outra, com o total de cada uma */}
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo={`Propinas em dívida — ${ano.designacao}`} numero={data(hojeISO())} />
          <div className="my-3 text-[13px] font-semibold">Total em dívida: {kz(d.data.total)}</div>
          {d.data.turmas.map((t) => (
            <table key={t.turma_id} className="mb-4 w-full border-collapse text-left">
              <thead>
                <tr>
                  <th colSpan={4} className="pb-1 text-[12px]">
                    {t.turma}
                  </th>
                  <th className="pb-1 text-right text-[12px]">{kz(t.total)}</th>
                </tr>
                <tr className="border-b border-black">
                  <th>Aluno</th>
                  <th>Telefone</th>
                  <th className="text-right">Meses</th>
                  <th>Desde</th>
                  <th className="text-right">Em dívida</th>
                </tr>
              </thead>
              <tbody>
                {t.alunos.map((a) => (
                  <tr key={a.entidade_id} className="border-b border-black/20">
                    <td>{a.nome}</td>
                    <td>{a.telefone ?? ""}</td>
                    <td className="text-right">{a.meses}</td>
                    <td>{nomeMes(a.desde)}</td>
                    <td className="text-right">{kz(a.valor)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ))}
        </div>
      </PrintPortal>
      {d.data.turmas.map((t) => (
        <Card key={t.turma_id}>
          <div className="flex items-center justify-between border-b border-line p-3">
            <span className="font-semibold">{t.turma}</span>
            <span className="num font-semibold text-danger">{kz(t.total)}</span>
          </div>
          <Table>
            <thead>
              <tr>
                <Th>Aluno</Th>
                <Th>Telefone</Th>
                <Th className="text-right">Meses</Th>
                <Th>Desde</Th>
                <Th className="text-right">Em dívida</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {t.alunos.map((a) => (
                <tr key={a.entidade_id} className="border-t border-line">
                  <Td className="font-medium">{a.nome}</Td>
                  <Td>{a.telefone ?? "—"}</Td>
                  <Td className="num text-right">{a.meses}</Td>
                  <Td>{nomeMes(a.desde)}</Td>
                  <Td className="num text-right font-semibold text-danger">{kz(a.valor)}</Td>
                  <Td className="text-right">
                    <Button size="sm" variant="secondary" onClick={() => nav(`/recibos/novo?entidade=${a.entidade_id}`)}>
                      Receber
                    </Button>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </Card>
      ))}
    </div>
  );
}

// ------------------------------------------------------------------ turmas e ano lectivo

function Turmas({ ano, anos, onAno }: { ano: Ano | null; anos: Ano[]; onAno: (id: number) => void }) {
  const qc = useQueryClient();
  const [editAno, setEditAno] = useState<Partial<Ano> | null>(null);
  const [editTurma, setEditTurma] = useState<Partial<Turma> | null>(null);
  const turmas = useQuery({ queryKey: ["escola-turmas", ano?.id], queryFn: () => call<Turma[]>("escola.turmas", { ano_lectivo_id: ano?.id }), enabled: !!ano });
  const actualizar = () => qc.invalidateQueries({ predicate: (x) => String(x.queryKey[0]).startsWith("escola-") });

  async function apagar(t: Turma) {
    try {
      await call("escola.turmaApagar", { id: t.id });
      toast.success("Turma apagada.");
      actualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  if (!ano)
    return (
      <Card>
        <Empty
          icon={<GraduationCap className="size-6" />}
          title="Comece pelo ano lectivo"
          text="Diga o primeiro e o último mês de propina; depois crie as turmas com o valor da propina."
          action={
            <Button onClick={() => setEditAno({})}>
              <Plus className="size-4" /> Novo ano lectivo
            </Button>
          }
        />
        {editAno && <AnoModal ano={editAno} onClose={() => setEditAno(null)} onFeito={(id) => (actualizar(), onAno(id))} />}
      </Card>
    );

  return (
    <div className="space-y-4">
      <Card>
        <div className="flex items-center justify-between border-b border-line p-3">
          <span className="font-semibold">Anos lectivos</span>
          <Button size="sm" variant="secondary" onClick={() => setEditAno({})}>
            <Plus className="size-4" /> Novo ano lectivo
          </Button>
        </div>
        <div className="flex flex-wrap gap-2 p-3">
          {anos.map((a) => (
            <button
              key={a.id}
              onClick={() => setEditAno(a)}
              className={cn("rounded-lg border px-3 py-2 text-left text-[13px] hover:border-brand/40", a.id === ano.id ? "border-brand bg-brand-50/50" : "border-line")}
            >
              <span className="font-medium">{a.designacao}</span> {a.corrente ? <Badge tone="brand">corrente</Badge> : null}
              <span className="block text-ink-3">
                {nomeMes(a.inicio)} a {nomeMes(a.fim)} · {a.alunos} alunos
              </span>
            </button>
          ))}
        </div>
      </Card>
      <Card>
        <div className="flex items-center justify-between border-b border-line p-3">
          <span className="font-semibold">Turmas de {ano.designacao}</span>
          <Button size="sm" onClick={() => setEditTurma({ ano_lectivo_id: ano.id, turma: "A" })}>
            <Plus className="size-4" /> Nova turma
          </Button>
        </div>
        {/* sem ano lectivo a consulta não corre: aí é vazio, não «a carregar» */}
        {ano && !turmas.data ? (
          <BlocoCarregando />
        ) : !turmas.data?.length ? (
          <Empty icon={<GraduationCap className="size-6" />} title="Sem turmas" text="Crie as turmas com o valor da propina mensal de cada uma." />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Classe</Th>
                <Th>Turma</Th>
                <Th className="text-right">Propina / mês</Th>
                <Th className="text-right">Alunos</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {turmas.data.map((t) => (
                <tr key={t.id} onClick={() => setEditTurma(t)} className={cn("cursor-pointer border-t border-line hover:bg-sunken/50", !t.ativo && "opacity-55")}>
                  <Td className="font-medium">
                    {t.classe} {!t.ativo && <Badge>inactiva</Badge>}
                  </Td>
                  <Td>{t.turma}</Td>
                  <Td className="num text-right">{t.propina ? kz(t.propina) : <span className="text-ink-3">do artigo</span>}</Td>
                  <Td className="num text-right">{t.alunos}</Td>
                  <Td className="text-right">
                    {!t.alunos && (
                      <button
                        title="Apagar a turma"
                        onClick={(e) => {
                          e.stopPropagation();
                          apagar(t);
                        }}
                        className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-danger"
                      >
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
      {editAno && <AnoModal ano={editAno} onClose={() => setEditAno(null)} onFeito={(id) => (actualizar(), onAno(id))} />}
      {editTurma && <TurmaModal turma={editTurma} onClose={() => setEditTurma(null)} onFeito={actualizar} />}
    </div>
  );
}

function AnoModal({ ano, onClose, onFeito }: { ano: Partial<Ano>; onClose: () => void; onFeito: (id: number) => void }) {
  const y = new Date().getFullYear();
  const [f, setF] = useState({
    designacao: ano.designacao ?? `${y}/${y + 1}`,
    inicio: ano.inicio?.slice(0, 7) ?? `${y}-09`,
    fim: ano.fim?.slice(0, 7) ?? `${y + 1}-06`,
    corrente: ano.id ? !!ano.corrente : true,
  });
  const [busy, setBusy] = useState(false);
  async function guardar(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const r = await call<{ id: number }>("escola.anoGuardar", { id: ano.id, ...f });
      toast.success("Ano lectivo guardado.");
      onFeito(r.id);
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={onClose} title={ano.id ? `Ano lectivo ${ano.designacao}` : "Novo ano lectivo"} width="max-w-md">
      <form onSubmit={guardar} className="space-y-4">
        <Field label="Ano lectivo">
          <Input required autoFocus value={f.designacao} onChange={(e) => setF({ ...f, designacao: e.target.value })} />
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Primeira propina">
            <Input type="month" required value={f.inicio} onChange={(e) => setF({ ...f, inicio: e.target.value })} />
          </Field>
          <Field label="Última propina">
            <Input type="month" required value={f.fim} onChange={(e) => setF({ ...f, fim: e.target.value })} />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-[13.5px]">
          <input type="checkbox" checked={f.corrente} onChange={(e) => setF({ ...f, corrente: e.target.checked })} /> Ano corrente (abre por omissão)
        </label>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy}>Guardar</Button>
        </div>
      </form>
    </Modal>
  );
}

function TurmaModal({ turma, onClose, onFeito }: { turma: Partial<Turma>; onClose: () => void; onFeito: () => void }) {
  const [f, setF] = useState({ classe: turma.classe ?? "", turma: turma.turma ?? "A", propina: turma.propina ? toInputKz(turma.propina) : "", ativo: turma.ativo ?? 1 });
  const [busy, setBusy] = useState(false);
  async function guardar(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      const r = await call<{ matriculas_actualizadas: number }>("escola.turmaGuardar", {
        id: turma.id,
        ano_lectivo_id: turma.ano_lectivo_id,
        classe: f.classe,
        turma: f.turma,
        propina: parseKz(f.propina),
        ativo: !!f.ativo,
      });
      toast.success(r.matriculas_actualizadas ? `Turma guardada: a propina nova vale para os próximos meses de ${r.matriculas_actualizadas} alunos.` : "Turma guardada.");
      onFeito();
      onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={onClose} title={turma.id ? `Turma ${turma.classe} ${turma.turma}` : "Nova turma"} width="max-w-md">
      <form onSubmit={guardar} className="space-y-4">
        <div className="grid grid-cols-3 gap-3">
          <Field label="Classe" className="col-span-2">
            <Input required autoFocus placeholder="Ex.: 7.ª classe" value={f.classe} onChange={(e) => setF({ ...f, classe: e.target.value })} />
          </Field>
          <Field label="Turma">
            <Input required value={f.turma} onChange={(e) => setF({ ...f, turma: e.target.value })} />
          </Field>
        </div>
        <Field label="Propina mensal (Kz)" hint="Em branco: o preço do artigo «Propina mensal».">
          <Input className="num text-right" inputMode="decimal" value={f.propina} onChange={(e) => setF({ ...f, propina: e.target.value })} />
        </Field>
        {turma.id && (
          <label className="flex items-center gap-2 text-[13.5px]">
            <input type="checkbox" checked={!!f.ativo} onChange={(e) => setF({ ...f, ativo: e.target.checked ? 1 : 0 })} /> Turma activa (aceita matrículas)
          </label>
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy}>Guardar</Button>
        </div>
      </form>
    </Modal>
  );
}

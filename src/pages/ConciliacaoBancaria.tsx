// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Conciliação bancária (services/conciliacao.rs): importa-se o extracto do banco (Excel, ODS ou CSV) e cada linha casa
// com os movimentos da conta no KanGest, sozinha (pelo valor e pela data, ou as vendas com cartão de um dia numa só
// linha) ou à mão. O que sobra dos dois lados fica à vista. O que falta no KanGest (comissões, juros) lança-se a partir
// da linha, pela tesouraria de sempre.
import { useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ArrowLeft, Check, FileUp, Landmark, Link2, Lock, Plus, Trash2, Undo2 } from "lucide-react";
import { call } from "@/lib/api";
import { data, kz, numero } from "@/lib/format";
import { Badge, BlocoCarregando, Button, Card, Confirmar, Empty, Field, Input, LinhasCarregando, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface Conta {
  id: number;
  descricao: string;
  tipo: string;
  ativo: number;
}
interface Leitura {
  ficheiro: string;
  folha: string;
  folhas: string[];
  de: string;
  ate: string;
  movimentos: number;
  ignoradas: number;
  entradas: number;
  saidas: number;
  saldo_banco: number | null;
  cabecalho: string[];
  colunas: Record<string, number | null>;
  amostra: { linha: number; data: string; descricao: string; valor: number }[];
}
interface Resumo {
  id: number;
  conta: string;
  ficheiro: string;
  de: string;
  ate: string;
  estado: "ABERTA" | "FECHADA";
  linhas: number;
  casadas: number;
}
interface Movimento {
  chave: string;
  data: string;
  documento: string | null;
  descricao: string | null;
  origem: string;
  valor: number;
}
interface LinhaExtracto {
  id: number;
  linha: number;
  data: string;
  descricao: string;
  valor: number;
  casado: "auto" | "manual" | "lancado" | null;
  movimentos: Movimento[];
}
interface Detalhe {
  conciliacao: { id: number; conta: string; ficheiro: string; de: string; ate: string; estado: "ABERTA" | "FECHADA"; criado_por: string | null };
  linhas: LinhaExtracto[];
  pendentes: Movimento[];
  resumo: { total_extracto: number; extracto_por_casar: number; linhas_por_casar: number; kangest_por_casar: number; saldo_banco: number | null; saldo_kangest: number };
}

const COLUNAS: [string, string][] = [
  ["data", "Data"],
  ["descricao", "Descrição"],
  ["valor", "Valor (com sinal)"],
  ["debito", "Débito"],
  ["credito", "Crédito"],
  ["saldo", "Saldo"],
];

const lerBase64 = (f: File) =>
  new Promise<string>((ok, falha) => {
    const r = new FileReader();
    r.onload = () => ok(String(r.result).split(",")[1] ?? "");
    r.onerror = () => falha(new Error("Não consegui ler o ficheiro."));
    r.readAsDataURL(f);
  });

const Valor = ({ v, className }: { v: number; className?: string }) => <span className={cn("num whitespace-nowrap", v < 0 && "text-danger", className)}>{kz(v)}</span>;

export default function ConciliacaoBancaria() {
  const contas = useQuery({ queryKey: ["tesouraria-contas"], queryFn: () => call<Conta[]>("tesouraria.contas") });
  const bancos = useMemo(() => (contas.data ?? []).filter((c) => c.ativo).sort((a, b) => Number(b.tipo === "BANCO") - Number(a.tipo === "BANCO")), [contas.data]);
  const [contaId, setContaId] = useState<number | null>(null);
  const conta = contaId ?? bancos[0]?.id ?? null;
  const [aberta, setAberta] = useState<number | null>(null);
  const lista = useQuery({ queryKey: ["conciliacoes", conta], queryFn: () => call<Resumo[]>("conciliacao.listar", { conta_id: conta }), enabled: !!conta });
  const [importar, setImportar] = useState<{ nome: string; conteudo: string; leitura: Leitura } | null>(null);
  const entrada = useRef<HTMLInputElement>(null);
  const qc = useQueryClient();

  async function escolher(f: File | undefined) {
    if (!f) return;
    try {
      const conteudo = await lerBase64(f);
      const leitura = await call<Leitura>("conciliacao.ler", { nome: f.name, conteudo });
      setImportar({ nome: f.name, conteudo, leitura });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      if (entrada.current) entrada.current.value = "";
    }
  }

  if (aberta) return <Conciliacao id={aberta} onVoltar={() => setAberta(null)} />;

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader
        title="Conciliação bancária"
        subtitle="Importe o extracto do banco: o KanGest casa cada linha com os movimentos da conta e mostra o que sobra dos dois lados."
        actions={
          <Button disabled={!conta} onClick={() => entrada.current?.click()}>
            <FileUp className="size-4" /> Importar extracto
          </Button>
        }
      />
      <input ref={entrada} type="file" accept=".xlsx,.xls,.xlsm,.ods,.csv,.txt" className="hidden" onChange={(e) => escolher(e.target.files?.[0])} />

      <Card className="mb-4 flex flex-wrap items-end gap-4 p-4">
        <Field label="Conta" className="w-72">
          <Select value={conta ?? ""} onChange={(e) => setContaId(Number(e.target.value))}>
            {bancos.map((c) => (
              <option key={c.id} value={c.id}>
                {c.descricao}
                {c.tipo !== "BANCO" ? " (caixa)" : ""}
              </option>
            ))}
          </Select>
        </Field>
        <p className="max-w-md pb-2 text-[12.5px] text-ink-3">
          No site ou na aplicação do banco, exporte o extracto do período em Excel ou CSV. As colunas (data, descrição, débito e crédito ou valor, saldo) reconhecem-se
          pelos títulos.
        </p>
      </Card>

      <Card className="overflow-hidden">
        <Table className="tabela-compacta">
          <thead>
            <tr>
              <Th>Período</Th>
              <Th>Extracto</Th>
              <Th className="text-right">Linhas</Th>
              <Th>Conciliadas</Th>
              <Th>Estado</Th>
            </tr>
          </thead>
          <tbody>
            {lista.isLoading && <LinhasCarregando colunas={5} />}
            {lista.data?.map((c) => (
              <tr key={c.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setAberta(c.id)}>
                <Td className="font-medium text-ink">
                  {data(c.de)} a {data(c.ate)}
                </Td>
                <Td className="text-ink-2">{c.ficheiro}</Td>
                <Td className="num text-right">{numero(c.linhas, 0)}</Td>
                <Td>
                  <div className="flex items-center gap-2">
                    <div className="h-1.5 w-24 overflow-hidden rounded-full bg-sunken">
                      <div className="h-full rounded-full bg-ok" style={{ width: `${c.linhas ? (100 * c.casadas) / c.linhas : 0}%` }} />
                    </div>
                    <span className="num text-[12.5px] text-ink-3">
                      {numero(c.casadas, 0)} de {numero(c.linhas, 0)}
                    </span>
                  </div>
                </Td>
                <Td>{c.estado === "FECHADA" ? <Badge tone="neutral">Fechada</Badge> : <Badge tone="brand">Aberta</Badge>}</Td>
              </tr>
            ))}
          </tbody>
        </Table>
        {lista.data && lista.data.length === 0 && (
          <Empty icon={<Landmark className="size-6" />} title="Ainda sem conciliações nesta conta" text="Importe o extracto do banco para conferir os movimentos." />
        )}
      </Card>

      {importar && conta && (
        <Importar
          {...importar}
          contaNome={bancos.find((b) => b.id === conta)?.descricao ?? ""}
          onClose={() => setImportar(null)}
          onLer={async (colunas) => {
            try {
              const leitura = await call<Leitura>("conciliacao.ler", { nome: importar.nome, conteudo: importar.conteudo, colunas });
              setImportar({ ...importar, leitura });
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
          onImportar={async (colunas) => {
            try {
              const r = await call<{ id: number; linhas: number; casadas: number }>("conciliacao.criar", { conta_id: conta, nome: importar.nome, conteudo: importar.conteudo, colunas });
              toast.success(`${numero(r.casadas, 0)} de ${numero(r.linhas, 0)} linhas casadas sozinhas.`);
              setImportar(null);
              await qc.invalidateQueries({ queryKey: ["conciliacoes"] });
              setAberta(r.id);
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
        />
      )}
    </div>
  );
}

function Importar({
  nome,
  leitura: l,
  contaNome,
  onClose,
  onLer,
  onImportar,
}: {
  nome: string;
  leitura: Leitura;
  contaNome: string;
  onClose: () => void;
  onLer: (c: Record<string, number | null>) => Promise<void>;
  onImportar: (c: Record<string, number | null>) => Promise<void>;
}) {
  const [colunas, setColunas] = useState(l.colunas);
  const [busy, setBusy] = useState(false);
  const titulo = (i: number) => l.cabecalho[i] || `Coluna ${i + 1}`;
  const correr = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open
      onClose={onClose}
      title={`Importar ${nome}`}
      width="max-w-3xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Voltar
          </Button>
          <Button loading={busy} onClick={() => correr(() => onImportar(colunas))}>
            <Check className="size-4" /> Importar para {contaNome}
          </Button>
        </>
      }
    >
      <div className="space-y-4 text-[14px]">
        <div className="grid gap-3 sm:grid-cols-4">
          <Numero rotulo="Período" valor={`${data(l.de)} a ${data(l.ate)}`} />
          <Numero rotulo="Movimentos" valor={`${numero(l.movimentos, 0)}${l.ignoradas ? ` (+${numero(l.ignoradas, 0)} sem data ou valor)` : ""}`} />
          <Numero rotulo="Entradas · saídas" valor={`${kz(l.entradas)} · ${kz(l.saidas)}`} />
          <Numero rotulo="Saldo final do banco" valor={l.saldo_banco === null ? "—" : kz(l.saldo_banco)} />
        </div>
        <details className="rounded-lg border border-line px-3 py-2">
          <summary className="cursor-pointer text-[13px] text-ink-2">Colunas usadas (corrija se alguma estiver errada)</summary>
          <div className="mt-3 grid gap-3 sm:grid-cols-3">
            {COLUNAS.map(([k, rotulo]) => (
              <Field key={k} label={rotulo}>
                <Select value={colunas[k] ?? ""} onChange={(e) => setColunas({ ...colunas, [k]: e.target.value === "" ? null : Number(e.target.value) })}>
                  <option value="">—</option>
                  {l.cabecalho.map((_, i) => (
                    <option key={i} value={i}>
                      {titulo(i)}
                    </option>
                  ))}
                </Select>
              </Field>
            ))}
          </div>
          <Button size="sm" variant="secondary" className="mt-3" loading={busy} onClick={() => correr(() => onLer(colunas))}>
            Ler de novo com estas colunas
          </Button>
        </details>
        <Table className="tabela-compacta rounded-lg border border-line">
          <thead>
            <tr>
              <Th>Linha</Th>
              <Th>Data</Th>
              <Th>Descrição</Th>
              <Th className="text-right">Valor</Th>
            </tr>
          </thead>
          <tbody>
            {l.amostra.map((a) => (
              <tr key={a.linha}>
                <Td className="num text-ink-3">{a.linha}</Td>
                <Td>{data(a.data)}</Td>
                <Td>{a.descricao}</Td>
                <Td className="text-right">
                  <Valor v={a.valor} />
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
        {l.movimentos > l.amostra.length && <p className="text-[12.5px] text-ink-3">As primeiras {l.amostra.length} linhas do extracto.</p>}
      </div>
    </Modal>
  );
}

const Numero = ({ rotulo, valor }: { rotulo: string; valor: string }) => (
  <div className="rounded-lg bg-sunken/70 px-3 py-2">
    <div className="text-[12px] text-ink-3">{rotulo}</div>
    <div className="num font-medium text-ink">{valor}</div>
  </div>
);

const ORIGEM: Record<string, string> = { DOCUMENTO: "Documento", CAIXA: "Caixa", TESOURARIA: "Tesouraria" };
const textoMov = (m: Movimento) => [m.documento, m.descricao].filter(Boolean).join(" · ") || ORIGEM[m.origem] || m.chave;

function Conciliacao({ id, onVoltar }: { id: number; onVoltar: () => void }) {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["conciliacao", id], queryFn: () => call<Detalhe>("conciliacao.obter", { id }) });
  const [ativa, setAtiva] = useState<LinhaExtracto | null>(null);
  const [escolhidos, setEscolhidos] = useState<Set<string>>(new Set());
  const [diferenca, setDiferenca] = useState("Comissão do banco");
  const [aLancar, setALancar] = useState<LinhaExtracto | null>(null);
  const [aFechar, setAFechar] = useState(false);
  const [aApagar, setAApagar] = useState(false);
  const [busy, setBusy] = useState(false);
  const d = q.data;
  const fechada = d?.conciliacao.estado === "FECHADA";

  const actualizar = async () => {
    await qc.invalidateQueries({ queryKey: ["conciliacao", id] });
    await qc.invalidateQueries({ queryKey: ["conciliacoes"] });
  };
  async function correr(f: () => Promise<unknown>, ok?: string) {
    setBusy(true);
    try {
      await f();
      if (ok) toast.success(ok);
      await actualizar();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  function comecar(l: LinhaExtracto) {
    setAtiva(l);
    setEscolhidos(new Set());
  }
  const soma = (d?.pendentes ?? []).filter((m) => escolhidos.has(m.chave)).reduce((s, m) => s + m.valor, 0);
  const falta = ativa ? ativa.valor - soma : 0;
  async function casar(comDiferenca: boolean) {
    if (!ativa) return;
    await correr(
      () => call("conciliacao.casar", { linha_id: ativa.id, chaves: [...escolhidos], diferenca: comDiferenca ? { descricao: diferenca } : undefined }),
      comDiferenca ? "Linha casada, com a diferença lançada na tesouraria." : "Linha casada.",
    );
    setAtiva(null);
  }

  if (!d)
    return (
      <Card>
        <BlocoCarregando linhas={8} />
      </Card>
    );
  const r = d.resumo;
  const diferencaSaldo = r.saldo_banco === null ? null : r.saldo_banco - r.saldo_kangest;

  return (
    <div>
      <PageHeader
        title={`Conciliação · ${d.conciliacao.conta}`}
        subtitle={`${data(d.conciliacao.de)} a ${data(d.conciliacao.ate)} · ${d.conciliacao.ficheiro}`}
        actions={
          <>
            <Button variant="secondary" onClick={onVoltar}>
              <ArrowLeft className="size-4" /> Voltar
            </Button>
            {!fechada && (
              <>
                <Button variant="ghost" className="text-danger" onClick={() => setAApagar(true)}>
                  <Trash2 className="size-4" /> Apagar
                </Button>
                <Button onClick={() => setAFechar(true)}>
                  <Lock className="size-4" /> Fechar conciliação
                </Button>
              </>
            )}
            {fechada && <Badge tone="neutral">Fechada</Badge>}
          </>
        }
      />

      <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Card className="p-4">
          <div className="text-[12.5px] text-ink-3">Saldo segundo o banco</div>
          <div className="num mt-1 text-[20px] font-semibold">{r.saldo_banco === null ? "—" : kz(r.saldo_banco)}</div>
          <div className="text-[12px] text-ink-3">a {data(d.conciliacao.ate)}</div>
        </Card>
        <Card className="p-4">
          <div className="text-[12.5px] text-ink-3">Saldo no KanGest</div>
          <div className="num mt-1 text-[20px] font-semibold">{kz(r.saldo_kangest)}</div>
          <div className={cn("text-[12px]", diferencaSaldo ? "text-danger" : "text-ok")}>
            {diferencaSaldo === null ? "o extracto não traz o saldo" : diferencaSaldo === 0 ? "igual ao do banco" : `diferença ${kz(diferencaSaldo)}`}
          </div>
        </Card>
        <Card className="p-4">
          <div className="text-[12.5px] text-ink-3">Por conciliar no extracto</div>
          <div className="num mt-1 text-[20px] font-semibold">{numero(r.linhas_por_casar, 0)}</div>
          <div className="text-[12px] text-ink-3">{kz(r.extracto_por_casar)}</div>
        </Card>
        <Card className="p-4">
          <div className="text-[12.5px] text-ink-3">Por conciliar no KanGest</div>
          <div className="num mt-1 text-[20px] font-semibold">{numero(d.pendentes.length, 0)}</div>
          <div className="text-[12px] text-ink-3">{kz(r.kangest_por_casar)}</div>
        </Card>
      </div>

      {ativa && (
        <Card className="sticky top-2 z-[2] mb-4 flex flex-wrap items-center gap-x-6 gap-y-2 border-brand/40 p-4">
          <div className="min-w-0">
            <div className="text-[12.5px] text-ink-3">A casar a linha de {data(ativa.data)}</div>
            <div className="truncate font-medium">{ativa.descricao}</div>
          </div>
          <div className="text-[13px]">
            Extracto <Valor v={ativa.valor} className="font-semibold" /> · escolhidos <Valor v={soma} className="font-semibold" />
            {falta !== 0 && (
              <>
                {" "}
                · diferença <Valor v={falta} className="font-semibold" />
              </>
            )}
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-2">
            {falta !== 0 && escolhidos.size > 0 && (
              <>
                <Input className="h-8 w-52 text-[13px]" value={diferenca} onChange={(e) => setDiferenca(e.target.value)} aria-label="Descrição da diferença" />
                <Button size="sm" variant="secondary" loading={busy} disabled={!diferenca.trim()} onClick={() => casar(true)}>
                  Casar e lançar a diferença
                </Button>
              </>
            )}
            <Button size="sm" loading={busy} disabled={escolhidos.size === 0 || falta !== 0} onClick={() => casar(false)}>
              <Link2 className="size-4" /> Casar
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setAtiva(null)}>
              Cancelar
            </Button>
          </div>
        </Card>
      )}

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Card className="min-w-0 overflow-hidden">
          <div className="border-b border-line px-5 py-3 font-semibold">Extracto do banco</div>
          <Table className="tabela-compacta max-h-[calc(100vh-330px)]">
            <thead>
              <tr>
                <Th>Data</Th>
                <Th>Descrição</Th>
                <Th className="text-right">Valor</Th>
                <Th className="w-0" />
              </tr>
            </thead>
            <tbody>
              {d.linhas.map((l) => (
                <tr key={l.id} className={cn(ativa?.id === l.id && "bg-brand-50", !l.casado && ativa?.id !== l.id && "bg-accent-50/40")}>
                  <Td className="whitespace-nowrap">{data(l.data)}</Td>
                  <Td>
                    <div>{l.descricao}</div>
                    {l.movimentos.length > 0 && (
                      <div className="text-[12px] text-ink-3">
                        {l.casado === "lancado" ? "Lançado no KanGest: " : l.casado === "auto" ? "Casou sozinha com: " : "Casada com: "}
                        {l.movimentos.map((m) => `${data(m.data)} ${textoMov(m)}`).join("; ")}
                      </div>
                    )}
                  </Td>
                  <Td className="text-right">
                    <Valor v={l.valor} />
                  </Td>
                  <Td className="whitespace-nowrap text-right">
                    {fechada ? (
                      l.casado ? (
                        <Check className="inline size-4 text-ok" />
                      ) : null
                    ) : l.casado ? (
                      <Button size="sm" variant="ghost" title="Desfazer o casamento" disabled={busy} onClick={() => correr(() => call("conciliacao.descasar", { linha_id: l.id }))}>
                        <Check className="size-4 text-ok" />
                        <Undo2 className="size-3.5 opacity-60" />
                      </Button>
                    ) : (
                      <div className="flex justify-end gap-1">
                        <Button size="sm" variant="secondary" onClick={() => comecar(l)}>
                          <Link2 className="size-4" /> Casar
                        </Button>
                        <Button size="sm" variant="ghost" title="Lançar no KanGest (comissão, juros, depósito esquecido)" onClick={() => setALancar(l)}>
                          <Plus className="size-4" /> Lançar
                        </Button>
                      </div>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </Card>

        <Card className="min-w-0 overflow-hidden">
          <div className="border-b border-line px-5 py-3">
            <div className="font-semibold">No KanGest, por conciliar</div>
            <div className="text-[12.5px] text-ink-3">
              {ativa ? "Escolha os movimentos que fazem a linha do extracto." : "Movimentos da conta neste período que o banco não mostra (ainda)."}
            </div>
          </div>
          <Table className="tabela-compacta max-h-[calc(100vh-330px)]">
            <thead>
              <tr>
                {ativa && <Th className="w-0" />}
                <Th>Data</Th>
                <Th>Movimento</Th>
                <Th className="text-right">Valor</Th>
              </tr>
            </thead>
            <tbody>
              {d.pendentes.map((m) => {
                const marcado = escolhidos.has(m.chave);
                const sugerido = !!ativa && m.valor === ativa.valor;
                return (
                  <tr
                    key={m.chave}
                    className={cn(ativa && "cursor-pointer hover:bg-sunken/60", marcado && "bg-brand-50", sugerido && !marcado && "bg-ok-50/60")}
                    onClick={() => {
                      if (!ativa) return;
                      const n = new Set(escolhidos);
                      if (n.has(m.chave)) n.delete(m.chave);
                      else n.add(m.chave);
                      setEscolhidos(n);
                    }}
                  >
                    {ativa && (
                      <Td>
                        <input type="checkbox" readOnly checked={marcado} className="size-4 accent-[var(--color-brand)]" aria-label="Escolher" />
                      </Td>
                    )}
                    <Td className="whitespace-nowrap">{data(m.data)}</Td>
                    <Td>
                      {textoMov(m)}
                      <span className="ml-1.5 text-[12px] text-ink-3">{ORIGEM[m.origem] ?? m.origem}</span>
                    </Td>
                    <Td className="text-right">
                      <Valor v={m.valor} />
                    </Td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
          {d.pendentes.length === 0 && <Empty icon={<Check className="size-6" />} title="Nada por conciliar" text="Todos os movimentos da conta neste período estão no extracto." />}
        </Card>
      </div>

      {aLancar && (
        <Lancar
          linha={aLancar}
          onClose={() => setALancar(null)}
          onLancar={async (descricao) => {
            await correr(() => call("conciliacao.lancar", { linha_id: aLancar.id, descricao }), "Lançado na tesouraria e casado.");
            setALancar(null);
          }}
        />
      )}
      {aFechar && (
        <Confirmar
          titulo="Fechar a conciliação?"
          confirmar="Fechar"
          perigo={false}
          onClose={() => setAFechar(false)}
          onConfirmar={async () => {
            await correr(() => call("conciliacao.fechar", { id }), "Conciliação fechada.");
            setAFechar(false);
          }}
        >
          <p>Fica como está, para consulta: já não se casa nem se desfaz nada.</p>
          {r.linhas_por_casar > 0 && <p className="text-[12.5px] text-danger">Ainda há {numero(r.linhas_por_casar, 0)} linhas do extracto por casar.</p>}
        </Confirmar>
      )}
      {aApagar && (
        <Confirmar
          titulo="Apagar esta conciliação?"
          confirmar="Apagar"
          onClose={() => setAApagar(false)}
          onConfirmar={async () => {
            try {
              await call("conciliacao.eliminar", { id });
              await qc.invalidateQueries({ queryKey: ["conciliacoes"] });
              onVoltar();
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
        >
          <p>O extracto importado e os casamentos apagam-se. Os movimentos lançados a partir dele continuam na tesouraria (anulam-se lá).</p>
        </Confirmar>
      )}
    </div>
  );
}

function Lancar({ linha, onClose, onLancar }: { linha: LinhaExtracto; onClose: () => void; onLancar: (descricao: string) => Promise<void> }) {
  const [descricao, setDescricao] = useState(linha.descricao);
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      onClose={onClose}
      title="Lançar no KanGest"
      width="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Voltar
          </Button>
          <Button
            loading={busy}
            disabled={!descricao.trim()}
            onClick={async () => {
              setBusy(true);
              try {
                await onLancar(descricao.trim());
              } finally {
                setBusy(false);
              }
            }}
          >
            <Plus className="size-4" /> Lançar
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-[14px] text-ink-2">
        <p>
          Uma {linha.valor < 0 ? "saída" : "entrada"} de <Valor v={Math.abs(linha.valor)} className="font-semibold text-ink" /> na tesouraria, com a data de {data(linha.data)}
          , casada com esta linha do extracto.
        </p>
        <Field label="Descrição">
          <Input autoFocus value={descricao} maxLength={200} onChange={(e) => setDescricao(e.target.value)} />
        </Field>
      </div>
    </Modal>
  );
}

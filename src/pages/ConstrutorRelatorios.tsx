// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Construtor de relatórios: o utilizador escolhe a fonte (vendas, vendas por artigo, tesouraria…), o período, as
// colunas ou os agrupamentos e o que somar, os filtros e a ordenação; o resultado actualiza-se sozinho e exporta-se
// para Excel, CSV ou impressão. Tudo pelo motor central de dados (dados.fontes / dados.consultar), que só lê e só
// mostra as fontes que a permissão do utilizador deixa ver.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { BarChart3, FileSpreadsheet, FileText, Filter, Plus, Printer, Rows3, Save, Sigma, Table2, Trash2, X } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, hojeISO, kz, numero, parseKz } from "@/lib/format";
import { imprimir } from "@/lib/print";
import {
  exportarResultadoCsv,
  mostrar,
  exportarResultadoExcel,
  type AgregadoDados,
  type CampoFonte,
  type ConsultaDados,
  type FiltroDados,
  type FonteDados,
  type FuncaoAgregado,
  type Operador,
  type OrdemDados,
  type ResultadoDados,
} from "@/lib/dados";
import { Grelha } from "@/components/dados/Grelha";
import { BarChart } from "@/components/BarChart";
import { InputData } from "@/components/InputData";
import { PrintPortal } from "@/components/DocumentoPrint";
import { Button, Card, Empty, Field, Input, Modal, PageHeader, Segmentado, Select, cn } from "@/components/ui";

type Modo = "resumo" | "detalhe";

/** Ponto de partida de cada fonte (o utilizador muda à vontade). */
const PADRAO: Record<string, { campos: string[]; agrupar: string[]; agregados: AgregadoDados[] }> = {
  vendas: {
    campos: ["data", "documento", "cliente", "operador", "total"],
    agrupar: ["data"],
    agregados: [{ funcao: "contar" }, { funcao: "soma", campo: "liquido" }, { funcao: "soma", campo: "iva" }, { funcao: "soma", campo: "total" }],
  },
  linhas_venda: {
    campos: ["data", "documento", "artigo", "quantidade", "total"],
    agrupar: ["familia"],
    agregados: [{ funcao: "soma", campo: "quantidade" }, { funcao: "soma", campo: "total" }, { funcao: "soma", campo: "lucro" }],
  },
  tesouraria: {
    campos: ["data", "conta", "descricao", "entrada", "saida"],
    agrupar: ["conta"],
    agregados: [{ funcao: "soma", campo: "entrada" }, { funcao: "soma", campo: "saida" }, { funcao: "soma", campo: "saldo" }],
  },
  stock: {
    campos: ["referencia", "artigo", "loja", "existencia", "valor"],
    agrupar: ["familia"],
    agregados: [{ funcao: "soma", campo: "existencia" }, { funcao: "soma", campo: "valor" }],
  },
  compras: {
    campos: ["data", "documento", "fornecedor", "total", "pendente"],
    agrupar: ["fornecedor"],
    agregados: [{ funcao: "contar" }, { funcao: "soma", campo: "liquido" }, { funcao: "soma", campo: "iva" }, { funcao: "soma", campo: "total" }],
  },
  conta_corrente: {
    campos: ["vencimento", "documento", "cliente", "dias_atraso", "pendente"],
    agrupar: ["escalao"],
    agregados: [{ funcao: "contar" }, { funcao: "soma", campo: "pendente" }],
  },
};

const OPERADORES: Record<CampoFonte["tipo"], [Operador, string][]> = {
  texto: [
    ["contem", "contém"],
    ["=", "é igual a"],
    ["<>", "é diferente de"],
    ["comeca", "começa por"],
    ["vazio", "está vazio"],
    ["naovazio", "não está vazio"],
  ],
  numero: [
    [">=", "≥"],
    ["<=", "≤"],
    ["=", "="],
    [">", ">"],
    ["<", "<"],
  ],
  dinheiro: [
    [">=", "≥"],
    ["<=", "≤"],
    ["=", "="],
    [">", ">"],
    ["<", "<"],
  ],
  data: [
    [">=", "a partir de"],
    ["<=", "até"],
    ["=", "no dia"],
  ],
  logico: [["=", "é"]],
};

const FUNCOES: [FuncaoAgregado, string][] = [
  ["soma", "Soma"],
  ["media", "Média"],
  ["min", "Mínimo"],
  ["max", "Máximo"],
];

/** Um filtro como está no ecrã (o valor em texto, convertido só ao consultar). */
interface FiltroEcra {
  campo: string;
  op: Operador;
  texto: string;
}

const inicioMes = () => `${hojeISO().slice(0, 8)}01`;

/** Período relativo: um modelo guardado com «este mês» abre sempre no mês em curso. */
type Periodo = "mes" | "mes_anterior" | "ano" | "30dias" | "personalizado";
const PERIODOS: [Periodo, string][] = [
  ["mes", "Este mês"],
  ["mes_anterior", "Mês passado"],
  ["ano", "Este ano"],
  ["30dias", "Últimos 30 dias"],
  ["personalizado", "Personalizado"],
];
const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
function datasDe(p: Periodo): [string, string] | null {
  const hoje = hojeISO();
  const a = Number(hoje.slice(0, 4));
  const m = Number(hoje.slice(5, 7));
  switch (p) {
    case "mes":
      return [`${hoje.slice(0, 8)}01`, hoje];
    case "mes_anterior":
      return [iso(new Date(a, m - 2, 1)), iso(new Date(a, m - 1, 0))];
    case "ano":
      return [`${a}-01-01`, hoje];
    case "30dias": {
      const d = new Date();
      d.setDate(d.getDate() - 29);
      return [iso(d), hoje];
    }
    default:
      return null;
  }
}

/** O que um modelo guarda (dados.modeloGuardar): tudo o que define o relatório. */
interface Definicao {
  modo: Modo;
  periodo: Periodo;
  de: string;
  ate: string;
  campos: string[];
  agrupar: string[];
  agregados: AgregadoDados[];
  filtros: FiltroEcra[];
  ordenar: OrdemDados[];
  vista: "tabela" | "grafico";
}
interface Modelo {
  id: number;
  nome: string;
  fonte: string;
  definicao: Definicao;
  partilhado: number;
  meu: boolean;
  dono: string | null;
}

/** O valor de um filtro no tipo do campo, ou undefined se ainda não está preenchido. */
function valorFiltro(f: FiltroEcra, c: CampoFonte): FiltroDados | undefined {
  if (f.op === "vazio" || f.op === "naovazio") return { campo: f.campo, op: f.op };
  const t = f.texto.trim();
  if (!t) return undefined;
  if (c.tipo === "dinheiro") return { campo: f.campo, op: f.op, valor: parseKz(t) };
  if (c.tipo === "numero") {
    const n = Number(t.replace(",", "."));
    return Number.isFinite(n) ? { campo: f.campo, op: f.op, valor: n } : undefined;
  }
  if (c.tipo === "logico") return { campo: f.campo, op: f.op, valor: t === "1" };
  return { campo: f.campo, op: f.op, valor: t };
}

export default function ConstrutorRelatorios() {
  const fontes = useQuery({ queryKey: ["dados-fontes"], queryFn: () => call<FonteDados[]>("dados.fontes") });
  const [fonteId, setFonteId] = useState("vendas");
  const fonte = fontes.data?.find((f) => f.id === fonteId) ?? fontes.data?.[0];
  const [modo, setModo] = useState<Modo>("resumo");
  const [de, setDe] = useState(inicioMes);
  const [ate, setAte] = useState(hojeISO);
  const [campos, setCampos] = useState<string[]>([]);
  const [agrupar, setAgrupar] = useState<string[]>([]);
  const [agregados, setAgregados] = useState<AgregadoDados[]>([]);
  const [filtros, setFiltros] = useState<FiltroEcra[]>([]);
  const [ordenar, setOrdenar] = useState<OrdemDados[]>([]);
  const [periodo, setPeriodo] = useState<Periodo>("mes");
  const [vista, setVista] = useState<"tabela" | "grafico">("tabela");
  // a coluna de valores que o gráfico desenha (por omissão, o total ou a primeira em Kz)
  const [serieId, setSerieId] = useState<string | null>(null);
  const qc = useQueryClient();
  const modelos = useQuery({ queryKey: ["dados-modelos"], queryFn: () => call<Modelo[]>("dados.modelos") });
  const [modeloId, setModeloId] = useState<number | null>(null);
  const modelo = modelos.data?.find((m) => m.id === modeloId) ?? null;
  const [aGuardar, setAGuardar] = useState(false);
  // a definição a aplicar quando a fonte do modelo aberto ficar carregada
  const pendente = useRef<Definicao | null>(null);

  function aplicarPeriodo(p: Periodo, deFixo?: string, ateFixo?: string) {
    setPeriodo(p);
    const d = datasDe(p);
    if (d) {
      setDe(d[0]);
      setAte(d[1]);
    } else if (deFixo && ateFixo) {
      setDe(deFixo);
      setAte(ateFixo);
    }
  }
  function aplicar(d: Definicao) {
    setModo(d.modo ?? "resumo");
    setCampos(d.campos ?? []);
    setAgrupar(d.agrupar ?? []);
    setAgregados(d.agregados ?? []);
    setFiltros(d.filtros ?? []);
    setOrdenar(d.ordenar ?? []);
    setVista(d.vista ?? "tabela");
    aplicarPeriodo(d.periodo ?? "mes", d.de, d.ate);
  }
  function abrirModelo(id: number | null) {
    setModeloId(id);
    const m = modelos.data?.find((x) => x.id === id);
    if (!m) return;
    if (m.fonte === fonte?.id) aplicar(m.definicao);
    else {
      pendente.current = m.definicao;
      setFonteId(m.fonte);
    }
  }
  const definicao = (): Definicao => ({ modo, periodo, de, ate, campos, agrupar, agregados, filtros, ordenar, vista });

  // ao escolher outra fonte, o ponto de partida dela (ou a definição do modelo que se está a abrir)
  useEffect(() => {
    if (!fonte) return;
    const d = pendente.current;
    pendente.current = null;
    if (d) {
      aplicar(d);
      return;
    }
    const p = PADRAO[fonte.id] ?? {
      campos: fonte.campos.slice(0, 5).map((c) => c.id),
      agrupar: fonte.campos.filter((c) => c.tipo === "texto").slice(0, 1).map((c) => c.id),
      agregados: [{ funcao: "contar" as const }],
    };
    setCampos(p.campos);
    setAgrupar(p.agrupar);
    setAgregados(p.agregados);
    setFiltros([]);
    setOrdenar([]);
    // só quando muda de fonte (não quando a lista se recarrega)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fonte?.id]);

  const campoDe = useMemo(() => new Map((fonte?.campos ?? []).map((c) => [c.id, c])), [fonte]);
  const pedido: ConsultaDados | null = useMemo(() => {
    if (!fonte) return null;
    const extra = filtros.map((f) => (campoDe.get(f.campo) ? valorFiltro(f, campoDe.get(f.campo)!) : undefined)).filter((f): f is FiltroDados => !!f);
    return {
      fonte: fonte.id,
      // uma fonte sem data (o stock, as contas a receber) é um retrato de agora: sem período
      filtros: fonte.data ? [{ campo: fonte.data, op: "entre", valor: [de || "0000-01-01", ate || "9999-12-31"] }, ...extra] : extra,
      ...(modo === "detalhe" ? { campos } : { agrupar, agregados }),
      ordenar,
    };
  }, [fonte, filtros, campoDe, de, ate, modo, campos, agrupar, agregados, ordenar]);
  const pronto = !!pedido && (modo === "detalhe" ? campos.length > 0 : agregados.length > 0);
  const chave = useDebounce(JSON.stringify(pedido), 300);
  const resultado = useQuery({
    queryKey: ["dados-consulta", chave],
    queryFn: () => call<ResultadoDados>("dados.consultar", JSON.parse(chave)),
    enabled: pronto && chave !== "null",
    placeholderData: keepPreviousData,
  });

  // ordenação: o clique ordena por essa coluna (de novo: inverte); com Shift junta-a às que já estão
  function aoOrdenar(campo: string, juntar: boolean) {
    setOrdenar((o) => {
      const i = o.findIndex((x) => x.campo === campo);
      if (juntar) return i >= 0 ? o.map((x, k) => (k === i ? { ...x, desc: !x.desc } : x)) : [...o, { campo, desc: false }];
      if (o.length === 1 && i === 0) return [{ campo, desc: !o[0].desc }];
      return [{ campo, desc: false }];
    });
  }

  const titulo = !fonte ? "Relatório" : fonte.data ? `${fonte.rotulo} · ${data(de)} a ${data(ate)}` : `${fonte.rotulo} · situação a ${data(iso(new Date()))}`;
  async function exportar(tipo: "excel" | "csv") {
    if (!pedido) return;
    try {
      // o ficheiro leva todas as linhas (até 5 000), não só as que se vêem
      const completo = await call<ResultadoDados>("dados.consultar", { ...pedido, limite: 5000 });
      if (tipo === "excel") await exportarResultadoExcel(completo, titulo);
      else await exportarResultadoCsv(completo, titulo);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const quantitativos = (fonte?.campos ?? []).filter((c) => c.tipo === "dinheiro" || c.tipo === "numero");
  const agrupaveis = (fonte?.campos ?? []).filter((c) => c.tipo !== "dinheiro" && c.tipo !== "numero");
  const alternar = (lista: string[], id: string) => (lista.includes(id) ? lista.filter((x) => x !== id) : [...lista, id]);
  const r = resultado.data;
  // gráfico: num resumo com grupos, a primeira coluna de valores por grupo (até 60 grupos)
  const graficoPossivel = modo === "resumo" && agrupar.length > 0 && !!r && r.colunas.slice(agrupar.length).some((c) => c.tipo === "dinheiro" || c.tipo === "numero");
  const serie = useMemo(() => {
    if (!r || !graficoPossivel) return null;
    const candidatas = r.colunas.filter((c, i) => i >= agrupar.length && (c.tipo === "dinheiro" || c.tipo === "numero"));
    const col = candidatas.find((c) => c.id === serieId) ?? candidatas.find((c) => c.id.endsWith(":total")) ?? candidatas.find((c) => c.tipo === "dinheiro") ?? candidatas[0];
    if (!col) return null;
    const iv = r.colunas.indexOf(col);
    // o gráfico conta em cêntimos (o eixo divide por 100): as contagens e quantidades vão ×100
    const escala = col.tipo === "dinheiro" ? 1 : 100;
    const pontos = r.linhas.slice(0, 60).map((l) => {
      const chave = l
        .slice(0, agrupar.length)
        .map((v, i) => mostrar(v, r.colunas[i].tipo))
        .join(" · ");
      return { chave, rotulo: chave.length > 10 ? `${chave.slice(0, 9)}…` : chave, valor: Number(l[iv] ?? 0) * escala };
    });
    return { id: col.id, rotulo: col.rotulo, candidatas, pontos, formatar: (v: number) => (col.tipo === "dinheiro" ? kz(v) : numero(v / 100, 2)) };
  }, [r, graficoPossivel, agrupar.length, serieId]);

  if (fontes.data && fontes.data.length === 0) {
    return (
      <>
        <PageHeader title="Construtor de relatórios" />
        <Card>
          <Empty icon={<Rows3 className="size-6" />} title="Sem fontes de dados" text="O seu perfil não tem a permissão de relatórios nem a do fluxo de caixa. Peça-a ao administrador." />
        </Card>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Construtor de relatórios"
        subtitle="O resultado actualiza-se a cada escolha"
        actions={
          <>
            <Button variant="secondary" disabled={!fonte} onClick={() => setAGuardar(true)}>
              <Save className="size-4" /> Guardar modelo
            </Button>
            <Button variant="secondary" disabled={!r} onClick={() => exportar("excel")}>
              <FileSpreadsheet className="size-4" /> Excel
            </Button>
            <Button variant="secondary" disabled={!r} onClick={() => exportar("csv")}>
              <FileText className="size-4" /> CSV
            </Button>
            <Button variant="secondary" disabled={!r} onClick={imprimir}>
              <Printer className="size-4" /> Imprimir
            </Button>
          </>
        }
      />
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[320px_minmax(0,1fr)]">
        <div className="space-y-4">
          <Card className="space-y-3 p-4">
            {!!modelos.data?.length && (
              <Field label="Modelo">
                <Select value={modeloId ?? ""} onChange={(e) => abrirModelo(e.target.value ? Number(e.target.value) : null)}>
                  <option value="">— Relatório novo —</option>
                  {modelos.data.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.nome}
                      {m.partilhado ? " (partilhado)" : ""}
                      {!m.meu && m.dono ? ` · ${m.dono}` : ""}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            <Field label="Fonte de dados">
              <Select value={fonte?.id ?? ""} onChange={(e) => setFonteId(e.target.value)}>
                {fontes.data?.map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.rotulo}
                  </option>
                ))}
              </Select>
            </Field>
            {fonte && <p className="text-[12px] text-ink-3">{fonte.descricao}</p>}
            {fonte && !fonte.data ? (
              <p className="rounded-lg bg-sunken px-3 py-2 text-[12px] text-ink-2">Sem período: mostra a situação de agora. Para escolher, use os filtros.</p>
            ) : (
              <>
                <Field label="Período">
                  <Select value={periodo} onChange={(e) => aplicarPeriodo(e.target.value as Periodo, de, ate)}>
                    {PERIODOS.map(([p, nome]) => (
                      <option key={p} value={p}>
                        {nome}
                      </option>
                    ))}
                  </Select>
                </Field>
                <div className="grid grid-cols-2 gap-2">
                  <Field label="De">
                    <InputData
                      value={de}
                      onChange={(e) => {
                        setDe(e.target.value);
                        setPeriodo("personalizado");
                      }}
                    />
                  </Field>
                  <Field label="Até">
                    <InputData
                      value={ate}
                      onChange={(e) => {
                        setAte(e.target.value);
                        setPeriodo("personalizado");
                      }}
                    />
                  </Field>
                </div>
              </>
            )}
            <Segmentado
              valor={modo}
              onChange={(m) => {
                setModo(m);
                setOrdenar([]);
              }}
              opcoes={[
                { id: "resumo", label: "Resumo", icon: Sigma },
                { id: "detalhe", label: "Detalhe", icon: Rows3 },
              ]}
            />
          </Card>

          {modo === "detalhe" ? (
            <Card className="p-4">
              <div className="mb-2 text-[13px] font-semibold">Colunas</div>
              <div className="flex flex-wrap gap-1.5">
                {fonte?.campos.map((c) => (
                  <Pilula key={c.id} ligada={campos.includes(c.id)} onClick={() => setCampos((x) => alternar(x, c.id))}>
                    {c.rotulo}
                  </Pilula>
                ))}
              </div>
            </Card>
          ) : (
            <Card className="space-y-4 p-4">
              <div>
                <div className="mb-2 text-[13px] font-semibold">Agrupar por</div>
                <div className="flex flex-wrap gap-1.5">
                  {agrupaveis.map((c) => (
                    <Pilula key={c.id} ligada={agrupar.includes(c.id)} onClick={() => setAgrupar((x) => alternar(x, c.id))}>
                      {c.rotulo}
                    </Pilula>
                  ))}
                </div>
                {!agrupar.length && <p className="mt-1.5 text-[12px] text-ink-3">Sem agrupamento: uma só linha com os totais.</p>}
              </div>
              <div>
                <div className="mb-2 text-[13px] font-semibold">Valores</div>
                <label className="mb-1.5 flex items-center gap-2 text-[13px]">
                  <input
                    type="checkbox"
                    checked={agregados.some((a) => a.funcao === "contar")}
                    onChange={(e) => setAgregados((x) => (e.target.checked ? [{ funcao: "contar" }, ...x] : x.filter((a) => a.funcao !== "contar")))}
                  />
                  N.º de registos
                </label>
                <div className="space-y-1.5">
                  {quantitativos.map((c) => {
                    const a = agregados.find((x) => x.campo === c.id);
                    return (
                      <div key={c.id} className="flex items-center gap-2 text-[13px]">
                        <span className="min-w-0 flex-1 truncate">{c.rotulo}</span>
                        <Select
                          className="h-8 w-28 text-[12.5px]"
                          value={a?.funcao ?? ""}
                          onChange={(e) => {
                            const f = e.target.value as FuncaoAgregado | "";
                            setAgregados((x) => {
                              const sem = x.filter((y) => y.campo !== c.id);
                              return f ? [...sem, { funcao: f, campo: c.id }] : sem;
                            });
                          }}
                        >
                          <option value="">—</option>
                          {FUNCOES.map(([f, nome]) => (
                            <option key={f} value={f}>
                              {nome}
                            </option>
                          ))}
                        </Select>
                      </div>
                    );
                  })}
                </div>
              </div>
            </Card>
          )}

          <Card className="space-y-2 p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-1.5 text-[13px] font-semibold">
                <Filter className="size-4 text-ink-3" /> Filtros
              </div>
              <Button
                size="sm"
                variant="ghost"
                disabled={!fonte}
                onClick={() => fonte && setFiltros((x) => [...x, { campo: fonte.campos.find((c) => c.tipo === "texto")?.id ?? fonte.campos[0].id, op: "contem", texto: "" }])}
              >
                <Plus className="size-4" /> Filtro
              </Button>
            </div>
            {!filtros.length && <p className="text-[12px] text-ink-3">Só o período. Junte filtros por cliente, artigo, conta, valor…</p>}
            {filtros.map((f, i) => {
              const c = campoDe.get(f.campo);
              const ops = c ? OPERADORES[c.tipo] : [];
              const muda = (n: Partial<FiltroEcra>) => setFiltros((x) => x.map((y, k) => (k === i ? { ...y, ...n } : y)));
              return (
                <div key={i} className="space-y-1.5 rounded-lg border border-line p-2">
                  <div className="flex gap-1.5">
                    <Select
                      className="h-8 min-w-0 flex-1 text-[12.5px]"
                      value={f.campo}
                      onChange={(e) => {
                        const nc = campoDe.get(e.target.value);
                        muda({ campo: e.target.value, op: nc ? OPERADORES[nc.tipo][0][0] : "=", texto: "" });
                      }}
                    >
                      {fonte?.campos.map((x) => (
                        <option key={x.id} value={x.id}>
                          {x.rotulo}
                        </option>
                      ))}
                    </Select>
                    <button type="button" className="grid size-8 place-items-center rounded-md text-ink-3 hover:bg-sunken hover:text-ink" title="Tirar o filtro" onClick={() => setFiltros((x) => x.filter((_, k) => k !== i))}>
                      <X className="size-4" />
                    </button>
                  </div>
                  <div className="flex gap-1.5">
                    <Select className="h-8 w-32 text-[12.5px]" value={f.op} onChange={(e) => muda({ op: e.target.value as Operador })}>
                      {ops.map(([op, nome]) => (
                        <option key={op} value={op}>
                          {nome}
                        </option>
                      ))}
                    </Select>
                    {c && f.op !== "vazio" && f.op !== "naovazio" && (
                      <div className="min-w-0 flex-1">
                        {c.tipo === "data" ? (
                          <InputData tamanho="sm" value={f.texto} onChange={(e) => muda({ texto: e.target.value })} />
                        ) : c.tipo === "logico" ? (
                          <Select className="h-8 text-[12.5px]" value={f.texto} onChange={(e) => muda({ texto: e.target.value })}>
                            <option value="">—</option>
                            <option value="1">Sim</option>
                            <option value="0">Não</option>
                          </Select>
                        ) : (
                          <Input
                            className={cn("h-8 text-[12.5px]", (c.tipo === "dinheiro" || c.tipo === "numero") && "num text-right")}
                            inputMode={c.tipo === "texto" ? undefined : "decimal"}
                            placeholder={c.tipo === "dinheiro" ? "0,00 Kz" : ""}
                            value={f.texto}
                            onChange={(e) => muda({ texto: e.target.value })}
                          />
                        )}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </Card>
        </div>

        <Card className="min-w-0 overflow-hidden">
          <div className="flex flex-wrap items-baseline justify-between gap-2 border-b border-line px-5 py-3">
            <div className="font-semibold">{titulo}</div>
            <div className="flex items-center gap-3">
              <div className={cn("text-[12.5px] text-ink-3", resultado.isFetching && "animate-pulse")}>
                {r ? `${numero(r.total_linhas, 0)} ${modo === "resumo" && agrupar.length ? (r.total_linhas === 1 ? "grupo" : "grupos") : r.total_linhas === 1 ? "linha" : "linhas"}` : "…"}
                {ordenar.length > 1 && " · Shift + clique junta colunas à ordenação"}
              </div>
              {graficoPossivel && (
                <Segmentado
                  valor={vista}
                  onChange={setVista}
                  opcoes={[
                    { id: "tabela", label: "Tabela", icon: Table2 },
                    { id: "grafico", label: "Gráfico", icon: BarChart3 },
                  ]}
                />
              )}
            </div>
          </div>
          {resultado.error ? (
            <Empty icon={<Rows3 className="size-6" />} title="Não foi possível consultar" text={(resultado.error as Error).message} />
          ) : !pronto ? (
            <Empty icon={<Rows3 className="size-6" />} title={modo === "detalhe" ? "Escolha pelo menos uma coluna" : "Escolha o que somar ou contar"} />
          ) : vista === "grafico" && graficoPossivel && serie ? (
            <div className="p-5">
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2 text-[12.5px] text-ink-3">
                <span>
                  {serie.rotulo}
                  {r && r.linhas.length > serie.pontos.length && ` · os primeiros ${serie.pontos.length} grupos`}
                </span>
                {serie.candidatas.length > 1 && (
                  <Select className="h-8 w-48 text-[12.5px]" value={serie.id} onChange={(e) => setSerieId(e.target.value)} aria-label="Valor no gráfico">
                    {serie.candidatas.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.rotulo}
                      </option>
                    ))}
                  </Select>
                )}
              </div>
              <BarChart dados={serie.pontos} formatar={serie.formatar} altura={340} />
            </div>
          ) : (
            <Grelha r={r} aCarregar={resultado.isLoading} ordenar={ordenar} onOrdenar={aoOrdenar} className="max-h-[calc(100vh-260px)]" />
          )}
        </Card>
      </div>

      {aGuardar && fonte && (
        <GuardarModelo
          modelo={modelo}
          onClose={() => setAGuardar(false)}
          onGuardar={async (nome, partilhado, novo) => {
            const r2 = await call<{ id: number }>("dados.modeloGuardar", { id: novo ? undefined : modelo?.id, nome, fonte: fonte.id, definicao: definicao(), partilhado });
            await qc.invalidateQueries({ queryKey: ["dados-modelos"] });
            setModeloId(r2.id);
            toast.success(`Modelo «${nome}» guardado.`);
            setAGuardar(false);
          }}
          onApagar={async () => {
            if (!modelo) return;
            await call("dados.modeloEliminar", { id: modelo.id });
            await qc.invalidateQueries({ queryKey: ["dados-modelos"] });
            setModeloId(null);
            toast.success(`Modelo «${modelo.nome}» apagado.`);
            setAGuardar(false);
          }}
        />
      )}

      {r && (
        <PrintPortal>
          <div className="p-2 text-[11px] text-black">
            <div className="mb-2 text-[14px] font-bold">{titulo}</div>
            <Grelha r={r} ordenar={ordenar} />
          </div>
        </PrintPortal>
      )}
    </>
  );
}

function Pilula({ ligada, onClick, children }: { ligada: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={ligada}
      onClick={onClick}
      className={cn(
        "rounded-full border px-2.5 py-1 text-[12.5px] transition",
        ligada ? "border-brand bg-brand-50 font-medium text-brand-700" : "border-line text-ink-2 hover:border-brand/40 hover:text-ink",
      )}
    >
      {children}
    </button>
  );
}

/** Guardar o relatório como modelo: com o nome, partilhado ou só para quem o cria; o dono pode apagá-lo. */
function GuardarModelo({
  modelo,
  onClose,
  onGuardar,
  onApagar,
}: {
  modelo: Modelo | null;
  onClose: () => void;
  onGuardar: (nome: string, partilhado: boolean, novo: boolean) => Promise<void>;
  onApagar: () => Promise<void>;
}) {
  const meu = !!modelo?.meu;
  const [nome, setNome] = useState(meu ? (modelo?.nome ?? "") : "");
  const [partilhado, setPartilhado] = useState(meu ? !!modelo?.partilhado : false);
  const [busy, setBusy] = useState(false);
  async function correr(f: () => Promise<void>) {
    setBusy(true);
    try {
      await f();
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
      title={meu ? `Modelo «${modelo?.nome}»` : "Guardar como modelo"}
      width="max-w-md"
      footer={
        <>
          {meu && (
            <Button variant="ghost" className="mr-auto text-danger" disabled={busy} onClick={() => correr(onApagar)}>
              <Trash2 className="size-4" /> Apagar
            </Button>
          )}
          {meu && (
            <Button variant="secondary" disabled={busy || !nome.trim()} onClick={() => correr(() => onGuardar(nome.trim(), partilhado, true))}>
              Guardar como novo
            </Button>
          )}
          <Button loading={busy} disabled={!nome.trim()} onClick={() => correr(() => onGuardar(nome.trim(), partilhado, !meu))}>
            Guardar
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <Field label="Nome">
          <Input autoFocus value={nome} maxLength={80} placeholder="Ex.: Vendas por família este mês" onChange={(e) => setNome(e.target.value)} />
        </Field>
        <label className="flex items-start gap-2.5 text-[13.5px]">
          <input type="checkbox" className="mt-1" checked={partilhado} onChange={(e) => setPartilhado(e.target.checked)} />
          <span>
            Partilhar com a equipa
            <span className="block text-[12px] text-ink-3">Quem tiver a permissão desta fonte vê-o na lista de modelos; só o dono o muda.</span>
          </span>
        </label>
        <p className="text-[12px] text-ink-3">O modelo guarda a fonte, o período (se for «este mês», abre sempre no mês em curso), as colunas ou o agrupamento, os filtros, a ordenação e a vista.</p>
      </div>
    </Modal>
  );
}

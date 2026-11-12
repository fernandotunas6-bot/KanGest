import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { FileCode2, Printer, X } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, inicioMesISO, kz, numero, qtd } from "@/lib/format";
import { useSession } from "@/store/session";
import type { Entidade } from "@/lib/types";
import { EntidadePicker } from "@/components/Pickers";
import { Badge, Button, Card, Input, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { imprimir } from "@/components/DocumentoModal";

const VISTAS = [
  { id: "sintese", label: "Relatório síntese (ano a ano)" },
  { id: "dia", label: "Venda diária" },
  { id: "mes", label: "Por mês" },
  { id: "artigo", label: "Por artigo" },
  { id: "familia", label: "Por família" },
  { id: "cliente", label: "Por cliente" },
  { id: "operador", label: "Por operador" },
  { id: "vendedor", label: "Por vendedor" },
  { id: "pagamento", label: "Por forma de pagamento" },
  { id: "posto", label: "Por posto" },
  { id: "artigo_operador", label: "Artigos por operador" },
  { id: "artigo_entidade", label: "Artigos por entidade" },
  { id: "documento", label: "Detalhe de documentos" },
  { id: "iva", label: "Mapa de IVA" },
  { id: "iva_dia", label: "Mapa de imposto por dia" },
  { id: "iva_familia", label: "Mapa de imposto por família" },
  { id: "irt_c", label: "IRT Grupo C (por mês)" },
] as const;

type Linha = { chave: string; docs?: number; quantidade?: number; liquido?: number; iva?: number; total: number; lucro?: number; irt?: number };

const MESES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];
/** "2026-10" → "Outubro de 2026" */
const mesAno = (chave: string) => `${MESES[Number(chave.slice(5, 7)) - 1] ?? chave} de ${chave.slice(0, 4)}`;

interface Sintese {
  ano: number;
  anterior: number;
  meses: { mes: number; atual: number | null; anterior: number | null; docs_atual: number; docs_anterior: number }[];
  anos: number[];
  mes_em_curso: number | null;
  historico_kigest: number;
}

const pctVar = (atual: number | null, anterior: number | null) => (atual === null || !anterior ? null : (atual - anterior) / Math.abs(anterior));
const fmtPct = (p: number | null) => (p === null ? "—" : `${p > 0 ? "+" : ""}${numero(p * 100, 1, 1)}%`);

/** Tabela do relatório síntese: mês a mês, ano anterior × ano escolhido, variação e acumulados. */
function TabelaSintese({ s }: { s: Sintese }) {
  let acA = 0;
  let acB = 0;
  const linhas = s.meses.map((m) => {
    if (m.atual !== null) acA += m.atual;
    if (m.anterior !== null) acB += m.anterior;
    return { ...m, acA: m.atual !== null || acA ? acA : null, acB: m.anterior !== null || acB ? acB : null };
  });
  const comValor = (k: "atual" | "anterior") => s.meses.filter((m) => m[k] !== null);
  const total = (k: "atual" | "anterior") => comValor(k).reduce((t, m) => t + (m[k] ?? 0), 0);
  const media = (k: "atual" | "anterior") => (comValor(k).length ? Math.round(total(k) / comValor(k).length) : null);
  const melhor = (k: "atual" | "anterior") => comValor(k).reduce<(typeof s.meses)[number] | null>((b, m) => (!b || (m[k] ?? 0) > (b[k] ?? 0) ? m : b), null);
  const v = (x: number | null) => (x === null ? "—" : kz(x));
  const corVar = (x: number | null) => (x === null ? "" : x < 0 ? "text-danger" : x > 0 ? "text-ok" : "");
  return (
    <>
      <thead>
        <tr>
          <Th>Mês</Th>
          <Th className="text-right">{s.anterior}</Th>
          <Th className="text-right">{s.ano}</Th>
          <Th className="text-right">Variação</Th>
          <Th className="text-right">Var. %</Th>
          <Th className="text-right">Acumulado {s.anterior}</Th>
          <Th className="text-right">Acumulado {s.ano}</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((m) => {
          const dif = m.atual !== null && m.anterior !== null ? m.atual - m.anterior : null;
          const p = pctVar(m.atual, m.anterior);
          // mês em curso: valor parcial, a variação ainda não é comparável
          const cor = (x: number | null) => (s.mes_em_curso === m.mes ? "text-ink-3" : corVar(x));
          return (
            <tr key={m.mes}>
              <Td>
                <span className="inline-flex items-center gap-2">
                  {MESES[m.mes - 1]}
                  {s.mes_em_curso === m.mes && <Badge tone="accent">em curso</Badge>}
                </span>
              </Td>
              <Td className="num text-right text-ink-2">{v(m.anterior)}</Td>
              <Td className="num text-right font-medium">{v(m.atual)}</Td>
              <Td className={cn("num text-right", cor(dif))}>{dif === null ? "—" : `${dif > 0 ? "+" : ""}${kz(dif)}`}</Td>
              <Td className={cn("num text-right", cor(p))}>{fmtPct(p)}</Td>
              <Td className="num text-right text-ink-2">{v(m.acB)}</Td>
              <Td className="num text-right">{v(m.acA)}</Td>
            </tr>
          );
        })}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td>Total do ano</Td>
          <Td className="num text-right">{kz(total("anterior"))}</Td>
          <Td className="num text-right">{kz(total("atual"))}</Td>
          <Td className={cn("num text-right", corVar(total("atual") - total("anterior")))}>{kz(total("atual") - total("anterior"))}</Td>
          <Td className="num text-right">{fmtPct(pctVar(total("atual"), total("anterior")))}</Td>
          <Td />
          <Td />
        </tr>
        <tr className="text-ink-2">
          <Td>Média mensal</Td>
          <Td className="num text-right">{v(media("anterior"))}</Td>
          <Td className="num text-right">{v(media("atual"))}</Td>
          <Td colSpan={4} />
        </tr>
        <tr className="text-ink-2">
          <Td>Melhor mês</Td>
          <Td className="text-right">{melhor("anterior") ? `${MESES[melhor("anterior")!.mes - 1]}` : "—"}</Td>
          <Td className="text-right">{melhor("atual") ? `${MESES[melhor("atual")!.mes - 1]}` : "—"}</Td>
          <Td colSpan={4} />
        </tr>
      </tfoot>
    </>
  );
}

export default function Relatorios() {
  const [sp] = useSearchParams();
  const [vista, setVista] = useState<(typeof VISTAS)[number]["id"]>("dia");
  // ?agrupar=<vista>: os itens de relatório do menu (Total por Dia, Mapa de Imposto…) abrem a vista certa
  useEffect(() => {
    const v = VISTAS.find((x) => x.id === sp.get("agrupar"));
    if (v) setVista(v.id);
  }, [sp]);
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const sessao = useSession((s) => s.sessao);
  const saftParam = sp.get("saft");
  const [entidade, setEntidade] = useState<Entidade | null>(null);
  const sintese = vista === "sintese";
  const [ano, setAno] = useState(new Date().getFullYear());
  const { data: linhas = [], isFetching } = useQuery({
    queryKey: ["relatorio", vista, de, ate, entidade?.id ?? 0],
    queryFn: () => call<Linha[]>("relatorios.vendas", { de, ate, agrupar: vista, entidade_id: entidade?.id ?? null }),
    placeholderData: (p) => p,
    enabled: !sintese,
  });
  const qSintese = useQuery({
    queryKey: ["relatorio-sintese", ano],
    queryFn: () => call<Sintese>("relatorios.sintese", { ano }),
    placeholderData: (p) => p,
    enabled: sintese,
  });
  const anos = Array.from(new Set([new Date().getFullYear(), ano, ...(qSintese.data?.anos ?? [])])).sort((a, b) => b - a);
  const has = (k: keyof Linha) => linhas.some((l) => l[k] !== undefined && l[k] !== null);
  const soma = (k: keyof Linha) => linhas.reduce((s, l) => s + (Number(l[k]) || 0), 0);
  const titulo = VISTAS.find((v) => v.id === vista)!.label;
  const [exportando, setExportando] = useState<"" | "F" | "A" | "E">("");

  async function exportarSaft() {
    setExportando("F");
    try {
      const r = await call<{ ficheiro: string; facturas: number; recibos: number }>("saft.exportar", { de, ate });
      toast.success(`SAF-T de facturação gerado: ${r.facturas} facturas, ${r.recibos} recibos`, { description: r.ficheiro, duration: 10000 });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setExportando("");
    }
  }

  async function exportarSaftCompras() {
    setExportando("A");
    try {
      const r = await call<{ ficheiro: string; documentos: number; fornecedores: number; sem_assinatura: number }>("saft.exportarCompras", { de, ate });
      toast.success(`SAF-T de aquisição gerado: ${r.documentos} documentos de ${r.fornecedores} fornecedores`, { description: r.ficheiro, duration: 10000 });
      if (r.sem_assinatura) toast.warning(`${r.sem_assinatura} documento(s) de fornecedor sem os 4 caracteres de assinatura (exportados com "0").`, { duration: 10000 });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setExportando("");
    }
  }

  /** SAF-T de «Faturação para o Estado»: só documentos de entidades públicas. */
  async function exportarSaftEstado() {
    setExportando("E");
    try {
      const r = await call<{ ficheiro: string; facturas: number }>("saft.exportarEstado", { de, ate });
      if (!r.facturas) toast.warning("Não há documentos de entidades públicas no período.", { duration: 8000 });
      else toast.success(`SAF-T do Estado gerado: ${r.facturas} documentos`, { description: r.ficheiro, duration: 10000 });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setExportando("");
    }
  }

  const corpo = (
    <>
      <thead>
        <tr>
          <Th>{vista === "dia" ? "Dia" : vista === "mes" ? "Mês" : titulo.replace("Por ", "").replace(/^\w/, (c) => c.toUpperCase())}</Th>
          {has("docs") && <Th className="text-right">Docs</Th>}
          {has("quantidade") && <Th className="text-right">Quantidade</Th>}
          {has("liquido") && <Th className="text-right">Líquido</Th>}
          {has("iva") && <Th className="text-right">IVA</Th>}
          <Th className="text-right">Total</Th>
          {has("lucro") && <Th className="text-right">Margem bruta</Th>}
          {has("irt") && <Th className="text-right">IRT Grupo C</Th>}
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => (
          <tr key={l.chave}>
            <Td>{vista === "dia" ? data(l.chave) : vista === "mes" ? mesAno(l.chave) : l.chave}</Td>
            {has("docs") && <Td className="num text-right">{l.docs}</Td>}
            {has("quantidade") && <Td className="num text-right">{qtd(l.quantidade)}</Td>}
            {has("liquido") && <Td className="num text-right">{kz(l.liquido)}</Td>}
            {has("iva") && <Td className="num text-right">{kz(l.iva)}</Td>}
            <Td className="num text-right font-medium">{kz(l.total)}</Td>
            {has("lucro") && <Td className={cn("num text-right", (l.lucro ?? 0) < 0 && "text-danger")}>{kz(l.lucro)}</Td>}
            {has("irt") && <Td className="num text-right font-medium">{kz(l.irt)}</Td>}
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td>Total</Td>
          {has("docs") && <Td className="num text-right">{soma("docs")}</Td>}
          {has("quantidade") && <Td className="num text-right">{qtd(soma("quantidade"))}</Td>}
          {has("liquido") && <Td className="num text-right">{kz(soma("liquido"))}</Td>}
          {has("iva") && <Td className="num text-right">{kz(soma("iva"))}</Td>}
          <Td className="num text-right">{kz(soma("total"))}</Td>
          {has("lucro") && <Td className="num text-right">{kz(soma("lucro"))}</Td>}
          {has("irt") && <Td className="num text-right">{kz(soma("irt"))}</Td>}
        </tr>
      </tfoot>
    </>
  );

  return (
    <>
      <PageHeader
        title="Relatórios"
        subtitle="Análise de vendas (facturas e notas de crédito, excluindo anulados)"
        actions={
          <>
            <Button variant={saftParam === "compras" ? "secondary" : saftParam ? "primary" : "secondary"} loading={exportando === "F"} onClick={exportarSaft}>
              <FileCode2 className="size-4" /> SAF-T facturação
            </Button>
            <Button variant={saftParam === "compras" ? "primary" : "secondary"} loading={exportando === "A"} onClick={exportarSaftCompras}>
              <FileCode2 className="size-4" /> SAF-T aquisição
            </Button>
            <Button variant={saftParam === "estado" ? "primary" : "secondary"} loading={exportando === "E"} onClick={exportarSaftEstado}>
              <FileCode2 className="size-4" /> SAF-T Estado
            </Button>
            <Button variant="secondary" onClick={imprimir}>
              <Printer className="size-4" /> Imprimir
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        {sintese ? (
          <>
            <span className="text-[13px] text-ink-2">Ano</span>
            <Select className="w-32" value={String(ano)} onChange={(e) => setAno(Number(e.target.value))}>
              {anos.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </Select>
            <span className="text-[13px] text-ink-3">comparado com {ano - 1}</span>
          </>
        ) : (
          <>
            <Input type="date" className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
            <span className="text-ink-3">até</span>
            <Input type="date" className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
            <div className="w-64">
              <EntidadePicker tipo="C" label="Cliente (opcional)" valor={entidade} onChange={setEntidade} />
            </div>
            {entidade && (
              <Button variant="ghost" size="sm" onClick={() => setEntidade(null)}>
                <X className="size-4" /> Todos os clientes
              </Button>
            )}
          </>
        )}
        <div className="ml-2 flex flex-wrap gap-1 rounded-lg bg-sunken p-1">
          {VISTAS.map((v) => (
            <button
              key={v.id}
              onClick={() => {
                setVista(v.id);
                // «Por mês» com um período de um só mês mostraria uma linha: passa a ser o ano até hoje
                if (v.id === "mes" && de.slice(0, 7) === ate.slice(0, 7)) setDe(`${ate.slice(0, 4)}-01-01`);
              }}
              className={cn("rounded-md px-3 py-1.5 text-[13px]", vista === v.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
            >
              {v.label}
            </button>
          ))}
        </div>
      </div>
      {vista === "irt_c" && <TaxaGrupoC />}
      {sintese ? (
        <Card className={cn("transition-opacity", qSintese.isFetching && "opacity-60")}>
          {qSintese.data && (
            <Table>
              <TabelaSintese s={qSintese.data} />
            </Table>
          )}
          {!!qSintese.data?.historico_kigest && (
            <p className="border-t border-line px-4 py-2.5 text-[12px] text-ink-3">
              Inclui <span className="num">{kz(qSintese.data.historico_kigest)}</span> de vendas importadas do KIGEST.
            </p>
          )}
        </Card>
      ) : (
        <Card className={cn("transition-opacity", isFetching && "opacity-60")}>
          <Table className="max-h-[calc(100vh-250px)]">{corpo}</Table>
          {!linhas.length && <p className="py-12 text-center text-ink-3">Sem dados no período.</p>}
        </Card>
      )}
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <div className="mb-1 text-[16px] font-bold">{sintese ? `Relatório síntese de vendas ${ano}` : titulo}</div>
          <div className="mb-3">
            {sintese ? `Mês a mês, comparado com ${ano - 1}` : `Período ${data(de)} a ${data(ate)}`}
            {!sintese && entidade ? ` · ${entidade.nome}` : ""} · emitido por {sessao?.nome}
          </div>
          <table className="w-full border-collapse text-left">{sintese ? qSintese.data && <TabelaSintese s={qSintese.data} /> : corpo}</table>
        </div>
      </PrintPortal>
    </>
  );
}

/** Taxa do IRT Grupo C sobre o volume de vendas (empresário em nome individual / regime simplificado). */
function TaxaGrupoC() {
  const qc = useQueryClient();
  const admin = useSession((s) => s.tem)("sistema.parametrizacao");
  const atual = useQuery({ queryKey: ["irt-c"], queryFn: () => call<{ taxa: number }>("relatorios.irtC") }).data;
  const [taxa, setTaxa] = useState<string | null>(null);
  const valor = taxa ?? String(atual?.taxa ?? "").replace(".", ",");
  async function gravar() {
    try {
      await call("relatorios.irtCGuardar", { taxa: Number(valor.replace(",", ".")) });
      toast.success("Taxa do IRT Grupo C gravada.");
      setTaxa(null);
      qc.invalidateQueries({ queryKey: ["irt-c"] });
      qc.invalidateQueries({ queryKey: ["relatorio"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <div className="mb-3 flex flex-wrap items-center gap-3 text-[13px] text-ink-2">
      <span>Taxa sobre o volume de vendas sem IVA (%):</span>
      <Input className="num w-24 text-right" value={valor} disabled={!admin} onChange={(e) => setTaxa(e.target.value)} />
      {admin && taxa !== null && (
        <Button size="sm" onClick={gravar}>
          Gravar
        </Button>
      )}
      <span className="text-ink-3">
        6,5% é a taxa do Grupo C mantida no OGE 2026; actividades agrícolas, silvícolas, pecuárias e piscatórias acima de 10 milhões Kz: 10%. Confirme com o seu contabilista.
      </span>
    </div>
  );
}

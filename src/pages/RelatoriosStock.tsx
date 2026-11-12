import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, Boxes, CalendarClock, Coins, FileSpreadsheet, PackageX, Printer, Snowflake, Tags, TrendingUp } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data, kz, numero, qtd } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { imprimir } from "@/components/DocumentoModal";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Input, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface LinhaStock {
  referencia: string;
  descricao: string;
  unidade?: string | null;
  armazem?: string | null;
  quantidade: number;
  qtd_minima?: number;
  qtd_maxima?: number;
  custo_medio?: number;
  preco1?: number;
  valor_custo: number;
  ultima_entrada?: string | null;
  ultima_saida?: string | null;
  dias_sem_saida?: number | null;
  lote?: string;
  validade?: string | null;
  dias_validade?: number | null;
  // lista de preços
  familia?: string | null;
  subfamilia?: string | null;
  custo?: number;
  pvp1?: number;
  pvp2?: number;
  pvp3?: number;
  pvp4?: number;
  promo_atual?: number;
  // previsão de venda
  valor_venda?: number;
  lucro?: number;
}

const SITUACOES = [
  { id: "existencias", label: "Existências", icon: Boxes, texto: "Lista de stock do armazém por família, valorizada ao custo médio" },
  { id: "ruptura", label: "Ruptura", icon: PackageX, texto: "Artigos com existência igual ou inferior a zero" },
  { id: "minimo", label: "Abaixo do mínimo", icon: AlertTriangle, texto: "Ainda há stock, mas já abaixo da quantidade mínima" },
  { id: "excesso", label: "Excesso", icon: TrendingUp, texto: "Existência acima da quantidade máxima definida" },
  { id: "validade", label: "Validade", icon: CalendarClock, texto: "Lotes a expirar dentro do prazo indicado" },
  { id: "parado", label: "Sem saída", icon: Snowflake, texto: "Artigos com stock que não têm saída há muito tempo" },
  { id: "precos", label: "Lista de preços", icon: Tags, texto: "Preços de venda com IVA de todos os artigos activos, por família" },
  { id: "previsao", label: "Previsão de venda", icon: Coins, texto: "Stock existente ao custo e ao preço de venda (PVP 1 sem IVA), com o lucro previsto" },
] as const;

/** Margem do PVP 1 sobre o custo (preços sem IVA); sem custo não há margem. */
const margemCusto = (l: LinhaStock) => (l.custo && l.custo > 0 && l.preco1 !== undefined ? ((l.preco1 - l.custo) / l.custo) * 100 : null);

/** Relatórios de stock do GC: ruptura, alerta de mínimo, excesso, validade e artigos parados. */
export default function RelatoriosStock() {
  const t = useTabelas().data;
  const sessao = useSession((s) => s.sessao);
  const empresa = useEmpresa();
  const [situacao, setSituacao] = useState<(typeof SITUACOES)[number]["id"]>("ruptura");
  // ?situacao=precos|previsao|…: os itens do menu (Lista de Preços, Previsão de Venda) abrem a vista certa
  const [sp] = useSearchParams();
  useEffect(() => {
    const v = SITUACOES.find((x) => x.id === sp.get("situacao"));
    if (v) setSituacao(v.id);
  }, [sp]);
  const [armazem, setArmazem] = useState<number>(t?.posto?.armazem_id ?? 1);
  const [dias, setDias] = useState(30);
  const actual = SITUACOES.find((s) => s.id === situacao)!;
  const comDias = situacao === "validade" || situacao === "parado";
  const precos = situacao === "precos";
  const existencias = situacao === "existencias";
  const previsao = situacao === "previsao";
  const { data: linhas = [], isFetching } = useQuery({
    queryKey: ["rel-stock", situacao, armazem, dias],
    queryFn: () => call<LinhaStock[]>("relatorios.stock", { situacao, armazem_id: armazem, dias }),
    placeholderData: (p) => p,
  });
  const valor = linhas.reduce((s, l) => s + (l.valor_custo ?? 0), 0);
  const venda = linhas.reduce((s, l) => s + (l.valor_venda ?? 0), 0);
  const lucro = linhas.reduce((s, l) => s + (l.lucro ?? 0), 0);

  async function exportar() {
    if (precos) {
      const linhasCsv = [
        ["Referência", "Artigo", "Família", "Subfamília", "Custo", "PVP 1", "PVP 2", "PVP 3", "PVP 4", "Margem PVP 1 (%)", "Promoção (%)"],
        ...linhas.map((l) => [
          l.referencia,
          l.descricao,
          l.familia ?? "",
          l.subfamilia ?? "",
          csvKz(l.custo),
          csvKz(l.pvp1),
          csvKz(l.pvp2),
          csvKz(l.pvp3),
          csvKz(l.pvp4),
          margemCusto(l)?.toFixed(2).replace(".", ",") ?? "",
          l.promo_atual ? String(l.promo_atual).replace(".", ",") : "",
        ]),
      ];
      return exportarFicheiro("lista-de-precos.csv", csv(linhasCsv));
    }
    if (existencias) {
      const linhasCsv = [
        ["Referência", "Artigo", "Família", "Unidade", "Existência", "Custo médio", "Valor ao custo"],
        ...linhas.map((l) => [l.referencia, l.descricao, l.familia ?? "", l.unidade ?? "", qtd(l.quantidade), csvKz(l.custo_medio), csvKz(l.valor_custo)]),
      ];
      return exportarFicheiro("lista-de-stock.csv", csv(linhasCsv));
    }
    if (previsao) {
      const linhasCsv = [
        ["Referência", "Artigo", "Família", "Quantidade", "Custo médio", "PVP 1 s/ IVA", "Valor ao custo", "Valor de venda", "Lucro previsto"],
        ...linhas.map((l) => [l.referencia, l.descricao, l.familia ?? "", qtd(l.quantidade), csvKz(l.custo_medio), csvKz(l.preco1), csvKz(l.valor_custo), csvKz(l.valor_venda), csvKz(l.lucro)]),
      ];
      return exportarFicheiro("previsao-de-venda.csv", csv(linhasCsv));
    }
    const cab = ["Referência", "Artigo", "Armazém"];
    const linhasCsv = [
      situacao === "validade" ? [...cab, "Lote", "Validade", "Dias", "Quantidade", "Valor ao custo"] : [...cab, "Quantidade", "Mínimo", "Máximo", "Dias sem saída", "Valor ao custo"],
      ...linhas.map((l) =>
        situacao === "validade"
          ? [l.referencia, l.descricao, l.armazem ?? "", l.lote ?? "", l.validade ?? "", l.dias_validade ?? "", qtd(l.quantidade), csvKz(l.valor_custo)]
          : [l.referencia, l.descricao, l.armazem ?? "", qtd(l.quantidade), qtd(l.qtd_minima), qtd(l.qtd_maxima), l.dias_sem_saida ?? "", csvKz(l.valor_custo)],
      ),
    ];
    await exportarFicheiro(`stock-${situacao}.csv`, csv(linhasCsv));
  }

  const tabelaPrecos = (
    <>
      <thead>
        <tr>
          <Th>Referência</Th>
          <Th>Artigo</Th>
          <Th>Família</Th>
          <Th className="text-right">Custo</Th>
          <Th className="text-right">PVP 1</Th>
          <Th className="text-right">PVP 2</Th>
          <Th className="text-right">PVP 3</Th>
          <Th className="text-right">PVP 4</Th>
          <Th className="text-right">Margem</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => {
          const m = margemCusto(l);
          return (
            <tr key={l.referencia}>
              <Td className="font-mono text-[12.5px]">{l.referencia}</Td>
              <Td>
                <span className="font-medium">{l.descricao}</span>
                {!!l.promo_atual && (
                  <Badge tone="accent" className="ml-2">
                    −{numero(l.promo_atual, 2)}%
                  </Badge>
                )}
              </Td>
              <Td className="text-ink-2">{[l.familia, l.subfamilia].filter(Boolean).join(" · ") || "—"}</Td>
              <Td className="num text-right text-ink-2">{l.custo ? kz(l.custo) : "—"}</Td>
              <Td className="num text-right font-medium">{kz(l.pvp1)}</Td>
              <Td className="num text-right">{l.pvp2 ? kz(l.pvp2) : "—"}</Td>
              <Td className="num text-right">{l.pvp3 ? kz(l.pvp3) : "—"}</Td>
              <Td className="num text-right">{l.pvp4 ? kz(l.pvp4) : "—"}</Td>
              <Td className={cn("num text-right", m !== null && m < 0 && "text-danger")}>{m === null ? "—" : `${numero(m, 1, 1)}%`}</Td>
            </tr>
          );
        })}
      </tbody>
    </>
  );

  const tabelaPrevisao = (
    <>
      <thead>
        <tr>
          <Th>Referência</Th>
          <Th>Artigo</Th>
          <Th>Família</Th>
          <Th className="text-right">Existência</Th>
          <Th className="text-right">Custo médio</Th>
          <Th className="text-right">PVP 1 s/ IVA</Th>
          <Th className="text-right">Valor ao custo</Th>
          <Th className="text-right">Valor de venda</Th>
          <Th className="text-right">Lucro previsto</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => (
          <tr key={l.referencia}>
            <Td className="font-mono text-[12.5px]">{l.referencia}</Td>
            <Td className="font-medium">{l.descricao}</Td>
            <Td className="text-ink-2">{l.familia ?? "—"}</Td>
            <Td className="num text-right">
              {qtd(l.quantidade)} {l.unidade ?? ""}
            </Td>
            <Td className="num text-right text-ink-2">{kz(l.custo_medio)}</Td>
            <Td className="num text-right text-ink-2">{kz(l.preco1)}</Td>
            <Td className="num text-right">{kz(l.valor_custo)}</Td>
            <Td className="num text-right">{kz(l.valor_venda)}</Td>
            <Td className={cn("num text-right font-medium", (l.lucro ?? 0) < 0 && "text-danger")}>{kz(l.lucro)}</Td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td colSpan={6}>Total</Td>
          <Td className="num text-right">{kz(valor)}</Td>
          <Td className="num text-right">{kz(venda)}</Td>
          <Td className={cn("num text-right", lucro < 0 && "text-danger")}>{kz(lucro)}</Td>
        </tr>
      </tfoot>
    </>
  );

  const tabelaSituacao = (
    <>
      <thead>
        <tr>
          <Th>Referência</Th>
          <Th>Artigo</Th>
          {situacao === "validade" && <Th>Lote</Th>}
          {situacao === "validade" && <Th>Validade</Th>}
          <Th className="text-right">Existência</Th>
          {!comDias && <Th className="text-right">Mínimo</Th>}
          {situacao === "excesso" && <Th className="text-right">Máximo</Th>}
          {situacao === "parado" && <Th className="text-right">Dias sem saída</Th>}
          {situacao === "parado" && <Th>Última saída</Th>}
          <Th className="text-right">Valor ao custo</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l, i) => (
          <tr key={`${l.referencia}-${l.lote ?? i}`}>
            <Td className="font-mono text-[12.5px]">{l.referencia}</Td>
            <Td className="font-medium">{l.descricao}</Td>
            {situacao === "validade" && <Td className="font-mono text-[12.5px]">{l.lote}</Td>}
            {situacao === "validade" && (
              <Td>
                {data(l.validade)}{" "}
                <Badge tone={(l.dias_validade ?? 0) < 0 ? "danger" : (l.dias_validade ?? 0) <= 15 ? "accent" : "neutral"}>
                  {(l.dias_validade ?? 0) < 0 ? `expirou há ${-(l.dias_validade ?? 0)} dias` : `${l.dias_validade} dias`}
                </Badge>
              </Td>
            )}
            <Td className={cn("num text-right", l.quantidade <= 0 && "text-danger")}>
              {qtd(l.quantidade)} {l.unidade ?? ""}
            </Td>
            {!comDias && <Td className="num text-right text-ink-2">{qtd(l.qtd_minima)}</Td>}
            {situacao === "excesso" && <Td className="num text-right text-ink-2">{qtd(l.qtd_maxima)}</Td>}
            {situacao === "parado" && <Td className="num text-right">{l.dias_sem_saida ?? "—"}</Td>}
            {situacao === "parado" && <Td className="text-ink-2">{l.ultima_saida ? data(l.ultima_saida) : "nunca"}</Td>}
            <Td className="num text-right">{kz(l.valor_custo)}</Td>
          </tr>
        ))}
      </tbody>
    </>
  );
  const tabelaExistencias = (
    <>
      <thead>
        <tr>
          <Th>Referência</Th>
          <Th>Artigo</Th>
          <Th>Família</Th>
          <Th className="text-right">Existência</Th>
          <Th className="text-right">Custo médio</Th>
          <Th className="text-right">Valor ao custo</Th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l) => (
          <tr key={l.referencia}>
            <Td className="font-mono text-[12.5px]">{l.referencia}</Td>
            <Td className="font-medium">{l.descricao}</Td>
            <Td className="text-ink-2">{l.familia ?? "—"}</Td>
            <Td className={cn("num text-right", l.quantidade <= 0 ? "text-danger" : l.quantidade <= (l.qtd_minima ?? 0) && "text-[#9a6514]")}>
              {qtd(l.quantidade)} {l.unidade ?? ""}
            </Td>
            <Td className="num text-right text-ink-2">{kz(l.custo_medio)}</Td>
            <Td className="num text-right">{kz(l.valor_custo)}</Td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td colSpan={5}>Total</Td>
          <Td className="num text-right">{kz(valor)}</Td>
        </tr>
      </tfoot>
    </>
  );
  const tabela = existencias ? tabelaExistencias : precos ? tabelaPrecos : previsao ? tabelaPrevisao : tabelaSituacao;

  return (
    <>
      <PageHeader
        title="Relatórios de Stock"
        subtitle={actual.texto}
        actions={
          <>
            <Button variant="secondary" onClick={exportar} disabled={!linhas.length}>
              <FileSpreadsheet className="size-4" /> CSV
            </Button>
            <Button variant="secondary" onClick={imprimir} disabled={!linhas.length}>
              <Printer className="size-4" /> Imprimir
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <div className="flex flex-wrap gap-1 rounded-lg bg-sunken p-1">
          {SITUACOES.map((s) => (
            <button
              key={s.id}
              onClick={() => setSituacao(s.id)}
              className={cn(
                "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[13px]",
                situacao === s.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink",
              )}
            >
              <s.icon className="size-4" />
              {s.label}
            </button>
          ))}
        </div>
        <Select className="w-52" value={armazem} onChange={(e) => setArmazem(Number(e.target.value))}>
          {t?.armazens.map((a) => (
            <option key={a.id} value={a.id}>
              {a.nome}
            </option>
          ))}
        </Select>
        {comDias && (
          <label className="flex items-center gap-2 text-[13px] text-ink-2">
            {situacao === "validade" ? "A expirar em" : "Sem saída há"}
            <Input className="num w-20 text-right" value={dias} onChange={(e) => setDias(Number(e.target.value.replace(/\D/g, "")) || 0)} />
            dias
          </label>
        )}
        <div className="ml-auto text-[13px] text-ink-2">
          {linhas.length} {linhas.length === 1 ? "artigo" : "artigos"}
          {previsao ? (
            <>
              {" "}· lucro previsto <b className={cn(lucro < 0 ? "text-danger" : "text-ink")}>{kz(lucro)}</b>
            </>
          ) : (
            !precos && (
              <>
                {" "}· valor ao custo <b className="text-ink">{kz(valor)}</b>
              </>
            )
          )}
        </div>
      </div>
      <Card className={cn("transition-opacity", isFetching && "opacity-60")}>
        {linhas.length ? (
          <Table className="max-h-[calc(100vh-250px)]">{tabela}</Table>
        ) : (
          <Empty icon={<actual.icon className="size-5" />} title="Nada a assinalar" text={actual.texto} />
        )}
      </Card>
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo={existencias ? "Lista de Stock" : precos || previsao ? actual.label : `Relatório de stock — ${actual.label}`} />
          <div className="mb-3">
            {t?.armazens.find((a) => a.id === armazem)?.nome}
            {comDias ? ` · ${dias} dias` : ""} · emitido por {sessao?.nome}
          </div>
          <table className="w-full border-collapse text-left">{tabela}</table>
          {!precos && !previsao && !existencias && <div className="mt-2 text-right font-bold">Valor ao custo: {kz(valor)}</div>}
        </div>
      </PrintPortal>
    </>
  );
}

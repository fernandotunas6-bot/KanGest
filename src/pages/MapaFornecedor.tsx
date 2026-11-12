import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileSpreadsheet, Printer, Truck, X } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, inicioMesISO, kz } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import type { Entidade } from "@/lib/types";
import { EntidadePicker } from "@/components/Pickers";
import { imprimir } from "@/components/DocumentoModal";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Input, PageHeader, Table, Td, Th, cn } from "@/components/ui";

interface LinhaFornecedor {
  id: number;
  nome: string;
  nif: string;
  telefone: string | null;
  compras: number;
  iva: number;
  pagamentos: number;
  docs: number;
}
interface DocFornecedor {
  id: number;
  numero_doc: string;
  doc_externo: string | null;
  data_emissao: string;
  data_vencimento: string | null;
  tipo: string;
  classe: string;
  total: number;
  total_iva: number;
  valor_pendente: number;
  estado: string;
  moeda: string;
  cambio: number;
}

/** Mapa de fornecedor (Fr_MapaFornecedor): compras, pagamentos e saldo em dívida. */
export default function MapaFornecedor() {
  const empresa = useEmpresa();
  const sessao = useSession((s) => s.sessao);
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const [forn, setForn] = useState<Entidade | null>(null);
  const { data: r, isFetching } = useQuery({
    queryKey: ["mapa-fornecedor", de, ate, forn?.id ?? 0],
    queryFn: () => call<{ resumo: LinhaFornecedor[]; pendentes: { id: number; pendente: number }[]; documentos: DocFornecedor[] }>("relatorios.fornecedores", {
      de,
      ate,
      entidade_id: forn?.id ?? null,
    }),
    placeholderData: (p) => p,
  });
  const resumo = r?.resumo ?? [];
  const pendente = (id: number) => r?.pendentes.find((p) => p.id === id)?.pendente ?? 0;
  const soma = (k: "compras" | "iva" | "pagamentos" | "docs") => resumo.reduce((s, l) => s + (l[k] ?? 0), 0);
  const somaPendente = r?.pendentes.reduce((s, p) => s + p.pendente, 0) ?? 0;

  async function exportar() {
    await exportarFicheiro(
      "mapa-fornecedores.csv",
      csv([
        ["Fornecedor", "NIF", "Documentos", "Compras", "IVA dedutível", "Pagamentos", "Em dívida"],
        ...resumo.map((l) => [l.nome, l.nif, l.docs, csvKz(l.compras), csvKz(l.iva), csvKz(l.pagamentos), csvKz(pendente(l.id))]),
      ]),
    );
  }

  const tabelaResumo = (
    <>
      <thead>
        <tr>
          <Th>Fornecedor</Th>
          <Th>NIF</Th>
          <Th className="text-right">Docs</Th>
          <Th className="text-right">Compras</Th>
          <Th className="text-right">IVA dedutível</Th>
          <Th className="text-right">Pagamentos</Th>
          <Th className="text-right">Em dívida</Th>
        </tr>
      </thead>
      <tbody>
        {resumo.map((l) => (
          <tr key={l.id} className="hover:bg-sunken/60">
            <Td className="font-medium">{l.nome}</Td>
            <Td className="font-mono text-[12.5px]">{l.nif}</Td>
            <Td className="num text-right">{l.docs}</Td>
            <Td className="num text-right">{kz(l.compras)}</Td>
            <Td className="num text-right text-ink-2">{kz(l.iva)}</Td>
            <Td className="num text-right">{kz(l.pagamentos)}</Td>
            <Td className={cn("num text-right font-medium", pendente(l.id) > 0 && "text-danger")}>{kz(pendente(l.id))}</Td>
          </tr>
        ))}
      </tbody>
      <tfoot>
        <tr className="font-semibold">
          <Td colSpan={2}>Total</Td>
          <Td className="num text-right">{soma("docs")}</Td>
          <Td className="num text-right">{kz(soma("compras"))}</Td>
          <Td className="num text-right">{kz(soma("iva"))}</Td>
          <Td className="num text-right">{kz(soma("pagamentos"))}</Td>
          <Td className="num text-right">{kz(somaPendente)}</Td>
        </tr>
      </tfoot>
    </>
  );

  const tabelaDocs = (
    <>
      <thead>
        <tr>
          <Th>Documento</Th>
          <Th>N.º do fornecedor</Th>
          <Th>Data</Th>
          <Th>Vencimento</Th>
          <Th>Tipo</Th>
          <Th className="text-right">Total</Th>
          <Th className="text-right">IVA</Th>
          <Th className="text-right">Em dívida</Th>
        </tr>
      </thead>
      <tbody>
        {r?.documentos.map((d) => (
          <tr key={d.id} className={cn(d.estado === "A" && "opacity-55 line-through")}>
            <Td className="font-mono text-[12.5px] font-medium">{d.numero_doc}</Td>
            <Td className="text-ink-2">{d.doc_externo ?? "—"}</Td>
            <Td>{data(d.data_emissao)}</Td>
            <Td className="text-ink-2">{data(d.data_vencimento)}</Td>
            <Td>
              {d.tipo}
              {d.moeda !== "AOA" && (
                <Badge tone="neutral" className="ml-2">
                  {d.moeda} @ {d.cambio}
                </Badge>
              )}
            </Td>
            <Td className="num text-right">{kz(d.total)}</Td>
            <Td className="num text-right text-ink-2">{kz(d.total_iva)}</Td>
            <Td className={cn("num text-right", d.valor_pendente > 0 && "font-medium text-danger")}>{kz(d.valor_pendente)}</Td>
          </tr>
        ))}
      </tbody>
    </>
  );

  return (
    <>
      <PageHeader
        title="Mapa de Fornecedor"
        subtitle="Compras, IVA dedutível, pagamentos e saldo em dívida por fornecedor"
        actions={
          <>
            <Button variant="secondary" onClick={exportar} disabled={!resumo.length}>
              <FileSpreadsheet className="size-4" /> CSV
            </Button>
            <Button variant="secondary" onClick={imprimir} disabled={!resumo.length}>
              <Printer className="size-4" /> Imprimir
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <Input type="date" className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        <span className="pb-2 text-ink-3">até</span>
        <Input type="date" className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
        <div className="w-72">
          <EntidadePicker tipo="F" label="Fornecedor (opcional)" valor={forn} onChange={setForn} />
        </div>
        {forn && (
          <Button variant="ghost" size="sm" onClick={() => setForn(null)}>
            <X className="size-4" /> Todos
          </Button>
        )}
      </div>
      <Card className={cn("transition-opacity", isFetching && "opacity-60")}>
        {resumo.length ? <Table>{tabelaResumo}</Table> : <Empty icon={<Truck className="size-5" />} title="Sem compras no período" text="Escolha outro período ou outro fornecedor." />}
      </Card>
      {forn && !!r?.documentos.length && (
        <Card className="mt-5">
          <div className="border-b border-line p-4 font-medium">Documentos de {forn.nome}</div>
          <Table className="max-h-[50vh]">{tabelaDocs}</Table>
        </Card>
      )}
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo="Mapa de fornecedor" />
          <div className="mb-3">
            Período {data(de)} a {data(ate)}
            {forn ? ` · ${forn.nome}` : ""} · emitido por {sessao?.nome}
          </div>
          <table className="w-full border-collapse text-left">{tabelaResumo}</table>
          {forn && !!r?.documentos.length && (
            <>
              <div className="mb-1 mt-4 font-bold">Documentos de {forn.nome}</div>
              <table className="w-full border-collapse text-left">{tabelaDocs}</table>
            </>
          )}
        </div>
      </PrintPortal>
    </>
  );
}

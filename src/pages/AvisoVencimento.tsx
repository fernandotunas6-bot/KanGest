import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BellRing, FileSpreadsheet, Printer, Search } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, kz } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { imprimir } from "@/lib/print";
import { Badge, Button, Card, Empty, Input, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";

interface DocVencido {
  id: number;
  numero_doc: string;
  tipo_desc: string;
  data_emissao: string;
  vencimento: string;
  total: number;
  valor_pendente: number;
  dias_atraso: number;
  doc_externo: string | null;
}
interface EntVencida {
  entidade_id: number;
  nome: string;
  nif: string;
  endereco: string | null;
  localidade: string | null;
  telefone: string | null;
  email: string | null;
  total: number;
  documentos: DocVencido[];
}

/** Aviso de Vencimento (Fr_AvisoVencimento): documentos vencidos por entidade e carta de aviso. */
export default function AvisoVencimento() {
  const empresa = useEmpresa();
  const [tipo, setTipo] = useState<"C" | "F">("C");
  const [de, setDe] = useState("");
  const [ate, setAte] = useState("");
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<number | null>(null);
  const [cartas, setCartas] = useState<EntVencida[]>([]);
  const r = useQuery({ queryKey: ["vencidos", tipo, de, ate], queryFn: () => call<{ data: string; entidades: EntVencida[] }>("contas.vencidos", { tipo, de: de || null, ate: ate || null }) });
  const lista = (r.data?.entidades ?? []).filter((e) => !q.trim() || e.nome.toLowerCase().includes(q.trim().toLowerCase()) || e.nif.includes(q.trim()));
  const total = lista.reduce((s, e) => s + e.total, 0);
  const aberta = lista.find((e) => e.entidade_id === sel);

  function imprimirCartas(es: EntVencida[]) {
    setCartas(es);
    imprimir();
  }
  function exportar() {
    exportarFicheiro(
      `Aviso_vencimento_${hojeISO()}.csv`,
      csv([
        ["Entidade", "NIF", "Documento", "Emissão", "Vencimento", "Dias de atraso", "Total", "Pendente"],
        ...lista.flatMap((e) => e.documentos.map((d) => [e.nome, e.nif, d.numero_doc, d.data_emissao, d.vencimento, d.dias_atraso, csvKz(d.total), csvKz(d.valor_pendente)])),
      ]),
    );
  }

  return (
    <>
      <PageHeader
        title="Aviso de Vencimento"
        subtitle={tipo === "C" ? "Clientes com documentos vencidos e por pagar" : "Facturas de fornecedores vencidas"}
        actions={
          <>
            <Button variant="secondary" disabled={!lista.length} onClick={exportar}>
              <FileSpreadsheet className="size-4" /> Exportar
            </Button>
            <Button disabled={!lista.length} onClick={() => imprimirCartas(lista)}>
              <Printer className="size-4" /> Imprimir avisos ({lista.length})
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <Select className="w-44" value={tipo} onChange={(e) => setTipo(e.target.value as "C" | "F")}>
          <option value="C">Clientes</option>
          <option value="F">Fornecedores</option>
        </Select>
        <span className="text-ink-3">Vencidos de</span>
        <Input type="date" className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        <span className="text-ink-3">até</span>
        <Input type="date" className="w-40" value={ate} max={hojeISO()} onChange={(e) => setAte(e.target.value)} />
        <div className="relative min-w-56 flex-1">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
          <Input className="pl-9" placeholder="Pesquisar entidade…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <span className="text-ink-2">
          Total vencido: <b className="num text-ink">{kz(total)}</b>
        </span>
      </div>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] gap-4">
        <Card>
          {!lista.length ? (
            <Empty icon={<BellRing className="size-6" />} title="Sem documentos vencidos" />
          ) : (
            <Table className="max-h-[calc(100vh-270px)]">
              <thead>
                <tr>
                  <Th>Entidade</Th>
                  <Th className="text-right">Docs</Th>
                  <Th className="text-right">Valor vencido</Th>
                </tr>
              </thead>
              <tbody>
                {lista.map((e) => (
                  <tr key={e.entidade_id} className={cn("cursor-pointer hover:bg-sunken/60", sel === e.entidade_id && "bg-brand-50")} onClick={() => setSel(e.entidade_id)}>
                    <Td>
                      <div className="font-medium">{e.nome}</div>
                      <div className="text-[12px] text-ink-3">NIF {e.nif}</div>
                    </Td>
                    <Td className="num text-right">{e.documentos.length}</Td>
                    <Td className="num text-right font-medium text-danger">{kz(e.total)}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>
        <Card className="h-fit">
          {!aberta ? (
            <p className="px-5 py-12 text-center text-ink-3">Seleccione uma entidade para ver os documentos e imprimir o aviso.</p>
          ) : (
            <>
              <div className="flex items-center justify-between border-b border-line px-5 py-3">
                <div>
                  <div className="font-medium">{aberta.nome}</div>
                  <div className="text-[12px] text-ink-3">{[aberta.telefone, aberta.email].filter(Boolean).join(" · ") || "sem contactos"}</div>
                </div>
                <Button size="sm" onClick={() => imprimirCartas([aberta])}>
                  <Printer className="size-4" /> Imprimir aviso
                </Button>
              </div>
              <Table>
                <thead>
                  <tr>
                    <Th>Documento</Th>
                    <Th>Vencimento</Th>
                    <Th className="text-right">Total</Th>
                    <Th className="text-right">Pendente</Th>
                  </tr>
                </thead>
                <tbody>
                  {aberta.documentos.map((d) => (
                    <tr key={d.id}>
                      <Td>
                        <div className="font-medium">{d.numero_doc}</div>
                        <div className="text-[12px] text-ink-3">
                          {d.tipo_desc} · {data(d.data_emissao)}
                        </div>
                      </Td>
                      <Td>
                        {data(d.vencimento)} <Badge tone="danger" className="ml-1">{d.dias_atraso} d</Badge>
                      </Td>
                      <Td className="num text-right">{kz(d.total)}</Td>
                      <Td className="num text-right font-medium">{kz(d.valor_pendente)}</Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </>
          )}
        </Card>
      </div>
      {cartas.length > 0 && (
        <PrintPortal>
          {cartas.map((e, i) => (
            <div key={e.entidade_id} style={{ breakAfter: i < cartas.length - 1 ? "page" : "auto" }}>
              <div className="mx-auto w-[190mm] bg-white p-2 text-[12px] leading-relaxed text-black">
                <CabecalhoEmpresa empresa={empresa} titulo="Aviso de Vencimento" numero={data(hojeISO())} />
                <div className="mt-6 ml-auto w-[85mm]">
                  <div className="font-semibold">{e.nome}</div>
                  <div>NIF {e.nif}</div>
                  {e.endereco && <div>{e.endereco}</div>}
                  {e.localidade && <div>{e.localidade}</div>}
                </div>
                <p className="mt-8">Exmos. Senhores,</p>
                <p className="mt-2">
                  Verificámos que os documentos abaixo se encontram vencidos e por liquidar, no total de <b>{kz(e.total)}</b>. Agradecemos que regularizem a situação com a maior brevidade ou que nos contactem caso o
                  pagamento já tenha sido efectuado.
                </p>
                <table className="mt-4 w-full border-collapse text-left">
                  <thead>
                    <tr className="border-y border-black">
                      <th className="py-1">Documento</th>
                      <th className="py-1">Emissão</th>
                      <th className="py-1">Vencimento</th>
                      <th className="py-1 text-right">Dias</th>
                      <th className="py-1 text-right">Total</th>
                      <th className="py-1 text-right">Em dívida</th>
                    </tr>
                  </thead>
                  <tbody>
                    {e.documentos.map((d) => (
                      <tr key={d.id} className="border-b border-black/20">
                        <td className="py-1">{d.numero_doc}</td>
                        <td className="py-1">{data(d.data_emissao)}</td>
                        <td className="py-1">{data(d.vencimento)}</td>
                        <td className="py-1 text-right">{d.dias_atraso}</td>
                        <td className="py-1 text-right">{kz(d.total)}</td>
                        <td className="py-1 text-right">{kz(d.valor_pendente)}</td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr className="font-bold">
                      <td colSpan={5} className="py-1 text-right">
                        Total em dívida
                      </td>
                      <td className="py-1 text-right">{kz(e.total)}</td>
                    </tr>
                  </tfoot>
                </table>
                {!!empresa?.bancos?.length && (
                  <p className="mt-4 text-[11px]">
                    <b>Coordenadas bancárias:</b> {empresa.bancos.map((b) => `${b.banco} — IBAN ${b.iban}`).join(" · ")}
                  </p>
                )}
                <p className="mt-8">Com os melhores cumprimentos,</p>
                <div className="mt-14 w-64 border-t border-black pt-1 text-center">{empresa?.nome}</div>
              </div>
            </div>
          ))}
        </PrintPortal>
      )}
    </>
  );
}

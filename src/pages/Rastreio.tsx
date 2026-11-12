import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowDownToLine, ArrowUpFromLine, Printer, ScanSearch, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, kz, qtd } from "@/lib/format";
import type { Artigo } from "@/lib/types";
import { ArtigoPicker } from "@/components/Pickers";
import { imprimir } from "@/components/DocumentoModal";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Input, PageHeader, Table, Td, Th, cn } from "@/components/ui";

interface MovLote {
  id: number;
  numero_doc: string;
  data_emissao: string;
  entidade_nome: string;
  entidade_nif: string;
  estado: string;
  tipo: string;
  grupo: string;
  quantidade: number;
}
interface LoteRastreio {
  id: number;
  lote: string;
  data_fabrico: string | null;
  validade: string | null;
  saldo: number;
  custo: number;
  artigo_id: number;
  referencia: string;
  descricao: string;
  armazem: string;
  movimentos: MovLote[];
}

/** Rastreabilidade: para um lote (ou artigo), de quem veio e para quem foi. */
export default function Rastreio() {
  const empresa = useEmpresa();
  const sessao = useSession((s) => s.sessao);
  const [lote, setLote] = useState("");
  const [artigo, setArtigo] = useState<Artigo | null>(null);
  const dLote = useDebounce(lote);
  const procurar = !!dLote.trim() || !!artigo;
  const { data: lotes = [], isFetching } = useQuery({
    queryKey: ["rastreio", dLote, artigo?.id ?? 0],
    queryFn: () => call<LoteRastreio[]>("relatorios.rastreio", { lote: dLote || null, artigo_id: artigo?.id ?? null }),
    enabled: procurar,
    placeholderData: (p) => p,
  });

  const corpo = lotes.map((l) => {
    const entradas = l.movimentos.filter((m) => m.quantidade > 0);
    const saidas = l.movimentos.filter((m) => m.quantidade < 0);
    return (
      <Card key={l.id} className="mb-4">
        <div className="flex flex-wrap items-center gap-x-6 gap-y-1 border-b border-line p-4">
          <div>
            <div className="text-[12px] uppercase tracking-wide text-ink-3">Lote</div>
            <div className="font-mono text-[15px] font-semibold">{l.lote}</div>
          </div>
          <div>
            <div className="text-[12px] uppercase tracking-wide text-ink-3">Artigo</div>
            <div className="font-medium">
              <span className="font-mono text-[12.5px] text-ink-3">{l.referencia}</span> {l.descricao}
            </div>
          </div>
          <div>
            <div className="text-[12px] uppercase tracking-wide text-ink-3">Armazém</div>
            <div>{l.armazem}</div>
          </div>
          <div>
            <div className="text-[12px] uppercase tracking-wide text-ink-3">Fabrico / Validade</div>
            <div>
              {data(l.data_fabrico)} · {data(l.validade)}
            </div>
          </div>
          <div className="ml-auto text-right">
            <div className="text-[12px] uppercase tracking-wide text-ink-3">Em stock</div>
            <div className={cn("num text-[15px] font-semibold", l.saldo <= 0 && "text-ink-3")}>{qtd(l.saldo)}</div>
          </div>
        </div>
        <Table>
          <thead>
            <tr>
              <Th>Documento</Th>
              <Th>Data</Th>
              <Th>Tipo</Th>
              <Th>Entidade</Th>
              <Th className="text-right">Quantidade</Th>
            </tr>
          </thead>
          <tbody>
            {l.movimentos.map((m) => (
              <tr key={`${m.id}-${m.quantidade}`} className={cn(m.estado === "A" && "opacity-55 line-through")}>
                <Td className="font-mono text-[12.5px] font-medium">{m.numero_doc}</Td>
                <Td>{data(m.data_emissao)}</Td>
                <Td>
                  <Badge tone={m.quantidade > 0 ? "ok" : "accent"}>
                    {m.quantidade > 0 ? <ArrowDownToLine className="mr-1 size-3" /> : <ArrowUpFromLine className="mr-1 size-3" />}
                    {m.tipo}
                  </Badge>
                </Td>
                <Td>
                  {m.entidade_nome} <span className="text-ink-3">· {m.entidade_nif}</span>
                </Td>
                <Td className={cn("num text-right font-medium", m.quantidade < 0 && "text-danger")}>{qtd(m.quantidade)}</Td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="font-semibold">
              <Td colSpan={4}>
                Entradas {qtd(entradas.reduce((s, m) => s + m.quantidade, 0))} · saídas {qtd(-saidas.reduce((s, m) => s + m.quantidade, 0))} · custo do lote {kz(l.custo)}
              </Td>
              <Td className="num text-right">{qtd(l.movimentos.reduce((s, m) => s + m.quantidade, 0))}</Td>
            </tr>
          </tfoot>
        </Table>
        {!l.movimentos.length && <p className="py-8 text-center text-ink-3">Este lote ainda não tem movimentos em documentos.</p>}
      </Card>
    );
  });

  return (
    <>
      <PageHeader
        title="Rastreabilidade de Lotes"
        subtitle="Para cada lote: o documento e o fornecedor de entrada, e todos os clientes a quem saiu"
        actions={
          <Button variant="secondary" onClick={imprimir} disabled={!lotes.length}>
            <Printer className="size-4" /> Imprimir
          </Button>
        }
      />
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <div className="relative w-72">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
          <Input className="pl-9 font-mono" placeholder="N.º do lote…" value={lote} onChange={(e) => setLote(e.target.value)} />
        </div>
        <div className="w-96">
          <ArtigoPicker onPick={(a) => setArtigo(a)} />
        </div>
        {artigo && (
          <Button variant="ghost" size="sm" onClick={() => setArtigo(null)}>
            {artigo.descricao} ✕
          </Button>
        )}
      </div>
      {!procurar ? (
        <Empty icon={<ScanSearch className="size-5" />} title="Indique o lote ou o artigo" text="Pode procurar por parte do número do lote." />
      ) : !lotes.length && !isFetching ? (
        <Empty icon={<ScanSearch className="size-5" />} title="Sem lotes" text="Nenhum lote corresponde à procura." />
      ) : (
        <div className={cn("transition-opacity", isFetching && "opacity-60")}>{corpo}</div>
      )}
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo="Rastreabilidade de lotes" />
          <div className="mb-3">
            {lote && `Lote ${lote} · `}
            {artigo?.descricao ?? ""} · emitido por {sessao?.nome}
          </div>
          {lotes.map((l) => (
            <div key={l.id} className="mb-4">
              <div className="font-bold">
                Lote {l.lote} — {l.referencia} {l.descricao} ({l.armazem}) · validade {data(l.validade)} · em stock {qtd(l.saldo)}
              </div>
              <table className="w-full border-collapse text-left">
                <thead>
                  <tr>
                    <Th>Documento</Th>
                    <Th>Data</Th>
                    <Th>Tipo</Th>
                    <Th>Entidade</Th>
                    <Th className="text-right">Quantidade</Th>
                  </tr>
                </thead>
                <tbody>
                  {l.movimentos.map((m) => (
                    <tr key={`${m.id}-p`}>
                      <Td>{m.numero_doc}</Td>
                      <Td>{data(m.data_emissao)}</Td>
                      <Td>{m.tipo}</Td>
                      <Td>
                        {m.entidade_nome} · {m.entidade_nif}
                      </Td>
                      <Td className="num text-right">{qtd(m.quantidade)}</Td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      </PrintPortal>
    </>
  );
}

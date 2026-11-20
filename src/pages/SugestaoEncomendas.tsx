// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { PackagePlus } from "lucide-react";
import { call } from "@/lib/api";
import { kz, numero, qtd } from "@/lib/format";
import { exportarExcel } from "@/lib/csv";
import { kzXL } from "@/lib/xlsx";
import type { DocCompleto, Entidade } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, PageHeader, Table, Td, Th } from "@/components/ui";
import { EntidadePicker } from "@/components/Pickers";
import DocumentoModal from "@/components/DocumentoModal";

interface Linha {
  artigo_id: number;
  referencia: string;
  descricao: string;
  stock: number;
  encomendado: number;
  minimo: number;
  maximo: number;
  media_diaria: number;
  dias_cobertura: number | null;
  sugerido: number;
  custo: number;
  valor: number;
  fornecedor_id: number | null;
  fornecedor: string | null;
  motivo: string;
}

/** Sugestão de encomendas a fornecedores: mínimos e máximos, rotação e encomendas em curso. */
export default function SugestaoEncomendas() {
  const [dias, setDias] = useState(30);
  const [cobertura, setCobertura] = useState(15);
  const { data, refetch, isFetching } = useQuery({
    queryKey: ["sugestao", dias, cobertura],
    queryFn: () => call<{ linhas: Linha[] }>("compras.sugestao", { dias, cobertura }),
  });
  // quantidade a encomendar (editável) e linhas escolhidas
  const [qtds, setQtds] = useState<Record<number, number>>({});
  const [fora, setFora] = useState<Set<number>>(new Set());
  const [semFornecedor, setSemFornecedor] = useState<Entidade | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [aberto, setAberto] = useState<number | null>(null);
  const grupos = useMemo(() => {
    const m = new Map<string, Linha[]>();
    for (const l of data?.linhas ?? []) {
      const k = l.fornecedor_id ? String(l.fornecedor_id) : "";
      m.set(k, [...(m.get(k) ?? []), l]);
    }
    return [...m.entries()];
  }, [data]);

  async function encomendar(chave: string, linhas: Linha[]) {
    const fornecedor = chave ? Number(chave) : semFornecedor?.id;
    if (!fornecedor) return toast.error("Escolha o fornecedor destas linhas.");
    const escolhidas = linhas.filter((l) => !fora.has(l.artigo_id) && (qtds[l.artigo_id] ?? l.sugerido) > 0);
    if (!escolhidas.length) return toast.error("Nenhuma linha escolhida.");
    setBusy(chave);
    try {
      const d = await call<DocCompleto>("documentos.emitir", {
        tipo: "VNE",
        entidade_id: fornecedor,
        observacoes: `Sugestão de encomenda (rotação de ${dias} dias, cobertura de ${cobertura} dias)`,
        linhas: escolhidas.map((l) => ({ artigo_id: l.artigo_id, quantidade: qtds[l.artigo_id] ?? l.sugerido, preco_unitario: l.custo })),
      });
      toast.success(`${d.documento.numero_doc} criada.`, { action: { label: "Abrir", onClick: () => setAberto(d.documento.id) } });
      setQtds({});
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(null);
    }
  }

  function exportar() {
    exportarExcel(
      "sugestao-encomendas.xlsx",
      [
        ["Fornecedor", "Referência", "Artigo", "Stock", "Encomendado", "Mínimo", "Máximo", "Venda/dia", "A encomendar", "Custo", "Valor", "Motivo"],
        ...(data?.linhas ?? []).map((l) => [l.fornecedor ?? "", l.referencia, l.descricao, l.stock, l.encomendado, l.minimo, l.maximo, l.media_diaria, l.sugerido, kzXL(l.custo), kzXL(l.valor), l.motivo]),
      ],
      { titulo: "Sugestão de encomendas a fornecedores" },
    );
  }

  return (
    <>
      <PageHeader
        title="Sugestão de Encomendas"
        subtitle="Artigos abaixo do mínimo, ou cujo stock não cobre os próximos dias ao ritmo de venda, já descontando o que está encomendado"
        actions={
          <Button variant="secondary" onClick={exportar} disabled={!data?.linhas.length}>
            Exportar Excel
          </Button>
        }
      />
      <Card className="mb-4 flex flex-wrap items-end gap-4 p-4">
        <Field label="Rotação: vendas dos últimos (dias)">
          <Input className="num w-28" type="number" min={1} max={365} value={dias} onChange={(e) => setDias(Number(e.target.value) || 30)} />
        </Field>
        <Field label="O stock deve cobrir (dias)">
          <Input className="num w-28" type="number" min={1} max={365} value={cobertura} onChange={(e) => setCobertura(Number(e.target.value) || 15)} />
        </Field>
        <p className="max-w-xl text-[12.5px] text-ink-3">
          Com máximo definido na ficha do artigo, encomenda-se até ao máximo; sem máximo, o que cobre os dias pedidos. O fornecedor sugerido é o da última compra.
        </p>
      </Card>
      {!isFetching && !data?.linhas.length && (
        <Card>
          <Empty icon={<PackagePlus className="size-6" />} title="Nada a encomendar" text="Nenhum artigo está abaixo do mínimo nem fica sem stock nos próximos dias." />
        </Card>
      )}
      <div className="grid gap-4">
        {grupos.map(([chave, linhas]) => {
          const total = linhas.filter((l) => !fora.has(l.artigo_id)).reduce((s, l) => s + Math.round((qtds[l.artigo_id] ?? l.sugerido) * l.custo), 0);
          return (
            <Card key={chave || "sem"}>
              <div className="flex flex-wrap items-center justify-between gap-3 px-5 pt-4">
                <div className="font-semibold">{linhas[0].fornecedor ?? "Sem fornecedor habitual"}</div>
                <div className="flex items-end gap-2">
                  {!chave && <EntidadePicker tipo="F" valor={semFornecedor} onChange={setSemFornecedor} />}
                  <span className="num text-ink-2">{kz(total)}</span>
                  <Button loading={busy === chave} onClick={() => encomendar(chave, linhas)}>
                    Criar encomenda
                  </Button>
                </div>
              </div>
              <Table>
                <thead>
                  <tr>
                    <Th className="w-8" />
                    <Th>Artigo</Th>
                    <Th className="text-right">Stock</Th>
                    <Th className="text-right">Encomendado</Th>
                    <Th className="text-right">Mín. / Máx.</Th>
                    <Th className="text-right">Venda/dia</Th>
                    <Th className="text-right">A encomendar</Th>
                    <Th className="text-right">Custo</Th>
                  </tr>
                </thead>
                <tbody>
                  {linhas.map((l) => (
                    <tr key={l.artigo_id} className={fora.has(l.artigo_id) ? "opacity-50" : undefined}>
                      <Td>
                        <input
                          type="checkbox"
                          checked={!fora.has(l.artigo_id)}
                          onChange={(e) => {
                            const n = new Set(fora);
                            if (e.target.checked) n.delete(l.artigo_id);
                            else n.add(l.artigo_id);
                            setFora(n);
                          }}
                        />
                      </Td>
                      <Td>
                        <div className="font-medium">{l.descricao}</div>
                        <div className="text-[12px] text-ink-3">
                          {l.referencia} · <Badge tone={l.motivo === "abaixo do mínimo" ? "danger" : "accent"}>{l.motivo}</Badge>
                          {l.dias_cobertura !== null && <span className="ml-1">dá para {numero(l.dias_cobertura, 0)} dia(s)</span>}
                        </div>
                      </Td>
                      <Td className="num text-right">{qtd(l.stock)}</Td>
                      <Td className="num text-right">{qtd(l.encomendado)}</Td>
                      <Td className="num text-right text-ink-2">
                        {qtd(l.minimo)} / {l.maximo ? qtd(l.maximo) : "—"}
                      </Td>
                      <Td className="num text-right text-ink-2">{qtd(l.media_diaria)}</Td>
                      <Td className="text-right">
                        <Input
                          className="num ml-auto h-8 w-24 text-right"
                          type="number"
                          min={0}
                          step="any"
                          value={qtds[l.artigo_id] ?? l.sugerido}
                          onChange={(e) => setQtds({ ...qtds, [l.artigo_id]: Number(e.target.value) })}
                        />
                      </Td>
                      <Td className="num text-right">{kz(l.custo)}</Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </Card>
          );
        })}
      </div>
      <DocumentoModal id={aberto} onClose={() => setAberto(null)} />
    </>
  );
}

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CheckCircle2, RefreshCw } from "lucide-react";
import { call } from "@/lib/api";
import { qtd } from "@/lib/format";
import { Button, Card, Empty, PageHeader, Table, Td, Th } from "@/components/ui";

interface Diferenca {
  artigo_id: number;
  armazem_id: number;
  referencia: string;
  descricao: string;
  armazem: string | null;
  gravado: number;
  calculado: number;
}
interface DiferencaLote {
  id: number;
  lote: string;
  referencia: string;
  descricao: string;
  gravado: number;
  calculado: number;
}
interface Resultado {
  diferencas: Diferenca[];
  lotes: DiferencaLote[];
  aplicado: boolean;
  saldos_corrigidos: number;
}

/** Recálculo de Stock (Fr_RecalculoStock): reconstrói as existências a partir dos movimentos. */
export default function RecalculoStock() {
  const qc = useQueryClient();
  const analise = useQuery({ queryKey: ["stock-recalculo"], queryFn: () => call<Resultado>("stock.recalcular", { aplicar: false }) });
  const [busy, setBusy] = useState(false);
  const r = analise.data;
  const n = (r?.diferencas.length ?? 0) + (r?.lotes.length ?? 0);

  async function aplicar() {
    setBusy(true);
    try {
      const x = await call<Resultado>("stock.recalcular", { aplicar: true });
      toast.success(`Stock recalculado: ${x.diferencas.length} existência(s) e ${x.lotes.length} lote(s) corrigidos; ${x.saldos_corrigidos} saldo(s) de movimentos refeitos.`);
      qc.invalidateQueries();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Recálculo de Stock"
        subtitle="Compara as existências gravadas com a soma dos movimentos de cada artigo, armazém e lote"
        actions={
          <div className="flex gap-2">
            <Button variant="secondary" loading={analise.isFetching} onClick={() => analise.refetch()}>
              <RefreshCw className="size-4" /> Analisar de novo
            </Button>
            <Button loading={busy} disabled={!r} onClick={aplicar}>
              Recalcular {n ? `(${n})` : ""}
            </Button>
          </div>
        }
      />
      <p className="mb-4 max-w-3xl text-[13px] text-ink-3">
        O stock é actualizado em cada documento, por isso normalmente não há diferenças. Use o recálculo depois de uma importação, de restaurar uma cópia ou se suspeitar de
        existências erradas. «Recalcular» também refaz o saldo acumulado do extracto de movimentos e fica registado na auditoria.
      </p>
      <Card>
        {analise.isLoading ? (
          <div className="p-8 text-center text-ink-3">A analisar…</div>
        ) : !n ? (
          <Empty icon={<CheckCircle2 className="size-6" />} title="Sem diferenças" text="As existências gravadas coincidem com os movimentos." />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Artigo</Th>
                <Th>Armazém / lote</Th>
                <Th className="text-right">Gravado</Th>
                <Th className="text-right">Pelos movimentos</Th>
                <Th className="text-right">Diferença</Th>
              </tr>
            </thead>
            <tbody>
              {r?.diferencas.map((d) => (
                <tr key={`s${d.artigo_id}-${d.armazem_id}`}>
                  <Td>
                    <div className="font-medium">{d.descricao}</div>
                    <div className="font-mono text-[12px] text-ink-3">{d.referencia}</div>
                  </Td>
                  <Td>{d.armazem ?? `Armazém ${d.armazem_id}`}</Td>
                  <Td className="num text-right">{qtd(d.gravado)}</Td>
                  <Td className="num text-right">{qtd(d.calculado)}</Td>
                  <Td className="num text-right font-medium text-danger">{qtd(d.calculado - d.gravado)}</Td>
                </tr>
              ))}
              {r?.lotes.map((l) => (
                <tr key={`l${l.id}`}>
                  <Td>
                    <div className="font-medium">{l.descricao}</div>
                    <div className="font-mono text-[12px] text-ink-3">{l.referencia}</div>
                  </Td>
                  <Td>Lote {l.lote}</Td>
                  <Td className="num text-right">{qtd(l.gravado)}</Td>
                  <Td className="num text-right">{qtd(l.calculado)}</Td>
                  <Td className="num text-right font-medium text-danger">{qtd(l.calculado - l.gravado)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
    </>
  );
}

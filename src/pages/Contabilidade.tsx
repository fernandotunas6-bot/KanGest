import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { call } from "@/lib/api";
import { data, hojeISO, inicioMesISO, kz } from "@/lib/format";
import { exportarExcel } from "@/lib/csv";
import { dataXL, kzXL } from "@/lib/xlsx";
import { useSession } from "@/store/session";
import { InputData } from "@/components/InputData";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Table, Td, Th } from "@/components/ui";

interface Lancamentos {
  linhas: { data: string; documento: string; diario: string; conta: string; descricao: string; nif: string; debito: number; credito: number }[];
  balancete: { conta: string; debito: number; credito: number; saldo: number }[];
  debito: number;
  credito: number;
  equilibrado: boolean;
}

const NOMES: Record<string, string> = {
  clientes: "Clientes (conta corrente)",
  fornecedores: "Fornecedores (conta corrente)",
  caixa: "Caixa (numerário)",
  bancos: "Bancos (Multicaixa, transferências…)",
  vendas_produtos: "Vendas de produtos",
  vendas_servicos: "Prestações de serviços",
  compras: "Compras",
  iva_liquidado: "IVA liquidado",
  iva_dedutivel: "IVA dedutível",
  retencoes_clientes: "Retenções sofridas (clientes)",
  retencoes_fornecedores: "Retenções efectuadas (fornecedores)",
  fidelizacao: "Pontos de fidelização usados a pagar",
};

/** Lançamentos dos documentos para o programa de contabilidade (PGC angolano, contas configuráveis). */
export default function Contabilidade() {
  const tem = useSession((s) => s.tem);
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const [contas, setContas] = useState(false);
  const { data: l, isFetching, refetch } = useQuery({ queryKey: ["lancamentos", de, ate], queryFn: () => call<Lancamentos>("contabilidade.lancamentos", { de, ate }) });
  function exportar() {
    if (!l) return;
    exportarExcel(
      `lancamentos-${de}-${ate}.xlsx`,
      [["Data", "Diário", "Documento", "Conta", "Descrição", "NIF", "Débito", "Crédito"], ...l.linhas.map((x) => [dataXL(x.data), x.diario, x.documento, x.conta, x.descricao, x.nif, kzXL(x.debito), kzXL(x.credito)])],
      {
        titulo: `Lançamentos de ${data(de)} a ${data(ate)}`,
        folha: "Lançamentos",
        extra: [{ nome: "Balancete", linhas: [["Conta", "Débito", "Crédito", "Saldo"], ...l.balancete.map((b) => [b.conta, kzXL(b.debito), kzXL(b.credito), kzXL(b.saldo)])] }],
      },
    );
  }
  return (
    <>
      <PageHeader
        title="Exportação para a Contabilidade"
        subtitle="Diário dos documentos do período (vendas, notas, recibos, compras e pagamentos) com débito e crédito por conta"
        actions={
          <>
            {tem("sistema.configuracoes") && (
              <Button variant="secondary" onClick={() => setContas(true)}>
                Plano de contas
              </Button>
            )}
            <Button onClick={exportar} disabled={!l?.linhas.length}>
              Exportar Excel
            </Button>
          </>
        }
      />
      <Card className="mb-4 flex flex-wrap items-end gap-4 p-4">
        <Field label="De">
          <InputData className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        </Field>
        <Field label="Até">
          <InputData className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
        </Field>
        {l && (
          <div className="ml-auto flex items-center gap-3 text-[13px]">
            <span>
              Débito <b className="num">{kz(l.debito)}</b> · Crédito <b className="num">{kz(l.credito)}</b>
            </span>
            <Badge tone={l.equilibrado ? "ok" : "danger"}>{l.equilibrado ? "Equilibrado" : "Desequilibrado"}</Badge>
          </div>
        )}
      </Card>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[2fr_1fr]">
        <Card>
          <div className="px-5 pt-4 font-semibold">Lançamentos {isFetching && <span className="text-ink-3">…</span>}</div>
          <Table className="max-h-[60vh]">
            <thead>
              <tr>
                <Th>Data</Th>
                <Th>Documento</Th>
                <Th>Conta</Th>
                <Th>Descrição</Th>
                <Th className="text-right">Débito</Th>
                <Th className="text-right">Crédito</Th>
              </tr>
            </thead>
            <tbody>
              {l?.linhas.map((x, i) => (
                <tr key={i}>
                  <Td className="text-ink-2">{data(x.data)}</Td>
                  <Td className="font-mono text-[12px]">{x.documento}</Td>
                  <Td className="font-mono">{x.conta}</Td>
                  <Td>{x.descricao}</Td>
                  <Td className="num text-right">{x.debito ? kz(x.debito) : ""}</Td>
                  <Td className="num text-right">{x.credito ? kz(x.credito) : ""}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          {!l?.linhas.length && <p className="py-6 text-center text-ink-3">Sem documentos no período.</p>}
        </Card>
        <Card>
          <div className="px-5 pt-4 font-semibold">Balancete do período</div>
          <Table>
            <thead>
              <tr>
                <Th>Conta</Th>
                <Th className="text-right">Saldo</Th>
              </tr>
            </thead>
            <tbody>
              {l?.balancete.map((b) => (
                <tr key={b.conta}>
                  <Td className="font-mono">{b.conta}</Td>
                  <Td className="num text-right">{kz(b.saldo)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          <p className="px-5 py-3 text-[12px] text-ink-3">Saldo = débito − crédito. Ficam de fora os documentos anulados e os importados de outro programa (já contabilizados lá).</p>
        </Card>
      </div>
      {contas && <PlanoContas onClose={() => setContas(false)} onDone={() => { setContas(false); refetch(); }} />}
    </>
  );
}

function PlanoContas({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { data: c } = useQuery({ queryKey: ["contabilidade-contas"], queryFn: () => call<{ contas: Record<string, string>; padrao: Record<string, string> }>("contabilidade.contas") });
  const [f, setF] = useState<Record<string, string>>({});
  useEffect(() => {
    if (c) setF(c.contas);
  }, [c]);
  async function gravar() {
    try {
      await call("contabilidade.contasGuardar", f);
      toast.success("Plano de contas gravado.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title="Plano de contas (PGC)" width="max-w-xl" footer={<Button onClick={gravar}>Gravar</Button>}>
      <div className="grid grid-cols-2 gap-3">
        {Object.keys(NOMES).map((k) => (
          <Field key={k} label={NOMES[k]} hint={c?.padrao[k] ? `Por omissão ${c.padrao[k]}` : undefined}>
            <Input className="font-mono" value={f[k] ?? ""} onChange={(e) => setF({ ...f, [k]: e.target.value })} />
          </Field>
        ))}
      </div>
    </Modal>
  );
}

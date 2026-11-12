import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data, kz, parseKz, toInputKz } from "@/lib/format";
import type { DocCompleto, Entidade } from "@/lib/types";
import { Badge, Button, Card, Field, Input, PageHeader, Select, Table, Td, Th } from "@/components/ui";
import DocumentoModal from "@/components/DocumentoModal";
import { EntidadePicker } from "@/components/Pickers";

interface Credito {
  id: number;
  numero_doc: string;
  tipo_desc: string;
  data_emissao: string;
  credito_disponivel: number;
}

interface Pendente {
  id: number;
  numero_doc: string;
  tipo_desc: string;
  doc_externo: string | null;
  data_emissao: string;
  data_vencimento: string | null;
  total: number;
  valor_pendente: number;
  dias_atraso: number;
}

/** Recibo de cliente (RC) ou Pagamento a fornecedor (VRC). */
export default function Recibo({ tipo = "RC" }: { tipo?: "RC" | "VRC" }) {
  const nav = useNavigate();
  const qc = useQueryClient();
  const t = useTabelas().data;
  const fornecedor = tipo === "VRC";
  const [entidade, setEntidade] = useState<Entidade | null>(null);
  const [pendentes, setPendentes] = useState<Pendente[]>([]);
  const [valores, setValores] = useState<Record<number, string>>({});
  const [modo, setModo] = useState(fornecedor ? "TB" : "NU");
  const [creditos, setCreditos] = useState<Credito[]>([]);
  const [compensar, setCompensar] = useState("");
  const [busy, setBusy] = useState(false);
  const [emitido, setEmitido] = useState<number | null>(null);

  useEffect(() => {
    setEntidade(null);
    setPendentes([]);
  }, [tipo]);

  useEffect(() => {
    if (!entidade) return;
    call<{ documentos: Pendente[] }>("contas.pendentes", { tipo: fornecedor ? "F" : "C", entidade_id: entidade.id }).then((r) => {
      setPendentes(r.documentos);
      setValores({});
    });
    call<Credito[]>("contas.creditos", { entidade_id: entidade.id, grupo: fornecedor ? "COMPRAS" : "VENDAS" }).then(setCreditos);
    setCompensar("");
  }, [entidade, fornecedor]);

  const total = Object.values(valores).reduce((s, v) => s + parseKz(v || "0"), 0);
  const divida = pendentes.reduce((s, p) => s + p.valor_pendente, 0);
  const creditoTotal = creditos.reduce((s, c) => s + c.credito_disponivel, 0);
  const cs = Math.min(parseKz(compensar || "0"), creditoTotal, total);
  const dinheiro = total - cs;

  async function emitir() {
    setBusy(true);
    try {
      const liquidar = Object.entries(valores)
        .map(([id, v]) => ({ documento_id: Number(id), valor: parseKz(v || "0") }))
        .filter((l) => l.valor > 0);
      const pagamentos = [
        { modo: "CS", valor: cs },
        { modo, valor: dinheiro },
      ].filter((p) => p.valor > 0);
      const d = await call<DocCompleto>("documentos.emitir", { tipo, entidade_id: entidade?.id, liquidar, pagamentos });
      toast.success(`${d.documento.numero_doc} emitido.`);
      qc.invalidateQueries();
      setEmitido(d.documento.id);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title={fornecedor ? "Pagamento a Fornecedor" : "Recibo de Cliente"}
        subtitle={fornecedor ? "Liquidação de facturas de fornecedores em dívida" : "Liquidação de facturas de clientes em dívida"}
        actions={
          <Button variant="ghost" onClick={() => nav(-1)}>
            <ArrowLeft className="size-4" /> Voltar
          </Button>
        }
      />
      <Card className="p-5">
        <div className="grid grid-cols-3 gap-4">
          <EntidadePicker tipo={fornecedor ? "F" : "C"} valor={entidade} onChange={setEntidade} />
          <Field label="Modo de pagamento">
            <Select value={modo} onChange={(e) => setModo(e.target.value)}>
              {t?.modos_pagamento
                .filter((m) => m.codigo !== "CS")
                .map((m) => (
                  <option key={m.codigo} value={m.codigo}>
                    {m.descricao}
                  </option>
                ))}
            </Select>
          </Field>
          {entidade && (
            <div className="self-end pb-2 text-ink-2">
              Saldo em dívida: <b className="num text-ink">{kz(divida)}</b>
            </div>
          )}
        </div>
        {creditoTotal > 0 && (
          <div className="mt-4 flex flex-wrap items-end gap-4 rounded-xl border border-ok/30 bg-ok-50 px-4 py-3">
            <div className="min-w-0 flex-1">
              <div className="font-medium text-ok">Crédito disponível {kz(creditoTotal)}</div>
              <div className="text-[12.5px] text-ink-2">
                {creditos.map((c) => `${c.numero_doc} (${kz(c.credito_disponivel)})`).join(" · ")} — {fornecedor ? "notas de crédito do fornecedor" : "adiantamentos e notas de crédito"} por utilizar.
              </div>
            </div>
            <Field label="Compensar neste recibo">
              <div className="flex gap-1">
                <Input className="num h-9 w-36 text-right" value={compensar} placeholder="0,00" onChange={(e) => setCompensar(e.target.value)} />
                <Button size="sm" variant="secondary" className="h-9" disabled={total <= 0} onClick={() => setCompensar(toInputKz(Math.min(creditoTotal, total)))}>
                  Máximo
                </Button>
              </div>
            </Field>
          </div>
        )}
      </Card>

      {entidade && (
        <Card className="mt-4">
          <Table>
            <thead>
              <tr>
                <Th>Documento de origem</Th>
                <Th>Emissão</Th>
                <Th>Vencimento</Th>
                <Th className="text-right">Total</Th>
                <Th className="text-right">Pendente</Th>
                <Th className="w-48 text-right">{fornecedor ? "Valor a pagar" : "Valor a receber"}</Th>
              </tr>
            </thead>
            <tbody>
              {pendentes.map((p) => (
                <tr key={p.id}>
                  <Td>
                    <div className="font-medium">{p.numero_doc}</div>
                    <div className="text-[12px] text-ink-3">
                      {p.tipo_desc}
                      {p.doc_externo ? ` · ${p.doc_externo}` : ""}
                    </div>
                  </Td>
                  <Td>{data(p.data_emissao)}</Td>
                  <Td>
                    {data(p.data_vencimento)} {p.dias_atraso > 0 && <Badge tone="danger" className="ml-1">{p.dias_atraso} dias</Badge>}
                  </Td>
                  <Td className="num text-right">{kz(p.total)}</Td>
                  <Td className="num text-right font-medium">{kz(p.valor_pendente)}</Td>
                  <Td>
                    <div className="flex gap-1">
                      <Input className="num h-8 text-right" value={valores[p.id] ?? ""} placeholder="0,00" onChange={(e) => setValores({ ...valores, [p.id]: e.target.value })} />
                      <Button size="sm" variant="secondary" onClick={() => setValores({ ...valores, [p.id]: toInputKz(p.valor_pendente) })}>
                        Tudo
                      </Button>
                    </div>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
          {!pendentes.length && <p className="py-10 text-center text-ink-3">Não há documentos em dívida para esta entidade.</p>}
          <div className="flex items-center justify-end gap-6 border-t border-line px-5 py-4">
            {cs > 0 && (
              <span className="text-[13px] text-ink-2">
                Compensação {kz(cs)} · {t?.modos_pagamento.find((m) => m.codigo === modo)?.descricao ?? modo} {kz(dinheiro)}
              </span>
            )}
            <span className="text-ink-2">
              Total {fornecedor ? "a pagar" : "do recibo"}: <b className="num text-[20px] text-ink">{kz(total)}</b>
            </span>
            <Button size="lg" loading={busy} disabled={total <= 0} onClick={emitir}>
              {fornecedor ? "Gravar pagamento" : "Emitir recibo"}
            </Button>
          </div>
        </Card>
      )}
      <DocumentoModal id={emitido} onClose={() => { setEmitido(null); nav(`/documentos?grupo=${fornecedor ? "COMPRAS" : "VENDAS"}`); }} />
    </>
  );
}

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ArrowLeft, Info } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { kz, parseKz } from "@/lib/format";
import type { DocCompleto, Entidade } from "@/lib/types";
import { Button, Card, Field, Input, PageHeader, Select } from "@/components/ui";
import DocumentoModal from "@/components/DocumentoModal";
import { EntidadePicker } from "@/components/Pickers";

/** Adiantamento de Cliente (Fr_AdiantamentoClientes): recibo de adiantamento que fica como crédito do cliente. */
export default function Adiantamento() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const t = useTabelas().data;
  const [entidade, setEntidade] = useState<Entidade | null>(null);
  const [valor, setValor] = useState("");
  const [modo, setModo] = useState("NU");
  const [referencia, setReferencia] = useState("");
  const [obs, setObs] = useState("");
  const [credito, setCredito] = useState(0);
  const [busy, setBusy] = useState(false);
  const [emitido, setEmitido] = useState<number | null>(null);

  useEffect(() => {
    if (!entidade) return;
    call<{ credito_disponivel: number }[]>("contas.creditos", { entidade_id: entidade.id, grupo: "VENDAS" }).then((c) => setCredito(c.reduce((s, x) => s + x.credito_disponivel, 0)));
  }, [entidade]);

  async function gravar() {
    setBusy(true);
    try {
      const v = parseKz(valor);
      const d = await call<DocCompleto>("documentos.emitir", {
        tipo: "RA",
        entidade_id: entidade?.id,
        valor: v,
        referencia: referencia || null,
        observacoes: obs || null,
        pagamentos: [{ modo, valor: v }],
      });
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
        title="Adiantamento de Cliente"
        subtitle="Valor recebido antes da factura; fica como crédito do cliente para usar nos recibos"
        actions={
          <Button variant="ghost" onClick={() => nav(-1)}>
            <ArrowLeft className="size-4" /> Voltar
          </Button>
        }
      />
      <div className="grid grid-cols-[minmax(0,1fr)_330px] gap-4">
        <Card className="p-5">
          <div className="grid grid-cols-2 gap-4">
            <div className="col-span-2">
              <EntidadePicker tipo="C" valor={entidade} onChange={setEntidade} />
            </div>
            {entidade && (
              <div className="col-span-2 grid grid-cols-4 gap-4 rounded-xl border border-line px-4 py-3 text-[13px]">
                <Info2 k="Nome" v={entidade.nome} />
                <Info2 k="Contribuinte" v={entidade.nif} />
                <Info2 k="Morada" v={[entidade.endereco, entidade.localidade].filter(Boolean).join(", ") || "—"} />
                <Info2 k="Crédito já disponível" v={kz(credito)} />
              </div>
            )}
            <Field label="Valor recebido (Kz)">
              <Input className="num text-right text-[18px]" value={valor} onChange={(e) => setValor(e.target.value)} />
            </Field>
            <Field label="Tipo de pagamento">
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
            <Field label="Referência">
              <Input value={referencia} placeholder="Encomenda, obra, contrato…" onChange={(e) => setReferencia(e.target.value)} />
            </Field>
            <Field label="Observação">
              <Input value={obs} onChange={(e) => setObs(e.target.value)} />
            </Field>
          </div>
        </Card>
        <Card className="h-fit p-5">
          <div className="flex items-baseline justify-between">
            <span className="font-medium">Valor total</span>
            <span className="num text-[26px] font-semibold tracking-tight">{kz(parseKz(valor || "0"))}</span>
          </div>
          <Button size="lg" className="mt-5 w-full" loading={busy} disabled={!entidade || entidade.id === 1 || parseKz(valor || "0") <= 0} onClick={gravar}>
            Gravar adiantamento
          </Button>
          <p className="mt-3 flex gap-2 text-[12px] leading-relaxed text-ink-3">
            <Info className="mt-0.5 size-4 shrink-0" />
            O recibo de adiantamento é um documento interno. Quando facturar, use «Compensar» no recibo do cliente para abater este valor. Se o adiantamento já corresponde a bens ou serviços definidos, o
            Código do IVA pode exigir factura no momento do recebimento (confirme com o seu contabilista); nesse caso use uma Factura/Recibo.
          </p>
        </Card>
      </div>
      <DocumentoModal
        id={emitido}
        onClose={() => {
          setEmitido(null);
          nav("/documentos?grupo=VENDAS");
        }}
      />
    </>
  );
}

function Info2({ k, v }: { k: string; v: string }) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] uppercase tracking-wide text-ink-3">{k}</div>
      <div className="truncate font-medium">{v}</div>
    </div>
  );
}

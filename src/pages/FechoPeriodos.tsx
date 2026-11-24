// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Fecho de períodos (services/periodos.rs): o administrador fecha um mês que já terminou e, a partir daí, nada com data
// desse mês se lança nem se anula. Os documentos corrigem-se com nota de crédito. Reabrir pede um motivo. Fechar e
// reabrir ficam na auditoria.
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CalendarCheck, Lock, LockOpen } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora, numero } from "@/lib/format";
import { Badge, Button, Card, Confirmar, Empty, Field, Input, LinhasCarregando, Modal, PageHeader, Table, Td, Th } from "@/components/ui";

interface Periodo {
  mes: string;
  nome: string;
  estado: "fechado" | "aberto" | "em_curso";
  fechado_em: string | null;
  fechado_por: string | null;
  documentos: number;
  movimentos: number;
}

const maiuscula = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export default function FechoPeriodos() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["periodos"], queryFn: () => call<Periodo[]>("periodos.listar") });
  const [aFechar, setAFechar] = useState<Periodo | null>(null);
  const [aReabrir, setAReabrir] = useState<Periodo | null>(null);
  const actualizar = () => qc.invalidateQueries({ queryKey: ["periodos"] });

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader title="Fecho de períodos" subtitle="Feche os meses já conferidos: nada com data de um mês fechado se lança nem se anula." />

      <Card className="mb-4 p-5 text-[13.5px] text-ink-2">
        <p className="mb-2 font-medium text-ink">Com um mês fechado:</p>
        <ul className="grid gap-x-6 gap-y-1 sm:grid-cols-2">
          <li>• os documentos desse mês já não se anulam (corrigem-se com nota de crédito);</li>
          <li>• não se lançam nem anulam movimentos de tesouraria com data desse mês;</li>
          <li>• não se registam compras com a data do fornecedor nesse mês;</li>
          <li>• as regularizações de conta corrente desse mês não se anulam.</li>
        </ul>
        <p className="mt-2 text-[12.5px] text-ink-3">
          As vendas, os recibos, os vales e as transferências têm sempre a data de hoje. Só se fecha um mês que já terminou; reabrir pede um motivo, e os dois ficam
          no registo de auditoria.
        </p>
      </Card>

      <Card className="overflow-hidden">
        <Table className="tabela-compacta">
          <thead>
            <tr>
              <Th className="w-[22%]">Mês</Th>
              <Th className="w-[13%]">Estado</Th>
              <Th className="w-[13%] text-right">Documentos</Th>
              <Th className="w-[13%] text-right">Movimentos</Th>
              <Th>Fechado</Th>
              <Th className="w-0" />
            </tr>
          </thead>
          <tbody>
            {q.isLoading && <LinhasCarregando colunas={6} />}
            {q.data?.map((p) => (
              <tr key={p.mes} className="hover:bg-sunken/60">
                <Td className="font-medium text-ink">{maiuscula(p.nome)}</Td>
                <Td>
                  {p.estado === "fechado" ? (
                    <Badge tone="neutral">
                      <Lock className="mr-1 size-3" /> Fechado
                    </Badge>
                  ) : p.estado === "em_curso" ? (
                    <Badge tone="brand">Em curso</Badge>
                  ) : (
                    <Badge tone="ok">Aberto</Badge>
                  )}
                </Td>
                <Td className="num text-right">{numero(p.documentos, 0)}</Td>
                <Td className="num text-right">{numero(p.movimentos, 0)}</Td>
                <Td className="text-[12.5px] text-ink-3">{p.fechado_em ? `${dataHora(p.fechado_em)}${p.fechado_por ? ` · ${p.fechado_por}` : ""}` : "—"}</Td>
                <Td className="text-right">
                  {p.estado === "fechado" ? (
                    <Button size="sm" variant="ghost" onClick={() => setAReabrir(p)}>
                      <LockOpen className="size-4" /> Reabrir
                    </Button>
                  ) : p.estado === "aberto" ? (
                    <Button size="sm" variant="secondary" onClick={() => setAFechar(p)}>
                      <Lock className="size-4" /> Fechar
                    </Button>
                  ) : null}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
        {q.isError && <Empty icon={<CalendarCheck className="size-6" />} title="Não foi possível ler os períodos" text={(q.error as Error).message} />}
      </Card>

      {aFechar && (
        <Confirmar
          titulo={`Fechar ${aFechar.nome}?`}
          confirmar="Fechar o mês"
          perigo={false}
          onClose={() => setAFechar(null)}
          onConfirmar={async () => {
            try {
              await call("periodos.fechar", { mes: aFechar.mes });
              toast.success(`${maiuscula(aFechar.nome)} fechado.`);
              setAFechar(null);
              await actualizar();
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
        >
          <p>
            Depois de fechado, nada com data de {aFechar.nome} se lança nem se anula: os documentos corrigem-se com nota de crédito. Pode reabri-lo mais tarde, com
            um motivo.
          </p>
          <p className="text-[12.5px] text-ink-3">
            {numero(aFechar.documentos, 0)} documentos e {numero(aFechar.movimentos, 0)} movimentos de tesouraria neste mês.
          </p>
        </Confirmar>
      )}
      {aReabrir && <Reabrir periodo={aReabrir} onClose={() => setAReabrir(null)} onFeito={actualizar} />}
    </div>
  );
}

function Reabrir({ periodo, onClose, onFeito }: { periodo: Periodo; onClose: () => void; onFeito: () => Promise<unknown> }) {
  const [motivo, setMotivo] = useState("");
  const [busy, setBusy] = useState(false);
  const curto = motivo.trim().length < 5;
  async function reabrir() {
    setBusy(true);
    try {
      await call("periodos.reabrir", { mes: periodo.mes, motivo: motivo.trim() });
      toast.success(`${maiuscula(periodo.nome)} reaberto.`);
      onClose();
      await onFeito();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={`Reabrir ${periodo.nome}`}
      width="max-w-md"
      sujo={!!motivo.trim()}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Voltar
          </Button>
          <Button variant="danger" loading={busy} disabled={curto} onClick={reabrir}>
            <LockOpen className="size-4" /> Reabrir
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-[14px] text-ink-2">
        <p>Com o mês reaberto, volta a poder lançar e anular com data dele. O motivo fica no registo de auditoria.</p>
        <Field label="Motivo" hint="Pelo menos 5 caracteres.">
          <Input autoFocus value={motivo} maxLength={200} onChange={(e) => setMotivo(e.target.value)} placeholder="Ex.: falta lançar a renda do armazém" />
        </Field>
      </div>
    </Modal>
  );
}

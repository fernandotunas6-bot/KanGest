// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Regras de aprovação (services/aprovacoes.rs): o administrador liga as operações que precisam do PIN de quem pode
// aprovar, e o valor a partir do qual. Desligadas, tudo funciona como antes. Em baixo, as aprovações dadas.
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { toast } from "sonner";
import { Banknote, FileX2, Receipt, ShieldCheck, Wallet } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora, hojeISO, inicioMesISO, kz, parseKz, toInputKz } from "@/lib/format";
import { Badge, Button, Card, Empty, Field, Input, LinhasCarregando, PageHeader, Table, Td, Th, cn } from "@/components/ui";
import { Interruptor } from "@/components/Acesso";

interface Regra {
  regra: "anular" | "saidas" | "vales" | "creditos";
  permissao: string;
  ativo: boolean;
  valor_minimo: number;
  so_dias_anteriores: boolean;
  aprovadores: { nome: string; tem_pin: boolean }[];
}
interface Aprovacao {
  id: number;
  regra: Regra["regra"];
  operacao: string;
  referencia: string | null;
  valor: number;
  criado_em: string;
  pedido_por: string | null;
  aprovado_por: string | null;
}

const INFO: Record<Regra["regra"], { titulo: string; texto: string; permissao: string; icon: LucideIcon }> = {
  anular: { titulo: "Anular documentos", texto: "Facturas, recibos e outros documentos anulados.", permissao: "Aprovar anulações de documentos", icon: FileX2 },
  saidas: {
    titulo: "Saídas de tesouraria",
    texto: "Pagamentos e saídas na tesouraria, pagamentos a fornecedores e devoluções de dinheiro a clientes.",
    permissao: "Aprovar saídas de tesouraria, pagamentos e devoluções",
    icon: Banknote,
  },
  vales: { titulo: "Vales de caixa", texto: "Dinheiro tirado da gaveta do POS.", permissao: "Aprovar vales de caixa", icon: Wallet },
  creditos: {
    titulo: "Notas de crédito e regularizações",
    texto: "Notas de crédito emitidas e regularizações de conta corrente (perdões e acertos).",
    permissao: "Aprovar notas de crédito e regularizações",
    icon: Receipt,
  },
};

export default function RegrasAprovacao() {
  const regras = useQuery({ queryKey: ["aprovacoes-regras"], queryFn: () => call<Regra[]>("aprovacoes.regras") });
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const dadas = useQuery({ queryKey: ["aprovacoes", de, ate], queryFn: () => call<Aprovacao[]>("aprovacoes.listar", { de, ate }) });

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader
        title="Regras de aprovação"
        subtitle="Ligue as operações que precisam do PIN de quem pode aprovar. Desligadas, tudo funciona como antes."
      />
      <div className="mb-6 grid gap-4 lg:grid-cols-2">
        {regras.isLoading &&
          [0, 1, 2, 3].map((i) => (
            <Card key={i} className="h-48 animate-pulse">
              <span />
            </Card>
          ))}
        {regras.data?.map((r) => <CartaoRegra key={r.regra} regra={r} />)}
      </div>

      <Card className="overflow-hidden">
        <div className="flex flex-wrap items-end gap-3 border-b border-line px-5 py-3">
          <div className="mr-auto">
            <div className="font-semibold">Aprovações dadas</div>
            <div className="text-[12.5px] text-ink-3">Quem pediu, quem aprovou com o PIN, o quê e quanto.</div>
          </div>
          <Field label="De" className="w-40">
            <Input type="date" value={de} onChange={(e) => setDe(e.target.value)} />
          </Field>
          <Field label="Até" className="w-40">
            <Input type="date" value={ate} onChange={(e) => setAte(e.target.value)} />
          </Field>
        </div>
        <Table className="tabela-compacta">
          <thead>
            <tr>
              <Th>Data</Th>
              <Th>Operação</Th>
              <Th>Documento ou movimento</Th>
              <Th className="text-right">Valor</Th>
              <Th>Pedido por</Th>
              <Th>Aprovado por</Th>
            </tr>
          </thead>
          <tbody>
            {dadas.isLoading && <LinhasCarregando colunas={6} />}
            {dadas.data?.map((a) => (
              <tr key={a.id}>
                <Td className="whitespace-nowrap">{dataHora(a.criado_em)}</Td>
                <Td>
                  <div>{a.operacao}</div>
                  <div className="text-[12px] text-ink-3">{INFO[a.regra]?.titulo}</div>
                </Td>
                <Td className="text-ink-2">{a.referencia ?? "—"}</Td>
                <Td className="num whitespace-nowrap text-right">{kz(a.valor)}</Td>
                <Td>{a.pedido_por ?? "—"}</Td>
                <Td className={cn(a.pedido_por === a.aprovado_por && "text-ink-2")}>
                  {a.aprovado_por ?? "—"}
                  {a.pedido_por === a.aprovado_por && <span className="ml-1 text-[12px] text-ink-3">(o próprio)</span>}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
        {dadas.data && dadas.data.length === 0 && <Empty icon={<ShieldCheck className="size-6" />} title="Nenhuma aprovação neste período" />}
      </Card>
    </div>
  );
}

function CartaoRegra({ regra: r }: { regra: Regra }) {
  const qc = useQueryClient();
  const info = INFO[r.regra];
  const [ativo, setAtivo] = useState(r.ativo);
  const [valor, setValor] = useState(r.valor_minimo ? toInputKz(r.valor_minimo) : "");
  const [anteriores, setAnteriores] = useState(r.so_dias_anteriores);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setAtivo(r.ativo);
    setValor(r.valor_minimo ? toInputKz(r.valor_minimo) : "");
    setAnteriores(r.so_dias_anteriores);
  }, [r]);
  const minimo = parseKz(valor);
  const mudou = ativo !== r.ativo || minimo !== r.valor_minimo || anteriores !== r.so_dias_anteriores;
  const comPin = r.aprovadores.filter((a) => a.tem_pin);
  const semPin = r.aprovadores.filter((a) => !a.tem_pin);
  const Icone = info.icon;

  async function guardar() {
    setBusy(true);
    try {
      await call("aprovacoes.guardar", { regra: r.regra, ativo, valor_minimo: minimo, so_dias_anteriores: anteriores });
      toast.success(ativo ? `«${info.titulo}» passa a pedir o PIN.` : `«${info.titulo}» já não pede o PIN.`);
      await qc.invalidateQueries({ queryKey: ["aprovacoes-regras"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className={cn("flex flex-col p-5", ativo && "border-brand/40")}>
      <div className="mb-3 flex items-start gap-3">
        <div className={cn("grid size-9 shrink-0 place-items-center rounded-lg", ativo ? "bg-brand-50 text-brand" : "bg-sunken text-ink-3")}>
          <Icone className="size-[18px]" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="font-semibold text-ink">{info.titulo}</h3>
            {r.ativo ? <Badge tone="brand">Pede o PIN</Badge> : <Badge tone="neutral">Desligada</Badge>}
          </div>
          <p className="mt-0.5 text-[13px] text-ink-3">{info.texto}</p>
        </div>
        <Interruptor ligado={ativo} onChange={setAtivo} rotulo="" dica={ativo ? "Desligar" : "Ligar"} />
      </div>

      <div className={cn("grid gap-3 sm:grid-cols-2", !ativo && "opacity-60")}>
        <Field label="A partir de (Kz)" hint="Vazio: qualquer valor">
          <Input inputMode="decimal" className="num" placeholder="Qualquer valor" value={valor} disabled={!ativo} onChange={(e) => setValor(e.target.value)} />
        </Field>
        {r.regra === "anular" && (
          <label className="flex items-center gap-2 self-center pt-4 text-[13.5px] text-ink-2">
            <input type="checkbox" className="size-4 accent-[var(--color-brand)]" checked={anteriores} disabled={!ativo} onChange={(e) => setAnteriores(e.target.checked)} />
            Só os documentos de dias anteriores
          </label>
        )}
      </div>

      <div className="mt-3 text-[12.5px] text-ink-3">
        <span className="text-ink-2">Quem pode aprovar: </span>
        {comPin.length ? comPin.map((a) => a.nome).join(", ") : <span className="text-danger">ninguém com PIN</span>}
        {semPin.length > 0 && <span> · sem PIN: {semPin.map((a) => a.nome).join(", ")}</span>}
        <div>
          Permissão «{info.permissao}», em{" "}
          <Link to="/utilizadores" className="text-accent-ink underline-offset-2 hover:underline">
            Utilizadores e Permissões
          </Link>
          .
        </div>
      </div>

      <div className="mt-4 flex justify-end">
        <Button size="sm" loading={busy} disabled={!mudou} onClick={guardar}>
          Guardar
        </Button>
      </div>
    </Card>
  );
}

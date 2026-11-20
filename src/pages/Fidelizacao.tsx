// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Star } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, kz, kzNum, numero, parseKz } from "@/lib/format";
import { exportarExcel } from "@/lib/csv";
import { dataXL, kzXL } from "@/lib/xlsx";
import type { FidelizacaoCfg } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Table, Td, Th } from "@/components/ui";

interface ClientePontos {
  id: number;
  nome: string;
  nif: string;
  telefone: string | null;
  saldo: number;
  ganhos: number;
  gastos: number;
  compras: number;
  ultimo: string;
}

interface Movimento {
  id: number;
  data_hora: string;
  tipo: string;
  pontos: number;
  valor: number;
  descricao: string | null;
  numero_doc: string | null;
  utilizador: string | null;
}

const TIPOS: Record<string, string> = {
  GANHO: "Compra",
  RESGATE: "Pagamento",
  ESTORNO: "Nota de crédito",
  DEVOLUCAO: "Devolvidos",
  ANULACAO: "Anulação",
  AJUSTE: "Ajuste",
};

/** Cartão de pontos: regras, saldos dos clientes, histórico e ajustes. */
export default function Fidelizacao() {
  const tem = useSession((s) => s.tem);
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const cfg = useQuery({ queryKey: ["fidelizacao-cfg"], queryFn: () => call<FidelizacaoCfg>("fidelizacao.config") });
  const lista = useQuery({ queryKey: ["fidelizacao-clientes", dq], queryFn: () => call<ClientePontos[]>("fidelizacao.clientes", { q: dq }) });
  const [aberto, setAberto] = useState<ClientePontos | null>(null);
  const valorPonto = cfg.data?.valor_ponto ?? 0;

  function exportar() {
    exportarExcel(
      "fidelizacao-clientes.xlsx",
      [
        ["Cliente", "NIF", "Telefone", "Saldo (pontos)", "Valor do saldo", "Pontos ganhos", "Pontos usados", "Compras com pontos", "Último movimento"],
        ...(lista.data ?? []).map((c) => [c.nome, c.nif, c.telefone ?? "", c.saldo, kzXL(Math.max(0, c.saldo) * valorPonto), c.ganhos, c.gastos, kzXL(c.compras), dataXL(c.ultimo?.slice(0, 10))]),
      ],
      { titulo: "Fidelização de clientes — saldos de pontos" },
    );
  }

  return (
    <>
      <PageHeader
        title="Fidelização de Clientes"
        subtitle="Cartão de pontos: os clientes identificados ganham pontos nas compras e pagam com eles no POS"
        actions={
          <Button variant="secondary" onClick={exportar} disabled={!lista.data?.length}>
            Exportar Excel
          </Button>
        }
      />
      {cfg.data && <Regras cfg={cfg.data} editavel={tem("sistema.configuracoes")} />}
      <Card>
        <div className="flex flex-wrap items-center justify-between gap-3 px-5 pt-4">
          <div className="font-semibold">Saldos dos clientes</div>
          <Input className="w-72" placeholder="Procurar por nome ou NIF…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        {lista.data && lista.data.length === 0 ? (
          <Empty icon={<Star className="size-6" />} title="Ainda sem pontos" text="Os pontos aparecem aqui quando um cliente identificado comprar com a fidelização activa." />
        ) : (
          <Table>
            <thead>
              <tr>
                <Th>Cliente</Th>
                <Th className="text-right">Saldo</Th>
                <Th className="text-right">Vale</Th>
                <Th className="text-right">Ganhos</Th>
                <Th className="text-right">Usados</Th>
                <Th className="text-right">Compras</Th>
                <Th>Último movimento</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((c) => (
                <tr key={c.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setAberto(c)}>
                  <Td>
                    <div className="font-medium">{c.nome}</div>
                    <div className="text-[12px] text-ink-3">
                      NIF {c.nif}
                      {c.telefone ? ` · ${c.telefone}` : ""}
                    </div>
                  </Td>
                  <Td className="num text-right font-semibold">{numero(c.saldo, 0)}</Td>
                  <Td className="num text-right text-ink-2">{kz(Math.max(0, c.saldo) * valorPonto)}</Td>
                  <Td className="num text-right">{numero(c.ganhos, 0)}</Td>
                  <Td className="num text-right">{numero(c.gastos, 0)}</Td>
                  <Td className="num text-right">{kz(c.compras)}</Td>
                  <Td className="text-ink-2">{data(c.ultimo)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {aberto && <Historico cliente={aberto} valorPonto={valorPonto} ajustar={tem("sistema.configuracoes")} onClose={() => setAberto(null)} />}
    </>
  );
}

function Regras({ cfg, editavel }: { cfg: FidelizacaoCfg; editavel: boolean }) {
  const qc = useQueryClient();
  const [ativa, setAtiva] = useState(cfg.ativa);
  const [compra, setCompra] = useState(kzNum(cfg.compra_por_ponto));
  const [valor, setValor] = useState(kzNum(cfg.valor_ponto));
  const [minimo, setMinimo] = useState(String(cfg.minimo_resgate));
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setAtiva(cfg.ativa);
    setCompra(kzNum(cfg.compra_por_ponto));
    setValor(kzNum(cfg.valor_ponto));
    setMinimo(String(cfg.minimo_resgate));
  }, [cfg]);
  const c = parseKz(compra || "0");
  const v = parseKz(valor || "0");
  const retorno = c > 0 ? (v / c) * 100 : 0;
  async function gravar() {
    setBusy(true);
    try {
      await call("fidelizacao.configGuardar", { ativa, compra_por_ponto: c, valor_ponto: v, minimo_resgate: Number(minimo) || 0 });
      toast.success(ativa ? "Fidelização activa." : "Regras gravadas (fidelização desactivada).");
      qc.invalidateQueries({ queryKey: ["fidelizacao-cfg"] });
      qc.invalidateQueries({ queryKey: ["tabelas"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card className="mb-4 p-5">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2 font-semibold">
          Regras {cfg.ativa ? <Badge tone="ok">Activa</Badge> : <Badge>Desactivada</Badge>}
        </div>
        <label className="flex items-center gap-2 text-[13.5px]">
          <input type="checkbox" checked={ativa} disabled={!editavel} onChange={(e) => setAtiva(e.target.checked)} />
          Fidelização activa
        </label>
      </div>
      <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] items-end gap-4">
        <Field label="1 ponto por cada (Kz de compra)">
          <Input className="num" value={compra} disabled={!editavel} onChange={(e) => setCompra(e.target.value)} />
        </Field>
        <Field label="Cada ponto vale (Kz)">
          <Input className="num" value={valor} disabled={!editavel} onChange={(e) => setValor(e.target.value)} />
        </Field>
        <Field label="Pagar com pontos a partir de (pontos)">
          <Input className="num" type="number" min={1} value={minimo} disabled={!editavel} onChange={(e) => setMinimo(e.target.value)} />
        </Field>
        {editavel && (
          <Button loading={busy} onClick={gravar}>
            Gravar regras
          </Button>
        )}
      </div>
      <p className="mt-3 text-[12.5px] text-ink-3">
        O cliente recebe {numero(retorno, 2)}% do que paga em pontos. O que paga com pontos não dá pontos. No documento os pontos são um meio de pagamento
        («DE» no SAF-T), não um desconto: o total e o IVA não mudam. As notas de crédito retiram os pontos ganhos e devolvem em pontos o que foi pago com pontos;
        anular um documento reverte os seus pontos.
      </p>
    </Card>
  );
}

function Historico({ cliente, valorPonto, ajustar, onClose }: { cliente: ClientePontos; valorPonto: number; ajustar: boolean; onClose: () => void }) {
  const qc = useQueryClient();
  const movs = useQuery({ queryKey: ["fidelizacao-movs", cliente.id], queryFn: () => call<Movimento[]>("fidelizacao.movimentos", { entidade_id: cliente.id }) });
  const [pontos, setPontos] = useState("");
  const [motivo, setMotivo] = useState("");
  const saldo = (movs.data ?? []).reduce((s, m) => s + m.pontos, 0);
  async function gravar() {
    try {
      const r = await call<{ saldo: number }>("fidelizacao.ajustar", { entidade_id: cliente.id, pontos: Number(pontos) || 0, motivo });
      toast.success(`Novo saldo: ${numero(r.saldo, 0)} pontos.`);
      setPontos("");
      setMotivo("");
      qc.invalidateQueries({ queryKey: ["fidelizacao-movs", cliente.id] });
      qc.invalidateQueries({ queryKey: ["fidelizacao-clientes"] });
      qc.invalidateQueries({ queryKey: ["pontos"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title={`Pontos de ${cliente.nome}`} width="max-w-3xl">
      <div className="mb-4 flex items-baseline gap-3">
        <span className="num text-[28px] font-semibold">{numero(saldo, 0)}</span>
        <span className="text-ink-2">pontos · valem {kz(Math.max(0, saldo) * valorPonto)}</span>
      </div>
      {ajustar && (
        <div className="mb-4 grid grid-cols-[140px_1fr_auto] items-end gap-3 rounded-xl border border-line p-3">
          <Field label="Pontos (+ ou −)">
            <Input className="num" type="number" value={pontos} onChange={(e) => setPontos(e.target.value)} placeholder="ex.: 100 ou -20" />
          </Field>
          <Field label="Motivo">
            <Input value={motivo} onChange={(e) => setMotivo(e.target.value)} placeholder="Campanha, correcção…" />
          </Field>
          <Button variant="secondary" onClick={gravar} disabled={!Number(pontos) || motivo.trim().length < 3}>
            Ajustar
          </Button>
        </div>
      )}
      <Table className="max-h-[50vh]">
        <thead>
          <tr>
            <Th>Data</Th>
            <Th>Movimento</Th>
            <Th>Documento</Th>
            <Th className="text-right">Valor</Th>
            <Th className="text-right">Pontos</Th>
          </tr>
        </thead>
        <tbody>
          {movs.data?.map((m) => (
            <tr key={m.id}>
              <Td className="text-ink-2">{data(m.data_hora)}</Td>
              <Td>
                <div>{TIPOS[m.tipo] ?? m.tipo}</div>
                {m.tipo === "AJUSTE" && <div className="text-[12px] text-ink-3">{m.descricao} · {m.utilizador}</div>}
              </Td>
              <Td className="font-mono text-[12px]">{m.numero_doc ?? "—"}</Td>
              <Td className="num text-right text-ink-2">{m.valor ? kz(m.valor) : ""}</Td>
              <Td className={`num text-right font-medium ${m.pontos < 0 ? "text-danger" : "text-ok"}`}>
                {m.pontos > 0 ? "+" : ""}
                {numero(m.pontos, 0)}
              </Td>
            </tr>
          ))}
        </tbody>
      </Table>
    </Modal>
  );
}

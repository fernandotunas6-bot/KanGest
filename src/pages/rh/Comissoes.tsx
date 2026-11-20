// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { Fragment, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { BadgePercent, ChevronDown, ChevronRight, Send } from "lucide-react";
import { call } from "@/lib/api";
import { hojeISO, inicioMesISO, kz, pct } from "@/lib/format";
import { useSession } from "@/store/session";
import { InputData } from "@/components/InputData";
import { Button, Card, Input, PageHeader, Table, Td, Th } from "@/components/ui";

interface Taxas {
  vendedores: { id: number; nome: string; numero: string | null; pct: number }[];
  familias: { id: number; descricao: string; pct: number | null }[];
}

interface Relatorio {
  de: string;
  ate: string;
  base: number;
  total: number;
  vendedores: {
    vendedor_id: number;
    vendedor: string;
    pct: number;
    base: number;
    comissao: number;
    documentos: number;
    familias: { familia_id: number | null; familia: string; base: number; pct: number; comissao: number }[];
  }[];
}

const MESES = ["Janeiro", "Fevereiro", "Março", "Abril", "Maio", "Junho", "Julho", "Agosto", "Setembro", "Outubro", "Novembro", "Dezembro"];

/** Percentagem editável: grava ao sair do campo (vazio = sem taxa própria). */
function CampoPct({ valor, vazio, onGravar, disabled }: { valor: number | null; vazio: string; onGravar: (v: number | null) => void; disabled?: boolean }) {
  const [texto, setTexto] = useState(valor == null ? "" : String(valor).replace(".", ","));
  const gravar = () => {
    const t = texto.trim().replace(",", ".");
    const v = t === "" ? null : Number(t);
    if (v != null && (!Number.isFinite(v) || v < 0 || v > 100)) return toast.error("A comissão é uma percentagem entre 0 e 100.");
    if (v !== valor) onGravar(v);
  };
  return (
    <div className="flex items-center justify-end gap-1">
      <Input
        className="h-8 w-20 text-right"
        inputMode="decimal"
        placeholder={vazio}
        value={texto}
        disabled={disabled}
        onChange={(e) => setTexto(e.target.value)}
        onBlur={gravar}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
      <span className="text-ink-3">%</span>
    </div>
  );
}

export default function Comissoes() {
  const { tem } = useSession();
  const gerir = tem("rh.processamento");
  const qc = useQueryClient();
  const [de, setDe] = useState(inicioMesISO());
  const [ate, setAte] = useState(hojeISO());
  const [aberto, setAberto] = useState<number | null>(null);
  const [lancando, setLancando] = useState(false);
  const taxas = useQuery({ queryKey: ["comissoes-taxas"], queryFn: () => call<Taxas>("comissoes.taxas") });
  const rel = useQuery({ queryKey: ["comissoes", de, ate], queryFn: () => call<Relatorio>("comissoes.relatorio", { de, ate }), enabled: !!de && !!ate });

  async function guardarTaxa(tipo: "VENDEDOR" | "FAMILIA", id: number, valor: number | null) {
    try {
      await call("comissoes.taxaGuardar", { tipo, id, pct: valor });
      qc.invalidateQueries({ queryKey: ["comissoes-taxas"] });
      qc.invalidateQueries({ queryKey: ["comissoes"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  // o lançamento nos salários é por mês: o do início do período
  const [ano, mes] = [Number(de.slice(0, 4)), Number(de.slice(5, 7))];
  const mesTexto = `${MESES[mes - 1] ?? ""} de ${ano}`;
  async function lancar() {
    setLancando(true);
    try {
      const r = await call<{ lancados: number; total: number }>("comissoes.lancar", { ano, mes });
      toast.success(`Comissões de ${mesTexto} lançadas nos salários.`, { description: `${r.lancados} vendedores · ${kz(r.total)}. Lançar de novo substitui estas.` });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setLancando(false);
    }
  }

  const r = rel.data;
  return (
    <>
      <PageHeader
        title="Comissões de Vendedores"
        subtitle="Percentagem sobre as vendas sem IVA de cada vendedor (o escolhido no documento); as notas de crédito descontam"
        actions={
          gerir && (
            <Button onClick={lancar} loading={lancando} title="Junta as comissões do mês às alterações do processamento salarial (rubrica 019, sujeita a INSS e IRT)">
              <Send className="size-4" /> Lançar nos salários de {mesTexto}
            </Button>
          )
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <InputData className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        <span className="text-ink-3">a</span>
        <InputData className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
      </div>

      <div className="grid gap-6 xl:grid-cols-[1fr_380px]">
        <Card>
          <Table>
            <thead>
              <tr>
                <Th>Vendedor</Th>
                <Th className="text-right">Documentos</Th>
                <Th className="text-right">Vendas sem IVA</Th>
                <Th className="text-right">Taxa</Th>
                <Th className="text-right">Comissão</Th>
              </tr>
            </thead>
            <tbody>
              {r?.vendedores.map((v) => (
                <Fragment key={v.vendedor_id}>
                  <tr className="cursor-pointer hover:bg-sunken" onClick={() => setAberto(aberto === v.vendedor_id ? null : v.vendedor_id)}>
                    <Td className="font-medium">
                      <span className="inline-flex items-center gap-1">
                        {aberto === v.vendedor_id ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
                        {v.vendedor}
                      </span>
                    </Td>
                    <Td className="num text-right">{v.documentos}</Td>
                    <Td className="num text-right">{kz(v.base)}</Td>
                    <Td className="num text-right text-ink-2">{v.pct > 0 ? pct(v.pct) : "sem comissão"}</Td>
                    <Td className="num text-right font-semibold">{kz(v.comissao)}</Td>
                  </tr>
                  {aberto === v.vendedor_id &&
                    v.familias.map((f) => (
                      <tr key={`${v.vendedor_id}-${f.familia_id}`} className="bg-sunken/60 text-[13px]">
                        <Td className="pl-10 text-ink-2">{f.familia}</Td>
                        <Td />
                        <Td className="num text-right text-ink-2">{kz(f.base)}</Td>
                        <Td className="num text-right text-ink-2">{pct(f.pct)}</Td>
                        <Td className="num text-right text-ink-2">{kz(f.comissao)}</Td>
                      </tr>
                    ))}
                </Fragment>
              ))}
            </tbody>
            {!!r?.vendedores.length && (
              <tfoot>
                <tr className="font-semibold">
                  <Td>Total</Td>
                  <Td />
                  <Td className="num text-right">{kz(r.base)}</Td>
                  <Td />
                  <Td className="num text-right">{kz(r.total)}</Td>
                </tr>
              </tfoot>
            )}
          </Table>
          {r && !r.vendedores.length && <p className="py-8 text-center text-ink-3">Sem vendas com vendedor neste período.</p>}
        </Card>

        <div className="grid content-start gap-6">
          <Card>
            <div className="flex items-center gap-2 px-5 pt-4 font-semibold">
              <BadgePercent className="size-4 text-brand" /> Taxa de cada vendedor
            </div>
            <p className="px-5 pb-2 text-[12.5px] text-ink-3">0 ou vazio: sem comissão.</p>
            <Table>
              <tbody>
                {taxas.data?.vendedores.map((v) => (
                  <tr key={v.id}>
                    <Td>{v.nome}</Td>
                    <Td className="py-1.5">
                      <CampoPct key={`v${v.id}-${v.pct}`} valor={v.pct || null} vazio="0" disabled={!gerir} onGravar={(x) => guardarTaxa("VENDEDOR", v.id, x)} />
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
            {taxas.data && !taxas.data.vendedores.length && <p className="px-5 pb-4 text-[13px] text-ink-3">Os vendedores são os funcionários activos (Recursos Humanos → Funcionários).</p>}
          </Card>
          <Card>
            <div className="px-5 pt-4 font-semibold">Taxa própria por família</div>
            <p className="px-5 pb-2 text-[12.5px] text-ink-3">Substitui a do vendedor nos artigos da família (só para quem tem comissão). Vazio: vale a do vendedor.</p>
            <div className="max-h-80 overflow-auto">
              <Table>
                <tbody>
                  {taxas.data?.familias.map((f) => (
                    <tr key={f.id}>
                      <Td>{f.descricao}</Td>
                      <Td className="py-1.5">
                        <CampoPct key={`f${f.id}-${f.pct}`} valor={f.pct} vazio="—" disabled={!gerir} onGravar={(x) => guardarTaxa("FAMILIA", f.id, x)} />
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </div>
          </Card>
        </div>
      </div>
    </>
  );
}

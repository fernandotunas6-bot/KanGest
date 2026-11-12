import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AlertTriangle, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { kz, parseKz, toInputKz } from "@/lib/format";
import type { Rubrica } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Field, Input, PageHeader, Table, Td, Th, cn } from "@/components/ui";
import { useTabelasRH } from "./Funcionarios";

interface EscalaoForm {
  key: string;
  de: string;
  ate: string;
  parcela_fixa: string;
  taxa: string;
  excesso: string;
}

const n = (s: string | null | undefined, padrao: number) => {
  const v = Number(String(s ?? "").replace(",", "."));
  return Number.isFinite(v) && String(s ?? "").trim() !== "" ? v : padrao;
};

/** Configuração salarial: taxas de INSS, tabela de IRT do Grupo A e incidência das rubricas. */
export default function ConfigRH() {
  const qc = useQueryClient();
  const admin = useSession((s) => s.tem("rh.configuracao"));
  const tabs = useTabelasRH().data;
  const [p, setP] = useState({ inss_trabalhador: "3", inss_empregador: "8", dias_uteis: "22", horas_dia: "8", acrescimo_horas_extra: "50", irt_independentes: "6.5" });
  const [confirmada, setConfirmada] = useState(false);
  const [escaloes, setEscaloes] = useState<EscalaoForm[]>([]);
  const [rubricas, setRubricas] = useState<(Rubrica & { isento: string })[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!tabs) return;
    const pr = tabs.parametros;
    setP({
      inss_trabalhador: pr.inss_trabalhador ?? "3",
      inss_empregador: pr.inss_empregador ?? "8",
      dias_uteis: pr.dias_uteis ?? "22",
      horas_dia: pr.horas_dia ?? "8",
      acrescimo_horas_extra: pr.acrescimo_horas_extra ?? "50",
      irt_independentes: pr.irt_independentes ?? "6.5",
    });
    setConfirmada(pr.tabela_confirmada);
    setEscaloes(
      tabs.escaloes.map((e) => ({
        key: crypto.randomUUID(),
        de: toInputKz(e.de),
        ate: e.ate === null ? "" : toInputKz(e.ate),
        parcela_fixa: toInputKz(e.parcela_fixa),
        taxa: String(e.taxa).replace(".", ","),
        excesso: toInputKz(e.excesso),
      })),
    );
    setRubricas(tabs.rubricas.map((r) => ({ ...r, isento: toInputKz(r.isento_irt_ate) })));
  }, [tabs]);

  async function guardar() {
    setBusy(true);
    try {
      await call("rh.configGuardar", {
        inss_trabalhador: n(p.inss_trabalhador, 3),
        inss_empregador: n(p.inss_empregador, 8),
        dias_uteis: n(p.dias_uteis, 22),
        horas_dia: n(p.horas_dia, 8),
        acrescimo_horas_extra: n(p.acrescimo_horas_extra, 50),
        irt_independentes: n(p.irt_independentes, 6.5),
        tabela_confirmada: confirmada,
        escaloes: escaloes.map((e) => ({
          de: parseKz(e.de),
          ate: e.ate.trim() ? parseKz(e.ate) : null,
          parcela_fixa: parseKz(e.parcela_fixa),
          taxa: n(e.taxa, 0),
          excesso: parseKz(e.excesso),
        })),
        rubricas: rubricas.map((r) => ({ codigo: r.codigo, incide_inss: !!r.incide_inss, incide_irt: !!r.incide_irt, isento_irt_ate: parseKz(r.isento) })),
      });
      toast.success("Configuração salarial guardada.");
      qc.invalidateQueries({ queryKey: ["rh-tabelas"] });
      qc.invalidateQueries({ queryKey: ["rh-simular"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const upd = (k: string, patch: Partial<EscalaoForm>) => setEscaloes((xs) => xs.map((x) => (x.key === k ? { ...x, ...patch } : x)));
  const campo = (k: keyof typeof p, label: string, hint?: string) => (
    <Field label={label} hint={hint}>
      <Input className="num text-right" disabled={!admin} value={p[k]} onChange={(e) => setP({ ...p, [k]: e.target.value })} />
    </Field>
  );

  return (
    <>
      <PageHeader
        title="Configuração Salarial"
        subtitle="INSS, tabela de IRT (Grupo A) e incidência das rubricas"
        actions={
          admin && (
            <Button loading={busy} onClick={guardar}>
              Guardar
            </Button>
          )
        }
      />
      {!admin && <p className="mb-4 rounded-lg bg-sunken px-4 py-2.5 text-ink-2">Só um administrador pode alterar estes valores.</p>}
      <div className={`mb-4 flex items-start gap-3 rounded-xl border px-4 py-3 text-[13px] ${confirmada ? "border-ok/30 bg-ok-50" : "border-accent/40 bg-accent-50"}`}>
        {confirmada ? <ShieldCheck className="mt-0.5 size-4 shrink-0 text-ok" /> : <AlertTriangle className="mt-0.5 size-4 shrink-0 text-[#9a6514]" />}
        <div className="flex-1">
          Valores pré-carregados: tabela de IRT do Grupo A da Lei n.º 14/25 (em vigor desde 2026, isenção até 150 000 Kz), INSS de 3% (trabalhador) e 8% (empresa), e subsídios de alimentação e de transporte isentos de IRT
          até 30 000 Kz cada. Confirme-os com a AGT, o INSS ou o seu contabilista; a lei muda com o Orçamento Geral do Estado.
          <label className="mt-2 flex items-center gap-2 font-medium">
            <input type="checkbox" disabled={!admin} checked={confirmada} onChange={(e) => setConfirmada(e.target.checked)} /> Confirmei que estes valores são os que estão em vigor
          </label>
        </div>
      </div>

      <div className="grid grid-cols-[280px_minmax(0,1fr)] gap-4">
        <Card className="h-fit p-5">
          <div className="mb-4 font-medium">Parâmetros</div>
          <div className="grid grid-cols-1 gap-4">
            {campo("inss_trabalhador", "INSS trabalhador (%)")}
            {campo("inss_empregador", "INSS empresa (%)")}
            {campo("dias_uteis", "Dias úteis por mês", "Valor/dia = salário base ÷ dias")}
            {campo("horas_dia", "Horas por dia", "Valor/hora = valor/dia ÷ horas")}
            {campo("acrescimo_horas_extra", "Acréscimo horas extra (%)", "Ex.: 50 = valor/hora × 1,5")}
            {campo("irt_independentes", "IRT conta própria (%)", "Retenção nos honorários (Grupo B)")}
          </div>
        </Card>

        <Card>
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <span className="font-medium">Tabela de IRT — Grupo A (valores mensais em Kz)</span>
            {admin && (
              <Button size="sm" variant="secondary" onClick={() => setEscaloes((xs) => [...xs, { key: crypto.randomUUID(), de: "", ate: "", parcela_fixa: "0,00", taxa: "0", excesso: "" }])}>
                <Plus className="size-4" /> Escalão
              </Button>
            )}
          </div>
          <Table>
            <thead>
              <tr>
                <Th>De</Th>
                <Th>Até</Th>
                <Th>Parcela fixa</Th>
                <Th>Taxa %</Th>
                <Th>Sobre o excesso de</Th>
                <Th className="w-10" />
              </tr>
            </thead>
            <tbody>
              {escaloes.map((e) => (
                <tr key={e.key}>
                  {(["de", "ate", "parcela_fixa", "taxa", "excesso"] as const).map((k) => (
                    <Td key={k} className="py-1.5">
                      <Input
                        className={cn("num h-8 text-right", k === "taxa" ? "min-w-[64px]" : "min-w-[112px]")}
                        disabled={!admin}
                        placeholder={k === "ate" ? "sem limite" : undefined}
                        value={e[k]}
                        onChange={(x) => upd(e.key, { [k]: x.target.value })}
                      />
                    </Td>
                  ))}
                  <Td className="py-1.5">
                    {admin && (
                      <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setEscaloes((xs) => xs.filter((x) => x.key !== e.key))}>
                        <Trash2 className="size-4" />
                      </button>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
          <p className="px-5 py-3 text-[12px] text-ink-3">IRT = parcela fixa + taxa × (matéria colectável − excesso). Matéria colectável = remunerações sujeitas − faltas − INSS do trabalhador.</p>
        </Card>
      </div>

      <Card className="mt-4">
        <div className="border-b border-line px-5 py-3 font-medium">Rubricas e incidência</div>
        <Table>
          <thead>
            <tr>
              <Th>Código</Th>
              <Th>Rubrica</Th>
              <Th>Tipo</Th>
              <Th className="text-center">Incide INSS</Th>
              <Th className="text-center">Incide IRT</Th>
              <Th className="text-right">Isento de IRT até (Kz/mês)</Th>
            </tr>
          </thead>
          <tbody>
            {rubricas.map((r) => (
              <tr key={r.codigo}>
                <Td className="font-mono text-[12.5px]">{r.codigo}</Td>
                <Td>{r.descricao}</Td>
                <Td>{r.tipo === "DESCONTO" ? <Badge tone="danger">Desconto</Badge> : <Badge tone="brand">Remuneração</Badge>}</Td>
                <Td className="text-center">
                  <input type="checkbox" disabled={!admin} checked={!!r.incide_inss} onChange={(e) => setRubricas((xs) => xs.map((x) => (x.codigo === r.codigo ? { ...x, incide_inss: e.target.checked ? 1 : 0 } : x)))} />
                </Td>
                <Td className="text-center">
                  <input type="checkbox" disabled={!admin} checked={!!r.incide_irt} onChange={(e) => setRubricas((xs) => xs.map((x) => (x.codigo === r.codigo ? { ...x, incide_irt: e.target.checked ? 1 : 0 } : x)))} />
                </Td>
                <Td className="w-56 py-1.5">
                  {r.tipo === "REMUNERACAO" ? (
                    <Input
                      className="num h-8 text-right"
                      disabled={!admin}
                      value={r.isento}
                      onChange={(e) => setRubricas((xs) => xs.map((x) => (x.codigo === r.codigo ? { ...x, isento: e.target.value } : x)))}
                    />
                  ) : (
                    <span className="block text-right text-ink-3">—</span>
                  )}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
        <p className="px-5 py-3 text-[12px] text-ink-3">
          Exemplo: subsídio de alimentação de {kz(4000000)} com isenção de {kz(3000000)} → {kz(1000000)} entram na matéria colectável de IRT.
        </p>
      </Card>
    </>
  );
}

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Pencil } from "lucide-react";
import { call } from "@/lib/api";
import type { OpcoesImpresso, TipoDocumento } from "@/lib/types";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface Modo {
  codigo: string;
  descricao: string;
  ativo: number;
  conta_id: number;
  conta: string | null;
}
interface Dados {
  tipos: TipoDocumento[];
  modos: Modo[];
  contas: { id: number; descricao: string; tipo: string }[];
  opcoes: OpcoesImpresso;
}

const VISTAS = [
  { id: "documentos", label: "Documentos" },
  { id: "venda", label: "Opções de venda" },
  { id: "impresso", label: "Faturas e recibos" },
] as const;
type Vista = (typeof VISTAS)[number]["id"];

const GRUPOS: Record<string, string> = { VENDAS: "Vendas", COMPRAS: "Compras", STOCK: "Stock" };
const STOCK: Record<number, string> = { [-1]: "Saída", 0: "—", 1: "Entrada", 2: "Transferência", 3: "Inventário" };

/** Parametrização de documentos, Opções de Venda e Definição de Faturas e Recibos (Sistema do KIGEST GC). */
export default function Parametrizacao() {
  const [sp, setSp] = useSearchParams();
  const vista: Vista = (VISTAS.find((v) => v.id === sp.get("vista"))?.id as Vista) ?? "documentos";
  const q = useQuery({ queryKey: ["parametros"], queryFn: () => call<Dados>("parametros.obter") });

  return (
    <>
      <PageHeader title="Parametrização de Documentos" subtitle="Textos, vias, validade e formato de cada documento, contas das formas de pagamento e o que aparece no impresso" />
      <div className="mb-4 flex w-fit gap-1 rounded-lg bg-sunken p-1">
        {VISTAS.map((v) => (
          <button
            key={v.id}
            onClick={() => setSp({ vista: v.id })}
            className={cn("rounded-md px-3 py-1.5 text-[13px]", vista === v.id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
          >
            {v.label}
          </button>
        ))}
      </div>
      {!q.data ? (
        <Card className="p-8 text-center text-ink-3">{q.error ? (q.error as Error).message : "A carregar…"}</Card>
      ) : vista === "documentos" ? (
        <Documentos tipos={q.data.tipos} />
      ) : vista === "venda" ? (
        <OpcoesVenda modos={q.data.modos} contas={q.data.contas} />
      ) : (
        <Impresso opcoes={q.data.opcoes} />
      )}
    </>
  );
}

// ======================================================================= documentos
function Documentos({ tipos }: { tipos: TipoDocumento[] }) {
  const [edit, setEdit] = useState<TipoDocumento | null>(null);
  return (
    <>
      <p className="mb-4 max-w-3xl text-[13px] text-ink-3">
        O comportamento fiscal de cada documento (stock, conta corrente, assinatura e SAF-T) é fixo. Aqui altera-se a designação impressa, o número de vias, a validade, a
        repetição de linhas, o formato do papel e os textos próprios do documento.
      </p>
      <Card>
        <Table>
          <thead>
            <tr>
              <Th>Código</Th>
              <Th>Descrição</Th>
              <Th>Grupo</Th>
              <Th>Stock</Th>
              <Th>C. corrente</Th>
              <Th className="text-right">Vias</Th>
              <Th className="text-right">Validade</Th>
              <Th>Linhas</Th>
              <Th>Papel</Th>
              <Th />
            </tr>
          </thead>
          <tbody>
            {tipos.map((t) => (
              <tr key={t.codigo} className="hover:bg-sunken/60">
                <Td className="font-mono text-[12.5px]">{t.codigo}</Td>
                <Td>
                  <div className="font-medium">{t.descricao}</div>
                  {t.texto_rodape && <div className="truncate text-[12px] text-ink-3">{t.texto_rodape}</div>}
                </Td>
                <Td className="text-ink-2">{GRUPOS[t.grupo] ?? t.grupo}</Td>
                <Td className="text-ink-2">{STOCK[t.stock] ?? t.stock}</Td>
                <Td className="text-ink-2">{t.conta_corrente > 0 ? "Débito" : t.conta_corrente < 0 ? "Crédito" : "—"}</Td>
                <Td className="num text-right">{t.vias}</Td>
                <Td className="num text-right">{t.validade_dias ? `${t.validade_dias} dias` : "—"}</Td>
                <Td>{t.repetir_linhas ? <Badge>Repete</Badge> : <Badge tone="brand">Soma</Badge>}</Td>
                <Td>{t.formato}</Td>
                <Td className="text-right">
                  <Button variant="ghost" title="Editar" onClick={() => setEdit(t)}>
                    <Pencil className="size-4" />
                  </Button>
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      </Card>
      {edit && <TipoModal t={edit} onClose={() => setEdit(null)} />}
    </>
  );
}

function TipoModal({ t, onClose }: { t: TipoDocumento; onClose: () => void }) {
  const qc = useQueryClient();
  const [f, setF] = useState(t);
  const [busy, setBusy] = useState(false);
  const set = <K extends keyof TipoDocumento>(k: K, v: TipoDocumento[K]) => setF((x) => ({ ...x, [k]: v }));

  async function gravar() {
    setBusy(true);
    try {
      await call("parametros.tipoGuardar", {
        codigo: f.codigo,
        descricao: f.descricao,
        vias: Number(f.vias) || 1,
        validade_dias: Number(f.validade_dias) || 0,
        repetir_linhas: !!f.repetir_linhas,
        prefixo_extenso: f.prefixo_extenso || null,
        texto_rodape: f.texto_rodape || null,
        formato: f.formato,
      });
      toast.success("Documento actualizado.");
      qc.invalidateQueries({ queryKey: ["parametros"] });
      qc.invalidateQueries({ queryKey: ["tabelas"] });
      onClose();
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
      title={`Documento ${t.codigo}`}
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={gravar}>
            Gravar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-3 gap-4">
        <Field label="Descrição (impressa no documento)" className="col-span-3">
          <Input autoFocus value={f.descricao} onChange={(e) => set("descricao", e.target.value)} />
        </Field>
        <Field label="Vias de impressão">
          <Input className="num text-right" type="number" min={1} max={4} value={f.vias} onChange={(e) => set("vias", Number(e.target.value))} />
        </Field>
        <Field label="Validade (dias)" hint="0 = sem validade">
          <Input className="num text-right" type="number" min={0} max={3650} value={f.validade_dias} onChange={(e) => set("validade_dias", Number(e.target.value))} />
        </Field>
        <Field label="Papel (A4)">
          <Select value={f.formato} onChange={(e) => set("formato", e.target.value as "A4" | "A5")}>
            <option value="A4">A4</option>
            <option value="A5">A5 (meia folha)</option>
          </Select>
        </Field>
        <label className="col-span-3 flex items-center gap-2 text-ink-2">
          <input type="checkbox" checked={!!f.repetir_linhas} onChange={(e) => set("repetir_linhas", e.target.checked ? 1 : 0)} />
          Duplicar linha (o mesmo artigo cria uma linha nova; desligado, soma a quantidade)
        </label>
        <Field label="Prefixo do valor por extenso" hint="Ex.: «São»" className="col-span-3">
          <Input value={f.prefixo_extenso ?? ""} onChange={(e) => set("prefixo_extenso", e.target.value)} placeholder="Valor por extenso" />
        </Field>
        <Field label="Texto próprio no rodapé" className="col-span-3">
          <Input value={f.texto_rodape ?? ""} onChange={(e) => set("texto_rodape", e.target.value)} placeholder="Ex.: Preços válidos durante 15 dias." />
        </Field>
      </div>
    </Modal>
  );
}

// ======================================================================= opções de venda
function OpcoesVenda({ modos, contas }: { modos: Modo[]; contas: Dados["contas"] }) {
  const qc = useQueryClient();
  async function guardar(m: Modo, patch: Partial<Modo>) {
    const n = { ...m, ...patch };
    try {
      await call("parametros.modoGuardar", { codigo: n.codigo, conta_id: n.conta_id, ativo: !!n.ativo });
      toast.success(`${n.descricao}: gravado.`);
      qc.invalidateQueries({ queryKey: ["parametros"] });
      qc.invalidateQueries({ queryKey: ["tabelas"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <>
      <p className="mb-4 max-w-3xl text-[13px] text-ink-3">
        Cada forma de pagamento entra numa conta da tesouraria: por exemplo, o numerário no Caixa e o Multicaixa e as transferências no banco. As contas criam-se em Contas e
        Centros de Custo. A mudança vale para os documentos já emitidos e para os novos.
      </p>
      <Card>
        <Table>
          <thead>
            <tr>
              <Th>Forma de pagamento</Th>
              <Th>Conta de destino</Th>
              <Th>Activa</Th>
            </tr>
          </thead>
          <tbody>
            {modos.map((m) => (
              <tr key={m.codigo}>
                <Td>
                  <div className="font-medium">{m.descricao}</div>
                  <div className="font-mono text-[12px] text-ink-3">{m.codigo}</div>
                </Td>
                <Td>
                  <Select className="max-w-xs" value={m.conta_id} onChange={(e) => guardar(m, { conta_id: Number(e.target.value) })}>
                    {contas.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.descricao}
                      </option>
                    ))}
                  </Select>
                </Td>
                <Td>
                  <input type="checkbox" disabled={m.codigo === "NU"} checked={!!m.ativo} onChange={(e) => guardar(m, { ativo: e.target.checked ? 1 : 0 })} />
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      </Card>
    </>
  );
}

// ======================================================================= definição de faturas e recibos
function Impresso({ opcoes }: { opcoes: OpcoesImpresso }) {
  const qc = useQueryClient();
  const [o, setO] = useState(opcoes);
  const [busy, setBusy] = useState(false);
  async function gravar() {
    setBusy(true);
    try {
      await call("parametros.opcoesGuardar", o);
      toast.success("Opções do impresso gravadas.");
      qc.invalidateQueries({ queryKey: ["parametros"] });
      qc.invalidateQueries({ queryKey: ["documento"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const itens: [keyof OpcoesImpresso, string, string][] = [
    ["mostrar_bancos", "Coordenadas bancárias", "Os IBAN da empresa no fim das facturas e proformas"],
    ["valor_extenso", "Valor por extenso", "O total escrito por extenso, com o prefixo de cada documento"],
    ["assinaturas", "Linhas de assinatura", "Espaço para a assinatura do cliente e da empresa"],
    ["mostrar_operador", "Operador", "O nome de quem emitiu o documento"],
  ];
  return (
    <Card className="max-w-2xl p-6">
      <div className="space-y-4">
        {itens.map(([k, titulo, texto]) => (
          <label key={k} className="flex cursor-pointer items-start gap-3">
            <input className="mt-1" type="checkbox" checked={o[k]} onChange={(e) => setO((x) => ({ ...x, [k]: e.target.checked }))} />
            <span>
              <span className="font-medium">{titulo}</span>
              <span className="block text-[13px] text-ink-3">{texto}</span>
            </span>
          </label>
        ))}
      </div>
      <p className="mt-5 text-[12.5px] text-ink-3">
        O logótipo, os dados da empresa e o rodapé geral estão em Configurações → Empresa. O papel (A4 ou A5) e o texto de cada documento estão no separador Documentos.
      </p>
      <div className="mt-5 flex justify-end">
        <Button loading={busy} onClick={gravar}>
          Gravar
        </Button>
      </div>
    </Card>
  );
}

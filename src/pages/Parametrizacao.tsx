import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Pencil } from "lucide-react";
import { call } from "@/lib/api";
import type { BalancaConfig, FechoConfig, OpcoesImpresso, TipoDocumento } from "@/lib/types";
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
        <>
          <OpcoesVenda modos={q.data.modos} contas={q.data.contas} />
          <FechoCaixa />
          <Balanca />
        </>
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

// ======================================================================= fecho de caixa
/** Kz a partir de cêntimos, como se escreve nas listas de notas e moedas (0,50 Kz → «0.5»). */
const kzLista = (v: number[]) => v.map((c) => String(c / 100)).join(", ");

function FechoCaixa() {
  const cfg = useQuery({ queryKey: ["caixa-config"], queryFn: () => call<FechoConfig>("caixa.config") });
  const [f, setF] = useState<{ fecho_cego: boolean; notas: string; moedas: string } | null>(null);
  const [busy, setBusy] = useState(false);
  if (!cfg.data) return null;
  const v = f ?? { fecho_cego: cfg.data.fecho_cego, notas: kzLista(cfg.data.notas), moedas: kzLista(cfg.data.moedas) };
  async function gravar() {
    setBusy(true);
    try {
      await call("caixa.configGuardar", v);
      toast.success("Opções do fecho de caixa gravadas.");
      setF(null);
      cfg.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card className="mt-5 max-w-3xl p-6">
      <div className="mb-4 font-medium">Fecho de caixa</div>
      <label className="flex cursor-pointer items-start gap-3">
        <input className="mt-1" type="checkbox" checked={v.fecho_cego} onChange={(e) => setF({ ...v, fecho_cego: e.target.checked })} />
        <span>
          <span className="font-medium">Fecho cego</span>
          <span className="block text-[13px] text-ink-3">
            Com o caixa aberto, o operador não vê o valor esperado nem os totais: conta a gaveta e só depois de fechar o relatório mostra a diferença. Quem tem a permissão
            Relatórios continua a ver tudo.
          </span>
        </span>
      </label>
      <div className="mt-4 grid grid-cols-2 gap-4">
        <Field label="Notas na contagem (Kz)" hint="Separadas por vírgulas">
          <Input value={v.notas} onChange={(e) => setF({ ...v, notas: e.target.value })} />
        </Field>
        <Field label="Moedas na contagem (Kz)" hint="Os cêntimos com ponto: 0.5">
          <Input value={v.moedas} onChange={(e) => setF({ ...v, moedas: e.target.value })} />
        </Field>
      </div>
      <div className="mt-5 flex justify-end gap-2">
        {f && (
          <Button variant="ghost" onClick={() => setF(null)}>
            Cancelar
          </Button>
        )}
        <Button loading={busy} onClick={gravar}>
          Gravar
        </Button>
      </div>
    </Card>
  );
}

// ======================================================================= balança (etiquetas com peso ou preço)
function Balanca() {
  const cfg = useQuery({ queryKey: ["balanca"], queryFn: () => call<BalancaConfig>("balanca.obter") });
  const [f, setF] = useState<BalancaConfig | null>(null);
  const [busy, setBusy] = useState(false);
  if (!cfg.data) return null;
  const v = f ?? cfg.data;
  const n = Number(v.digitos_codigo) || 5;
  // exemplo de etiqueta com o formato escolhido: prefixo + código do artigo + valor + controlo
  const exemplo = `${(v.prefixos.split(",")[0] || "20").trim()} ${"1".padStart(n, "0")} ${"750".padStart(10 - n, "0")} x`;
  const valorEx = v.tipo === "PESO" ? `${(750 / 10 ** v.decimais).toLocaleString("pt-PT")} (peso)` : `${(750 / 10 ** v.decimais).toLocaleString("pt-PT")} Kz (preço a pagar)`;
  async function gravar() {
    setBusy(true);
    try {
      await call("balanca.guardar", { ...v, digitos_codigo: n, decimais: Number(v.decimais) || 0 });
      toast.success("Formato das etiquetas da balança gravado.");
      setF(null);
      cfg.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card className="mt-5 max-w-3xl p-6">
      <div className="mb-1 font-medium">Balança: etiquetas com peso ou preço</div>
      <p className="mb-4 text-[13px] text-ink-3">
        A balança imprime etiquetas EAN-13 com o código do artigo (o «Código na balança» da ficha) e o peso ou o preço. O leitor do POS lança o artigo com essa quantidade.
      </p>
      <label className="flex cursor-pointer items-center gap-2">
        <input type="checkbox" checked={v.ativo} onChange={(e) => setF({ ...v, ativo: e.target.checked })} />
        <span className="font-medium">Ler etiquetas da balança</span>
      </label>
      <div className="mt-4 grid grid-cols-4 gap-4">
        <Field label="A etiqueta traz">
          <Select value={v.tipo} onChange={(e) => setF({ ...v, tipo: e.target.value as BalancaConfig["tipo"], decimais: e.target.value === "PESO" ? 3 : 0 })}>
            <option value="PESO">O peso</option>
            <option value="PRECO">O preço</option>
          </Select>
        </Field>
        <Field label="Prefixos" hint="20 a 29">
          <Input value={v.prefixos} onChange={(e) => setF({ ...v, prefixos: e.target.value })} />
        </Field>
        <Field label="Algarismos do artigo" hint="4 a 6">
          <Input className="num" value={v.digitos_codigo} onChange={(e) => setF({ ...v, digitos_codigo: Number(e.target.value.replace(/\D/g, "")) || 0 })} />
        </Field>
        <Field label="Casas decimais" hint={v.tipo === "PESO" ? "Gramas: 3" : "Kz: 0 · cêntimos: 2"}>
          <Input className="num" value={v.decimais} onChange={(e) => setF({ ...v, decimais: Number(e.target.value.replace(/\D/g, "")) || 0 })} />
        </Field>
      </div>
      <p className="mt-3 text-[12.5px] text-ink-3">
        Exemplo: <span className="font-mono">{exemplo}</span> → artigo {"1".padStart(n, "0")}, {valorEx}.
      </p>
      <div className="mt-5 flex justify-end gap-2">
        {f && (
          <Button variant="ghost" onClick={() => setF(null)}>
            Cancelar
          </Button>
        )}
        <Button loading={busy} onClick={gravar}>
          Gravar
        </Button>
      </div>
    </Card>
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

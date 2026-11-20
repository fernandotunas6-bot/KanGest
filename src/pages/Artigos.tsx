import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Camera, Copy, FileSpreadsheet, Image as ImageIcon, Layers, Package, Plus, Search, Tags, Trash2 } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { comIva, kz, parseKz, qtd, semIva, toInputKz } from "@/lib/format";
import { exportarExcel, kzXL } from "@/lib/csv";
import type { Artigo, Componente } from "@/lib/types";
import { ArtigoPicker } from "@/components/Pickers";
import { useSession } from "@/store/session";
import { Atalho, Badge, BotaoCancelar, Button, Card, Confirmar, Empty, Field, Input, Modal, PageHeader, Segmentado, Select, Table, Td, Th, cn, useOrdenacao } from "@/components/ui";
import { useJaUsado, useSujo } from "@/lib/ficha";
import { bip, gtinValido, useLeitor } from "@/lib/leitor";
import { atributosDe, ramoPorId, resumoAtributos, type Ramo } from "@/lib/ramos";
import { prepararFoto, useMiniaturas } from "@/lib/fotos";
import { InputData } from "@/components/InputData";
import { AbasFicha } from "@/components/Auditoria";

export default function Artigos() {
  const [q, setQ] = useState("");
  const [familia, setFamilia] = useState<string>("");
  const [editar, setEditar] = useState<Partial<Artigo> | null>(null);
  const [origem, setOrigem] = useState<number | undefined>();
  // cada ficha nova é outra (o «Guardar e novo» abre uma limpa)
  const [vez, setVez] = useState(0);
  const [estado, setEstado] = useState<"activos" | "inactivos" | "todos">("activos");
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [lote, setLote] = useState<"familia" | "preco" | "tipo_inventario" | null>(null);
  const qc = useQueryClient();
  const dq = useDebounce(q);
  const tabelas = useTabelas();
  const ramo = ramoPorId(tabelas.data?.ramo);
  const tem = useSession((s) => s.tem);
  const [sp, setSp] = useSearchParams();
  // ?id=… (pesquisa global, Ctrl+K): abre logo a ficha do artigo
  useEffect(() => {
    // ?novo=código (POS → «Criar artigo com este código»): ficha nova com o código de barras lido
    const novo = sp.get("novo");
    if (novo) {
      setSp({}, { replace: true });
      if (tem("artigos.criar")) novoArtigo(novo);
      return;
    }
    const id = Number(sp.get("id"));
    if (!id) return;
    setSp({}, { replace: true });
    if (!tem("artigos.editar")) return;
    call<Artigo>("artigos.obter", { id }).then(
      (a) => {
        setOrigem(undefined);
        setEditar(a);
      },
      (e: Error) => toast.error(e.message),
    );
  }, [sp, setSp, tem]);
  const lista = useQuery({
    queryKey: ["artigos", dq, familia, estado !== "activos"],
    queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, familia_id: familia ? Number(familia) : null, inativos: estado !== "activos" }),
  });
  const visiveis = estado === "inactivos" ? lista.data?.filter((a) => !a.ativo) : lista.data;
  const miniaturas = useMiniaturas(visiveis);
  const seleccionados = (visiveis ?? []).filter((a) => sel.has(a.id));
  const todosMarcados = !!visiveis?.length && visiveis.every((a) => sel.has(a.id));
  function marcar(id: number) {
    setSel((x) => {
      const n = new Set(x);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }
  function novoArtigo(codigo?: string) {
    setOrigem(undefined);
    setVez((v) => v + 1);
    // valores por omissão do ramo (serviços à hora, farmácia com lotes, restauração sem stock…)
    const n = ramo.novoArtigo ?? {};
    const tipo = n.tipo ?? "P";
    const unidade = tabelas.data?.unidades.find((u) => u.abreviatura === n.unidade)?.id ?? 1;
    setEditar({
      tipo,
      taxa_iva_id: 1,
      unidade_id: unidade,
      familia_id: 1,
      controla_stock: tipo === "P" && n.controla_stock !== false ? 1 : 0,
      controla_lote: n.controla_lote ? 1 : 0,
      ativo: 1,
      ...(typeof codigo === "string" && codigo ? { codigo_barras: codigo } : {}),
    });
  }
  // leitor de códigos na lista: abre o artigo lido; se nenhum o tiver, uma ficha nova já com o código
  useLeitor(async (l) => {
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo: l.codigo });
      if (a) {
        bip(true);
        if (!tem("artigos.editar")) return toast.info(`${a.referencia} · ${a.descricao}`);
        setOrigem(undefined);
        setEditar(await call<Artigo>("artigos.obter", { id: a.id }));
      } else {
        bip(false);
        if (tem("artigos.criar")) {
          toast.info(`Nenhum artigo tem o código ${l.codigo}: ficha nova com ele.`);
          novoArtigo(l.codigo);
        } else toast.warning(`Nenhum artigo com o código ${l.codigo}.`);
      }
    } catch (e) {
      toast.error((e as Error).message);
    }
  }, !editar && !lote);
  async function emLote(accao: string, extra: Record<string, unknown> = {}) {
    try {
      const r = await call<{ alterados: number }>("artigos.lote", { ids: seleccionados.map((a) => a.id), accao, ...extra });
      toast.success(`${r.alterados} artigo(s) alterado(s).`);
      setSel(new Set());
      setLote(null);
      qc.invalidateQueries({ queryKey: ["artigos"] });
      qc.invalidateQueries({ queryKey: ["pos-artigos"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  function exportar() {
    exportarExcel(
      "artigos.xlsx",
      [
        ["Referência", "Descrição", "Código de barras", "Família", "Tipo", "IVA %", "Custo médio", "PVP 1 (c/ IVA)", "Stock", "Estado"],
        ...(ordenados ?? []).map((a) => [
          a.referencia,
          a.descricao,
          a.codigo_barras ?? "",
          a.familia ?? "",
          a.tipo === "S" ? "Serviço" : "Produto",
          a.taxa_pct,
          kzXL(a.custo_medio),
          kzXL(a.preco1_iva),
          a.tipo === "S" || !a.controla_stock ? "" : a.stock,
          a.ativo ? "Activo" : "Inactivo",
        ]),
      ],
      { titulo: "Artigos" },
    );
  }
  const { ordenados, cab } = useOrdenacao(visiveis, (a, k: "ref" | "desc" | "familia" | "iva" | "custo" | "pvp" | "stock") =>
    k === "ref" ? a.referencia : k === "desc" ? a.descricao : k === "familia" ? a.familia : k === "iva" ? a.taxa_pct : k === "custo" ? a.custo_medio : k === "pvp" ? a.preco1_iva : a.tipo === "S" ? null : a.stock,
  );

  return (
    <>
      <PageHeader
        title="Artigos"
        subtitle="Produtos e serviços, preços e IVA"
        actions={
          <div className="flex gap-2">
            <Button variant="secondary" onClick={exportar} disabled={!visiveis?.length}>
              <FileSpreadsheet className="size-4" /> Excel
            </Button>
            <Link to="/etiquetas">
              <Button variant="secondary">
                <Tags className="size-4" /> Etiquetas
              </Button>
            </Link>
            {tem("artigos.criar") && (
              <Button onClick={() => novoArtigo()}>
                <Plus className="size-4" /> Novo artigo
              </Button>
            )}
          </div>
        }
      />
      <Card>
        <div className="flex flex-wrap gap-3 border-b border-line p-4">
          <div className="relative min-w-60 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input
              className="pl-9"
              placeholder={ramo.campos.length ? `Pesquisar por descrição, referência, código ou ${ramo.campos.slice(0, 2).map((c) => c.label.toLowerCase()).join(", ")}…` : "Pesquisar por descrição, referência ou código de barras…"}
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <Select className="w-56" value={familia} onChange={(e) => setFamilia(e.target.value)}>
            <option value="">Todas as famílias</option>
            {tabelas.data?.familias.map((f) => (
              <option key={f.id} value={f.id}>
                {f.descricao}
              </option>
            ))}
          </Select>
          <Segmentado
            className="w-full sm:w-72"
            valor={estado}
            onChange={(v) => {
              setEstado(v);
              setSel(new Set());
            }}
            opcoes={[
              { id: "activos", label: "Activos" },
              { id: "inactivos", label: "Inactivos" },
              { id: "todos", label: "Todos" },
            ]}
          />
        </div>
        {seleccionados.length > 0 && tem("artigos.editar") && (
          <div className="flex flex-wrap items-center gap-2 border-b border-line bg-brand-50/60 px-4 py-2.5 text-[13px]">
            <span className="mr-2 font-medium text-brand-700">{seleccionados.length} seleccionado(s)</span>
            <Button size="sm" variant="secondary" onClick={() => emLote("activar")}>
              Activar
            </Button>
            <Button size="sm" variant="secondary" onClick={() => emLote("desactivar")}>
              Desactivar
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setLote("familia")}>
              Mudar de família
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setLote("preco")}>
              Ajustar preços
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setLote("tipo_inventario")}>
              Tipo de inventário
            </Button>
            <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setSel(new Set())}>
              Limpar a selecção
            </Button>
          </div>
        )}
        {visiveis?.length === 0 ? (
          <Empty
            icon={<Package className="size-6" />}
            title={estado === "inactivos" ? "Sem artigos desactivados" : "Sem artigos"}
            text={estado === "inactivos" ? undefined : dq || familia ? "Nenhum artigo com esta pesquisa." : "Crie o primeiro artigo para começar a vender."}
          />
        ) : (
          <Table className="max-h-[calc(100vh-304px)]">
            <thead>
              <tr>
                {tem("artigos.editar") && (
                  <Th className="w-8">
                    <input
                      type="checkbox"
                      aria-label="Seleccionar todos"
                      checked={todosMarcados}
                      onChange={() => setSel(todosMarcados ? new Set() : new Set((visiveis ?? []).map((a) => a.id)))}
                    />
                  </Th>
                )}
                <Th {...cab("ref")}>Referência</Th>
                <Th {...cab("desc")}>Descrição</Th>
                <Th {...cab("familia")}>Família</Th>
                <Th className="text-right" {...cab("iva")}>
                  IVA
                </Th>
                <Th className="text-right" {...cab("custo")}>
                  Custo médio
                </Th>
                <Th className="text-right" {...cab("pvp")}>
                  PVP (c/ IVA)
                </Th>
                <Th className="text-right" {...cab("stock")}>
                  Stock
                </Th>
              </tr>
            </thead>
            <tbody>
              {ordenados?.map((a) => (
                <tr key={a.id} className={cn("cursor-pointer hover:bg-sunken/60", !a.ativo && "opacity-50")} onClick={() => {
                    if (!tem("artigos.editar")) return;
                    setOrigem(undefined);
                    setEditar(a);
                  }}>
                  {tem("artigos.editar") && (
                    <Td onClick={(e) => e.stopPropagation()}>
                      <input type="checkbox" aria-label={`Seleccionar ${a.descricao}`} checked={sel.has(a.id)} onChange={() => marcar(a.id)} />
                    </Td>
                  )}
                  <Td className="font-mono text-[12.5px] text-ink-2">{a.referencia}</Td>
                  <Td>
                    <div className="flex items-start gap-2.5">
                    {miniaturas[a.id] ? (
                      <img src={miniaturas[a.id]} alt="" className="size-9 shrink-0 rounded-md border border-line object-cover" loading="lazy" />
                    ) : null}
                    <div className="min-w-0">
                    <div className="font-medium">
                      {a.descricao}
                      {!!a.composto && <Badge tone="brand" className="ml-2">Kit</Badge>}
                      {!!a.controla_lote && <Badge tone="accent" className="ml-2">Lotes</Badge>}
                      {!a.ativo && <Badge className="ml-2">Inactivo</Badge>}
                    </div>
                    {resumoAtributos(a, ramo) && <div className="text-[12px] text-ink-2">{resumoAtributos(a, ramo)}</div>}
                    {a.codigo_barras && <div className="font-mono text-[11.5px] text-ink-3">{a.codigo_barras}</div>}
                    </div>
                    </div>
                  </Td>
                  <Td className="text-ink-2">{a.familia}</Td>
                  <Td className="num text-right text-ink-2">{a.taxa_pct}%</Td>
                  <Td className="num text-right text-ink-2">{kz(a.custo_medio)}</Td>
                  <Td className="num text-right font-medium">{kz(a.preco1_iva)}</Td>
                  <Td className="num text-right">
                    {a.tipo === "S" ? (
                      <Badge>Serviço</Badge>
                    ) : a.composto ? (
                      <span className="text-[12px] text-ink-3">pelos componentes</span>
                    ) : a.controla_stock ? (
                      <span className={cn(a.stock <= a.qtd_minima && "font-medium text-danger")}>
                        {qtd(a.stock)} {a.unidade}
                      </span>
                    ) : (
                      "—"
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {editar && (
        <ArtigoForm
          key={`${editar.id ?? "novo"}-${origem ?? ""}-${vez}`}
          inicial={editar}
          origem={origem}
          onClose={() => setEditar(null)}
          onNovo={tem("artigos.criar") ? () => novoArtigo() : undefined}
          onClonar={
            tem("artigos.criar")
              ? (c) => {
                  setOrigem(c.id);
                  setEditar(clonar(c));
                }
              : undefined
          }
        />
      )}
      {lote === "tipo_inventario" && <LoteTipoInventario n={seleccionados.length} onAplicar={(tipo_inventario) => emLote("tipo_inventario", { tipo_inventario })} onClose={() => setLote(null)} />}
      {lote === "familia" && <LoteFamilia n={seleccionados.length} onAplicar={(familia_id) => emLote("familia", { familia_id })} onClose={() => setLote(null)} />}
      {lote === "preco" && <LotePreco artigos={seleccionados} onAplicar={(percentagem, arredondar) => emLote("preco", { percentagem, arredondar })} onClose={() => setLote(null)} />}
    </>
  );
}

/** Tipos de inventário do SAF-T (iguais aos do servidor, services/inventario.rs). */
const TIPOS_INVENTARIO: [string, string][] = [
  ["M", "Mercadorias"],
  ["P", "Matérias-primas, subsidiárias e de consumo"],
  ["A", "Produtos acabados e intermédios"],
  ["S", "Subprodutos, desperdícios e refugos"],
  ["T", "Produtos e trabalhos em curso"],
];

function LoteTipoInventario({ n, onAplicar, onClose }: { n: number; onAplicar: (tipo: string) => Promise<void>; onClose: () => void }) {
  const [t, setT] = useState("M");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      onClose={onClose}
      title={`Tipo de inventário de ${n} artigo(s)`}
      width="max-w-md"
      footer={
        <>
          <BotaoCancelar />
          <Button
            loading={busy}
            onClick={async () => {
              setBusy(true);
              await onAplicar(t);
              setBusy(false);
            }}
          >
            Aplicar
          </Button>
        </>
      }
    >
      <Field label="Tipo no SAF-T de inventários" hint="Quem só revende tem tudo em «Mercadorias»; quem fabrica separa as matérias-primas, os produtos acabados…">
        <Select autoFocus value={t} onChange={(e) => setT(e.target.value)}>
          {TIPOS_INVENTARIO.map(([c, nome]) => (
            <option key={c} value={c}>
              {c} · {nome}
            </option>
          ))}
        </Select>
      </Field>
    </Modal>
  );
}

function LoteFamilia({ n, onAplicar, onClose }: { n: number; onAplicar: (familia: number) => Promise<void>; onClose: () => void }) {
  const familias = useTabelas().data?.familias ?? [];
  const [f, setF] = useState<number | "">("");
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      open
      onClose={onClose}
      title={`Mudar ${n} artigo(s) de família`}
      width="max-w-md"
      footer={
        <>
          <BotaoCancelar />
          <Button
            disabled={!f}
            loading={busy}
            onClick={async () => {
              setBusy(true);
              await onAplicar(Number(f));
              setBusy(false);
            }}
          >
            Mudar
          </Button>
        </>
      }
    >
      <Field label="Nova família" hint="A subfamília dos artigos fica vazia (era da família antiga).">
        <Select autoFocus value={f} onChange={(e) => setF(e.target.value ? Number(e.target.value) : "")}>
          <option value="">Escolha…</option>
          {familias.map((x) => (
            <option key={x.id} value={x.id}>
              {x.descricao}
            </option>
          ))}
        </Select>
      </Field>
    </Modal>
  );
}

/** Ajustar o PVP 1 de vários artigos numa percentagem, com arredondamento e uma amostra do resultado. */
function LotePreco({ artigos, onAplicar, onClose }: { artigos: Artigo[]; onAplicar: (pct: number, arredondar: number) => Promise<void>; onClose: () => void }) {
  const [pct, setPct] = useState("");
  const [arred, setArred] = useState(0);
  const [busy, setBusy] = useState(false);
  const p = Number(pct.replace(",", "."));
  const valido = pct.trim() !== "" && Number.isFinite(p) && p !== 0 && p > -100 && p <= 1000;
  const novo = (pvp: number) => {
    let v = Math.round(pvp * (1 + p / 100));
    if (arred > 0) v = Math.floor((v + (arred * 100) / 2) / (arred * 100)) * arred * 100;
    return Math.max(0, v);
  };
  return (
    <Modal
      open
      onClose={onClose}
      title={`Ajustar o preço de ${artigos.length} artigo(s)`}
      width="max-w-lg"
      footer={
        <>
          <BotaoCancelar />
          <Button
            disabled={!valido}
            loading={busy}
            onClick={async () => {
              setBusy(true);
              await onAplicar(p, arred);
              setBusy(false);
            }}
          >
            Aplicar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Percentagem" hint="Ex.: 10 para subir 10%, -5 para baixar 5%">
          <Input autoFocus inputMode="decimal" className="num text-right" value={pct} onChange={(e) => setPct(e.target.value)} placeholder="0" />
        </Field>
        <Field label="Arredondar o PVP a">
          <Select value={arred} onChange={(e) => setArred(Number(e.target.value))}>
            <option value={0}>Sem arredondar</option>
            {[1, 5, 10, 50, 100, 500].map((v) => (
              <option key={v} value={v}>
                {v} Kz
              </option>
            ))}
          </Select>
        </Field>
      </div>
      <p className="mt-3 text-[13px] text-ink-2">Muda o PVP 1 (com IVA); o preço sem IVA acompanha. Os outros níveis de preço não mudam. Cada artigo fica na auditoria.</p>
      {valido && (
        <table className="mt-3 w-full text-[13px]">
          <thead>
            <tr className="text-left text-[11.5px] uppercase tracking-wide text-ink-3">
              <th className="py-1 font-medium">Artigo</th>
              <th className="py-1 text-right font-medium">Agora</th>
              <th className="py-1 text-right font-medium">Fica</th>
            </tr>
          </thead>
          <tbody>
            {artigos.slice(0, 5).map((a) => (
              <tr key={a.id} className="border-t border-line/70">
                <td className="py-1">{a.descricao}</td>
                <td className="num py-1 text-right text-ink-2">{kz(a.preco1_iva)}</td>
                <td className="num py-1 text-right font-medium">{kz(novo(a.preco1_iva))}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {valido && artigos.length > 5 && <p className="mt-1 text-[12px] text-ink-3">… e mais {artigos.length - 5}.</p>}
    </Modal>
  );
}

/** Cópia de um artigo para criar outro parecido (Clonar artigo do KIGEST): sem referência, código de barras nem stock. */
function clonar(a: Partial<Artigo>): Partial<Artigo> {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const { id, referencia, codigo_barras, stock, ...resto } = a;
  return { ...resto, descricao: `${a.descricao ?? ""} (cópia)`, ativo: 1 };
}

function ArtigoForm({
  inicial,
  origem,
  onClose,
  onClonar,
  onNovo,
}: {
  inicial: Partial<Artigo>;
  origem?: number;
  onClose: () => void;
  onClonar?: (a: Partial<Artigo>) => void;
  /** «Guardar e novo»: depois de guardar, abre uma ficha nova */
  onNovo?: () => void;
}) {
  const t = useTabelas().data;
  const tem = useSession((s) => s.tem);
  const ramo = ramoPorId(t?.ramo);
  const qc = useQueryClient();
  const [a, setA] = useState<Partial<Artigo>>(inicial);
  // dados do ramo (autor, tamanho, garantia…); os de outros ramos que o artigo já tenha mantêm-se
  const [atrib, setAtrib] = useState<Record<string, string | number | boolean>>(() => atributosDe(inicial));
  const [variantes, setVariantes] = useState(false);
  // fotografia: a actual (carregada da ficha), a nova escolhida (grava-se com o artigo) ou "apagar"
  const [fotoAtual, setFotoAtual] = useState<string | null>(null);
  const [fotoNova, setFotoNova] = useState<{ miniatura: string; imagem: string } | "apagar" | null>(null);
  useEffect(() => {
    if (inicial.id && inicial.foto) call<{ imagem: string | null }>("fotos.obter", { artigo_id: inicial.id }).then((r) => setFotoAtual(r.imagem)).catch(() => {});
  }, [inicial.id, inicial.foto]);
  async function escolherFoto(f: File | undefined) {
    if (!f) return;
    try {
      setFotoNova(await prepararFoto(f));
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  const fotoVista = fotoNova === "apagar" ? null : fotoNova?.imagem ?? fotoAtual;
  const taxa = t?.taxas.find((x) => x.id === a.taxa_iva_id)?.percentagem ?? 0;
  // PVP com IVA: o guardado quando bate com o preço sem IVA (é o valor mestre); senão, calculado
  const [pvp, setPvp] = useState(() =>
    [
      [a.preco1, a.pvp1],
      [a.preco2, a.pvp2],
      [a.preco3, a.pvp3],
      [a.preco4, a.pvp4],
    ].map(([p, g]) => toInputKz(g != null && semIva(g, taxa) === (p ?? 0) ? g : comIva(p ?? 0, taxa))),
  );
  const [custo, setCusto] = useState(toInputKz(a.custo ?? 0));
  const [busy, setBusy] = useState(false);
  const [comps, setComps] = useState<Componente[]>([]);
  // outros códigos de barras (embalagens): carregados da ficha; só se gravam depois de carregados
  const [codigos, setCodigos] = useState<{ codigo: string; descricao: string; quantidade: number }[] | null>(inicial.id ? null : []);
  // o que veio do servidor (para saber se a lista de códigos ou de componentes foi mexida)
  const [codigosBase, setCodigosBase] = useState<string>("[]");
  const [compsBase, setCompsBase] = useState<string>("[]");
  useEffect(() => {
    if (!inicial.id) return;
    call<{ codigos: { codigo: string; descricao: string | null; quantidade: number }[] }>("artigos.obter", { id: inicial.id })
      .then((x) => {
        const cs = x.codigos.map((c) => ({ codigo: c.codigo, descricao: c.descricao ?? "", quantidade: c.quantidade }));
        setCodigos(cs);
        setCodigosBase(JSON.stringify(cs));
      })
      .catch(() => {});
  }, [inicial.id]);
  useEffect(() => {
    const fonte = inicial.id ?? origem;
    if (fonte && inicial.composto)
      call<Componente[]>("artigos.componentes", { id: fonte })
        .then((cs) => {
          setComps(cs);
          if (inicial.id) setCompsBase(JSON.stringify(cs));
        })
        .catch(() => {});
  }, [inicial.id, origem, inicial.composto]);
  const custoKit = comps.reduce((s, c) => s + Math.round((c.custo_medio ?? 0) * c.quantidade), 0);
  const set = (k: keyof Artigo, v: unknown) => setA((x) => ({ ...x, [k]: v }));
  const [erros, setErros] = useState<{ descricao?: string }>({});
  const [eliminar, setEliminar] = useState(false);
  const sujo =
    useSujo({ a, pvp, custo, atrib }) || fotoNova !== null || (codigos !== null && JSON.stringify(codigos) !== codigosBase) || (!!inicial.id && JSON.stringify(comps) !== compsBase);
  // referência e código de barras que já estão noutro artigo (avisa enquanto se escreve)
  type Outro = { id: number; referencia: string; descricao: string; ativo: boolean };
  const refUsada = useJaUsado<Outro>("artigos.codigoUsado", "codigo", a.referencia, a.id, inicial.referencia);
  const cobarUsado = useJaUsado<Outro>("artigos.codigoUsado", "codigo", a.codigo_barras, a.id, inicial.codigo_barras);
  const jaUsado = (o: Outro | null) => (o ? `Já está no artigo ${o.referencia} «${o.descricao}»${o.ativo ? "" : " (desactivado)"}.` : null);

  const preco1 = semIva(parseKz(pvp[0]), taxa);
  // margem sobre o custo de cada preço (Margem1–4 do KIGEST): preço s/ IVA = custo × (1 + margem)
  const custoC = parseKz(custo);
  const semIvaDe = (i: number) => semIva(parseKz(pvp[i]), taxa);
  const margemDe = (i: number) => (custoC > 0 && semIvaDe(i) > 0 ? ((semIvaDe(i) - custoC) / custoC) * 100 : null);
  const [margemTxt, setMargemTxt] = useState<(string | null)[]>([null, null, null, null]);
  function aplicarMargem(i: number, texto: string) {
    setMargemTxt((xs) => xs.map((x, j) => (j === i ? texto : x)));
    const m = Number(texto.replace(",", "."));
    if (texto.trim() === "" || !Number.isFinite(m) || custoC <= 0) return;
    setPvp((xs) => xs.map((x, j) => (j === i ? toInputKz(comIva(Math.round(custoC * (1 + m / 100)), taxa)) : x)));
  }
  const hojeStr = new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  const promoPct = a.promo_desconto ?? 0;
  const promoEstado =
    promoPct <= 0
      ? null
      : a.promo_inicio && a.promo_inicio > hojeStr
        ? { tom: "neutral" as const, txt: `Começa a ${a.promo_inicio.split("-").reverse().join("/")}` }
        : a.promo_fim && a.promo_fim < hojeStr
          ? { tom: "neutral" as const, txt: "Terminada" }
          : { tom: "accent" as const, txt: `Em promoção −${promoPct}%` };

  async function guardar(depois: "fechar" | "novo" = "fechar") {
    if (busy) return;
    const e: typeof erros = {};
    if (!a.descricao?.trim()) e.descricao = "Escreva a descrição do artigo.";
    setErros(e);
    if (e.descricao) {
      document.querySelector<HTMLInputElement>("[role=dialog] input[data-campo=descricao]")?.focus();
      return;
    }
    if (refUsada || cobarUsado) {
      toast.error(jaUsado(refUsada ?? cobarUsado) ?? "");
      return;
    }
    setBusy(true);
    try {
      const precos = pvp.map((p) => semIva(parseKz(p), taxa));
      const gravado = await call<{ id: number }>("artigos.guardar", {
        ...a,
        custo: parseKz(custo),
        preco1: precos[0],
        preco2: precos[1],
        preco3: precos[2],
        preco4: precos[3],
        // o PVP com IVA escrito é o valor mestre (o servidor deriva dele o preço sem IVA)
        pvp1: parseKz(pvp[0]),
        pvp2: parseKz(pvp[1]),
        pvp3: parseKz(pvp[2]),
        pvp4: parseKz(pvp[3]),
        controla_stock: !!a.controla_stock && !a.composto,
        permite_negativo: !!a.permite_negativo,
        ativo: a.ativo === undefined ? true : !!a.ativo,
        controla_lote: !!a.controla_lote && !a.composto,
        aviso_validade_dias: a.aviso_validade_dias ?? 30,
        composto: !!a.composto,
        promo_desconto: Math.min(100, Math.max(0, Number(a.promo_desconto ?? 0) || 0)),
        promo_inicio: a.promo_inicio || null,
        promo_fim: a.promo_fim || null,
        componentes: a.composto ? comps.map((c) => ({ componente_id: c.componente_id, quantidade: c.quantidade })) : null,
        codigos: codigos ? codigos.filter((c) => c.codigo.trim()).map((c) => ({ codigo: c.codigo.trim(), descricao: c.descricao || null, quantidade: c.quantidade || 1 })) : undefined,
        codigo_balanca: a.codigo_balanca ?? "",
        atributos: atrib,
      });
      // a fotografia grava-se depois (o artigo novo só tem número agora)
      if (fotoNova === "apagar") await call("fotos.apagar", { artigo_id: gravado.id });
      else if (fotoNova) await call("fotos.guardar", { artigo_id: gravado.id, ...fotoNova });
      toast.success("Artigo guardado.");
      qc.invalidateQueries({ queryKey: ["artigos"] });
      qc.invalidateQueries({ queryKey: ["pos-artigos"] });
      qc.invalidateQueries({ queryKey: ["auditoria-historico", "artigos"] });
      if (depois === "novo" && onNovo) onNovo();
      else onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function confirmarEliminar() {
    try {
      const r = await call<{ eliminado: boolean; documentos?: number }>("artigos.eliminar", { id: a.id });
      if (r.eliminado) toast.success("Artigo eliminado.");
      else
        toast.success("O artigo ficou desactivado", {
          description: `Já tinha ${r.documentos ? `${r.documentos} documento(s)` : "movimentos de stock ou ligações"}: deixa de aparecer nas vendas e pode reactivá-lo na ficha (estado «Inactivos»).`,
          duration: 9000,
        });
      qc.invalidateQueries({ queryKey: ["artigos"] });
      qc.invalidateQueries({ queryKey: ["pos-artigos"] });
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <Modal
      open
      onClose={onClose}
      title={a.id ? `Editar ${a.referencia}` : origem ? "Novo artigo (clonado)" : "Novo artigo"}
      width="max-w-3xl"
      sujo={sujo}
      onGuardar={() => guardar()}
      footer={
        <>
          <span className="mr-auto flex gap-1">
            {a.id && tem("artigos.editar") && (
              <Button variant="ghost" className="text-danger hover:bg-danger-50 hover:text-danger" onClick={() => setEliminar(true)}>
                <Trash2 className="size-4" /> Eliminar
              </Button>
            )}
            {a.id && onClonar && (
              <Button variant="ghost" onClick={() => onClonar({ ...a, custo: parseKz(custo) })}>
                <Copy className="size-4" /> Clonar
              </Button>
            )}
          </span>
          <BotaoCancelar />
          {!a.id && onNovo && (
            <Button variant="secondary" loading={busy} onClick={() => guardar("novo")}>
              Guardar e novo
            </Button>
          )}
          <Button loading={busy} onClick={() => guardar()}>
            Guardar <Atalho tecla="S" />
          </Button>
        </>
      }
    >
      {eliminar && (
        <Confirmar titulo={`Eliminar «${a.descricao ?? ""}»?`} confirmar="Eliminar" onConfirmar={confirmarEliminar} onClose={() => setEliminar(false)}>
          <p>Se o artigo nunca foi usado, é apagado.</p>
          <p>
            Se já tem vendas, compras, movimentos de stock, lotes ou entra num kit, fica só <strong className="text-ink">desactivado</strong>: deixa de aparecer nas vendas e na
            pesquisa, os documentos antigos continuam a mostrá-lo e pode reactivá-lo na ficha.
          </p>
          <p className="text-[13px] text-ink-3">Fica registado na auditoria.</p>
        </Confirmar>
      )}
      <AbasFicha entidade="artigos" id={a.id}>
        <div className="mb-4 flex items-center gap-4">
          <div className="grid size-20 shrink-0 place-items-center overflow-hidden rounded-xl border border-dashed border-line-strong bg-sunken">
            {fotoVista ? <img src={fotoVista} alt="Fotografia do artigo" className="size-full object-cover" /> : <ImageIcon className="size-6 text-ink-3" />}
          </div>
          <div className="space-y-1.5">
            <div className="text-[13px] font-medium">Fotografia</div>
            <div className="flex flex-wrap gap-2">
              <label className="kg-btn kg-btn-sm inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-lg border border-line-strong bg-surface px-3 text-[13px] font-medium hover:bg-sunken">
                <Camera className="size-4" /> {fotoVista ? "Trocar" : "Escolher ou tirar foto"}
                <input type="file" accept="image/*" className="hidden" onChange={(e) => escolherFoto(e.target.files?.[0])} />
              </label>
              {fotoVista && (
                <Button variant="ghost" size="sm" onClick={() => setFotoNova("apagar")}>
                  Remover
                </Button>
              )}
            </div>
            <div className="text-[12px] text-ink-3">Aparece na grelha do POS e na loja online (API). Reduzida a 800 px.</div>
          </div>
        </div>

        <div className="grid grid-cols-4 gap-4">
          <Field label="Descrição" className="col-span-3" erro={erros.descricao}>
            <Input
              autoFocus
              data-campo="descricao"
              value={a.descricao ?? ""}
              onChange={(e) => {
                set("descricao", e.target.value);
                if (e.target.value.trim()) setErros((x) => ({ ...x, descricao: undefined }));
              }}
            />
          </Field>
          <Field label="Tipo">
            <Select value={a.tipo} onChange={(e) => set("tipo", e.target.value)}>
              <option value="P">Produto</option>
              <option value="S">Serviço</option>
            </Select>
          </Field>
          <Field label="Referência" hint={a.id ? undefined : "Vazio = automática"} erro={jaUsado(refUsada)}>
            <Input value={a.referencia ?? ""} onChange={(e) => set("referencia", e.target.value)} />
          </Field>
          <Field
            label={ramo.rotuloCodigo ?? "Código de barras"}
            hint={ramo.id === "livraria" ? "ISBN-10 ou 13, com ou sem hífenes" : undefined}
            erro={jaUsado(cobarUsado)}
            aviso={gtinValido(a.codigo_barras ?? "") === false ? "O último algarismo não bate com os outros (EAN/UPC): confirme o código." : undefined}
          >
            <Input value={a.codigo_barras ?? ""} onChange={(e) => set("codigo_barras", e.target.value)} />
          </Field>
          <Field label="Família">
            <Select value={a.familia_id ?? ""} onChange={(e) => set("familia_id", Number(e.target.value))}>
              {t?.familias.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.descricao}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Unidade">
            <Select value={a.unidade_id ?? ""} onChange={(e) => set("unidade_id", Number(e.target.value))}>
              {t?.unidades.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.descricao} ({u.abreviatura})
                </option>
              ))}
            </Select>
          </Field>

          <Field label="Taxa de IVA">
            <Select value={a.taxa_iva_id} onChange={(e) => set("taxa_iva_id", Number(e.target.value))}>
              {t?.taxas.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.descricao}
                </option>
              ))}
            </Select>
          </Field>
          {taxa === 0 && (
            <Field label="Motivo de isenção" className="col-span-3">
              <Select value={a.motivo_isencao ?? ""} onChange={(e) => set("motivo_isencao", e.target.value)}>
                <option value="">Seleccione…</option>
                {t?.motivos.map((m) => (
                  <option key={m.codigo} value={m.codigo}>
                    {m.codigo} — {m.descricao}
                  </option>
                ))}
              </Select>
            </Field>
          )}
        </div>

        {ramo.campos.length > 0 && (
          <div className="mt-6 rounded-xl border border-line p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <span className="flex items-center gap-2 font-medium">
                <ramo.icone className="size-4 text-brand" /> Dados de {ramo.nome.split(/[,/]/)[0].toLowerCase()}
              </span>
              {ramo.variantes && a.id && (
                <Button variant="secondary" size="sm" onClick={() => setVariantes(true)} title="Criar um artigo por combinação (ex.: tamanho × cor), cada um com o seu stock">
                  <Layers className="size-4" /> Gerar variantes
                </Button>
              )}
            </div>
            <div className="grid grid-cols-4 gap-4">
              {ramo.campos.map((c) =>
                c.tipo === "sim" ? (
                  <label key={c.id} className="col-span-2 flex items-center gap-2 self-end pb-2 text-[13px]">
                    <input type="checkbox" checked={!!atrib[c.id]} onChange={(e) => setAtrib({ ...atrib, [c.id]: e.target.checked })} />
                    {c.label}
                  </label>
                ) : (
                  <Field key={c.id} label={c.label} hint={c.dica} className={c.largo ? "col-span-4" : "col-span-2"}>
                    <Input
                      inputMode={c.tipo === "numero" ? "numeric" : undefined}
                      value={atrib[c.id] === undefined || typeof atrib[c.id] === "boolean" ? "" : String(atrib[c.id])}
                      onChange={(e) => {
                        const v = e.target.value;
                        setAtrib({ ...atrib, [c.id]: c.tipo === "numero" ? (v.trim() === "" ? "" : Number(v.replace(",", ".")) || 0) : v });
                      }}
                    />
                  </Field>
                ),
              )}
            </div>
          </div>
        )}
        {variantes && a.id && <GerarVariantes artigo={a as Artigo} ramo={ramo} onClose={() => setVariantes(false)} />}

        <div className="mt-6 rounded-xl border border-line p-4">
          <div className="mb-3 flex items-center justify-between">
            <span className="font-medium">Preços</span>
            {margemDe(0) !== null && (
              <Badge tone={(margemDe(0) ?? 0) < 0 ? "danger" : "ok"}>Lucro PVP 1: {kz(semIvaDe(0) - custoC)}</Badge>
            )}
          </div>
          <div className="grid grid-cols-5 gap-4">
            <Field label="Custo (s/ IVA)">
              <Input className="num text-right" value={custo} onChange={(e) => setCusto(e.target.value)} />
            </Field>
            {pvp.map((p, i) => (
              <Field key={i} label={`PVP ${i + 1} (c/ IVA)`} hint={i === 0 ? `s/ IVA ${kz(preco1)}` : undefined}>
                <Input
                  className="num text-right"
                  value={p}
                  onChange={(e) => {
                    setPvp((xs) => xs.map((x, j) => (j === i ? e.target.value : x)));
                    setMargemTxt((xs) => xs.map((x, j) => (j === i ? null : x)));
                  }}
                />
              </Field>
            ))}
            <div className="self-center text-[12px] text-ink-3">Margem s/ custo</div>
            {pvp.map((_, i) => {
              const m = margemDe(i);
              return (
                <div key={i} className="flex items-center gap-1.5">
                  <Input
                    className={cn("num h-8 text-right", m !== null && m < 0 && "text-danger")}
                    value={margemTxt[i] ?? (m === null ? "" : m.toFixed(2).replace(".", ","))}
                    placeholder={custoC > 0 ? "—" : "sem custo"}
                    disabled={custoC <= 0}
                    title="Escreva a margem para calcular o PVP a partir do custo"
                    onChange={(e) => aplicarMargem(i, e.target.value)}
                    onBlur={() => setMargemTxt((xs) => xs.map((x, j) => (j === i ? null : x)))}
                  />
                  <span className="text-[12px] text-ink-3">%</span>
                </div>
              );
            })}
          </div>
        </div>

        <div className="mt-4 rounded-xl border border-line p-4">
          <div className="mb-3 flex items-center justify-between">
            <span>
              <span className="font-medium">Outros códigos e balança</span>
              <span className="block text-[12px] text-ink-3">Códigos de embalagens (ex.: o pack de 12 vende 12 unidades) e o código do artigo nas etiquetas da balança.</span>
            </span>
            <Field label="Código na balança (PLU)" className="w-48">
              <Input className="num" inputMode="numeric" value={a.codigo_balanca ?? ""} onChange={(e) => set("codigo_balanca", e.target.value.replace(/\D/g, "").slice(0, 6))} />
            </Field>
          </div>
          {codigos === null ? (
            <p className="text-[13px] text-ink-3">A carregar…</p>
          ) : (
            <div className="space-y-2">
              {codigos.map((c, i) => (
                <div key={i} className="grid grid-cols-[1fr_1fr_110px_36px] items-center gap-2">
                  <Input className="font-mono" placeholder="Código de barras" value={c.codigo} onChange={(e) => setCodigos(codigos.map((x, j) => (j === i ? { ...x, codigo: e.target.value } : x)))} />
                  <Input placeholder="Descrição (ex.: Pack de 12)" value={c.descricao} onChange={(e) => setCodigos(codigos.map((x, j) => (j === i ? { ...x, descricao: e.target.value } : x)))} />
                  <Input className="num text-right" type="number" min={0} step="any" title="Unidades que este código vende" value={c.quantidade} onChange={(e) => setCodigos(codigos.map((x, j) => (j === i ? { ...x, quantidade: Number(e.target.value) } : x)))} />
                  <button type="button" className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" title="Retirar" onClick={() => setCodigos(codigos.filter((_, j) => j !== i))}>
                    ×
                  </button>
                </div>
              ))}
              <Button type="button" size="sm" variant="secondary" onClick={() => setCodigos([...codigos, { codigo: "", descricao: "", quantidade: 1 }])}>
                Acrescentar código
              </Button>
            </div>
          )}
        </div>

        <div className="mt-4 rounded-xl border border-line p-4">
          <div className="mb-3 flex items-center justify-between">
            <span>
              <span className="font-medium">Promoção</span>
              <span className="block text-[12px] text-ink-3">Desconto aplicado automaticamente nas vendas entre as datas, sem pedir autorização.</span>
            </span>
            {promoEstado && <Badge tone={promoEstado.tom}>{promoEstado.txt}</Badge>}
          </div>
          <div className="grid grid-cols-3 gap-4">
            <Field label="Desconto (%)">
              <Input
                className="num text-right"
                type="number"
                min={0}
                max={100}
                step="0.5"
                value={promoPct || ""}
                placeholder="0"
                onChange={(e) => set("promo_desconto", Number(e.target.value))}
              />
            </Field>
            <Field label="De" hint="Vazio = desde já">
              <InputData value={a.promo_inicio ?? ""} disabled={promoPct <= 0} onChange={(e) => set("promo_inicio", e.target.value || null)} />
            </Field>
            <Field label="Até" hint="Vazio = sem fim">
              <InputData value={a.promo_fim ?? ""} disabled={promoPct <= 0} onChange={(e) => set("promo_fim", e.target.value || null)} />
            </Field>
          </div>
        </div>

        {a.tipo === "P" && (
          <div className="mt-4 rounded-xl border border-line p-4">
            <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={!!a.composto} onChange={(e) => set("composto", e.target.checked ? 1 : 0)} />
                <span>
                  <span className="font-medium">Artigo composto (kit)</span>
                  <span className="block text-[12px] text-ink-3">A venda baixa o stock de cada componente.</span>
                </span>
              </label>
              {!a.composto && (
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={!!a.controla_lote} onChange={(e) => set("controla_lote", e.target.checked ? 1 : 0)} />
                  <span>
                    <span className="font-medium">Controla lotes e validades</span>
                    <span className="block text-[12px] text-ink-3">Lote obrigatório nas entradas; saídas pelo que expira primeiro.</span>
                  </span>
                </label>
              )}
              {!a.composto && !!a.controla_lote && (
                <Field label="Avisar validade com (dias)" className="w-44">
                  <Input className="num text-right" type="number" min={0} value={a.aviso_validade_dias ?? 30} onChange={(e) => set("aviso_validade_dias", Number(e.target.value))} />
                </Field>
              )}
            </div>
            {!!a.composto && (
              <div className="mt-4">
                <div className="mb-2 flex items-center gap-2 text-[12px] font-medium uppercase tracking-wide text-ink-2">
                  <Layers className="size-4" /> Composição
                </div>
                <ArtigoPicker
                  placeholder="Adicionar componente…"
                  filtro={(x) => x.id !== a.id && !x.composto && !comps.some((c) => c.componente_id === x.id)}
                  onPick={(x) => setComps((cs) => [...cs, { componente_id: x.id, quantidade: 1, referencia: x.referencia, descricao: x.descricao, custo_medio: x.custo_medio, unidade: x.unidade }])}
                />
                {comps.length > 0 && (
                  <table className="mt-3 w-full text-left">
                    <tbody>
                      {comps.map((c) => (
                        <tr key={c.componente_id} className="border-b border-line/70">
                          <td className="py-1.5">
                            <span className="font-medium">{c.descricao}</span>
                            <span className="ml-2 font-mono text-[11.5px] text-ink-3">{c.referencia}</span>
                          </td>
                          <td className="w-32 py-1.5">
                            <Input
                              className="num h-8 text-right"
                              type="number"
                              min={0}
                              step="any"
                              value={c.quantidade}
                              onChange={(e) => setComps((cs) => cs.map((x) => (x.componente_id === c.componente_id ? { ...x, quantidade: Number(e.target.value) } : x)))}
                            />
                          </td>
                          <td className="w-16 py-1.5 pl-2 text-[12px] text-ink-3">{c.unidade ?? "UN"}</td>
                          <td className="num w-32 py-1.5 text-right text-ink-2">{kz(Math.round((c.custo_medio ?? 0) * c.quantidade))}</td>
                          <td className="w-10 py-1.5 text-right">
                            <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setComps((cs) => cs.filter((x) => x.componente_id !== c.componente_id))}>
                              <Trash2 className="size-4" />
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
                {comps.length > 0 && <div className="mt-2 text-right text-[13px] text-ink-2">Custo dos componentes (custo médio): <b className="num">{kz(custoKit)}</b></div>}
              </div>
            )}
          </div>
        )}

        {a.tipo === "P" && !a.composto && (
          <div className="mt-4 grid grid-cols-4 items-end gap-4">
            <Field label="Tipo de inventário (SAF-T)" className="col-span-4 sm:col-span-2" hint="Como o artigo entra no SAF-T de inventários da AGT">
              <Select value={a.tipo_inventario ?? "M"} onChange={(e) => set("tipo_inventario", e.target.value)}>
                {TIPOS_INVENTARIO.map(([c, nome]) => (
                  <option key={c} value={c}>
                    {c} · {nome}
                  </option>
                ))}
              </Select>
            </Field>
            <div className="hidden sm:col-span-2 sm:block" />
            <Field label="Stock mínimo">
              <Input className="num text-right" type="number" value={a.qtd_minima ?? 0} onChange={(e) => set("qtd_minima", Number(e.target.value))} />
            </Field>
            <Field label="Stock máximo">
              <Input className="num text-right" type="number" value={a.qtd_maxima ?? 0} onChange={(e) => set("qtd_maxima", Number(e.target.value))} />
            </Field>
            <label className="flex h-10 items-center gap-2">
              <input type="checkbox" checked={!!a.controla_stock} onChange={(e) => set("controla_stock", e.target.checked ? 1 : 0)} /> Controla stock
            </label>
            <label className="flex h-10 items-center gap-2">
              <input type="checkbox" checked={!!a.permite_negativo} onChange={(e) => set("permite_negativo", e.target.checked ? 1 : 0)} /> Permite stock negativo
            </label>
          </div>
        )}
        {a.id && (
          <label className="mt-4 flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={a.ativo !== 0} onChange={(e) => set("ativo", e.target.checked ? 1 : 0)} /> Artigo activo
          </label>
        )}
      </AbasFicha>
    </Modal>
  );
}

/** Moda e calçado: um artigo por combinação (ex.: 6 tamanhos × 3 cores = 18), cada um com o seu stock. */
function GerarVariantes({ artigo, ramo, onClose }: { artigo: Artigo; ramo: Ramo; onClose: () => void }) {
  const qc = useQueryClient();
  const eixos = ramo.variantes ?? [];
  const [sel, setSel] = useState<Record<string, string[]>>(() => Object.fromEntries(eixos.map((e) => [e.nome, []])));
  const [extra, setExtra] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const valores = (nome: string) => [...(sel[nome] ?? []), ...(extra[nome] ?? "").split(",").map((x) => x.trim()).filter(Boolean)];
  const total = eixos.map((e) => valores(e.nome).length).filter((n) => n > 0).reduce((a, b) => a * b, 1);
  const algum = eixos.some((e) => valores(e.nome).length > 0);

  async function gerar() {
    setBusy(true);
    try {
      const r = await call<{ criados: { referencia: string }[]; existentes: number }>("artigos.variantes", {
        artigo_id: artigo.id,
        eixos: eixos.map((e) => ({ nome: e.nome, valores: valores(e.nome) })),
      });
      toast.success(`${r.criados.length} ${r.criados.length === 1 ? "variante criada" : "variantes criadas"}${r.existentes ? ` (${r.existentes} já existiam)` : ""}.`, {
        description: r.criados.slice(0, 4).map((c) => c.referencia).join(", ") + (r.criados.length > 4 ? "…" : ""),
      });
      qc.invalidateQueries({ queryKey: ["artigos"] });
      qc.invalidateQueries({ queryKey: ["pos-artigos"] });
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
      title={`Variantes de ${artigo.referencia}`}
      width="max-w-xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!algum} onClick={gerar}>
            Criar {algum ? total : ""} {total === 1 ? "artigo" : "artigos"}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-[13px] text-ink-2">
        Cada combinação passa a ser um artigo (ex.: <b className="font-mono">{artigo.referencia}-M-AZUL</b>, «{artigo.descricao} M / Azul»), com o preço, o IVA e a família
        deste e o seu próprio stock. As que já existem não se repetem.
      </p>
      {eixos.map((e) => (
        <div key={e.nome} className="mb-4">
          <div className="mb-2 text-[13px] font-medium">{e.label}</div>
          <div className="flex flex-wrap gap-1.5">
            {e.sugestoes.map((v) => {
              const on = sel[e.nome]?.includes(v);
              return (
                <button
                  key={v}
                  type="button"
                  onClick={() => setSel({ ...sel, [e.nome]: on ? sel[e.nome].filter((x) => x !== v) : [...(sel[e.nome] ?? []), v] })}
                  className={cn("min-w-11 rounded-lg border px-2.5 py-1.5 text-[13px]", on ? "border-brand bg-brand text-white" : "border-line-strong bg-surface hover:border-brand/50")}
                >
                  {v}
                </button>
              );
            })}
          </div>
          <Input className="mt-2" placeholder="Outros, separados por vírgulas" value={extra[e.nome] ?? ""} onChange={(x) => setExtra({ ...extra, [e.nome]: x.target.value })} />
        </div>
      ))}
    </Modal>
  );
}

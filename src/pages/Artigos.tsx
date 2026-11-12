import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Copy, Layers, Package, Plus, Search, Tags, Trash2 } from "lucide-react";
import { Link } from "react-router-dom";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { comIva, kz, parseKz, qtd, semIva, toInputKz } from "@/lib/format";
import type { Artigo, Componente } from "@/lib/types";
import { ArtigoPicker } from "@/components/Pickers";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

export default function Artigos() {
  const [q, setQ] = useState("");
  const [familia, setFamilia] = useState<string>("");
  const [editar, setEditar] = useState<Partial<Artigo> | null>(null);
  const [origem, setOrigem] = useState<number | undefined>();
  const dq = useDebounce(q);
  const tabelas = useTabelas();
  const tem = useSession((s) => s.tem);
  const lista = useQuery({
    queryKey: ["artigos", dq, familia],
    queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, familia_id: familia ? Number(familia) : null, inativos: true }),
  });

  return (
    <>
      <PageHeader
        title="Artigos"
        subtitle="Produtos e serviços, preços e IVA"
        actions={
          <div className="flex gap-2">
            <Link to="/etiquetas">
              <Button variant="secondary">
                <Tags className="size-4" /> Etiquetas
              </Button>
            </Link>
            {tem("artigos.criar") && (
              <Button
                onClick={() => {
                  setOrigem(undefined);
                  setEditar({ tipo: "P", taxa_iva_id: 1, unidade_id: 1, familia_id: 1, controla_stock: 1, ativo: 1 });
                }}
              >
                <Plus className="size-4" /> Novo artigo
              </Button>
            )}
          </div>
        }
      />
      <Card>
        <div className="flex gap-3 border-b border-line p-4">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Pesquisar por descrição, referência ou código de barras…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Select className="w-56" value={familia} onChange={(e) => setFamilia(e.target.value)}>
            <option value="">Todas as famílias</option>
            {tabelas.data?.familias.map((f) => (
              <option key={f.id} value={f.id}>
                {f.descricao}
              </option>
            ))}
          </Select>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<Package className="size-6" />} title="Sem artigos" text="Crie o primeiro artigo para começar a vender." />
        ) : (
          <Table className="max-h-[calc(100vh-260px)]">
            <thead>
              <tr>
                <Th>Referência</Th>
                <Th>Descrição</Th>
                <Th>Família</Th>
                <Th className="text-right">IVA</Th>
                <Th className="text-right">Custo médio</Th>
                <Th className="text-right">PVP (c/ IVA)</Th>
                <Th className="text-right">Stock</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((a) => (
                <tr key={a.id} className={cn("cursor-pointer hover:bg-sunken/60", !a.ativo && "opacity-50")} onClick={() => {
                    if (!tem("artigos.editar")) return;
                    setOrigem(undefined);
                    setEditar(a);
                  }}>
                  <Td className="font-mono text-[12.5px] text-ink-2">{a.referencia}</Td>
                  <Td>
                    <div className="font-medium">
                      {a.descricao}
                      {!!a.composto && <Badge tone="brand" className="ml-2">Kit</Badge>}
                      {!!a.controla_lote && <Badge tone="accent" className="ml-2">Lotes</Badge>}
                    </div>
                    {a.codigo_barras && <div className="font-mono text-[11.5px] text-ink-3">{a.codigo_barras}</div>}
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
          key={`${editar.id ?? "novo"}-${origem ?? ""}`}
          inicial={editar}
          origem={origem}
          onClose={() => setEditar(null)}
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
    </>
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
}: {
  inicial: Partial<Artigo>;
  origem?: number;
  onClose: () => void;
  onClonar?: (a: Partial<Artigo>) => void;
}) {
  const t = useTabelas().data;
  const qc = useQueryClient();
  const [a, setA] = useState<Partial<Artigo>>(inicial);
  const taxa = t?.taxas.find((x) => x.id === a.taxa_iva_id)?.percentagem ?? 0;
  const [pvp, setPvp] = useState(() => [a.preco1, a.preco2, a.preco3, a.preco4].map((p) => toInputKz(comIva(p ?? 0, taxa))));
  const [custo, setCusto] = useState(toInputKz(a.custo ?? 0));
  const [busy, setBusy] = useState(false);
  const [comps, setComps] = useState<Componente[]>([]);
  useEffect(() => {
    const fonte = inicial.id ?? origem;
    if (fonte && inicial.composto) call<Componente[]>("artigos.componentes", { id: fonte }).then(setComps).catch(() => {});
  }, [inicial.id, origem, inicial.composto]);
  const custoKit = comps.reduce((s, c) => s + Math.round((c.custo_medio ?? 0) * c.quantidade), 0);
  const set = (k: keyof Artigo, v: unknown) => setA((x) => ({ ...x, [k]: v }));

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

  async function guardar() {
    setBusy(true);
    try {
      const precos = pvp.map((p) => semIva(parseKz(p), taxa));
      await call("artigos.guardar", {
        ...a,
        custo: parseKz(custo),
        preco1: precos[0],
        preco2: precos[1],
        preco3: precos[2],
        preco4: precos[3],
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
      });
      toast.success("Artigo guardado.");
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
      title={a.id ? `Editar ${a.referencia}` : origem ? "Novo artigo (clonado)" : "Novo artigo"}
      width="max-w-3xl"
      footer={
        <>
          {a.id && onClonar && (
            <Button variant="ghost" className="mr-auto" onClick={() => onClonar({ ...a, custo: parseKz(custo) })}>
              <Copy className="size-4" /> Clonar
            </Button>
          )}
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={guardar}>
            Guardar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-4 gap-4">
        <Field label="Descrição" className="col-span-3">
          <Input autoFocus value={a.descricao ?? ""} onChange={(e) => set("descricao", e.target.value)} />
        </Field>
        <Field label="Tipo">
          <Select value={a.tipo} onChange={(e) => set("tipo", e.target.value)}>
            <option value="P">Produto</option>
            <option value="S">Serviço</option>
          </Select>
        </Field>
        <Field label="Referência" hint={a.id ? undefined : "Vazio = automática"}>
          <Input value={a.referencia ?? ""} onChange={(e) => set("referencia", e.target.value)} />
        </Field>
        <Field label="Código de barras">
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
            <Input type="date" value={a.promo_inicio ?? ""} disabled={promoPct <= 0} onChange={(e) => set("promo_inicio", e.target.value || null)} />
          </Field>
          <Field label="Até" hint="Vazio = sem fim">
            <Input type="date" value={a.promo_fim ?? ""} disabled={promoPct <= 0} onChange={(e) => set("promo_fim", e.target.value || null)} />
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
    </Modal>
  );
}

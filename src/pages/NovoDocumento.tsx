import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ArrowLeft, Coins, FileSearch, KeyRound, NotebookPen, Plus, Trash2, Truck, Wrench } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data, hojeISO, kz, kzNum, parseKz, qtd, toInputKz } from "@/lib/format";
import type { Artigo, DocCompleto, DocResumo, Entidade, Lote, Moeda, TipoDocumento, Transporte } from "@/lib/types";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import DocumentoModal from "@/components/DocumentoModal";
import { ArtigoPicker, EntidadePicker } from "@/components/Pickers";
import { useSession } from "@/store/session";
import type { Serie } from "@/pages/Series";

interface L {
  key: string;
  artigo_id: number;
  referencia: string;
  descricao: string;
  quantidade: number;
  preco: string; // s/ IVA, texto editável
  desconto: number;
  taxa: number;
  unidade: string | null;
  existencia: number; // inventário
  controla_lote: boolean;
  lote: string;
  validade: string;
}

interface Custo {
  key: string;
  tipo: string;
  descricao: string;
  valor: string;
}

interface Auxiliares {
  prazos: { codigo: string; descricao: string; dias: number }[];
  retencoes: { codigo: string; descricao: string; percentagem: number; base: string }[];
  moedas: Moeda[];
  tipos_custo: { codigo: string; descricao: string }[];
}

interface OrdemServico {
  id: number;
  numero: string;
  entidade_id: number | null;
  matricula: string | null;
  marca: string;
  modelo: string | null;
  estado: string;
  linhas: { artigo_id: number | null; descricao: string; quantidade: number }[];
}

const DEDUCOES = [
  { codigo: "", descricao: "Automático (pelo tipo de artigo)" },
  { codigo: "INV", descricao: "INV — Existências / inventário" },
  { codigo: "MFI", descricao: "MFI — Meios fixos e investimentos" },
  { codigo: "OBC", descricao: "OBC — Outros bens de consumo" },
  { codigo: "SERV", descricao: "SERV — Serviços" },
  { codigo: "IMPT", descricao: "IMPT — Importação" },
  { codigo: "SCE", descricao: "SCE — Serviços contratados no estrangeiro" },
];

const somarDias = (iso: string, dias: number) => {
  const d = new Date(iso + "T12:00:00");
  d.setDate(d.getDate() + dias);
  return d.toISOString().slice(0, 10);
};

export default function NovoDocumento() {
  const { tipo: codigo = "FT" } = useParams();
  const [sp, setSp] = useSearchParams();
  const origemId = sp.get("origem") ? Number(sp.get("origem")) : null;
  const ordemId = sp.get("ordem") ? Number(sp.get("ordem")) : null;
  const nav = useNavigate();
  const qc = useQueryClient();
  const t = useTabelas().data;
  const tipo: TipoDocumento | undefined = t?.tipos_documento.find((x) => x.codigo === codigo);
  const aux = useQuery({ queryKey: ["doc-auxiliares"], queryFn: () => call<Auxiliares>("documentos.auxiliares"), staleTime: 300_000 }).data;

  const grupo = tipo?.grupo ?? "VENDAS";
  const eStock = grupo === "STOCK";
  const eCompra = grupo === "COMPRAS";
  const inventario = tipo?.stock === 3;
  const transporte = ["GT", "VGT"].includes(codigo);
  const pedeMotivo = !!tipo?.credito || codigo === "ND" || codigo === "VND";
  const pedeVencimento = (tipo?.conta_corrente ?? 0) > 0 || ["PP", "OR", "NE", "VNE", "VOR"].includes(codigo);

  const [entidade, setEntidade] = useState<Entidade | null>(null);
  const [linhas, setLinhas] = useState<L[]>([]);
  const [armazem, setArmazem] = useState<number>(t?.armazem_id ?? 1);
  const [destino, setDestino] = useState<number | "">("");
  const [prazo, setPrazo] = useState("PP");
  const [venc, setVenc] = useState(hojeISO());
  // Validade do tipo de documento (Parametrização): o «válido até» das proformas e orçamentos.
  useEffect(() => {
    if (tipo?.validade_dias) setVenc(somarDias(hojeISO(), tipo.validade_dias));
  }, [tipo?.codigo, tipo?.validade_dias]);
  const [referencia, setReferencia] = useState("");
  const [docExterno, setDocExterno] = useState("");
  const [motivo, setMotivo] = useState("");
  const [obs, setObs] = useState("");
  const [descGlobal, setDescGlobal] = useState(0);
  const [retencoes, setRetencoes] = useState<string[]>([]);
  const [transp, setTransp] = useState<Transporte>({});
  const [pagModo, setPagModo] = useState("NU");
  const [busy, setBusy] = useState(false);
  const [emitido, setEmitido] = useState<number | null>(null);
  const [escolherOrigem, setEscolherOrigem] = useState(false);
  const [origemDoc, setOrigemDoc] = useState<string | null>(null);
  // compras: documento do fornecedor, moeda e custos de importação
  const [docHash, setDocHash] = useState("");
  const [docData, setDocData] = useState("");
  const [deducao, setDeducao] = useState("");
  const [moeda, setMoeda] = useState("AOA");
  const [cambio, setCambio] = useState("1");
  const [custos, setCustos] = useState<Custo[]>([]);
  const [ordem, setOrdem] = useState<OrdemServico | null>(null);
  // série, documento manual (recuperação), vendedor, local de entrega e PIN de supervisor
  const tem = useSession((x) => x.tem);
  const [serie, setSerie] = useState("");
  const [manual, setManual] = useState(false);
  const [manualSerie, setManualSerie] = useState("");
  const [manualNumero, setManualNumero] = useState("");
  const [manualData, setManualData] = useState("");
  const [vendedor, setVendedor] = useState("");
  const [entrega, setEntrega] = useState("");
  const [pinPedido, setPinPedido] = useState<string | null>(null);
  const [pin, setPin] = useState("");
  const series = useQuery({ queryKey: ["series", codigo], queryFn: () => call<Serie[]>("series.listar", { tipo: codigo }), staleTime: 60_000 }).data ?? [];
  const vendedores = useQuery({ queryKey: ["vendedores"], queryFn: () => call<{ id: number; nome: string }[]>("rh.vendedores"), staleTime: 300_000 }).data ?? [];
  const enderecos = useQuery({
    queryKey: ["enderecos", entidade?.id],
    queryFn: () => call<{ id: number; descricao: string; endereco: string; localidade: string | null }[]>("entidades.enderecos", { id: entidade?.id }),
    enabled: !!entidade && entidade.id !== 1,
  }).data ?? [];

  // ao mudar de tipo, limpa o formulário
  useEffect(() => {
    setLinhas([]);
    setEntidade(null);
    setMotivo("");
    setDocExterno("");
    setDocHash("");
    setDocData("");
    setDeducao("");
    setMoeda("AOA");
    setCambio("1");
    setCustos([]);
    setRetencoes([]);
    setOrigemDoc(null);
    setSerie("");
    setManual(false);
    setManualNumero("");
    setManualData("");
    setEntrega("");
  }, [codigo]);

  // Facturar uma ordem de serviço da oficina (?ordem=id)
  useEffect(() => {
    if (!ordemId) return;
    (async () => {
      const o = await call<OrdemServico>("oficina.ordem", { id: ordemId });
      setOrdem(o);
      if (o.entidade_id) setEntidade(await call<Entidade>("entidades.obter", { id: o.entidade_id }));
      setReferencia(`${o.numero}${o.matricula ? ` · ${o.matricula}` : ""}`);
      const ids = o.linhas.map((l) => l.artigo_id).filter((x): x is number => !!x);
      const arts = ids.length ? await call<Artigo[]>("artigos.porIds", { ids }) : [];
      setLinhas(
        o.linhas
          .filter((l) => arts.some((x) => x.id === l.artigo_id))
          .map((l) => {
            const a = arts.find((x) => x.id === l.artigo_id)!;
            return {
              key: crypto.randomUUID(),
              artigo_id: a.id,
              referencia: a.referencia,
              descricao: l.descricao || a.descricao,
              quantidade: l.quantidade,
              preco: toInputKz(a.preco1),
              desconto: 0,
              taxa: a.taxa_pct,
              unidade: a.unidade,
              existencia: a.stock,
              controla_lote: !!a.controla_lote,
              lote: "",
              validade: "",
            };
          }),
      );
      const semArtigo = o.linhas.filter((l) => !arts.some((x) => x.id === l.artigo_id)).length;
      if (semArtigo) toast.warning(`${semArtigo} linha(s) da OS sem artigo associado não foram copiadas.`);
    })().catch((e) => toast.error((e as Error).message));
  }, [ordemId]);

  // Documento de origem (NC / conversão de proforma, encomenda, orçamento ou guia)
  useEffect(() => {
    if (!origemId) return;
    call<DocCompleto>("documentos.obter", { id: origemId }).then(async (d) => {
      if (d.documento.entidade_id) setEntidade(await call<Entidade>("entidades.obter", { id: d.documento.entidade_id }));
      setOrigemDoc(d.documento.numero_doc);
      // o controlo de lotes vem da ficha do artigo (o documento de origem pode não ter lote, ex.: histórico)
      const ids = [...new Set(d.linhas.map((l) => l.artigo_id).filter(Boolean))];
      const comLote = new Set((ids.length ? await call<Artigo[]>("artigos.porIds", { ids }) : []).filter((a) => a.controla_lote && !a.composto).map((a) => a.id));
      setLinhas(
        d.linhas.map((l) => ({
          key: crypto.randomUUID(),
          artigo_id: l.artigo_id,
          referencia: l.referencia,
          descricao: l.descricao,
          quantidade: l.quantidade,
          preco: toInputKz(l.preco_unitario),
          desconto: l.desconto_pct,
          taxa: l.taxa_iva_pct,
          unidade: l.unidade,
          existencia: 0,
          controla_lote: comLote.has(l.artigo_id),
          lote: l.lote && !l.lote.includes("(") ? l.lote : "",
          validade: l.validade ?? "",
        })),
      );
    });
  }, [origemId]);

  const moedaSel = aux?.moedas.find((m) => m.codigo === moeda);
  const estrangeira = eCompra && moeda !== "AOA";
  const fator = estrangeira ? Number(cambio.replace(",", ".")) || 0 : 1;

  const saldo = useQuery({
    queryKey: ["saldo-entidade", entidade?.id, grupo],
    queryFn: () => call<{ documentos: { valor_pendente: number }[] }>("contas.pendentes", { tipo: eCompra ? "F" : "C", entidade_id: entidade?.id }),
    enabled: !!entidade && entidade.id !== 1 && !eStock,
  }).data?.documentos.reduce((s, d) => s + d.valor_pendente, 0);

  const calc = useMemo(
    () =>
      linhas.map((l) => {
        const bruto = Math.round(l.quantidade * Math.round(parseKz(l.preco) * fator));
        const pct = 100 - ((100 - l.desconto) * (100 - descGlobal)) / 100;
        const descLinha = Math.round((bruto * l.desconto) / 100);
        const desc = Math.round((bruto * pct) / 100);
        const liq = bruto - desc;
        const iva = eStock ? 0 : Math.round((liq * l.taxa) / 100);
        return { bruto, descLinha, descGlobal: desc - descLinha, desc, liq, iva, total: liq + iva };
      }),
    [linhas, descGlobal, eStock, fator],
  );
  const tot = calc.reduce(
    (s, c) => ({ bruto: s.bruto + c.bruto, descLinha: s.descLinha + c.descLinha, descGlobal: s.descGlobal + c.descGlobal, liq: s.liq + c.liq, iva: s.iva + c.iva, total: s.total + c.total }),
    { bruto: 0, descLinha: 0, descGlobal: 0, liq: 0, iva: 0, total: 0 },
  );
  const retencaoValor = retencoes.reduce((s, cod) => {
    const r = aux?.retencoes.find((x) => x.codigo === cod);
    if (!r) return s;
    return s + Math.round(((r.base === "IVA" ? tot.iva : tot.liq) * r.percentagem) / 100);
  }, 0);
  const aPagar = tot.total - retencaoValor;
  const totalCustos = custos.reduce((s, c) => s + parseKz(c.valor), 0);
  const podeManual = !!tipo?.assinado && grupo === "VENDAS" && ["VENDA", "TRANSPORTE"].includes(tipo?.classe ?? "") && tem("vendas.documento_manual");
  const seriesNormais = series.filter((x) => x.ativa && !x.manual);
  const seriesManuais = series.filter((x) => x.ativa && x.manual);
  const entradaStock = tipo?.stock === 1 || tipo?.stock === 3;

  function adicionar(a: Artigo) {
    const base = eCompra ? a.custo : eStock ? a.custo_medio : [a.preco1, a.preco2, a.preco3, a.preco4][(entidade?.nivel_preco ?? 1) - 1] || a.preco1;
    const preco = estrangeira && fator > 0 ? Math.round(base / fator) : base;
    // «Duplicar Linha» desligado no tipo: o mesmo artigo soma à linha existente.
    if (tipo && !tipo.repetir_linhas && !inventario) {
      const existente = linhas.find((l) => l.artigo_id === a.id && !l.lote);
      if (existente) {
        upd(existente.key, { quantidade: +(Number(existente.quantidade) + 1).toFixed(3) });
        return;
      }
    }
    setLinhas((ls) => [
      ...ls,
      {
        key: crypto.randomUUID(),
        artigo_id: a.id,
        referencia: a.referencia,
        descricao: a.descricao,
        quantidade: inventario ? a.stock : 1,
        preco: toInputKz(preco),
        desconto: eCompra || eStock ? 0 : Math.max(entidade?.desconto ?? 0, a.promo_atual ?? 0),
        taxa: a.taxa_pct,
        unidade: a.unidade,
        existencia: a.stock,
        controla_lote: !!a.controla_lote && !a.composto,
        lote: "",
        validade: "",
      },
    ]);
  }
  const upd = (k: string, patch: Partial<L>) => setLinhas((ls) => ls.map((l) => (l.key === k ? { ...l, ...patch } : l)));

  async function emitir(autorizacao?: string) {
    setBusy(true);
    try {
      const d = await call<DocCompleto>("documentos.emitir", {
        tipo: codigo,
        serie: manual ? manualSerie || null : serie || null,
        manual: manual ? { numero: manualNumero, data: manualData } : null,
        vendedor_id: vendedor ? Number(vendedor) : null,
        endereco_entrega: entrega || null,
        autorizacao_pin: autorizacao || null,
        entidade_id: eStock ? null : entidade?.id ?? (eCompra ? null : 1),
        armazem_id: armazem,
        armazem_destino_id: codigo === "TS" ? destino || null : null,
        data_vencimento: pedeVencimento ? venc : null,
        prazo: pedeVencimento ? prazo : null,
        referencia: referencia || null,
        doc_externo: docExterno || null,
        doc_externo_hash: eCompra && docHash ? docHash : null,
        doc_externo_data: eCompra && docData ? docData : null,
        deducao_tipo: eCompra && deducao ? deducao : null,
        moeda: eCompra ? moeda : null,
        cambio: estrangeira ? fator : null,
        custos: eCompra ? custos.filter((c) => parseKz(c.valor) > 0).map((c) => ({ tipo: c.tipo, descricao: c.descricao || null, valor: parseKz(c.valor) })) : [],
        motivo: motivo || null,
        doc_origem_id: origemId,
        observacoes: obs || null,
        desconto_global_pct: descGlobal || null,
        retencoes: retencoes.map((codigo) => ({ codigo })),
        transporte: transporte ? transp : null,
        linhas: linhas.map((l) => ({
          artigo_id: l.artigo_id,
          descricao: l.descricao,
          quantidade: l.quantidade,
          preco_unitario: parseKz(l.preco),
          desconto_pct: l.desconto,
          lote: l.controla_lote && l.lote.trim() ? l.lote.trim() : null,
          validade: l.controla_lote && l.validade ? l.validade : null,
        })),
        pagamentos: tipo?.pago_no_acto ? [{ modo: pagModo, valor: aPagar }] : [],
      });
      if (ordem) {
        await call("oficina.associar", { ordem_id: ordem.id, documento_id: d.documento.id }).catch((e) =>
          toast.error(`Documento emitido, mas a OS não foi associada: ${(e as Error).message}`),
        );
      }
      toast.success(`${d.documento.numero_doc} emitido.`);
      qc.invalidateQueries();
      setPinPedido(null);
      setPin("");
      setEmitido(d.documento.id);
    } catch (e) {
      const msg = (e as Error).message;
      if (/supervisor \(PIN\)/.test(msg) && !autorizacao) {
        setPinPedido(msg);
      } else {
        toast.error(msg);
      }
    } finally {
      setBusy(false);
    }
  }

  if (!tipo) {
    return <div className="py-20 text-center text-ink-3">A carregar…</div>;
  }

  return (
    <>
      <PageHeader
        title={tipo.descricao}
        subtitle={`${{ VENDAS: "Vendas", COMPRAS: "Compras", STOCK: "Gestão de Stock" }[grupo]} · ${tipo.codigo}${tipo.assinado ? " · documento certificado" : ""}`}
        actions={
          <Button variant="ghost" onClick={() => nav(-1)}>
            <ArrowLeft className="size-4" /> Voltar
          </Button>
        }
      />

      <div className="grid grid-cols-[minmax(0,1fr)_330px] gap-4">
        <div className="space-y-4">
          {/* Cabeçalho do documento */}
          <Card className="p-5">
            <div className="grid grid-cols-4 items-end gap-4">
              {!eStock && (
                <div className="col-span-2">
                  <EntidadePicker tipo={eCompra ? "F" : "C"} valor={entidade} onChange={setEntidade} bloqueado={!!origemId && !!tipo.credito} />
                </div>
              )}
              <Field label="Data">
                <Input value={data(manual && manualData ? manualData : hojeISO())} disabled />
              </Field>
              <Field label={codigo === "TS" ? "Armazém de origem" : "Armazém"}>
                <Select value={armazem} onChange={(e) => setArmazem(Number(e.target.value))}>
                  {t?.armazens.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.nome}
                    </option>
                  ))}
                </Select>
              </Field>
              {codigo === "TS" && (
                <Field label="Armazém de destino">
                  <Select value={destino} onChange={(e) => setDestino(e.target.value ? Number(e.target.value) : "")}>
                    <option value="">Seleccione…</option>
                    {t?.armazens.filter((a) => a.id !== armazem).map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.nome}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              {pedeVencimento && (
                <>
                  <Field label="Prazo de pagamento">
                    <Select
                      value={prazo}
                      onChange={(e) => {
                        setPrazo(e.target.value);
                        const p = aux?.prazos.find((x) => x.codigo === e.target.value);
                        if (p) setVenc(somarDias(hojeISO(), p.dias));
                      }}
                    >
                      {aux?.prazos.map((p) => (
                        <option key={p.codigo} value={p.codigo}>
                          {p.descricao}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field label={["PP", "OR", "VOR"].includes(codigo) ? "Válido até" : "Data de vencimento"}>
                    <Input type="date" value={venc} onChange={(e) => setVenc(e.target.value)} />
                  </Field>
                </>
              )}
              {!eStock && (
                <Field label={eCompra ? "Referência" : "V/ Referência"}>
                  <Input value={referencia} placeholder="Requisição, obra…" onChange={(e) => setReferencia(e.target.value)} />
                </Field>
              )}
              {!manual && seriesNormais.length > 0 && (
                <Field label="Série">
                  <Select value={serie} onChange={(e) => setSerie(e.target.value)}>
                    <option value="">{seriesNormais.find((x) => x.padrao) ? `Por defeito (${seriesNormais.find((x) => x.padrao)?.serie})` : "Automática"}</option>
                    {seriesNormais.map((x) => (
                      <option key={x.id} value={x.serie}>
                        {x.serie} — {x.descricao}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              {grupo === "VENDAS" && vendedores.length > 0 && (
                <Field label="Vendedor">
                  <Select value={vendedor} onChange={(e) => setVendedor(e.target.value)}>
                    <option value="">—</option>
                    {vendedores.map((v) => (
                      <option key={v.id} value={v.id}>
                        {v.nome}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
              {grupo === "VENDAS" && entidade && entidade.id !== 1 && (
                <Field label="Local de entrega" className="col-span-2">
                  <Input
                    list="entregas"
                    value={entrega}
                    placeholder={enderecos.length ? "Escolha um endereço da ficha ou escreva…" : "Morada de entrega (se diferente)"}
                    onChange={(e) => setEntrega(e.target.value)}
                  />
                  <datalist id="entregas">
                    {enderecos.map((x) => (
                      <option key={x.id} value={[x.endereco, x.localidade].filter(Boolean).join(", ")}>
                        {x.descricao}
                      </option>
                    ))}
                  </datalist>
                </Field>
              )}
              {!!tipo.externo && (
                <Field label="N.º documento do fornecedor">
                  <Input value={docExterno} placeholder="Ex.: FT A/123" onChange={(e) => setDocExterno(e.target.value)} />
                </Field>
              )}
              {!!tipo.externo && eCompra && (
                <>
                  <Field label="Data do documento" hint="Vazio = data de hoje">
                    <Input type="date" max={hojeISO()} value={docData} onChange={(e) => setDocData(e.target.value)} />
                  </Field>
                  <Field label="Assinatura" hint="4 caracteres impressos no documento (SAF-T)">
                    <Input
                      className="font-mono uppercase"
                      maxLength={4}
                      value={docHash}
                      placeholder="Ex.: A1b2"
                      onChange={(e) => setDocHash(e.target.value.replace(/\s/g, ""))}
                    />
                  </Field>
                  <Field label="Dedução do IVA">
                    <Select value={deducao} onChange={(e) => setDeducao(e.target.value)}>
                      {DEDUCOES.map((d) => (
                        <option key={d.codigo} value={d.codigo}>
                          {d.descricao}
                        </option>
                      ))}
                    </Select>
                  </Field>
                </>
              )}
              {pedeMotivo && (
                <Field label="Motivo de emissão" className="col-span-4">
                  <Input value={motivo} placeholder="Ex.: Devolução de mercadoria, correcção de preço…" onChange={(e) => setMotivo(e.target.value)} />
                </Field>
              )}
            </div>

            {podeManual && !origemId && (
              <div className={cn("mt-4 rounded-xl border px-4 py-3", manual ? "border-accent/50 bg-accent-50" : "border-line")}>
                <label className="flex items-center gap-2 font-medium">
                  <input type="checkbox" checked={manual} onChange={(e) => setManual(e.target.checked)} />
                  <NotebookPen className="size-4 text-[#9a6514]" /> Recuperar documento emitido à mão
                </label>
                {manual && (
                  <div className="mt-3 grid grid-cols-3 gap-4">
                    <Field label="Série de documentos manuais">
                      <Select value={manualSerie} onChange={(e) => setManualSerie(e.target.value)}>
                        <option value="">Seleccione…</option>
                        {seriesManuais.map((x) => (
                          <option key={x.id} value={x.serie}>
                            {x.serie} — {x.descricao}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field label="N.º do documento manual">
                      <Input value={manualNumero} placeholder="N.º impresso no livro" onChange={(e) => setManualNumero(e.target.value)} />
                    </Field>
                    <Field label="Data do documento manual">
                      <Input type="date" max={hojeISO()} value={manualData} onChange={(e) => setManualData(e.target.value)} />
                    </Field>
                    {!seriesManuais.length && (
                      <p className="col-span-3 text-[12.5px] text-[#9a6514]">Crie primeiro uma «série de documentos manuais» para {tipo.codigo} em Sistema → Séries de Documentos.</p>
                    )}
                  </div>
                )}
              </div>
            )}

            {ordem && (
              <div className="mt-4 flex items-center gap-3 rounded-xl bg-brand-50 px-4 py-3 text-brand-700">
                <Wrench className="size-4" />
                <span>
                  Factura da ordem de serviço <b>{ordem.numero}</b> — {ordem.marca} {ordem.modelo ?? ""} {ordem.matricula ? `· ${ordem.matricula}` : ""}
                </span>
                <span className="ml-auto text-[12px]">Ao gravar, a OS fica marcada como facturada.</span>
              </div>
            )}

            {(!!tipo.credito || origemDoc) && (
              <div className="mt-4 flex items-center gap-3 rounded-xl bg-sunken px-4 py-3">
                <FileSearch className="size-4 text-ink-3" />
                <span className="text-ink-2">Documento de origem:</span>
                {origemDoc ? <Badge tone="brand">{origemDoc}</Badge> : <span className="text-ink-3">nenhum</span>}
                {!!tipo.credito && (
                  <Button size="sm" variant="secondary" className="ml-auto" disabled={!entidade} onClick={() => setEscolherOrigem(true)}>
                    Escolher documento…
                  </Button>
                )}
              </div>
            )}

            {entidade && entidade.id !== 1 && (
              <div className="mt-4 grid grid-cols-4 gap-4 rounded-xl border border-line px-4 py-3 text-[13px]">
                <Info k="Nome" v={entidade.nome} />
                <Info k="Contribuinte" v={entidade.nif} />
                <Info k="Morada" v={[entidade.endereco, entidade.localidade].filter(Boolean).join(", ") || "—"} />
                <Info k="Saldo em conta corrente" v={kz(saldo ?? 0)} />
              </div>
            )}
          </Card>

          {/* Linhas */}
          <Card>
            <div className="border-b border-line p-4">
              <ArtigoPicker onPick={adicionar} />
            </div>
            <Table className="tabela-compacta">
              <thead>
                <tr>
                  <Th className="min-w-[160px]">Referência / Descrição</Th>
                  {inventario && <Th className="w-24 text-right">Existência</Th>}
                  <Th className="w-24 text-right">{inventario ? "Contado" : "Quantidade"}</Th>
                  {inventario ? (
                    <Th className="w-24 text-right">Diferença</Th>
                  ) : (
                    <>
                      <Th className="w-32 text-right">{eCompra || eStock ? `Custo unit.${estrangeira ? ` (${moeda})` : ""}` : "Preço unit."}</Th>
                      {!eStock && <Th className="w-16 text-right">Desc. %</Th>}
                      {!eStock && <Th className="w-14 text-right">Taxa</Th>}
                      <Th className="w-28 text-right">Valor total</Th>
                    </>
                  )}
                  <Th className="w-10" />
                </tr>
              </thead>
              <tbody>
                {linhas.map((l, i) => (
                  <tr key={l.key}>
                    <Td>
                      <input className="w-full truncate bg-transparent font-medium outline-none" title={l.descricao} value={l.descricao} onChange={(e) => upd(l.key, { descricao: e.target.value })} />
                      <div className="font-mono text-[11.5px] text-ink-3">{l.referencia}</div>
                      {l.controla_lote && tipo.stock !== 0 && (
                        <LoteCampos linha={l} entrada={entradaStock} armazem={armazem} onChange={(patch) => upd(l.key, patch)} />
                      )}
                    </Td>
                    {inventario && <Td className="num text-right text-ink-2">{qtd(l.existencia)}</Td>}
                    <Td>
                      <Input className="num h-8 min-w-[64px] text-right" type="number" min={0} step="any" value={l.quantidade} onChange={(e) => upd(l.key, { quantidade: Number(e.target.value) })} />
                    </Td>
                    {inventario ? (
                      <Td className={cn("num text-right font-medium", l.quantidade - l.existencia < 0 ? "text-danger" : l.quantidade - l.existencia > 0 ? "text-ok" : "text-ink-3")}>
                        {qtd(l.quantidade - l.existencia)}
                      </Td>
                    ) : (
                      <>
                        <Td>
                          <Input className="num h-8 min-w-[96px] text-right" value={l.preco} onChange={(e) => upd(l.key, { preco: e.target.value })} disabled={!!tipo.credito && !!origemId} />
                        </Td>
                        {!eStock && (
                          <Td>
                            <Input className="num h-8 min-w-[52px] text-right" type="number" min={0} max={100} value={l.desconto} onChange={(e) => upd(l.key, { desconto: Number(e.target.value) })} />
                          </Td>
                        )}
                        {!eStock && <Td className="num text-right text-ink-2">{l.taxa}%</Td>}
                        <Td className="num text-right font-medium">{kzNum(calc[i]?.total)}</Td>
                      </>
                    )}
                    <Td>
                      <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setLinhas((ls) => ls.filter((x) => x.key !== l.key))}>
                        <Trash2 className="size-4" />
                      </button>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
            {!linhas.length && (
              <p className="py-10 text-center text-ink-3">
                {inventario ? "Adicione os artigos contados (a quantidade sugerida é a existência actual)." : "Pesquise e adicione artigos acima."}
              </p>
            )}
          </Card>

          {eCompra && (
            <Card className="p-5">
              <div className="mb-4 flex items-center gap-2 font-semibold">
                <Coins className="size-4 text-brand" /> Moeda e custos de importação
              </div>
              <div className="grid grid-cols-4 gap-4">
                <Field label="Moeda dos preços">
                  <Select
                    value={moeda}
                    onChange={(e) => {
                      const m = aux?.moedas.find((x) => x.codigo === e.target.value);
                      setMoeda(e.target.value);
                      setCambio(String(m?.cambio ?? 1).replace(".", ","));
                    }}
                  >
                    {aux?.moedas.map((m) => (
                      <option key={m.codigo} value={m.codigo}>
                        {m.codigo} — {m.descricao}
                      </option>
                    ))}
                  </Select>
                </Field>
                {estrangeira && (
                  <Field label={`Câmbio (Kz por 1 ${moedaSel?.simbolo ?? moeda})`} hint={fator > 0 ? undefined : "Indique o câmbio do dia"}>
                    <Input className="num text-right" value={cambio} onChange={(e) => setCambio(e.target.value)} />
                  </Field>
                )}
                {estrangeira && fator > 0 && (
                  <div className="col-span-2 self-end pb-2.5 text-[13px] text-ink-2">
                    Total em {moeda}: <b className="num">{kzNum(Math.round(tot.total / fator))}</b> · os valores do documento ficam em Kz.
                  </div>
                )}
              </div>
              <div className="mt-5">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Custos adicionais (entram no custo dos artigos)</span>
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => setCustos((c) => [...c, { key: crypto.randomUUID(), tipo: aux?.tipos_custo[0]?.codigo ?? "OUTROS", descricao: "", valor: "" }])}
                  >
                    <Plus className="size-4" /> Custo
                  </Button>
                </div>
                {custos.length === 0 ? (
                  <p className="text-[13px] text-ink-3">Frete, seguro, desalfandegamento, despachante… São repartidos pelas linhas na proporção do valor.</p>
                ) : (
                  <div className="space-y-2">
                    {custos.map((c) => (
                      <div key={c.key} className="grid grid-cols-[200px_minmax(0,1fr)_160px_36px] gap-2">
                        <Select value={c.tipo} onChange={(e) => setCustos((xs) => xs.map((x) => (x.key === c.key ? { ...x, tipo: e.target.value } : x)))}>
                          {aux?.tipos_custo.map((t) => (
                            <option key={t.codigo} value={t.codigo}>
                              {t.descricao}
                            </option>
                          ))}
                        </Select>
                        <Input placeholder="Descrição (opcional)" value={c.descricao} onChange={(e) => setCustos((xs) => xs.map((x) => (x.key === c.key ? { ...x, descricao: e.target.value } : x)))} />
                        <Input className="num text-right" placeholder="Valor em Kz" value={c.valor} onChange={(e) => setCustos((xs) => xs.map((x) => (x.key === c.key ? { ...x, valor: e.target.value } : x)))} />
                        <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setCustos((xs) => xs.filter((x) => x.key !== c.key))}>
                          <Trash2 className="size-4" />
                        </button>
                      </div>
                    ))}
                    <div className="flex justify-end gap-6 pt-1 text-[13px] text-ink-2">
                      <span>
                        Custos adicionais <b className="num">{kz(totalCustos)}</b>
                      </span>
                      <span>
                        Custo total de aquisição <b className="num">{kz(tot.liq + totalCustos)}</b>
                      </span>
                    </div>
                  </div>
                )}
              </div>
            </Card>
          )}

          {transporte && (
            <Card className="p-5">
              <div className="mb-4 flex items-center gap-2 font-semibold">
                <Truck className="size-4 text-brand" /> Dados de transporte
              </div>
              <div className="grid grid-cols-4 gap-4">
                <Field label="Local de carga" className="col-span-2">
                  <Input value={transp.carga_local ?? ""} onChange={(e) => setTransp({ ...transp, carga_local: e.target.value })} />
                </Field>
                <Field label="Data e hora de carga" className="col-span-2">
                  <Input type="datetime-local" value={transp.carga_data ?? ""} onChange={(e) => setTransp({ ...transp, carga_data: e.target.value })} />
                </Field>
                <Field label="Local de descarga" className="col-span-2">
                  <Input value={transp.descarga_local ?? ""} onChange={(e) => setTransp({ ...transp, descarga_local: e.target.value })} />
                </Field>
                <Field label="Data e hora de descarga" className="col-span-2">
                  <Input type="datetime-local" value={transp.descarga_data ?? ""} onChange={(e) => setTransp({ ...transp, descarga_data: e.target.value })} />
                </Field>
                <Field label="Matrícula da viatura" className="col-span-2">
                  <Input value={transp.matricula ?? ""} placeholder="LD-00-00-AA" onChange={(e) => setTransp({ ...transp, matricula: e.target.value.toUpperCase() })} />
                </Field>
                <Field label="Motorista" className="col-span-2">
                  <Input value={transp.motorista ?? ""} onChange={(e) => setTransp({ ...transp, motorista: e.target.value })} />
                </Field>
              </div>
            </Card>
          )}

          <Card className="p-5">
            <Field label="Observação">
              <Input value={obs} onChange={(e) => setObs(e.target.value)} />
            </Field>
          </Card>
        </div>

        {/* Totais (como no KIGEST) */}
        <Card className="sticky top-6 h-fit p-5">
          <div className="mb-3 flex items-center justify-between text-[13px] text-ink-3">
            <span>Itens</span>
            <span className="num">{linhas.length}</span>
          </div>
          {!inventario && (
            <>
              <div className="space-y-2 text-ink-2">
                <Row k="Total Merc./Serviços" v={kz(tot.bruto)} />
                <Row k="Descontos por linha" v={kz(tot.descLinha)} />
                {!eStock && (
                  <div className="flex items-center justify-between gap-3">
                    <span>Desconto global</span>
                    <div className="flex items-center gap-2">
                      <Input className="num h-8 w-16 text-right" type="number" min={0} max={100} value={descGlobal} onChange={(e) => setDescGlobal(Math.min(100, Math.max(0, Number(e.target.value))))} />
                      <span className="text-[12px]">%</span>
                    </div>
                  </div>
                )}
                {tot.descGlobal > 0 && <Row k="" v={`− ${kz(tot.descGlobal)}`} />}
                {!eStock && <Row k="Imposto (IVA)" v={kz(tot.iva)} />}
                <Row k="Total líquido" v={kz(tot.liq)} />
              </div>

              {!eStock && !!aux?.retencoes.length && (
                <div className="mt-4">
                  <div className="mb-1.5 text-[12px] font-medium uppercase tracking-wide text-ink-2">Retenção na fonte</div>
                  <div className="flex flex-wrap gap-1.5">
                    {aux.retencoes.map((r) => {
                      const on = retencoes.includes(r.codigo);
                      return (
                        <button
                          key={r.codigo}
                          title={`${r.descricao} — ${r.percentagem}%`}
                          onClick={() => setRetencoes((x) => (on ? x.filter((c) => c !== r.codigo) : [...x, r.codigo]))}
                          className={cn("rounded-md border px-2 py-1 text-[12px] transition", on ? "border-brand bg-brand-50 font-medium text-brand-700" : "border-line text-ink-2 hover:border-brand/40")}
                        >
                          {r.codigo} {r.percentagem}%
                        </button>
                      );
                    })}
                  </div>
                  {retencaoValor > 0 && <Row k="Retenção" v={`− ${kz(retencaoValor)}`} />}
                </div>
              )}

              <div className="mt-4 flex flex-wrap items-baseline justify-between gap-x-3 border-t border-line pt-3">
                <span className="whitespace-nowrap font-medium">{retencaoValor > 0 ? "Total a pagar" : "Total do documento"}</span>
                <span className="num ml-auto text-[26px] font-semibold tracking-tight">{kz(aPagar)}</span>
              </div>
              {retencaoValor > 0 && <div className="text-right text-[12px] text-ink-3">Total do documento {kz(tot.total)}</div>}
            </>
          )}

          {!!tipo.pago_no_acto && (
            <Field label="Modo de pagamento" className="mt-4">
              <Select value={pagModo} onChange={(e) => setPagModo(e.target.value)}>
                {t?.modos_pagamento.map((m) => (
                  <option key={m.codigo} value={m.codigo}>
                    {m.descricao}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <Button size="lg" className="mt-5 w-full" loading={busy} disabled={!linhas.length || (!!tipo.credito && !origemId) || (estrangeira && fator <= 0) || (manual && (!manualSerie || !manualNumero.trim() || !manualData))}
            onClick={() => emitir()}
          >
            Gravar {tipo.codigo}
          </Button>
          <p className="mt-3 text-[12px] leading-relaxed text-ink-3">
            {tipo.conta_corrente === 1 && "O valor fica em dívida na conta corrente. "}
            {tipo.stock === -1 && "Dá saída de stock. "}
            {tipo.stock === 1 && "Dá entrada de stock e actualiza o custo médio. "}
            {tipo.stock === 3 && "Acerta o stock para as quantidades contadas. "}
            {tipo.assinado ? "Após gravado, o documento é assinado e não pode ser alterado." : "Documento interno (não assinado)."}
          </p>
        </Card>
      </div>

      {escolherOrigem && entidade && (
        <OrigemModal
          grupo={grupo}
          entidadeId={entidade.id}
          onClose={() => setEscolherOrigem(false)}
          onEscolher={(id) => {
            setEscolherOrigem(false);
            setSp({ origem: String(id) });
          }}
        />
      )}

      {pinPedido && (
        <Modal
          open
          onClose={() => setPinPedido(null)}
          title="Autorização do supervisor"
          width="max-w-md"
          footer={
            <>
              <Button variant="secondary" onClick={() => setPinPedido(null)}>
                Cancelar
              </Button>
              <Button loading={busy} disabled={pin.length < 4} onClick={() => emitir(pin)}>
                <KeyRound className="size-4" /> Autorizar
              </Button>
            </>
          }
        >
          <p className="mb-4 text-ink-2">{pinPedido}</p>
          <Field label="PIN do supervisor">
            <Input autoFocus type="password" inputMode="numeric" value={pin} onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))} onKeyDown={(e) => e.key === "Enter" && pin.length >= 4 && emitir(pin)} />
          </Field>
        </Modal>
      )}

      <DocumentoModal
        id={emitido}
        onClose={() => {
          setEmitido(null);
          nav(`/documentos?grupo=${grupo}`);
        }}
      />
    </>
  );
}

function Info({ k, v }: { k: string; v: string }) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] uppercase tracking-wide text-ink-3">{k}</div>
      <div className="truncate font-medium">{v}</div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <span>{k}</span>
      <span className="num">{v}</span>
    </div>
  );
}

/** Escolha da factura de origem (nota de crédito). */
function OrigemModal({ grupo, entidadeId, onClose, onEscolher }: { grupo: string; entidadeId: number; onClose: () => void; onEscolher: (id: number) => void }) {
  const { data: lista } = useQuery({
    queryKey: ["origens", grupo, entidadeId],
    queryFn: () => call<DocResumo[]>("documentos.listar", { grupo, entidade_id: entidadeId, estado: "N", limite: 200 }),
  });
  const facturas = lista?.filter((d) => (d.classe === "VENDA" || d.classe === "COMPRA") && !["NC", "VNC"].includes(d.tipo_codigo));
  return (
    <Modal open onClose={onClose} title="Documento de origem" width="max-w-2xl">
      {!facturas?.length ? (
        <p className="py-8 text-center text-ink-3">Esta entidade não tem facturas por regularizar.</p>
      ) : (
        <ul className="max-h-[60vh] divide-y divide-line overflow-y-auto">
          {facturas.map((d) => (
            <li key={d.id}>
              <button onClick={() => onEscolher(d.id)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2.5 text-left hover:bg-sunken">
                <div className="min-w-0 flex-1">
                  <div className="font-medium">{d.numero_doc}</div>
                  <div className="text-[12px] text-ink-3">
                    {d.tipo_desc} · {data(d.data_emissao)} {d.doc_externo ? `· ${d.doc_externo}` : ""}
                  </div>
                </div>
                <span className="num font-medium">{kz(d.total)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

/** Lote e validade de uma linha: entrada (indicar lote/validade) ou saída (lote ou FEFO automático). */
function LoteCampos({ linha, entrada, armazem, onChange }: { linha: L; entrada: boolean; armazem: number; onChange: (p: Partial<L>) => void }) {
  const lotes = useQuery({
    queryKey: ["lotes-artigo", linha.artigo_id, armazem],
    queryFn: () => call<Lote[]>("lotes.listar", { artigo_id: linha.artigo_id, armazem_id: armazem }),
    staleTime: 30_000,
  }).data;
  if (entrada) {
    return (
      <div className="mt-1.5 flex items-center gap-2">
        <input
          list={`lotes-${linha.key}`}
          className="h-7 w-32 rounded-md border border-line-strong bg-surface px-2 text-[12.5px] outline-none focus:border-brand"
          placeholder="Lote *"
          value={linha.lote}
          onChange={(e) => {
            const l = lotes?.find((x) => x.lote === e.target.value);
            onChange({ lote: e.target.value, validade: l?.validade ?? linha.validade });
          }}
        />
        <datalist id={`lotes-${linha.key}`}>
          {lotes?.map((l) => (
            <option key={l.id} value={l.lote} />
          ))}
        </datalist>
        <input
          type="date"
          className="select-kg h-7 max-w-full rounded-md border border-line-strong bg-surface pl-2 pr-7 text-[12.5px] outline-none focus:border-brand"
          title="Validade"
          value={linha.validade}
          onChange={(e) => onChange({ validade: e.target.value })}
        />
      </div>
    );
  }
  return (
    <div className="mt-1.5">
      <select
        className="select-kg h-7 max-w-full rounded-md border border-line-strong bg-surface pl-2 pr-7 text-[12.5px] outline-none focus:border-brand"
        value={linha.lote}
        onChange={(e) => onChange({ lote: e.target.value })}
      >
        <option value="">Lote automático (o que expira primeiro)</option>
        {lotes?.map((l) => (
          <option key={l.id} value={l.lote}>
            {l.lote} · {qtd(l.quantidade)} un.{l.validade ? ` · val. ${data(l.validade)}` : ""}
          </option>
        ))}
      </select>
    </div>
  );
}

import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { DatabaseBackup, ImagePlus, Inbox, Monitor, MonitorSmartphone, Network, Plus, Printer, Server, Wifi } from "lucide-react";
import { abrirVisorEcra, logoParaRaster } from "@/lib/equipamento";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { PrintPortal } from "@/components/DocumentoPrint";
import EmailConfig from "@/components/EmailConfig";
import { imprimir } from "@/lib/print";
import { call, reiniciar } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { dataHora, parseKz, toInputKz } from "@/lib/format";
import type { Empresa, EscPos, Posto, PostoOpcoes, VisorOpcoes } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

const ABAS = ["Empresa", "Postos e impressão", "Rede", "Email", "Tabelas", "Sistema", "Certificação AGT"] as const;

export default function Configuracoes() {
  const { tem } = useSession();
  const nav = useNavigate();
  const permitido: Record<(typeof ABAS)[number], boolean> = {
    Empresa: tem("sistema.configuracoes"),
    "Postos e impressão": true,
    Rede: tem("sistema.configuracoes"),
    Email: tem("sistema.configuracoes"),
    Tabelas: tem("sistema.tabelas"),
    Sistema: tem("sistema.backup"),
    "Certificação AGT": tem("sistema.certificacao"),
  };
  const visiveis = ABAS.filter((a) => permitido[a]);
  const [aba, setAba] = useState<(typeof ABAS)[number]>(visiveis[0] ?? "Postos e impressão");
  // ?aba=…: «Cadastro de Empresa», «Utilizadores», «Cópia de Segurança», «Armazém» abrem o separador certo
  const [sp] = useSearchParams();
  const pedida = ({ empresa: "Empresa", postos: "Postos e impressão", rede: "Rede", email: "Email", tabelas: "Tabelas", sistema: "Sistema", agt: "Certificação AGT" } as const)[
    sp.get("aba") as "empresa"
  ];
  // os utilizadores passaram para o seu próprio módulo
  useEffect(() => {
    if (sp.get("aba") === "utilizadores") nav("/utilizadores", { replace: true });
  }, [sp, nav]);
  useEffect(() => {
    if (pedida && permitido[pedida]) setAba(pedida);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pedida]);
  return (
    <>
      <PageHeader title="Configurações" subtitle="Empresa, postos, rede, email, tabelas auxiliares, cópias de segurança e certificação" />
      <div className="mb-5 flex gap-1 overflow-x-auto border-b border-line">
        {visiveis.map((a) => (
          <button
            key={a}
            onClick={() => setAba(a)}
            className={cn("-mb-px whitespace-nowrap border-b-2 px-4 py-2.5", aba === a ? "border-brand font-medium text-ink" : "border-transparent text-ink-2 hover:text-ink")}
          >
            {a}
          </button>
        ))}
      </div>
      {aba === "Empresa" && <EmpresaForm />}
      {aba === "Postos e impressão" && <Postos />}
      {aba === "Rede" && <Rede />}
      {aba === "Email" && <EmailConfig />}
      {aba === "Tabelas" && <Tabelas />}
      {aba === "Sistema" && <Sistema />}
      {aba === "Certificação AGT" && <Agt />}
    </>
  );
}

/** Lê uma imagem e reduz para no máximo 600 px (PNG), para caber nos documentos. */
function lerLogo(ficheiro: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(new Error("Não foi possível ler a imagem."));
    r.onload = () => {
      const img = new Image();
      img.onerror = () => reject(new Error("Formato de imagem não suportado."));
      img.onload = () => {
        const escala = Math.min(1, 600 / Math.max(img.width, img.height));
        const c = document.createElement("canvas");
        c.width = Math.round(img.width * escala);
        c.height = Math.round(img.height * escala);
        c.getContext("2d")!.drawImage(img, 0, 0, c.width, c.height);
        resolve(c.toDataURL("image/png"));
      };
      img.src = r.result as string;
    };
    r.readAsDataURL(ficheiro);
  });
}

function EmpresaForm() {
  const { data } = useQuery({ queryKey: ["empresa"], queryFn: () => call<Empresa>("empresa.obter") });
  const [e, setE] = useState<Partial<Empresa>>({});
  const qc = useQueryClient();
  useEffect(() => data && setE(data), [data]);
  const set = (k: keyof Empresa, v: unknown) => setE((x) => ({ ...x, [k]: v }));
  const bancos = e.bancos ?? [];
  const [papel, setPapel] = useState(false);
  function imprimirPapel() {
    setPapel(true);
    imprimir();
    setTimeout(() => setPapel(false), 4000);
  }

  async function guardar() {
    try {
      await call("empresa.guardar", e);
      toast.success("Dados da empresa guardados.");
      qc.invalidateQueries({ queryKey: ["empresa"] });
      qc.invalidateQueries({ queryKey: ["documento"] });
    } catch (err) {
      toast.error((err as Error).message);
    }
  }

  return (
    <Card className="max-w-3xl p-6">
      <div className="mb-6 flex items-center gap-4">
        <div className="grid size-24 place-items-center overflow-hidden rounded-xl border border-dashed border-line-strong bg-sunken">
          {e.logo ? <img src={e.logo} alt="Logótipo" className="max-h-full max-w-full object-contain" /> : <ImagePlus className="size-6 text-ink-3" />}
        </div>
        <div className="space-y-2">
          <div className="font-medium">Logótipo</div>
          <div className="text-[13px] text-ink-3">Impresso nas facturas A4 e nos talões.</div>
          <div className="flex gap-2">
            <label className="inline-flex h-8 cursor-pointer items-center rounded-lg border border-line-strong bg-surface px-3 text-[13px] hover:bg-sunken">
              Escolher imagem
              <input
                type="file"
                accept="image/png,image/jpeg"
                className="hidden"
                onChange={async (ev) => {
                  const f = ev.target.files?.[0];
                  if (!f) return;
                  try {
                    set("logo", await lerLogo(f));
                  } catch (err) {
                    toast.error((err as Error).message);
                  }
                }}
              />
            </label>
            {e.logo && (
              <Button size="sm" variant="ghost" onClick={() => set("logo", null)}>
                Remover
              </Button>
            )}
          </div>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Nome / Razão social" className="col-span-2">
          <Input value={e.nome ?? ""} onChange={(x) => set("nome", x.target.value)} />
        </Field>
        <Field label="Designação comercial">
          <Input value={e.designacao_comercial ?? ""} onChange={(x) => set("designacao_comercial", x.target.value)} />
        </Field>
        <Field label="NIF">
          <Input value={e.nif ?? ""} onChange={(x) => set("nif", x.target.value)} />
        </Field>
        <Field label="Endereço" className="col-span-2">
          <Input value={e.endereco ?? ""} onChange={(x) => set("endereco", x.target.value)} />
        </Field>
        <Field label="Localidade">
          <Input value={e.localidade ?? ""} onChange={(x) => set("localidade", x.target.value)} />
        </Field>
        <Field label="Província">
          <Input value={e.provincia ?? ""} onChange={(x) => set("provincia", x.target.value)} />
        </Field>
        <Field label="Telefone">
          <Input value={e.telefone ?? ""} onChange={(x) => set("telefone", x.target.value)} />
        </Field>
        <Field label="Email">
          <Input value={e.email ?? ""} onChange={(x) => set("email", x.target.value)} />
        </Field>
        <Field label="Regime de IVA">
          <Select value={e.regime_iva ?? "GERAL"} onChange={(x) => set("regime_iva", x.target.value)}>
            <option value="GERAL">Regime Geral</option>
            <option value="SIMPLIFICADO">Regime Simplificado</option>
            <option value="EXCLUSAO">Regime de Exclusão</option>
          </Select>
        </Field>
        <Field label="Rodapé dos documentos" className="col-span-2">
          <Input value={e.rodape ?? ""} onChange={(x) => set("rodape", x.target.value)} />
        </Field>
      </div>
      <div className="mt-6">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Contas bancárias (impressas nas facturas)</span>
          <Button size="sm" variant="secondary" onClick={() => set("bancos", [...bancos, { banco: "", iban: "" }])}>
            <Plus className="size-4" /> Conta
          </Button>
        </div>
        {bancos.map((b, i) => (
          <div key={i} className="mb-2 grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto] gap-2">
            <Input placeholder="Banco" value={b.banco} onChange={(x) => set("bancos", bancos.map((y, j) => (j === i ? { ...y, banco: x.target.value } : y)))} />
            <Input placeholder="IBAN" value={b.iban} onChange={(x) => set("bancos", bancos.map((y, j) => (j === i ? { ...y, iban: x.target.value } : y)))} />
            <Button variant="ghost" onClick={() => set("bancos", bancos.filter((_, j) => j !== i))}>
              Remover
            </Button>
          </div>
        ))}
      </div>
      <div className="mt-6 flex justify-end gap-2">
        <Button variant="secondary" onClick={imprimirPapel} disabled={!data} title="Folha A4 com o cabeçalho e o rodapé da empresa (PapelTimbrado do KIGEST)">
          <Printer className="size-4" /> Papel timbrado
        </Button>
        <Button onClick={guardar}>Guardar</Button>
      </div>
      {papel && data && (
        <PrintPortal>
          <PapelTimbrado empresa={data} />
        </PrintPortal>
      )}
    </Card>
  );
}

/** Folha A4 em branco com o cabeçalho e o rodapé da empresa, para cartas e ofícios. */
function PapelTimbrado({ empresa }: { empresa: Empresa }) {
  const bancos = (empresa.bancos ?? []).filter((b) => b.banco || b.iban);
  return (
    <div className="flex min-h-[267mm] flex-col p-2 text-[11px] text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="" />
      <div className="flex-1" />
      <div className="border-t border-black pt-2 text-center text-[10px] leading-relaxed">
        {empresa.rodape && <div>{empresa.rodape}</div>}
        {bancos.length > 0 && <div>{bancos.map((b) => [b.banco, b.iban].filter(Boolean).join(" · IBAN ")).join("   |   ")}</div>}
        <div>
          {[empresa.designacao_comercial || empresa.nome, `NIF ${empresa.nif}`, empresa.telefone && `Tel. ${empresa.telefone}`, empresa.email].filter(Boolean).join(" · ")}
        </div>
      </div>
    </div>
  );
}

const ESCPOS_PADRAO: EscPos = {
  ativo: false,
  ligacao: "fila",
  destino: "",
  colunas: 42,
  cortar: true,
  acentos: true,
  gaveta: "numerario",
  pino: 2,
};

const VISOR_PADRAO: VisorOpcoes = { modo: "nenhum", porta: "", velocidade: 9600, colunas: 20, protocolo: "cd5220", mensagem1: "", mensagem2: "" };

const OPCOES_PADRAO: PostoOpcoes = {
  formato: "ticket",
  vias: 1,
  imprimir_auto: false,
  mostrar_opcoes_impressao: true,
  valor_minimo_impressao: 0,
  repetir_linhas: false,
  confirmar_cliente: false,
  escpos: ESCPOS_PADRAO,
  talao_mm: 80,
};

function Postos() {
  const t = useTabelas();
  const qc = useQueryClient();
  const tem = useSession((s) => s.tem);
  const { data, refetch } = useQuery({ queryKey: ["postos"], queryFn: () => call<{ atual: Posto; postos: Posto[] }>("posto.obter") });
  const termica = useQuery({ queryKey: ["impressao"], queryFn: () => call<{ impressoras: string[]; sistema: string }>("impressao.estado") });
  const [p, setP] = useState<Posto | null>(null);
  const [minimo, setMinimo] = useState("0,00");
  const empresa = useEmpresa();
  const [previaLogo, setPreviaLogo] = useState<string | null>(null);

  function editar(x: Posto) {
    setP({ ...x, opcoes: { ...OPCOES_PADRAO, ...(x.opcoes ?? {}) } });
    setMinimo(toInputKz(x.opcoes?.valor_minimo_impressao ?? 0));
  }

  async function gravar(fechar = true) {
    if (!p) return false;
    try {
      let opcoes: PostoOpcoes = { ...OPCOES_PADRAO, ...p.opcoes, valor_minimo_impressao: parseKz(minimo) };
      // logótipo do talão térmico: convertido aqui (o browser lê PNG e JPEG) para a largura do papel em uso
      const esc = opcoes.escpos;
      if (esc?.ativo && esc.logo_ativo) {
        if (!empresa?.logo) {
          toast.warning("A empresa não tem logótipo: carregue-o em Configurações → Empresa.");
          opcoes = { ...opcoes, escpos: { ...esc, logo: null } };
        } else {
          const { raster, previa } = await logoParaRaster(empresa.logo, esc.colunas);
          setPreviaLogo(previa);
          opcoes = { ...opcoes, escpos: { ...esc, logo: raster } };
        }
      }
      await call("posto.guardar", { ...p, id: p.id || undefined, opcoes });
      if (fechar) {
        toast.success("Posto guardado.");
        setP(null);
      }
      refetch();
      qc.invalidateQueries({ queryKey: ["tabelas"] });
      return true;
    } catch (e) {
      toast.error((e as Error).message);
      return false;
    }
  }

  /** Grava primeiro (o backend lê a configuração gravada do posto desta máquina) e só depois testa. */
  async function testar(metodo: "impressao.teste" | "impressao.gaveta" | "impressao.visorTeste") {
    if (!(await gravar(false))) return;
    try {
      await call(metodo);
      toast.success(
        metodo === "impressao.gaveta" ? "Comando enviado à gaveta." : metodo === "impressao.visorTeste" ? "Teste enviado ao visor de cliente." : "Talão de teste enviado à impressora.",
      );
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function usarNestaMaquina(id: number) {
    try {
      await call("posto.selecionar", { id });
      toast.success("Esta máquina passou a ser este posto.");
      refetch();
      qc.invalidateQueries();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const o = p?.opcoes ?? OPCOES_PADRAO;
  const setO = (patch: Partial<PostoOpcoes>) => setP((x) => (x ? { ...x, opcoes: { ...OPCOES_PADRAO, ...x.opcoes, ...patch } } : x));
  const ep = { ...ESCPOS_PADRAO, ...(o.escpos ?? {}) };
  const setEp = (patch: Partial<EscPos>) => setO({ escpos: { ...ep, ...patch } });
  const vs: VisorOpcoes = { ...VISOR_PADRAO, ...(o.visor ?? {}) };
  const setVs = (patch: Partial<VisorOpcoes>) => setO({ visor: { ...vs, ...patch } });

  return (
    <Card className="max-w-4xl">
      <div className="flex items-center justify-between p-4">
        <div className="text-[13px] text-ink-2">
          Esta máquina: <b className="text-ink">{data?.atual.descricao}</b>
        </div>
        {tem("pos.config_posto") && (
          <Button onClick={() => editar({ id: 0, descricao: "", armazem_id: 1, serie_prefixo: "", opcoes: OPCOES_PADRAO })}>
            <Plus className="size-4" /> Novo posto
          </Button>
        )}
      </div>
      <Table>
        <thead>
          <tr>
            <Th>Posto</Th>
            <Th>Armazém</Th>
            <Th>Série</Th>
            <Th>Impressão</Th>
            <Th />
          </tr>
        </thead>
        <tbody>
          {data?.postos.map((x) => (
            <tr key={x.id} className="hover:bg-sunken/60">
              <Td className="font-medium">
                {x.descricao} {data.atual.id === x.id && <Badge tone="brand" className="ml-1"><Monitor className="mr-1 size-3" />esta máquina</Badge>}
              </Td>
              <Td>{x.armazem}</Td>
              <Td className="font-mono text-[12.5px]">{x.serie_prefixo || "—"}</Td>
              <Td className="text-ink-2">
                {(x.opcoes?.formato ?? "ticket") === "ticket" ? `Talão ${x.opcoes?.talao_mm ?? 80} mm` : "A4"} · {x.opcoes?.vias ?? 1} via(s)
                {x.opcoes?.escpos?.ativo && (
                  <Badge tone="accent" className="ml-2">
                    <Printer className="mr-1 size-3" />
                    térmica directa
                  </Badge>
                )}
              </Td>
              <Td className="text-right">
                {tem("pos.config_posto") && (
                  <>
                    {data.atual.id !== x.id && (
                      <Button size="sm" variant="ghost" onClick={() => usarNestaMaquina(x.id)}>
                        Usar nesta máquina
                      </Button>
                    )}
                    <Button size="sm" variant="secondary" onClick={() => editar(x)}>
                      Configurar
                    </Button>
                  </>
                )}
              </Td>
            </tr>
          ))}
        </tbody>
      </Table>
      {p && (
        <Modal open onClose={() => setP(null)} title={p.id ? `Configurar ${p.descricao}` : "Novo posto"} width="max-w-2xl" footer={<Button onClick={() => gravar()}>Guardar</Button>}>
          <div className="grid grid-cols-3 gap-4">
            <Field label="Descrição">
              <Input autoFocus value={p.descricao} onChange={(e) => setP({ ...p, descricao: e.target.value })} />
            </Field>
            <Field label="Armazém">
              <Select value={p.armazem_id} onChange={(e) => setP({ ...p, armazem_id: Number(e.target.value) })}>
                {t.data?.armazens.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.nome}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Prefixo de série" hint="Ex.: P2 → FR P22026/1">
              <Input value={p.serie_prefixo} onChange={(e) => setP({ ...p, serie_prefixo: e.target.value.toUpperCase() })} />
            </Field>
          </div>
          <div className="mt-5 grid grid-cols-2 gap-6">
            <div className="space-y-2">
              <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Venda</div>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={o.repetir_linhas} onChange={(e) => setO({ repetir_linhas: e.target.checked })} /> Repetir linhas de artigos
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={o.confirmar_cliente} onChange={(e) => setO({ confirmar_cliente: e.target.checked })} /> Confirmar cliente ao gravar
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={!!o.pos_ecra_inteiro} onChange={(e) => setO({ pos_ecra_inteiro: e.target.checked })} /> Abrir o POS em ecrã inteiro (terminais POS)
              </label>
              <label className="flex items-center gap-2" title="Quem fecha o caixa conta o dinheiro sem ver o valor esperado (excepto quem tem a permissão «ver o valor esperado»)">
                <input type="checkbox" checked={!!o.fecho_cego} onChange={(e) => setO({ fecho_cego: e.target.checked })} /> Fecho de caixa cego
              </label>
            </div>
            <div className="space-y-3">
              <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Impressão</div>
              <div className="grid grid-cols-2 gap-3">
                <Field label="Formato">
                  <Select value={o.formato} onChange={(e) => setO({ formato: e.target.value as PostoOpcoes["formato"] })}>
                    <option value="ticket">Talão</option>
                    <option value="a4">A4</option>
                  </Select>
                </Field>
                {o.formato === "ticket" && (
                  <Field label="Papel do talão">
                    <Select
                      value={o.talao_mm ?? 80}
                      onChange={(e) => {
                        const mm = Number(e.target.value) === 58 ? 58 : 80;
                        // a impressão directa (ESC/POS) acompanha a largura: 42 ou 32 colunas
                        setO({ talao_mm: mm, escpos: o.escpos ? { ...o.escpos, colunas: mm === 58 ? 32 : 42 } : o.escpos });
                      }}
                    >
                      <option value={80}>80 mm</option>
                      <option value={58}>58 mm</option>
                    </Select>
                  </Field>
                )}
                <Field label="Vias">
                  <Select value={o.vias} onChange={(e) => setO({ vias: Number(e.target.value) })}>
                    {[1, 2, 3].map((n) => (
                      <option key={n}>{n}</option>
                    ))}
                  </Select>
                </Field>
              </div>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={o.mostrar_opcoes_impressao} onChange={(e) => setO({ mostrar_opcoes_impressao: e.target.checked })} /> Mostrar opções de impressão ao finalizar venda
              </label>
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={o.imprimir_auto} onChange={(e) => setO({ imprimir_auto: e.target.checked })} /> Imprimir automaticamente
              </label>
              <Field label="Valor mínimo de impressão (Kz)" hint="Vendas abaixo deste valor não são impressas automaticamente">
                <Input className="num text-right" value={minimo} onChange={(e) => setMinimo(e.target.value)} />
              </Field>
            </div>
          </div>
          <div className="mt-6 border-t border-line pt-4">
            <div className="flex items-center justify-between">
              <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Impressora térmica (ESC/POS) e gaveta</div>
              <label className="flex items-center gap-2 text-[13px]">
                <input type="checkbox" checked={ep.ativo} onChange={(e) => setEp({ ativo: e.target.checked })} /> Imprimir directamente (corta o papel e abre a gaveta)
              </label>
            </div>
            {!ep.ativo ? (
              <p className="mt-2 text-[12.5px] text-ink-3">
                Sem esta opção, o talão sai pelo diálogo de impressão do sistema: não corta o papel nem abre a gaveta.
              </p>
            ) : (
              <>
                <div className="mt-3 grid grid-cols-3 gap-3">
                  <Field label="Ligação">
                    <Select value={ep.ligacao} onChange={(e) => setEp({ ligacao: e.target.value as EscPos["ligacao"] })}>
                      <option value="fila">Fila de impressão local</option>
                      <option value="rede">Impressora de rede (TCP)</option>
                      <option value="ficheiro">Porta / dispositivo</option>
                    </Select>
                  </Field>
                  <Field
                    label="Destino"
                    className="col-span-2"
                    hint={
                      ep.ligacao === "rede"
                        ? "Endereço IP da impressora (porta 9100 por omissão): ex. 192.168.1.50"
                        : ep.ligacao === "ficheiro"
                          ? termica.data?.sistema === "windows"
                            ? "Porta local: ex. \\\\.\\COM3 ou LPT1"
                            : "Dispositivo: ex. /dev/usb/lp0"
                          : termica.data?.sistema === "windows"
                            ? "Nome da partilha da impressora no Windows (partilhe a impressora com um nome sem espaços)"
                            : "Nome da fila de impressão (CUPS)"
                    }
                  >
                    <Input
                      list="filas-impressao"
                      className="font-mono"
                      placeholder={ep.ligacao === "rede" ? "192.168.1.50:9100" : ep.ligacao === "ficheiro" ? "/dev/usb/lp0" : "POS80"}
                      value={ep.destino}
                      onChange={(e) => setEp({ destino: e.target.value })}
                    />
                    {ep.ligacao === "fila" && (
                      <datalist id="filas-impressao">
                        {termica.data?.impressoras.map((n) => (
                          <option key={n} value={n} />
                        ))}
                      </datalist>
                    )}
                  </Field>
                </div>
                <div className="mt-3 grid grid-cols-3 gap-3">
                  <Field label="Largura do papel">
                    <Select value={ep.colunas} onChange={(e) => setEp({ colunas: Number(e.target.value) })}>
                      <option value={42}>80 mm (42 colunas)</option>
                      <option value={48}>80 mm, letra pequena (48)</option>
                      <option value={32}>58 mm (32 colunas)</option>
                    </Select>
                  </Field>
                  <Field label="Abrir a gaveta">
                    <Select value={ep.gaveta} onChange={(e) => setEp({ gaveta: e.target.value as EscPos["gaveta"] })}>
                      <option value="numerario">Só em pagamentos com numerário</option>
                      <option value="sempre">Em todas as vendas</option>
                      <option value="nunca">Nunca</option>
                    </Select>
                  </Field>
                  <Field label="Pino da gaveta" hint="Quase todas usam o pino 2">
                    <Select value={ep.pino} onChange={(e) => setEp({ pino: Number(e.target.value) as EscPos["pino"] })}>
                      <option value={2}>Pino 2</option>
                      <option value={5}>Pino 5</option>
                    </Select>
                  </Field>
                </div>
                <div className="mt-3 flex flex-wrap items-center gap-5">
                  <label className="flex items-center gap-2 text-[13px]">
                    <input type="checkbox" checked={ep.cortar} onChange={(e) => setEp({ cortar: e.target.checked })} /> Cortar o papel no fim
                  </label>
                  <label className="flex items-center gap-2 text-[13px]">
                    <input type="checkbox" checked={ep.acentos} onChange={(e) => setEp({ acentos: e.target.checked })} /> Imprimir acentos
                  </label>
                  <label className="flex items-center gap-2 text-[13px]">
                    <input type="checkbox" checked={!!ep.logo_ativo} onChange={(e) => setEp({ logo_ativo: e.target.checked })} /> Imprimir o logótipo da empresa
                  </label>
                </div>
                {ep.logo_ativo && (
                  <div className="mt-3 flex items-center gap-4 rounded-lg border border-line bg-sunken px-3 py-2 text-[12.5px] text-ink-2">
                    {previaLogo ? (
                      <img src={previaLogo} alt="Pré-visualização do logótipo como sai na térmica" className="h-12 rounded bg-white [image-rendering:pixelated]" />
                    ) : (
                      <span className="text-ink-3">{empresa?.logo ? "O logótipo é convertido para a impressora ao gravar." : "A empresa ainda não tem logótipo."}</span>
                    )}
                    <span>Convertido a preto e branco para a largura do papel ({ep.colunas <= 32 ? "58" : "80"} mm).</span>
                  </div>
                )}
                <div className="mt-4 flex flex-wrap items-center gap-2">
                  <Button variant="secondary" size="sm" disabled={data?.atual.id !== p.id} onClick={() => testar("impressao.teste")}>
                    <Printer className="size-4" /> Gravar e testar impressão
                  </Button>
                  <Button variant="secondary" size="sm" disabled={data?.atual.id !== p.id} onClick={() => testar("impressao.gaveta")}>
                    <Inbox className="size-4" /> Abrir gaveta
                  </Button>
                  {data?.atual.id !== p.id && <span className="text-[12.5px] text-ink-3">O teste só pode ser feito no posto desta máquina.</span>}
                </div>
              </>
            )}
          </div>
          <div className="mt-6 border-t border-line pt-4">
            <div className="mb-3 flex items-center gap-2 text-[12px] font-medium uppercase tracking-wide text-ink-2">
              <MonitorSmartphone className="size-4" /> Visor de cliente
            </div>
            <div className="grid grid-cols-3 gap-3">
              <Field label="Tipo" className="col-span-3 sm:col-span-1">
                <Select value={vs.modo} onChange={(e) => setVs({ modo: e.target.value as VisorOpcoes["modo"] })}>
                  <option value="nenhum">Sem visor</option>
                  <option value="serie">Visor de coluna (série/USB)</option>
                  <option value="ecra">Segundo ecrã</option>
                </Select>
              </Field>
              {vs.modo !== "nenhum" && (
                <>
                  <Field label="Mensagem de boas-vindas" hint="Linha 1 (vazio = nome da empresa)">
                    <Input value={vs.mensagem1} maxLength={40} onChange={(e) => setVs({ mensagem1: e.target.value })} />
                  </Field>
                  <Field label="&nbsp;" hint="Linha 2">
                    <Input value={vs.mensagem2} maxLength={40} placeholder="Bem-vindo!" onChange={(e) => setVs({ mensagem2: e.target.value })} />
                  </Field>
                </>
              )}
            </div>
            {vs.modo === "serie" && (
              <div className="mt-3 grid grid-cols-4 gap-3">
                <Field label="Porta" hint={termica.data?.sistema === "windows" ? "Ex.: COM3 (ver no Gestor de Dispositivos)" : "Ex.: /dev/cu.usbserial-1410"}>
                  <Input className="font-mono" value={vs.porta} placeholder={termica.data?.sistema === "windows" ? "COM3" : "/dev/cu.usbserial"} onChange={(e) => setVs({ porta: e.target.value })} />
                </Field>
                <Field label="Velocidade">
                  <Select value={vs.velocidade} onChange={(e) => setVs({ velocidade: Number(e.target.value) })}>
                    {[2400, 4800, 9600, 19200, 38400, 115200].map((v) => (
                      <option key={v} value={v}>
                        {v} bps
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label="Comandos">
                  <Select value={vs.protocolo} onChange={(e) => setVs({ protocolo: e.target.value as VisorOpcoes["protocolo"] })}>
                    <option value="cd5220">Genérico (CD5220)</option>
                    <option value="escpos">Epson (ESC/POS)</option>
                  </Select>
                </Field>
                <Field label="Colunas">
                  <Select value={vs.colunas} onChange={(e) => setVs({ colunas: Number(e.target.value) })}>
                    {[16, 20, 40].map((c) => (
                      <option key={c} value={c}>
                        2 × {c}
                      </option>
                    ))}
                  </Select>
                </Field>
              </div>
            )}
            {vs.modo !== "nenhum" && (
              <div className="mt-3 flex flex-wrap items-center gap-2">
                {vs.modo === "serie" ? (
                  <Button variant="secondary" size="sm" disabled={data?.atual.id !== p.id} onClick={() => testar("impressao.visorTeste")}>
                    <MonitorSmartphone className="size-4" /> Gravar e testar o visor
                  </Button>
                ) : (
                  <Button
                    variant="secondary"
                    size="sm"
                    disabled={data?.atual.id !== p.id}
                    onClick={() =>
                      abrirVisorEcra()
                        .then((r) => toast.success(r === "segundo-monitor" ? "Ecrã do cliente aberto no segundo monitor." : "Ecrã do cliente aberto numa janela (só há um monitor)."))
                        .catch((e) => toast.error((e as Error).message))
                    }
                  >
                    <MonitorSmartphone className="size-4" /> Abrir o ecrã do cliente
                  </Button>
                )}
                <span className="text-[12.5px] text-ink-3">
                  {vs.modo === "serie"
                    ? "Mostra o artigo, o total e o troco. A linha 2 do teste deve mostrar a régua 1234… até ao fim."
                    : "Abre ao entrar no POS: em ecrã inteiro no segundo monitor, com os artigos, o total e o troco."}
                </span>
              </div>
            )}
          </div>
        </Modal>
      )}
    </Card>
  );
}

function Tabelas() {
  const t = useTabelas().data;
  const qc = useQueryClient();
  const [novo, setNovo] = useState<{ tabela: string; descricao: string; abreviatura?: string; percentagem?: number; codigo?: string; familia_id?: number } | null>(null);

  async function guardar() {
    try {
      await call("tabela.guardar", novo);
      toast.success("Guardado.");
      setNovo(null);
      qc.invalidateQueries({ queryKey: ["tabelas"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const bloco = (titulo: string, tabela: string, itens: { id: number; texto: string }[]) => (
    <Card>
      <div className="flex items-center justify-between px-5 pb-2 pt-4">
        <span className="font-semibold">{titulo}</span>
        <Button size="sm" variant="secondary" onClick={() => setNovo({ tabela, descricao: "", familia_id: t?.familias[0]?.id })}>
          <Plus className="size-4" /> Adicionar
        </Button>
      </div>
      <ul className="max-h-64 overflow-y-auto px-2 pb-3">
        {itens.map((i) => (
          <li key={i.id} className="rounded-md px-3 py-1.5 hover:bg-sunken">
            {i.texto}
          </li>
        ))}
      </ul>
    </Card>
  );

  return (
    <>
      <div className="grid grid-cols-2 gap-4">
        {bloco("Famílias", "familias", t?.familias.map((f) => ({ id: f.id, texto: f.descricao })) ?? [])}
        {bloco(
          "Subfamílias",
          "subfamilias",
          t?.subfamilias.map((s) => ({ id: s.id, texto: `${s.descricao} — ${t.familias.find((f) => f.id === s.familia_id)?.descricao ?? ""}` })) ?? [],
        )}
        {bloco("Unidades", "unidades", t?.unidades.map((u) => ({ id: u.id, texto: `${u.descricao} (${u.abreviatura})` })) ?? [])}
        {bloco("Armazéns", "armazens", t?.armazens.map((a) => ({ id: a.id, texto: a.nome })) ?? [])}
        {bloco("Taxas de IVA", "taxas", t?.taxas.map((x) => ({ id: x.id, texto: `${x.descricao} — ${x.percentagem}% (${x.codigo})` })) ?? [])}
      </div>
      {novo && (
        <Modal open onClose={() => setNovo(null)} title="Adicionar" width="max-w-md" footer={<Button onClick={guardar}>Guardar</Button>}>
          <div className="space-y-4">
            <Field label="Descrição">
              <Input autoFocus value={novo.descricao} onChange={(e) => setNovo({ ...novo, descricao: e.target.value })} />
            </Field>
            {novo.tabela === "subfamilias" && (
              <Field label="Família">
                <Select value={novo.familia_id} onChange={(e) => setNovo({ ...novo, familia_id: Number(e.target.value) })}>
                  {t?.familias.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.descricao}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            {novo.tabela === "unidades" && (
              <Field label="Abreviatura">
                <Input value={novo.abreviatura ?? ""} onChange={(e) => setNovo({ ...novo, abreviatura: e.target.value })} />
              </Field>
            )}
            {novo.tabela === "taxas" && (
              <>
                <Field label="Percentagem">
                  <Input type="number" value={novo.percentagem ?? 0} onChange={(e) => setNovo({ ...novo, percentagem: Number(e.target.value) })} />
                </Field>
                <Field label="Código SAF-T">
                  <Select value={novo.codigo ?? "NOR"} onChange={(e) => setNovo({ ...novo, codigo: e.target.value })}>
                    <option value="NOR">NOR — Normal</option>
                    <option value="RED">RED — Reduzida</option>
                    <option value="INT">INT — Intermédia</option>
                    <option value="ISE">ISE — Isenta</option>
                    <option value="OUT">OUT — Outra</option>
                  </Select>
                </Field>
              </>
            )}
          </div>
        </Modal>
      )}
    </>
  );
}

interface Copia {
  ficheiro: string;
  nome: string;
  bytes: number;
  data: string;
  verificada: boolean;
  cifrada: boolean;
}
interface EstadoCopias {
  pasta: string;
  copias: Copia[];
  externas: Copia[];
  config: { intervalo_horas: number; manter: number; pasta_externa: string; cifrar_externa: boolean; tem_senha: boolean };
  estado: { ultima: string | null; ultima_externa: string | null; erro_externa: string | null; pasta_externa_acessivel: boolean };
  posto_cliente: boolean;
}

function Sistema() {
  const { data, refetch } = useQuery({ queryKey: ["copias"], queryFn: () => call<EstadoCopias>("sistema.copias") });
  const [busy, setBusy] = useState<"" | "agora" | "exportar" | "gravar">("");
  const [f, setF] = useState<{ intervalo_horas: number; manter: number; pasta_externa: string; cifrar_externa: boolean; senha: string } | null>(null);
  const [restaurar, setRestaurar] = useState<{ ficheiro: string; cifrada: boolean; descricao: string } | null>(null);
  const [outro, setOutro] = useState("");
  useEffect(() => {
    if (data && !f) setF({ ...data.config, senha: "" });
  }, [data, f]);

  async function acao(tipo: "agora" | "exportar") {
    setBusy(tipo);
    try {
      const r = await call<{ ficheiro: string; externa?: string | null }>(tipo === "agora" ? "sistema.copiaAgora" : "sistema.backup");
      toast.success(tipo === "agora" ? "Cópia de segurança feita e verificada." : "Cópia exportada.", { description: r.externa ?? r.ficheiro, duration: 8000 });
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }
  async function gravar() {
    if (!f) return;
    setBusy("gravar");
    try {
      await call("sistema.copiasGuardar", { ...f, senha: f.senha || undefined });
      toast.success("Configuração das cópias guardada.");
      setF(null);
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }
  const mb = (b: number) => `${(b / 1024 / 1024).toFixed(2)} MB`;
  const linha = (c: Copia, onde: string) => (
    <tr key={c.ficheiro}>
      <Td>
        <div className="font-mono text-[12px]">{c.nome}</div>
        <div className="text-[11.5px] text-ink-3">{onde}</div>
      </Td>
      <Td className="text-ink-2">{dataHora(c.data)}</Td>
      <Td className="num text-right text-ink-2">{mb(c.bytes)}</Td>
      <Td>
        {c.verificada && <Badge tone="ok">verificada</Badge>} {c.cifrada && <Badge tone="brand">cifrada</Badge>}
      </Td>
      <Td className="text-right">
        {!data?.posto_cliente && (
          <Button size="sm" variant="secondary" onClick={() => setRestaurar({ ficheiro: c.ficheiro, cifrada: c.cifrada, descricao: `${c.nome} (${dataHora(c.data)})` })}>
            Restaurar
          </Button>
        )}
      </Td>
    </tr>
  );
  if (data?.posto_cliente) {
    return (
      <Card className="max-w-4xl p-6">
        <div className="mb-1 font-semibold">Cópias de segurança</div>
        <p className="text-ink-2">Este posto trabalha sobre a base de dados do servidor: as cópias de segurança fazem-se e restauram-se na máquina do servidor.</p>
      </Card>
    );
  }
  return (
    <div className="grid max-w-4xl gap-4">
      <Card className="p-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="mb-1 font-semibold">Cópias de segurança</div>
            <p className="text-ink-2">
              Automáticas ao abrir, de {data?.config.intervalo_horas || "—"} em {data?.config.intervalo_horas || "—"} horas e em cada fecho de caixa; cada cópia é verificada.
              Guarde-as também fora deste computador (pen, disco de rede ou pasta do OneDrive / Google Drive).
            </p>
            <p className="mt-2 text-[13px] text-ink-3">
              Última cópia: {data?.estado.ultima ? dataHora(data.estado.ultima) : "—"} · Última na pasta externa:{" "}
              {data?.estado.ultima_externa ? dataHora(data.estado.ultima_externa) : "—"}
            </p>
            {data?.estado.erro_externa && <p className="mt-1 text-[13px] text-danger">Pasta externa: {data.estado.erro_externa}</p>}
          </div>
          <div className="flex shrink-0 flex-col gap-2">
            <Button loading={busy === "agora"} onClick={() => acao("agora")}>
              <DatabaseBackup className="size-4" /> Fazer cópia agora
            </Button>
            <Button variant="secondary" loading={busy === "exportar"} onClick={() => acao("exportar")}>
              Exportar para Transferências
            </Button>
          </div>
        </div>
      </Card>
      {f && (
        <Card className="p-6">
          <div className="mb-3 font-semibold">Automáticas e pasta externa</div>
          <div className="grid grid-cols-3 gap-4">
            <Field label="Frequência">
              <Select value={f.intervalo_horas} onChange={(e) => setF({ ...f, intervalo_horas: Number(e.target.value) })}>
                <option value={0}>Só ao abrir o programa</option>
                {[1, 2, 4, 8, 12, 24].map((h) => (
                  <option key={h} value={h}>
                    De {h} em {h} horas
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Cópias a guardar">
              <Input className="num" type="number" min={3} max={365} value={f.manter} onChange={(e) => setF({ ...f, manter: Number(e.target.value) || 30 })} />
            </Field>
            <div />
            <Field label="Pasta externa (opcional)" className="col-span-3" hint="Ex.: E:\Copias KanGest (pen) ou C:\Users\…\OneDrive\KanGest. Se a pen não estiver ligada, a cópia local faz-se na mesma.">
              <Input className="font-mono" value={f.pasta_externa} onChange={(e) => setF({ ...f, pasta_externa: e.target.value })} placeholder="E:\Copias KanGest" />
            </Field>
            <label className="col-span-3 flex items-center gap-2 text-[13.5px]">
              <input type="checkbox" checked={f.cifrar_externa} onChange={(e) => setF({ ...f, cifrar_externa: e.target.checked })} />
              Cifrar as cópias da pasta externa com uma senha (protege os dados se a pen se perder)
            </label>
            {f.cifrar_externa && (
              <Field label="Senha das cópias" className="col-span-2" hint="Mínimo 8 caracteres. Guarde-a num sítio seguro: sem ela a cópia não se restaura noutro computador.">
                <Input type="password" value={f.senha} onChange={(e) => setF({ ...f, senha: e.target.value })} placeholder={data?.config.tem_senha ? "•••••• (manter)" : ""} />
              </Field>
            )}
          </div>
          <Button className="mt-4" loading={busy === "gravar"} onClick={gravar}>
            Guardar
          </Button>
        </Card>
      )}
      <Card>
        <div className="px-5 pt-4 font-semibold">Cópias guardadas</div>
        <div className="px-5 pb-2 text-[12px] text-ink-3" data-selectable>
          {data?.pasta}
        </div>
        <Table>
          <tbody>
            {data?.copias.map((c) => linha(c, "Neste computador"))}
            {data?.externas.map((c) => linha(c, "Pasta externa"))}
          </tbody>
        </Table>
        {!data?.copias.length && !data?.externas.length && <p className="py-6 text-center text-ink-3">Ainda não há cópias.</p>}
        <div className="flex items-end gap-2 border-t border-line px-5 py-4">
          <Field label="Restaurar outro ficheiro" className="flex-1" hint="Caminho completo de uma cópia .db ou .kgc (ex.: numa pen).">
            <Input className="font-mono" value={outro} onChange={(e) => setOutro(e.target.value)} placeholder="E:\Copias KanGest\kangest-….kgc" />
          </Field>
          <Button variant="secondary" disabled={!outro.trim()} onClick={() => setRestaurar({ ficheiro: outro.trim(), cifrada: outro.trim().endsWith(".kgc"), descricao: outro.trim() })}>
            Restaurar
          </Button>
        </div>
      </Card>
      {restaurar && <RestaurarCopia alvo={restaurar} temSenha={!!data?.config.tem_senha} onClose={() => setRestaurar(null)} metodo="sistema.restaurar" />}
    </div>
  );
}

/** Confirmação do restauro: guarda a base actual, marca a troca e reinicia. */
export function RestaurarCopia({
  alvo,
  temSenha,
  onClose,
  metodo,
}: {
  alvo: { ficheiro: string; cifrada: boolean; descricao: string };
  temSenha: boolean;
  onClose: () => void;
  metodo: "sistema.restaurar" | "setup.restaurar";
}) {
  const [senha, setSenha] = useState("");
  const [busy, setBusy] = useState(false);
  async function confirmar() {
    setBusy(true);
    try {
      await call(metodo, { ficheiro: alvo.ficheiro, senha: senha || undefined });
      toast.success("Cópia verificada. O KanGest vai reiniciar para a restaurar.");
      setTimeout(() => reiniciar(), 1200);
    } catch (e) {
      toast.error((e as Error).message);
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={onClose} title="Restaurar cópia de segurança" width="max-w-lg">
      <p className="text-ink-2">
        A base de dados passa a ser a da cópia <b className="break-all">{alvo.descricao}</b>. O que foi registado depois dessa cópia deixa de aparecer.
        {metodo === "sistema.restaurar" && " Antes, o KanGest guarda uma cópia da base actual (em «copias»)."} O programa reinicia no fim.
      </p>
      {alvo.cifrada && (
        <Field label="Senha da cópia" className="mt-4" hint={temSenha ? "Deixe vazio para usar a senha guardada neste computador." : undefined}>
          <Input type="password" autoFocus value={senha} onChange={(e) => setSenha(e.target.value)} />
        </Field>
      )}
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="secondary" onClick={onClose}>
          Cancelar
        </Button>
        <Button loading={busy} onClick={confirmar}>
          Restaurar e reiniciar
        </Button>
      </div>
    </Modal>
  );
}

function Agt() {
  const { data, refetch } = useQuery({
    queryKey: ["agt"],
    queryFn: () =>
      call<{
        chave_publica: string;
        certificado: string;
        certificado_anterior: string | null;
        chave_importada: string | null;
        chave_origem: "PRODUTOR" | "IMPORTADA" | "DESENVOLVIMENTO";
        certificado_embutido: boolean;
        exigir_certificacao: boolean;
        versao_chave: string;
      }>("agt.info"),
  });
  const [cert, setCert] = useState("");
  useEffect(() => data && setCert(data.certificado), [data]);

  async function carregarChave(f: File) {
    try {
      await call("agt.importarChave", { pem: await f.text() });
      toast.success("Chave do produtor carregada. A chave pública mostrada passou a ser a correspondente.");
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function guardar() {
    try {
      await call("agt.guardarCertificado", { certificado: cert });
      toast.success("Número de certificado guardado.");
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <div className="grid max-w-4xl gap-4">
      <Card className="p-6">
        <div className="mb-1 font-semibold">Assinatura dos documentos</div>
        <p className="mb-4 text-ink-2">
          Cada factura, recibo e nota de crédito é assinada com RSA-SHA1 sobre <code className="rounded bg-sunken px-1">Data;DataHora;Número;Total;HashAnterior</code>, formando uma
          cadeia por série. Os 4 caracteres de controlo (posições 1, 11, 21 e 31 do hash) são impressos no documento.
        </p>
        <Field
          label="N.º do certificado AGT"
          hint={
            data?.certificado_embutido
              ? "Vem com esta versão do KanGest (certificado do produtor)."
              : "Ex.: 123 — aparece como “Processado por programa válido n.º 123/AGT”."
          }
        >
          <div className="flex gap-2">
            <Input value={cert} onChange={(e) => setCert(e.target.value)} disabled={data?.certificado_embutido} />
            {!data?.certificado_embutido && <Button onClick={guardar}>Guardar</Button>}
          </div>
        </Field>
        {data && (
          <div className="mt-3 text-[13px] text-ink-3">Versão da chave nos documentos novos (HashControl do SAF-T): {data.versao_chave}</div>
        )}
      </Card>
      {data?.chave_origem === "DESENVOLVIMENTO" && (
        <div className="rounded-xl bg-accent-50 px-4 py-3 text-accent-ink">
          <b>Chave de desenvolvimento.</b> Os documentos estão a ser assinados com uma chave gerada nesta instalação, que a AGT não conhece. Use a versão oficial do
          KanGest, que traz a chave e o certificado do produtor.
        </div>
      )}
      {data?.certificado_anterior && (
        <Card className="p-6">
          <div className="mb-1 font-semibold">Certificado do sistema anterior (KIGEST)</div>
          <p className="text-ink-2">
            <Badge tone="neutral" className="mr-2">{data.certificado_anterior}</Badge>
            Importado como referência e usado apenas nas 2.ªs vias dos documentos históricos do KIGEST. Não foi activado no KanGest novo: o certificado pertence ao produtor e
            ao programa certificado pela AGT.
          </p>
        </Card>
      )}
      <Card className="p-6">
        <div className="mb-1 flex items-center justify-between">
          <span className="font-semibold">Chave de assinatura</span>
          {data && data.chave_origem !== "PRODUTOR" && (
            <label className="inline-flex h-8 cursor-pointer items-center rounded-lg border border-line-strong bg-surface px-3 text-[13px] hover:bg-sunken">
              Carregar chave do produtor (PEM)
              <input type="file" accept=".pem,.key,text/plain" className="hidden" onChange={(e) => e.target.files?.[0] && carregarChave(e.target.files[0])} />
            </label>
          )}
        </div>
        {data?.chave_origem === "PRODUTOR" ? (
          <p className="mb-3 text-ink-2">
            <Badge tone="ok" className="mr-2">Chave do produtor</Badge>
            A chave privada vem dentro do programa e não fica na base de dados nem nas cópias de segurança. Abaixo está a chave pública correspondente.
          </p>
        ) : (
          <p className="mb-3 text-ink-2">
            Só para testes e desenvolvimento: a versão oficial do KanGest já traz a chave do produtor. Uma chave carregada aqui fica guardada nesta instalação.
            {data?.chave_importada && <span className="ml-1 text-ink-3">Chave carregada em {data.chave_importada.replace("T", " ")}.</span>}
          </p>
        )}
        <pre data-selectable className="overflow-x-auto rounded-xl bg-[#1d2422] p-4 font-mono text-[12px] leading-relaxed text-white/90">
          {data?.chave_publica ?? "…"}
        </pre>
      </Card>
      <Integridade />
    </div>
  );
}

interface ResultadoIntegridade {
  ok: boolean;
  resumo: string;
  verificados: number;
  chaves: number;
  data: string;
  protecao: { ativa: boolean; em_falta: string[] };
  series: { tipo: string; serie: string; documentos: number; primeiro: string | null; ultimo: string | null; total_erros: number; erros: { documento: string; problema: string }[] }[];
}

/** Refaz a cadeia de assinaturas de todas as séries, como faria um auditor da AGT. */
function Integridade() {
  const [r, setR] = useState<ResultadoIntegridade | null>(null);
  const [busy, setBusy] = useState(false);
  async function verificar() {
    setBusy(true);
    try {
      setR(await call<ResultadoIntegridade>("agt.verificarIntegridade"));
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const comErro = r?.series.filter((s) => s.total_erros > 0) ?? [];
  return (
    <Card className="p-6">
      <div className="mb-1 flex items-center justify-between">
        <span className="font-semibold">Integridade da cadeia de assinaturas</span>
        <Button variant="secondary" loading={busy} onClick={verificar}>
          Verificar agora
        </Button>
      </div>
      <p className="mb-3 text-ink-2">
        Refaz a assinatura de cada documento, série a série, e confirma a numeração sem saltos, os caracteres de controlo e os totais. Os documentos assinados estão
        protegidos na base de dados: não se alteram nem se apagam (corrigem-se com nota de crédito ou anulação com motivo).
      </p>
      {r && (
        <div className="grid gap-3">
          <div className={cn("rounded-xl px-4 py-3 font-medium", r.ok ? "bg-ok-50 text-ok" : "bg-danger-50 text-danger")}>
            {r.resumo} <span className="font-normal text-ink-3">({dataHora(r.data)}, {r.chaves} chave(s) pública(s))</span>
          </div>
          {!r.protecao.ativa && (
            <div className="rounded-xl bg-danger-50 px-4 py-3 text-danger">
              A protecção dos documentos assinados foi removida da base de dados ({r.protecao.em_falta.join(", ")}). Contacte o suporte: a base pode ter sido mexida fora do KanGest.
            </div>
          )}
          {comErro.map((s) => (
            <div key={s.tipo + s.serie} className="rounded-xl border border-line p-3">
              <div className="mb-2 font-medium">
                {s.tipo} {s.serie}: {s.total_erros} problema(s) em {s.documentos} documento(s)
              </div>
              <ul className="grid gap-1 text-[13px]">
                {s.erros.map((e, i) => (
                  <li key={i}>
                    <Badge tone="danger" className="mr-2">{e.documento}</Badge>
                    {e.problema}
                  </li>
                ))}
                {s.total_erros > s.erros.length && <li className="text-ink-3">… e mais {s.total_erros - s.erros.length}.</li>}
              </ul>
            </div>
          ))}
          {r.ok && <div className="text-[13px] text-ink-3">{r.series.map((s) => `${s.tipo} ${s.serie}: ${s.documentos}`).join(" · ")}</div>}
        </div>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------- rede (postos ligados)

interface EstadoRede {
  modo: "local" | "servidor" | "cliente";
  porta: number;
  servidor: string;
  chave: string;
  /** A chave em uso tem menos de 10 caracteres (aceite, mas fraca). */
  chave_curta?: boolean;
  servidor_activo: number | null;
  enderecos: string[];
  ligados: { posto_id: number; posto: string; utilizador: string | null; endereco: string; ultimo: string; pedidos: number }[];
}

const MODOS = [
  { id: "local", label: "Sozinho", icon: Monitor, texto: "Esta máquina tem a sua própria base de dados. É o normal numa só caixa." },
  { id: "servidor", label: "Servidor", icon: Server, texto: "Esta máquina guarda a base de dados e serve os outros postos da rede local." },
  { id: "cliente", label: "Posto em rede", icon: Wifi, texto: "Esta máquina trabalha sobre a base de dados do servidor." },
] as const;

/** Chave de rede forte: 20 caracteres sem ambiguidades (sem 0/O, 1/I/L), em grupos de 5. */
function gerarChaveRede() {
  const alfabeto = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";
  const b = crypto.getRandomValues(new Uint8Array(20));
  const c = Array.from(b, (x) => alfabeto[x % alfabeto.length]).join("");
  return c.match(/.{5}/g)!.join("-");
}

/** Vários postos ligados ao mesmo servidor (substitui o SQL Server partilhado do KIGEST). */
function Rede() {
  const [f, setF] = useState<Partial<EstadoRede> | null>(null);
  const [busy, setBusy] = useState(false);
  const { data: r, refetch } = useQuery({
    queryKey: ["rede"],
    queryFn: () => call<EstadoRede>("rede.estado"),
    // enquanto serve postos, a lista de ligados vai-se actualizando
    refetchInterval: (q) => (q.state.data?.modo === "servidor" ? 5000 : false),
  });
  const v = { ...r, ...f } as EstadoRede;
  const set = (patch: Partial<EstadoRede>) => setF({ ...f, ...patch });

  async function guardar() {
    setBusy(true);
    try {
      const x = await call<{ aviso: string | null }>("rede.guardar", { modo: v.modo, porta: Number(v.porta) || 7799, servidor: v.servidor ?? "", chave: v.chave ?? "" });
      toast.success("Configuração de rede guardada.", { description: x.aviso ?? undefined, duration: x.aviso ? 10000 : 4000 });
      setF(null);
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function testar() {
    try {
      const x = await call<{ empresa: string | null }>("rede.testar");
      toast.success(`Ligado ao servidor${x.empresa ? `: ${x.empresa}` : ""}.`);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  if (!r) return <Card className="max-w-3xl"><div className="p-6 text-ink-3">A carregar…</div></Card>;

  return (
    <div className="max-w-3xl space-y-5">
      <Card>
        <div className="flex items-center gap-2 border-b border-line p-4">
          <Network className="size-4 text-ink-3" />
          <span className="font-medium">Postos em rede</span>
          {r.servidor_activo && <Badge tone="ok">a servir na porta {r.servidor_activo}</Badge>}
        </div>
        <div className="space-y-3 p-4">
          {MODOS.map((m) => (
            <label key={m.id} className={cn("flex cursor-pointer gap-3 rounded-xl border p-3", v.modo === m.id ? "border-brand bg-brand-50/50" : "border-line hover:bg-sunken/60")}>
              <input type="radio" className="mt-1" checked={v.modo === m.id} onChange={() => set({ modo: m.id })} />
              <div>
                <div className="flex items-center gap-2 font-medium">
                  <m.icon className="size-4" /> {m.label}
                </div>
                <div className="text-[12.5px] text-ink-3">{m.texto}</div>
              </div>
            </label>
          ))}

          {v.modo !== "local" && (
            <div className="grid grid-cols-3 gap-4 pt-2">
              {v.modo === "cliente" && (
                <Field label="Endereço do servidor" className="col-span-2" hint="O IP da máquina que guarda a base de dados">
                  <Input className="font-mono" placeholder="192.168.1.10" value={v.servidor ?? ""} onChange={(e) => set({ servidor: e.target.value })} />
                </Field>
              )}
              <Field label="Porta">
                <Input className="num" value={v.porta ?? 7799} onChange={(e) => set({ porta: Number(e.target.value.replace(/\D/g, "")) || 0 })} />
              </Field>
              <Field
                label="Chave de rede"
                className={v.modo === "cliente" ? "col-span-3" : "col-span-2"}
                hint="Igual em todos os postos (mínimo 10 caracteres). Nunca passa na rede: serve para cifrar os pedidos."
              >
                <div className="flex gap-2">
                  <Input className="font-mono" value={v.chave ?? ""} onChange={(e) => set({ chave: e.target.value })} />
                  {v.modo === "servidor" && (
                    <Button variant="secondary" onClick={() => set({ chave: gerarChaveRede() })}>
                      Gerar chave
                    </Button>
                  )}
                </div>
              </Field>
              {r.chave_curta && (
                <p className="col-span-3 rounded-lg bg-accent-50 px-3 py-2 text-[12.5px] text-accent-ink">
                  A chave de rede em uso é curta. Gere uma chave nova no servidor e escreva-a em cada posto.
                </p>
              )}
            </div>
          )}

          {v.modo === "servidor" && !!r.enderecos.length && (
            <p className="text-[12.5px] text-ink-2">
              Nos outros postos indique o endereço <b className="font-mono">{r.enderecos.join(" ou ")}</b> e esta porta. A base de dados fica só aqui — as cópias de
              segurança desta máquina passam a guardar o trabalho de todos.
            </p>
          )}
          {v.modo === "cliente" && (
            <p className="text-[12.5px] text-ink-2">
              Os talões e a gaveta continuam a ser desta máquina; tudo o resto (artigos, stock, documentos, caixa) é do servidor. Sem rede, o posto não vende.
            </p>
          )}

          <div className="flex items-center gap-2 pt-1">
            <Button loading={busy} onClick={guardar}>
              Guardar
            </Button>
            {r.modo === "cliente" && (
              <Button variant="secondary" onClick={testar}>
                <Wifi className="size-4" /> Testar ligação
              </Button>
            )}
            {f && (
              <Button variant="ghost" onClick={() => setF(null)}>
                Cancelar
              </Button>
            )}
          </div>
        </div>
      </Card>

      {r.modo === "servidor" && (
        <Card>
          <div className="border-b border-line p-4 font-medium">Postos ligados</div>
          {r.ligados.length ? (
            <Table>
              <thead>
                <tr>
                  <Th>Posto</Th>
                  <Th>Utilizador</Th>
                  <Th>Endereço</Th>
                  <Th>Último pedido</Th>
                  <Th className="text-right">Pedidos</Th>
                </tr>
              </thead>
              <tbody>
                {r.ligados.map((l) => (
                  <tr key={`${l.posto_id}-${l.endereco}`}>
                    <Td className="font-medium">{l.posto || `Posto #${l.posto_id}`}</Td>
                    <Td>{l.utilizador ?? <span className="text-ink-3">sem sessão</span>}</Td>
                    <Td className="font-mono text-[12.5px]">{l.endereco}</Td>
                    <Td className="text-ink-2">{l.ultimo.replace("T", " ").slice(0, 19)}</Td>
                    <Td className="num text-right">{l.pedidos}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          ) : (
            <p className="p-6 text-ink-3">Nenhum posto se ligou desde que a aplicação abriu.</p>
          )}
        </Card>
      )}
    </div>
  );
}

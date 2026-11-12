import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AlertTriangle, CheckCircle2, Database, Download, FileArchive, FileUp, Info, Printer, RefreshCw, ShieldAlert, XCircle } from "lucide-react";
import { exportarFicheiro } from "@/lib/csv";
import { call } from "@/lib/api";
import { imprimir } from "@/lib/print";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { dataHora, kz, numero, qtd } from "@/lib/format";
import exportadorPs1 from "../../ferramentas/exportar-kigest/ExportarKIGEST.ps1?raw";
import exportadorBat from "../../ferramentas/exportar-kigest/ExportarKIGEST.bat?raw";
import exportadorLeiaMe from "../../ferramentas/exportar-kigest/LEIA-ME.txt?raw";

interface Conexao {
  servidor: string;
  instancia: string;
  porta: string;
  base: string;
  utilizador: string;
  senha: string;
}
interface Analise {
  tabelas_total: number;
  reconhecidas: { conjunto: string; tabela: string | null; registos?: number; candidatos?: string[]; colunas?: string[] }[];
  nao_importadas?: { tabela: string; registos: number; colunas: string[] }[];
  /** Só na análise de um ficheiro exportado. */
  ficheiro?: { origem: string; data: string; computador: string; completo: boolean; erros: string[]; registos: number };
}
interface FicheiroKigest {
  caminho: string;
  nome: string;
  tamanho: number;
  modificado: string;
}

const tamanho = (b: number) => (b >= 1048576 ? `${numero(b / 1048576, 1)} MB` : `${numero(Math.max(1, Math.round(b / 1024)))} KB`);
const pastaDe = (caminho: string) => caminho.slice(0, Math.max(caminho.lastIndexOf("/"), caminho.lastIndexOf("\\")));
interface Relatorio {
  origem: string;
  contagens: { conjunto: string; registos: number }[];
  avisos: string[];
  senhas: { login: string; senha: string }[];
  simulacao?: boolean;
  conferencia?: Conferencia[];
}
/** Total no KIGEST ao lado do que ficou no KanGest ("kz" em cêntimos). */
interface Conferencia {
  item: string;
  unidade: "n" | "qtd" | "kz";
  kigest: number;
  kigest_novo: number;
}

const valorConf = (c: Conferencia, v: number) => (c.unidade === "kz" ? kz(v) : c.unidade === "qtd" ? qtd(v) : numero(v));
const difConf = (c: Conferencia) => (c.unidade === "qtd" ? Math.round((c.kigest_novo - c.kigest) * 1000) / 1000 : c.kigest_novo - c.kigest);

const NOMES: Record<string, string> = {
  empresa: "Empresa (dados, logótipo, certificado)",
  taxas: "Taxas de IVA",
  motivos: "Motivos de isenção",
  unidades: "Unidades",
  familias: "Famílias",
  subfamilias: "Subfamílias",
  armazens: "Armazéns",
  artigos: "Artigos e preços",
  stock: "Existências por armazém",
  clientes: "Clientes",
  fornecedores: "Fornecedores",
  utilizadores: "Utilizadores",
  tipos: "Tipos de documento",
  documentos: "Documentos (cabeçalhos)",
  linhas: "Documentos (linhas)",
  pagamentos: "Modos de pagamento dos documentos",
  conta_corrente: "Conta corrente (valores em dívida)",
  rh_funcionarios: "Funcionários",
  rh_departamentos: "Departamentos",
  rh_funcoes: "Funções",
  rh_categorias: "Categorias profissionais",
  rh_agregado: "Agregado familiar",
  rh_ferias: "Mapa de férias",
  lotes: "Lotes de artigos",
  viaturas: "Viaturas (oficina)",
  ordens: "Ordens de serviço",
  ordem_linhas: "Ordens de serviço (peças e serviços)",
  recibos: "Recibos (histórico)",
  recibo_linhas: "Recibos (documentos liquidados)",
  recibo_pagamentos: "Recibos (meios de pagamento)",
  retencoes: "Retenções na fonte",
  custos: "Custos de importação das compras",
  tipos_custo: "Tipos de custo de importação",
  proformas: "Proformas e orçamentos",
  proforma_linhas: "Proformas (linhas)",
  tes_contas: "Contas do fluxo de caixa",
  bancos: "Bancos",
  bancos_contas: "Contas bancárias",
  centros_custo: "Centros de custo",
  tes_movimentos: "Movimentos do fluxo de caixa",
  caixa_fechos: "Fechos de caixa (POS)",
  moedas: "Moedas e câmbios",
  prazos: "Prazos de pagamento",
  rh_alteracoes: "Alterações mensais",
  rh_adiantamentos: "Adiantamentos salariais",
  rh_proc_cab: "Processamentos salariais",
  rh_proc_linhas: "Recibos de salário (linhas)",
  rh_honorarios: "Honorários (conta própria)",
};

const OPCOES = [
  { id: "empresa", label: "Empresa", hint: "Nome, NIF, morada, bancos, logótipo" },
  { id: "artigos", label: "Artigos", hint: "Famílias, unidades, IVA, preços e custos" },
  { id: "stock", label: "Stock", hint: "Existências por armazém (saldo inicial)" },
  { id: "clientes", label: "Clientes", hint: "Fichas de clientes" },
  { id: "fornecedores", label: "Fornecedores", hint: "Fichas de fornecedores" },
  { id: "utilizadores", label: "Utilizadores", hint: "Com senhas provisórias" },
  { id: "documentos", label: "Documentos e conta corrente", hint: "Histórico com as assinaturas originais e valores em dívida" },
  { id: "rh", label: "Recursos humanos", hint: "Funcionários, salários e subsídios, agregado familiar, férias" },
  { id: "lotes", label: "Lotes", hint: "Saldo e validade por lote" },
  { id: "oficina", label: "Oficina", hint: "Viaturas e ordens de serviço" },
  { id: "tesouraria", label: "Tesouraria e caixa", hint: "Contas, bancos, centros de custo, movimentos, fechos de caixa, moedas e prazos" },
] as const;

/** Lê o Conexao.ini do KIGEST ([DATA] Source, BDados, User, Senha). */
function lerIni(texto: string): Partial<Conexao> {
  const v = (k: string) => texto.match(new RegExp(`^\\s*${k}\\s*=\\s*(.*)$`, "im"))?.[1]?.trim();
  const source = v("Source") ?? "";
  const [servidor, instancia] = source.split("\\");
  return { servidor: servidor?.trim(), instancia: instancia?.trim() ?? "", base: v("BDados"), utilizador: v("User"), senha: v("Senha") };
}

export default function Importar() {
  const qc = useQueryClient();
  const [c, setC] = useState<Conexao>({ servidor: "", instancia: "", porta: "", base: "KigestGC", utilizador: "sa", senha: "" });
  const [opcoes, setOpcoes] = useState<Record<string, boolean>>(Object.fromEntries(OPCOES.map((o) => [o.id, true])));
  const [analise, setAnalise] = useState<Analise | null>(null);
  const [relatorio, setRelatorio] = useState<Relatorio | null>(null);
  const [busy, setBusy] = useState<"" | "analisar" | "importar" | "simular">("");
  const [confirmar, setConfirmar] = useState(false);
  const [fonte, setFonte] = useState<"ficheiro" | "sqlserver">("ficheiro");
  const [caminho, setCaminho] = useState("");
  const [encontrados, setEncontrados] = useState<FicheiroKigest[] | null>(null);
  const [procurando, setProcurando] = useState(false);

  const conexao = () => ({ ...c, instancia: c.instancia || null, porta: c.porta ? Number(c.porta) : null });
  const executar = (op: Record<string, boolean>) =>
    fonte === "ficheiro" ? call<Relatorio>("importar.executarFicheiro", { caminho, opcoes: op }) : call<Relatorio>("importar.executar", { conexao: conexao(), opcoes: op });

  async function procurar() {
    setProcurando(true);
    try {
      const l = await call<FicheiroKigest[]>("importar.ficheiros");
      setEncontrados(l);
      if (l.length && !caminho) setCaminho(l[0].caminho);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setProcurando(false);
    }
  }

  useEffect(() => {
    procurar();
    // ficheiro arrastado para a janela (só na aplicação desktop)
    if (!("__TAURI_INTERNALS__" in window)) return;
    let desligar: (() => void) | undefined;
    let activo = true;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((ev) => {
          if (ev.payload.type !== "drop") return;
          const f = ev.payload.paths.find((x) => x.toLowerCase().endsWith(".kigest"));
          if (f) {
            setFonte("ficheiro");
            setCaminho(f);
            setAnalise(null);
            setRelatorio(null);
          } else toast.error("Arraste um ficheiro .kigest criado pelo ExportarKIGEST.");
        }),
      )
      .then((u) => (activo ? (desligar = u) : u()))
      .catch(() => {});
    return () => {
      activo = false;
      desligar?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function guardarExportador() {
    try {
      const crlf = (t: string) => t.replace(/\r?\n/g, "\r\n");
      const bom = (t: string) => (t.startsWith("\uFEFF") ? t : "\uFEFF" + t);
      await call("sistema.exportar", { nome: "ExportarKIGEST.ps1", conteudo: bom(exportadorPs1) });
      await call("sistema.exportar", { nome: "ExportarKIGEST.bat", conteudo: crlf(exportadorBat) });
      const r = await call<{ ficheiro: string }>("sistema.exportar", { nome: "ExportarKIGEST-LEIA-ME.txt", conteudo: bom(crlf(exportadorLeiaMe)) });
      toast.success("ExportarKIGEST guardado", {
        description: `${pastaDe(r.ficheiro)} — copie ExportarKIGEST.bat e ExportarKIGEST.ps1 para o computador do KIGEST.`,
        duration: 15000,
      });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function analisar() {
    setBusy("analisar");
    setAnalise(null);
    setRelatorio(null);
    try {
      setAnalise(fonte === "ficheiro" ? await call<Analise>("importar.analisarFicheiro", { caminho }) : await call<Analise>("importar.analisar", conexao()));
    } catch (e) {
      toast.error((e as Error).message, { duration: 12000 });
    } finally {
      setBusy("");
    }
  }

  async function simular() {
    setBusy("simular");
    try {
      setRelatorio(await executar({ ...opcoes, simular: true }));
      toast.success("Simulação concluída: nada foi gravado.");
    } catch (e) {
      toast.error((e as Error).message, { duration: 15000 });
    } finally {
      setBusy("");
    }
  }

  async function importar() {
    setConfirmar(false);
    setBusy("importar");
    try {
      const copia = await call<{ ficheiro: string }>("sistema.backup");
      toast.info("Cópia de segurança feita antes da importação.", { description: copia.ficheiro });
      const r = await executar(opcoes);
      setRelatorio(r);
      qc.invalidateQueries();
      toast.success("Importação concluída.");
    } catch (e) {
      toast.error((e as Error).message, { duration: 15000 });
    } finally {
      setBusy("");
    }
  }

  return (
    <>
      <PageHeader title="Importar dados do KIGEST" subtitle="Copia todos os dados do KIGEST GC/POS: empresa, artigos, stock, entidades, documentos, tesouraria, recursos humanos e oficina" />

      <div className="grid grid-cols-[minmax(0,1fr)_360px] gap-4">
        <div className="space-y-4">
          <div className="inline-flex rounded-xl border border-line bg-surface p-1">
            {(
              [
                ["ficheiro", "Ficheiro exportado (.kigest)"],
                ["sqlserver", "Ligação directa ao SQL Server"],
              ] as const
            ).map(([id, rotulo]) => (
              <button
                key={id}
                type="button"
                onClick={() => {
                  setFonte(id);
                  setAnalise(null);
                  setRelatorio(null);
                }}
                className={cn("rounded-lg px-4 py-1.5 text-[13px] font-medium", fonte === id ? "bg-brand text-white shadow-sm" : "text-ink-2 hover:bg-sunken")}
              >
                {rotulo}
              </button>
            ))}
          </div>

          {fonte === "ficheiro" && (
            <Card className="p-6">
              <div className="mb-4 flex items-center justify-between gap-3">
                <div className="flex items-center gap-2 font-semibold">
                  <FileArchive className="size-5 text-brand" /> 1. Ficheiro exportado do KIGEST
                </div>
                <Button variant="secondary" size="sm" onClick={guardarExportador} title="Grava o ExportarKIGEST na pasta Transferências, para copiar para o computador do KIGEST">
                  <Download className="size-4" /> Guardar o ExportarKIGEST
                </Button>
              </div>
              <ol className="mb-4 list-decimal space-y-1 pl-5 text-[13px] text-ink-2">
                <li>No computador onde está o KIGEST, feche o GC e o POS e faça duplo clique em <b>ExportarKIGEST.bat</b>.</li>
                <li>Traga o ficheiro <span className="font-mono text-[12.5px]">KIGEST-exportacao-….kigest</span> para este computador (pen, rede ou email).</li>
                <li>Escolha-o abaixo ou arraste-o para esta janela, e carregue em Analisar.</li>
              </ol>
              <div className="rounded-xl border border-line">
                <div className="flex items-center justify-between border-b border-line px-4 py-2 text-[12.5px] text-ink-3">
                  <span>Ficheiros encontrados (Transferências, Ambiente de Trabalho, Documentos e pens)</span>
                  <Button variant="ghost" size="sm" loading={procurando} onClick={procurar}>
                    <RefreshCw className="size-4" /> Procurar
                  </Button>
                </div>
                {encontrados && encontrados.length === 0 && <div className="px-4 py-3 text-[13px] text-ink-3">Nenhum ficheiro .kigest encontrado.</div>}
                {!!encontrados?.length && (
                  <ul className="max-h-48 divide-y divide-line overflow-auto">
                    {encontrados.map((f) => (
                      <li key={f.caminho}>
                        <label className={cn("flex cursor-pointer items-center gap-3 px-4 py-2 hover:bg-sunken", caminho === f.caminho && "bg-brand-50")}>
                          <input
                            type="radio"
                            name="ficheiro-kigest"
                            checked={caminho === f.caminho}
                            onChange={() => {
                              setCaminho(f.caminho);
                              setAnalise(null);
                              setRelatorio(null);
                            }}
                          />
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-medium">{f.nome}</span>
                            <span className="block truncate text-[12px] text-ink-3">{pastaDe(f.caminho)}</span>
                          </span>
                          <span className="num shrink-0 text-right text-[12px] text-ink-3">
                            {tamanho(f.tamanho)}
                            <br />
                            {dataHora(f.modificado)}
                          </span>
                        </label>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              <Field label="Caminho do ficheiro" className="mt-4">
                <Input
                  value={caminho}
                  placeholder="Escolha um ficheiro da lista, arraste-o para a janela ou cole o caminho"
                  onChange={(e) => {
                    setCaminho(e.target.value);
                    setAnalise(null);
                  }}
                />
              </Field>
              <div className="mt-5 flex items-center gap-3">
                <Button loading={busy === "analisar"} disabled={!caminho.trim() || !!busy} onClick={analisar}>
                  Analisar ficheiro
                </Button>
                <span className="text-[12px] text-ink-3">O ficheiro só é lido. Depois da importação pode apagá-lo.</span>
              </div>
            </Card>
          )}

          {fonte === "sqlserver" && (
            <Card className="p-6">
              <div className="mb-5 flex items-center justify-between">
                <div className="flex items-center gap-2 font-semibold">
                  <Database className="size-5 text-brand" /> 1. Ligação ao SQL Server do KIGEST
                </div>
                <label className="inline-flex h-8 cursor-pointer items-center gap-2 rounded-lg border border-line-strong bg-surface px-3 text-[13px] hover:bg-sunken">
                  <FileUp className="size-4" /> Ler Conexao.ini
                  <input
                    type="file"
                    accept=".ini,text/plain"
                    className="hidden"
                    onChange={async (e) => {
                      const f = e.target.files?.[0];
                      if (!f) return;
                      const lido = lerIni(await f.text());
                      setC((x) => ({ ...x, ...Object.fromEntries(Object.entries(lido).filter(([, v]) => v !== undefined && v !== "")) }));
                      toast.success("Dados de ligação lidos do Conexao.ini.");
                    }}
                  />
                </label>
              </div>
              <div className="grid grid-cols-6 gap-4">
                <Field label="Servidor (nome ou IP)" className="col-span-3" hint="Ex.: DESKTOP-G26LDJM ou 192.168.1.20">
                  <Input value={c.servidor} onChange={(e) => setC({ ...c, servidor: e.target.value })} />
                </Field>
                <Field label="Instância" className="col-span-2" hint="Vazio = instância por omissão">
                  <Input value={c.instancia} placeholder="SQLEXPRESS" onChange={(e) => setC({ ...c, instancia: e.target.value })} />
                </Field>
                <Field label="Porta">
                  <Input value={c.porta} placeholder="1433" onChange={(e) => setC({ ...c, porta: e.target.value.replace(/\D/g, "") })} />
                </Field>
                <Field label="Base de dados" className="col-span-2">
                  <Input value={c.base} onChange={(e) => setC({ ...c, base: e.target.value })} />
                </Field>
                <Field label="Utilizador" className="col-span-2">
                  <Input value={c.utilizador} onChange={(e) => setC({ ...c, utilizador: e.target.value })} />
                </Field>
                <Field label="Senha" className="col-span-2">
                  <Input type="password" value={c.senha} onChange={(e) => setC({ ...c, senha: e.target.value })} />
                </Field>
              </div>
              <div className="mt-5 flex items-center gap-3">
                <Button loading={busy === "analisar"} disabled={!c.servidor || !c.base} onClick={analisar}>
                  Ligar e analisar
                </Button>
                <span className="text-[12px] text-ink-3">A senha só é usada nesta ligação e não fica guardada.</span>
              </div>
            </Card>
          )}

          {analise && (
            <Card>
              {analise.ficheiro && (
                <div className={cn("mx-5 mt-4 flex items-start gap-2 rounded-lg px-3 py-2 text-[13px]", analise.ficheiro.completo ? "bg-sunken text-ink-2" : "bg-danger-50 text-danger")}>
                  {analise.ficheiro.completo ? <Info className="mt-0.5 size-4 shrink-0 text-brand" /> : <AlertTriangle className="mt-0.5 size-4 shrink-0" />}
                  <span>
                    Exportado de <b>{analise.ficheiro.origem || analise.ficheiro.computador}</b> em {dataHora(analise.ficheiro.data.replace(" ", "T"))} ·{" "}
                    <span className="num">{numero(analise.ficheiro.registos)}</span> registos
                    {!analise.ficheiro.completo && " — o ficheiro está incompleto (a exportação não terminou): volte a correr o ExportarKIGEST."}
                    {analise.ficheiro.erros.length > 0 && <span className="block text-[12px] text-[#9a6514]">Não exportadas pelo KIGEST: {analise.ficheiro.erros.join(" · ")}</span>}
                  </span>
                </div>
              )}
              <div className="flex items-center justify-between px-5 pb-2 pt-4">
                <span className="font-semibold">2. Tabelas encontradas</span>
                <span className="flex items-center gap-3 text-[13px] text-ink-3">
                  {analise.tabelas_total} tabelas e vistas {analise.ficheiro ? "no ficheiro" : "na base"}
                  <Button
                    variant="ghost"
                    size="sm"
                    title="Grava a análise (tabelas, colunas e registos) para enviar ao suporte"
                    onClick={() => exportarFicheiro(`analise-kigest-${new Date().toISOString().slice(0, 10)}.json`, JSON.stringify(analise, null, 2))}
                  >
                    <Download className="size-4" /> Exportar análise
                  </Button>
                </span>
              </div>
              <Table>
                <thead>
                  <tr>
                    <Th>Conjunto</Th>
                    <Th>Tabela do KIGEST</Th>
                    <Th className="text-right">Registos</Th>
                  </tr>
                </thead>
                <tbody>
                  {analise.reconhecidas.map((r) => (
                    <tr key={r.conjunto}>
                      <Td>
                        <span className="inline-flex items-center gap-2">
                          {r.tabela ? <CheckCircle2 className="size-4 shrink-0 text-ok" /> : <XCircle className="size-4 shrink-0 text-ink-3" />}
                          {NOMES[r.conjunto] ?? r.conjunto}
                        </span>
                      </Td>
                      <Td className={cn("font-mono text-[12.5px]", !r.tabela && "text-ink-3")} title={r.colunas?.join(", ")}>
                        {r.tabela ?? `não encontrada (${r.candidatos?.join(", ")})`}
                      </Td>
                      <Td className="num text-right">{r.tabela ? (r.registos ?? 0) >= 0 ? r.registos : "vista" : "—"}</Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
              {!!analise.nao_importadas?.length && (
                <details className="border-t border-line px-5 py-3 text-[13px]">
                  <summary className="cursor-pointer text-ink-2">
                    {analise.nao_importadas.length} outra(s) tabela(s) com dados que o KanGest não importa (configurações internas do KIGEST, temporárias…)
                  </summary>
                  <ul className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1">
                    {analise.nao_importadas.map((x) => (
                      <li key={x.tabela} className="flex justify-between font-mono text-[12px] text-ink-3" title={x.colunas.join(", ")}>
                        <span>{x.tabela}</span>
                        <span className="num">{x.registos}</span>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </Card>
          )}

          {relatorio && (
            <Card className="p-6">
              <div className="mb-4 flex items-center justify-between">
                <div className="flex items-center gap-2 font-semibold">
                  {relatorio.simulacao ? (
                    <>
                      <Info className="size-5 text-accent" /> Simulação — nada foi gravado · {relatorio.origem}
                    </>
                  ) : (
                    <>
                      <CheckCircle2 className="size-5 text-ok" /> Importação concluída — {relatorio.origem}
                    </>
                  )}
                </div>
                <Button variant="secondary" size="sm" onClick={imprimir}>
                  <Printer className="size-4" /> Imprimir relatório
                </Button>
              </div>
              <div className="grid grid-cols-3 gap-3">
                {relatorio.contagens.map((x) => (
                  <div key={x.conjunto} className="rounded-xl border border-line px-4 py-3">
                    <div className="text-[12px] text-ink-3">{x.conjunto}</div>
                    <div className="num text-[20px] font-semibold">{numero(x.registos)}</div>
                  </div>
                ))}
              </div>
              {(relatorio.conferencia?.length ?? 0) > 0 && (
                <div className="mt-5">
                  <div className="mb-2 font-medium">Conferência KIGEST × KanGest</div>
                  <Table>
                    <thead>
                      <tr>
                        <Th>Conjunto</Th>
                        <Th className="text-right">No KIGEST</Th>
                        <Th className="text-right">No KanGest</Th>
                        <Th className="text-right">Diferença</Th>
                      </tr>
                    </thead>
                    <tbody>
                      {relatorio.conferencia!.map((c) => {
                        const d = difConf(c);
                        return (
                          <tr key={c.item}>
                            <Td>{c.item}</Td>
                            <Td className="num text-right">{valorConf(c, c.kigest)}</Td>
                            <Td className="num text-right">{valorConf(c, c.kigest_novo)}</Td>
                            <Td className="text-right">
                              {d === 0 ? (
                                <Badge tone="ok">Igual</Badge>
                              ) : (
                                <span className="num font-medium text-[#9a6514]">
                                  {d > 0 ? "+" : ""}
                                  {valorConf(c, d)}
                                </span>
                              )}
                            </Td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </Table>
                  <p className="mt-2 text-[12px] text-ink-3">
                    «No KanGest» conta só o que veio do KIGEST. Diferenças normais: artigos com a mesma referência juntam-se num só e os registos sem artigo,
                    conta ou documento conhecido ficam de fora (ver os avisos abaixo).
                  </p>
                </div>
              )}
              {relatorio.avisos.length > 0 && (
                <ul className="mt-4 space-y-2">
                  {relatorio.avisos.map((a) => (
                    <li key={a} className="flex gap-2 rounded-lg bg-accent-50 px-3 py-2 text-[13px] text-ink-2">
                      <Info className="mt-0.5 size-4 shrink-0 text-[#9a6514]" />
                      {a}
                    </li>
                  ))}
                </ul>
              )}
              {relatorio.senhas.length > 0 && (
                <div className="mt-4">
                  <div className="mb-2 font-medium">Senhas provisórias dos utilizadores importados</div>
                  <Table>
                    <thead>
                      <tr>
                        <Th>Utilizador</Th>
                        <Th>Senha provisória</Th>
                      </tr>
                    </thead>
                    <tbody>
                      {relatorio.senhas.map((s) => (
                        <tr key={s.login}>
                          <Td className="font-mono">{s.login}</Td>
                          <Td className="font-mono" >{s.senha}</Td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </div>
              )}
              <PrintPortal>
                <div className="p-2 text-[12px] text-black">
                  <div className="text-[16px] font-bold">Relatório de importação do KIGEST</div>
                  <div className="mb-3">{relatorio.origem}</div>
                  {relatorio.contagens.map((x) => (
                    <div key={x.conjunto}>
                      {x.conjunto}: {x.registos}
                    </div>
                  ))}
                  {(relatorio.conferencia?.length ?? 0) > 0 && (
                    <div className="mt-3">
                      <b>Conferência KIGEST × KanGest</b>
                      {relatorio.conferencia!.map((c) => (
                        <div key={c.item}>
                          {c.item}: {valorConf(c, c.kigest)} → {valorConf(c, c.kigest_novo)}
                          {difConf(c) === 0 ? " (igual)" : ` (diferença ${valorConf(c, difConf(c))})`}
                        </div>
                      ))}
                    </div>
                  )}
                  {relatorio.avisos.map((a) => (
                    <p key={a} className="mt-2">
                      • {a}
                    </p>
                  ))}
                  {relatorio.senhas.length > 0 && (
                    <div className="mt-3">
                      <b>Senhas provisórias</b>
                      {relatorio.senhas.map((s) => (
                        <div key={s.login}>
                          {s.login}: {s.senha}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </PrintPortal>
            </Card>
          )}
        </div>

        <div className="space-y-4">
          <Card className="p-5">
            <div className="mb-3 font-semibold">3. O que importar</div>
            <div className="space-y-2.5">
              {OPCOES.map((o) => (
                <label key={o.id} className="flex items-start gap-2.5">
                  <input type="checkbox" className="mt-1" checked={!!opcoes[o.id]} onChange={(e) => setOpcoes({ ...opcoes, [o.id]: e.target.checked })} />
                  <span>
                    <span className="font-medium">{o.label}</span>
                    <span className="block text-[12px] text-ink-3">{o.hint}</span>
                  </span>
                </label>
              ))}
            </div>
            <div className="mt-5 grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-2">
              <Button size="lg" variant="secondary" loading={busy === "simular"} disabled={!analise || !!busy || analise.ficheiro?.completo === false} onClick={simular} title="Corre a importação e desfaz tudo no fim: mostra o que seria importado">
                Simular
              </Button>
              <Button size="lg" loading={busy === "importar"} disabled={!analise || !!busy || analise.ficheiro?.completo === false} onClick={() => setConfirmar(true)}>
                Importar dados
              </Button>
            </div>
            <p className="mt-2 text-[12px] text-ink-3">Pode repetir a importação: artigos, entidades e documentos já importados não são duplicados.</p>
          </Card>
          <Card className="p-5">
            <div className="mb-2 flex items-center gap-2 font-semibold">
              <ShieldAlert className="size-4 text-[#9a6514]" /> Certificado AGT
            </div>
            <p className="text-[13px] leading-relaxed text-ink-2">
              Os documentos do KIGEST entram como <b>histórico</b>, com as assinaturas originais, e não fazem parte do SAF-T do sistema novo. O n.º de certificado do KIGEST fica
              guardado como referência. O certificado e a chave de assinatura pertencem ao produtor e ao programa certificado, por isso não são activados no KanGest novo. As vendas
              novas continuam em séries próprias.
            </p>
            <Badge tone="neutral" className="mt-3">Configurações → Certificação AGT</Badge>
          </Card>
        </div>
      </div>

      <Modal
        open={confirmar}
        onClose={() => setConfirmar(false)}
        title="Confirmar importação"
        width="max-w-md"
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirmar(false)}>
              Cancelar
            </Button>
            <Button onClick={importar}>Importar</Button>
          </>
        }
      >
        <p className="text-ink-2">
          Os dados do KIGEST serão juntados aos deste sistema. Antes de importar é feita automaticamente uma cópia de segurança da base actual na pasta Transferências.
        </p>
      </Modal>
    </>
  );
}

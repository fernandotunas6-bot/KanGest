// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { Fragment, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, FileSpreadsheet, History, Printer, RefreshCw, Search, ShieldAlert, ShieldCheck, X } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora, hojeISO, kz } from "@/lib/format";
import { dataXL, exportarExcel } from "@/lib/csv";
import { imprimir } from "@/lib/print";
import { useDebounce } from "@/lib/hooks";
import { useSession } from "@/store/session";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import { InputData } from "@/components/InputData";
import { Badge, Button, Card, Empty, Input, Segmentado, Select, Table, Td, Th, cn } from "@/components/ui";

/** Uma linha do registo de auditoria (services/auditoria.rs). */
export interface LinhaAuditoria {
  id: number;
  data_hora: string;
  acao: string;
  detalhe: string | null;
  entidade: string | null;
  entidade_nome: string | null;
  registo_id: string | null;
  /** os campos como estavam (numa alteração, só os que mudaram) */
  antes: Record<string, unknown> | null;
  /** os campos como ficaram */
  depois: Record<string, unknown> | null;
  origem: string | null;
  utilizador_id: number | null;
  utilizador: string | null;
  posto: string | null;
}

interface Verificacao {
  integra: boolean;
  total: number;
  conferidas: number;
  quebras: number;
  primeiro: string | null;
  ultimo: string | null;
  problema: { id: number; data_hora?: string; motivo: string } | null;
  verificado_em: string;
}

type Tom = "ok" | "danger" | "accent" | "neutral" | "brand";

const ACOES: Record<string, { label: string; tom: Tom }> = {
  CRIAR: { label: "Criação", tom: "ok" },
  ALTERAR: { label: "Alteração", tom: "brand" },
  ELIMINAR: { label: "Eliminação", tom: "danger" },
  ANULAR: { label: "Anulação", tom: "danger" },
  REVOGAR: { label: "Revogação", tom: "danger" },
  AUDITORIA_QUEBRA: { label: "Registo mexido por fora", tom: "danger" },
  LOGIN: { label: "Entrada", tom: "neutral" },
  EMITIR: { label: "Emissão", tom: "neutral" },
  FECHO_CAIXA: { label: "Fecho de caixa", tom: "neutral" },
  IMPORTAR: { label: "Importação", tom: "neutral" },
  LICENCA: { label: "Licença", tom: "neutral" },
  COPIA_SEGURANCA: { label: "Cópia de segurança", tom: "neutral" },
  RESTAURO_PEDIDO: { label: "Restauro pedido", tom: "accent" },
  RECALCULO_STOCK: { label: "Recálculo do stock", tom: "neutral" },
  VERIFICAR_INTEGRIDADE: { label: "Integridade dos documentos", tom: "neutral" },
  API: { label: "API de integração", tom: "neutral" },
  API_CHAVE: { label: "Chave da API", tom: "neutral" },
  API_WEBHOOK: { label: "Webhook", tom: "neutral" },
  ACESSO_WEB: { label: "Acesso pela Internet", tom: "neutral" },
  DOIS_PASSOS: { label: "Dois passos", tom: "neutral" },
  FE_CONFIG: { label: "Facturação electrónica", tom: "neutral" },
  EMAIL: { label: "Envio por email", tom: "neutral" },
  PONTOS: { label: "Pontos de fidelização", tom: "neutral" },
  CUPAO: { label: "Cupão", tom: "neutral" },
  PROMOCAO: { label: "Promoção", tom: "neutral" },
  SERIE: { label: "Série", tom: "neutral" },
  UTILIZADOR: { label: "Utilizador", tom: "neutral" },
  PERFIL: { label: "Perfil", tom: "neutral" },
  SEGURANCA: { label: "Política de segurança", tom: "neutral" },
  INTERNET: { label: "Ligação à Internet", tom: "neutral" },
  CONFIG: { label: "Configuração", tom: "neutral" },
  COPIAS_CONFIG: { label: "Cópias automáticas", tom: "neutral" },
  AGT_CHAVE: { label: "Chave da AGT", tom: "accent" },
  COMISSAO: { label: "Comissões", tom: "neutral" },
  DEVOLUCAO: { label: "Devolução", tom: "neutral" },
  REGULARIZACAO: { label: "Regularização", tom: "neutral" },
  REGULARIZACAO_ANULAR: { label: "Regularização anulada", tom: "danger" },
  RH_PROCESSAR: { label: "Salários processados", tom: "neutral" },
  RH_ANULAR: { label: "Salários anulados", tom: "danger" },
};

export function rotuloAcao(a: string) {
  if (ACOES[a]) return ACOES[a].label;
  const t = a.replace(/_/g, " ").toLowerCase();
  return t.charAt(0).toUpperCase() + t.slice(1);
}

export function BadgeAcao({ acao }: { acao: string }) {
  return <Badge tone={ACOES[acao]?.tom ?? "neutral"}>{rotuloAcao(acao)}</Badge>;
}

const ORIGENS: Record<string, string> = {
  local: "Este computador",
  rede: "Rede da loja",
  internet: "Pela Internet",
  api: "API de integração",
};

/** Nomes dos campos para mostrar (os que não estão aqui ficam como na base, sem «_»). */
const CAMPOS: Record<string, string> = {
  descricao: "Descrição",
  descricao_curta: "Descrição curta",
  designacao_comercial: "Designação comercial",
  codigo: "Código",
  codigo_barras: "Código de barras",
  referencia: "Referência",
  familia_id: "Família",
  subfamilia_id: "Subfamília",
  unidade_id: "Unidade",
  taxa_iva_id: "Taxa de IVA",
  armazem_id: "Loja / armazém",
  ativo: "Activo",
  ativa: "Activa",
  nome: "Nome",
  nif: "NIF",
  telefone: "Telefone",
  telemovel: "Telemóvel",
  email: "Email",
  endereco: "Endereço",
  morada: "Morada",
  localidade: "Localidade",
  provincia: "Província",
  pais: "País",
  login: "Utilizador",
  perfil: "Perfil",
  senha_hash: "Senha",
  pin_hash: "PIN",
  foto: "Fotografia",
  permissoes: "Permissões",
  limite_desconto: "Limite de desconto",
  limite_credito: "Limite de crédito",
  cambio: "Câmbio",
  simbolo: "Símbolo",
  stock_minimo: "Stock mínimo",
  custo: "Custo",
  preco_custo: "Preço de custo",
  estado: "Estado",
  motivo: "Motivo",
  tipo: "Tipo",
  serie: "Série",
  numero_doc: "Documento",
  data_inicio: "Início",
  data_fim: "Fim",
  mudar_senha: "Mudar a senha ao entrar",
  controla_stock: "Controla stock",
  permite_negativo: "Permite stock negativo",
  controla_lote: "Controla lotes",
  composto: "Artigo composto",
  nivel_preco: "Nível de preço",
  prazo_pagamento: "Prazo de pagamento",
  observacoes: "Observações",
  salario_base: "Salário base",
  custo_medio: "Custo médio",
  aviso_validade_dias: "Aviso de validade (dias)",
  qtd_minima: "Stock mínimo",
  qtd_maxima: "Stock máximo",
};

const DINHEIRO = /^(preco\d?|pvp\d?|custo|custo_medio|preco_custo|limite_credito|salario_base)$/;
const SIM_NAO = /^(ativo|ativa|mudar_senha|controla_stock|permite_negativo|controla_lote|composto|padrao|manual|sistema)$/;

export function nomeCampo(c: string) {
  if (CAMPOS[c]) return CAMPOS[c];
  const preco = /^preco(\d)$/.exec(c);
  if (preco) return `Preço ${preco[1]}`;
  const pvp = /^pvp(\d)$/.exec(c);
  if (pvp) return `PVP ${pvp[1]} (com IVA)`;
  const t = c.replace(/_/g, " ");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

function valorCampo(c: string, v: unknown): string {
  if (v === null || v === undefined || v === "") return "—";
  if (typeof v === "number" && DINHEIRO.test(c)) return kz(v);
  if ((v === 0 || v === 1) && SIM_NAO.test(c)) return v ? "Sim" : "Não";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** «Preço 1: 1 000,00 Kz → 1 200,00 Kz; Descrição: …» (para o Excel e a impressão). */
export function resumoMudancas(l: LinhaAuditoria) {
  const campos = [...new Set([...Object.keys(l.depois ?? {}), ...Object.keys(l.antes ?? {})])];
  return campos
    .map((c) => {
      const a = l.antes ? valorCampo(c, l.antes[c]) : null;
      const d = l.depois ? valorCampo(c, l.depois[c]) : null;
      return `${nomeCampo(c)}: ${a !== null && d !== null ? `${a} → ${d}` : (d ?? a)}`;
    })
    .join("; ");
}

/** O antes e o depois, campo a campo. */
export function Diferencas({ l }: { l: LinhaAuditoria }) {
  const campos = [...new Set([...Object.keys(l.depois ?? {}), ...Object.keys(l.antes ?? {})])];
  if (!campos.length) return <p className="text-[13px] text-ink-3">Sem pormenores dos campos.</p>;
  const comAntes = !!l.antes;
  const comDepois = !!l.depois;
  return (
    <table className="w-full text-[12.5px]">
      <thead>
        <tr className="text-left text-[11.5px] uppercase tracking-wide text-ink-3">
          <th className="w-1/4 py-1 pr-3 font-medium">Campo</th>
          {comAntes && <th className="py-1 pr-3 font-medium">{comDepois ? "Antes" : "Como estava"}</th>}
          {comDepois && <th className="py-1 font-medium">{comAntes ? "Depois" : "Como ficou"}</th>}
        </tr>
      </thead>
      <tbody>
        {campos.map((c) => (
          <tr key={c} className="border-t border-line/70 align-top">
            <td className="py-1 pr-3 text-ink-2">{nomeCampo(c)}</td>
            {comAntes && <td className={cn("break-all py-1 pr-3", comDepois && "text-danger line-through decoration-danger/40")}>{valorCampo(c, l.antes?.[c])}</td>}
            {comDepois && <td className={cn("break-all py-1", comAntes && "font-medium text-ok")}>{valorCampo(c, l.depois?.[c])}</td>}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Estado do selo em cadeia: verde quando ninguém mexeu no registo por fora. */
function EstadoIntegridade() {
  const v = useQuery({
    queryKey: ["auditoria-verificar"],
    queryFn: () => call<Verificacao>("auditoria.verificar"),
    staleTime: 300_000,
  });
  const r = v.data;
  return (
    <div className={cn("flex flex-wrap items-center gap-3 border-b border-line px-4 py-3 text-[13px]", r && !r.integra ? "bg-danger-50 text-danger" : r ? "bg-ok-50/60" : "")}>
      {r && !r.integra ? <ShieldAlert className="size-5 shrink-0" /> : <ShieldCheck className={cn("size-5 shrink-0", r ? "text-ok" : "text-ink-3")} />}
      <div className="min-w-[15rem] flex-1">
        {v.isFetching && !r && <span className="text-ink-2">A conferir o selo de todas as linhas…</span>}
        {v.isError && <span className="text-danger">{(v.error as Error).message}</span>}
        {r?.integra && (
          <>
            <span className="font-medium text-ok">Registo íntegro.</span>{" "}
            <span className="text-ink-2">
              {r.total.toLocaleString("pt-PT")} linhas seladas em cadeia
              {r.primeiro && r.ultimo ? `, de ${dataHora(r.primeiro.replace(" ", "T"))} a ${dataHora(r.ultimo.replace(" ", "T"))}` : ""}. Ninguém as alterou nem apagou fora do
              KanGest.
            </span>
          </>
        )}
        {r && !r.integra && r.problema && (
          <>
            <span className="font-semibold">O registo de auditoria foi mexido fora do KanGest.</span> Linha n.º {r.problema.id}
            {r.problema.data_hora ? ` (${dataHora(r.problema.data_hora.replace(" ", "T"))})` : ""}: {r.problema.motivo}
          </>
        )}
      </div>
      <Button variant="secondary" onClick={() => v.refetch()} loading={v.isFetching}>
        <RefreshCw className="size-4" /> Verificar integridade
      </Button>
    </div>
  );
}

const seteDiasAtras = () => new Date(Date.now() - 6 * 86400000 - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);

/** Utilizadores → Auditoria: filtros, o antes e o depois de cada linha, integridade, Excel e impressão. */
export function PainelAuditoria() {
  const [sp, setSp] = useSearchParams();
  const tem = useSession((s) => s.tem);
  const [de, setDe] = useState(() => (sp.get("registo") ? "2000-01-01" : seteDiasAtras()));
  const [ate, setAte] = useState(hojeISO());
  const [quem, setQuem] = useState<number | "">("");
  const [acao, setAcao] = useState("");
  const [ficha, setFicha] = useState(sp.get("entidade") ?? "");
  const [registo, setRegisto] = useState(sp.get("registo") ?? "");
  const [origem, setOrigem] = useState("");
  const [texto, setTexto] = useState("");
  const dt = useDebounce(texto, 300);
  const [abertas, setAbertas] = useState<Set<number>>(new Set());
  const empresa = useEmpresa();

  const opcoes = useQuery({
    queryKey: ["auditoria-opcoes"],
    queryFn: () =>
      call<{
        acoes: string[];
        fichas: { entidade: string; nome: string }[];
        limite: number;
      }>("auditoria.opcoes"),
  });
  const utilizadores = useQuery({
    queryKey: ["utilizadores"],
    queryFn: () => call<{ id: number; nome: string }[]>("utilizadores.listar"),
    enabled: tem("sistema.utilizadores"),
  });
  const q = useQuery({
    queryKey: ["auditoria", de, ate, quem, acao, ficha, registo, origem, dt],
    queryFn: () =>
      call<LinhaAuditoria[]>("auditoria.listar", {
        de,
        ate,
        utilizador_id: quem || null,
        evento: acao || null,
        entidade: ficha || null,
        registo_id: registo || null,
        origem: origem || null,
        texto: dt || null,
      }),
    placeholderData: (p) => p,
  });
  const linhas = q.data ?? [];
  const limite = opcoes.data?.limite ?? 5000;
  const nomeFicha = (e: string | null) => opcoes.data?.fichas.find((f) => f.entidade === e)?.nome ?? e ?? "";

  function alternar(id: number) {
    setAbertas((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }
  function verFicha(l: LinhaAuditoria) {
    if (!l.entidade || !l.registo_id) return;
    setFicha(l.entidade);
    setRegisto(l.registo_id);
    setDe("2000-01-01");
  }
  function limparFicha() {
    setRegisto("");
    setDe(seteDiasAtras());
    if (sp.get("registo")) setSp({ vista: "auditoria" }, { replace: true });
  }
  function exportar() {
    exportarExcel(
      `auditoria-${de}-a-${ate}.xlsx`,
      [
        ["Data e hora", "Utilizador", "Acção", "Ficha", "N.º", "Detalhe", "Alterações", "Posto", "Via"],
        ...linhas.map((l) => [
          dataXL(l.data_hora.replace(" ", "T")),
          l.utilizador ?? "",
          rotuloAcao(l.acao),
          l.entidade_nome ?? "",
          l.registo_id ?? "",
          l.detalhe ?? "",
          resumoMudancas(l),
          l.posto ?? "",
          ORIGENS[l.origem ?? ""] ?? "",
        ]),
      ],
      { titulo: "Registo de auditoria" },
    );
  }

  return (
    <Card className={cn("transition-opacity", q.isFetching && "opacity-80")}>
      <EstadoIntegridade />
      <div className="flex flex-wrap items-center gap-3 border-b border-line p-4">
        <InputData className="w-36 sm:w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        <span className="text-ink-3">até</span>
        <InputData className="w-36 sm:w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
        {tem("sistema.utilizadores") && (
          <Select className="w-full sm:w-48" value={quem} onChange={(e) => setQuem(e.target.value ? Number(e.target.value) : "")} aria-label="Utilizador">
            <option value="">Todos os utilizadores</option>
            {utilizadores.data?.map((u) => (
              <option key={u.id} value={u.id}>
                {u.nome}
              </option>
            ))}
          </Select>
        )}
        <Select className="w-full sm:w-44" value={acao} onChange={(e) => setAcao(e.target.value)} aria-label="Acção">
          <option value="">Todas as acções</option>
          {opcoes.data?.acoes.map((a) => (
            <option key={a} value={a}>
              {rotuloAcao(a)}
            </option>
          ))}
        </Select>
        <Select
          className="w-full sm:w-48"
          value={ficha}
          onChange={(e) => {
            setFicha(e.target.value);
            setRegisto("");
          }}
          aria-label="Ficha"
        >
          <option value="">Todas as fichas</option>
          {opcoes.data?.fichas.map((f) => (
            <option key={f.entidade} value={f.entidade}>
              {f.nome}
            </option>
          ))}
        </Select>
        <Select className="w-full sm:w-44" value={origem} onChange={(e) => setOrigem(e.target.value)} aria-label="Via">
          <option value="">Todas as vias</option>
          {Object.entries(ORIGENS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </Select>
        <div className="relative min-w-52 flex-1">
          <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
          <Input className="pl-9" placeholder="Pesquisar no detalhe e nos valores…" value={texto} onChange={(e) => setTexto(e.target.value)} />
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-4 py-2.5 text-[13px] text-ink-2">
        {registo && (
          <span className="inline-flex items-center gap-1.5 rounded-md bg-brand-50 px-2 py-1 text-brand-700">
            <History className="size-3.5" /> Histórico de {nomeFicha(ficha)} n.º {registo}
            <button onClick={limparFicha} aria-label="Ver todas as fichas" className="rounded p-0.5 hover:bg-brand-100">
              <X className="size-3.5" />
            </button>
          </span>
        )}
        <span>
          {linhas.length.toLocaleString("pt-PT")} registos
          {linhas.length >= limite && <span className="text-accent-ink"> · mostram-se os {limite.toLocaleString("pt-PT")} mais recentes: afine os filtros</span>}
        </span>
        <span className="ml-auto flex gap-2">
          <Button variant="secondary" onClick={exportar} disabled={!linhas.length}>
            <FileSpreadsheet className="size-4" /> Excel
          </Button>
          <Button variant="secondary" onClick={imprimir} disabled={!linhas.length}>
            <Printer className="size-4" /> Imprimir
          </Button>
        </span>
      </div>
      {q.data?.length === 0 ? (
        <Empty icon={<ShieldCheck className="size-6" />} title="Sem registos com estes filtros" />
      ) : (
        <Table className="tabela-compacta max-h-[calc(100vh-420px)]">
          <thead>
            <tr>
              <Th className="w-6" />
              <Th>Data e hora</Th>
              <Th>Utilizador</Th>
              <Th>Acção</Th>
              <Th>Ficha</Th>
              <Th>Detalhe</Th>
              <Th>Posto · via</Th>
            </tr>
          </thead>
          <tbody>
            {linhas.map((l) => {
              const tem_mudancas = !!(l.antes || l.depois);
              const aberta = abertas.has(l.id);
              return (
                <Fragment key={l.id}>
                  <tr className={cn(tem_mudancas && "cursor-pointer hover:bg-sunken/60", aberta && "bg-sunken/50")} onClick={() => tem_mudancas && alternar(l.id)}>
                    <Td className="text-ink-3">{tem_mudancas && (aberta ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />)}</Td>
                    <Td className="whitespace-nowrap text-ink-2">{dataHora(l.data_hora.replace(" ", "T"))}</Td>
                    <Td className="font-medium">{l.utilizador || <span className="text-ink-3">sistema</span>}</Td>
                    <Td>
                      <BadgeAcao acao={l.acao} />
                    </Td>
                    <Td className="whitespace-nowrap">
                      {l.entidade_nome ? (
                        l.registo_id ? (
                          <button
                            className="text-brand-700 hover:underline"
                            title="Ver o histórico desta ficha"
                            onClick={(e) => {
                              e.stopPropagation();
                              verFicha(l);
                            }}
                          >
                            {l.entidade_nome} n.º {l.registo_id}
                          </button>
                        ) : (
                          l.entidade_nome
                        )
                      ) : (
                        <span className="text-ink-3">—</span>
                      )}
                    </Td>
                    <Td className="text-ink-2">{l.detalhe ?? "—"}</Td>
                    <Td className="whitespace-nowrap text-ink-2">{[l.posto, ORIGENS[l.origem ?? ""]].filter(Boolean).join(" · ") || "—"}</Td>
                  </tr>
                  {aberta && (
                    <tr className="bg-sunken/30">
                      <td />
                      <td colSpan={6} className="px-3 pb-3 pt-1">
                        <Diferencas l={l} />
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
        </Table>
      )}
      <PrintPortal>
        <div className="p-2 text-[10.5px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo="Registo de auditoria">
            <div>
              {de === "2000-01-01" ? "Desde o início" : `De ${de}`} até {ate}
              {registo ? ` · ${nomeFicha(ficha)} n.º ${registo}` : ""}
            </div>
          </CabecalhoEmpresa>
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th className="py-1 pr-2">Data e hora</th>
                <th className="py-1 pr-2">Utilizador</th>
                <th className="py-1 pr-2">Acção</th>
                <th className="py-1 pr-2">Detalhe</th>
                <th className="py-1">Alterações</th>
              </tr>
            </thead>
            <tbody>
              {linhas.map((l) => (
                <tr key={l.id} className="border-b border-dotted border-gray-300 align-top">
                  <td className="whitespace-nowrap py-0.5 pr-2">{dataHora(l.data_hora.replace(" ", "T"))}</td>
                  <td className="py-0.5 pr-2">{l.utilizador ?? "sistema"}</td>
                  <td className="py-0.5 pr-2">{rotuloAcao(l.acao)}</td>
                  <td className="py-0.5 pr-2">{l.detalhe}</td>
                  <td className="py-0.5">{resumoMudancas(l)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="mt-2 font-bold">{linhas.length} registos</div>
        </div>
      </PrintPortal>
    </Card>
  );
}

/** O histórico de uma ficha: quem a criou e mudou, quando e o quê. Só para quem pode ver a auditoria. */
export function HistoricoFicha({ entidade, id }: { entidade: string; id: number | string }) {
  const q = useQuery({
    queryKey: ["auditoria-historico", entidade, String(id)],
    queryFn: () =>
      call<LinhaAuditoria[]>("auditoria.historico", {
        entidade,
        registo_id: id,
      }),
  });
  if (q.isLoading) return <p className="py-6 text-center text-[13px] text-ink-3">A carregar o histórico…</p>;
  if (q.isError) return <p className="py-6 text-center text-[13px] text-danger">{(q.error as Error).message}</p>;
  const linhas = q.data ?? [];
  if (!linhas.length)
    return (
      <Empty
        icon={<History className="size-6" />}
        title="Ainda sem alterações registadas"
        text="As criações e alterações desta ficha passam a ficar aqui, com o antes e o depois."
      />
    );
  return (
    <ol className="relative max-h-[60vh] space-y-4 overflow-y-auto border-l border-line pl-5">
      {linhas.map((l) => (
        <li key={l.id} className="relative">
          <span className="absolute -left-[26px] top-1 size-2.5 rounded-full bg-brand ring-4 ring-surface" />
          <div className="mb-1.5 flex flex-wrap items-center gap-2 text-[13px]">
            <BadgeAcao acao={l.acao} />
            <span className="font-medium">{l.utilizador ?? "sistema"}</span>
            <span className="text-ink-3">
              {dataHora(l.data_hora.replace(" ", "T"))}
              {l.posto ? ` · ${l.posto}` : ""}
              {l.origem && l.origem !== "local" ? ` · ${ORIGENS[l.origem] ?? l.origem}` : ""}
            </span>
          </div>
          <div className="rounded-lg border border-line bg-bg/50 px-3 py-2">
            <Diferencas l={l} />
          </div>
        </li>
      ))}
    </ol>
  );
}

/**
 * Separadores «Ficha» e «Histórico» dentro do modal de uma ficha que já existe (para quem pode ver a auditoria).
 * Numa ficha nova, ou sem a permissão, mostra só a ficha.
 */
export function AbasFicha({ entidade, id, children }: { entidade: string; id?: number | string | null; children: ReactNode }) {
  const tem = useSession((s) => s.tem);
  const [aba, setAba] = useState<"ficha" | "historico">("ficha");
  if (!id || !tem("sistema.auditoria")) return <>{children}</>;
  return (
    <>
      <Segmentado
        className="mb-4 w-full sm:w-72"
        valor={aba}
        onChange={setAba}
        opcoes={[
          { id: "ficha", label: "Ficha" },
          { id: "historico", label: "Histórico", icon: History },
        ]}
      />
      {aba === "historico" ? <HistoricoFicha entidade={entidade} id={id} /> : children}
    </>
  );
}

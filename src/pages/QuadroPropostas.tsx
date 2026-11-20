// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { FilePlus2, List } from "lucide-react";
import { call } from "@/lib/api";
import { data, hojeISO, kz } from "@/lib/format";
import { useSession } from "@/store/session";
import { Badge, Button, PageHeader, Segmentado, Select, cn } from "@/components/ui";
import DocumentoModal from "@/components/DocumentoModal";

interface Proposta {
  id: number;
  numero_doc: string;
  tipo_codigo: string;
  tipo_desc: string;
  data_emissao: string;
  data_vencimento: string | null;
  entidade_nome: string;
  total: number;
  estado: "N" | "F" | "A";
  importado: number;
  convertido_id: number | null;
  convertido_em: string | null;
}

type Coluna = "aberto" | "expirado" | "convertido" | "anulado";
type Grupo = "VENDAS" | "COMPRAS";

const dias = (de: string, ate: string) => Math.round((Date.parse(ate.slice(0, 10)) - Date.parse(de.slice(0, 10))) / 86_400_000);

/** Em que coluna está cada documento: anulado, convertido, expirado (validade passada) ou em aberto. */
function colunaDe(d: Proposta, hoje: string): Coluna {
  if (d.estado === "A") return "anulado";
  if (d.estado === "F") return "convertido";
  if (d.data_vencimento && d.data_vencimento.slice(0, 10) < hoje) return "expirado";
  return "aberto";
}

const COLUNAS: Record<Grupo, { id: Coluna; label: string; tom: "accent" | "neutral" | "ok" | "danger" | "brand" }[]> = {
  VENDAS: [
    { id: "aberto", label: "Em aberto", tom: "brand" },
    { id: "expirado", label: "Expiradas", tom: "accent" },
    { id: "convertido", label: "Facturadas", tom: "ok" },
    { id: "anulado", label: "Anuladas", tom: "danger" },
  ],
  COMPRAS: [
    { id: "aberto", label: "Em aberto", tom: "brand" },
    { id: "expirado", label: "Fora do prazo", tom: "accent" },
    { id: "convertido", label: "Convertidas em V/Factura", tom: "ok" },
    { id: "anulado", label: "Anuladas", tom: "danger" },
  ],
};

/**
 * Quadro de propostas e encomendas (como o kanban de cotações do Odoo): proformas, orçamentos e notas de encomenda
 * de clientes, ou encomendas e orçamentos a fornecedores, por estado e com o valor de cada coluna. Arrastar um
 * cartão para «Facturadas» abre a factura a partir dele; para «Anuladas», pede o motivo da anulação.
 */
export default function QuadroPropostas() {
  const [sp, setSp] = useSearchParams();
  const grupo: Grupo = sp.get("grupo") === "COMPRAS" ? "COMPRAS" : "VENDAS";
  const [periodo, setPeriodo] = useState("90");
  const hoje = hojeISO();
  const desde = periodo ? new Date(Date.now() - Number(periodo) * 86_400_000).toISOString().slice(0, 10) : "";
  const tem = useSession((s) => s.tem);
  const nav = useNavigate();
  const [aberto, setAberto] = useState<{ id: number; anular?: boolean } | null>(null);
  const [arrastado, setArrastado] = useState<Proposta | null>(null);
  const [alvo, setAlvo] = useState<Coluna | null>(null);
  const lista = useQuery({
    queryKey: ["quadro-propostas", grupo, desde],
    queryFn: () => call<Proposta[]>("documentos.quadro", { grupo, desde }),
  });

  const podeConverter = grupo === "VENDAS" ? tem("vendas.fatura") : tem("compras.fatura");
  const podeLargar = (c: Coluna) => {
    if (!arrastado || !["aberto", "expirado"].includes(colunaDe(arrastado, hoje))) return false;
    if (c === "convertido") return podeConverter && !arrastado.importado;
    if (c === "anulado") return tem("documentos.anular") && !arrastado.importado;
    return false;
  };

  function largar(d: Proposta, c: Coluna) {
    if (c === "convertido") {
      if (colunaDe(d, hoje) === "expirado") toast.warning(`${d.numero_doc} expirou a ${data(d.data_vencimento)}: confirme os preços antes de gravar.`, { duration: 8000 });
      nav(grupo === "VENDAS" ? `/documentos/novo/FT?origem=${d.id}` : `/documentos/novo/FC?origem=${d.id}`);
    } else if (c === "anulado") {
      setAberto({ id: d.id, anular: true });
    }
  }

  const docs = lista.data ?? [];
  const novo = grupo === "VENDAS" ? "/documentos/novo/PP" : "/documentos/novo/VNE";
  return (
    <>
      <PageHeader
        title={grupo === "VENDAS" ? "Propostas e encomendas de clientes" : "Encomendas a fornecedores"}
        subtitle={grupo === "VENDAS" ? "Proformas, orçamentos e notas de encomenda por estado" : "Encomendas e orçamentos de fornecedores por estado"}
        actions={
          <>
            <Button variant="secondary" onClick={() => nav(`/documentos?grupo=${grupo}`)}>
              <List className="size-4" /> Lista
            </Button>
            <Button onClick={() => nav(novo)}>
              <FilePlus2 className="size-4" /> {grupo === "VENDAS" ? "Nova proforma" : "Nova encomenda"}
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <Segmentado
          className="w-64"
          valor={grupo}
          opcoes={[
            { id: "VENDAS", label: "Clientes" },
            { id: "COMPRAS", label: "Fornecedores" },
          ]}
          onChange={(g) => setSp({ grupo: g })}
        />
        <span className="ml-auto text-[13px] text-ink-3">Fechados nos últimos</span>
        <Select className="w-36" value={periodo} onChange={(e) => setPeriodo(e.target.value)}>
          <option value="30">30 dias</option>
          <option value="90">90 dias</option>
          <option value="365">12 meses</option>
          <option value="">Sempre</option>
        </Select>
      </div>

      {!lista.data ? (
        <div className="h-[460px] animate-pulse rounded-xl bg-sunken/60" />
      ) : (
        <div className="grid grid-cols-4 gap-3">
          {COLUNAS[grupo].map((col) => {
            const cartoes = docs.filter((d) => colunaDe(d, hoje) === col.id);
            const total = cartoes.reduce((s, d) => s + d.total, 0);
            const largavel = podeLargar(col.id);
            return (
              <section
                key={col.id}
                aria-label={col.label}
                onDragOver={(e) => {
                  if (!largavel) return;
                  e.preventDefault();
                  setAlvo(col.id);
                }}
                onDragLeave={() => setAlvo((a) => (a === col.id ? null : a))}
                onDrop={(e) => {
                  e.preventDefault();
                  setAlvo(null);
                  if (arrastado && largavel) largar(arrastado, col.id);
                }}
                className={cn(
                  "flex min-h-[460px] flex-col rounded-xl border bg-sunken/50 p-2 transition",
                  alvo === col.id ? "border-brand bg-brand-50/60" : "border-transparent",
                  arrastado && !largavel && col.id !== colunaDe(arrastado, hoje) && "opacity-60",
                )}
              >
                <header className="mb-2 px-1.5 pt-1">
                  <div className="flex items-center gap-2">
                    <Badge tone={col.tom}>{col.label}</Badge>
                    <span className="num text-[12.5px] text-ink-3">{cartoes.length}</span>
                  </div>
                  <div className="num mt-1 text-[13px] font-semibold">{kz(total)}</div>
                </header>
                <div className="flex flex-1 flex-col gap-2">
                  {cartoes.map((d) => {
                    const fixo = !["aberto", "expirado"].includes(col.id) || !!d.importado;
                    const validade = d.data_vencimento ? dias(hoje, d.data_vencimento) : null;
                    return (
                      <article
                        key={d.id}
                        draggable={!fixo}
                        onDragStart={(e) => {
                          e.dataTransfer.setData("text/plain", String(d.id));
                          e.dataTransfer.effectAllowed = "move";
                          setArrastado(d);
                        }}
                        onDragEnd={() => {
                          setArrastado(null);
                          setAlvo(null);
                        }}
                        onClick={() => setAberto({ id: d.id })}
                        title={fixo ? "Clique para abrir" : "Arraste para facturar ou anular; clique para abrir"}
                        className={cn(
                          "cursor-pointer rounded-lg border border-line bg-surface p-3 shadow-card transition hover:border-line-strong",
                          arrastado?.id === d.id && "opacity-50",
                          col.id === "anulado" && "opacity-75",
                        )}
                      >
                        <div className="flex items-baseline justify-between gap-2">
                          <span className={cn("text-[13px] font-semibold", col.id === "anulado" && "line-through")}>{d.numero_doc}</span>
                          <span className="num text-[13px] font-medium">{kz(d.total)}</span>
                        </div>
                        <div className="truncate text-[13px] text-ink-2">{d.entidade_nome}</div>
                        <div className="mt-1.5 flex flex-wrap items-center gap-1.5 text-[11.5px] text-ink-3">
                          <span>{d.tipo_desc}</span>
                          <span>· {data(d.data_emissao)}</span>
                        </div>
                        {(col.id === "aberto" || col.id === "expirado") && validade !== null && (
                          <div className="mt-2">
                            <Badge tone={validade < 0 ? "danger" : validade <= 7 ? "accent" : "neutral"}>
                              {validade < 0 ? `Expirou há ${-validade} ${validade === -1 ? "dia" : "dias"}` : validade === 0 ? "Expira hoje" : `Válida mais ${validade} ${validade === 1 ? "dia" : "dias"}`}
                            </Badge>
                          </div>
                        )}
                        {d.convertido_em && (
                          <div className="mt-2">
                            <Badge tone="ok">→ {d.convertido_em}</Badge>
                          </div>
                        )}
                      </article>
                    );
                  })}
                  {!cartoes.length && <p className="px-2 py-6 text-center text-[12.5px] text-ink-3">{largavel ? "Largue aqui" : "Sem documentos"}</p>}
                </div>
              </section>
            );
          })}
        </div>
      )}
      <p className="mt-3 text-[12.5px] text-ink-3">
        Arraste um cartão em aberto para «{COLUNAS[grupo][2].label}» para emitir {grupo === "VENDAS" ? "a factura" : "a V/Factura"} a partir dele, ou para «Anuladas» para o anular com motivo.
      </p>
      <DocumentoModal id={aberto?.id ?? null} anularAoAbrir={aberto?.anular} onClose={() => setAberto(null)} />
    </>
  );
}

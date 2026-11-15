import { ChevronRight, CornerLeftUp, FileText, Link2, Receipt } from "lucide-react";
import { data, hojeISO, kz } from "@/lib/format";
import type { DocCompleto } from "@/lib/types";
import { Badge, cn } from "./ui";

/** Documentos que ficam em dívida até serem pagos (conta corrente). */
const A_CREDITO = ["FT", "ND", "FC", "VND"];

interface Percurso {
  fases: string[];
  atual: number;
  aviso?: { tom: "danger" | "neutral"; texto: string };
}

/** Em que ponto do seu percurso está o documento (como a barra de estado dos documentos no Odoo). */
function percurso(d: DocCompleto["documento"]): Percurso | null {
  const hoje = hojeISO();
  const venc = d.data_vencimento?.slice(0, 10);
  if (d.estado === "A") return { fases: ["Emitido", "Anulado"], atual: 1 };
  if (d.classe === "PROFORMA" || d.classe === "TRANSPORTE") {
    const expirada = d.estado === "N" && !!venc && venc < hoje && d.classe === "PROFORMA";
    return {
      fases: ["Emitida", "Facturada"],
      atual: d.estado === "F" ? 1 : 0,
      aviso: expirada ? { tom: "neutral", texto: `Expirou em ${data(venc)}` } : undefined,
    };
  }
  if (d.classe === "COMPRA_DOC") return { fases: ["Registado", "Convertido em V/Factura"], atual: d.estado === "F" ? 1 : 0 };
  if (A_CREDITO.includes(d.tipo_codigo) && d.total > 0) {
    const atual = d.valor_pendente <= 0 ? 2 : d.valor_pendente < d.total ? 1 : 0;
    let aviso: Percurso["aviso"];
    if (d.valor_pendente > 0 && venc && venc < hoje) {
      const dias = Math.round((Date.parse(hoje) - Date.parse(venc)) / 86_400_000);
      aviso = { tom: "danger", texto: `Vencida há ${dias} ${dias === 1 ? "dia" : "dias"} (${data(venc)})` };
    } else if (d.valor_pendente > 0 && venc) {
      aviso = { tom: "neutral", texto: `Vence a ${data(venc)}` };
    }
    return { fases: ["Por pagar", "Paga em parte", "Paga"], atual, aviso };
  }
  return null;
}

const ESTADO_AGT: Record<string, { tom: "ok" | "accent" | "neutral" | "danger"; texto: string }> = {
  VALIDO: { tom: "ok", texto: "AGT: válida" },
  ENVIADO: { tom: "neutral", texto: "AGT: enviada, a aguardar validação" },
  PENDENTE: { tom: "accent", texto: "AGT: por comunicar" },
  INVALIDO: { tom: "danger", texto: "AGT: rejeitada" },
};

/**
 * Barra de estado e documentos ligados de um documento: em que fase está (por pagar → paga, emitida →
 * facturada…), o estado na AGT e os documentos de origem, recibos e documentos emitidos a partir dele,
 * cada um abre-se com um clique.
 */
export function EstadoDocumento({ d, onAbrir }: { d: DocCompleto; onAbrir: (id: number) => void }) {
  const doc = d.documento;
  const p = percurso(doc);
  const agt = d.fe ? ESTADO_AGT[d.fe.estado] : null;
  // Um documento pode aparecer em mais de uma lista (a nota de crédito tem a factura como origem e também
  // abate nela): mostra-se uma vez, com a primeira descrição.
  const ligados: { id: number; icone: typeof FileText; rotulo: string; detalhe?: string; anulado?: boolean }[] = [];
  const juntar = (l: (typeof ligados)[number]) => !ligados.some((x) => x.id === l.id) && ligados.push(l);
  if (doc.doc_origem_id && doc.doc_origem) juntar({ id: doc.doc_origem_id, icone: CornerLeftUp, rotulo: doc.doc_origem, detalhe: "origem" });
  for (const l of d.liquidacoes ?? []) juntar({ id: l.id, icone: FileText, rotulo: l.numero_doc, detalhe: `liquida ${kz(l.valor)}` });
  for (const r of d.recibos ?? [])
    juntar({ id: r.id, icone: r.tipo_codigo === "NC" ? FileText : Receipt, rotulo: r.numero_doc, detalhe: `${r.tipo_desc ?? "Recibo"} · ${kz(r.valor)}`, anulado: r.estado === "A" });
  for (const x of d.derivados ?? []) juntar({ id: x.id, icone: FileText, rotulo: x.numero_doc, detalhe: x.tipo_desc, anulado: x.estado === "A" });

  if (!p && !agt && !ligados.length) return null;
  return (
    <div className="mb-4 space-y-2.5 rounded-xl border border-line bg-bg/60 px-4 py-3">
      {(p || agt) && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          {p && (
            <ol className="flex items-center gap-1" aria-label="Estado do documento">
              {p.fases.map((f, i) => {
                const feito = i < p.atual;
                const atual = i === p.atual;
                const anulado = doc.estado === "A" && atual;
                return (
                  <li key={f} className="flex items-center gap-1">
                    {i > 0 && <ChevronRight className="size-3.5 text-ink-3/70" />}
                    <span
                      aria-current={atual ? "step" : undefined}
                      className={cn(
                        "rounded-full px-2.5 py-1 text-[12.5px]",
                        anulado
                          ? "bg-danger font-medium text-white"
                          : atual
                            ? "bg-brand font-medium text-white"
                            : feito
                              ? "bg-brand-50 text-brand-700"
                              : "text-ink-3",
                      )}
                    >
                      {f}
                    </span>
                  </li>
                );
              })}
            </ol>
          )}
          {p?.aviso && <Badge tone={p.aviso.tom === "danger" ? "danger" : "neutral"}>{p.aviso.texto}</Badge>}
          {A_CREDITO.includes(doc.tipo_codigo) && doc.estado !== "A" && doc.valor_pendente > 0 && doc.valor_pendente < doc.total && (
            <span className="num text-[12.5px] text-ink-2">Em dívida: {kz(doc.valor_pendente)}</span>
          )}
          {agt && (
            <Badge tone={agt.tom} className="ml-auto">
              {agt.texto}
            </Badge>
          )}
        </div>
      )}
      {ligados.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="inline-flex items-center gap-1.5 text-[12px] font-medium uppercase tracking-wide text-ink-3">
            <Link2 className="size-3.5" /> Ligados
          </span>
          {ligados.map((l) => (
            <button
              key={l.id}
              onClick={() => onAbrir(l.id)}
              title="Abrir este documento"
              className="inline-flex items-center gap-1.5 rounded-lg border border-line bg-surface px-2.5 py-1 text-[12.5px] transition hover:border-brand/50 hover:bg-brand-50"
            >
              <l.icone className="size-3.5 text-ink-3" />
              <span className={cn("font-medium", l.anulado && "line-through")}>{l.rotulo}</span>
              {l.detalhe && <span className="num text-ink-3">· {l.detalhe}</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

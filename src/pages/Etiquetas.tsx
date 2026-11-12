import { useState } from "react";
import { toast } from "sonner";
import { Minus, Plus, Printer, Tags, Trash2 } from "lucide-react";
import { comIva, kz } from "@/lib/format";
import { svgBarras } from "@/lib/barcode";
import { imprimir } from "@/lib/print";
import type { Artigo } from "@/lib/types";
import { ArtigoPicker } from "@/components/Pickers";
import { PrintPortal } from "@/components/DocumentoPrint";
import { useEmpresa } from "@/components/PrintCabecalho";
import { Button, Card, Empty, Field, PageHeader, Select, Table, Td, Th } from "@/components/ui";

type Linha = { artigo: Artigo; copias: number };

/** Formatos: folha A4 de etiquetas autocolantes (3 × 8) ou rolo de impressora de etiquetas. */
const FORMATOS = {
  a4: { label: "Folha A4 · 3 × 8 (70 × 37 mm)", largura: 70, altura: 37, colunas: 3, pagina: "A4", margem: "6mm 0" },
  rolo50: { label: "Rolo 50 × 30 mm", largura: 50, altura: 30, colunas: 1, pagina: "50mm 30mm", margem: "0" },
  rolo40: { label: "Rolo 40 × 25 mm", largura: 40, altura: 25, colunas: 1, pagina: "40mm 25mm", margem: "0" },
} as const;
type Formato = keyof typeof FORMATOS;

const precoNivel = (a: Artigo, nivel: number) => comIva([a.preco1, a.preco2, a.preco3, a.preco4][nivel - 1] || a.preco1, a.taxa_pct ?? 0);
const codigoDe = (a: Artigo) => (a.codigo_barras?.trim() || a.referencia).trim();

/** Etiquetas de preço e códigos de barras (Code 128) dos artigos. */
export default function Etiquetas() {
  const empresa = useEmpresa();
  const [linhas, setLinhas] = useState<Linha[]>([]);
  const [formato, setFormato] = useState<Formato>("a4");
  const [nivel, setNivel] = useState(1);
  const [mostrar, setMostrar] = useState({ preco: true, empresa: true, referencia: true });
  const total = linhas.reduce((s, l) => s + l.copias, 0);
  const f = FORMATOS[formato];

  function juntar(a: Artigo) {
    setLinhas((ls) => (ls.some((l) => l.artigo.id === a.id) ? ls.map((l) => (l.artigo.id === a.id ? { ...l, copias: l.copias + 1 } : l)) : [...ls, { artigo: a, copias: 1 }]));
  }
  const copias = (id: number, n: number) => setLinhas((ls) => ls.map((l) => (l.artigo.id === id ? { ...l, copias: Math.max(1, Math.min(500, n)) } : l)));

  function imprimirEtiquetas() {
    if (!total) return toast.error("Escolha pelo menos um artigo.");
    imprimir();
  }

  const etiquetas = linhas.flatMap((l) => Array.from({ length: l.copias }, () => l.artigo));

  return (
    <>
      <PageHeader
        title="Etiquetas e Códigos de Barras"
        subtitle="Etiquetas de preço com código de barras Code 128 (código de barras do artigo ou, se não tiver, a referência)"
        actions={
          <Button disabled={!total} onClick={imprimirEtiquetas}>
            <Printer className="size-4" /> Imprimir {total ? `${total} etiqueta${total > 1 ? "s" : ""}` : ""}
          </Button>
        }
      />
      <div className="grid grid-cols-[minmax(0,1fr)_320px] gap-4">
        <Card>
          <div className="border-b border-line p-4">
            <ArtigoPicker onPick={juntar} placeholder="Adicionar artigo (pesquisa ou código de barras)…" />
          </div>
          {!linhas.length ? (
            <Empty icon={<Tags className="size-6" />} title="Sem artigos" text="Adicione os artigos e o número de etiquetas de cada um." />
          ) : (
            <Table>
              <thead>
                <tr>
                  <Th>Artigo</Th>
                  <Th>Código</Th>
                  <Th className="text-right">Preço</Th>
                  <Th className="text-center">Etiquetas</Th>
                  <Th />
                </tr>
              </thead>
              <tbody>
                {linhas.map((l) => (
                  <tr key={l.artigo.id}>
                    <Td>
                      <div className="font-medium">{l.artigo.descricao}</div>
                      <div className="font-mono text-[12px] text-ink-3">{l.artigo.referencia}</div>
                    </Td>
                    <Td className="font-mono text-[12.5px]">{codigoDe(l.artigo)}</Td>
                    <Td className="num text-right">{kz(precoNivel(l.artigo, nivel))}</Td>
                    <Td>
                      <div className="flex items-center justify-center gap-1">
                        <button className="rounded-md p-1 text-ink-3 hover:bg-sunken" onClick={() => copias(l.artigo.id, l.copias - 1)} aria-label="Menos">
                          <Minus className="size-4" />
                        </button>
                        <input
                          className="num w-14 rounded-md border border-line bg-surface px-2 py-1 text-center"
                          type="number"
                          min={1}
                          value={l.copias}
                          onChange={(e) => copias(l.artigo.id, Number(e.target.value) || 1)}
                        />
                        <button className="rounded-md p-1 text-ink-3 hover:bg-sunken" onClick={() => copias(l.artigo.id, l.copias + 1)} aria-label="Mais">
                          <Plus className="size-4" />
                        </button>
                      </div>
                    </Td>
                    <Td className="text-right">
                      <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setLinhas((ls) => ls.filter((x) => x.artigo.id !== l.artigo.id))} aria-label="Remover">
                        <Trash2 className="size-4" />
                      </button>
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>

        <div className="space-y-4">
          <Card className="space-y-4 p-5">
            <Field label="Formato">
              <Select value={formato} onChange={(e) => setFormato(e.target.value as Formato)}>
                {Object.entries(FORMATOS).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Preço">
              <Select value={nivel} onChange={(e) => setNivel(Number(e.target.value))}>
                {[1, 2, 3, 4].map((n) => (
                  <option key={n} value={n}>
                    Preço {n} (com IVA)
                  </option>
                ))}
              </Select>
            </Field>
            {(
              [
                ["preco", "Mostrar preço"],
                ["referencia", "Mostrar referência"],
                ["empresa", "Mostrar nome da empresa"],
              ] as const
            ).map(([k, label]) => (
              <label key={k} className="flex items-center gap-2 text-ink-2">
                <input type="checkbox" checked={mostrar[k]} onChange={(e) => setMostrar((m) => ({ ...m, [k]: e.target.checked }))} /> {label}
              </label>
            ))}
          </Card>
          {linhas[0] && (
            <Card className="p-5">
              <div className="mb-3 text-[12px] uppercase tracking-wide text-ink-3">Pré-visualização</div>
              <div className="mx-auto w-fit rounded-md border border-dashed border-line bg-white text-black">
                <Etiqueta a={linhas[0].artigo} f={f} nivel={nivel} mostrar={mostrar} empresa={empresa?.nome} />
              </div>
            </Card>
          )}
        </div>
      </div>

      {total > 0 && (
        <PrintPortal>
          <style>{`@media print { @page { size: ${f.pagina}; margin: ${f.margem}; } }`}</style>
          <div style={{ display: "grid", gridTemplateColumns: `repeat(${f.colunas}, ${f.largura}mm)`, justifyContent: "center" }}>
            {etiquetas.map((a, i) => (
              <div key={i} style={{ breakInside: "avoid", pageBreakAfter: f.colunas === 1 && i < etiquetas.length - 1 ? "always" : "auto" }}>
                <Etiqueta a={a} f={f} nivel={nivel} mostrar={mostrar} empresa={empresa?.nome} />
              </div>
            ))}
          </div>
        </PrintPortal>
      )}
    </>
  );
}

function Etiqueta({
  a,
  f,
  nivel,
  mostrar,
  empresa,
}: {
  a: Artigo;
  f: (typeof FORMATOS)[Formato];
  nivel: number;
  mostrar: { preco: boolean; empresa: boolean; referencia: boolean };
  empresa?: string | null;
}) {
  const codigo = codigoDe(a);
  const alturaBarras = f.altura >= 35 ? 11 : 8;
  const { d, largura } = svgBarras(codigo, 40);
  const pequeno = f.largura < 50;
  return (
    <div
      style={{
        width: `${f.largura}mm`,
        height: `${f.altura}mm`,
        padding: "1.5mm 2.5mm",
        boxSizing: "border-box",
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
        overflow: "hidden",
        fontFamily: "Inter, system-ui, sans-serif",
        color: "#000",
      }}
    >
      <div>
        {mostrar.empresa && empresa && <div style={{ fontSize: pequeno ? "5.5pt" : "6.5pt", textTransform: "uppercase", letterSpacing: "0.04em", opacity: 0.7, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{empresa}</div>}
        <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "1.5mm" }}>
          <div style={{ fontSize: pequeno ? "7pt" : "8.5pt", fontWeight: 600, lineHeight: 1.15, maxHeight: "2.3em", overflow: "hidden", flex: 1, minWidth: 0 }}>{a.descricao_curta || a.descricao}</div>
          {mostrar.preco && (
            <div style={{ fontSize: pequeno ? "9pt" : "12pt", fontWeight: 700, whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums", lineHeight: 1.1 }}>{kz(precoNivel(a, nivel))}</div>
          )}
        </div>
      </div>
      <div>
        <svg viewBox={`0 0 ${largura} 40`} preserveAspectRatio="none" style={{ width: "100%", height: `${alturaBarras}mm`, display: "block" }} shapeRendering="crispEdges">
          <path d={d} fill="#000" />
        </svg>
        <div style={{ fontFamily: "ui-monospace, Menlo, monospace", fontSize: pequeno ? "5.5pt" : "6.5pt", textAlign: "center", letterSpacing: "0.08em" }}>
          {codigo}
          {mostrar.referencia && a.codigo_barras && a.codigo_barras !== a.referencia ? ` · ${a.referencia}` : ""}
        </div>
      </div>
    </div>
  );
}

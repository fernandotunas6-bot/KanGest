// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useMemo, useState } from "react";
import { bip, useLeitor } from "@/lib/leitor";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ClipboardCheck, Printer, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { data, hojeISO, kz, qtd } from "@/lib/format";
import { imprimir } from "@/lib/print";
import type { Artigo } from "@/lib/types";
import { Button, Card, Field, Input, PageHeader, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import DocumentoModal from "@/components/DocumentoModal";

/** Inventário / contagem física: regista as quantidades contadas e acerta o stock (documento IN). */
export default function Inventario() {
  const t = useTabelas().data;
  const qc = useQueryClient();
  const [armazem, setArmazem] = useState<number>(t?.armazem_id ?? 1);
  const [familia, setFamilia] = useState<string>("");
  const [q, setQ] = useState("");
  const [contado, setContado] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState(false);
  const [emitido, setEmitido] = useState<number | null>(null);
  const dq = useDebounce(q);

  const artigos = useQuery({
    queryKey: ["inventario", armazem, familia, dq],
    queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, familia_id: familia ? Number(familia) : null, armazem_id: armazem, limite: 5000 }),
  });
  const lista = useMemo(() => (artigos.data ?? []).filter((a) => a.tipo === "P" && a.controla_stock === 1 && a.ativo === 1), [artigos.data]);

  const contados = lista.filter((a) => (contado[a.id] ?? "").trim() !== "");
  const num = (s: string) => Number(s.replace(",", "."));
  const diferencas = contados.filter((a) => Math.abs(num(contado[a.id]) - a.stock) > 1e-9);
  const valorDif = diferencas.reduce((s, a) => s + Math.round((num(contado[a.id]) - a.stock) * a.custo_medio), 0);

  // contar com o leitor: cada leitura soma 1 (ou as unidades da embalagem lida) à contagem do artigo
  const [ultimo, setUltimo] = useState<number | null>(null);
  useLeitor(async (l) => {
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo: l.codigo });
      const daLista = a ? lista.find((x) => x.id === a.id) : undefined;
      if (!a || !daLista) {
        bip(false);
        toast.warning(a ? `${a.descricao} não está na lista (outra família, pesquisa ou sem controlo de stock).` : `Nenhum artigo com o código ${l.codigo}.`);
        return;
      }
      bip(true);
      const soma = a.quantidade_lida ?? 1;
      setContado((c) => ({ ...c, [a.id]: String(+((num(c[a.id] ?? "") || 0) + soma).toFixed(3)).replace(".", ",") }));
      setUltimo(a.id);
      window.setTimeout(() => document.getElementById(`inv-${a.id}`)?.scrollIntoView({ block: "center", behavior: "smooth" }), 60);
    } catch (e) {
      toast.error((e as Error).message);
    }
  });

  async function gravar() {
    if (!contados.length) return toast.error("Indique pelo menos uma quantidade contada.");
    if (contados.some((a) => !Number.isFinite(num(contado[a.id])) || num(contado[a.id]) < 0)) return toast.error("Há quantidades inválidas.");
    setBusy(true);
    try {
      const d = await call<{ documento: { id: number; numero_doc: string } }>("documentos.emitir", {
        tipo: "IN",
        armazem_id: armazem,
        observacoes: `Inventário de ${data(hojeISO())} — ${contados.length} artigos contados`,
        linhas: contados.map((a) => ({ artigo_id: a.id, quantidade: num(contado[a.id]) })),
      });
      toast.success(`${d.documento.numero_doc} gravado. Stock acertado.`);
      setContado({});
      qc.invalidateQueries();
      setEmitido(d.documento.id);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const nomeArmazem = t?.armazens.find((a) => a.id === armazem)?.nome ?? "";

  return (
    <>
      <PageHeader
        title="Inventário"
        subtitle="Contagem física e regularização do stock"
        actions={
          <>
            <Button variant="secondary" onClick={imprimir}>
              <Printer className="size-4" /> Folha de contagem
            </Button>
            <Button loading={busy} disabled={!contados.length} onClick={gravar}>
              <ClipboardCheck className="size-4" /> Gravar inventário
            </Button>
          </>
        }
      />
      <div className="mb-4 flex flex-wrap items-end gap-3">
        <Field label="Armazém">
          <Select className="w-52" value={armazem} onChange={(e) => setArmazem(Number(e.target.value))}>
            {t?.armazens.map((a) => (
              <option key={a.id} value={a.id}>
                {a.nome}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Família">
          <Select className="w-52" value={familia} onChange={(e) => setFamilia(e.target.value)}>
            <option value="">Todas</option>
            {t?.familias.map((f) => (
              <option key={f.id} value={f.id}>
                {f.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <div className="relative min-w-64 flex-1">
          <Search className="absolute left-3 top-[34px] size-4 text-ink-3" />
          <Field label="Pesquisar">
            <Input className="pl-9" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Descrição, referência ou código — com o leitor, cada leitura conta 1" />
          </Field>
        </div>
      </div>

      <div className="mb-4 grid grid-cols-3 gap-4">
        <Stat label="Artigos na lista" value={lista.length} />
        <Stat label="Contados" value={contados.length} hint={`${diferencas.length} com diferença`} tone="accent" />
        <Stat label="Valor das diferenças (custo médio)" value={kz(valorDif)} tone={valorDif < 0 ? "danger" : "ok"} />
      </div>

      <Card>
        <Table className="max-h-[calc(100vh-464px)]">
          <thead>
            <tr>
              <Th>Artigo</Th>
              <Th>Un.</Th>
              <Th className="text-right">Existência</Th>
              <Th className="w-36 text-right">Contado</Th>
              <Th className="text-right">Diferença</Th>
            </tr>
          </thead>
          <tbody>
            {lista.map((a) => {
              const v = contado[a.id] ?? "";
              const dif = v.trim() === "" ? null : num(v) - a.stock;
              return (
                <tr key={a.id} id={`inv-${a.id}`} className={cn(dif !== null && Math.abs(dif) > 1e-9 && "bg-accent-50/40", ultimo === a.id && "bg-brand-50 ring-1 ring-inset ring-brand/30")}>
                  <Td>
                    <div className="font-medium">{a.descricao}</div>
                    <div className="font-mono text-[11.5px] text-ink-3">{a.referencia}</div>
                  </Td>
                  <Td className="text-ink-2">{a.unidade}</Td>
                  <Td className="num text-right">{qtd(a.stock)}</Td>
                  <Td>
                    <Input className="num h-8 text-right" inputMode="decimal" value={v} placeholder="—" onChange={(e) => setContado({ ...contado, [a.id]: e.target.value })} />
                  </Td>
                  <Td className={cn("num text-right font-medium", dif === null ? "text-ink-3" : dif < 0 ? "text-danger" : dif > 0 ? "text-ok" : "text-ink-3")}>
                    {dif === null ? "" : `${dif > 0 ? "+" : ""}${qtd(dif)}`}
                  </Td>
                </tr>
              );
            })}
          </tbody>
        </Table>
        {!lista.length && <p className="py-10 text-center text-ink-3">Sem artigos com controlo de stock para os filtros escolhidos.</p>}
      </Card>

      {!emitido && (
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <div className="text-[16px] font-bold">Folha de contagem de inventário</div>
          <div className="mb-3">
            Armazém: {nomeArmazem} · Data: {data(hojeISO())} · Contado por: ____________________
          </div>
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th className="py-1">Referência</th>
                <th>Descrição</th>
                <th>Un.</th>
                <th className="w-32 text-right">Quantidade contada</th>
              </tr>
            </thead>
            <tbody>
              {lista.map((a) => (
                <tr key={a.id} className="border-b border-black/30">
                  <td className="py-1.5">{a.referencia}</td>
                  <td>{a.descricao}</td>
                  <td>{a.unidade}</td>
                  <td className="text-right">________</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PrintPortal>
      )}
      <DocumentoModal id={emitido} onClose={() => setEmitido(null)} />
    </>
  );
}

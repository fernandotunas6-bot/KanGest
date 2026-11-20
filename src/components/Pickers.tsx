// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { bip } from "@/lib/leitor";
import { kz, qtd } from "@/lib/format";
import type { Artigo, Entidade } from "@/lib/types";
import { Field, Input, cn } from "@/components/ui";

/** Pesquisa de cliente / fornecedor com lista suspensa. */
export function EntidadePicker({
  tipo,
  valor,
  onChange,
  bloqueado,
  label,
}: {
  tipo: "C" | "F";
  valor: Entidade | null;
  onChange: (e: Entidade) => void;
  bloqueado?: boolean;
  label?: string;
}) {
  const [q, setQ] = useState("");
  const [aberto, setAberto] = useState(false);
  const dq = useDebounce(q);
  const lista = useQuery({ queryKey: ["ent-picker", tipo, dq], queryFn: () => call<Entidade[]>("entidades.listar", { tipo, q: dq }), enabled: aberto });
  return (
    <Field label={label ?? (tipo === "C" ? "Cliente" : "Fornecedor")} className="relative">
      <Input
        disabled={bloqueado}
        value={aberto ? q : valor ? `${valor.nome} · ${valor.nif}` : ""}
        placeholder={tipo === "C" ? "Consumidor Final" : "Seleccione o fornecedor…"}
        onFocus={() => {
          setAberto(true);
          setQ("");
        }}
        onBlur={() => setTimeout(() => setAberto(false), 150)}
        onChange={(e) => setQ(e.target.value)}
      />
      {aberto && (
        <div className="absolute left-0 right-0 top-full z-20 mt-1 max-h-72 overflow-y-auto rounded-xl border border-line bg-surface p-1 shadow-pop">
          {lista.data?.map((e) => (
            <button key={e.id} type="button" onMouseDown={() => onChange(e)} className="block w-full rounded-lg px-3 py-2 text-left hover:bg-sunken">
              <div className="font-medium">{e.nome}</div>
              <div className="text-[12px] text-ink-3">
                NIF {e.nif}
                {(e.saldo ?? 0) > 0 ? ` · saldo ${kz(e.saldo)}` : ""}
              </div>
            </button>
          ))}
          {lista.data?.length === 0 && <div className="px-3 py-2 text-ink-3">Sem resultados.</div>}
        </div>
      )}
    </Field>
  );
}

/** Pesquisa de artigos (descrição, referência ou código de barras); Enter escolhe o primeiro. */
export function ArtigoPicker({
  onPick,
  placeholder,
  filtro,
  className,
}: {
  /** `quantidade`: a que o código lido vale (embalagem de 12, etiqueta da balança); sem ela, 1 */
  onPick: (a: Artigo, quantidade?: number) => void;
  placeholder?: string;
  /** Exclui artigos da lista (ex.: kits na composição de outro kit). */
  filtro?: (a: Artigo) => boolean;
  className?: string;
}) {
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const lista = useQuery({ queryKey: ["art-picker", dq], queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, limite: 12 }), enabled: dq.length > 0 });
  const itens = (lista.data ?? []).filter((a) => !filtro || filtro(a));
  // Enter (o leitor acaba a leitura com ele): primeiro o código exacto — com a embalagem, a balança e o GS1 —, só
  // depois o primeiro da pesquisa; a pesquisa ainda pode estar atrasada em relação ao que o leitor escreveu
  async function escolherPeloTexto() {
    const texto = q.trim();
    if (!texto) return;
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo: texto });
      if (a && (!filtro || filtro(a))) {
        bip(true);
        onPick(a, a.quantidade_lida);
        setQ("");
        return;
      }
      const r = (await call<Artigo[]>("artigos.listar", { q: texto, limite: 12 })).filter((x) => !filtro || filtro(x));
      if (r[0]) {
        onPick(r[0]);
        setQ("");
      } else {
        bip(false);
        toast.warning(`Nenhum artigo com «${texto}».`);
      }
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <div className={cn("relative", className)} data-leitor>
      <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
      <Input
        className="pl-9"
        placeholder={placeholder ?? "Pesquisar artigo por descrição, referência ou código de barras…"}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void escolherPeloTexto();
          }
        }}
      />
      {q && !!itens.length && (
        <div className="absolute left-0 right-0 top-full z-20 mt-1 max-h-80 overflow-y-auto rounded-xl border border-line bg-surface p-1 shadow-pop">
          {itens.map((a) => (
            <button
              key={a.id}
              type="button"
              onClick={() => {
                onPick(a);
                setQ("");
              }}
              className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left hover:bg-sunken"
            >
              <span>
                <span className="font-medium">{a.descricao}</span>
                <span className="ml-2 font-mono text-[11.5px] text-ink-3">{a.referencia}</span>
                {!!a.composto && <span className="ml-2 rounded bg-brand-50 px-1.5 text-[11px] text-brand-700">kit</span>}
                {!!a.controla_lote && <span className="ml-2 rounded bg-accent-50 px-1.5 text-[11px] text-accent-ink">lotes</span>}
              </span>
              <span className="num text-[13px] text-ink-2">
                {kz(a.preco1_iva)} · stock {qtd(a.stock)}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

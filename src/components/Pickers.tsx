import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
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
  onPick: (a: Artigo) => void;
  placeholder?: string;
  /** Exclui artigos da lista (ex.: kits na composição de outro kit). */
  filtro?: (a: Artigo) => boolean;
  className?: string;
}) {
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const lista = useQuery({ queryKey: ["art-picker", dq], queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, limite: 12 }), enabled: dq.length > 0 });
  const itens = (lista.data ?? []).filter((a) => !filtro || filtro(a));
  return (
    <div className={cn("relative", className)}>
      <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
      <Input
        className="pl-9"
        placeholder={placeholder ?? "Pesquisar artigo por descrição, referência ou código de barras…"}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && itens[0]) {
            onPick(itens[0]);
            setQ("");
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
                {!!a.controla_lote && <span className="ml-2 rounded bg-accent-50 px-1.5 text-[11px] text-[#9a6514]">lotes</span>}
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

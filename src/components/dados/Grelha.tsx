// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Grelha do motor central de dados: mostra o resultado de dados.consultar (colunas, linhas, total geral), ordena por
// várias colunas (clique ordena; Shift + clique junta mais uma coluna) e formata cada valor pelo tipo do campo.
// Feita com as peças de components/ui (Table, Td, LinhasCarregando, Empty), para ter o aspecto do resto do programa.
import type { MouseEvent } from "react";
import { ArrowDown, ArrowUp, ChevronsUpDown, Table2 } from "lucide-react";
import { alinhaDireita, mostrar, type OrdemDados, type ResultadoDados } from "@/lib/dados";
import { numero } from "@/lib/format";
import { Empty, LinhasCarregando, Table, Td, cn } from "@/components/ui";

export function Grelha({
  r,
  aCarregar,
  ordenar,
  onOrdenar,
  ordenavel,
  className,
}: {
  r?: ResultadoDados;
  aCarregar?: boolean;
  ordenar: OrdemDados[];
  /** campo e se junta à ordenação actual (Shift) em vez de a substituir */
  onOrdenar?: (campo: string, juntar: boolean) => void;
  /** as colunas que se podem ordenar (por omissão, todas); as da comparação de períodos não vêm do motor */
  ordenavel?: (id: string) => boolean;
  className?: string;
}) {
  if (!r && !aCarregar) return null;
  const colunas = r?.colunas ?? [];
  const temTotais = !!r && Object.keys(r.totais).length > 0;
  return (
    <>
      <Table className={cn("tabela-compacta grelha-dados", className)}>
        <thead>
          <tr>
            {colunas.map((c) => {
              const i = ordenar.findIndex((o) => o.campo === c.id);
              const o = i >= 0 ? ordenar[i] : null;
              const Seta = !o ? ChevronsUpDown : o.desc ? ArrowDown : ArrowUp;
              return (
                <th
                  key={c.id}
                  aria-sort={!o ? undefined : o.desc ? "descending" : "ascending"}
                  className={cn(
                    "sticky top-0 z-[1] border-b border-line bg-surface px-4 py-[var(--kg-cab-y)] text-[12px] font-medium uppercase tracking-wide text-ink-3",
                    alinhaDireita(c.tipo) && "text-right",
                  )}
                >
                  {onOrdenar && (!ordenavel || ordenavel(c.id)) ? (
                    <button
                      type="button"
                      onClick={(e: MouseEvent) => onOrdenar(c.id, e.shiftKey)}
                      title="Ordenar por esta coluna (Shift + clique junta-a à ordenação)"
                      className={cn("group/ord inline-flex items-center gap-1 uppercase tracking-wide transition hover:text-ink", o && "text-ink")}
                    >
                      {c.rotulo}
                      <Seta className={cn("size-3.5 shrink-0 print:hidden", o ? "text-brand" : "opacity-0 group-hover/ord:opacity-60")} />
                      {o && ordenar.length > 1 && <span className="num text-[10px] text-brand">{i + 1}</span>}
                    </button>
                  ) : (
                    c.rotulo
                  )}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {!r && <LinhasCarregando colunas={Math.max(3, colunas.length)} />}
          {r?.linhas.map((l, n) => (
            <tr key={n} className="hover:bg-sunken/60">
              {l.map((v, i) => (
                <Td
                  key={i}
                  className={cn(
                    alinhaDireita(colunas[i].tipo) && "num text-right",
                    typeof v === "number" && v < 0 && (colunas[i].tipo === "dinheiro" || colunas[i].tipo === "percentagem") && "text-danger",
                    typeof v === "number" && v > 0 && colunas[i].tipo === "percentagem" && "text-ok",
                  )}
                >
                  {mostrar(v, colunas[i].tipo)}
                </Td>
              ))}
            </tr>
          ))}
        </tbody>
        {temTotais && (
          <tfoot>
            {/* o total fica sempre à vista no fundo da tabela, mesmo com muitas linhas */}
            <tr className="font-semibold">
              {colunas.map((c, i) => (
                <Td key={c.id} className={cn("sticky bottom-0 z-[1] border-t border-line-strong bg-surface", alinhaDireita(c.tipo) && "num text-right")}>
                  {c.id in r.totais ? mostrar(r.totais[c.id], c.tipo) : i === 0 ? "Total" : ""}
                </Td>
              ))}
            </tr>
          </tfoot>
        )}
      </Table>
      {r && r.linhas.length === 0 && <Empty icon={<Table2 className="size-6" />} title="Nada com estes filtros" text="Alargue o período ou retire algum filtro." />}
      {r && r.truncado && (
        <p className="px-5 py-2 text-[12.5px] text-ink-3">
          A mostrar {numero(r.linhas.length, 0)} de {numero(r.total_linhas, 0)} linhas; o total conta todas. O Excel e o CSV levam até 5 000 linhas; para ver o resto aqui, junte filtros.
        </p>
      )}
    </>
  );
}

// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { cn } from "@/components/ui";
import { numero } from "@/lib/format";
import type { Ponto } from "@/components/BarChart";

/** Partes mostradas uma a uma; as outras juntam-se numa linha «Outros». */
const MAX_PARTES = 11;

/**
 * Que parte do total é cada grupo: barras horizontais de uma só cor, da maior para a menor, num só eixo (lêem-se melhor
 * do que as fatias de um gráfico circular), com o valor e a percentagem ao lado em texto. `total` é o total geral da
 * coluna (inclui os grupos que não vieram na lista), por isso «Outros» fecha a conta. `aoClicar` abre o detalhe.
 */
export function PesoNoTotal({ dados, total, formatar, aoClicar }: { dados: Ponto[]; total: number; formatar: (v: number) => string; aoClicar?: (i: number) => void }) {
  if (dados.some((d) => d.valor < 0) || total <= 0) {
    return <p className="py-10 text-center text-[13px] text-ink-3">O peso no total só se mostra quando todos os valores são positivos.</p>;
  }
  const ordenados = dados.map((d, i) => ({ ...d, i })).sort((a, b) => b.valor - a.valor);
  const mostrados = ordenados.slice(0, MAX_PARTES);
  const resto = total - mostrados.reduce((s, d) => s + d.valor, 0);
  const linhas = [...mostrados, ...(resto > 0.5 ? [{ chave: "Outros", rotulo: "Outros", valor: resto, i: -1 }] : [])];
  const maior = Math.max(...linhas.map((l) => l.valor), 1);

  return (
    <div className="space-y-1.5 py-2" role="list" aria-label="Peso de cada grupo no total">
      {linhas.map((l) => {
        const clicavel = !!aoClicar && l.i >= 0;
        const pct = (100 * l.valor) / total;
        return (
          <button
            key={`${l.chave}-${l.i}`}
            type="button"
            role="listitem"
            disabled={!clicavel}
            onClick={clicavel ? () => aoClicar!(l.i) : undefined}
            title={clicavel ? "Ver o detalhe" : undefined}
            className={cn(
              "grid w-full grid-cols-[minmax(6rem,14rem)_1fr_auto] items-center gap-3 rounded-md px-1.5 py-1 text-left text-[13px]",
              clicavel ? "hover:bg-sunken/70" : "cursor-default",
            )}
          >
            <span className={cn("truncate", l.i < 0 ? "text-ink-3" : "text-ink")}>{l.chave}</span>
            <span className="h-3.5 overflow-hidden rounded-r-[4px]">
              <span className="block h-full rounded-r-[4px]" style={{ width: `${Math.max(0.5, (100 * l.valor) / maior)}%`, background: l.i < 0 ? "var(--color-line-strong)" : "var(--color-chart)" }} />
            </span>
            <span className="num whitespace-nowrap text-right text-ink-2">
              {formatar(l.valor)} <span className="ml-1 inline-block w-14 text-ink-3">{numero(pct, 1)}%</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

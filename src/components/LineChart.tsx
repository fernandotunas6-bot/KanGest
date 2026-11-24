// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useRef, useState } from "react";
import { numero } from "@/lib/format";
import type { Ponto } from "@/components/BarChart";

/** Escala «limpa» que inclui o zero e os negativos: −50K / 0 / 50K / 100K… */
function ticks(min: number, max: number, n = 4) {
  if (max <= min) return [min === 0 ? 0 : Math.min(0, min), Math.max(0, max) || 1];
  const bruto = (max - min) / n;
  const mag = 10 ** Math.floor(Math.log10(bruto));
  const passo = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((p) => p >= bruto) ?? bruto;
  const ini = Math.floor(min / passo) * passo;
  const out: number[] = [];
  for (let v = ini; v <= max + passo * 0.999; v += passo) out.push(Math.abs(v) < passo * 1e-9 ? 0 : v);
  return out.length > 1 ? out : [ini, ini + passo];
}

const compacto = (v: number) => {
  const a = Math.abs(v);
  if (a >= 1e6) return `${numero(+(v / 1e6).toFixed(1), 1)} M`;
  if (a >= 1e4) return `${numero(+(v / 1e3).toFixed(0), 0)} mil`;
  return numero(v, 0);
};

/**
 * Linha de uma série ao longo do tempo (sem legenda: o título nomeia a série). Linha de 2px, marcador de 8px com anel
 * da cor do fundo no ponto sob o cursor, guia vertical e dica com o valor; aceita valores negativos (linha do zero).
 * Os valores vêm em cêntimos, como no BarChart. `aoClicar` abre o detalhe do ponto.
 */
export function LineChart({ dados, formatar, altura = 220, aoClicar }: { dados: Ponto[]; formatar: (v: number) => string; altura?: number; aoClicar?: (i: number) => void }) {
  const [hover, setHover] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const [largura, setLargura] = useState(720);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setLargura(el.clientWidth));
    ro.observe(el);
    setLargura(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const valores = dados.map((d) => d.valor);
  const ys = ticks(Math.min(0, ...valores), Math.max(0, ...valores));
  const lo = ys[0];
  const hi = ys[ys.length - 1];
  const m = { l: 52, r: 12, t: 12, b: 26 };
  const total = Math.max(200, largura);
  const w = total - m.l - m.r;
  const h = altura - m.t - m.b;
  const banda = w / Math.max(1, dados.length);
  const x = (i: number) => m.l + banda * i + banda / 2;
  const y = (v: number) => m.t + h - ((v - lo) / (hi - lo || 1)) * h;
  const passoRotulo = Math.ceil(dados.length / 8);
  const caminho = dados.map((d, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(d.valor).toFixed(1)}`).join(" ");
  const ultimo = dados.length - 1;

  return (
    <div ref={ref} className="relative w-full">
      <svg width="100%" height={altura} viewBox={`0 0 ${total} ${altura}`} role="img" aria-label="Gráfico de linhas">
        {ys.map((t) => (
          <g key={t}>
            <line x1={m.l} x2={m.l + w} y1={y(t)} y2={y(t)} stroke={t === 0 ? "var(--color-line-strong)" : "var(--color-grid)"} strokeWidth={1} />
            <text x={m.l - 8} y={y(t)} dy="0.32em" textAnchor="end" className="num fill-ink-3 text-[11px]">
              {compacto(t / 100)}
            </text>
          </g>
        ))}
        {hover !== null && dados[hover] && <line x1={x(hover)} x2={x(hover)} y1={m.t} y2={m.t + h} stroke="var(--color-line-strong)" strokeWidth={1} />}
        {dados.length > 0 && <path d={caminho} fill="none" stroke="var(--color-chart)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />}
        {/* um só ponto, ou o último: marcador para a linha não começar nem acabar no ar */}
        {dados.length > 0 && (hover === null || dados.length === 1) && (
          <circle cx={x(ultimo)} cy={y(dados[ultimo].valor)} r={4} fill="var(--color-chart)" stroke="var(--color-surface)" strokeWidth={2} />
        )}
        {hover !== null && dados[hover] && <circle cx={x(hover)} cy={y(dados[hover].valor)} r={4} fill="var(--color-chart)" stroke="var(--color-surface)" strokeWidth={2} />}
        {dados.map((d, i) => (
          <g key={d.chave}>
            {/* alvo de hover: toda a faixa do ponto (maior do que o marcador) */}
            <rect
              x={m.l + i * banda}
              y={m.t}
              width={banda}
              height={h}
              fill="transparent"
              tabIndex={0}
              aria-label={`${d.chave}: ${formatar(d.valor)}`}
              onPointerEnter={() => setHover(i)}
              onPointerLeave={() => setHover(null)}
              onFocus={() => setHover(i)}
              onBlur={() => setHover(null)}
              onClick={aoClicar ? () => aoClicar(i) : undefined}
              onKeyDown={aoClicar ? (e) => e.key === "Enter" && aoClicar(i) : undefined}
              className={aoClicar ? "cursor-pointer outline-none" : "outline-none"}
            />
            {i % passoRotulo === 0 && (
              <text x={x(i)} y={altura - 8} textAnchor="middle" className="num fill-ink-3 text-[11px]">
                {d.rotulo}
              </text>
            )}
          </g>
        ))}
      </svg>
      {hover !== null && dados[hover] && (
        <div
          className="pointer-events-none absolute z-10 -translate-x-1/2 -translate-y-full rounded-lg border border-line bg-surface px-3 py-2 shadow-pop"
          style={{ left: x(hover), top: Math.min(y(dados[hover].valor), m.t + h) - 10 }}
        >
          <div className="num font-semibold text-ink">{formatar(dados[hover].valor)}</div>
          <div className="text-[12px] text-ink-3">{dados[hover].chave}</div>
          {aoClicar && <div className="mt-0.5 text-[11.5px] text-ink-3">Clique para ver o detalhe</div>}
        </div>
      )}
    </div>
  );
}

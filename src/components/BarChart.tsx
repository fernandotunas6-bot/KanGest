import { useEffect, useMemo, useRef, useState } from "react";
import { numero } from "@/lib/format";

export interface Ponto {
  chave: string; // rótulo (ex.: data)
  rotulo: string; // rótulo curto para o eixo
  valor: number;
}

/** Escala "limpa" para o eixo Y: 0 / 50K / 100K… */
function ticks(max: number, n = 4) {
  if (max <= 0) return [0]; // sem dados: só a linha de base (antes repetia o "0")
  const bruto = max / n;
  const mag = 10 ** Math.floor(Math.log10(bruto));
  const passo = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((p) => p >= bruto) ?? bruto;
  const out: number[] = [];
  for (let v = 0; v <= max + passo * 0.001; v += passo) out.push(v);
  if (out[out.length - 1] < max) out.push(out[out.length - 1] + passo);
  return out;
}

const compacto = (v: number) => {
  const a = Math.abs(v);
  if (a >= 1e6) return `${numero(+(v / 1e6).toFixed(1), 1)} M`;
  if (a >= 1e4) return `${numero(+(v / 1e3).toFixed(0), 0)} mil`;
  return numero(v, 0);
};

/**
 * Colunas de uma série (sem legenda — o título do cartão nomeia a série).
 * Barras ≤ 24px, topo arredondado 4px e base recta, 2px de ar entre barras,
 * grelha hairline sólida, tooltip por barra (hover e foco de teclado).
 */
export function BarChart({ dados, formatar, altura = 220 }: { dados: Ponto[]; formatar: (v: number) => string; altura?: number }) {
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

  const max = Math.max(0, ...dados.map((d) => d.valor));
  const ys = useMemo(() => ticks(max), [max]);
  const topo = ys[ys.length - 1] || 1;
  const m = { l: 48, r: 8, t: 10, b: 26 };
  const w = Math.max(200, largura) - m.l - m.r;
  const h = altura - m.t - m.b;
  const banda = w / Math.max(1, dados.length);
  const bw = Math.min(24, Math.max(2, banda - 2));
  const y = (v: number) => m.t + h - (Math.max(0, v) / topo) * h;
  const passoRotulo = Math.ceil(dados.length / 8);

  return (
    <div ref={ref} className="relative w-full">
      <svg width="100%" height={altura} viewBox={`0 0 ${Math.max(200, largura)} ${altura}`} role="img" aria-label="Gráfico de colunas">
        {ys.map((t) => (
          <g key={t}>
            <line x1={m.l} x2={m.l + w} y1={y(t)} y2={y(t)} stroke="var(--color-grid)" strokeWidth={1} />
            <text x={m.l - 8} y={y(t)} dy="0.32em" textAnchor="end" className="num fill-ink-3 text-[11px]">
              {compacto(t / 100)}
            </text>
          </g>
        ))}
        {dados.map((d, i) => {
          const x = m.l + i * banda + (banda - bw) / 2;
          const top = y(d.valor);
          const hb = m.t + h - top;
          const r = Math.min(4, hb, bw / 2);
          // topo arredondado, base recta
          const path =
            hb <= 0
              ? ""
              : `M${x},${m.t + h} V${top + r} Q${x},${top} ${x + r},${top} H${x + bw - r} Q${x + bw},${top} ${x + bw},${top + r} V${m.t + h} Z`;
          return (
            <g key={d.chave}>
              {path && <path d={path} fill="var(--color-chart)" opacity={hover === null || hover === i ? 1 : 0.55} />}
              {/* alvo de hover maior do que a barra: toda a banda */}
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
                className="outline-none"
              />
              {i % passoRotulo === 0 && (
                <text x={m.l + i * banda + banda / 2} y={altura - 8} textAnchor="middle" className="num fill-ink-3 text-[11px]">
                  {d.rotulo}
                </text>
              )}
            </g>
          );
        })}
        <line x1={m.l} x2={m.l + w} y1={m.t + h} y2={m.t + h} stroke="var(--color-line-strong)" strokeWidth={1} />
      </svg>
      {hover !== null && dados[hover] && (
        <div
          className="pointer-events-none absolute z-10 -translate-x-1/2 -translate-y-full rounded-lg border border-line bg-surface px-3 py-2 shadow-pop"
          style={{ left: m.l + hover * banda + banda / 2, top: y(dados[hover].valor) - 6 }}
        >
          <div className="num font-semibold text-ink">{formatar(dados[hover].valor)}</div>
          <div className="text-[12px] text-ink-3">{dados[hover].chave}</div>
        </div>
      )}
    </div>
  );
}

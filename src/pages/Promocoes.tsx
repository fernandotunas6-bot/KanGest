// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Plus, Tag, Ticket } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas } from "@/lib/hooks";
import { data, kz, numero, parseKz, toInputKz } from "@/lib/format";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th } from "@/components/ui";
import { ArtigoPicker } from "@/components/Pickers";
import type { Artigo } from "@/lib/types";

interface Regra {
  id: number;
  descricao: string;
  tipo: "LEVE_PAGUE" | "QUANTIDADE" | "HORARIO";
  artigo_id: number | null;
  artigo: string | null;
  familia_id: number | null;
  familia: string | null;
  leve: number | null;
  pague: number | null;
  quantidade_min: number | null;
  desconto_pct: number | null;
  hora_inicio: string | null;
  hora_fim: string | null;
  dias_semana: string | null;
  data_inicio: string | null;
  data_fim: string | null;
  ativo: number;
}

interface Cupao {
  id: number;
  codigo: string;
  descricao: string | null;
  tipo: "PCT" | "VALOR";
  valor: number;
  valor_minimo: number;
  data_inicio: string | null;
  data_fim: string | null;
  utilizacoes_max: number | null;
  utilizacoes: number;
  ativo: number;
}

const TIPOS = { LEVE_PAGUE: "Leve N pague M", QUANTIDADE: "Desconto por quantidade", HORARIO: "Desconto por horário" } as const;
const DIAS = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];

function resumoRegra(r: Regra) {
  const alvo = r.artigo ?? (r.familia ? `família ${r.familia}` : "todos os artigos");
  const regra =
    r.tipo === "LEVE_PAGUE"
      ? `leve ${numero(r.leve ?? 0, 3)} pague ${numero(r.pague ?? 0, 3)}`
      : r.tipo === "QUANTIDADE"
        ? `−${numero(r.desconto_pct ?? 0, 2)}% a partir de ${numero(r.quantidade_min ?? 0, 3)} un.`
        : `−${numero(r.desconto_pct ?? 0, 2)}% das ${r.hora_inicio} às ${r.hora_fim}${r.dias_semana ? ` (${r.dias_semana.split("").map((d) => DIAS[Number(d) - 1]).join(", ")})` : ""}`;
  return `${regra} · ${alvo}`;
}

/** Promoções por regras (leve N pague M, quantidade, horário) e cupões de desconto. */
export default function Promocoes() {
  const tem = useSession((s) => s.tem);
  const { data: d, refetch } = useQuery({ queryKey: ["promocoes"], queryFn: () => call<{ regras: Regra[]; cupoes: Cupao[] }>("promocoes.listar") });
  const [regra, setRegra] = useState<Partial<Regra> | null>(null);
  const [cupao, setCupao] = useState<Partial<Cupao> | null>(null);
  const pode = tem("artigos.editar");
  const validade = (i: string | null, f: string | null) => (i || f ? `${i ? data(i) : "…"} a ${f ? data(f) : "…"}` : "Sempre");
  return (
    <>
      <PageHeader title="Promoções e Cupões" subtitle="Regras aplicadas automaticamente no POS e nas facturas, e cupões de desconto" />
      <div className="grid gap-4">
        <Card>
          <div className="flex items-center justify-between px-5 pt-4">
            <span className="flex items-center gap-2 font-semibold">
              <Tag className="size-4 text-brand" /> Regras de promoção
            </span>
            {pode && (
              <Button size="sm" onClick={() => setRegra({ tipo: "LEVE_PAGUE", ativo: 1, leve: 3, pague: 2 })}>
                <Plus className="size-4" /> Nova regra
              </Button>
            )}
          </div>
          <p className="px-5 pb-2 text-[12.5px] text-ink-3">
            Somam-se à promoção da ficha do artigo: em cada linha vale o maior desconto. Até esse desconto, o operador não precisa de autorização.
          </p>
          <Table>
            <thead>
              <tr>
                <Th>Promoção</Th>
                <Th>Regra</Th>
                <Th>Validade</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {d?.regras.map((r) => (
                <tr key={r.id} className={r.ativo ? undefined : "opacity-60"}>
                  <Td className="font-medium">
                    {r.descricao} {!r.ativo && <Badge>inactiva</Badge>}
                  </Td>
                  <Td className="text-ink-2">{resumoRegra(r)}</Td>
                  <Td className="text-ink-2">{validade(r.data_inicio, r.data_fim)}</Td>
                  <Td className="text-right">{pode && <Button size="sm" variant="ghost" onClick={() => setRegra(r)}>Editar</Button>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          {!d?.regras.length && <p className="py-6 text-center text-ink-3">Ainda não há regras.</p>}
        </Card>

        <Card>
          <div className="flex items-center justify-between px-5 pt-4">
            <span className="flex items-center gap-2 font-semibold">
              <Ticket className="size-4 text-brand" /> Cupões de desconto
            </span>
            {pode && (
              <Button size="sm" onClick={() => setCupao({ tipo: "PCT", ativo: 1, valor: 10, valor_minimo: 0 })}>
                <Plus className="size-4" /> Novo cupão
              </Button>
            )}
          </div>
          <p className="px-5 pb-2 text-[12.5px] text-ink-3">No POS, botão «Cupão»: o desconto aplica-se à venda toda, sem pedir autorização.</p>
          <Table>
            <thead>
              <tr>
                <Th>Código</Th>
                <Th>Desconto</Th>
                <Th>Compra mínima</Th>
                <Th>Validade</Th>
                <Th className="text-right">Utilizações</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {d?.cupoes.map((c) => (
                <tr key={c.id} className={c.ativo ? undefined : "opacity-60"}>
                  <Td>
                    <span className="font-mono font-medium">{c.codigo}</span>
                    {c.descricao && <span className="ml-2 text-ink-3">{c.descricao}</span>}
                  </Td>
                  <Td>{c.tipo === "PCT" ? `${numero(c.valor, 2)}%` : kz(c.valor)}</Td>
                  <Td className="text-ink-2">{c.valor_minimo ? kz(c.valor_minimo) : "—"}</Td>
                  <Td className="text-ink-2">{validade(c.data_inicio, c.data_fim)}</Td>
                  <Td className="num text-right">
                    {c.utilizacoes}
                    {c.utilizacoes_max ? ` / ${c.utilizacoes_max}` : ""}
                  </Td>
                  <Td className="text-right">{pode && <Button size="sm" variant="ghost" onClick={() => setCupao(c)}>Editar</Button>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
          {!d?.cupoes.length && <p className="py-6 text-center text-ink-3">Ainda não há cupões.</p>}
        </Card>
      </div>
      {regra && <RegraModal inicial={regra} onClose={() => setRegra(null)} onDone={() => { setRegra(null); refetch(); }} />}
      {cupao && <CupaoModal inicial={cupao} onClose={() => setCupao(null)} onDone={() => { setCupao(null); refetch(); }} />}
    </>
  );
}

function RegraModal({ inicial, onClose, onDone }: { inicial: Partial<Regra>; onClose: () => void; onDone: () => void }) {
  const familias = useTabelas().data?.familias ?? [];
  const [r, setR] = useState<Partial<Regra>>(inicial);
  const [alvo, setAlvo] = useState<"artigo" | "familia" | "todos">(inicial.artigo_id ? "artigo" : inicial.familia_id ? "familia" : "artigo");
  const [artigo, setArtigo] = useState<{ id: number; descricao: string } | null>(inicial.artigo_id ? { id: inicial.artigo_id, descricao: inicial.artigo ?? "" } : null);
  const [busy, setBusy] = useState(false);
  const set = (p: Partial<Regra>) => setR((x) => ({ ...x, ...p }));
  async function gravar() {
    setBusy(true);
    try {
      await call("promocoes.regraGuardar", {
        ...r,
        artigo_id: alvo === "artigo" ? artigo?.id ?? null : null,
        familia_id: alvo === "familia" ? r.familia_id ?? null : null,
        ativo: !!r.ativo,
      });
      toast.success("Promoção gravada.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const diaMarcado = (d: number) => (r.dias_semana ?? "").includes(String(d));
  return (
    <Modal open onClose={onClose} title={r.id ? "Editar promoção" : "Nova promoção"} width="max-w-xl" footer={<Button loading={busy} onClick={gravar}>Gravar</Button>}>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Nome (aparece no POS)" className="col-span-2">
          <Input autoFocus value={r.descricao ?? ""} onChange={(e) => set({ descricao: e.target.value })} placeholder="Ex.: Leve 3 pague 2" />
        </Field>
        <Field label="Tipo">
          <Select value={r.tipo} onChange={(e) => set({ tipo: e.target.value as Regra["tipo"] })}>
            {Object.entries(TIPOS).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Aplica-se a">
          <Select value={alvo} onChange={(e) => setAlvo(e.target.value as typeof alvo)}>
            <option value="artigo">Um artigo</option>
            <option value="familia">Uma família</option>
            <option value="todos">Todos os artigos</option>
          </Select>
        </Field>
        {alvo === "artigo" && (
          <Field label="Artigo" className="col-span-2">
            {artigo ? (
              <div className="flex items-center gap-2">
                <span className="font-medium">{artigo.descricao}</span>
                <Button size="sm" variant="ghost" onClick={() => setArtigo(null)}>
                  Trocar
                </Button>
              </div>
            ) : (
              <ArtigoPicker onPick={(a: Artigo) => setArtigo({ id: a.id, descricao: a.descricao })} />
            )}
          </Field>
        )}
        {alvo === "familia" && (
          <Field label="Família" className="col-span-2">
            <Select value={r.familia_id ?? ""} onChange={(e) => set({ familia_id: Number(e.target.value) || null })}>
              <option value="">—</option>
              {familias.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.descricao}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {r.tipo === "LEVE_PAGUE" && (
          <>
            <Field label="Leve (unidades)">
              <Input className="num" type="number" min={1} step="any" value={r.leve ?? ""} onChange={(e) => set({ leve: Number(e.target.value) })} />
            </Field>
            <Field label="Pague (unidades)">
              <Input className="num" type="number" min={0} step="any" value={r.pague ?? ""} onChange={(e) => set({ pague: Number(e.target.value) })} />
            </Field>
          </>
        )}
        {r.tipo === "QUANTIDADE" && (
          <Field label="A partir de (unidades)">
            <Input className="num" type="number" min={0} step="any" value={r.quantidade_min ?? ""} onChange={(e) => set({ quantidade_min: Number(e.target.value) })} />
          </Field>
        )}
        {r.tipo === "HORARIO" && (
          <>
            <Field label="Das (HH:MM)">
              <Input className="num" value={r.hora_inicio ?? ""} onChange={(e) => set({ hora_inicio: e.target.value })} placeholder="17:00" />
            </Field>
            <Field label="Às (HH:MM)">
              <Input className="num" value={r.hora_fim ?? ""} onChange={(e) => set({ hora_fim: e.target.value })} placeholder="19:00" />
            </Field>
            <div className="col-span-2 flex flex-wrap gap-3 text-[13px]">
              {DIAS.map((nome, i) => (
                <label key={nome} className="flex items-center gap-1">
                  <input
                    type="checkbox"
                    checked={diaMarcado(i + 1)}
                    onChange={(e) => {
                      const atuais = new Set((r.dias_semana ?? "").split("").filter(Boolean));
                      if (e.target.checked) atuais.add(String(i + 1));
                      else atuais.delete(String(i + 1));
                      set({ dias_semana: [...atuais].sort().join("") });
                    }}
                  />
                  {nome}
                </label>
              ))}
              <span className="text-ink-3">(nenhum = todos os dias)</span>
            </div>
          </>
        )}
        {r.tipo !== "LEVE_PAGUE" && (
          <Field label="Desconto (%)">
            <Input className="num" type="number" min={0} max={100} step="any" value={r.desconto_pct ?? ""} onChange={(e) => set({ desconto_pct: Number(e.target.value) })} />
          </Field>
        )}
        <Field label="Desde (AAAA-MM-DD)">
          <Input value={r.data_inicio ?? ""} onChange={(e) => set({ data_inicio: e.target.value })} placeholder="vazio = já" />
        </Field>
        <Field label="Até (AAAA-MM-DD)">
          <Input value={r.data_fim ?? ""} onChange={(e) => set({ data_fim: e.target.value })} placeholder="vazio = sem fim" />
        </Field>
        <label className="col-span-2 flex items-center gap-2">
          <input type="checkbox" checked={!!r.ativo} onChange={(e) => set({ ativo: e.target.checked ? 1 : 0 })} /> Activa
        </label>
      </div>
    </Modal>
  );
}

function CupaoModal({ inicial, onClose, onDone }: { inicial: Partial<Cupao>; onClose: () => void; onDone: () => void }) {
  const [c, setC] = useState<Partial<Cupao>>(inicial);
  const [valorKz, setValorKz] = useState(inicial.tipo === "VALOR" ? toInputKz(inicial.valor ?? 0) : "");
  const [minimo, setMinimo] = useState(toInputKz(inicial.valor_minimo ?? 0));
  const [busy, setBusy] = useState(false);
  const set = (p: Partial<Cupao>) => setC((x) => ({ ...x, ...p }));
  async function gravar() {
    setBusy(true);
    try {
      await call("promocoes.cupaoGuardar", {
        ...c,
        valor: c.tipo === "VALOR" ? parseKz(valorKz) : c.valor,
        valor_minimo: parseKz(minimo || "0"),
        utilizacoes_max: c.utilizacoes_max || null,
        ativo: !!c.ativo,
      });
      toast.success("Cupão gravado.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal open onClose={onClose} title={c.id ? "Editar cupão" : "Novo cupão"} width="max-w-lg" footer={<Button loading={busy} onClick={gravar}>Gravar</Button>}>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Código">
          <Input autoFocus className="font-mono uppercase" value={c.codigo ?? ""} onChange={(e) => set({ codigo: e.target.value.toUpperCase() })} placeholder="NATAL10" />
        </Field>
        <Field label="Descrição">
          <Input value={c.descricao ?? ""} onChange={(e) => set({ descricao: e.target.value })} />
        </Field>
        <Field label="Desconto">
          <Select value={c.tipo} onChange={(e) => set({ tipo: e.target.value as Cupao["tipo"] })}>
            <option value="PCT">Percentagem</option>
            <option value="VALOR">Valor em Kz</option>
          </Select>
        </Field>
        {c.tipo === "PCT" ? (
          <Field label="Desconto (%)">
            <Input className="num" type="number" min={0} max={100} step="any" value={c.valor ?? ""} onChange={(e) => set({ valor: Number(e.target.value) })} />
          </Field>
        ) : (
          <Field label="Desconto (Kz, com IVA)">
            <Input className="num text-right" value={valorKz} onChange={(e) => setValorKz(e.target.value)} />
          </Field>
        )}
        <Field label="Compra mínima (Kz)">
          <Input className="num text-right" value={minimo} onChange={(e) => setMinimo(e.target.value)} />
        </Field>
        <Field label="Utilizações (vazio = sem limite)">
          <Input className="num" type="number" min={0} value={c.utilizacoes_max ?? ""} onChange={(e) => set({ utilizacoes_max: Number(e.target.value) || null })} />
        </Field>
        <Field label="Desde (AAAA-MM-DD)">
          <Input value={c.data_inicio ?? ""} onChange={(e) => set({ data_inicio: e.target.value })} />
        </Field>
        <Field label="Até (AAAA-MM-DD)">
          <Input value={c.data_fim ?? ""} onChange={(e) => set({ data_fim: e.target.value })} />
        </Field>
        <label className="col-span-2 flex items-center gap-2">
          <input type="checkbox" checked={!!c.ativo} onChange={(e) => set({ ativo: e.target.checked ? 1 : 0 })} /> Activo
        </label>
      </div>
    </Modal>
  );
}

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { ArrowDownLeft, ArrowUpRight, History, Lock, Printer, ShoppingCart, Wallet } from "lucide-react";
import { call } from "@/lib/api";
import { useTabelas, useTalao58 } from "@/lib/hooks";
import { dataHora, kz, parseKz, qtd } from "@/lib/format";
import { imprimir } from "@/lib/print";
import type { CaixaResumo } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Stat, Table, Td, Th, cn } from "@/components/ui";
import { PrintPortal } from "@/components/DocumentoPrint";
import { useEmpresa } from "@/components/PrintCabecalho";

interface SessaoLista {
  id: number;
  posto: string | null;
  utilizador: string;
  aberta_em: string;
  fechada_em: string | null;
  valor_inicial: number;
  valor_contado: number | null;
  estado: string;
  total_vendido: number;
}

export default function Caixa() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const tem = useSession((s) => s.tem);
  const { data: c, refetch } = useQuery({ queryKey: ["caixa"], queryFn: () => call<CaixaResumo | null>("caixa.estado") });
  const [mov, setMov] = useState(false);
  const [fechar, setFechar] = useState(false);
  const [relatorio, setRelatorio] = useState<CaixaResumo | null>(null);
  const [historico, setHistorico] = useState(false);
  const [inicial, setInicial] = useState("0,00");

  async function abrir() {
    try {
      await call("caixa.abrir", { valor_inicial: parseKz(inicial) });
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const acoesTopo = (
    <Button variant="secondary" onClick={() => setHistorico(true)}>
      <History className="size-4" /> Fechos anteriores
    </Button>
  );

  return (
    <>
      {!c ? (
        <>
          <PageHeader title="Caixa" subtitle="Abertura, movimentos e fecho de caixa" actions={acoesTopo} />
          <Card className="mx-auto max-w-lg">
            <Empty
              icon={<Wallet className="size-6" />}
              title="Caixa fechado"
              text="Abra o caixa com o fundo de maneio para começar a vender."
              action={
                <div className="flex items-end gap-2">
                  <Field label="Fundo de maneio">
                    <Input className="num w-40 text-right" value={inicial} onChange={(e) => setInicial(e.target.value)} />
                  </Field>
                  <Button onClick={abrir}>Abrir caixa</Button>
                </div>
              }
            />
          </Card>
        </>
      ) : (
        <>
          <PageHeader
            title="Caixa"
            subtitle={`${c.sessao.posto ?? "Posto"} · aberto por ${c.sessao.utilizador} em ${dataHora(c.sessao.aberta_em)}`}
            actions={
              <>
                {acoesTopo}
                {tem("caixa.movimentos") && (
                  <Button variant="secondary" onClick={() => setMov(true)}>
                    Entrada / Saída de dinheiro
                  </Button>
                )}
                <Button variant="secondary" onClick={() => nav("/pos")}>
                  <ShoppingCart className="size-4" /> POS
                </Button>
                {tem("pos.fecho_caixa") && (
                  <Button variant="danger" onClick={() => setFechar(true)}>
                    <Lock className="size-4" /> Fechar caixa
                  </Button>
                )}
              </>
            }
          />
          <div className="grid grid-cols-4 gap-4">
            <Stat label="Fundo de maneio" value={kz(c.sessao.valor_inicial)} />
            <Stat label="Total recebido" value={kz(c.total_vendido)} hint={`${c.documentos} documentos`} />
            <Stat label="Entradas / Saídas" value={kz(c.entradas - c.saidas)} hint={`+${kz(c.entradas)} · −${kz(c.saidas)}`} tone="accent" />
            <Stat label="Numerário esperado na gaveta" value={kz(c.numerario_esperado)} tone="ok" icon={<Wallet className="size-4" />} />
          </div>
          <div className="mt-4 grid grid-cols-2 gap-4">
            <Card>
              <div className="px-5 pt-4 font-semibold">Recebimentos por modo de pagamento</div>
              <Table>
                <tbody>
                  {c.por_modo.map((m) => (
                    <tr key={m.codigo}>
                      <Td>{m.descricao}</Td>
                      <Td className="num text-right font-medium">{kz(m.total)}</Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </Card>
            <Card>
              <div className="px-5 pt-4 font-semibold">Movimentos de caixa</div>
              <Table>
                <tbody>
                  {c.movimentos.map((m) => (
                    <tr key={m.id}>
                      <Td className="text-ink-2">{dataHora(m.data_hora)}</Td>
                      <Td>
                        <span className="inline-flex items-center gap-1.5">
                          {m.tipo === "ENTRADA" ? <ArrowDownLeft className="size-4 text-ok" /> : <ArrowUpRight className="size-4 text-danger" />}
                          {m.descricao}
                        </span>
                        {m.rubrica && <span className="ml-2 text-[12px] text-ink-3">{m.rubrica}</span>}
                      </Td>
                      <Td className={cn("num text-right", m.tipo === "ENTRADA" ? "text-ok" : "text-danger")}>
                        {m.tipo === "ENTRADA" ? "+" : "−"}
                        {kz(m.valor)}
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
              {!c.movimentos.length && <p className="py-8 text-center text-ink-3">Sem movimentos.</p>}
            </Card>
          </div>
        </>
      )}

      {mov && <Movimento onClose={() => setMov(false)} onDone={() => { setMov(false); refetch(); }} />}
      {fechar && c && (
        <Fechar
          esperado={c.numerario_esperado}
          onClose={() => setFechar(false)}
          onDone={(r) => {
            setFechar(false);
            setRelatorio(r);
            qc.setQueryData(["caixa"], null);
          }}
        />
      )}
      {historico && (
        <Historico
          onClose={() => setHistorico(false)}
          onAbrir={async (id) => {
            try {
              setRelatorio(await call<CaixaResumo>("caixa.obter", { id }));
            } catch (e) {
              toast.error((e as Error).message);
            }
          }}
        />
      )}
      {relatorio && <RelatorioFecho r={relatorio} onClose={() => setRelatorio(null)} />}
    </>
  );
}

function Movimento({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const modos = useTabelas().data?.modos_pagamento ?? [];
  const [tipo, setTipo] = useState("SAIDA");
  const [valor, setValor] = useState("");
  const [desc, setDesc] = useState("");
  const [rubrica, setRubrica] = useState("");
  const [modo, setModo] = useState("NU");
  async function guardar() {
    try {
      await call("caixa.movimento", { tipo, valor: parseKz(valor), descricao: desc, rubrica, modo });
      toast.success("Movimento registado.");
      onDone();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title="Movimento de caixa" width="max-w-md" footer={<Button onClick={guardar}>Registar</Button>}>
      <div className="grid grid-cols-2 gap-4">
        <Field label="Tipo" className="col-span-2">
          <Select value={tipo} onChange={(e) => setTipo(e.target.value)}>
            <option value="SAIDA">Saída (sangria, pagamento, regularização)</option>
            <option value="ENTRADA">Entrada (inserir dinheiro / reforço)</option>
          </Select>
        </Field>
        <Field label="Valor (Kz)">
          <Input autoFocus className="num text-right" value={valor} onChange={(e) => setValor(e.target.value)} />
        </Field>
        <Field label="Método">
          <Select value={modo} onChange={(e) => setModo(e.target.value)}>
            {modos.map((m) => (
              <option key={m.codigo} value={m.codigo}>
                {m.descricao}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Descrição" className="col-span-2">
          <Input value={desc} onChange={(e) => setDesc(e.target.value)} />
        </Field>
        <Field label="Rubrica" hint="Ex.: Combustível, Água, Reforço de trocos" className="col-span-2">
          <Input value={rubrica} onChange={(e) => setRubrica(e.target.value)} />
        </Field>
      </div>
    </Modal>
  );
}

function Fechar({ esperado, onClose, onDone }: { esperado: number; onClose: () => void; onDone: (r: CaixaResumo) => void }) {
  const [contado, setContado] = useState("");
  const dif = parseKz(contado || "0") - esperado;
  async function fechar() {
    try {
      const r = await call<CaixaResumo>("caixa.fechar", { valor_contado: parseKz(contado || "0") });
      toast.success("Caixa fechado.");
      onDone(r);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title="Fecho de caixa" width="max-w-md" footer={<Button variant="danger" onClick={fechar}>Finalizar fecho</Button>}>
      <p className="mb-4 text-ink-2">Conte o numerário na gaveta e indique o valor físico. O sistema compara com o total do sistema.</p>
      <Field label="Valor físico (Kz)">
        <Input autoFocus className="num text-right text-lg" value={contado} onChange={(e) => setContado(e.target.value)} />
      </Field>
      <div className="mt-4 space-y-1 rounded-xl bg-sunken p-4">
        <div className="flex justify-between">
          <span>Total do sistema (numerário)</span>
          <span className="num">{kz(esperado)}</span>
        </div>
        <div className={cn("flex justify-between font-medium", dif < 0 ? "text-danger" : dif > 0 ? "text-[#9a6514]" : "text-ok")}>
          <span>Diferença</span>
          <span className="num">{kz(dif)}</span>
        </div>
      </div>
    </Modal>
  );
}

function Historico({ onClose, onAbrir }: { onClose: () => void; onAbrir: (id: number) => void }) {
  const { data } = useQuery({ queryKey: ["caixa-sessoes"], queryFn: () => call<SessaoLista[]>("caixa.sessoes") });
  return (
    <Modal open onClose={onClose} title="Fechos de caixa" width="max-w-4xl">
      <Table className="max-h-[60vh]">
        <thead>
          <tr>
            <Th>Posto</Th>
            <Th>Operador</Th>
            <Th>Abertura</Th>
            <Th>Fecho</Th>
            <Th className="text-right">Vendido</Th>
            <Th className="text-right">Contado</Th>
            <Th />
          </tr>
        </thead>
        <tbody>
          {data?.map((s) => (
            <tr key={s.id}>
              <Td>{s.posto ?? "—"}</Td>
              <Td>{s.utilizador}</Td>
              <Td className="text-ink-2">{dataHora(s.aberta_em)}</Td>
              <Td className="text-ink-2">{s.estado === "ABERTA" ? <Badge tone="ok">Aberto</Badge> : dataHora(s.fechada_em)}</Td>
              <Td className="num text-right">{kz(s.total_vendido)}</Td>
              <Td className="num text-right">{s.valor_contado === null ? "—" : kz(s.valor_contado)}</Td>
              <Td>
                <Button size="sm" variant="ghost" onClick={() => onAbrir(s.id)}>
                  <Printer className="size-4" /> Ver
                </Button>
              </Td>
            </tr>
          ))}
        </tbody>
      </Table>
      {!data?.length && <p className="py-8 text-center text-ink-3">Ainda não há fechos de caixa.</p>}
    </Modal>
  );
}

function RelatorioFecho({ r, onClose }: { r: CaixaResumo; onClose: () => void }) {
  const dif = (r.sessao.valor_contado ?? 0) - r.numerario_esperado;
  const t58 = useTalao58();
  const e = useEmpresa();
  const tabelas = useTabelas();
  const armazem = tabelas.data?.armazens.find((a) => a.id === tabelas.data?.armazem_id);
  const linha = (k: string, v: string, forte = false) => (
    <div className={cn("flex justify-between gap-2", forte && "font-bold")}>
      <span>{k}</span>
      <span>{v}</span>
    </div>
  );
  // desenho do FechoCaixa.fr3 do KIGEST POS: empresa, n.º do fecho, datas, armazém, posto e vendedor,
  // valores por forma de pagamento, total, valor físico e diferença
  const conteudo = (
    <div
      className={cn("print-ticket mx-auto bg-white text-black", t58 ? "talao-58 w-[220px] p-2 text-[9.5px] leading-[1.3]" : "w-[300px] p-3 text-[11px] leading-[1.35]")}
      style={{ fontFamily: "Arial, Helvetica, sans-serif" }}
    >
      {e && (
        <div className="text-center">
          <div className="text-[13px] font-bold">{e.nome}</div>
          {e.designacao_comercial && e.designacao_comercial !== e.nome && <div>{e.designacao_comercial}</div>}
          {e.endereco && <div>{e.endereco}</div>}
          {e.telefone && <div>Telf.: {e.telefone}</div>}
          <div>{e.localidade || e.provincia || "Luanda"} - Angola</div>
        </div>
      )}
      <div className="mt-2 text-[12.5px] font-bold">FECHO DE CAIXA Nº - {r.sessao.id}</div>
      {linha("Data Abertura:", dataHora(r.sessao.aberta_em))}
      {linha("Data de Fecho:", r.sessao.fechada_em ? dataHora(r.sessao.fechada_em) : "(em aberto)")}
      {armazem && linha("Armazém:", armazem.nome)}
      {linha("Posto:", r.sessao.posto ?? "—")}
      {linha("Vendedor:", r.sessao.utilizador)}
      <div className="mt-2 flex justify-between border-y border-black py-0.5 font-bold">
        <span>Descrição Pagamento</span>
        <span>Valor</span>
      </div>
      {r.por_modo.map((m) => (
        <div key={m.codigo}>{linha(m.descricao, kz(m.total))}</div>
      ))}
      <div className="mt-1 border-t border-black pt-1">
        {linha(`Total de ${r.documentos} documento(s):`, kz(r.total_vendido), true)}
        {linha("Fundo de maneio", kz(r.sessao.valor_inicial))}
        {linha("Entradas", kz(r.entradas))}
        {linha("Saídas", kz(r.saidas))}
        {linha("Numerário esperado:", kz(r.numerario_esperado), true)}
        {linha("Valor Físico:", r.sessao.valor_contado === null ? "—" : kz(r.sessao.valor_contado), true)}
        {r.sessao.valor_contado !== null && linha(dif < 0 ? "Diferença (falta):" : dif > 0 ? "Diferença (sobra):" : "Diferença:", kz(dif), true)}
      </div>
      {r.artigos.length > 0 && (
        <>
          <div className="mt-2 border-y border-black py-0.5 text-center">Artigos vendidos</div>
          {r.artigos.map((a) => (
            <div key={a.referencia} className="flex justify-between gap-2">
              <span className="truncate">
                {qtd(a.quantidade)} x {a.descricao}
              </span>
              <span>{kz(a.total)}</span>
            </div>
          ))}
        </>
      )}
      {e?.rodape && <div className="mt-2 text-center">{e.rodape}</div>}
    </div>
  );
  return (
    <Modal open onClose={onClose} title="Relatório de fecho" width="max-w-md" footer={<Button onClick={imprimir}><Printer className="size-4" /> Imprimir</Button>}>
      <div className="flex max-h-[60vh] justify-center overflow-y-auto rounded-xl bg-sunken p-4">{conteudo}</div>
      {r.sessao.valor_contado !== null && dif !== 0 && <Badge tone={dif < 0 ? "danger" : "accent"} className="mt-3">Diferença de {kz(dif)}</Badge>}
      <PrintPortal>{conteudo}</PrintPortal>
    </Modal>
  );
}

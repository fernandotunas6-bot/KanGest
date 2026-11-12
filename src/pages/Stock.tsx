import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { ArrowDownToLine, ArrowRightLeft, ArrowUpFromLine, Boxes, History, Search } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { dataHora, kz, qtd } from "@/lib/format";
import { Badge, Button, Card, Empty, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface LinhaStock {
  id: number;
  referencia: string;
  descricao: string;
  unidade: string | null;
  qtd_minima: number;
  qtd_maxima: number;
  custo_medio: number;
  quantidade: number;
  valor_custo: number;
  ultima_entrada: string | null;
  ultima_saida: string | null;
  /** Notas de encomenda de clientes por facturar / encomendas a fornecedores por receber. */
  reservado: number;
  encomendado: number;
  disponivel: number;
}

export default function Stock() {
  const t = useTabelas().data;
  const nav = useNavigate();
  const [armazem, setArmazem] = useState(1);
  const [q, setQ] = useState("");
  const [baixo, setBaixo] = useState(false);
  const [hist, setHist] = useState<LinhaStock | null>(null);
  const dq = useDebounce(q);
  const lista = useQuery({
    queryKey: ["stock", armazem, dq, baixo],
    queryFn: () => call<LinhaStock[]>("stock.listar", { armazem_id: armazem, q: dq, abaixo_minimo: baixo }),
  });
  const valor = lista.data?.reduce((s, l) => s + l.valor_custo, 0) ?? 0;
  // as colunas de encomendas só aparecem quando há encomendas pendentes
  const temReservas = !!lista.data?.some((l) => l.reservado);
  const temEncomendas = !!lista.data?.some((l) => l.encomendado);

  return (
    <>
      <PageHeader
        title="Stock"
        subtitle="Existências por armazém, valorizadas ao custo médio"
        actions={
          <>
            <Button variant="secondary" onClick={() => nav("/documentos/novo/ES")}>
              <ArrowDownToLine className="size-4" /> Entrada
            </Button>
            <Button variant="secondary" onClick={() => nav("/documentos/novo/SS")}>
              <ArrowUpFromLine className="size-4" /> Saída
            </Button>
            <Button variant="secondary" onClick={() => nav("/documentos/novo/TS")}>
              <ArrowRightLeft className="size-4" /> Transferência
            </Button>
          </>
        }
      />
      <Card>
        <div className="flex flex-wrap items-center gap-3 border-b border-line p-4">
          <div className="relative min-w-64 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Pesquisar artigo…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Select className="w-48" value={armazem} onChange={(e) => setArmazem(Number(e.target.value))}>
            {t?.armazens.map((a) => (
              <option key={a.id} value={a.id}>
                {a.nome}
              </option>
            ))}
          </Select>
          <label className="flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={baixo} onChange={(e) => setBaixo(e.target.checked)} /> Só abaixo do mínimo
          </label>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<Boxes className="size-6" />} title="Sem artigos" />
        ) : (
          <Table className="tabela-compacta max-h-[calc(100vh-300px)]">
            <thead>
              <tr>
                <Th>Artigo</Th>
                <Th className="text-right">Existência</Th>
                {temReservas && <Th className="text-right">Reservado</Th>}
                {temReservas && <Th className="text-right">Disponível</Th>}
                {temEncomendas && <Th className="text-right">Encomendado</Th>}
                <Th className="text-right">Mínimo</Th>
                <Th className="text-right">Custo médio</Th>
                <Th className="text-right">Valor</Th>
                <Th>Última entrada</Th>
                <Th>Última saída</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((l) => (
                <tr key={l.id} className="hover:bg-sunken/60">
                  <Td>
                    <div className="font-medium">{l.descricao}</div>
                    <div className="font-mono text-[11.5px] text-ink-3">{l.referencia}</div>
                  </Td>
                  <Td className={cn("num text-right font-medium", l.quantidade <= l.qtd_minima && "text-danger")}>
                    {qtd(l.quantidade)} {l.unidade}
                  </Td>
                  {temReservas && <Td className="num text-right text-ink-2">{l.reservado ? qtd(l.reservado) : "—"}</Td>}
                  {temReservas && <Td className={cn("num text-right", l.disponivel < 0 && "text-danger")}>{qtd(l.disponivel)}</Td>}
                  {temEncomendas && <Td className="num text-right text-ink-2">{l.encomendado ? qtd(l.encomendado) : "—"}</Td>}
                  <Td className="num text-right text-ink-2">{qtd(l.qtd_minima)}</Td>
                  <Td className="num text-right text-ink-2">{kz(l.custo_medio)}</Td>
                  <Td className="num text-right">{kz(l.valor_custo)}</Td>
                  <Td className="text-ink-2">{dataHora(l.ultima_entrada)}</Td>
                  <Td className="text-ink-2">{dataHora(l.ultima_saida)}</Td>
                  <Td>
                    <Button variant="ghost" size="sm" onClick={() => setHist(l)}>
                      <History className="size-4" /> Movimentos
                    </Button>
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
        <div className="flex justify-between border-t border-line px-5 py-3 text-[13px] text-ink-2">
          <span>{lista.data?.length ?? 0} artigos</span>
          <span>
            Valor do stock: <b className="num text-ink">{kz(valor)}</b>
          </span>
        </div>
      </Card>
      {hist && <Movimentos artigo={hist} armazem={armazem} onClose={() => setHist(null)} />}
    </>
  );
}

function Movimentos({ artigo, armazem, onClose }: { artigo: LinhaStock; armazem: number; onClose: () => void }) {
  const { data } = useQuery({
    queryKey: ["movimentos", artigo.id, armazem],
    queryFn: () =>
      call<{ id: number; data_hora: string; quantidade: number; saldo: number; custo: number; numero_doc: string | null; descricao: string; armazem: string }[]>(
        "stock.movimentos",
        { artigo_id: artigo.id, armazem_id: armazem },
      ),
  });
  return (
    <Modal open onClose={onClose} title={`Extracto de movimentos — ${artigo.descricao}`} width="max-w-4xl">
      <Table className="max-h-[60vh]">
        <thead>
          <tr>
            <Th>Data</Th>
            <Th>Documento</Th>
            <Th>Armazém</Th>
            <Th className="text-right">Entrada</Th>
            <Th className="text-right">Saída</Th>
            <Th className="text-right">Saldo</Th>
            <Th className="text-right">Custo médio</Th>
          </tr>
        </thead>
        <tbody>
          {data?.map((m) => (
            <tr key={m.id}>
              <Td className="text-ink-2">{dataHora(m.data_hora)}</Td>
              <Td>{m.numero_doc ?? <Badge>{m.descricao}</Badge>}</Td>
              <Td className="text-ink-2">{m.armazem}</Td>
              <Td className="num text-right text-ok">{m.quantidade > 0 ? qtd(m.quantidade) : ""}</Td>
              <Td className="num text-right text-danger">{m.quantidade < 0 ? qtd(-m.quantidade) : ""}</Td>
              <Td className="num text-right font-medium">{qtd(m.saldo)}</Td>
              <Td className="num text-right text-ink-2">{kz(m.custo)}</Td>
            </tr>
          ))}
        </tbody>
      </Table>
      {data?.length === 0 && <p className="py-8 text-center text-ink-3">Sem movimentos.</p>}
    </Modal>
  );
}

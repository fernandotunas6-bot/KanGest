import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, FileSpreadsheet, Info } from "lucide-react";
import { call } from "@/lib/api";
import { data, kz, qtd } from "@/lib/format";
import { dataXL, exportarExcel, kzXL, type CelulaXL } from "@/lib/csv";
import { Button, Field, Modal, Select, Stat, Table, Td, Th } from "./ui";

interface LinhaInventario {
  referencia: string;
  descricao: string;
  codigo_barras: string | null;
  tipo: string;
  unidade: string;
  unidade_desc: string | null;
  familia: string | null;
  quantidade: number;
  custo_medio: number;
  valor: number;
}

interface Inventario {
  ano: number;
  data_referencia: string;
  data_geracao: string;
  provisorio: boolean;
  empresa: { nif: string; nome: string };
  software: { nome: string; versao: string; certificado: string | null };
  metodo_custeio: string;
  sem_operacoes: boolean;
  total_artigos: number;
  total_valor: number;
  linhas: LinhaInventario[];
  negativos: { referencia: string; descricao: string; quantidade: number }[];
}

/**
 * SAF-T de inventários da AGT (Decreto Presidencial n.º 71/25; estrutura na Circular n.º 05/GACA/GJ/DSF/AGT/2026):
 * as existências a 31/12 de cada artigo, valorizadas ao custo médio ponderado, num livro Excel para o Portal do
 * Contribuinte. Entregam todos os contribuintes, até 15 de Fevereiro; sem existências entrega-se «sem operações».
 */
export function InventarioSaft({ onClose }: { onClose: () => void }) {
  const hoje = new Date();
  // até ao fim de Março o que se prepara é o do exercício anterior (prazo: 15 de Fevereiro)
  const [ano, setAno] = useState(hoje.getMonth() < 3 ? hoje.getFullYear() - 1 : hoje.getFullYear());
  const anos = Array.from({ length: 5 }, (_, i) => hoje.getFullYear() - i);
  const q = useQuery({ queryKey: ["saft-inventario", ano], queryFn: () => call<Inventario>("saft.inventario", { ano }) });
  const inv = q.data;

  function exportar() {
    if (!inv) return;
    const cert = inv.software.certificado ? ` — programa validado n.º ${inv.software.certificado}` : "";
    const linhas: CelulaXL[][] = [
      ["NIF do contribuinte", inv.empresa.nif],
      ["Nome", inv.empresa.nome],
      ["Exercício (período de tributação)", inv.ano],
      ["Data de referência do inventário", dataXL(inv.data_referencia)],
      ["Data de geração do ficheiro", dataXL(inv.data_geracao)],
      ["Software", `${inv.software.nome} ${inv.software.versao}${cert}`],
      ["Método de custeio", inv.metodo_custeio],
      [],
      ["Identificador do produto", "Descrição", "Tipo de produto", "Método de custeio", "Quantidade", "Unidade de medida", "Valor"],
      ...(inv.sem_operacoes
        ? [["Sem operações", `Sem existências a ${data(inv.data_referencia)}`]]
        : inv.linhas.map((l): CelulaXL[] => [l.referencia, l.descricao, l.tipo, inv.metodo_custeio, l.quantidade, l.unidade, kzXL(l.valor)])),
      ...(inv.sem_operacoes ? [] : [["Total", "", "", "", "", "", kzXL(inv.total_valor)] as CelulaXL[]]),
    ];
    exportarExcel(`SAFT-Inventario-${inv.empresa.nif}-${inv.ano}.xlsx`, linhas, { titulo: `SAF-T de inventários ${inv.ano}`, folha: `Inventário ${inv.ano}`, linhaCabecalho: 9 });
  }

  return (
    <Modal
      open
      onClose={onClose}
      title="SAF-T de inventários"
      width="max-w-4xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Fechar
          </Button>
          <Button onClick={exportar} disabled={!inv}>
            <FileSpreadsheet className="size-4" /> Exportar Excel
          </Button>
        </>
      }
    >
      <div className="flex flex-wrap items-end gap-4">
        <Field label="Exercício" className="w-40">
          <Select value={String(ano)} onChange={(e) => setAno(Number(e.target.value))}>
            {anos.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </Select>
        </Field>
        <p className="flex-1 pb-2 text-[13px] text-ink-2">
          Existências a <b>31/12/{ano}</b>, valorizadas ao custo médio ponderado. Entrega no Portal do Contribuinte até <b>15 de Fevereiro de {ano + 1}</b>, mesmo sem existências.
        </p>
      </div>

      {inv?.provisorio && (
        <div className="mt-4 flex gap-2.5 rounded-lg border border-accent/40 bg-accent-50 px-4 py-3 text-[13px] text-accent-ink">
          <AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <span>O exercício de {ano} ainda não terminou: estas são as existências de hoje (inventário provisório). O ficheiro a entregar é o do exercício fechado.</span>
        </div>
      )}
      {!!inv?.negativos.length && (
        <div className="mt-3 flex gap-2.5 rounded-lg border border-danger/30 bg-danger-50 px-4 py-3 text-[13px] text-danger">
          <AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <span>
            {inv.negativos.length} {inv.negativos.length === 1 ? "artigo tinha" : "artigos tinham"} existência negativa e {inv.negativos.length === 1 ? "fica" : "ficam"} de fora (
            {inv.negativos
              .slice(0, 3)
              .map((n) => n.referencia)
              .join(", ")}
            {inv.negativos.length > 3 ? "…" : ""}). Corrija com um Inventário ou o Recálculo de Stock antes de entregar.
          </span>
        </div>
      )}

      <div className="mt-4 grid grid-cols-3 gap-3">
        <Stat label="Artigos com existências" value={inv ? inv.total_artigos : "…"} />
        <Stat label="Valor do inventário" value={inv ? kz(inv.total_valor) : "…"} />
        <Stat label="Método de custeio" value={<span className="text-[17px]">{inv?.metodo_custeio ?? "…"}</span>} />
      </div>

      <div className="mt-4 overflow-hidden rounded-xl border border-line">
        {inv?.sem_operacoes ? (
          <p className="py-8 text-center text-ink-3">Sem existências a {data(inv.data_referencia)}: o ficheiro sai como «sem operações».</p>
        ) : (
          <Table className="max-h-[300px]">
            <thead>
              <tr>
                <Th>Identificador</Th>
                <Th>Descrição</Th>
                <Th>Tipo</Th>
                <Th className="text-right">Quantidade</Th>
                <Th>Unidade</Th>
                <Th className="text-right">Valor</Th>
              </tr>
            </thead>
            <tbody>
              {inv?.linhas.map((l) => (
                <tr key={l.referencia}>
                  <Td className="font-mono text-[12.5px]">{l.referencia}</Td>
                  <Td>{l.descricao}</Td>
                  <Td className="text-ink-2">{l.tipo}</Td>
                  <Td className="num text-right">{qtd(l.quantidade)}</Td>
                  <Td className="text-ink-2">{l.unidade}</Td>
                  <Td className="num text-right">{kz(l.valor)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </div>

      <p className="mt-3 flex gap-2 text-[12.5px] text-ink-3">
        <Info className="mt-0.5 size-3.5 shrink-0" />
        Campos da Circular n.º 05/GACA/GJ/DSF/AGT/2026: NIF, período, data de geração, software certificado e, por produto, identificador, descrição, tipo, método de custeio, quantidade,
        unidade e valor. Antes de carregar, confirme os nomes das colunas no modelo do Portal do Contribuinte.
      </p>
    </Modal>
  );
}

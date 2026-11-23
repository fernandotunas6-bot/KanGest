// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Importar uma folha de cálculo (Excel, LibreOffice) ou um CSV: a lista de artigos, de clientes ou de fornecedores que
// qualquer loja tem. O servidor sugere a ligação das colunas pelos títulos; aqui confirma-se, vê-se como fica e importa-se.
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AlertTriangle, CheckCircle2, FolderOpen, Info, Sheet } from "lucide-react";
import { call } from "@/lib/api";
import { escolherFicheiro, useEscolherNoComputador } from "@/lib/dialogos";
import { numero } from "@/lib/format";
import { Button, Card, Field, Input, Modal, Segmentado, Select, Table, Td, Th } from "@/components/ui";

type Tipo = "artigos" | "clientes" | "fornecedores";
type Grupo = "artigos" | "entidades";
const grupo = (t: Tipo): Grupo => (t === "artigos" ? "artigos" : "entidades");

interface FolhaAnalise {
  nome: string;
  linha_titulos: number;
  cabecalho: string[];
  amostra: string[][];
  linhas: number;
  tipo: Tipo;
  colunas: Record<Grupo, Record<string, number>>;
}
interface AnaliseFolha {
  folhas: FolhaAnalise[];
  taxas: { id: number; descricao: string; percentagem: number; codigo: string }[];
  armazens: { id: number; nome: string }[];
}
interface RelatorioFolha {
  simulacao: boolean;
  tipo: Tipo;
  linhas: number;
  novos: number;
  existentes: number;
  actualizados?: number;
  ignoradas: number;
  familias_novas?: number;
  com_stock?: number;
  avisos: string[];
}

const CAMPOS: Record<Grupo, { id: string; label: string; obrigatorio?: boolean }[]> = {
  artigos: [
    { id: "descricao", label: "Descrição", obrigatorio: true },
    { id: "referencia", label: "Referência" },
    { id: "codigo_barras", label: "Código de barras" },
    { id: "preco", label: "Preço de venda" },
    { id: "custo", label: "Preço de custo" },
    { id: "iva", label: "Taxa de IVA (%)" },
    { id: "familia", label: "Família" },
    { id: "stock", label: "Stock inicial" },
  ],
  entidades: [
    { id: "nome", label: "Nome", obrigatorio: true },
    { id: "nif", label: "NIF" },
    { id: "telefone", label: "Telefone" },
    { id: "email", label: "Email" },
    { id: "endereco", label: "Morada" },
    { id: "localidade", label: "Localidade" },
  ],
};

const TIPOS: { id: Tipo; label: string }[] = [
  { id: "artigos", label: "Artigos" },
  { id: "clientes", label: "Clientes" },
  { id: "fornecedores", label: "Fornecedores" },
];

export function ImportarFolha() {
  const qc = useQueryClient();
  const nativo = useEscolherNoComputador();
  const [caminho, setCaminho] = useState("");
  const [analise, setAnalise] = useState<AnaliseFolha | null>(null);
  const [folha, setFolha] = useState<FolhaAnalise | null>(null);
  const [tipo, setTipo] = useState<Tipo>("artigos");
  const [colunas, setColunas] = useState<Record<string, number>>({});
  const [precosComIva, setPrecosComIva] = useState(true);
  const [taxa, setTaxa] = useState<number | "">("");
  const [armazem, setArmazem] = useState<number | "">("");
  const [actualizar, setActualizar] = useState(false);
  const [relatorio, setRelatorio] = useState<RelatorioFolha | null>(null);
  const [busy, setBusy] = useState<"" | "analisar" | "simular" | "importar">("");
  const [confirmar, setConfirmar] = useState(false);

  function limpar(c: string) {
    setCaminho(c);
    setAnalise(null);
    setFolha(null);
    setRelatorio(null);
  }

  function escolherFolha(f: FolhaAnalise, t: Tipo = f.tipo) {
    setFolha(f);
    setTipo(t);
    setColunas({ ...f.colunas[grupo(t)] });
    setRelatorio(null);
  }

  async function escolher() {
    const c = await escolherFicheiro("Escolher a folha de cálculo", [{ name: "Folhas de cálculo e CSV", extensions: ["xlsx", "xls", "xlsm", "ods", "csv", "txt"] }]);
    if (c) {
      limpar(c);
      await analisar(c);
    }
  }

  async function analisar(c = caminho) {
    setBusy("analisar");
    try {
      const a = await call<AnaliseFolha>("importar.analisarFolha", { caminho: c.trim() });
      setAnalise(a);
      setTaxa(a.taxas.find((t) => t.codigo === "NOR")?.id ?? a.taxas[0]?.id ?? "");
      setArmazem(a.armazens[0]?.id ?? "");
      // a folha com mais linhas é a que interessa, quase sempre
      escolherFolha(a.folhas.reduce((m, f) => (f.linhas > m.linhas ? f : m), a.folhas[0]));
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }

  async function executar(simular: boolean) {
    if (!folha) return;
    setBusy(simular ? "simular" : "importar");
    try {
      const r = await call<RelatorioFolha>("importar.executarFolha", {
        caminho: caminho.trim(),
        folha: folha.nome,
        tipo,
        colunas,
        simular,
        precos_com_iva: precosComIva,
        taxa_iva_id: taxa === "" ? null : taxa,
        armazem_id: armazem === "" ? null : armazem,
        actualizar_precos: actualizar,
      });
      setRelatorio(r);
      if (!simular) {
        toast.success("Folha importada.");
        qc.invalidateQueries();
      }
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
      setConfirmar(false);
    }
  }

  const campos = CAMPOS[grupo(tipo)];
  const ligados = campos.filter((c) => colunas[c.id] !== undefined);
  const falta = campos.find((c) => c.obrigatorio && colunas[c.id] === undefined);
  const titulo = (i: number) => folha?.cabecalho[i]?.trim() || `Coluna ${i + 1} (sem título)`;
  const n = (x: number | undefined) => numero(x ?? 0, 0);

  return (
    <>
      <Card className="p-6">
        <div className="mb-2 flex items-center gap-2 font-semibold">
          <Sheet className="size-5 text-brand" /> 1. Folha de cálculo ou CSV
        </div>
        <p className="mb-4 text-[13.5px] text-ink-2">
          A lista de artigos, de clientes ou de fornecedores numa folha do <b>Excel</b> (.xlsx, .xls), do <b>LibreOffice</b> (.ods) ou num <b>CSV</b> exportado de outro programa. A primeira linha com
          títulos diz o que é cada coluna; o KanGest reconhece os títulos habituais e pode corrigir a ligação antes de importar.
        </p>
        <Field label="Caminho do ficheiro">
          <div className="flex gap-2">
            <Input className="font-mono" value={caminho} placeholder="C:\Users\…\Documents\artigos.xlsx" onChange={(e) => limpar(e.target.value)} />
            {nativo && (
              <Button variant="secondary" onClick={escolher} disabled={!!busy}>
                <FolderOpen className="size-4" /> Escolher…
              </Button>
            )}
          </div>
        </Field>
        <div className="mt-4">
          <Button loading={busy === "analisar"} disabled={!caminho.trim() || !!busy} onClick={() => analisar()}>
            Analisar folha
          </Button>
        </div>
      </Card>

      {analise && folha && (
        <Card className="p-6">
          <div className="mb-3 font-semibold">2. O que a folha tem</div>
          <div className="grid gap-4 sm:grid-cols-2">
            {analise.folhas.length > 1 && (
              <Field label="Folha">
                <Select value={folha.nome} onChange={(e) => escolherFolha(analise.folhas.find((f) => f.nome === e.target.value) ?? folha)}>
                  {analise.folhas.map((f) => (
                    <option key={f.nome} value={f.nome}>
                      {f.nome} ({n(f.linhas)} linhas)
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            <Field label="A folha tem" hint={`${n(folha.linhas)} linhas, títulos na linha ${folha.linha_titulos + 1}`}>
              <Segmentado valor={tipo} opcoes={TIPOS} onChange={(t) => escolherFolha(folha, t)} />
            </Field>
          </div>

          <div className="mt-5 mb-2 text-[13px] font-semibold">Que coluna é cada campo</div>
          <div className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
            {campos.map((c) => (
              <Field key={c.id} label={c.obrigatorio ? `${c.label} *` : c.label}>
                <Select
                  value={colunas[c.id] ?? ""}
                  onChange={(e) => {
                    const v = e.target.value;
                    const novo = { ...colunas };
                    if (v === "") delete novo[c.id];
                    else novo[c.id] = Number(v);
                    setColunas(novo);
                    setRelatorio(null);
                  }}
                >
                  <option value="">— não importar —</option>
                  {folha.cabecalho.map((_, i) => (
                    <option key={i} value={i}>
                      {titulo(i)}
                    </option>
                  ))}
                </Select>
              </Field>
            ))}
          </div>

          {ligados.length > 0 && folha.amostra.length > 0 && (
            <>
              <div className="mt-5 mb-2 text-[13px] font-semibold">Como fica (primeiras linhas)</div>
              <div className="overflow-x-auto">
                <Table>
                  <thead>
                    <tr>
                      {ligados.map((c) => (
                        <Th key={c.id}>{c.label}</Th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {folha.amostra.slice(0, 5).map((l, i) => (
                      <tr key={i}>
                        {ligados.map((c) => (
                          <Td key={c.id} className="max-w-[220px] truncate">
                            {l[colunas[c.id]] ?? ""}
                          </Td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </div>
            </>
          )}

          {tipo === "artigos" && (
            <div className="mt-5 space-y-3">
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="IVA quando a folha não o diz">
                  <Select value={taxa} onChange={(e) => setTaxa(e.target.value === "" ? "" : Number(e.target.value))}>
                    {analise.taxas.map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.descricao} ({numero(t.percentagem, 0)}%)
                      </option>
                    ))}
                  </Select>
                </Field>
                {colunas.stock !== undefined && analise.armazens.length > 1 && (
                  <Field label="Armazém do stock inicial">
                    <Select value={armazem} onChange={(e) => setArmazem(e.target.value === "" ? "" : Number(e.target.value))}>
                      {analise.armazens.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.nome}
                        </option>
                      ))}
                    </Select>
                  </Field>
                )}
              </div>
              <label className="flex items-start gap-2.5 text-[13.5px]">
                <input type="checkbox" className="mt-1" checked={precosComIva} onChange={(e) => setPrecosComIva(e.target.checked)} />
                <span>
                  Os preços da folha já incluem o IVA
                  <span className="block text-[12px] text-ink-3">O preço da etiqueta (PVP). Desligue se a folha tem os preços sem IVA.</span>
                </span>
              </label>
              <label className="flex items-start gap-2.5 text-[13.5px]">
                <input type="checkbox" className="mt-1" checked={actualizar} onChange={(e) => setActualizar(e.target.checked)} />
                <span>
                  Actualizar os preços dos artigos que já existem
                  <span className="block text-[12px] text-ink-3">Para uma tabela de preços nova. Sem isto, num artigo que já existe só se preenche o que lhe falta.</span>
                </span>
              </label>
              {colunas.stock !== undefined && (
                <p className="flex gap-2 text-[12.5px] text-ink-3">
                  <Info className="mt-0.5 size-4 shrink-0" /> O stock entra só nos artigos novos, como saldo inicial. O dos que já existem acerta-se por um inventário.
                </p>
              )}
            </div>
          )}

          <div className="mt-5 flex flex-wrap items-center gap-2">
            <Button variant="secondary" loading={busy === "simular"} disabled={!!busy || !!falta} onClick={() => executar(true)} title="Corre a importação e desfaz tudo no fim">
              Simular
            </Button>
            <Button loading={busy === "importar"} disabled={!!busy || !!falta} onClick={() => setConfirmar(true)}>
              Importar
            </Button>
            {falta && <span className="text-[12.5px] text-accent-ink">Diga qual é a coluna «{falta.label}».</span>}
          </div>
        </Card>
      )}

      {relatorio && (
        <Card className="p-6">
          <div className="mb-3 flex items-center gap-2 font-semibold">
            {relatorio.simulacao ? <Info className="size-5 text-brand" /> : <CheckCircle2 className="size-5 text-ok" />}
            {relatorio.simulacao ? "Simulação: nada foi gravado" : "Importação concluída"}
          </div>
          <Table>
            <tbody>
              {(
                [
                  ["Linhas com dados", relatorio.linhas],
                  [`Novos (${TIPOS.find((t) => t.id === relatorio.tipo)?.label.toLowerCase()})`, relatorio.novos],
                  ["Já existiam", relatorio.existentes],
                  ["… dos quais actualizados", relatorio.actualizados],
                  ["Famílias novas", relatorio.familias_novas],
                  ["Com stock inicial", relatorio.com_stock],
                  ["Linhas de fora", relatorio.ignoradas],
                ] as const
              )
                .filter(([, v]) => v !== undefined)
                .map(([rotulo, v]) => (
                  <tr key={rotulo}>
                    <Td className="font-medium">{rotulo}</Td>
                    <Td className="num text-right">{n(v)}</Td>
                  </tr>
                ))}
            </tbody>
          </Table>
          {relatorio.avisos.length > 0 && (
            <ul className="mt-4 space-y-1.5 text-[13px] text-accent-ink">
              {relatorio.avisos.map((a) => (
                <li key={a} className="flex gap-2">
                  <AlertTriangle className="mt-0.5 size-4 shrink-0" /> {a}
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}

      <Modal
        open={confirmar}
        onClose={() => setConfirmar(false)}
        title="Importar a folha"
        width="max-w-md"
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirmar(false)}>
              Cancelar
            </Button>
            <Button loading={busy === "importar"} onClick={() => executar(false)}>
              Importar
            </Button>
          </>
        }
      >
        <p className="text-[13.5px] text-ink-2">
          {tipo === "artigos"
            ? "Os artigos da folha passam para o KanGest. Os que já existem (mesma referência, código de barras ou descrição) não se duplicam."
            : `Os ${tipo} da folha passam para o KanGest. Os que já existem (mesmo NIF ou mesmo nome) não se duplicam.`}{" "}
          Faça antes uma cópia de segurança, se ainda não fez.
        </p>
      </Modal>
    </>
  );
}

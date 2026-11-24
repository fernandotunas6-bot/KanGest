// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Qualidade dos dados: corre as verificações do motor central (dados.verificar: NIF e códigos de barras repetidos,
// isentos sem motivo, artigos sem preço ou abaixo do custo, stock negativo ou sem custo, documentos vencidos) e mostra,
// em cada uma, quantos casos há, alguns exemplos e o ecrã onde se corrige. Só lê: as correcções fazem-se nos ecrãs de
// sempre, com as permissões e a auditoria deles. Cada utilizador vê as verificações das suas permissões (e da sua loja).
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { AlertTriangle, ArrowRight, CheckCircle2, ListChecks, RefreshCw, ShieldCheck, XCircle } from "lucide-react";
import { call } from "@/lib/api";
import { data, kz, numero, qtd } from "@/lib/format";
import { Badge, BlocoCarregando, Button, Card, Empty, PageHeader, cn } from "@/components/ui";

interface Verificacao {
  id: string;
  titulo: string;
  /** porque importa e como se corrige */
  texto: string;
  gravidade: "erro" | "aviso";
  casos: number;
  exemplos: { rotulo: string; detalhe: string | null }[];
  /** o ecrã onde se corrige */
  corrigir: string;
}

/** Exemplos à vista antes de «Mostrar mais». */
const VISIVEIS = 5;

const casos = (n: number) => `${numero(n, 0)} ${n === 1 ? "caso" : "casos"}`;

/** O detalhe de um exemplo com os valores no formato do programa: {kz:cêntimos}, {qtd:número}, {data:AAAA-MM-DD}. */
const detalhe = (t: string) =>
  t
    .replace(/\{kz:(-?\d+(?:\.\d+)?)\}/g, (_, v: string) => kz(Number(v)))
    .replace(/\{qtd:(-?\d+(?:\.\d+)?)\}/g, (_, v: string) => qtd(Number(v)))
    .replace(/\{data:(\d{4}-\d{2}-\d{2})\}/g, (_, d: string) => data(d));

export default function QualidadeDados() {
  const navigate = useNavigate();
  const q = useQuery({ queryKey: ["dados-verificar"], queryFn: () => call<Verificacao[]>("dados.verificar") });
  const [abertas, setAbertas] = useState<Set<string>>(new Set());
  const lista = q.data ?? [];
  // primeiro os erros, depois os avisos, cada grupo com mais casos primeiro
  const problemas = lista.filter((v) => v.casos > 0).sort((a, b) => (a.gravidade === b.gravidade ? b.casos - a.casos : a.gravidade === "erro" ? -1 : 1));
  const emOrdem = lista.filter((v) => v.casos === 0);
  const erros = problemas.filter((v) => v.gravidade === "erro").length;

  function alternar(id: string) {
    setAbertas((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader
        title="Qualidade dos dados"
        subtitle="O que está repetido, incompleto ou incoerente nos dados, e onde se corrige."
        actions={
          <Button variant="secondary" onClick={() => q.refetch()} loading={q.isFetching && !q.isLoading}>
            {!(q.isFetching && !q.isLoading) && <RefreshCw className="size-4" />}
            Verificar de novo
          </Button>
        }
      />

      {q.isLoading ? (
        <Card>
          <BlocoCarregando linhas={6} />
        </Card>
      ) : q.isError ? (
        <Card>
          <Empty icon={<XCircle className="size-6" />} title="Não foi possível verificar" text={String((q.error as Error)?.message ?? q.error)} />
        </Card>
      ) : lista.length === 0 ? (
        <Card>
          <Empty icon={<ListChecks className="size-6" />} title="Sem verificações para o seu perfil" text="As verificações aparecem conforme as permissões (artigos, clientes, inventário)." />
        </Card>
      ) : (
        <div className="space-y-4">
          {/* resumo: erros, avisos e verificações em ordem */}
          <div className="grid gap-3 sm:grid-cols-3">
            <Resumo icon={XCircle} tom="danger" valor={erros} rotulo={erros === 1 ? "erro a corrigir" : "erros a corrigir"} />
            <Resumo icon={AlertTriangle} tom="accent" valor={problemas.length - erros} rotulo={problemas.length - erros === 1 ? "aviso" : "avisos"} />
            <Resumo icon={CheckCircle2} tom="ok" valor={emOrdem.length} rotulo={`de ${lista.length} em ordem`} />
          </div>

          {problemas.length === 0 && (
            <Card className="flex items-center gap-3 p-5">
              <ShieldCheck className="size-6 shrink-0 text-ok" />
              <div>
                <p className="font-medium text-ink">Tudo em ordem</p>
                <p className="text-ink-3">Nenhuma verificação encontrou problemas nos dados.</p>
              </div>
            </Card>
          )}

          {problemas.map((v) => {
            const erro = v.gravidade === "erro";
            const Icone = erro ? XCircle : AlertTriangle;
            const aberta = abertas.has(v.id);
            const exemplos = aberta ? v.exemplos : v.exemplos.slice(0, VISIVEIS);
            return (
              <Card key={v.id} className="p-5">
                <div className="flex flex-wrap items-start gap-3 sm:flex-nowrap">
                  <Icone className={cn("mt-0.5 size-5 shrink-0", erro ? "text-danger" : "text-accent")} />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <h2 className="font-semibold text-ink">{v.titulo}</h2>
                      <Badge tone={erro ? "danger" : "accent"}>{casos(v.casos)}</Badge>
                    </div>
                    <p className="mt-1 text-ink-2">{v.texto}</p>
                    {exemplos.length > 0 && (
                      <ul className="mt-3 divide-y divide-line overflow-hidden rounded-lg border border-line">
                        {exemplos.map((e, i) => (
                          <li key={i} className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-0.5 px-3 py-2 text-[13px]">
                            <span className="min-w-0 break-words font-medium text-ink">{e.rotulo}</span>
                            {e.detalhe && <span className="text-ink-3">{detalhe(e.detalhe)}</span>}
                          </li>
                        ))}
                      </ul>
                    )}
                    {(v.exemplos.length > VISIVEIS || v.casos > v.exemplos.length) && (
                      <div className="mt-2 flex flex-wrap items-center gap-3 text-[12.5px] text-ink-3">
                        {v.exemplos.length > VISIVEIS && (
                          <button type="button" onClick={() => alternar(v.id)} className="font-medium text-brand hover:underline">
                            {aberta ? "Mostrar menos" : `Mostrar mais ${v.exemplos.length - VISIVEIS}`}
                          </button>
                        )}
                        {aberta && v.casos > v.exemplos.length && <span>Aqui só aparecem os primeiros {v.exemplos.length}.</span>}
                      </div>
                    )}
                  </div>
                  <Button size="sm" variant="secondary" className="ml-8 sm:ml-0" onClick={() => navigate(v.corrigir)}>
                    Corrigir
                    <ArrowRight className="size-4" />
                  </Button>
                </div>
              </Card>
            );
          })}

          {emOrdem.length > 0 && (
            <Card className="p-5">
              <h2 className="mb-2 text-[12px] font-medium uppercase tracking-wide text-ink-3">Em ordem</h2>
              <ul className="grid gap-x-6 gap-y-1.5 sm:grid-cols-2">
                {emOrdem.map((v) => (
                  <li key={v.id} className="flex items-center gap-2 text-ink-2">
                    <CheckCircle2 className="size-4 shrink-0 text-ok" />
                    {v.titulo}
                  </li>
                ))}
              </ul>
            </Card>
          )}

          <p className="px-1 text-[12.5px] text-ink-3">
            As verificações só lêem os dados. As correcções fazem-se nos ecrãs de sempre, com as mesmas permissões, e ficam no registo de auditoria.
          </p>
        </div>
      )}
    </div>
  );
}

function Resumo({ icon: Icone, tom, valor, rotulo }: { icon: typeof XCircle; tom: "danger" | "accent" | "ok"; valor: number; rotulo: string }) {
  const cor = { danger: "text-danger bg-danger-50", accent: "text-accent-ink bg-accent-50", ok: "text-ok bg-ok-50" }[tom];
  return (
    <Card className="flex items-center gap-3 p-4">
      <span className={cn("rounded-lg p-2", valor > 0 || tom === "ok" ? cor : "bg-sunken text-ink-3")}>
        <Icone className="size-5" />
      </span>
      <div>
        <div className="num text-[22px] font-semibold leading-none text-ink">{numero(valor, 0)}</div>
        <div className="mt-1 text-[12.5px] text-ink-3">{rotulo}</div>
      </div>
    </Card>
  );
}

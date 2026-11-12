import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Link, useSearchParams } from "react-router-dom";
import { BookOpen, Database, Download, Info, Keyboard, RefreshCw } from "lucide-react";
import { call } from "@/lib/api";
import { numero } from "@/lib/format";
import { MENU } from "@/lib/menu";
import { Button, Card, Field, Input, PageHeader, cn } from "@/components/ui";
import { useSession } from "@/store/session";

interface Info {
  versao: string;
  plataforma: string;
  sqlite: string;
  migracoes: number;
  pasta_dados: string;
  pasta_exportacao: string;
  base_bytes: number;
  ultima_copia: string | null;
  artigos: number;
  entidades: number;
  documentos: number;
  importados: number;
  utilizadores: number;
}

const ATALHOS_POS: [string, string][] = [
  ["F2", "Pesquisar artigo / ler código de barras (3*código = 3 unidades)"],
  ["F3", "Quantidade da linha"],
  ["F4", "Escolher cliente"],
  ["F5", "Novo cliente / alterar dados do cliente"],
  ["F6", "Desconto da linha"],
  ["F7", "Alterar preço (permissão ou PIN de supervisor)"],
  ["F8", "Desconto global (% ou valor)"],
  ["F9", "Descrição da linha"],
  ["F10", "Suspender documento"],
  ["F11", "Abrir documentos suspensos"],
  ["F12", "Pagamento e finalizar"],
  ["Del", "Apagar linha (permissão ou PIN)"],
  ["↑ / ↓", "Mudar de linha"],
  ["Ctrl+G", "Abrir a gaveta (impressora térmica)"],
];

const MANUAL: { titulo: string; texto: string }[] = [
  { titulo: "Primeiros passos", texto: "Preencha a empresa (Configurações), crie os utilizadores e as permissões, configure o posto e a impressora e, se vem do KIGEST antigo, use «Importar dados do KIGEST» antes de começar a facturar." },
  { titulo: "Documentos", texto: "Todos os documentos (facturas, recibos, notas de crédito, guias, compras e stock) usam o mesmo ecrã. As séries são automáticas por posto e ano. Um documento emitido não se altera: anula-se com motivo ou corrige-se com nota de crédito." },
  { titulo: "POS", texto: "O caixa tem de estar aberto para vender. No fecho conte o dinheiro: o sistema mostra a diferença para o valor esperado. Os talões podem sair na impressora térmica (ESC/POS) ou pelo diálogo de impressão." },
  { titulo: "Stock", texto: "O stock muda em cada documento. Artigos com lotes saem pelo lote que expira primeiro. Use o Inventário para acertar contagens e o Recálculo de Stock se suspeitar de existências erradas." },
  { titulo: "Tesouraria", texto: "Recebimentos e pagamentos dos documentos entram sozinhos no fluxo de caixa. Movimentos manuais, transferências entre contas e centros de custo estão em Tesouraria." },
  { titulo: "Recursos humanos", texto: "Lance as alterações do mês (horas extra, faltas, subsídios), simule, processe e pague pela tesouraria. Confirme a tabela de IRT na Configuração Salarial antes do primeiro processamento." },
  { titulo: "SAF-T e AGT", texto: "Exporte o SAF-T (facturação, aquisição ou Estado) em Gestão de Finanças. Carregue a chave privada da empresa em Configurações para assinar os documentos." },
  { titulo: "Cópias de segurança", texto: "É feita uma cópia automática por dia (as últimas 14 ficam guardadas). Faça também cópias manuais para um disco externo ou para a nuvem." },
];

const mb = (b: number) => (b < 1024 * 1024 ? `${numero(Math.max(1, Math.round(b / 1024)), 0)} KB` : `${numero(b / 1024 / 1024, 1)} MB`);

/** Sobre o KanGest, manual rápido e atalhos de teclado. */
export default function Sobre() {
  const [sp, setSp] = useSearchParams();
  const vista = sp.get("vista") === "manual" ? "manual" : "sobre";
  const info = useQuery({ queryKey: ["sistema-info"], queryFn: () => call<Info>("sistema.info") }).data;

  return (
    <>
      <PageHeader title={vista === "manual" ? "Manual e Atalhos" : "Sobre o KanGest"} subtitle="Gestão comercial e ponto de venda para Angola" />
      <div className="mb-4 flex w-fit gap-1 rounded-lg bg-sunken p-1">
        {(
          [
            ["sobre", "Sobre"],
            ["manual", "Manual e atalhos"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            onClick={() => setSp({ vista: id })}
            className={cn("rounded-md px-3 py-1.5 text-[13px]", vista === id ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}
          >
            {label}
          </button>
        ))}
      </div>

      {vista === "sobre" ? (
        <div className="grid grid-cols-2 gap-4">
          <Card className="p-6">
            <div className="mb-4 flex items-center gap-2 font-medium">
              <Info className="size-4 text-brand" /> Aplicação
            </div>
            <dl className="grid grid-cols-[150px_minmax(0,1fr)] gap-y-2 text-[13.5px]">
              <dt className="text-ink-3">Versão</dt>
              <dd className="num font-medium">{info?.versao ?? "…"}</dd>
              <dt className="text-ink-3">Sistema</dt>
              <dd>{info?.plataforma}</dd>
              <dt className="text-ink-3">Motor da base</dt>
              <dd>
                SQLite {info?.sqlite} · {info?.migracoes} actualizações de estrutura aplicadas
              </dd>
            </dl>
          </Card>
          <Card className="p-6">
            <div className="mb-4 flex items-center gap-2 font-medium">
              <Database className="size-4 text-brand" /> Dados
            </div>
            <dl className="grid grid-cols-[150px_minmax(0,1fr)] gap-y-2 text-[13.5px]">
              <dt className="text-ink-3">Pasta de dados</dt>
              <dd className="break-all font-mono text-[12px]">{info?.pasta_dados}</dd>
              <dt className="text-ink-3">Exportações</dt>
              <dd className="break-all font-mono text-[12px]">{info?.pasta_exportacao}</dd>
              <dt className="text-ink-3">Tamanho da base</dt>
              <dd className="num">{info ? mb(info.base_bytes) : "…"}</dd>
              <dt className="text-ink-3">Última cópia</dt>
              <dd>{info?.ultima_copia ?? "nenhuma ainda"}</dd>
              <dt className="text-ink-3">Registos</dt>
              <dd>
                {info && (
                  <>
                    {info.artigos} artigos · {info.entidades} entidades · {info.documentos} documentos
                    {info.importados ? ` (${info.importados} importados do KIGEST)` : ""} · {info.utilizadores} {info.utilizadores === 1 ? "utilizador" : "utilizadores"}
                  </>
                )}
              </dd>
            </dl>
          </Card>
          <Atualizacoes />
        </div>
      ) : (
        <div className="grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-4">
          <Card className="p-6">
            <div className="mb-4 flex items-center gap-2 font-medium">
              <BookOpen className="size-4 text-brand" /> Manual rápido
            </div>
            <div className="space-y-4">
              {MANUAL.map((m) => (
                <div key={m.titulo}>
                  <div className="font-medium">{m.titulo}</div>
                  <p className="mt-0.5 text-[13.5px] leading-relaxed text-ink-2">{m.texto}</p>
                </div>
              ))}
            </div>
            <div className="mt-6 border-t border-line pt-4">
              <div className="mb-2 text-[12px] uppercase tracking-wide text-ink-3">Menus</div>
              <div className="grid grid-cols-2 gap-x-6 gap-y-3 text-[13px]">
                {MENU.map((g) => (
                  <div key={g.id}>
                    <div className="font-medium">{g.titulo}</div>
                    <div className="text-ink-3">
                      {g.itens
                        .filter((i) => i.to)
                        .map((i, k) => (
                          <span key={i.label}>
                            {k > 0 && " · "}
                            <Link className="hover:text-brand hover:underline" to={i.to!}>
                              {i.label}
                            </Link>
                          </span>
                        ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </Card>
          <Card className="h-fit p-6">
            <div className="mb-4 flex items-center gap-2 font-medium">
              <Keyboard className="size-4 text-brand" /> Atalhos do POS
            </div>
            <table className="w-full text-[13.5px]">
              <tbody>
                {ATALHOS_POS.map(([k, d]) => (
                  <tr key={k} className="border-b border-line last:border-0">
                    <td className="w-20 py-1.5">
                      <kbd className="rounded border border-line bg-sunken px-1.5 py-0.5 font-mono text-[12px]">{k}</kbd>
                    </td>
                    <td className="py-1.5 text-ink-2">{d}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </div>
      )}
    </>
  );
}

interface EstadoAtualizacao {
  versao_atual: string;
  plataforma: string;
  fonte: string;
  chave_publica: boolean;
  ultima_verificacao: string | null;
}
interface Verificacao {
  versao_atual: string;
  versao: string;
  data: string | null;
  notas: string | null;
  disponivel: boolean;
  pacote: string | null;
}

/** Verificar Atualização (KIGEST GC → Sistema): origem num endereço ou numa pasta partilhada na rede. */
function Atualizacoes() {
  const qc = useQueryClient();
  const admin = useSession((s) => s.tem)("sistema.atualizacoes");
  const estado = useQuery({ queryKey: ["atualizacao"], queryFn: () => call<EstadoAtualizacao>("atualizacao.estado") }).data;
  const [fonte, setFonte] = useState<string | null>(null);
  const [chave, setChave] = useState("");
  const [v, setV] = useState<Verificacao | null>(null);
  const [baixado, setBaixado] = useState<{ ficheiro: string; versao: string; copia: string | null } | null>(null);
  const [busy, setBusy] = useState<"" | "gravar" | "verificar" | "descarregar" | "instalar">("");

  async function executar<T>(qual: typeof busy, f: () => Promise<T>) {
    setBusy(qual);
    try {
      return await f();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }
  const gravar = () =>
    executar("gravar", async () => {
      await call("atualizacao.guardar", { fonte: fonte ?? estado?.fonte ?? "", chave_publica: chave.trim() ? chave : undefined });
      toast.success("Origem das actualizações gravada.");
      setFonte(null);
      setChave("");
      qc.invalidateQueries({ queryKey: ["atualizacao"] });
    });
  const verificar = () =>
    executar("verificar", async () => {
      const r = await call<Verificacao>("atualizacao.verificar");
      setV(r);
      qc.invalidateQueries({ queryKey: ["atualizacao"] });
      if (!r.disponivel) toast.success(`Já tem a versão mais recente (${r.versao_atual}).`);
    });
  const descarregar = () =>
    executar("descarregar", async () => {
      const r = await call<{ ficheiro: string; versao: string; copia: string | null }>("atualizacao.descarregar");
      setBaixado(r);
      toast.success(`Versão ${r.versao} descarregada e conferida.${r.copia ? " Foi feita uma cópia de segurança da base." : ""}`);
    });
  const instalar = () =>
    executar("instalar", async () => {
      await call("atualizacao.instalar", { ficheiro: baixado?.ficheiro });
      toast.success("O instalador foi aberto. Feche o KanGest para concluir a instalação.", { duration: 12000 });
    });

  return (
    <Card className="col-span-2 p-6">
      <div className="mb-4 flex items-center gap-2 font-medium">
        <RefreshCw className="size-4 text-brand" /> Actualizações
      </div>
      <div className="grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-6">
        <div className="space-y-3">
          <Field label="Origem (endereço ou pasta partilhada na rede)" hint="Onde está o ficheiro kangest-atualizacao.json, ex.: https://… ou \\SERVIDOR\KanGest">
            <Input disabled={!admin} value={fonte ?? estado?.fonte ?? ""} onChange={(e) => setFonte(e.target.value)} placeholder="https://… ou \\SERVIDOR\KanGest" />
          </Field>
          <Field
            label="Chave pública de assinatura (opcional)"
            hint={estado?.chave_publica ? "Configurada: só se instalam pacotes assinados com esta chave." : "Sem chave, o pacote é conferido apenas pelo SHA-256 do manifesto."}
          >
            <Input disabled={!admin} value={chave} onChange={(e) => setChave(e.target.value)} placeholder={estado?.chave_publica ? "•••• (deixe vazio para manter)" : "Chave minisign (tauri signer)"} />
          </Field>
          {admin && (
            <Button variant="secondary" loading={busy === "gravar"} disabled={fonte === null && !chave.trim()} onClick={gravar}>
              Gravar origem
            </Button>
          )}
        </div>
        <div className="text-[13.5px]">
          <dl className="grid grid-cols-[130px_minmax(0,1fr)] gap-y-2">
            <dt className="text-ink-3">Versão instalada</dt>
            <dd className="num font-medium">{estado?.versao_atual ?? "…"}</dd>
            <dt className="text-ink-3">Sistema</dt>
            <dd>{estado?.plataforma}</dd>
            <dt className="text-ink-3">Última verificação</dt>
            <dd>{estado?.ultima_verificacao?.replace("T", " ") ?? "nunca"}</dd>
            {v && (
              <>
                <dt className="text-ink-3">Versão publicada</dt>
                <dd className="num">
                  {v.versao}
                  {v.data ? ` · ${v.data}` : ""}
                </dd>
              </>
            )}
          </dl>
          {v?.notas && <p className="mt-3 whitespace-pre-line rounded-lg bg-sunken p-3 text-ink-2">{v.notas}</p>}
          <div className="mt-4 flex flex-wrap gap-2">
            <Button variant="secondary" loading={busy === "verificar"} disabled={!estado?.fonte} onClick={verificar}>
              <RefreshCw className="size-4" /> Verificar atualização
            </Button>
            {admin && v?.disponivel && !baixado && (
              <Button loading={busy === "descarregar"} disabled={!v.pacote} onClick={descarregar}>
                <Download className="size-4" /> Descarregar {v.versao}
              </Button>
            )}
            {admin && baixado && (
              <Button loading={busy === "instalar"} onClick={instalar}>
                Instalar {baixado.versao}
              </Button>
            )}
          </div>
          {v?.disponivel && !v.pacote && <p className="mt-2 text-danger">A versão {v.versao} não tem pacote para este sistema.</p>}
          <p className="mt-3 text-[12.5px] text-ink-3">Os dados e as cópias ficam na pasta de dados e não são apagados pela instalação. Cada posto actualiza-se a si próprio.</p>
        </div>
      </div>
    </Card>
  );
}

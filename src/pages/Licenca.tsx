import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AlertTriangle, CheckCircle2, Clock, Copy, ExternalLink, KeyRound, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { data } from "@/lib/format";
import { NOMES_PLANO, urlPedirLicenca, useLicenca, type LicencaEstado } from "@/lib/licenca";
import { useSession } from "@/store/session";
import { Badge, Button, Card, PageHeader, cn } from "@/components/ui";

const MODOS: Record<LicencaEstado["modo"], { rotulo: string; tom: "ok" | "accent" | "danger" | "brand" }> = {
  EXPERIENCIA: { rotulo: "Período de experiência", tom: "brand" },
  ACTIVA: { rotulo: "Licença activa", tom: "ok" },
  TOLERANCIA: { rotulo: "Assinatura terminada · em tolerância", tom: "accent" },
  EXPIRADA: { rotulo: "Modo de consulta", tom: "danger" },
  RELOGIO: { rotulo: "Data do computador atrasada", tom: "danger" },
  INVALIDA: { rotulo: "Licença por resolver", tom: "danger" },
};

async function abrirPedido(e?: LicencaEstado | null) {
  const url = urlPedirLicenca(e);
  try {
    await call("licenca.abrirSite", { url });
  } catch {
    window.open(url, "_blank", "noopener"); // fora da aplicação (auditoria no browser)
  }
}

export default function Licenca() {
  const { data: e, isLoading } = useLicenca();
  const tem = useSession((s) => s.tem);
  const qc = useQueryClient();
  const [chave, setChave] = useState("");
  const [aActivar, setAActivar] = useState(false);
  const podeActivar = tem("sistema.configuracoes");

  async function copiar() {
    if (!e) return;
    try {
      await navigator.clipboard.writeText(e.pedido);
      toast.success("Código do pedido copiado.");
    } catch {
      toast.info(`Código do pedido: ${e.pedido}`);
    }
  }

  async function activar() {
    setAActivar(true);
    try {
      const r = await call<{ estado: LicencaEstado }>("licenca.activar", { chave });
      qc.setQueryData(["licenca"], r.estado);
      setChave("");
      toast.success(r.estado.mensagem);
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setAActivar(false);
    }
  }

  if (isLoading || !e) return <PageHeader title="Licença e Assinatura" subtitle="A carregar…" />;
  const modo = MODOS[e.modo];
  const Icone = e.modo === "ACTIVA" ? CheckCircle2 : e.pode_emitir ? Clock : AlertTriangle;

  return (
    <>
      <PageHeader title="Licença e Assinatura" subtitle="Estado da licença deste KanGest, pedido e activação" />

      <Card className="p-6">
        <div className="flex flex-wrap items-start gap-4">
          <div className={cn("grid size-12 shrink-0 place-items-center rounded-xl", e.pode_emitir ? (e.aviso ? "bg-accent-50 text-[#9a6514]" : "bg-brand-50 text-brand") : "bg-danger-50 text-danger")}>
            <Icone className="size-6" />
          </div>
          <div className="min-w-0 flex-1">
            <Badge tone={modo.tom}>{modo.rotulo}</Badge>
            <p className="mt-2 text-[15px]">{e.mensagem}</p>
          </div>
          {e.pode_emitir && e.dias > 0 && (
            <div className="text-right">
              <div className="num text-[34px] font-semibold leading-none">{e.dias}</div>
              <div className="text-[12.5px] text-ink-3">{e.dias === 1 ? "dia restante" : "dias restantes"}</div>
            </div>
          )}
        </div>
        <dl className="mt-6 grid grid-cols-2 gap-x-8 gap-y-3 border-t border-line pt-5 text-[13.5px] md:grid-cols-4">
          <div><dt className="text-ink-3">Plano</dt><dd className="font-medium">{NOMES_PLANO[e.plano] ?? e.plano}</dd></div>
          <div><dt className="text-ink-3">{e.modo === "EXPERIENCIA" ? "Experiência até" : "Válida até"}</dt><dd className="font-medium">{e.fim ? data(e.fim) : "—"}</dd></div>
          <div><dt className="text-ink-3">Postos</dt><dd className="font-medium">{e.postos > 0 ? e.postos : "Sem limite"}</dd></div>
          <div><dt className="text-ink-3">Empresa (NIF)</dt><dd className="font-medium">{e.nif ?? "—"}</dd></div>
          {e.licenca_id && <div><dt className="text-ink-3">Licença</dt><dd className="font-mono text-[12.5px]">{e.licenca_id}</dd></div>}
          <div><dt className="text-ink-3">Este computador</dt><dd className="font-mono text-[12.5px]">{e.maquina}</dd></div>
        </dl>
        {!e.pode_emitir && (
          <p className="mt-5 rounded-lg bg-sunken px-4 py-3 text-[13px] text-ink-2">
            Em modo de consulta continua a ver e imprimir todos os documentos, os relatórios e o SAF-T. Só a emissão de documentos novos fica parada até activar uma licença válida.
          </p>
        )}
      </Card>

      <div className="mt-6 grid gap-6 lg:grid-cols-2">
        <Card className="p-6">
          <div className="mb-1 flex items-center gap-2 font-semibold"><KeyRound className="size-4 text-brand" /> Pedir ou renovar</div>
          <p className="text-[13.5px] text-ink-2">Envie este código com o pedido. Identifica a empresa e o computador, para a licença ser emitida para este KanGest.</p>
          <div className="mt-4 flex items-center gap-2">
            <code className="min-w-0 flex-1 select-all truncate rounded-lg border border-line bg-sunken px-3 py-2.5 font-mono text-[14px] tracking-wide">{e.pedido}</code>
            <Button variant="secondary" onClick={copiar} title="Copiar o código do pedido"><Copy className="size-4" /> Copiar</Button>
          </div>
          <ol className="mt-5 grid gap-2 text-[13.5px] text-ink-2">
            <li><b className="text-ink">1.</b> Copie o código e escolha o plano.</li>
            <li><b className="text-ink">2.</b> Faça o pagamento indicado na proposta.</li>
            <li><b className="text-ink">3.</b> Receba a licença (começa por «KG1.») e cole-a ao lado.</li>
          </ol>
          <Button className="mt-5" onClick={() => abrirPedido(e)}><ExternalLink className="size-4" /> {e.licenca_id ? "Pedir a renovação" : "Pedir a licença"}</Button>
        </Card>

        <Card className="p-6">
          <div className="mb-1 flex items-center gap-2 font-semibold"><ShieldCheck className="size-4 text-brand" /> Activar a licença</div>
          <p className="text-[13.5px] text-ink-2">Cole aqui o texto da licença tal como o recebeu, por email ou WhatsApp. É verificada neste computador, sem internet.</p>
          <textarea
            className="mt-4 h-32 w-full resize-none rounded-lg border border-line bg-surface px-3 py-2.5 font-mono text-[12.5px] outline-none focus:border-brand focus:ring-2 focus:ring-brand-50 disabled:opacity-60"
            placeholder="KG1.…"
            spellCheck={false}
            value={chave}
            disabled={!podeActivar}
            onChange={(ev) => setChave(ev.target.value)}
            aria-label="Texto da licença"
          />
          {podeActivar ? (
            <Button className="mt-3" disabled={!chave.trim()} loading={aActivar} onClick={activar}>Activar</Button>
          ) : (
            <p className="mt-3 text-[13px] text-ink-3">Só um administrador pode activar a licença.</p>
          )}
        </Card>
      </div>
    </>
  );
}

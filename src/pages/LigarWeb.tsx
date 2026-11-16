import { useState, type FormEvent } from "react";
import { KeyRound, MonitorSmartphone, ShieldCheck } from "lucide-react";
import { ligarWeb, ligacaoWeb } from "@/lib/api";
import { Button, Field, Input } from "@/components/ui";
import { Moldura } from "@/components/Sessao";

/**
 * Browser (telemóvel, tablet, terminal POS, outro computador) ainda sem ligação ao servidor KanGest:
 * pede a chave de rede uma vez. A chave fica só derivada neste aparelho; os pedidos seguem cifrados.
 */
export default function LigarWeb({ onLigado }: { onLigado: () => void }) {
  const anterior = ligacaoWeb();
  const [chave, setChave] = useState("");
  const [posto, setPosto] = useState(String(anterior?.posto ?? 1));
  const [erro, setErro] = useState("");
  const [busy, setBusy] = useState(false);
  const seguro = window.isSecureContext;

  async function ligar(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErro("");
    try {
      await ligarWeb(chave, Number(posto) || 1);
      onLigado();
    } catch (err) {
      setErro((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Moldura>
      <div className="w-full max-w-sm">
        <div className="mb-6">
          <div className="mb-4 inline-flex rounded-xl bg-brand-50 p-2.5 text-brand">
            <MonitorSmartphone className="size-5" />
          </div>
          <h2 className="text-[24px] font-semibold tracking-tight">Ligar este aparelho</h2>
          <p className="mt-1 text-ink-3">
            Introduza a <b>chave de rede</b> do servidor KanGest da loja. Encontra-a no computador servidor, em Configurações → Rede.
          </p>
        </div>
        <form onSubmit={ligar} className="space-y-4">
          <Field label="Chave de rede">
            <Input
              autoFocus
              required
              type="password"
              autoComplete="off"
              autoCapitalize="off"
              spellCheck={false}
              value={chave}
              onChange={(e) => setChave(e.target.value)}
              className="h-12 text-[16px]"
            />
          </Field>
          <Field label="N.º do posto" hint="Define a série, o armazém, a impressora e o caixa deste aparelho. Pode mudá-lo depois.">
            <Input type="number" inputMode="numeric" min={1} required value={posto} onChange={(e) => setPosto(e.target.value)} className="h-12 text-[16px]" />
          </Field>
          <Button size="lg" className="h-12 w-full" loading={busy}>
            <KeyRound className="size-4" /> Ligar
          </Button>
        </form>
        {busy && !window.crypto?.subtle && <p className="mt-3 text-[12px] text-ink-3">A preparar a chave de cifra neste aparelho… pode demorar alguns segundos.</p>}
        {erro && <p className="mt-4 rounded-lg bg-danger-50 px-3 py-2 text-[13px] text-danger">{erro}</p>}
        <p className="mt-6 flex items-start gap-2 text-[12px] leading-relaxed text-ink-3">
          <ShieldCheck className="mt-0.5 size-4 shrink-0 text-brand" />
          {seguro
            ? "Ligação segura (HTTPS). Os pedidos vão também cifrados com a chave de rede."
            : "Os pedidos vão cifrados com a chave de rede, mesmo sem HTTPS. A chave não fica guardada neste aparelho — só a chave de cifra derivada dela."}
        </p>
      </div>
    </Moldura>
  );
}

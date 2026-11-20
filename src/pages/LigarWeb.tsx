// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState, type FormEvent } from "react";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { ligarWeb, ligacaoWeb } from "@/lib/api";
import { Button } from "@/components/ui";
import { BoasVindas, CampoAcesso, InputAcesso, InputSenha, Moldura, type SeccaoAjuda } from "@/components/Acesso";
import PerguntasAcesso from "@/pages/PerguntasAcesso";

const OBRIGATORIO = "Este campo é obrigatório";

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
  const [tocado, setTocado] = useState(false);
  const [ajuda, setAjuda] = useState<{ seccao?: SeccaoAjuda } | null>(null);
  const seguro = window.isSecureContext;

  async function ligar(e: FormEvent) {
    e.preventDefault();
    setTocado(true);
    if (!chave || !posto.trim()) return;
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

  const abrirAjuda = (seccao?: SeccaoAjuda) => setAjuda({ seccao });
  if (ajuda) return <PerguntasAcesso seccao={ajuda.seccao ?? "aparelhos"} onVoltar={() => setAjuda(null)} onAjuda={abrirAjuda} />;

  return (
    <Moldura onAjuda={abrirAjuda}>
      <div className="w-full max-w-[440px]">
        <BoasVindas
          texto={
            <>
              Ligue este aparelho ao servidor da loja com a <b className="text-ink-2">chave de rede</b>. Encontra-a no computador servidor, em
              Configurações → Rede.
            </>
          }
        />
        <form noValidate onSubmit={ligar} className="space-y-5">
          <CampoAcesso label="Chave de rede" erro={tocado && !chave && OBRIGATORIO}>
            <InputSenha autoFocus autoComplete="off" autoCapitalize="off" spellCheck={false} value={chave} onChange={(e) => setChave(e.target.value)} invalido={tocado && !chave} />
          </CampoAcesso>
          <CampoAcesso
            label="N.º do posto"
            erro={tocado && !posto.trim() && OBRIGATORIO}
            dica="Define a série, o armazém, a impressora e o caixa deste aparelho. Pode mudá-lo depois."
          >
            <InputAcesso type="number" inputMode="numeric" min={1} value={posto} onChange={(e) => setPosto(e.target.value)} invalido={tocado && !posto.trim()} />
          </CampoAcesso>
          <Button variant="accent" size="lg" className="h-12 w-full text-[15px]" loading={busy}>
            Ligar <ArrowRight className="size-4" />
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

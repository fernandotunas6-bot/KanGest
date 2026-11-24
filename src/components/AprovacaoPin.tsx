// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

// Pedido de PIN das regras de aprovação (services/aprovacoes.rs). Montado uma vez na raiz da aplicação: quando o
// servidor recusa uma operação por falta de aprovação, `call` (lib/api.ts) abre este diálogo e repete a operação com
// o PIN. Um PIN errado, ou de quem não pode aprovar, fica no diálogo para se tentar outro; qualquer outro erro segue
// para o ecrã que fez a operação, como se o diálogo não existisse.
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { KeyRound } from "lucide-react";
import { definirPedidoAprovacao } from "@/lib/api";
import { Button, Field, Input, Modal } from "@/components/ui";

interface Pedido {
  operacao: string;
  tentar: (pin: string) => Promise<unknown>;
  resolver: (v: unknown) => void;
  rejeitar: (e: Error) => void;
}

export function AprovacaoPin() {
  const [pedido, setPedido] = useState<Pedido | null>(null);
  const aberto = useRef(false);
  const [pin, setPin] = useState("");
  const [erro, setErro] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(
    () =>
      definirPedidoAprovacao(
        (mensagem, tentar) =>
          new Promise((resolver, rejeitar) => {
            // um pedido de cada vez (dois ao mesmo tempo só por duplo clique)
            if (aberto.current) return rejeitar(new Error("Já há uma aprovação à espera do PIN."));
            aberto.current = true;
            const operacao = mensagem.split(": precisa da aprovação")[0].trim();
            setPin("");
            setErro(null);
            setPedido({ operacao, tentar, resolver, rejeitar });
          }),
      ),
    [],
  );

  function fechar() {
    aberto.current = false;
    setPedido(null);
  }

  function desistir() {
    pedido?.rejeitar(new Error("Operação cancelada: falta a aprovação de um supervisor."));
    fechar();
  }

  async function aprovar() {
    if (!pedido || pin.length < 4 || busy) return;
    setBusy(true);
    try {
      const r = await pedido.tentar(pin);
      toast.success("Aprovado.");
      pedido.resolver(r);
      fechar();
    } catch (e) {
      const m = (e as Error).message;
      if (/PIN/.test(m)) {
        setErro(m);
        setPin("");
      } else {
        pedido.rejeitar(e as Error);
        fechar();
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={!!pedido}
      onClose={desistir}
      title="Aprovação do supervisor"
      width="max-w-sm"
      footer={
        <>
          <Button variant="secondary" onClick={desistir}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={pin.length < 4} onClick={aprovar}>
            Aprovar
          </Button>
        </>
      }
    >
      <div className="mb-4 flex items-start gap-3 rounded-xl bg-accent-50 p-3 text-[13px] text-ink-2">
        <KeyRound className="mt-0.5 size-4 shrink-0 text-accent-ink" />
        <span>
          <b className="text-ink">{pedido?.operacao}</b> precisa de aprovação. Peça o PIN a quem pode aprovar (ou use o seu, se tiver essa permissão).
        </span>
      </div>
      <Field label="PIN" erro={erro ?? undefined}>
        <Input
          autoFocus
          type="password"
          inputMode="numeric"
          autoComplete="off"
          aria-label="PIN de quem aprova"
          className="num h-14 text-center text-[24px] tracking-[0.5em]"
          value={pin}
          onChange={(e) => {
            setPin(e.target.value.replace(/\D/g, "").slice(0, 8));
            setErro(null);
          }}
          onKeyDown={(e) => e.key === "Enter" && aprovar()}
        />
      </Field>
    </Modal>
  );
}

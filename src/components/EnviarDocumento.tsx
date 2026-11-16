import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { Copy, Mail, MessageCircle } from "lucide-react";
import { abrirExterno, call, copiarTexto } from "@/lib/api";
import { Button, Field, Input, Modal } from "./ui";

interface MensagemDoc {
  texto: string;
  url: string;
  telefone: string;
  numero: string | null;
  email: string;
}

/** Recibo digital: o documento por email (servidor da empresa) ou por WhatsApp (abre no posto). */
export default function EnviarDocumento({ id, numero, onClose }: { id: number; numero: string; onClose: () => void }) {
  const m = useQuery({ queryKey: ["mensagem-doc", id], queryFn: () => call<MensagemDoc>("documentos.mensagem", { id }) });
  const [email, setEmail] = useState("");
  const [telefone, setTelefone] = useState("");
  const [busy, setBusy] = useState<"" | "email" | "whatsapp">("");
  useEffect(() => {
    if (m.data) {
      setEmail(m.data.email);
      setTelefone(m.data.telefone);
    }
  }, [m.data]);

  async function porEmail() {
    setBusy("email");
    try {
      const r = await call<{ para: string[] }>("documentos.enviarEmail", { id, para: email });
      toast.success(`${numero} enviado para ${r.para.join(", ")}.`);
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }

  async function porWhatsapp() {
    setBusy("whatsapp");
    try {
      const r = await call<MensagemDoc>("documentos.mensagem", { id, telefone });
      // no posto (aplicação) abre o WhatsApp do sistema; no navegador, um separador novo
      await abrirExterno(r.url);
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }

  async function copiar() {
    try {
      await copiarTexto(m.data?.texto ?? "");
      toast.success("Texto copiado.");
    } catch {
      toast.error("Não foi possível copiar o texto.");
    }
  }

  return (
    <Modal open onClose={onClose} title={`Enviar ${numero}`} width="max-w-xl">
      <div className="grid gap-5">
        <form
          className="grid grid-cols-[1fr_auto] items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            porEmail();
          }}
        >
          <Field label="Email do cliente" hint="Vários endereços separados por «;». O envio usa o servidor de email das Configurações.">
            <Input inputMode="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="cliente@exemplo.ao" autoFocus />
          </Field>
          <Button type="submit" loading={busy === "email"} disabled={!email.trim()} className="mb-[22px]">
            <Mail className="size-4" /> Enviar por email
          </Button>
        </form>
        <form
          className="grid grid-cols-[1fr_auto] items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            porWhatsapp();
          }}
        >
          <Field label="Telefone (WhatsApp)" hint="Números de Angola com 9 algarismos; de outros países com o indicativo (ex.: +351…).">
            <Input value={telefone} onChange={(e) => setTelefone(e.target.value)} placeholder="923 456 789" />
          </Field>
          <Button type="submit" variant="secondary" loading={busy === "whatsapp"} className="mb-[22px]">
            <MessageCircle className="size-4" /> Abrir no WhatsApp
          </Button>
        </form>
        {m.data && (
          <div>
            <div className="mb-1.5 flex items-center justify-between text-[12px] font-medium uppercase tracking-wide text-ink-2">
              Mensagem
              <button type="button" onClick={copiar} className="flex items-center gap-1 normal-case tracking-normal text-brand hover:underline">
                <Copy className="size-3.5" /> Copiar texto
              </button>
            </div>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-xl border border-line bg-sunken/60 p-3 font-sans text-[12.5px] text-ink-2">{m.data.texto}</pre>
          </div>
        )}
      </div>
    </Modal>
  );
}

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { qrcode, qrSvgPath } from "@/lib/qrcode";
import { Badge, Button, Field, Input, Modal } from "@/components/ui";

interface Estado {
  activo: boolean;
  codigos_restantes: number;
  exigido_internet: boolean;
}

/**
 * Verificação em dois passos da própria conta (TOTP): ligar com o código QR numa aplicação de autenticação
 * (Google Authenticator, Microsoft Authenticator, Authy…) e guardar os códigos de recuperação; ou desligar.
 */
export default function DoisPassos({ onClose }: { onClose: () => void }) {
  const estado = useQuery({ queryKey: ["dois-passos"], queryFn: () => call<Estado>("auth.doisPassosEstado") });
  const [novo, setNovo] = useState<{ segredo: string; uri: string } | null>(null);
  const [codigo, setCodigo] = useState("");
  const [senha, setSenha] = useState("");
  const [codigos, setCodigos] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);

  async function comecar() {
    setNovo(await call<{ segredo: string; uri: string }>("auth.doisPassosNovo"));
  }
  async function ligar() {
    if (!novo) return;
    setBusy(true);
    try {
      const r = await call<{ codigos: string[] }>("auth.doisPassosLigar", { segredo: novo.segredo, codigo, senha });
      setCodigos(r.codigos);
      setNovo(null);
      setCodigo("");
      setSenha("");
      await estado.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  async function desligar() {
    setBusy(true);
    try {
      await call("auth.doisPassosDesligar", { senha, codigo });
      toast.success("Verificação em dois passos desligada.");
      setCodigo("");
      setSenha("");
      await estado.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const e = estado.data;
  const q = novo ? qrcode(novo.uri, "M") : null;
  const lado = q ? q.tamanho + 8 : 0;
  return (
    <Modal open title="Verificação em dois passos" onClose={onClose} width="max-w-lg">
      <div className="space-y-4 text-[13px] text-ink-2">
        {!e ? (
          <p>A carregar…</p>
        ) : codigos ? (
          <>
            <p className="flex items-center gap-2 font-medium text-ink">
              <ShieldCheck className="size-4 text-ok" /> Ligada. Guarde estes códigos de recuperação num sítio seguro.
            </p>
            <p>Cada um serve uma vez, para entrar se perder o telemóvel. Não voltam a ser mostrados.</p>
            <div className="grid grid-cols-2 gap-2 rounded-lg bg-sunken p-3 font-mono text-[15px] text-ink">
              {codigos.map((c) => (
                <span key={c}>{c}</span>
              ))}
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => navigator.clipboard.writeText(codigos.join("\n")).then(() => toast.success("Códigos copiados."))}>
                Copiar
              </Button>
              <Button onClick={onClose}>Já os guardei</Button>
            </div>
          </>
        ) : e.activo ? (
          <>
            <p className="flex flex-wrap items-center gap-2">
              <Badge tone="ok">Ligada</Badge> Ao entrar, depois da senha, pede o código da aplicação.
              <span className="text-ink-3">Códigos de recuperação por usar: {e.codigos_restantes}.</span>
            </p>
            <p>Para desligar, confirme com a senha e um código (da aplicação ou de recuperação).</p>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Senha">
                <Input type="password" autoComplete="current-password" value={senha} onChange={(x) => setSenha(x.target.value)} />
              </Field>
              <Field label="Código">
                <Input inputMode="numeric" autoComplete="one-time-code" value={codigo} onChange={(x) => setCodigo(x.target.value)} />
              </Field>
            </div>
            <div className="flex justify-end">
              <Button variant="danger" onClick={desligar} disabled={!senha || !codigo || busy}>
                Desligar
              </Button>
            </div>
          </>
        ) : novo && q ? (
          <>
            <p>1. Na aplicação de autenticação do telemóvel, acrescente uma conta e leia este código:</p>
            <div className="flex flex-wrap items-center gap-4">
              <svg viewBox={`0 0 ${lado} ${lado}`} className="size-44 shrink-0 rounded-lg border border-line" shapeRendering="crispEdges" role="img" aria-label="Código QR da verificação em dois passos">
                <rect width={lado} height={lado} fill="#fff" />
                <path d={qrSvgPath(q)} fill="#000" />
              </svg>
              <div className="min-w-0 flex-1 space-y-1">
                <p className="text-ink-3">Sem câmara? Escreva esta chave na aplicação:</p>
                <p className="break-all font-mono text-[13px] text-ink">{novo.segredo.replace(/(.{4})/g, "$1 ").trim()}</p>
              </div>
            </div>
            <p>2. Escreva o código que a aplicação mostra e a sua senha:</p>
            <div className="grid grid-cols-2 gap-3">
              <Field label="Código de 6 algarismos">
                <Input inputMode="numeric" autoComplete="one-time-code" value={codigo} onChange={(x) => setCodigo(x.target.value.replace(/\D/g, "").slice(0, 6))} className="font-mono tracking-[0.2em]" />
              </Field>
              <Field label="Senha">
                <Input type="password" autoComplete="current-password" value={senha} onChange={(x) => setSenha(x.target.value)} />
              </Field>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setNovo(null)}>
                Cancelar
              </Button>
              <Button onClick={ligar} disabled={codigo.length !== 6 || !senha || busy}>
                Ligar
              </Button>
            </div>
          </>
        ) : (
          <>
            <p>
              Depois da senha, o KanGest pede também um código que muda de 30 em 30 segundos numa aplicação do telemóvel (Google Authenticator, Microsoft Authenticator, Authy…).
              Quem souber a sua senha não entra sem o seu telemóvel.
            </p>
            {e.exigido_internet && <p className="text-accent-ink">Nesta empresa é obrigatória para entrar pela Internet.</p>}
            <div className="flex justify-end">
              <Button onClick={comecar}>
                <ShieldCheck className="size-4" /> Ligar a verificação em dois passos
              </Button>
            </div>
          </>
        )}
      </div>
    </Modal>
  );
}

import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { LockKeyhole, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { useSession } from "@/store/session";
import type { Sessao } from "@/lib/types";
import { Button, Field, Input, cn } from "@/components/ui";
import { Moldura, TecladoPin, inicioDe, usePolitica, useTecladoFisicoPin } from "@/components/Sessao";

export default function Login({ empresa }: { empresa: string | null }) {
  const [login, setLogin] = useState("");
  const [senha, setSenha] = useState("");
  const [pin, setPin] = useState("");
  const [modo, setModo] = useState<"senha" | "pin">("senha");
  const [erro, setErro] = useState("");
  // verificação em dois passos: a senha está certa e falta o código da aplicação (ou um de recuperação)
  const [pedirCodigo, setPedirCodigo] = useState(false);
  const [codigo, setCodigo] = useState("");
  const [busy, setBusy] = useState(false);
  const setSessao = useSession((s) => s.setSessao);
  const politica = usePolitica();
  const nav = useNavigate();
  const comPin = !!politica.data?.login_pin;

  async function entrar(metodo: "auth.login" | "auth.loginPin", params: object) {
    setBusy(true);
    setErro("");
    try {
      const s = await call<Sessao | { dois_passos: true }>(metodo, params);
      if ("dois_passos" in s) {
        setPedirCodigo(true);
        return;
      }
      setSessao(s);
      nav(inicioDe(s), { replace: true });
    } catch (err) {
      setErro((err as Error).message);
      setCodigo("");
      if (!pedirCodigo) setSenha("");
      setPin("");
    } finally {
      setBusy(false);
    }
  }

  function submeter(e: FormEvent) {
    e.preventDefault();
    entrar("auth.login", pedirCodigo ? { login, senha, codigo } : { login, senha });
  }

  function recomecar() {
    setPedirCodigo(false);
    setCodigo("");
    setSenha("");
    setErro("");
  }

  useTecladoFisicoPin(comPin && modo === "pin", pin, setPin, () => entrar("auth.loginPin", { pin }));

  return (
    <Moldura empresa={empresa}>
      <div className="w-full max-w-sm">
        <div className="mb-7">
          <div className="mb-4 inline-flex rounded-xl bg-brand-50 p-2.5 text-brand">
            <LockKeyhole className="size-5" />
          </div>
          <h2 className="text-[24px] font-semibold tracking-tight">Iniciar sessão</h2>
          <p className="mt-1 text-ink-3">{modo === "pin" ? "Introduza o seu PIN de operador." : "Introduza as suas credenciais."}</p>
        </div>

        {comPin && (
          <div className="mb-5 grid grid-cols-2 gap-1 rounded-lg bg-sunken p-1 text-[13px]">
            {(["senha", "pin"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => {
                  setModo(m);
                  setErro("");
                }}
                className={cn("rounded-md py-1.5", modo === m ? "bg-surface font-medium shadow-sm" : "text-ink-2")}
              >
                {m === "senha" ? "Utilizador e senha" : "PIN"}
              </button>
            ))}
          </div>
        )}

        {pedirCodigo ? (
          <form onSubmit={submeter} className="space-y-4">
            <div className="flex gap-3 rounded-lg bg-brand-50 p-3 text-[13px] text-ink-2">
              <ShieldCheck className="mt-0.5 size-4 shrink-0 text-brand" />
              <span>Verificação em dois passos: escreva o código de 6 algarismos que a aplicação de autenticação mostra agora (ou um dos códigos de recuperação).</span>
            </div>
            <Field label="Código">
              <Input
                autoFocus
                required
                inputMode="numeric"
                autoComplete="one-time-code"
                value={codigo}
                onChange={(e) => setCodigo(e.target.value.slice(0, 12))}
                className="h-11 text-center font-mono text-[20px] tracking-[0.3em]"
                placeholder="000000"
              />
            </Field>
            <Button size="lg" className="w-full" loading={busy}>
              Confirmar
            </Button>
            <button type="button" onClick={recomecar} className="w-full text-[13px] text-ink-3 hover:text-ink">
              Entrar com outro utilizador
            </button>
          </form>
        ) : modo === "pin" && comPin ? (
          <TecladoPin valor={pin} onChange={setPin} ocupado={busy} onConfirmar={() => entrar("auth.loginPin", { pin })} />
        ) : (
          <form onSubmit={submeter} className="space-y-4">
            <Field label="Utilizador">
              <Input autoFocus required autoComplete="username" value={login} onChange={(e) => setLogin(e.target.value)} className="h-11" />
            </Field>
            <Field label="Senha">
              <Input type="password" required autoComplete="current-password" value={senha} onChange={(e) => setSenha(e.target.value)} className="h-11" />
            </Field>
            <Button size="lg" className="w-full" loading={busy}>
              Entrar
            </Button>
          </form>
        )}

        {erro && <p className="mt-4 rounded-lg bg-danger-50 px-3 py-2 text-[13px] text-danger">{erro}</p>}
      </div>
    </Moldura>
  );
}

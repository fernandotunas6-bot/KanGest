import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { useSession } from "@/store/session";
import type { Sessao } from "@/lib/types";
import { useOcultarValores } from "@/lib/privacidade";
import { Button, cn } from "@/components/ui";
import { BoasVindas, CampoAcesso, InputAcesso, InputSenha, Interruptor, Moldura, type SeccaoAjuda } from "@/components/Acesso";
import { TecladoPin, inicioDe, usePolitica, useTecladoFisicoPin } from "@/components/Sessao";
import RecuperarSenha from "@/pages/RecuperarSenha";
import PerguntasAcesso from "@/pages/PerguntasAcesso";

const OBRIGATORIO = "Este campo é obrigatório";

export default function Login({ empresa, imagemPropria = false }: { empresa: string | null; imagemPropria?: boolean }) {
  const [login, setLogin] = useState("");
  const [senha, setSenha] = useState("");
  const [pin, setPin] = useState("");
  const [modo, setModo] = useState<"senha" | "pin">("senha");
  const [erro, setErro] = useState("");
  // verificação em dois passos: a senha está certa e falta o código da aplicação (ou um de recuperação)
  const [pedirCodigo, setPedirCodigo] = useState(false);
  const [codigo, setCodigo] = useState("");
  const [busy, setBusy] = useState(false);
  // os avisos «Este campo é obrigatório» só aparecem depois de carregar em Entrar
  const [tocado, setTocado] = useState(false);
  const [vista, setVista] = useState<"entrar" | "recuperar" | "perguntas">("entrar");
  const [seccao, setSeccao] = useState<SeccaoAjuda | undefined>();
  const [ocultar, setOcultar] = useOcultarValores();
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
        setTocado(false);
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
    setTocado(true);
    if (pedirCodigo) {
      if (codigo.trim()) entrar("auth.login", { login, senha, codigo });
      return;
    }
    if (!login.trim() || !senha) return;
    entrar("auth.login", { login, senha });
  }

  function recomecar() {
    setPedirCodigo(false);
    setCodigo("");
    setSenha("");
    setErro("");
    setTocado(false);
  }

  const ajuda = (s?: SeccaoAjuda) => {
    setSeccao(s);
    setVista("perguntas");
  };

  useTecladoFisicoPin(vista === "entrar" && comPin && modo === "pin", pin, setPin, () => entrar("auth.loginPin", { pin }));

  if (vista === "recuperar") return <RecuperarSenha loginInicial={login} onVoltar={() => setVista("entrar")} onAjuda={ajuda} />;
  if (vista === "perguntas") return <PerguntasAcesso seccao={seccao} onVoltar={() => setVista("entrar")} onAjuda={ajuda} />;

  return (
    <Moldura empresa={empresa} onAjuda={ajuda} imagemPropria={imagemPropria}>
      <div className="w-full max-w-[440px]">
        <BoasVindas texto={pedirCodigo ? undefined : modo === "pin" && comPin ? "Introduza o seu PIN de operador." : undefined} />

        {comPin && !pedirCodigo && (
          <div className="mb-6 grid grid-cols-2 gap-1 rounded-lg bg-sunken p-1 text-[13px]">
            {(["senha", "pin"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => {
                  setModo(m);
                  setErro("");
                  setTocado(false);
                }}
                className={cn("rounded-md py-1.5", modo === m ? "bg-surface font-medium shadow-sm" : "text-ink-2")}
              >
                {m === "senha" ? "Utilizador e senha" : "PIN"}
              </button>
            ))}
          </div>
        )}

        {pedirCodigo ? (
          <form noValidate onSubmit={submeter} className="space-y-5">
            <div className="flex gap-3 rounded-lg bg-brand-50 p-3 text-[13px] text-ink-2">
              <ShieldCheck className="mt-0.5 size-4 shrink-0 text-brand" />
              <span>Verificação em dois passos: escreva o código de 6 algarismos que a aplicação de autenticação mostra agora (ou um dos códigos de recuperação).</span>
            </div>
            <CampoAcesso label="Código" erro={tocado && !codigo.trim() && OBRIGATORIO}>
              <InputAcesso
                autoFocus
                inputMode="numeric"
                autoComplete="one-time-code"
                value={codigo}
                onChange={(e) => setCodigo(e.target.value.slice(0, 12))}
                invalido={tocado && !codigo.trim()}
                className="text-center font-mono text-[20px] tracking-[0.3em]"
                placeholder="000000"
              />
            </CampoAcesso>
            <Button variant="accent" size="lg" className="h-12 w-full text-[15px]" loading={busy}>
              Confirmar <ArrowRight className="size-4" />
            </Button>
            <button type="button" onClick={recomecar} className="w-full text-[13px] text-ink-3 hover:text-ink">
              Entrar com outro utilizador
            </button>
          </form>
        ) : modo === "pin" && comPin ? (
          <TecladoPin valor={pin} onChange={setPin} ocupado={busy} onConfirmar={() => entrar("auth.loginPin", { pin })} />
        ) : (
          <form noValidate onSubmit={submeter} className="space-y-5">
            <CampoAcesso label="Utilizador" erro={tocado && !login.trim() && OBRIGATORIO}>
              <InputAcesso
                autoFocus
                autoComplete="username"
                autoCapitalize="off"
                spellCheck={false}
                value={login}
                onChange={(e) => setLogin(e.target.value)}
                invalido={tocado && !login.trim()}
              />
            </CampoAcesso>
            <CampoAcesso label="Senha" erro={tocado && !senha && OBRIGATORIO}>
              <InputSenha autoComplete="current-password" value={senha} onChange={(e) => setSenha(e.target.value)} invalido={tocado && !senha} />
            </CampoAcesso>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <Interruptor ligado={ocultar} onChange={setOcultar} rotulo="Ocultar valores" dica="Esconde os valores em kwanzas do painel neste aparelho" />
              <button type="button" onClick={() => setVista("recuperar")} className="text-[14px] font-medium text-ink underline underline-offset-2 hover:text-brand">
                Esqueci-me da senha
              </button>
            </div>
            <Button variant="accent" size="lg" className="h-12 w-full text-[15px]" loading={busy}>
              Entrar <ArrowRight className="size-4" />
            </Button>
          </form>
        )}

        {erro && <p className="mt-4 rounded-lg bg-danger-50 px-3 py-2 text-[13px] text-danger">{erro}</p>}
      </div>
    </Moldura>
  );
}

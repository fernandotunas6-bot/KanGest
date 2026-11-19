import { useState, type FormEvent, type ReactNode } from "react";
import { CheckCircle2, ShieldCheck, Smartphone } from "lucide-react";
import { call } from "@/lib/api";
import { Button } from "@/components/ui";
import { CampoFlutuante, PaginaAcesso, Passos, type SeccaoAjuda } from "@/components/Acesso";
import { usePolitica } from "@/components/Sessao";

const PASSOS = ["Identificação", "Verificar a operação", "Nova senha"];

/**
 * «Esqueci-me da senha»: 1) o utilizador, 2) o código da aplicação de autenticação (ou um código de recuperação),
 * conferido no servidor, 3) a senha nova. Só para contas com a verificação em dois passos (docs/UTILIZADORES.md);
 * as outras pedem ao administrador uma senha provisória. Pela Internet não se faz.
 */
export default function RecuperarSenha({ loginInicial = "", onVoltar, onAjuda }: { loginInicial?: string; onVoltar: () => void; onAjuda: (s?: SeccaoAjuda) => void }) {
  const politica = usePolitica();
  const minimo = politica.data?.senha_min ?? 6;
  const [passo, setPasso] = useState(0);
  const [feito, setFeito] = useState(false);
  const [login, setLogin] = useState(loginInicial);
  const [codigo, setCodigo] = useState("");
  const [nova, setNova] = useState("");
  const [confirmar, setConfirmar] = useState("");
  const [autorizacao, setAutorizacao] = useState<{ token: string; nome: string; minutos: number } | null>(null);
  const [tocado, setTocado] = useState(false);
  const [erro, setErro] = useState("");
  const [busy, setBusy] = useState(false);

  const falta = (v: string) => tocado && !v.trim();

  async function continuar(e: FormEvent) {
    e.preventDefault();
    setTocado(true);
    setErro("");
    if (passo === 0) {
      if (!login.trim()) return;
      setTocado(false);
      setPasso(1);
      return;
    }
    if (passo === 1) {
      if (!codigo.trim()) return;
      setBusy(true);
      try {
        const r = await call<{ token: string; nome: string; minutos: number }>("auth.recuperarVerificar", { login: login.trim(), codigo: codigo.trim() });
        setAutorizacao(r);
        setTocado(false);
        setPasso(2);
      } catch (err) {
        setErro((err as Error).message);
        setCodigo("");
        // o campo ficou vazio por causa do erro, não por esquecimento: só a mensagem do servidor
        setTocado(false);
      } finally {
        setBusy(false);
      }
      return;
    }
    if (!nova || !confirmar) return;
    if (nova !== confirmar) return setErro("A confirmação não coincide com a nova senha.");
    setBusy(true);
    try {
      await call("auth.recuperarConcluir", { token: autorizacao?.token ?? "", nova });
      setFeito(true);
    } catch (err) {
      const m = (err as Error).message;
      setErro(m);
      setTocado(false);
      // a autorização acabou (10 minutos): volta-se a pedir o código
      if (/expirou/i.test(m)) {
        setAutorizacao(null);
        setCodigo("");
        setPasso(1);
      }
    } finally {
      setBusy(false);
    }
  }

  function recuar() {
    setErro("");
    setTocado(false);
    if (passo === 0) return onVoltar();
    setPasso(passo - 1);
  }

  return (
    <PaginaAcesso titulo="Esqueceu-se da senha?" onVoltar={onVoltar} onAjuda={onAjuda}>
      <div className="mx-auto w-full max-w-5xl">
        {feito ? (
          <div className="mx-auto max-w-md py-6 text-center">
            <CheckCircle2 className="mx-auto size-14 text-brand" />
            <h2 className="mt-4 text-[22px] font-semibold">Senha alterada{autorizacao?.nome ? `, ${autorizacao.nome.split(" ")[0]}` : ""}.</h2>
            <p className="mt-2 text-ink-3">Já pode entrar com a nova senha. O código de dois passos continua a ser pedido, como sempre.</p>
            <Button variant="accent" size="lg" className="mt-7 h-12 w-full max-w-[330px] text-[15px]" onClick={onVoltar}>
              Entrar
            </Button>
          </div>
        ) : (
          <>
            <Passos passos={PASSOS} atual={passo} />
            <form noValidate onSubmit={continuar} className="mt-8">
              {passo === 0 && (
                <div className="grid gap-5 md:grid-cols-2">
                  <div>
                    <CampoFlutuante
                      label="Utilizador"
                      autoFocus
                      autoComplete="username"
                      autoCapitalize="off"
                      value={login}
                      onChange={(e) => setLogin(e.target.value)}
                      invalido={falta(login)}
                    />
                    {falta(login) && <p className="mt-1.5 text-[13px] text-danger">Este campo é obrigatório</p>}
                  </div>
                  <Nota icone={<ShieldCheck className="size-5" />}>
                    A recuperação aqui usa a <b>verificação em dois passos</b> da sua conta. Se não a tem ligada, peça ao
                    administrador uma senha provisória (Sistema → Utilizadores e Permissões → Redefinir senha).
                  </Nota>
                </div>
              )}
              {passo === 1 && (
                <div className="grid gap-5 md:grid-cols-2">
                  <div>
                    <CampoFlutuante
                      label="Código de verificação"
                      autoFocus
                      autoComplete="one-time-code"
                      autoCapitalize="off"
                      spellCheck={false}
                      value={codigo}
                      onChange={(e) => setCodigo(e.target.value.slice(0, 12))}
                      invalido={falta(codigo)}
                      className="[&_input]:font-mono [&_input]:tracking-[0.2em]"
                    />
                    {falta(codigo) && <p className="mt-1.5 text-[13px] text-danger">Este campo é obrigatório</p>}
                  </div>
                  <Nota icone={<Smartphone className="size-5" />}>
                    Abra a aplicação de autenticação e escreva o código de 6 algarismos do KanGest para <b>{login.trim()}</b>. Também
                    serve um dos códigos de recuperação (por exemplo, abcd-2345), que fica gasto.
                  </Nota>
                </div>
              )}
              {passo === 2 && (
                <>
                  <p className="mb-5 text-ink-2">
                    Olá, {autorizacao?.nome.split(" ")[0] ?? login}. Escolha a nova senha (tem {autorizacao?.minutos ?? 10} minutos).
                  </p>
                  <div className="grid gap-5 md:grid-cols-2">
                    <div>
                      <CampoFlutuante label="Nova senha" senha autoFocus autoComplete="new-password" value={nova} onChange={(e) => setNova(e.target.value)} invalido={falta(nova)} />
                      <p className={falta(nova) ? "mt-1.5 text-[13px] text-danger" : "mt-1.5 text-[12.5px] text-ink-3"}>
                        {falta(nova) ? "Este campo é obrigatório" : `Pelo menos ${minimo} caracteres.`}
                      </p>
                    </div>
                    <div>
                      <CampoFlutuante
                        label="Confirmar a nova senha"
                        senha
                        autoComplete="new-password"
                        value={confirmar}
                        onChange={(e) => setConfirmar(e.target.value)}
                        invalido={falta(confirmar)}
                      />
                      {falta(confirmar) && <p className="mt-1.5 text-[13px] text-danger">Este campo é obrigatório</p>}
                    </div>
                  </div>
                </>
              )}
              {erro && <p className="mx-auto mt-6 max-w-xl rounded-lg bg-danger-50 px-3 py-2 text-center text-[13px] text-danger">{erro}</p>}
              <div className="mt-10 flex flex-col items-center gap-3">
                <Button variant="accent" size="lg" className="h-12 w-full max-w-[330px] text-[15px]" loading={busy}>
                  {passo === 2 ? "Mudar a senha" : "Continuar"}
                </Button>
                <button type="button" onClick={recuar} className="text-[13.5px] text-ink-2 underline underline-offset-2 hover:text-ink">
                  {passo === 0 ? "Voltar à entrada" : "Voltar"}
                </button>
              </div>
            </form>
          </>
        )}
      </div>
    </PaginaAcesso>
  );
}

function Nota({ icone, children }: { icone: ReactNode; children: ReactNode }) {
  return (
    <div className="flex gap-3 rounded-lg border border-line bg-surface p-4 text-[13.5px] leading-relaxed text-ink-2">
      <span className="mt-0.5 shrink-0 text-brand">{icone}</span>
      <p>{children}</p>
    </div>
  );
}

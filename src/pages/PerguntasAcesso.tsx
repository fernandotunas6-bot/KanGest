import { useEffect } from "react";
import { ChevronDown } from "lucide-react";
import { PaginaAcesso, type SeccaoAjuda } from "@/components/Acesso";

/** As respostas seguem docs/UTILIZADORES.md e docs/WEB-PWA.md; quando o comportamento mudar, mudam as duas. */
const GRUPOS: { id: SeccaoAjuda; titulo: string; perguntas: [string, string][] }[] = [
  {
    id: "entrar",
    titulo: "Entrar",
    perguntas: [
      [
        "Como entro no KanGest?",
        "Com o utilizador e a senha que o administrador lhe deu. Se lhe deu uma senha provisória, o KanGest pede logo uma senha só sua. Nos postos da loja com a entrada por PIN ligada, pode entrar só com o PIN no teclado numérico.",
      ],
      [
        "Esqueci-me da senha. O que faço?",
        "Se tem a verificação em dois passos ligada, carregue em «Esqueci-me da senha» no ecrã de entrada: confirma com o código da aplicação de autenticação (ou um código de recuperação) e escolhe uma senha nova. Sem ela, peça ao administrador: em Sistema → Utilizadores e Permissões, «Redefinir senha» dá-lhe uma senha provisória.",
      ],
      [
        "A minha conta ficou bloqueada. Como a desbloqueio?",
        "Depois de várias tentativas falhadas seguidas (5, por omissão), a conta fica bloqueada durante alguns minutos (15, por omissão); a mensagem diz até que horas. Espere esse tempo, ou peça ao administrador para a desbloquear na lista de utilizadores.",
      ],
      [
        "O que é o PIN de operador?",
        "Um código curto para os operadores de caixa entrarem depressa e retomarem a sessão nos postos da loja. Define-se na ficha do utilizador. Pela Internet não se usa: aí entra-se sempre com o utilizador e a senha.",
      ],
      [
        "O que é a verificação em dois passos?",
        "Além da senha, o KanGest pede o código de 6 algarismos de uma aplicação de autenticação (Google Authenticator, Microsoft Authenticator…). Liga-se no menu do utilizador → Verificação em dois passos; guarde os códigos de recuperação num sítio seguro — servem quando o telemóvel não está à mão.",
      ],
    ],
  },
  {
    id: "aparelhos",
    titulo: "Aparelhos e rede",
    perguntas: [
      [
        "O que é a chave de rede?",
        "É a chave que liga um telemóvel, tablet, terminal POS ou outro computador ao servidor KanGest da loja. Encontra-a no computador servidor, em Configurações → Rede. Só se pede da primeira vez em cada aparelho.",
      ],
      [
        "Estou fora da loja. Posso entrar?",
        "Sim, se a loja tiver o acesso pela Internet ligado (Configurações → Rede → Acesso pela Internet): abra o endereço da empresa e entre com o utilizador e a senha. O computador servidor tem de estar ligado e com Internet; o administrador pode exigir a verificação em dois passos a quem entra assim.",
      ],
      [
        "O que faz «Ocultar valores»?",
        "Esconde os valores em kwanzas do painel (vendas, contas a receber e a pagar, recebimentos), para quem está do outro lado do balcão não os ver. Fica guardado neste aparelho; volta-se a mostrá-los no próprio painel, no botão «Mostrar valores».",
      ],
    ],
  },
  {
    id: "seguranca",
    titulo: "Segurança",
    perguntas: [
      [
        "Como protejo a minha conta?",
        "Use uma senha só sua e não a partilhe; ligue a verificação em dois passos; bloqueie a sessão quando se afastar do posto (o cadeado no menu ou na barra do POS) e termine a sessão em aparelhos que não são seus.",
      ],
      [
        "Os dados ficam guardados no telemóvel ou no browser?",
        "Não. Os dados ficam só no servidor da loja. No browser, cada pedido vai cifrado com a chave de rede (também pela Internet, por cima do HTTPS); a chave em si não fica guardada no aparelho, e «Desligar este aparelho» apaga a ligação.",
      ],
      [
        "Recebi um pedido para dar a minha senha. É do KanGest?",
        "Não. O KanGest, o administrador e o suporte nunca pedem a sua senha nem os códigos de dois passos — nem por telefone, email ou WhatsApp. Entre sempre pelo programa ou pelo endereço da sua empresa, e mude a senha se a tiver dado a alguém.",
      ],
    ],
  },
];

/** Perguntas frequentes do ecrã de entrada, por grupos; abre no grupo pedido (ex.: «Segurança», no rodapé). */
export default function PerguntasAcesso({ seccao, onVoltar, onAjuda }: { seccao?: SeccaoAjuda; onVoltar: () => void; onAjuda: (s?: SeccaoAjuda) => void }) {
  useEffect(() => {
    if (seccao) document.getElementById(`ajuda-${seccao}`)?.scrollIntoView({ block: "start" });
  }, [seccao]);

  return (
    <PaginaAcesso titulo="Perguntas frequentes" variante="escura" onVoltar={onVoltar} onAjuda={onAjuda}>
      <div className="mx-auto w-full max-w-5xl space-y-10">
        {GRUPOS.map((g) => (
          <section key={g.id} id={`ajuda-${g.id}`} className="scroll-mt-6">
            <h2 className="mb-4 text-[22px] font-semibold">{g.titulo}</h2>
            <div className="rounded-md border border-line bg-surface px-4">
              {g.perguntas.map(([p, r], i) => (
                <details key={p} open={g.id === seccao && i === 0} className="group border-b border-line last:border-b-0">
                  <summary className="flex cursor-pointer list-none items-center justify-between gap-4 py-4 text-[15px] [&::-webkit-details-marker]:hidden">
                    {p}
                    <ChevronDown className="size-5 shrink-0 text-ink-2 transition-transform group-open:rotate-180" />
                  </summary>
                  <p className="pb-5 pr-9 text-[14px] leading-relaxed text-ink-2">{r}</p>
                </details>
              ))}
            </div>
          </section>
        ))}
      </div>
    </PaginaAcesso>
  );
}

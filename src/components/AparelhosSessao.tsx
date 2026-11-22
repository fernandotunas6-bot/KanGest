// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Globe, Monitor, Smartphone } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora } from "@/lib/format";
import { useSession } from "@/store/session";
import { Badge, Button, Modal, Table, Td, Th, cn } from "@/components/ui";

interface SessaoAparelho {
  id: string;
  utilizador_id: number;
  utilizador: string;
  aparelho: string;
  posto: string;
  local: string;
  endereco: string;
  entrada: string | null;
  ultimo: string | null;
  pela_internet: boolean;
  actual: boolean;
}

function Icone({ s }: { s: SessaoAparelho }) {
  const I = /iPhone|Android|iPad/.test(s.aparelho) ? Smartphone : s.pela_internet ? Globe : Monitor;
  return <I className="size-4 shrink-0 text-ink-3" />;
}

/**
 * «Aparelhos com sessão», como o «Onde você fez login» do Claude: este computador e cada posto ou browser ligado ao
 * servidor com a conta, onde está (pela Internet, o local aproximado dito pela Cloudflare) e quando entrou. Termina-se
 * uma sessão, ou todas menos esta («Sair em todos os aparelhos»). Mudar a senha também termina as outras.
 */
export default function AparelhosSessao({ onClose }: { onClose: () => void }) {
  const tem = useSession((s) => s.tem);
  const qc = useQueryClient();
  const [todos, setTodos] = useState(false);
  const [confirmar, setConfirmar] = useState(false);
  const q = useQuery({
    queryKey: ["auth.sessoes", todos],
    queryFn: () => call<{ sessoes: SessaoAparelho[] }>("auth.sessoes", { todos }),
    refetchInterval: 30_000,
  });
  const lista = q.data?.sessoes ?? [];
  const outras = lista.filter((s) => !s.actual).length;
  const recarregar = () => qc.invalidateQueries({ queryKey: ["auth.sessoes"] });

  const terminar = async (s: SessaoAparelho) => {
    try {
      await call("auth.terminarSessao", { id: s.id });
      toast.success(`Sessão terminada: ${s.aparelho}.`);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
    void recarregar();
  };
  const sairEmTodos = async () => {
    if (!confirmar) return setConfirmar(true);
    setConfirmar(false);
    try {
      const r = await call<{ terminadas: number }>("auth.terminarOutras");
      toast.success(r.terminadas ? `${r.terminadas} ${r.terminadas === 1 ? "sessão terminada" : "sessões terminadas"}. Fica só esta.` : "Não havia outras sessões.");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    }
    void recarregar();
  };

  return (
    <Modal open onClose={onClose} title="Aparelhos com sessão" width="max-w-4xl">
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <p className="min-w-0 flex-1 text-sm text-ink-3">
          {todos ? "Onde há sessão iniciada neste servidor KanGest, de toda a gente." : "Onde a sua conta tem sessão iniciada neste servidor KanGest."}
        </p>
        {tem("sistema.utilizadores") && (
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={todos} onChange={(e) => setTodos(e.target.checked)} /> De toda a gente
          </label>
        )}
        {!todos && (
          <Button variant={confirmar ? "danger" : "secondary"} onClick={sairEmTodos} disabled={outras === 0} onBlur={() => setConfirmar(false)}>
            {confirmar ? `Confirmar: terminar ${outras} ${outras === 1 ? "sessão" : "sessões"}` : "Sair em todos os aparelhos"}
          </Button>
        )}
      </div>
      <Table>
        <thead>
          <tr>
            <Th>Aparelho</Th>
            {todos && <Th>Utilizador</Th>}
            <Th>Local</Th>
            <Th>Entrada</Th>
            <Th>Última actividade</Th>
            <Th />
          </tr>
        </thead>
        <tbody>
          {lista.map((s) => (
            <tr key={s.id} className={cn(s.actual && "bg-brand-50/60")}>
              <Td>
                <div className="flex items-center gap-2">
                  <Icone s={s} />
                  <span className="font-medium">{s.aparelho}</span>
                  {s.actual && <Badge tone="brand">Actual</Badge>}
                </div>
                {s.posto && !s.aparelho.includes(s.posto) && <div className="pl-6 text-xs text-ink-3">{s.posto}</div>}
              </Td>
              {todos && <Td>{s.utilizador}</Td>}
              <Td>
                <span title={s.endereco || undefined}>{s.local}</span>
              </Td>
              <Td className="whitespace-nowrap">{s.entrada ? dataHora(s.entrada) : "—"}</Td>
              <Td className="whitespace-nowrap">{s.actual ? "agora" : s.ultimo ? dataHora(s.ultimo) : "—"}</Td>
              <Td className="text-right">
                {!s.actual && (
                  <Button size="sm" variant="ghost" onClick={() => terminar(s)}>
                    Terminar
                  </Button>
                )}
              </Td>
            </tr>
          ))}
          {!q.isLoading && lista.length === 0 && (
            <tr>
              <Td colSpan={todos ? 6 : 5} className="py-6 text-center text-ink-3">
                Não há sessões iniciadas.
              </Td>
            </tr>
          )}
        </tbody>
      </Table>
      <p className="mt-4 text-xs text-ink-3">
        Mudar a senha termina as sessões nos outros aparelhos. Um posto ou browser sem uso há 12 horas sai sozinho. O local de quem entra pela
        Internet é aproximado (dito pela Cloudflare).
      </p>
    </Modal>
  );
}

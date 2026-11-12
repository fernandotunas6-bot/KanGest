import { useEffect, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { ChevronDown, KeyRound, LayoutDashboard, Lock, LogOut, ShoppingCart } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { call } from "@/lib/api";
import { MENU, type ItemMenu } from "@/lib/menu";
import { useSession } from "@/store/session";
import { Button, Field, Input, Modal, cn } from "@/components/ui";
import { Logo } from "@/components/Logo";
import { bloquearSessao } from "@/components/Sessao";
import type { Sessao } from "@/lib/types";

const CHAVE_ABERTOS = "kigest.menu.abertos";

function lerAbertos(): string[] {
  try {
    return JSON.parse(localStorage.getItem(CHAVE_ABERTOS) ?? '["vendas"]');
  } catch {
    return ["vendas"];
  }
}

export default function AppShell() {
  const { sessao, setSessao, pode, tem } = useSession();
  const nav = useNavigate();
  const loc = useLocation();
  const qc = useQueryClient();
  const [senha, setSenha] = useState(false);
  const [abertos, setAbertos] = useState<string[]>(lerAbertos);

  const visivel = (i: ItemMenu) => (!i.perm || tem(i.perm)) && (!i.minPerfil || pode(i.minPerfil));
  const atual = loc.pathname + loc.search;
  // um item com parâmetros (ex.: Lista de Preços = /relatorios/stock?situacao=precos) que corresponde
  // exactamente ao endereço tem prioridade sobre o item simples do mesmo ecrã
  const exacto = MENU.some((g) => g.itens.some((i) => i.to?.includes("?") && i.to === atual));
  const ativo = (to?: string) => !!to && (to.includes("?") ? atual === to : loc.pathname === to && !exacto);

  // abre automaticamente o grupo da página actual
  useEffect(() => {
    const g = MENU.find((g) => g.itens.some((i) => ativo(i.to)));
    if (g && !abertos.includes(g.id)) setAbertos((a) => [...a, g.id]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [atual]);

  useEffect(() => {
    try {
      localStorage.setItem(CHAVE_ABERTOS, JSON.stringify(abertos));
    } catch {
      /* armazenamento indisponível */
    }
  }, [abertos]);

  async function sair() {
    await call("auth.logout");
    qc.clear();
    setSessao(null);
  }

  return (
    <div className="flex h-full">
      <aside className="flex w-[248px] shrink-0 flex-col bg-brand-700 text-white/85">
        <div className="flex items-center gap-2.5 px-5 pb-5 pt-5">
          <Logo className="size-9 rounded-[22%] ring-1 ring-white/15" />
          <div className="leading-tight">
            <div className="font-semibold text-white">KanGest</div>
            <div className="text-[11px] text-white/55">Gestão Comercial</div>
          </div>
        </div>

        <div className="px-3">
          <button
            onClick={() => nav("/pos")}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-accent px-3 py-2.5 font-semibold text-ink shadow-sm transition hover:brightness-105"
          >
            <ShoppingCart className="size-[18px]" /> Abrir POS
          </button>
        </div>

        <nav className="mt-4 flex-1 overflow-y-auto px-3 pb-3">
          {pode("GESTOR") && (
            <NavLink
              to="/"
              end
              className={({ isActive }) =>
                cn("mb-2 flex items-center gap-3 rounded-lg px-3 py-2 transition", isActive ? "bg-white/12 font-medium text-white" : "hover:bg-white/6 hover:text-white")
              }
            >
              <LayoutDashboard className="size-[18px] opacity-90" /> Painel
            </NavLink>
          )}
          {MENU.map((g) => {
            const itens = g.itens.filter(visivel);
            if (!itens.length) return null;
            const aberto = abertos.includes(g.id);
            const Icon = g.icon;
            const temAtivo = itens.some((i) => ativo(i.to));
            return (
              <div key={g.id} className="mb-0.5">
                <button
                  onClick={() => setAbertos((a) => (aberto ? a.filter((x) => x !== g.id) : [...a, g.id]))}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition hover:bg-white/6 hover:text-white",
                    temAtivo && "text-white",
                  )}
                >
                  <Icon className="size-[18px] opacity-90" />
                  <span className="flex-1 text-[13.5px] font-medium">{g.titulo}</span>
                  <ChevronDown className={cn("size-4 opacity-60 transition-transform", aberto && "rotate-180")} />
                </button>
                {aberto && (
                  <div className="mb-1 ml-[21px] border-l border-white/12 pl-2">
                    {itens.map((i) =>
                      i.emBreve || !i.to ? (
                        <div key={i.label} className="flex items-center justify-between rounded-md px-2.5 py-1.5 text-[13px] text-white/35" title="Disponível numa próxima versão">
                          {i.label}
                          <span className="rounded bg-white/8 px-1.5 text-[10px] uppercase tracking-wide">brevemente</span>
                        </div>
                      ) : (
                        <NavLink
                          key={i.label}
                          to={i.to}
                          className={() =>
                            cn(
                              "block rounded-md px-2.5 py-1.5 text-[13px] transition",
                              ativo(i.to) ? "bg-white/12 font-medium text-white" : "text-white/70 hover:bg-white/6 hover:text-white",
                            )
                          }
                        >
                          {i.label}
                        </NavLink>
                      ),
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </nav>

        <div className="m-3 mt-0 flex items-center gap-2 rounded-xl bg-white/6 p-3">
          <div className="grid size-8 place-items-center rounded-full bg-white/15 text-[13px] font-semibold text-white">{sessao?.nome.slice(0, 1).toUpperCase()}</div>
          <div className="min-w-0 flex-1 leading-tight">
            <div className="truncate text-[13px] font-medium text-white">{sessao?.nome}</div>
            <div className="truncate text-[11px] text-white/50">{sessao?.perfil_descricao ?? sessao?.perfil}</div>
          </div>
          <button onClick={() => setSenha(true)} title="Alterar senha" className="rounded-md p-1.5 text-white/60 hover:bg-white/10 hover:text-white">
            <KeyRound className="size-4" />
          </button>
          <button onClick={bloquearSessao} title="Bloquear a sessão (retoma-se com a senha ou o PIN)" className="rounded-md p-1.5 text-white/60 hover:bg-white/10 hover:text-white">
            <Lock className="size-4" />
          </button>
          <button onClick={sair} title="Terminar sessão" className="rounded-md p-1.5 text-white/60 hover:bg-white/10 hover:text-white">
            <LogOut className="size-4" />
          </button>
        </div>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-[1400px] px-8 py-7">
          <Outlet />
        </div>
      </main>
      {senha && <AlterarSenha onClose={() => setSenha(false)} />}
    </div>
  );
}

function AlterarSenha({ onClose }: { onClose: () => void }) {
  const [f, setF] = useState({ atual: "", nova: "", confirmar: "" });
  const setSessao = useSession((s) => s.setSessao);
  async function guardar() {
    if (f.nova !== f.confirmar) return toast.error("As senhas não coincidem.");
    try {
      setSessao(await call<Sessao>("auth.alterarSenha", { atual: f.atual, nova: f.nova }));
      toast.success("Senha alterada.");
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title="Alterar senha" width="max-w-sm" footer={<Button onClick={guardar}>Guardar</Button>}>
      <div className="space-y-4">
        <Field label="Senha actual">
          <Input autoFocus type="password" value={f.atual} onChange={(e) => setF({ ...f, atual: e.target.value })} />
        </Field>
        <Field label="Nova senha">
          <Input type="password" value={f.nova} onChange={(e) => setF({ ...f, nova: e.target.value })} />
        </Field>
        <Field label="Confirmar nova senha">
          <Input type="password" value={f.confirmar} onChange={(e) => setF({ ...f, confirmar: e.target.value })} />
        </Field>
      </div>
    </Modal>
  );
}

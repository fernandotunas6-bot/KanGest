import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { ChevronDown, ChevronRight, Hand, Keyboard, KeyRound, LayoutDashboard, Lock, LogOut, Menu as IconeMenu, Monitor, Moon, PanelLeftClose, PanelLeftOpen, Plus, Search, ShieldCheck, ShoppingCart, Star, Sun, UserRound, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { call } from "@/lib/api";
import { MENU, type GrupoMenu, type ItemMenu } from "@/lib/menu";
import { destinoActual, destinoPorEndereco, useNavegacao, type Destino } from "@/lib/navegacao";
import { useAparencia, type Densidade, type Tema, type Toque } from "@/lib/aparencia";
import { useSession } from "@/store/session";
import { Button, Field, Input, MenuItem, Modal, Popover, Segmentado, cn } from "@/components/ui";
import { Logo } from "@/components/Logo";
import { bloquearSessao } from "@/components/Sessao";
import { AvisoLicenca } from "@/components/AvisoLicenca";
import { PaletaComandos } from "@/components/Comandos";
import DocumentoModal from "@/components/DocumentoModal";
import DoisPassos from "@/components/DoisPassos";
import MeuPerfil, { Avatar } from "@/components/MeuPerfil";
import { useMeuPerfil } from "@/lib/perfil";
import type { Sessao } from "@/lib/types";

const CHAVE_ABERTO = "kangest.menu.aberto";
const CHAVE_RECOLHIDO = "kangest.menu.recolhido";

function lerLocal(chave: string, omissao: string): string {
  try {
    return localStorage.getItem(chave) ?? omissao;
  } catch {
    return omissao;
  }
}
function gravarLocal(chave: string, valor: string) {
  try {
    localStorage.setItem(chave, valor);
  } catch {
    /* armazenamento indisponível */
  }
}

/** Documentos que se criam a partir do botão «Novo» da barra superior (os mais usados, como no Odoo e no Primavera). */
const NOVOS = ["/documentos/novo/FT", "/documentos/novo/FR", "/documentos/novo/PP", "/documentos/novo/OR", "/recibos/novo", "/documentos/novo/FC", "/pagamentos/novo", "/documentos/novo/ES"];

const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export default function AppShell() {
  const { sessao, setSessao, pode, tem } = useSession();
  const nav = useNavigate();
  const loc = useLocation();
  const qc = useQueryClient();
  const [senha, setSenha] = useState(false);
  const [doisPassos, setDoisPassos] = useState(false);
  const [meuPerfil, setMeuPerfil] = useState(false);
  const [paleta, setPaleta] = useState(false);
  const [docAberto, setDocAberto] = useState<number | null>(null);
  const [aberto, setAberto] = useState<string | null>(() => lerLocal(CHAVE_ABERTO, "vendas") || null);
  const [recolhido, setRecolhido] = useState(() => lerLocal(CHAVE_RECOLHIDO, "0") === "1");
  // ecrãs estreitos (telemóvel, tablet ao alto): o menu abre por cima, como uma gaveta
  const [gaveta, setGaveta] = useState(false);
  const { favoritos, iniciar, alternarFavorito, registarVisita } = useNavegacao();

  const visivelItem = (i: ItemMenu) => (!i.perm || tem(i.perm)) && (!i.minPerfil || pode(i.minPerfil));
  const visivel = (d: Destino) => (d.to === "/" ? pode("GESTOR") : !d.item || visivelItem(d.item));
  const destino = destinoActual(loc.pathname, loc.search);
  const ativo = (to?: string) => !!to && destino?.to === to;

  useEffect(() => iniciar(sessao?.login ?? null), [sessao?.login, iniciar]);

  // abre o grupo da página actual e guarda a visita (para «Abertos recentemente» na pesquisa)
  useEffect(() => {
    if (destino?.grupo) setAberto(destino.grupo.id);
    if (destino && destinoPorEndereco(destino.to)) registarVisita(destino.to);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [destino?.to]);

  useEffect(() => gravarLocal(CHAVE_ABERTO, aberto ?? ""), [aberto]);
  useEffect(() => setGaveta(false), [loc.pathname, loc.search]);
  useEffect(() => gravarLocal(CHAVE_RECOLHIDO, recolhido ? "1" : "0"), [recolhido]);

  // Ctrl+K (⌘K no Mac) em qualquer ecrã: pesquisa global
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
        if (useSession.getState().sessao?.bloqueada) return;
        e.preventDefault();
        setPaleta((p) => !p);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  async function sair() {
    await call("auth.logout");
    qc.clear();
    setSessao(null);
  }

  const grupos = MENU.map((g) => ({ ...g, itens: g.itens.filter((i) => i.to && !i.emBreve && visivelItem(i)) })).filter((g) => g.itens.length);
  const favs = favoritos.map(destinoPorEndereco).filter((d): d is Destino => !!d && visivel(d));

  /** Menu lateral completo (no ecrã largo ao lado; nos estreitos, numa gaveta por cima). */
  const menuCompleto = (naGaveta: boolean) => (
    <aside className={cn("flex shrink-0 flex-col bg-nav text-white/85", naGaveta ? "h-full w-[280px] max-w-[85vw]" : "w-[252px]")}>
      <div className="flex h-14 shrink-0 items-center gap-2.5 pl-4 pr-2">
        <Logo className="size-8 rounded-[22%] ring-1 ring-white/15" />
        <div className="min-w-0 flex-1 leading-tight">
          <div className="font-semibold text-white">KanGest</div>
          <div className="text-[11px] text-white/55">Gestão Comercial</div>
        </div>
        {naGaveta ? (
          <button onClick={() => setGaveta(false)} title="Fechar o menu" aria-label="Fechar o menu" className="rounded-md p-2 text-white/70 hover:bg-white/10 hover:text-white">
            <X className="size-5" />
          </button>
        ) : (
          <button onClick={() => setRecolhido(true)} title="Recolher o menu (mais espaço para as tabelas)" className="rounded-md p-1.5 text-white/55 hover:bg-white/10 hover:text-white">
            <PanelLeftClose className="size-[18px]" />
          </button>
        )}
      </div>

      <div className="px-3 pb-1">
        <button
          onClick={() => nav("/pos")}
          className="flex w-full items-center justify-center gap-2 rounded-xl bg-accent px-3 py-2.5 font-semibold text-on-accent shadow-sm transition hover:brightness-105"
        >
          <ShoppingCart className="size-[18px]" /> Abrir POS
        </button>
      </div>

      <nav className="mt-3 flex-1 overflow-y-auto px-3 pb-4">
        {pode("GESTOR") && (
          <NavLink
            to="/"
            end
            className={({ isActive }) =>
              cn("mb-1 flex items-center gap-3 rounded-lg px-3 py-2 transition", isActive ? "bg-white/12 font-medium text-white" : "hover:bg-white/6 hover:text-white")
            }
          >
            <LayoutDashboard className="size-[18px] opacity-90" /> Painel
          </NavLink>
        )}

        {favs.length > 0 && (
          <div className="mb-2 mt-3">
            <div className="flex items-center gap-2 px-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wider text-white/40">
              <Star className="size-3" /> Favoritos
            </div>
            {favs.map((d) => (
              <ItemNav key={d.to} d={d} ativo={ativo(d.to)} favorito onFavorito={() => alternarFavorito(d.to)} />
            ))}
          </div>
        )}

        <div className="mt-3 px-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wider text-white/40">Menu</div>
        {grupos.map((g) => {
          const estaAberto = aberto === g.id;
          const Icon = g.icon;
          const temAtivo = g.itens.some((i) => ativo(i.to));
          return (
            <div key={g.id} className="mb-0.5">
              <button
                onClick={() => setAberto(estaAberto ? null : g.id)}
                aria-expanded={estaAberto}
                className={cn("flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition hover:bg-white/6 hover:text-white", (temAtivo || estaAberto) && "text-white")}
              >
                <Icon className="size-[18px] opacity-90" />
                <span className="flex-1 text-[13.5px] font-medium">{g.titulo}</span>
                {temAtivo && !estaAberto && <span className="size-1.5 rounded-full bg-accent" />}
                <ChevronDown className={cn("size-4 opacity-60 transition-transform", estaAberto && "rotate-180")} />
              </button>
              {estaAberto && (
                <div className="mb-1 ml-[21px] border-l border-white/12 pl-2">
                  {g.itens.map((i) => (
                    <ItemNav
                      key={i.label}
                      d={{ to: i.to!, label: i.label, grupo: g, item: i }}
                      ativo={ativo(i.to)}
                      favorito={favoritos.includes(i.to!)}
                      onFavorito={() => alternarFavorito(i.to!)}
                    />
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </nav>
    </aside>
  );

  return (
    <div className="flex h-full">
      <div className="hidden lg:flex">
        {recolhido ? (
          <MenuRecolhido grupos={grupos} favoritos={favs} ativo={ativo} painel={pode("GESTOR")} onExpandir={() => setRecolhido(false)} onPos={() => nav("/pos")} />
        ) : (
          menuCompleto(false)
        )}
      </div>
      {gaveta && (
        <div className="fixed inset-0 z-50 flex lg:hidden">
          <div className="absolute inset-0 bg-scrim backdrop-blur-[1px]" onClick={() => setGaveta(false)} />
          <div className="relative h-full shadow-pop">{menuCompleto(true)}</div>
        </div>
      )}

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center gap-1.5 border-b border-line bg-surface px-2 sm:gap-2 sm:px-4">
          <button onClick={() => setGaveta(true)} className="grid size-10 place-items-center rounded-lg text-ink-2 hover:bg-sunken hover:text-ink lg:hidden" aria-label="Abrir o menu" title="Menu">
            <IconeMenu className="size-5" />
          </button>
          <nav aria-label="Localização" className="flex min-w-0 flex-1 items-center gap-1.5 text-[13px]">
            {destino?.grupo && (
              <>
                <span className="hidden truncate text-ink-3 sm:inline">{destino.grupo.titulo}</span>
                <ChevronRight className="hidden size-3.5 shrink-0 text-ink-3/70 sm:block" />
              </>
            )}
            <span className="truncate font-medium text-ink">{destino?.label ?? "KanGest"}</span>
            {destino && destino.to !== "/" && destinoPorEndereco(destino.to) && (
              <button
                onClick={() => alternarFavorito(destino.to)}
                title={favoritos.includes(destino.to) ? "Remover dos favoritos" : "Adicionar aos favoritos (fica no topo do menu)"}
                className="rounded-md p-1 text-ink-3 hover:bg-sunken hover:text-accent-ink"
              >
                <Star className={cn("size-4", favoritos.includes(destino.to) && "fill-accent text-accent")} />
              </button>
            )}
          </nav>

          <button
            onClick={() => setPaleta(true)}
            className="grid size-10 place-items-center rounded-lg text-ink-2 hover:bg-sunken hover:text-ink md:hidden"
            aria-label="Pesquisar"
            title="Pesquisar ecrãs, documentos, clientes e artigos"
          >
            <Search className="size-[18px]" />
          </button>
          <button
            onClick={() => setPaleta(true)}
            className="hidden h-8 w-[240px] items-center gap-2 rounded-lg border border-line bg-bg px-2.5 text-[13px] text-ink-3 transition hover:border-line-strong hover:text-ink-2 md:flex lg:w-[300px]"
            title="Pesquisar ecrãs, documentos, clientes e artigos"
          >
            <Search className="size-4" />
            <span className="flex-1 text-left">Pesquisar ou ir para…</span>
            <kbd>{mac ? "⌘" : "Ctrl"}</kbd>
            <kbd>K</kbd>
          </button>

          <MenuNovo visivel={(to) => !!destinoPorEndereco(to) && visivel(destinoPorEndereco(to)!)} />
          <span className="hidden sm:contents">
            <BotaoTema />
          </span>
          <MenuUtilizador sessao={sessao} onPerfil={() => setMeuPerfil(true)} onSenha={() => setSenha(true)} onDoisPassos={() => setDoisPassos(true)} onSair={sair} />
        </header>

        <main className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[1400px] px-3 py-4 sm:px-5 lg:px-8 lg:py-6">
            {loc.pathname !== "/licenca" && <AvisoLicenca />}
            <Outlet />
          </div>
        </main>
      </div>

      <PaletaComandos
        aberta={paleta}
        onFechar={() => setPaleta(false)}
        onAbrirDocumento={setDocAberto}
        onAlterarSenha={() => setSenha(true)}
        onSair={sair}
        visivel={visivel}
      />
      <DocumentoModal id={docAberto} onClose={() => setDocAberto(null)} />
      {senha && <AlterarSenha onClose={() => setSenha(false)} />}
      {doisPassos && <DoisPassos onClose={() => setDoisPassos(false)} />}
      {meuPerfil && <MeuPerfil onClose={() => setMeuPerfil(false)} />}
    </div>
  );
}

/** Uma entrada do menu, com a estrela para a pôr (ou tirar) dos favoritos. */
function ItemNav({ d, ativo, favorito, onFavorito }: { d: Destino; ativo: boolean; favorito: boolean; onFavorito: () => void }) {
  return (
    <div className="group/item relative">
      <NavLink
        to={d.to}
        className={cn(
          "block rounded-md py-1.5 pl-2.5 pr-8 text-[13px] transition",
          ativo ? "bg-white/12 font-medium text-white" : "text-white/70 hover:bg-white/6 hover:text-white",
        )}
      >
        {d.label}
      </NavLink>
      <button
        onClick={onFavorito}
        title={favorito ? "Remover dos favoritos" : "Adicionar aos favoritos"}
        className={cn(
          "absolute right-1 top-1/2 -translate-y-1/2 rounded p-1 transition hover:bg-white/10",
          favorito ? "text-accent opacity-90" : "text-white/50 opacity-0 group-hover/item:opacity-100 focus-visible:opacity-100",
        )}
      >
        <Star className={cn("size-3.5", favorito && "fill-accent")} />
      </button>
    </div>
  );
}

/** Menu recolhido: só ícones; cada grupo abre ao lado ao passar o rato (ou ao clicar). */
function MenuRecolhido({
  grupos,
  favoritos,
  ativo,
  painel,
  onExpandir,
  onPos,
}: {
  grupos: GrupoMenu[];
  favoritos: Destino[];
  ativo: (to?: string) => boolean;
  painel: boolean;
  onExpandir: () => void;
  onPos: () => void;
}) {
  const [lado, setLado] = useState<{ id: string; top: number } | null>(null);
  const fecho = useRef<ReturnType<typeof setTimeout>>(undefined);
  const abrir = (id: string, el: HTMLElement) => {
    clearTimeout(fecho.current);
    setLado({ id, top: el.getBoundingClientRect().top });
  };
  const agendarFecho = () => {
    clearTimeout(fecho.current);
    fecho.current = setTimeout(() => setLado(null), 180);
  };
  useEffect(() => () => clearTimeout(fecho.current), []);

  const conteudo =
    lado?.id === "favoritos"
      ? { titulo: "Favoritos", itens: favoritos }
      : (() => {
          const g = grupos.find((x) => x.id === lado?.id);
          return g ? { titulo: g.titulo, itens: g.itens.map((i) => ({ to: i.to!, label: i.label })) } : null;
        })();
  const alturaEstimada = conteudo ? 44 + conteudo.itens.length * 33 : 0;
  const top = lado ? Math.max(8, Math.min(lado.top - 6, window.innerHeight - 8 - alturaEstimada)) : 0;

  const icone = "grid size-10 place-items-center rounded-lg transition";
  return (
    <aside className="flex w-[64px] shrink-0 flex-col items-center bg-nav text-white/80">
      <button onClick={onExpandir} title="Expandir o menu" className="grid h-14 place-items-center">
        <Logo className="size-8 rounded-[22%] ring-1 ring-white/15" />
      </button>
      <button onClick={onPos} title="Abrir POS" className="grid size-10 place-items-center rounded-xl bg-accent text-on-accent shadow-sm hover:brightness-105">
        <ShoppingCart className="size-[18px]" />
      </button>
      <nav className="mt-3 flex w-full flex-1 flex-col items-center gap-1 overflow-y-auto pb-3">
        {painel && (
          <NavLink to="/" end title="Painel" className={({ isActive }) => cn(icone, isActive ? "bg-white/15 text-white" : "hover:bg-white/8 hover:text-white")}>
            <LayoutDashboard className="size-[19px]" />
          </NavLink>
        )}
        {favoritos.length > 0 && (
          <button
            title="Favoritos"
            onMouseEnter={(e) => abrir("favoritos", e.currentTarget)}
            onMouseLeave={agendarFecho}
            onClick={(e) => abrir("favoritos", e.currentTarget)}
            className={cn(icone, lado?.id === "favoritos" ? "bg-white/10 text-white" : "hover:bg-white/8 hover:text-white")}
          >
            <Star className="size-[19px]" />
          </button>
        )}
        <div className="my-1 h-px w-8 bg-white/10" />
        {grupos.map((g) => {
          const Icon = g.icon;
          const temAtivo = g.itens.some((i) => ativo(i.to));
          return (
            <button
              key={g.id}
              title={g.titulo}
              aria-label={g.titulo}
              onMouseEnter={(e) => abrir(g.id, e.currentTarget)}
              onMouseLeave={agendarFecho}
              onClick={(e) => abrir(g.id, e.currentTarget)}
              className={cn(icone, "relative", temAtivo ? "bg-white/15 text-white" : lado?.id === g.id ? "bg-white/10 text-white" : "hover:bg-white/8 hover:text-white")}
            >
              <Icon className="size-[19px]" />
            </button>
          );
        })}
      </nav>
      <button onClick={onExpandir} title="Expandir o menu" className="mb-3 grid size-10 place-items-center rounded-lg text-white/55 hover:bg-white/8 hover:text-white">
        <PanelLeftOpen className="size-[18px]" />
      </button>

      {lado && conteudo && (
        <div
          className="fixed left-[68px] z-40 max-h-[calc(100vh-16px)] w-64 overflow-y-auto rounded-xl border border-line bg-surface p-1.5 text-ink shadow-pop"
          style={{ top }}
          onMouseEnter={() => clearTimeout(fecho.current)}
          onMouseLeave={agendarFecho}
        >
          <div className="px-3 pb-1 pt-1.5 text-[11px] font-medium uppercase tracking-wide text-ink-3">{conteudo.titulo}</div>
          {conteudo.itens.map((i) => (
            <NavLink
              key={i.to}
              to={i.to}
              onClick={() => setLado(null)}
              className={cn("block rounded-lg px-3 py-1.5 text-[13px] transition", ativo(i.to) ? "bg-brand-50 font-medium text-brand-700" : "hover:bg-sunken")}
            >
              {i.label}
            </NavLink>
          ))}
        </div>
      )}
    </aside>
  );
}

/** «Novo»: criar os documentos mais usados sem procurar no menu. */
function MenuNovo({ visivel }: { visivel: (to: string) => boolean }) {
  const nav = useNavigate();
  const itens = NOVOS.filter(visivel).map((to) => destinoPorEndereco(to)!);
  if (!itens.length) return null;
  return (
    <Popover
      className="w-64"
      botao={({ alternar }) => (
        <Button size="sm" onClick={alternar} className="h-8 max-sm:px-2.5" aria-label="Novo documento" title="Novo documento">
          <Plus className="size-4" /> <span className="max-sm:hidden">Novo</span> <ChevronDown className="size-3.5 opacity-80 max-sm:hidden" />
        </Button>
      )}
    >
      {(fechar) =>
        itens.map((d, k) => (
          <div key={d.to}>
            {(k === 0 || itens[k - 1].grupo?.id !== d.grupo?.id) && (
              <div className="px-3 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-ink-3">{d.grupo?.titulo}</div>
            )}
            <MenuItem
              icon={d.grupo?.icon}
              onClick={() => {
                fechar();
                nav(d.to);
              }}
            >
              {d.label}
            </MenuItem>
          </div>
        ))
      }
    </Popover>
  );
}

function BotaoTema() {
  const { temaEfectivo, setTema } = useAparencia();
  const escuro = temaEfectivo === "escuro";
  return (
    <button
      onClick={() => setTema(escuro ? "claro" : "escuro")}
      title={escuro ? "Tema claro" : "Tema escuro"}
      className="grid size-8 place-items-center rounded-lg text-ink-2 transition hover:bg-sunken hover:text-ink"
    >
      {escuro ? <Sun className="size-[18px]" /> : <Moon className="size-[18px]" />}
    </button>
  );
}

const TEMAS: { id: Tema; label: string; icon: typeof Sun }[] = [
  { id: "claro", label: "Claro", icon: Sun },
  { id: "escuro", label: "Escuro", icon: Moon },
  { id: "sistema", label: "Sistema", icon: Monitor },
];
const TOQUES: { id: Toque; label: string }[] = [
  { id: "auto", label: "Automático" },
  { id: "sim", label: "Ligado" },
  { id: "nao", label: "Desligado" },
];
const DENSIDADES: { id: Densidade; label: string }[] = [
  { id: "confortavel", label: "Confortável" },
  { id: "compacta", label: "Compacta" },
];

function MenuUtilizador({
  sessao,
  onPerfil,
  onSenha,
  onDoisPassos,
  onSair,
}: {
  sessao: Sessao | null;
  onPerfil: () => void;
  onSenha: () => void;
  onDoisPassos: () => void;
  onSair: () => void;
}) {
  const nav = useNavigate();
  const { tema, setTema, densidade, setDensidade, toque, setToque } = useAparencia();
  const foto = useMeuPerfil().data?.foto;
  return (
    <Popover
      className="w-72"
      botao={({ alternar, aberto }) => (
        <button onClick={alternar} className={cn("flex items-center gap-2 rounded-lg py-1 pl-1 pr-2 transition hover:bg-sunken", aberto && "bg-sunken")} title="Conta e aparência">
          <Avatar nome={sessao?.nome} foto={foto} className="size-7 text-[12px]" />
          <span className="hidden max-w-[150px] truncate text-[13px] font-medium text-ink sm:inline">{sessao?.nome}</span>
          <ChevronDown className="hidden size-3.5 text-ink-3 sm:block" />
        </button>
      )}
    >
      {(fechar) => (
        <>
          <div className="flex items-center gap-3 px-3 pb-3 pt-2">
            <Avatar nome={sessao?.nome} foto={foto} className="size-10 text-[15px]" />
            <div className="min-w-0 leading-tight">
              <div className="truncate font-medium">{sessao?.nome}</div>
              <div className="truncate text-[12px] text-ink-3">
                {sessao?.perfil_descricao ?? sessao?.perfil} · {sessao?.login}
              </div>
            </div>
          </div>
          <div className="my-1 h-px bg-line" />
          <MenuItem
            icon={UserRound}
            onClick={() => {
              fechar();
              onPerfil();
            }}
          >
            O meu perfil
          </MenuItem>
          <MenuItem
            icon={KeyRound}
            onClick={() => {
              fechar();
              onSenha();
            }}
          >
            Alterar senha
          </MenuItem>
          <MenuItem
            icon={ShieldCheck}
            onClick={() => {
              fechar();
              onDoisPassos();
            }}
          >
            Verificação em dois passos
          </MenuItem>
          <MenuItem
            icon={Lock}
            onClick={() => {
              fechar();
              bloquearSessao();
            }}
          >
            Bloquear sessão
          </MenuItem>
          <MenuItem
            icon={Keyboard}
            onClick={() => {
              fechar();
              nav("/sobre?vista=manual");
            }}
          >
            Manual e atalhos
          </MenuItem>
          <div className="my-1 h-px bg-line" />
          <div className="px-3 pb-1.5 pt-2 text-[11px] font-medium uppercase tracking-wide text-ink-3">Tema</div>
          <Segmentado className="mx-2" valor={tema} opcoes={TEMAS} onChange={setTema} />
          <div className="px-3 pb-1.5 pt-3 text-[11px] font-medium uppercase tracking-wide text-ink-3">Tabelas</div>
          <Segmentado className="mx-2" valor={densidade} opcoes={DENSIDADES} onChange={setDensidade} />
          <div className="flex items-center gap-1.5 px-3 pb-1.5 pt-3 text-[11px] font-medium uppercase tracking-wide text-ink-3">
            <Hand className="size-3.5" /> Ecrã táctil
          </div>
          <Segmentado className="mx-2 mb-2" valor={toque} opcoes={TOQUES} onChange={setToque} />
          <div className="my-1 h-px bg-line" />
          <MenuItem icon={LogOut} perigo onClick={onSair}>
            Terminar sessão
          </MenuItem>
        </>
      )}
    </Popover>
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

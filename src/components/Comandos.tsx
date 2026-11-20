// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Clock,
  CornerDownLeft,
  FileText,
  KeyRound,
  LayoutDashboard,
  Lock,
  LogOut,
  Moon,
  Package,
  Rows3,
  Search,
  ShoppingCart,
  Star,
  Sun,
  UserRound,
  type LucideIcon,
} from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, kz, qtd } from "@/lib/format";
import { DESTINOS, PAINEL, destinoPorEndereco, normalizar, pontuar, useNavegacao, type Destino } from "@/lib/navegacao";
import { useAparencia } from "@/lib/aparencia";
import type { Artigo, DocResumo, Entidade } from "@/lib/types";
import { useSession } from "@/store/session";
import { bloquearSessao } from "@/components/Sessao";
import { cn } from "@/components/ui";

interface Item {
  id: string;
  titulo: string;
  detalhe?: string;
  direita?: string;
  icone: LucideIcon;
  executar: () => void;
}

interface Seccao {
  titulo: string;
  itens: Item[];
}

/**
 * Pesquisa global e comandos (Ctrl+K): ir para qualquer ecrã do menu e encontrar documentos, clientes,
 * fornecedores e artigos sem sair do ecrã actual — como a paleta de comandos do Odoo ou o «Diga-me» do
 * Business Central.
 */
export function PaletaComandos({
  aberta,
  onFechar,
  onAbrirDocumento,
  onAlterarSenha,
  onSair,
  visivel,
}: {
  aberta: boolean;
  onFechar: () => void;
  onAbrirDocumento: (id: number) => void;
  onAlterarSenha: () => void;
  onSair: () => void;
  /** o utilizador pode ver este destino (permissões do menu) */
  visivel: (d: Destino) => boolean;
}) {
  if (!aberta) return null;
  return <Paleta onFechar={onFechar} onAbrirDocumento={onAbrirDocumento} onAlterarSenha={onAlterarSenha} onSair={onSair} visivel={visivel} />;
}

function Paleta({
  onFechar,
  onAbrirDocumento,
  onAlterarSenha,
  onSair,
  visivel,
}: {
  onFechar: () => void;
  onAbrirDocumento: (id: number) => void;
  onAlterarSenha: () => void;
  onSair: () => void;
  visivel: (d: Destino) => boolean;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const lista = useRef<HTMLDivElement>(null);
  const nav = useNavigate();
  const { pode, tem } = useSession();
  const { favoritos, recentes } = useNavegacao();
  const { temaEfectivo, setTema, densidade, setDensidade } = useAparencia();
  const dq = useDebounce(q.trim(), 200);
  const remota = dq.length >= 2;

  const docs = useQuery({
    queryKey: ["paleta", "documentos", dq],
    queryFn: () => call<DocResumo[]>("documentos.listar", { q: dq, limite: 6 }),
    enabled: remota,
    staleTime: 30_000,
  });
  const verEntidades = tem("clientes") || tem("fornecedores");
  const ents = useQuery({
    queryKey: ["paleta", "entidades", dq],
    queryFn: () => call<Entidade[]>("entidades.listar", { q: dq }),
    enabled: remota && verEntidades,
    staleTime: 30_000,
  });
  const arts = useQuery({
    queryKey: ["paleta", "artigos", dq],
    queryFn: () => call<Artigo[]>("artigos.listar", { q: dq, limite: 6 }),
    enabled: remota,
    staleTime: 30_000,
  });
  const aCarregar = remota && (docs.isFetching || ents.isFetching || arts.isFetching);

  const fechar = (f: () => void) => () => {
    onFechar();
    f();
  };

  const seccoes = useMemo<Seccao[]>(() => {
    const ir = (d: Destino, icone: LucideIcon = ArrowRight): Item => ({
      id: "ir:" + d.to,
      titulo: d.label,
      detalhe: d.grupo?.titulo,
      icone: d.to === "/" ? LayoutDashboard : (d.grupo?.icon ?? icone),
      executar: fechar(() => nav(d.to)),
    });
    const podeVer = (d: Destino) => (d.to === "/" ? pode("GESTOR") : visivel(d));
    const accoes: Item[] = [
      { id: "a:pos", titulo: "Abrir o POS", detalhe: "Ponto de venda", icone: ShoppingCart, executar: fechar(() => nav("/pos")) },
      {
        id: "a:tema",
        titulo: temaEfectivo === "escuro" ? "Mudar para o tema claro" : "Mudar para o tema escuro",
        detalhe: "Aparência",
        icone: temaEfectivo === "escuro" ? Sun : Moon,
        executar: fechar(() => setTema(temaEfectivo === "escuro" ? "claro" : "escuro")),
      },
      {
        id: "a:densidade",
        titulo: densidade === "compacta" ? "Tabelas confortáveis (linhas mais altas)" : "Tabelas compactas (mais linhas no ecrã)",
        detalhe: "Aparência",
        icone: Rows3,
        executar: fechar(() => setDensidade(densidade === "compacta" ? "confortavel" : "compacta")),
      },
      { id: "a:senha", titulo: "Alterar a minha senha", detalhe: "Sessão", icone: KeyRound, executar: fechar(onAlterarSenha) },
      { id: "a:bloquear", titulo: "Bloquear a sessão", detalhe: "Sessão", icone: Lock, executar: fechar(bloquearSessao) },
      { id: "a:sair", titulo: "Terminar sessão", detalhe: "Sessão", icone: LogOut, executar: fechar(onSair) },
    ];

    if (!q.trim()) {
      const out: Seccao[] = [];
      const fav = favoritos.map(destinoPorEndereco).filter((d): d is Destino => !!d && podeVer(d));
      if (fav.length) out.push({ titulo: "Favoritos", itens: fav.map((d) => ({ ...ir(d), id: "fav:" + d.to, icone: Star })) });
      const rec = recentes
        .map(destinoPorEndereco)
        .filter((d): d is Destino => !!d && podeVer(d) && !favoritos.includes(d.to))
        .slice(0, 5);
      if (rec.length) out.push({ titulo: "Abertos recentemente", itens: rec.map((d) => ({ ...ir(d), id: "rec:" + d.to, icone: Clock })) });
      out.push({ titulo: "Acções", itens: accoes.slice(0, 3) });
      return out;
    }

    const palavras = normalizar(q.trim()).split(/\s+/).filter(Boolean);
    const out: Seccao[] = [];
    const ecras = [PAINEL, ...DESTINOS]
      .filter(podeVer)
      .map((d) => ({ d, p: pontuar(palavras, d.label) * 2 || pontuar(palavras, `${d.label} ${d.grupo?.titulo ?? ""}`) }))
      .filter((x) => x.p > 0)
      .sort((a, b) => b.p - a.p)
      .slice(0, 7)
      .map((x) => ir(x.d));
    if (ecras.length) out.push({ titulo: "Ir para", itens: ecras });

    if (remota) {
      const ds = (docs.data ?? []).map<Item>((d) => ({
        id: "doc:" + d.id,
        titulo: d.numero_doc + (d.estado === "A" ? " (anulado)" : ""),
        detalhe: `${d.tipo_desc} · ${d.entidade_nome} · ${data(d.data_emissao)}`,
        direita: kz(d.total),
        icone: FileText,
        executar: fechar(() => onAbrirDocumento(d.id)),
      }));
      if (ds.length) out.push({ titulo: "Documentos", itens: ds });

      const es = (ents.data ?? [])
        .filter((e) => e.id !== 1 && (e.tipo === "F" ? tem("fornecedores") : tem("clientes")))
        .slice(0, 5)
        .map<Item>((e) => ({
          id: "ent:" + e.id,
          titulo: e.nome,
          detalhe: `${e.tipo === "F" ? "Fornecedor" : e.tipo === "A" ? "Cliente e fornecedor" : "Cliente"} · NIF ${e.nif}`,
          direita: (e.saldo ?? 0) > 0 ? `Em dívida ${kz(e.saldo)}` : undefined,
          icone: UserRound,
          executar: fechar(() => nav(`${e.tipo === "F" ? "/fornecedores" : "/clientes"}?id=${e.id}`)),
        }));
      if (es.length) out.push({ titulo: "Clientes e fornecedores", itens: es });

      const as = (arts.data ?? []).slice(0, 5).map<Item>((a) => ({
        id: "art:" + a.id,
        titulo: a.descricao,
        detalhe: `${a.referencia}${a.tipo === "S" ? " · serviço" : ` · stock ${qtd(a.stock)}`}`,
        direita: kz(a.preco1_iva),
        icone: Package,
        executar: fechar(() => nav(`/artigos?id=${a.id}`)),
      }));
      if (as.length) out.push({ titulo: "Artigos", itens: as });
    }

    const ac = accoes.filter((a) => pontuar(palavras, `${a.titulo} ${a.detalhe ?? ""}`) > 0);
    if (ac.length) out.push({ titulo: "Acções", itens: ac });
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [q, remota, docs.data, ents.data, arts.data, favoritos, recentes, temaEfectivo, densidade]);

  const itens = seccoes.flatMap((s) => s.itens);
  useEffect(() => setSel(0), [q]);
  useEffect(() => {
    lista.current?.querySelector(`[data-indice="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  function tecla(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => (itens.length ? (s + 1) % itens.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => (itens.length ? (s - 1 + itens.length) % itens.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      itens[sel]?.executar();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onFechar();
    }
  }

  let indice = -1;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-scrim p-6 pt-[12vh] backdrop-blur-[2px]" onMouseDown={onFechar}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Pesquisar ou ir para"
        className="flex max-h-[70vh] w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-pop"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-3 border-b border-line px-4">
          <Search className="size-[18px] shrink-0 text-ink-3" />
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={tecla}
            placeholder="Ir para um ecrã, documento (FT 2026/12), cliente, NIF ou artigo…"
            className="h-14 min-w-0 flex-1 bg-transparent text-[15px] text-ink outline-none placeholder:text-ink-3 focus-visible:outline-none"
            aria-controls="paleta-resultados"
          />
          {aCarregar && <span className="size-4 shrink-0 animate-spin rounded-full border-2 border-ink-3 border-t-transparent" />}
          <kbd>Esc</kbd>
        </div>
        <div ref={lista} id="paleta-resultados" role="listbox" className="flex-1 overflow-y-auto p-2">
          {seccoes.map((s) => (
            <div key={s.titulo} className="mb-1">
              <div className="px-3 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-ink-3">{s.titulo}</div>
              {s.itens.map((it) => {
                indice += 1;
                const i = indice;
                const Icone = it.icone;
                return (
                  <button
                    key={it.id}
                    type="button"
                    role="option"
                    aria-selected={i === sel}
                    data-indice={i}
                    onMouseMove={() => sel !== i && setSel(i)}
                    onClick={it.executar}
                    className={cn("flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left", i === sel ? "bg-brand-50" : "")}
                  >
                    <span className={cn("grid size-8 shrink-0 place-items-center rounded-lg", i === sel ? "bg-surface text-brand" : "bg-sunken text-ink-2")}>
                      <Icone className="size-4" />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13.5px] font-medium text-ink">{it.titulo}</span>
                      {it.detalhe && <span className="block truncate text-[12px] text-ink-3">{it.detalhe}</span>}
                    </span>
                    {it.direita && <span className="num shrink-0 text-[12.5px] text-ink-2">{it.direita}</span>}
                    {i === sel && <CornerDownLeft className="size-4 shrink-0 text-ink-3" />}
                  </button>
                );
              })}
            </div>
          ))}
          {!itens.length && (
            <div className="px-6 py-10 text-center text-ink-3">
              {aCarregar ? "A pesquisar…" : remota ? `Nada encontrado para «${q.trim()}».` : "Escreva pelo menos duas letras para pesquisar documentos, clientes e artigos."}
            </div>
          )}
        </div>
        <div className="flex items-center gap-4 border-t border-line bg-bg/60 px-4 py-2 text-[12px] text-ink-3">
          <span className="inline-flex items-center gap-1.5">
            <kbd>↑</kbd>
            <kbd>↓</kbd> navegar
          </span>
          <span className="inline-flex items-center gap-1.5">
            <kbd>Enter</kbd> abrir
          </span>
          <span className="ml-auto inline-flex items-center gap-1.5">
            <kbd>Ctrl</kbd>
            <kbd>K</kbd> abre esta pesquisa em qualquer ecrã
          </span>
        </div>
      </div>
    </div>
  );
}

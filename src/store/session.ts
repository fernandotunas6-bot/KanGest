import { create } from "zustand";
import type { Perfil, Sessao } from "@/lib/types";

interface SessionState {
  sessao: Sessao | null;
  setSessao: (s: Sessao | null) => void;
  /** Perfil mínimo (CAIXA < GESTOR < ADMIN) — para menus. */
  pode: (minimo: Perfil) => boolean;
  /** Permissão por operação (ex.: "pos.desconto"). */
  tem: (permissao: string) => boolean;
}

const nivel: Record<Perfil, number> = { CAIXA: 1, GESTOR: 2, ADMIN: 3 };

export const useSession = create<SessionState>((set, get) => ({
  sessao: null,
  setSessao: (sessao) => set({ sessao }),
  pode: (minimo) => {
    const s = get().sessao;
    return !!s && (s.nivel ?? nivel[s.perfil as Perfil] ?? 1) >= nivel[minimo];
  },
  tem: (permissao) => !!get().sessao?.permissoes?.includes(permissao),
}));

import { useQuery } from "@tanstack/react-query";
import { call } from "@/lib/api";
import { useSession } from "@/store/session";

export interface LicencaEstado {
  modo: "EXPERIENCIA" | "ACTIVA" | "TOLERANCIA" | "EXPIRADA" | "RELOGIO" | "INVALIDA";
  pode_emitir: boolean;
  /** dias até deixar de poder emitir */
  dias: number;
  /** faltam poucos dias (ou já passou do fim e está na tolerância) */
  aviso: boolean;
  mensagem: string;
  fim: string | null;
  plano: string;
  /** postos permitidos (0 = sem limite) */
  postos: number;
  licenca_id: string | null;
  empresa: string | null;
  nif: string | null;
  maquina: string;
  /** código que o cliente envia ao produtor para pedir a licença */
  pedido: string;
}

/** Onde se pede ou renova a licença (página de contacto do site, com o assunto já escolhido). */
export const urlPedirLicenca = (e?: LicencaEstado | null) =>
  `https://kangest.pages.dev/contacto?assunto=${encodeURIComponent(e?.licenca_id ? "Renovar a licença do KanGest" : "Licença do KanGest")}`;

export const NOMES_PLANO: Record<string, string> = {
  EXPERIENCIA: "Experiência",
  ESSENCIAL: "Essencial",
  COMERCIO: "Comércio",
  EMPRESA: "Empresa",
};

export function useLicenca() {
  const sessao = useSession((s) => s.sessao);
  return useQuery({
    queryKey: ["licenca"],
    queryFn: () => call<LicencaEstado>("licenca.estado"),
    enabled: !!sessao && !sessao.bloqueada && !sessao.mudar_senha,
    staleTime: 5 * 60_000,
    refetchInterval: 30 * 60_000,
  });
}

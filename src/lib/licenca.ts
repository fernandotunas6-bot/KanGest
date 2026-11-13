import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
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
  /** registado no servidor de licenças (a licença renova-se sozinha) */
  registada: boolean;
  sincronizada_em: string | null;
  email_registo: string | null;
}

export interface Sincronizacao {
  mudou: boolean;
  mensagem?: string | null;
  estado: LicencaEstado;
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

let ultimaTentativa = 0;

/**
 * Num KanGest registado, pergunta ao servidor de licenças se há licença nova (por exemplo, depois de um pagamento).
 * O servidor só é contactado a cada 12 horas (o KanGest controla isso); sem internet, não faz nada.
 */
export function useSincronizarLicenca() {
  const { data: e } = useLicenca();
  const qc = useQueryClient();
  const registada = !!e?.registada;
  useEffect(() => {
    if (!registada || Date.now() - ultimaTentativa < 60 * 60_000) return;
    ultimaTentativa = Date.now();
    call<Sincronizacao>("licenca.sincronizar")
      .then((r) => {
        qc.setQueryData(["licenca"], r.estado);
        if (r.mudou) toast.success(r.estado.mensagem || "Licença actualizada.");
      })
      .catch(() => {}); // sem internet ou servidor indisponível: tenta-se mais tarde
  }, [registada, qc]);
}

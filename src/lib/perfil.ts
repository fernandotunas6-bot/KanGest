// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useQuery } from "@tanstack/react-query";
import { call } from "@/lib/api";
import { useSession } from "@/store/session";

/** O perfil de quem está a usar o programa (menu do utilizador → O meu perfil). */
export interface MeuPerfil {
  nome: string;
  login: string;
  email: string;
  telefone: string;
  foto: string | null;
  perfil: string;
}

/**
 * O meu perfil (fotografia e contactos). Com a sessão bloqueada o servidor não responde, mas fica o que já se tinha
 * lido (o ecrã de bloqueio mostra a fotografia). Um servidor de rede de uma versão anterior não tem perfil: sem foto.
 */
export function useMeuPerfil() {
  const sessao = useSession((s) => s.sessao);
  return useQuery({
    queryKey: ["perfil", sessao?.id],
    queryFn: () => call<MeuPerfil>("auth.perfil"),
    enabled: !!sessao && !sessao.bloqueada && !sessao.mudar_senha,
    staleTime: 10 * 60_000,
    retry: false,
  });
}

/** Lê uma imagem, corta-a ao quadrado (ao centro) e reduz para 256 px em JPEG: a fotografia do perfil. */
export function fotoDePerfil(ficheiro: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(new Error("Não foi possível ler a imagem."));
    r.onload = () => {
      const img = new Image();
      img.onerror = () => reject(new Error("Formato de imagem não suportado."));
      img.onload = () => {
        const lado = Math.min(img.width, img.height);
        const c = document.createElement("canvas");
        c.width = c.height = 256;
        c.getContext("2d")!.drawImage(img, (img.width - lado) / 2, (img.height - lado) / 2, lado, lado, 0, 0, 256, 256);
        resolve(c.toDataURL("image/jpeg", 0.85));
      };
      img.src = r.result as string;
    };
    r.readAsDataURL(ficheiro);
  });
}

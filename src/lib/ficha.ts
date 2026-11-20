// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useState } from "react";
import { call } from "./api";
import { useDebounce } from "./hooks";

/** Há alterações por guardar: compara o estado actual com o primeiro (em JSON). */
export function useSujo(valor: unknown): boolean {
  const atual = JSON.stringify(valor);
  const [base] = useState(atual);
  return atual !== base;
}

/**
 * Pergunta ao servidor, enquanto se escreve, se o valor já está noutra ficha: `metodo` recebe `{ [campo]: valor, id }`
 * e devolve essa outra ficha ou `null`. Não pergunta pelo valor que a ficha já tinha.
 */
export function useJaUsado<T>(metodo: string, campo: string, valor: string | null | undefined, id?: number | null, inicial?: string | null): T | null {
  const v = useDebounce((valor ?? "").trim(), 400);
  const [r, setR] = useState<{ v: string; ficha: T | null } | null>(null);
  const igualAoInicial = v === (inicial ?? "").trim();
  useEffect(() => {
    if (!v || igualAoInicial) return;
    let vivo = true;
    call<T | null>(metodo, { [campo]: v, id: id ?? null })
      .then((ficha) => vivo && setR({ v, ficha }))
      .catch(() => vivo && setR({ v, ficha: null }));
    return () => {
      vivo = false;
    };
  }, [metodo, campo, v, id, igualAoInicial]);
  // só vale a resposta do valor que está escrito agora
  return !v || igualAoInicial || r?.v !== v || (valor ?? "").trim() !== v ? null : r.ficha;
}

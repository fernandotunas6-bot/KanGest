import { invoke } from "@tauri-apps/api/core";

/**
 * Só em desenvolvimento: fora da aplicação desktop, fala com um servidor KanGest em rede por HTTP
 * (usado na auditoria visual com o browser). Em produção estas variáveis não existem.
 */
const RPC_HTTP = import.meta.env.DEV ? (import.meta.env.VITE_RPC_HTTP as string | undefined) : undefined;

async function viaHttp<T>(method: string, params?: unknown): Promise<T> {
  const r = await fetch(RPC_HTTP!, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      chave: import.meta.env.VITE_RPC_CHAVE,
      token: import.meta.env.VITE_RPC_TOKEN ?? "auditoria",
      posto: 1,
      metodo: method,
      params: params ?? null,
    }),
  });
  const j = (await r.json()) as { ok: boolean; dados?: T; erro?: string };
  if (!j.ok) throw new Error(j.erro ?? "Erro desconhecido");
  return j.dados as T;
}

/** Chamada única ao backend Rust (`rpc`). Erros chegam como string em português. */
export async function call<T = unknown>(method: string, params?: unknown): Promise<T> {
  if (!("__TAURI_INTERNALS__" in window)) {
    if (RPC_HTTP) return viaHttp<T>(method, params);
    throw new Error("Abra o KanGest pela aplicação desktop (npm run tauri dev).");
  }
  try {
    return await invoke<T>("rpc", { method, params: params ?? null });
  } catch (e) {
    throw new Error(typeof e === "string" ? e : (e as Error)?.message ?? "Erro desconhecido");
  }
}

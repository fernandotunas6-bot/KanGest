import { invoke } from "@tauri-apps/api/core";

/**
 * Só em desenvolvimento: fora da aplicação desktop, fala com um servidor KanGest em rede por HTTP
 * (usado na auditoria visual com o browser). Em produção estas variáveis não existem.
 */
const RPC_HTTP = import.meta.env.DEV ? (import.meta.env.VITE_RPC_HTTP as string | undefined) : undefined;

// Protocolo 2 da rede entre postos (src-tauri/src/rede.rs): PBKDF2-HMAC-SHA256 com o sal do servidor e
// AES-256-GCM no corpo de cada pedido e resposta.
const b64 = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b instanceof Uint8Array ? b : new Uint8Array(b))));
const deB64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
let chaveHttp: Promise<CryptoKey> | null = null;

async function chaveRede(): Promise<CryptoKey> {
  if (!chaveHttp) {
    chaveHttp = (async () => {
      const info = (await (await fetch(RPC_HTTP!)).json()) as { sal?: string };
      if (!info.sal) throw new Error("O servidor não tem a rede cifrada (versão antiga).");
      const base = await crypto.subtle.importKey("raw", new TextEncoder().encode(String(import.meta.env.VITE_RPC_CHAVE ?? "")), "PBKDF2", false, ["deriveKey"]);
      return crypto.subtle.deriveKey(
        { name: "PBKDF2", hash: "SHA-256", salt: new TextEncoder().encode(`kangest-rede:${info.sal}`), iterations: 100_000 },
        base,
        { name: "AES-GCM", length: 256 },
        false,
        ["encrypt", "decrypt"],
      );
    })();
  }
  return chaveHttp;
}

async function viaHttp<T>(method: string, params?: unknown): Promise<T> {
  const chave = await chaveRede();
  const id = b64(crypto.getRandomValues(new Uint8Array(16)));
  const pedido = JSON.stringify({
    token: import.meta.env.VITE_RPC_TOKEN ?? "auditoria",
    posto: 1,
    metodo: method,
    params: params ?? null,
    t: Date.now(),
    id,
  });
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const c = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: new TextEncoder().encode("kangest-rede-v2-pedido") }, chave, new TextEncoder().encode(pedido));
  const r = (await (await fetch(RPC_HTTP!, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ v: 2, n: b64(iv), c: b64(c) }) })).json()) as {
    n?: string;
    c?: string;
    erro?: string;
  };
  if (!r.c || !r.n) throw new Error(r.erro ?? "Erro desconhecido");
  const aberto = await crypto.subtle.decrypt({ name: "AES-GCM", iv: deB64(r.n), additionalData: new TextEncoder().encode("kangest-rede-v2-resposta") }, chave, deB64(r.c));
  const j = JSON.parse(new TextDecoder().decode(aberto)) as { ok: boolean; dados?: T; erro?: string; id?: string };
  if (j.id !== id) throw new Error("A resposta do servidor não corresponde ao pedido.");
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

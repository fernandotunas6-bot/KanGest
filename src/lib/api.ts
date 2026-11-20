// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { invoke } from "@tauri-apps/api/core";
import { gcm } from "@noble/ciphers/aes";
import { pbkdf2Async } from "@noble/hashes/pbkdf2";
import { sha256 } from "@noble/hashes/sha256";

/**
 * O KanGest corre de duas maneiras com o mesmo código:
 *  - na aplicação instalada (Windows, macOS): `invoke("rpc")` para o Rust desta máquina;
 *  - no browser (telemóvel, tablet, terminal POS Android, iPad, outro computador): a página vem do
 *    servidor KanGest da loja e cada pedido segue cifrado pelo protocolo da rede (src-tauri/src/rede.rs),
 *    com a chave de rede, como os outros postos.
 */
export const NA_APP = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
/** No browser, ligado a um servidor KanGest. */
export const MODO_WEB = !NA_APP;

/**
 * Só em desenvolvimento: fora da aplicação desktop, fala com um servidor KanGest noutro endereço
 * (VITE_RPC_HTTP + VITE_RPC_CHAVE). Em produção a página vem do próprio servidor (mesma origem).
 */
const RPC_HTTP = import.meta.env.DEV ? (import.meta.env.VITE_RPC_HTTP as string | undefined) : undefined;

// ------------------------------------------------------------------ base64 (sem rebentar a pilha)

function b64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

export function deB64(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Para o WebCrypto (os tipos exigem um ArrayBuffer simples, não partilhado). */
const buf = (u: Uint8Array) => u as Uint8Array<ArrayBuffer>;
const b64url = (b: Uint8Array) => b64(b).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const texto = new TextEncoder();
const aleatorio = (n: number) => crypto.getRandomValues(new Uint8Array(n));

// ------------------------------------------------------------------ cifra (protocolo 2 da rede)
// PBKDF2-HMAC-SHA256 (100 000 iterações, sal do servidor) e AES-256-GCM. O WebCrypto só existe em
// páginas seguras (https ou localhost); num endereço http da rede local usa-se a mesma cifra em JS.

const AAD_PEDIDO = texto.encode("kangest-rede-v2-pedido");
const AAD_RESPOSTA = texto.encode("kangest-rede-v2-resposta");
const subtil = typeof crypto !== "undefined" && crypto.subtle ? crypto.subtle : null;

async function derivar(chave: string, sal: string): Promise<Uint8Array> {
  const senha = texto.encode(chave.trim());
  const s = texto.encode(`kangest-rede:${sal}`);
  if (subtil) {
    const base = await subtil.importKey("raw", senha, "PBKDF2", false, ["deriveBits"]);
    return new Uint8Array(await subtil.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt: s, iterations: 100_000 }, base, 256));
  }
  return pbkdf2Async(sha256, senha, s, { c: 100_000, dkLen: 32, asyncTick: 20 });
}

const chavesAes = new Map<string, Promise<CryptoKey>>();
function chaveAes(k: Uint8Array): Promise<CryptoKey> {
  const id = b64(k);
  let c = chavesAes.get(id);
  if (!c) {
    c = subtil!.importKey("raw", buf(k), "AES-GCM", false, ["encrypt", "decrypt"]);
    chavesAes.set(id, c);
  }
  return c;
}

async function selar(k: Uint8Array, aad: Uint8Array, dados: Uint8Array): Promise<{ n: string; c: string }> {
  const n = aleatorio(12);
  const c = subtil
    ? new Uint8Array(await subtil.encrypt({ name: "AES-GCM", iv: n, additionalData: buf(aad) }, await chaveAes(k), buf(dados)))
    : gcm(k, n, aad).encrypt(dados);
  return { n: b64(n), c: b64(c) };
}

async function abrir(k: Uint8Array, aad: Uint8Array, n: string, c: string): Promise<Uint8Array | null> {
  try {
    return subtil
      ? new Uint8Array(await subtil.decrypt({ name: "AES-GCM", iv: deB64(n), additionalData: buf(aad) }, await chaveAes(k), deB64(c)))
      : gcm(k, deB64(n), aad).decrypt(deB64(c));
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------ ligação do browser ao servidor

export type LigacaoWeb = {
  /** "" = o próprio servidor que serviu a página. */
  base: string;
  sal: string;
  /** Chave derivada (a chave de rede em si nunca fica guardada no browser). */
  chave: string;
  posto: number;
  /** Identifica este browser no servidor (a sessão dele vive lá). */
  token: string;
  versao?: string;
};

const GUARDA = "kangest.web";
/** Evento quando o servidor deixa de aceitar este browser (chave de rede mudada): volta o ecrã de ligação. */
export const EVENTO_DESLIGADO = "kangest:desligado";

export function ligacaoWeb(): LigacaoWeb | null {
  try {
    const l = JSON.parse(localStorage.getItem(GUARDA) ?? "null") as LigacaoWeb | null;
    return l?.chave && l.token ? l : null;
  } catch {
    return null;
  }
}

function guardarLigacao(l: LigacaoWeb | null) {
  if (l) localStorage.setItem(GUARDA, JSON.stringify(l));
  else localStorage.removeItem(GUARDA);
}

/** Esquece a ligação deste browser (pede de novo a chave de rede). */
export function esquecerLigacao() {
  guardarLigacao(null);
  window.dispatchEvent(new Event(EVENTO_DESLIGADO));
}

/** Nome deste aparelho na lista de postos ligados do servidor. */
export function descricaoDispositivo(): string {
  const ua = navigator.userAgent;
  const so = /Android/i.test(ua)
    ? "Android"
    : /iPad/i.test(ua) || (/Macintosh/i.test(ua) && navigator.maxTouchPoints > 1)
      ? "iPad"
      : /iPhone/i.test(ua)
        ? "iPhone"
        : /Windows/i.test(ua)
          ? "Windows"
          : /Mac OS X/i.test(ua)
            ? "Mac"
            : /Linux/i.test(ua)
              ? "Linux"
              : "browser";
  const nav = /Edg\//.test(ua) ? "Edge" : /SamsungBrowser/.test(ua) ? "Samsung Internet" : /Firefox\//.test(ua) ? "Firefox" : /Chrome\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "browser";
  return `${nav} · ${so}`;
}

class ErroLigacao extends Error {}

async function enviar<T>(l: LigacaoWeb, metodo: string, params: unknown): Promise<T> {
  const k = deB64(l.chave);
  const id = b64url(aleatorio(16));
  const pedido = JSON.stringify({ token: l.token, posto: l.posto, metodo, params: params ?? null, descricao_posto: descricaoDispositivo(), t: Date.now(), id });
  const envelope = { v: 2, ...(await selar(k, AAD_PEDIDO, texto.encode(pedido))) };
  let r: Response;
  try {
    r = await fetch(`${l.base}/rpc`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(envelope), cache: "no-store" });
  } catch {
    throw new Error("Sem ligação ao servidor KanGest. Confirme que este aparelho está na rede da loja e que o servidor está ligado.");
  }
  let j: { n?: string; c?: string; erro?: string };
  try {
    j = await r.json();
  } catch {
    throw new Error(`O servidor respondeu de forma inesperada (${r.status}).`);
  }
  if (r.status === 401) throw new ErroLigacao(j.erro ?? "Chave de rede inválida.");
  if (!j.c || !j.n) throw new Error(j.erro ?? `Erro do servidor (${r.status}).`);
  const aberto = await abrir(k, AAD_RESPOSTA, j.n, j.c);
  if (!aberto) throw new ErroLigacao("A resposta do servidor não abre com a chave deste aparelho.");
  const res = JSON.parse(new TextDecoder().decode(aberto)) as { ok: boolean; dados?: T; erro?: string; id?: string };
  if (res.id !== id) throw new Error("A resposta do servidor não corresponde ao pedido.");
  if (!res.ok) throw new Error(res.erro ?? "Erro desconhecido");
  return res.dados as T;
}

type InfoServidor = { ok: boolean; versao: string; protocolo: number; sal?: string; web?: boolean };

async function infoServidor(base: string): Promise<InfoServidor> {
  let r: Response;
  try {
    r = await fetch(`${base}/rede/info`, { cache: "no-store" });
  } catch {
    throw new Error("Sem ligação ao servidor KanGest.");
  }
  const i = (await r.json()) as InfoServidor;
  if (!i.sal) throw new Error("O servidor tem uma versão antiga do KanGest (sem rede cifrada). Actualize o servidor.");
  return i;
}

/**
 * Liga este browser ao servidor com a chave de rede: deriva a chave (como os postos instalados) e
 * confirma-a com um pedido real. Devolve o nome da empresa.
 */
export async function ligarWeb(chave: string, posto: number, base = ""): Promise<{ empresa: string | null; versao: string }> {
  const info = await infoServidor(base);
  const anterior = ligacaoWeb();
  const l: LigacaoWeb = {
    base,
    sal: info.sal!,
    chave: b64(await derivar(chave, info.sal!)),
    posto: Math.max(1, Math.trunc(posto) || 1),
    token: anterior?.token ?? b64url(aleatorio(18)),
    versao: info.versao,
  };
  let e: { empresa: string | null };
  try {
    e = await enviar<{ empresa: string | null }>(l, "setup.estado", null);
  } catch (err) {
    if (err instanceof ErroLigacao) throw new Error("Chave de rede errada. Confirme-a no servidor, em Configurações → Rede.");
    throw err;
  }
  guardarLigacao(l);
  return { empresa: e.empresa, versao: info.versao };
}

/** Muda o n.º de posto deste browser (o posto define a série, o armazém, a impressora e o caixa). */
export function definirPostoWeb(posto: number) {
  const l = ligacaoWeb();
  if (l) guardarLigacao({ ...l, posto });
}

let ligacaoDev: Promise<LigacaoWeb> | null = null;
async function ligacaoActual(): Promise<LigacaoWeb | null> {
  const l = ligacaoWeb();
  if (l || !RPC_HTTP) return l;
  // desenvolvimento: liga-se sozinho ao servidor indicado nas variáveis VITE_RPC_*
  ligacaoDev ??= (async () => {
    const base = RPC_HTTP.replace(/\/+$/, "");
    const info = await infoServidor(base);
    return {
      base,
      sal: info.sal!,
      chave: b64(await derivar(String(import.meta.env.VITE_RPC_CHAVE ?? ""), info.sal!)),
      posto: 1,
      token: String(import.meta.env.VITE_RPC_TOKEN ?? "auditoria"),
      versao: info.versao,
    };
  })();
  return ligacaoDev;
}

// ------------------------------------------------------------------ ficheiros no browser

/** Entrega um ficheiro ao utilizador pelo browser (vai para as Transferências do aparelho). */
export function descarregar(nome: string, dados: Uint8Array | string, tipo = "application/octet-stream") {
  const parte: BlobPart = typeof dados === "string" ? dados : (dados.buffer.slice(dados.byteOffset, dados.byteOffset + dados.byteLength) as ArrayBuffer);
  const url = URL.createObjectURL(new Blob([parte], { type: tipo }));
  const a = document.createElement("a");
  a.href = url;
  a.download = nome;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/**
 * Ficheiro gerado no servidor (SAF-T, cópia de segurança): no browser, traz-se para este aparelho.
 * Na aplicação instalada não faz nada (o ficheiro já está na pasta indicada).
 */
export async function levarFicheiro(ficheiro: string | null | undefined): Promise<void> {
  if (!MODO_WEB || !ficheiro) return;
  const f = await call<{ nome: string; base64: string }>("ficheiros.descarregar", { ficheiro });
  descarregar(f.nome, deB64(f.base64));
}

const NAO = Symbol("remoto");

/** O que no computador corre localmente e no browser tem outra forma (ou não existe). */
function noBrowser(metodo: string, params: unknown): unknown {
  const p = (params ?? {}) as Record<string, string>;
  switch (metodo) {
    case "sistema.exportar":
      descarregar(p.nome, p.conteudo, /\.csv$/i.test(p.nome) ? "text/csv;charset=utf-8" : "text/plain;charset=utf-8");
      return { ficheiro: `Transferências / ${p.nome}` };
    case "sistema.exportarBinario":
      descarregar(p.nome, deB64(p.base64));
      return { ficheiro: `Transferências / ${p.nome}` };
    case "partilha.abrir":
    case "licenca.abrirSite":
      window.open(p.url, "_blank", "noopener,noreferrer");
      return null;
  }
  if (/^(rede|atualizacao|internet)\./.test(metodo)) {
    throw new Error("Esta função só existe na aplicação instalada no computador servidor.");
  }
  return NAO;
}

// ------------------------------------------------------------------ chamada única

/** Chamada única ao backend (`rpc`). Erros chegam como string em português. */
export async function call<T = unknown>(method: string, params?: unknown): Promise<T> {
  if (!NA_APP) {
    const local = noBrowser(method, params);
    if (local !== NAO) return local as T;
    const l = await ligacaoActual();
    if (!l) {
      window.dispatchEvent(new Event(EVENTO_DESLIGADO));
      throw new Error("Este aparelho ainda não está ligado ao servidor KanGest.");
    }
    try {
      const r = await enviar<T>(l, method, params);
      if (method === "posto.selecionar") definirPostoWeb(Number((r as { id?: number })?.id) || l.posto);
      return r;
    } catch (e) {
      if (e instanceof ErroLigacao && !RPC_HTTP) esquecerLigacao();
      throw e;
    }
  }
  try {
    return await invoke<T>("rpc", { method, params: params ?? null });
  } catch (e) {
    throw new Error(typeof e === "string" ? e : (e as Error)?.message ?? "Erro desconhecido");
  }
}

/** Reinicia a aplicação (ex.: depois de marcar o restauro de uma cópia de segurança). */
export async function reiniciar(): Promise<void> {
  if (NA_APP) await invoke("reiniciar");
  else location.reload();
}

/**
 * Copia texto para a área de transferência. `navigator.clipboard` só existe em páginas seguras: no browser
 * por http da rede local usa-se o método antigo (selecção + copiar).
 */
export async function copiarTexto(t: string): Promise<void> {
  if (navigator.clipboard && window.isSecureContext) return navigator.clipboard.writeText(t);
  const area = document.createElement("textarea");
  area.value = t;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  if (!ok) throw new Error("Não foi possível copiar.");
}

/** Abre um endereço externo (WhatsApp, site): no computador pelo sistema, no browser num separador novo. */
export async function abrirExterno(url: string): Promise<void> {
  if (NA_APP) {
    await call("partilha.abrir", { url });
    return;
  }
  // depois de um pedido ao servidor o browser pode bloquear a janela nova: abre-se então no próprio separador
  if (!window.open(url, "_blank", "noopener,noreferrer")) location.href = url;
}

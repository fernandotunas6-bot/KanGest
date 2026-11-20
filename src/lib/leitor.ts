// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

/**
 * Leitor de código de barras: os leitores USB e Bluetooth funcionam como um teclado. Escrevem muito mais depressa do
 * que uma pessoa (poucos milissegundos entre caracteres) e acabam a leitura com Enter, Tab ou nada. É pela
 * velocidade que se distingue uma leitura do que alguém escreve à mão.
 */
import { useEffect, useRef, useState } from "react";

export interface Leitura {
  codigo: string;
  terminador: "Enter" | "Tab" | "nenhum";
  /** média de milissegundos entre caracteres */
  msPorCaracter: number;
}

export interface ConfigLeitor {
  /** som curto a cada leitura (e outro, grave, quando o código não existe) */
  som: boolean;
  /** leitores sem fios (Bluetooth, 2,4 GHz) são mais lentos: aceita mais tempo entre caracteres */
  lento: boolean;
}

const CHAVE = "kangest.leitor";
const PADRAO: ConfigLeitor = { som: true, lento: false };

export function configLeitor(): ConfigLeitor {
  try {
    return { ...PADRAO, ...JSON.parse(localStorage.getItem(CHAVE) ?? "{}") };
  } catch {
    return PADRAO;
  }
}

/** Configuração do leitor neste aparelho (cada computador ou terminal tem o seu leitor). */
export function useConfigLeitor(): [ConfigLeitor, (c: Partial<ConfigLeitor>) => void] {
  const [c, setC] = useState(configLeitor);
  return [
    c,
    (n) => {
      const novo = { ...c, ...n };
      setC(novo);
      try {
        localStorage.setItem(CHAVE, JSON.stringify(novo));
      } catch {
        /* sem armazenamento: vale até fechar */
      }
    },
  ];
}

/** Tempo máximo entre dois caracteres de uma leitura (ms). Uma pessoa a escrever depressa raramente fica abaixo
 * disto em quatro teclas seguidas. */
export const intervaloMaximo = () => (configLeitor().lento ? 120 : 70);
/** Caracteres mínimos de uma leitura com terminador; sem terminador exige-se mais, para não confundir. */
const MINIMO = 4;
const MINIMO_SEM_TERMINADOR = 8;

/**
 * Mede uma sequência de teclas e diz se foi um leitor. Serve o `useLeitor` e o teste do leitor nas configurações.
 */
export class Medidor {
  private buf = "";
  private tempos: number[] = [];

  /** Junta um carácter; devolve `false` se a pausa desde o anterior mostra que não é a mesma leitura. */
  juntar(ch: string, agora: number = performance.now()): boolean {
    const ultimo = this.tempos[this.tempos.length - 1];
    const continua = ultimo === undefined || agora - ultimo <= intervaloMaximo();
    if (!continua) this.limpar();
    this.buf += ch;
    this.tempos.push(agora);
    return continua;
  }

  get texto() {
    return this.buf;
  }

  /** A leitura, se o que se juntou veio de um leitor (rápido e com caracteres suficientes). */
  fechar(terminador: Leitura["terminador"]): Leitura | null {
    const gaps = this.tempos.slice(1).map((t, i) => t - this.tempos[i]);
    const minimo = terminador === "nenhum" ? MINIMO_SEM_TERMINADOR : MINIMO;
    const rapido = this.buf.length >= minimo && gaps.length > 0 && gaps.every((g) => g <= intervaloMaximo());
    const l = rapido ? { codigo: this.buf.trim(), terminador, msPorCaracter: Math.round(gaps.reduce((s, g) => s + g, 0) / gaps.length) } : null;
    this.limpar();
    return l && l.codigo ? l : null;
  }

  /** Estatística do que se escreveu (para o teste), mesmo que não tenha sido um leitor. */
  medir(): { caracteres: number; msPorCaracter: number } {
    const gaps = this.tempos.slice(1).map((t, i) => t - this.tempos[i]);
    return { caracteres: this.buf.length, msPorCaracter: gaps.length ? Math.round(gaps.reduce((s, g) => s + g, 0) / gaps.length) : 0 };
  }

  limpar() {
    this.buf = "";
    this.tempos = [];
  }
}

/** Muda o valor de um campo controlado pelo React (o `onChange` do campo corre como se fosse escrito). */
function definirValor(el: HTMLInputElement | HTMLTextAreaElement, valor: string) {
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(el, valor);
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

/**
 * Recebe as leituras do leitor em qualquer sítio do ecrã, mesmo com o cursor noutro campo (o que o leitor escreveu
 * nesse campo é tirado). Não lê com uma janela aberta por cima, nem nos campos marcados com `data-leitor` (tratam a
 * leitura eles próprios, como a pesquisa do POS).
 */
export function useLeitor(onLer: (l: Leitura) => void, ativo = true) {
  const ref = useRef(onLer);
  ref.current = onLer;
  useEffect(() => {
    if (!ativo) return;
    const m = new Medidor();
    let alvo: HTMLInputElement | HTMLTextAreaElement | null = null;
    let antes = "";
    let semTerminador: number | undefined;
    const acabar = (terminador: Leitura["terminador"]) => {
      window.clearTimeout(semTerminador);
      const campo = alvo;
      const valorAntes = antes;
      alvo = null;
      const l = m.fechar(terminador);
      if (!l) return false;
      if (campo && campo.value !== valorAntes) definirValor(campo, valorAntes);
      ref.current(l);
      return true;
    };
    const h = (e: KeyboardEvent) => {
      const el = document.activeElement as HTMLElement | null;
      if (el?.closest("[data-leitor]") || document.querySelector("[role=dialog][aria-modal=true]")) {
        m.limpar();
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        if (m.texto && acabar(e.key)) {
          e.preventDefault();
          e.stopPropagation();
        }
        m.limpar();
        return;
      }
      if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) {
        if (e.key !== "Shift") m.limpar();
        return;
      }
      // a hora a que a tecla chegou (não a hora a que é tratada): com o ecrã ocupado, as teclas do leitor esperam em
      // fila e seriam medidas mais afastadas do que vieram
      m.juntar(e.key, e.timeStamp);
      if (m.texto.length === 1) {
        // começa uma leitura: lembra o campo onde está o cursor e o que lá estava
        alvo = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement ? el : null;
        antes = alvo?.value ?? "";
      }
      // leitores sem terminador: a leitura acaba quando param de escrever
      window.clearTimeout(semTerminador);
      semTerminador = window.setTimeout(() => acabar("nenhum"), intervaloMaximo() * 2 + 40);
    };
    window.addEventListener("keydown", h, true);
    return () => {
      window.removeEventListener("keydown", h, true);
      window.clearTimeout(semTerminador);
    };
  }, [ativo]);
}

let audio: AudioContext | null = null;

/** Som da leitura: curto e agudo quando o artigo existe, dois graves quando não. */
export function bip(ok = true) {
  if (!configLeitor().som) return;
  try {
    audio ??= new AudioContext();
    const a = audio;
    if (a.state === "suspended") void a.resume();
    const tom = (freq: number, inicio: number, duracao: number) => {
      const o = a.createOscillator();
      const g = a.createGain();
      o.type = "square";
      o.frequency.value = freq;
      const t = a.currentTime + inicio;
      g.gain.setValueAtTime(0.05, t);
      g.gain.exponentialRampToValueAtTime(0.0001, t + duracao);
      o.connect(g).connect(a.destination);
      o.start(t);
      o.stop(t + duracao + 0.02);
    };
    if (ok) tom(1760, 0, 0.07);
    else {
      tom(330, 0, 0.12);
      tom(247, 0.16, 0.18);
    }
  } catch {
    /* sem som neste aparelho */
  }
}

/** Dígito de controlo de um GTIN (EAN-8, UPC-A, EAN-13, GTIN-14). `null` se não for um GTIN. */
export function gtinValido(codigo: string): boolean | null {
  const c = codigo.trim();
  if (!/^\d+$/.test(c) || ![8, 12, 13, 14].includes(c.length)) return null;
  const d = [...c].map(Number);
  const controlo = d.pop()!;
  const soma = d.reverse().reduce((s, x, i) => s + x * (i % 2 === 0 ? 3 : 1), 0);
  return (10 - (soma % 10)) % 10 === controlo;
}

/** Que tipo de código parece (para mostrar no teste e no aviso de código desconhecido). */
export function tipoCodigo(codigo: string): string {
  const c = codigo.trim();
  if (/^(\]..)?(\(01\)|01\d{14})/.test(c) && c.replace(/^\]../, "").length > 16) return "GS1 (com lote ou validade)";
  if (/^97[89]\d{10}$/.test(c)) return "ISBN (livro)";
  if (/^\d{13}$/.test(c)) return "EAN-13";
  if (/^\d{8}$/.test(c)) return "EAN-8";
  if (/^\d{12}$/.test(c)) return "UPC-A";
  if (/^\d{14}$/.test(c)) return "GTIN-14 (caixa)";
  return "Código interno (Code 128, Code 39 ou QR)";
}

/** O aparelho consegue ler códigos com a câmara (Android, Chrome; no iPhone e no Windows ainda não). */
export const camaraDisponivel = () => typeof window !== "undefined" && "BarcodeDetector" in window && !!navigator.mediaDevices?.getUserMedia;

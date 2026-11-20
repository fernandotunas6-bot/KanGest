// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Camera, CheckCircle2, CircleAlert, Link2, Plus, ScanBarcode } from "lucide-react";
import { call } from "@/lib/api";
import { bip, gtinValido, intervaloMaximo, tipoCodigo, useConfigLeitor, type Leitura } from "@/lib/leitor";
import type { Artigo } from "@/lib/types";
import { useSession } from "@/store/session";
import { ArtigoPicker } from "@/components/Pickers";
import { Badge, BotaoCancelar, Button, Field, Input, Modal } from "@/components/ui";

interface Detector {
  detect(fonte: CanvasImageSource): Promise<{ rawValue: string; format: string }[]>;
}
type ConstrutorDetector = new (o?: { formats?: string[] }) => Detector;

/** Ler um código com a câmara do telemóvel ou do tablet (onde o browser sabe reconhecer códigos). */
export function LerComCamara({ onLer, onClose }: { onLer: (codigo: string) => void; onClose: () => void }) {
  const video = useRef<HTMLVideoElement>(null);
  const [erro, setErro] = useState<string | null>(null);
  // a câmara abre uma vez: o callback novo de cada desenho não a reinicia
  const lerRef = useRef(onLer);
  lerRef.current = onLer;
  useEffect(() => {
    let parar = false;
    let fluxo: MediaStream | null = null;
    let tempo: number | undefined;
    (async () => {
      try {
        const Construtor = (window as unknown as { BarcodeDetector: ConstrutorDetector }).BarcodeDetector;
        const formatos = ["ean_13", "ean_8", "upc_a", "upc_e", "code_128", "code_39", "itf", "data_matrix", "qr_code"];
        const detector = new Construtor({ formats: formatos });
        fluxo = await navigator.mediaDevices.getUserMedia({ video: { facingMode: { ideal: "environment" } }, audio: false });
        if (parar || !video.current) return;
        video.current.srcObject = fluxo;
        await video.current.play();
        const procurar = async () => {
          if (parar || !video.current) return;
          try {
            const r = await detector.detect(video.current);
            if (r[0]?.rawValue) {
              parar = true;
              lerRef.current(r[0].rawValue);
              return;
            }
          } catch {
            /* imagem ainda sem código */
          }
          tempo = window.setTimeout(procurar, 180);
        };
        void procurar();
      } catch (e) {
        setErro((e as Error).name === "NotAllowedError" ? "Sem autorização para usar a câmara. Autorize-a nas definições do browser." : "Não foi possível abrir a câmara deste aparelho.");
      }
    })();
    return () => {
      parar = true;
      window.clearTimeout(tempo);
      fluxo?.getTracks().forEach((t) => t.stop());
    };
  }, []);
  return (
    <Modal open onClose={onClose} title="Ler com a câmara" width="max-w-md" footer={<BotaoCancelar>Fechar</BotaoCancelar>}>
      {erro ? (
        <p className="text-[14px] text-danger">{erro}</p>
      ) : (
        <div className="relative overflow-hidden rounded-xl bg-black">
          <video ref={video} playsInline muted className="aspect-[4/3] w-full object-cover" />
          <div className="pointer-events-none absolute inset-x-8 top-1/2 h-24 -translate-y-1/2 rounded-lg border-2 border-white/80 shadow-[0_0_0_999px_rgba(0,0,0,.35)]" />
        </div>
      )}
      <p className="mt-3 text-[13px] text-ink-2">Aponte para o código de barras, com luz e sem tremer. Lê EAN, UPC, Code 128, Code 39, ITF, DataMatrix e QR.</p>
    </Modal>
  );
}

/**
 * O leitor leu um código que não está em nenhum artigo: juntá-lo a um artigo (outra embalagem, o código do
 * fornecedor…) e continuar a vender, ou criar o artigo com o código.
 */
export function CodigoDesconhecido({ codigo, onAssociado, onClose }: { codigo: string; onAssociado?: (a: Artigo) => void; onClose: () => void }) {
  const tem = useSession((s) => s.tem);
  const nav = useNavigate();
  const [artigo, setArtigo] = useState<Artigo | null>(null);
  const [quantidade, setQuantidade] = useState("1");
  const [descricao, setDescricao] = useState("");
  const [busy, setBusy] = useState(false);
  const valido = gtinValido(codigo);
  async function associar() {
    if (!artigo) return;
    setBusy(true);
    try {
      const a = await call<Artigo>("artigos.associarCodigo", { artigo_id: artigo.id, codigo, quantidade: Number(quantidade.replace(",", ".")) || 1, descricao: descricao || null });
      toast.success(`Código ${codigo} juntado a «${a.descricao}».`);
      bip(true);
      onAssociado?.(a);
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title="Código desconhecido"
      width="max-w-xl"
      footer={
        <>
          {tem("artigos.criar") && (
            <Button variant="ghost" className="mr-auto" onClick={() => nav(`/artigos?novo=${encodeURIComponent(codigo)}`)}>
              <Plus className="size-4" /> Criar artigo novo
            </Button>
          )}
          <BotaoCancelar>Fechar</BotaoCancelar>
          {tem("artigos.editar") && (
            <Button disabled={!artigo} loading={busy} onClick={associar}>
              <Link2 className="size-4" /> Juntar ao artigo
            </Button>
          )}
        </>
      }
    >
      <div className="mb-4 flex items-center gap-3 rounded-xl border border-line bg-sunken px-4 py-3">
        <ScanBarcode className="size-6 text-ink-3" />
        <div className="min-w-0">
          <div className="break-all font-mono text-[17px] font-semibold">{codigo}</div>
          <div className="text-[12.5px] text-ink-2">
            {tipoCodigo(codigo)}
            {valido === false && <span className="ml-1 font-medium text-danger">· o último algarismo não bate: pode ter sido mal lido, leia outra vez</span>}
          </div>
        </div>
      </div>
      {tem("artigos.editar") ? (
        <>
          <p className="mb-3 text-[13.5px] text-ink-2">Nenhum artigo tem este código. Se for outra embalagem ou outro código de um artigo que já existe, junte-o: a partir daí o leitor reconhece-o.</p>
          {artigo ? (
            <div className="mb-3 flex items-center justify-between gap-3 rounded-lg border border-brand/30 bg-brand-50 px-3 py-2 text-[14px]">
              <span>
                <span className="font-mono text-[12.5px] text-ink-2">{artigo.referencia}</span> <span className="font-medium">{artigo.descricao}</span>
              </span>
              <Button size="sm" variant="ghost" onClick={() => setArtigo(null)}>
                Trocar
              </Button>
            </div>
          ) : (
            <ArtigoPicker className="mb-3" onPick={(a) => setArtigo(a)} placeholder="Escolher o artigo a que pertence…" />
          )}
          <div className="grid grid-cols-2 gap-3">
            <Field label="Unidades que este código vende" hint="Ex.: 12 numa caixa de 12">
              <Input inputMode="decimal" className="num text-right" value={quantidade} onChange={(e) => setQuantidade(e.target.value)} />
            </Field>
            <Field label="Nome da embalagem (opcional)">
              <Input value={descricao} maxLength={60} placeholder="Caixa de 12" onChange={(e) => setDescricao(e.target.value)} />
            </Field>
          </div>
        </>
      ) : (
        <p className="text-[13.5px] text-ink-2">Nenhum artigo tem este código. Peça a quem gere os artigos para o juntar ao artigo certo.</p>
      )}
    </Modal>
  );
}

/** Configurações → Postos: o leitor deste aparelho (som, leitor sem fios) e um campo para o testar. */
export function ConfigLeitorAparelho() {
  const [c, setC] = useConfigLeitor();
  // tempos de todas as teclas desta leitura (o teste mede também o que é escrito devagar, à mão)
  const tempos = useRef<number[]>([]);
  const [resultado, setResultado] = useState<{ texto: string; leitura: Leitura | null; ms: number; artigo?: Artigo | null } | null>(null);
  const [texto, setTexto] = useState("");
  const semSufixo = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(semSufixo.current), []);
  async function fim(terminador: Leitura["terminador"], valor: string) {
    const ts = tempos.current;
    tempos.current = [];
    setTexto("");
    const escrito = valor.trim();
    if (!escrito) return;
    const gaps = ts.slice(1).map((t, i) => t - ts[i]);
    const ms = gaps.length ? Math.round(gaps.reduce((a, g) => a + g, 0) / gaps.length) : 0;
    const minimo = terminador === "nenhum" ? 8 : 4;
    const doLeitor = escrito.length >= minimo && gaps.length > 0 && gaps.every((g) => g <= intervaloMaximo());
    const leitura = doLeitor ? { codigo: escrito, terminador, msPorCaracter: ms } : null;
    setResultado({ texto: escrito, leitura, ms });
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo: escrito });
      setResultado({ texto: escrito, leitura, ms, artigo: a });
      bip(!!a);
    } catch {
      /* sem acesso aos artigos */
    }
  }
  const r = resultado;
  const valido = r ? gtinValido(r.texto) : null;
  return (
    <div>
      <div className="flex flex-wrap gap-5">
        <label className="flex items-center gap-2 text-[13px]">
          <input type="checkbox" checked={c.som} onChange={(e) => setC({ som: e.target.checked })} /> Som ao ler (agudo: encontrado; grave: não existe)
        </label>
        <label className="flex items-center gap-2 text-[13px]">
          <input type="checkbox" checked={c.lento} onChange={(e) => setC({ lento: e.target.checked })} /> Leitor sem fios (Bluetooth ou 2,4 GHz)
        </label>
      </div>
      <Field label="Testar o leitor" hint="Ponha o cursor aqui e leia um código. Valem só para este aparelho." className="mt-3">
        <Input
          data-leitor
          value={texto}
          placeholder="Leia um código de barras…"
          onChange={(e) => setTexto(e.target.value)}
          onKeyDown={(e) => {
            window.clearTimeout(semSufixo.current);
            const campo = e.currentTarget;
            if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              void fim(e.key, campo.value);
            } else if (e.key.length === 1) {
              tempos.current.push(e.timeStamp);
              // leitores sem sufixo: quando param de escrever
              semSufixo.current = window.setTimeout(() => campo.value.trim().length >= 8 && void fim("nenhum", campo.value), 400);
            }
          }}
        />
      </Field>
      {r && (
        <div className="mt-3 grid gap-1.5 rounded-lg border border-line bg-sunken px-3 py-2.5 text-[13px]">
          <div className="flex flex-wrap items-center gap-2">
            {r.leitura ? <CheckCircle2 className="size-4 text-ok" /> : <CircleAlert className="size-4 text-accent-ink" />}
            <span className="font-mono font-semibold">{r.texto}</span>
            <Badge>{tipoCodigo(r.texto)}</Badge>
            {valido === true && <Badge tone="ok">Dígito de controlo certo</Badge>}
            {valido === false && <Badge tone="danger">Dígito de controlo errado</Badge>}
          </div>
          <div className="text-ink-2">
            {r.leitura
              ? `Leitor reconhecido: ${r.leitura.msPorCaracter} ms por carácter, acaba com ${r.leitura.terminador === "nenhum" ? "nada (sem sufixo)" : r.leitura.terminador}.`
              : `Parece escrito à mão (${r.ms} ms por carácter). Se foi o leitor, ligue «Leitor sem fios» ou configure-o para enviar Enter no fim.`}
          </div>
          {r.artigo !== undefined && <div className={r.artigo ? "text-ok" : "text-ink-2"}>{r.artigo ? `Artigo: ${r.artigo.referencia} «${r.artigo.descricao}»` : "Nenhum artigo tem este código."}</div>}
        </div>
      )}
    </div>
  );
}

/** Botão da câmara para um campo de leitura (só aparece onde o aparelho sabe ler com a câmara). */
export function BotaoCamara({ onLer, className }: { onLer: (codigo: string) => void; className?: string }) {
  const [aberta, setAberta] = useState(false);
  return (
    <>
      <button type="button" title="Ler com a câmara" aria-label="Ler com a câmara" onClick={() => setAberta(true)} className={className}>
        <Camera className="size-5" />
      </button>
      {aberta && (
        <LerComCamara
          onClose={() => setAberta(false)}
          onLer={(c) => {
            setAberta(false);
            onLer(c);
          }}
        />
      )}
    </>
  );
}

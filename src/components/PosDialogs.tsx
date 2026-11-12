import { useEffect, useRef, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { Delete, KeyRound } from "lucide-react";
import { call } from "@/lib/api";
import { Button, Field, Input, Modal, cn } from "./ui";

/** Teclado numérico táctil (Quantidade / Alterar preço / Desconto do KIGEST POS). */
export function TecladoModal({
  open,
  titulo,
  valorInicial,
  sufixo,
  ajuda,
  extra,
  onConfirm,
  onClose,
}: {
  open: boolean;
  titulo: string;
  valorInicial: string;
  sufixo?: string;
  ajuda?: string;
  extra?: ReactNode;
  onConfirm: (valor: number) => void;
  onClose: () => void;
}) {
  const [v, setV] = useState(valorInicial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (open) {
      setV(valorInicial);
      setTimeout(() => ref.current?.select(), 30);
    }
  }, [open, valorInicial]);

  const tecla = (t: string) => {
    if (t === "C") return setV("");
    if (t === "⌫") return setV((x) => x.slice(0, -1));
    if (t === "," && v.includes(",")) return;
    setV((x) => x + t);
  };
  const confirmar = () => {
    const limpo = v.replace(/\s/g, "");
    const n = Number(limpo.includes(",") ? limpo.replace(/\./g, "").replace(",", ".") : limpo);
    if (!Number.isFinite(n) || n < 0) return toast.error("Valor inválido.");
    onConfirm(n);
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={titulo}
      width="max-w-sm"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={confirmar}>Aplicar</Button>
        </>
      }
    >
      {extra}
      <div className="relative">
        <input
          ref={ref}
          value={v}
          onChange={(e) => setV(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && confirmar()}
          className="num h-16 w-full rounded-xl border border-line-strong bg-surface px-4 pr-14 text-right text-[28px] font-semibold outline-none focus:border-brand focus:ring-4 focus:ring-brand/15"
        />
        {sufixo && <span className="absolute right-4 top-1/2 -translate-y-1/2 text-ink-3">{sufixo}</span>}
      </div>
      {ajuda && <p className="mt-2 text-[12px] text-ink-3">{ajuda}</p>}
      <div className="mt-4 grid grid-cols-3 gap-2">
        {["7", "8", "9", "4", "5", "6", "1", "2", "3", ",", "0", "00"].map((t) => (
          <button key={t} onClick={() => tecla(t)} className="num h-14 rounded-xl border border-line bg-surface text-[20px] font-medium hover:bg-sunken active:bg-brand-50">
            {t}
          </button>
        ))}
        <button onClick={() => tecla("C")} className="h-12 rounded-xl border border-line bg-surface font-medium text-danger hover:bg-danger-50">
          C
        </button>
        <button onClick={() => tecla("⌫")} className="col-span-2 grid h-12 place-items-center rounded-xl border border-line bg-surface hover:bg-sunken">
          <Delete className="size-5" />
        </button>
      </div>
    </Modal>
  );
}

/** Autorização por código (PIN) de um supervisor — ecrã "Autenticação" do KIGEST POS. */
export function PinModal({
  pedido,
  onOk,
  onClose,
}: {
  pedido: { permissao: string; descricao: string; desconto?: number } | null;
  onOk: (pin: string, supervisor: string) => void;
  onClose: () => void;
}) {
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => setPin(""), [pedido]);

  async function validar() {
    if (!pedido) return;
    setBusy(true);
    try {
      const s = await call<{ nome: string }>("auth.autorizar", { pin, permissao: pedido.permissao, desconto: pedido.desconto ?? 0 });
      toast.success(`Autorizado por ${s.nome}`);
      onOk(pin, s.nome);
    } catch (e) {
      toast.error((e as Error).message);
      setPin("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={!!pedido}
      onClose={onClose}
      title="Autorização do supervisor"
      width="max-w-sm"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={validar}>
            Autorizar
          </Button>
        </>
      }
    >
      <div className="mb-4 flex items-start gap-3 rounded-xl bg-accent-50 p-3 text-[13px] text-ink-2">
        <KeyRound className="mt-0.5 size-4 shrink-0 text-[#9a6514]" />
        <span>
          <b className="text-ink">{pedido?.descricao}</b> exige autorização. Peça a um supervisor para introduzir o PIN.
        </span>
      </div>
      <Field label="PIN">
        <Input
          autoFocus
          type="password"
          inputMode="numeric"
          className="num h-14 text-center text-[24px] tracking-[0.5em]"
          value={pin}
          onChange={(e) => setPin(e.target.value.replace(/\D/g, "").slice(0, 8))}
          onKeyDown={(e) => e.key === "Enter" && validar()}
        />
      </Field>
    </Modal>
  );
}

/** Texto livre (Descrição de linha / descrição do documento suspenso). */
export function TextoModal({
  open,
  titulo,
  label,
  valorInicial,
  onConfirm,
  onClose,
}: {
  open: boolean;
  titulo: string;
  label: string;
  valorInicial: string;
  onConfirm: (v: string) => void;
  onClose: () => void;
}) {
  const [v, setV] = useState(valorInicial);
  useEffect(() => {
    if (open) setV(valorInicial);
  }, [open, valorInicial]);
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={titulo}
      width="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={() => onConfirm(v)}>Aplicar</Button>
        </>
      }
    >
      <Field label={label}>
        <Input autoFocus value={v} onChange={(e) => setV(e.target.value)} onKeyDown={(e) => e.key === "Enter" && onConfirm(v)} />
      </Field>
    </Modal>
  );
}

export function Segmento<T extends string>({ valor, opcoes, onChange }: { valor: T; opcoes: { id: T; label: string }[]; onChange: (v: T) => void }) {
  return (
    <div className="mb-4 inline-flex rounded-lg bg-sunken p-1">
      {opcoes.map((o) => (
        <button
          key={o.id}
          type="button"
          onClick={() => onChange(o.id)}
          className={cn("rounded-md px-4 py-1.5 text-[13px]", valor === o.id ? "bg-surface font-medium shadow-sm" : "text-ink-2")}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

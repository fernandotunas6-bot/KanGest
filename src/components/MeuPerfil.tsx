// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Camera, ImagePlus, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { fotoDePerfil, useMeuPerfil } from "@/lib/perfil";
import type { Sessao } from "@/lib/types";
import { useSession } from "@/store/session";
import { Button, Field, Input, Modal, cn } from "@/components/ui";

/** A fotografia do utilizador, ou a inicial do nome num círculo da cor da marca. */
export function Avatar({ nome, foto, className }: { nome?: string | null; foto?: string | null; className?: string }) {
  if (foto) return <img src={foto} alt="" draggable={false} className={cn("shrink-0 rounded-full object-cover ring-1 ring-black/5", className)} />;
  return (
    <span className={cn("grid shrink-0 place-items-center rounded-full bg-brand font-semibold text-white", className)}>{(nome || "?").slice(0, 1).toUpperCase()}</span>
  );
}

/**
 * O meu perfil: cada um muda a sua fotografia (do computador ou, no telemóvel, com a câmara), o nome e os contactos.
 * O utilizador, o perfil e as permissões são do administrador (Sistema → Utilizadores e Permissões).
 */
export default function MeuPerfil({ onClose }: { onClose: () => void }) {
  const perfil = useMeuPerfil();
  const qc = useQueryClient();
  const setSessao = useSession((s) => s.setSessao);
  const [f, setF] = useState({ nome: "", email: "", telefone: "" });
  // undefined: a fotografia fica como está; "": tira-se; data URL: a nova
  const [foto, setFoto] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const tactil = typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;

  useEffect(() => {
    if (perfil.data) setF({ nome: perfil.data.nome, email: perfil.data.email, telefone: perfil.data.telefone });
  }, [perfil.data]);

  const mostrada = foto === undefined ? perfil.data?.foto : foto || null;

  async function escolher(ficheiro?: File) {
    if (!ficheiro) return;
    try {
      setFoto(await fotoDePerfil(ficheiro));
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function guardar() {
    setBusy(true);
    try {
      const s = await call<Sessao>("auth.guardarPerfil", { ...f, ...(foto !== undefined ? { foto } : {}) });
      setSessao(s);
      qc.invalidateQueries({ queryKey: ["perfil"] });
      qc.invalidateQueries({ queryKey: ["utilizadores"] });
      toast.success("Perfil guardado.");
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
      title="O meu perfil"
      width="max-w-xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={guardar} loading={busy} disabled={!perfil.data || !f.nome.trim()}>
            Guardar
          </Button>
        </>
      }
    >
      {!perfil.data ? (
        <p className="py-8 text-center text-ink-3">{perfil.isError ? "O servidor ainda não tem o perfil: actualize o KanGest no servidor." : "A carregar…"}</p>
      ) : (
        <div className="grid gap-6 sm:grid-cols-[auto_minmax(0,1fr)]">
          <div className="flex flex-col items-center gap-2">
            <Avatar nome={f.nome} foto={mostrada} className="size-28 text-[40px]" />
            <label className="mt-1 inline-flex cursor-pointer items-center gap-1.5 text-[13px] font-medium text-brand hover:underline">
              <ImagePlus className="size-4" /> Escolher fotografia
              <input type="file" accept="image/jpeg,image/png,image/webp" className="hidden" onChange={(e) => { escolher(e.target.files?.[0]); e.target.value = ""; }} />
            </label>
            {tactil && (
              <label className="inline-flex cursor-pointer items-center gap-1.5 text-[13px] font-medium text-brand hover:underline">
                <Camera className="size-4" /> Tirar com a câmara
                <input type="file" accept="image/*" capture="user" className="hidden" onChange={(e) => { escolher(e.target.files?.[0]); e.target.value = ""; }} />
              </label>
            )}
            {mostrada && (
              <button type="button" onClick={() => setFoto("")} className="inline-flex items-center gap-1.5 text-[13px] text-ink-3 hover:text-danger">
                <Trash2 className="size-4" /> Tirar a fotografia
              </button>
            )}
          </div>
          <div className="space-y-4">
            <Field label="Nome">
              <Input autoFocus value={f.nome} onChange={(e) => setF({ ...f, nome: e.target.value })} maxLength={80} />
            </Field>
            <Field label="Email">
              <Input type="email" value={f.email} onChange={(e) => setF({ ...f, email: e.target.value })} placeholder="nome@empresa.ao" />
            </Field>
            <Field label="Telefone">
              <Input inputMode="tel" value={f.telefone} onChange={(e) => setF({ ...f, telefone: e.target.value })} placeholder="9xx xxx xxx" />
            </Field>
            <p className="rounded-lg bg-sunken px-3 py-2 text-[12.5px] text-ink-3">
              Utilizador <b className="text-ink-2">{perfil.data.login}</b> · {perfil.data.perfil}. O utilizador, o perfil e as permissões muda-os o
              administrador.
            </p>
          </div>
        </div>
      )}
    </Modal>
  );
}

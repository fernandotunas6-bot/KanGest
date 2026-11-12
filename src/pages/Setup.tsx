import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Building2, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { Button, Card, Field, Input, Select } from "@/components/ui";
import { Logo } from "@/components/Logo";

export default function Setup({ onDone }: { onDone: () => void }) {
  const [e, setE] = useState({ nome: "", nif: "", endereco: "", localidade: "Luanda", provincia: "Luanda", telefone: "", email: "", regime_iva: "GERAL" });
  const [a, setA] = useState({ nome: "Administrador", login: "admin", senha: "", confirmar: "" });
  const [busy, setBusy] = useState(false);

  async function guardar(ev: FormEvent) {
    ev.preventDefault();
    if (a.senha !== a.confirmar) return toast.error("As senhas não coincidem.");
    setBusy(true);
    try {
      await call("setup.inicializar", { empresa: e, admin: { nome: a.nome, login: a.login, senha: a.senha } });
      toast.success("Sistema configurado. Inicie sessão.");
      onDone();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-full bg-[radial-gradient(ellipse_at_top_left,var(--color-brand-100),transparent_55%)] px-6 py-12">
      <form onSubmit={guardar} className="mx-auto max-w-3xl">
        <div className="mb-8">
          <Logo className="mb-4 size-12 drop-shadow-sm" />
          <h1 className="text-[28px] font-semibold tracking-tight">Bem-vindo ao KanGest</h1>
          <p className="mt-1 text-ink-2">Configure a empresa e o administrador. Pode alterar estes dados mais tarde.</p>
        </div>

        <Card className="p-6">
          <div className="mb-5 flex items-center gap-2 font-semibold">
            <Building2 className="size-5 text-brand" /> Dados da empresa
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Field label="Nome / Razão social" className="col-span-2">
              <Input required autoFocus value={e.nome} onChange={(x) => setE({ ...e, nome: x.target.value })} />
            </Field>
            <Field label="NIF">
              <Input required value={e.nif} onChange={(x) => setE({ ...e, nif: x.target.value })} />
            </Field>
            <Field label="Regime de IVA">
              <Select value={e.regime_iva} onChange={(x) => setE({ ...e, regime_iva: x.target.value })}>
                <option value="GERAL">Regime Geral</option>
                <option value="SIMPLIFICADO">Regime Simplificado</option>
                <option value="EXCLUSAO">Regime de Exclusão</option>
              </Select>
            </Field>
            <Field label="Endereço" className="col-span-2">
              <Input value={e.endereco} onChange={(x) => setE({ ...e, endereco: x.target.value })} />
            </Field>
            <Field label="Localidade">
              <Input value={e.localidade} onChange={(x) => setE({ ...e, localidade: x.target.value })} />
            </Field>
            <Field label="Província">
              <Input value={e.provincia} onChange={(x) => setE({ ...e, provincia: x.target.value })} />
            </Field>
            <Field label="Telefone">
              <Input value={e.telefone} onChange={(x) => setE({ ...e, telefone: x.target.value })} />
            </Field>
            <Field label="Email">
              <Input type="email" value={e.email} onChange={(x) => setE({ ...e, email: x.target.value })} />
            </Field>
          </div>
        </Card>

        <Card className="mt-5 p-6">
          <div className="mb-5 flex items-center gap-2 font-semibold">
            <ShieldCheck className="size-5 text-brand" /> Administrador
          </div>
          <div className="grid grid-cols-2 gap-4">
            <Field label="Nome">
              <Input required value={a.nome} onChange={(x) => setA({ ...a, nome: x.target.value })} />
            </Field>
            <Field label="Utilizador">
              <Input required value={a.login} onChange={(x) => setA({ ...a, login: x.target.value })} />
            </Field>
            <Field label="Senha">
              <Input required type="password" minLength={4} value={a.senha} onChange={(x) => setA({ ...a, senha: x.target.value })} />
            </Field>
            <Field label="Confirmar senha">
              <Input required type="password" value={a.confirmar} onChange={(x) => setA({ ...a, confirmar: x.target.value })} />
            </Field>
          </div>
        </Card>

        <div className="mt-6 flex justify-end">
          <Button size="lg" loading={busy}>Concluir configuração</Button>
        </div>
      </form>
    </div>
  );
}

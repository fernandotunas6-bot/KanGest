// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Mail } from "lucide-react";
import { call } from "@/lib/api";
import { Button, Card, Field, Input, Select } from "./ui";

interface EmailCfg {
  servidor: string;
  porta: number;
  seguranca: "SSL" | "STARTTLS" | "NENHUMA";
  utilizador: string;
  remetente: string;
  nome: string;
  copia: string;
  tem_senha?: boolean;
}

const VAZIO: EmailCfg = { servidor: "", porta: 465, seguranca: "SSL", utilizador: "", remetente: "", nome: "", copia: "" };

const PREDEFINIDOS: { nome: string; servidor: string; porta: number; seguranca: EmailCfg["seguranca"] }[] = [
  { nome: "Gmail", servidor: "smtp.gmail.com", porta: 465, seguranca: "SSL" },
  { nome: "Outlook / Microsoft 365", servidor: "smtp.office365.com", porta: 587, seguranca: "STARTTLS" },
];

/** Configurações › Email: servidor SMTP para enviar os documentos aos clientes. */
export default function EmailConfig() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["email-config"], queryFn: () => call<EmailCfg>("email.config") });
  const [f, setF] = useState<EmailCfg>(VAZIO);
  const [senha, setSenha] = useState("");
  const [teste, setTeste] = useState("");
  const [busy, setBusy] = useState<"" | "gravar" | "testar">("");
  useEffect(() => {
    if (q.data) {
      setF({ ...VAZIO, ...q.data });
      setTeste(q.data.remetente);
    }
  }, [q.data]);
  const set = <K extends keyof EmailCfg>(k: K, v: EmailCfg[K]) => setF((x) => ({ ...x, [k]: v }));

  async function gravar() {
    setBusy("gravar");
    try {
      await call("email.configGuardar", { ...f, senha: senha || undefined });
      setSenha("");
      toast.success("Configuração de email gravada.");
      qc.invalidateQueries({ queryKey: ["email-config"] });
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }

  async function testar() {
    setBusy("testar");
    try {
      await call("email.testar", { para: teste });
      toast.success(`Email de teste enviado para ${teste}.`);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy("");
    }
  }

  return (
    <div className="grid max-w-3xl gap-4">
      <Card className="p-5">
        <div className="mb-1 flex items-center gap-2 font-semibold">
          <Mail className="size-4 text-ink-3" /> Envio de documentos por email
        </div>
        <p className="mb-4 text-[13px] text-ink-3">
          Os documentos (facturas, recibos, proformas) seguem pelo servidor de email da empresa, com o documento completo no corpo da mensagem e o rodapé da AGT.
        </p>
        <div className="mb-4 flex flex-wrap gap-2">
          {PREDEFINIDOS.map((p) => (
            <Button key={p.nome} size="sm" variant="secondary" onClick={() => setF((x) => ({ ...x, servidor: p.servidor, porta: p.porta, seguranca: p.seguranca }))}>
              {p.nome}
            </Button>
          ))}
        </div>
        <div className="grid grid-cols-[1fr_110px_190px] gap-3">
          <Field label="Servidor (SMTP)">
            <Input value={f.servidor} onChange={(e) => set("servidor", e.target.value)} placeholder="smtp.empresa.ao" />
          </Field>
          <Field label="Porta">
            <Input className="num" type="number" value={f.porta} onChange={(e) => set("porta", Number(e.target.value) || 0)} />
          </Field>
          <Field label="Segurança">
            <Select value={f.seguranca} onChange={(e) => set("seguranca", e.target.value as EmailCfg["seguranca"])}>
              <option value="SSL">SSL/TLS (465)</option>
              <option value="STARTTLS">STARTTLS (587)</option>
              <option value="NENHUMA">Sem cifra (só rede local)</option>
            </Select>
          </Field>
        </div>
        <div className="mt-3 grid grid-cols-2 gap-3">
          <Field label="Utilizador">
            <Input value={f.utilizador} onChange={(e) => set("utilizador", e.target.value)} placeholder="geral@empresa.ao" autoComplete="off" />
          </Field>
          <Field label="Senha" hint={f.tem_senha ? "Gravada (cifrada). Deixe em branco para manter." : "No Gmail e no Outlook use uma «senha de aplicação»."}>
            <Input type="password" value={senha} onChange={(e) => setSenha(e.target.value)} placeholder={f.tem_senha ? "••••••••" : ""} autoComplete="new-password" />
          </Field>
          <Field label="Email do remetente">
            <Input value={f.remetente} onChange={(e) => set("remetente", e.target.value)} placeholder="geral@empresa.ao" />
          </Field>
          <Field label="Nome do remetente" hint="Por omissão, o nome da empresa.">
            <Input value={f.nome} onChange={(e) => set("nome", e.target.value)} />
          </Field>
          <Field label="Cópia oculta (arquivo)" hint="Recebe uma cópia de cada documento enviado." className="col-span-2">
            <Input value={f.copia} onChange={(e) => set("copia", e.target.value)} placeholder="arquivo@empresa.ao" />
          </Field>
        </div>
        {f.seguranca === "NENHUMA" && (
          <p className="mt-3 rounded-lg bg-danger-50 px-3 py-2 text-[12.5px] text-danger">Sem cifra, a senha e os documentos passam em claro: use só com um servidor da rede local.</p>
        )}
        <div className="mt-4 flex justify-end">
          <Button loading={busy === "gravar"} onClick={gravar}>
            Gravar
          </Button>
        </div>
      </Card>
      <Card className="flex flex-wrap items-end gap-3 p-5">
        <Field label="Enviar um email de teste para" className="min-w-72 flex-1">
          <Input value={teste} onChange={(e) => setTeste(e.target.value)} placeholder="geral@empresa.ao" />
        </Field>
        <Button variant="secondary" loading={busy === "testar"} disabled={!teste.trim() || !q.data?.servidor} onClick={testar}>
          Enviar teste
        </Button>
      </Card>
    </div>
  );
}

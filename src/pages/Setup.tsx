// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState, type FormEvent } from "react";
import { toast } from "sonner";
import { Building2, Coins, DatabaseBackup, Palette, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { Button, Card, Field, Input, Select } from "@/components/ui";
import { Logo } from "@/components/Logo";
import { EscolherRamo } from "@/components/EscolherRamo";
import { EscolherCores } from "@/components/EscolherCores";
import { useAparencia } from "@/lib/aparencia";
import type { Cores } from "@/lib/cores";
import { Store } from "lucide-react";
import { RestaurarCopia } from "@/pages/Configuracoes";
import { cn } from "@/components/ui";

/** Moedas que se podem aceitar em numerário no POS logo na instalação (as da tabela de moedas de origem). */
const MOEDAS_POS = [
  { codigo: "USD", nome: "Dólar americano", simbolo: "US$" },
  { codigo: "EUR", nome: "Euro", simbolo: "€" },
  { codigo: "ZAR", nome: "Rand sul-africano", simbolo: "R" },
  { codigo: "CNY", nome: "Yuan chinês", simbolo: "¥" },
];

export default function Setup({ onDone }: { onDone: () => void }) {
  const [e, setE] = useState({ nome: "", nif: "", endereco: "", localidade: "Luanda", provincia: "Luanda", telefone: "", email: "", regime_iva: "GERAL" });
  const [a, setA] = useState({ nome: "Administrador", login: "admin", senha: "", confirmar: "" });
  const [busy, setBusy] = useState(false);
  const [ramo, setRamo] = useState<string>("comercio");
  const [cores, setCoresLocal] = useState<Cores>({ paleta: "petroleo" });
  // moedas aceites em numerário no POS, com o câmbio do dia (nenhuma: o POS só recebe kwanzas)
  const [moedasPos, setMoedasPos] = useState<Record<string, string>>({});
  // o jogo escolhido vê-se logo neste ecrã
  const escolherCores = (c: Cores) => {
    setCoresLocal(c);
    useAparencia.getState().setCores(c);
  };
  // computador novo: restaurar uma cópia de segurança em vez de configurar do zero
  const [copia, setCopia] = useState("");
  const [restaurar, setRestaurar] = useState(false);

  async function guardar(ev: FormEvent) {
    ev.preventDefault();
    if (a.senha !== a.confirmar) return toast.error("As senhas não coincidem.");
    const moedas_pos = Object.entries(moedasPos).map(([codigo, c]) => ({ codigo, cambio: Number(c.replace(/\s/g, "").replace(",", ".")) || 0 }));
    const semCambio = moedas_pos.find((m) => m.cambio <= 0);
    if (semCambio) return toast.error(`Indique o câmbio do dia de ${MOEDAS_POS.find((x) => x.codigo === semCambio.codigo)?.nome ?? semCambio.codigo} (quantos kwanzas vale 1).`);
    setBusy(true);
    try {
      await call("setup.inicializar", { empresa: e, admin: { nome: a.nome, login: a.login, senha: a.senha }, ramo, cores, moedas_pos });
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
          <div className="mb-1 flex items-center gap-2 font-semibold">
            <Store className="size-5 text-brand" /> Ramo de actividade
          </div>
          <p className="mb-4 text-[13px] text-ink-3">O KanGest adapta os artigos e o POS ao seu negócio. Pode mudar depois em Configurações → Empresa.</p>
          <EscolherRamo valor={ramo} onChange={setRamo} />
        </Card>

        <Card className="mt-5 p-6">
          <div className="mb-1 flex items-center gap-2 font-semibold">
            <Palette className="size-5 text-brand" /> Cores do sistema
          </div>
          <p className="mb-4 text-[13px] text-ink-3">O jogo de cores do menu, dos botões e do ecrã de entrada. Pode mudar depois em Configurações → Aparência.</p>
          <EscolherCores valor={cores} onChange={escolherCores} />
        </Card>

        <Card className="mt-5 p-6">
          <div className="mb-1 flex items-center gap-2 font-semibold">
            <Coins className="size-5 text-brand" /> Moedas no POS
          </div>
          <p className="mb-4 text-[13px] text-ink-3">
            O POS recebe sempre kwanzas. Marque só as moedas que aceita em numerário (o troco sai em kwanzas): só essas aparecem no pagamento. Pode mudar depois em Tabelas →
            Moedas.
          </p>
          <div className="grid gap-2 sm:grid-cols-2">
            {MOEDAS_POS.map((m) => {
              const marcada = m.codigo in moedasPos;
              return (
                <div key={m.codigo} className={cn("flex items-center gap-3 rounded-xl border px-3 py-2.5", marcada ? "border-brand/40 bg-brand-50/50" : "border-line")}>
                  <label className="flex min-w-0 flex-1 items-center gap-2 text-[14px]">
                    <input
                      type="checkbox"
                      checked={marcada}
                      onChange={(x) =>
                        setMoedasPos((v) => {
                          const n = { ...v };
                          if (x.target.checked) n[m.codigo] = "";
                          else delete n[m.codigo];
                          return n;
                        })
                      }
                    />
                    <span className="truncate">
                      {m.nome} <span className="text-ink-3">({m.simbolo})</span>
                    </span>
                  </label>
                  {marcada && (
                    <Input
                      aria-label={`Câmbio do ${m.nome} em kwanzas`}
                      className="num h-9 w-28 text-right"
                      inputMode="decimal"
                      placeholder="Kz por 1"
                      value={moedasPos[m.codigo]}
                      onChange={(x) => setMoedasPos((v) => ({ ...v, [m.codigo]: x.target.value }))}
                    />
                  )}
                </div>
              );
            })}
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

      <Card className="mx-auto mt-8 max-w-3xl p-6">
        <div className="mb-2 flex items-center gap-2 font-semibold">
          <DatabaseBackup className="size-5 text-brand" /> Já tem uma cópia de segurança do KanGest?
        </div>
        <p className="mb-3 text-ink-2">Num computador novo, restaure a cópia (de uma pen ou da pasta do OneDrive / Google Drive) em vez de configurar do zero.</p>
        <div className="flex items-end gap-2">
          <Field label="Caminho da cópia (.db ou .kgc)" className="flex-1">
            <Input className="font-mono" value={copia} onChange={(x) => setCopia(x.target.value)} placeholder="E:\Copias KanGest\kangest-….kgc" />
          </Field>
          <Button type="button" variant="secondary" disabled={!copia.trim()} onClick={() => setRestaurar(true)}>
            Restaurar
          </Button>
        </div>
      </Card>
      {restaurar && (
        <RestaurarCopia
          alvo={{ ficheiro: copia.trim(), cifrada: copia.trim().endsWith(".kgc"), descricao: copia.trim() }}
          temSenha={false}
          onClose={() => setRestaurar(false)}
          metodo="setup.restaurar"
        />
      )}
    </div>
  );
}

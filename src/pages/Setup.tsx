// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState, type FormEvent, type ReactNode } from "react";
import { toast } from "sonner";
import { ArrowLeft, ArrowRight, Building2, Check, Coins, DatabaseBackup, GraduationCap, ShieldCheck } from "lucide-react";
import { call } from "@/lib/api";
import { Badge, Button, Card, Field, Input, Select, cn } from "@/components/ui";
import { Logo } from "@/components/Logo";
import { FAMILIA_ENSINO, MODOS, MOTIVO_ENSINO, artigosEscola, modoPorId, type ModoId } from "@/lib/modos";
import { kz, parseKz } from "@/lib/format";
import { RestaurarCopia } from "@/pages/Configuracoes";

/** Moedas que se podem aceitar em numerário no POS logo na instalação (as da tabela de moedas de origem). */
const MOEDAS_POS = [
  { codigo: "USD", nome: "Dólar americano", simbolo: "US$" },
  { codigo: "EUR", nome: "Euro", simbolo: "€" },
  { codigo: "ZAR", nome: "Rand sul-africano", simbolo: "R" },
  { codigo: "CNY", nome: "Yuan chinês", simbolo: "¥" },
];

const REGIMES: Record<string, string> = { GERAL: "Regime Geral", SIMPLIFICADO: "Regime Simplificado", EXCLUSAO: "Regime de Exclusão" };

/**
 * Instalação curta, só com o que é preciso para facturar e vender: o tipo de gestão, os dados fiscais,
 * o administrador e (conforme o tipo) as moedas do POS ou a propina. Ramo, cores e o resto mudam-se
 * depois em Configurações.
 */
type Passo = "modo" | "dados" | "rever";
const PASSOS: Passo[] = ["modo", "dados", "rever"];
const TITULOS: Record<Passo, string> = { modo: "Tipo de gestão", dados: "Dados", rever: "Rever e começar" };

export default function Setup({ onDone }: { onDone: () => void }) {
  const [modo, setModo] = useState<ModoId>("loja");
  const [e, setE] = useState({ nome: "", nif: "", endereco: "", localidade: "Luanda", provincia: "Luanda", telefone: "", email: "", regime_iva: "GERAL" });
  const [a, setA] = useState({ login: "admin", senha: "", confirmar: "" });
  // escola: valores da propina mensal e da matrícula (vazios = a definir em Artigos)
  const [propina, setPropina] = useState("");
  const [matricula, setMatricula] = useState("");
  // moedas aceites em numerário no POS, com o câmbio do dia (nenhuma: o POS só recebe kwanzas)
  const [moedasPos, setMoedasPos] = useState<Record<string, string>>({});
  const [passo, setPasso] = useState(0);
  const [busy, setBusy] = useState(false);
  // computador novo: restaurar uma cópia de segurança em vez de configurar do zero
  const [copia, setCopia] = useState("");
  const [restaurar, setRestaurar] = useState(false);

  const escola = modo === "escola";
  const atual = PASSOS[passo];
  const moedasEscolhidas = () => Object.entries(moedasPos).map(([codigo, c]) => ({ codigo, cambio: Number(c.replace(/\s/g, "").replace(",", ".")) || 0 }));

  function conferir(): string | null {
    const sem = moedasEscolhidas().find((m) => m.cambio <= 0);
    if (sem) return `Indique o câmbio do dia de ${MOEDAS_POS.find((x) => x.codigo === sem.codigo)?.nome ?? sem.codigo} (quantos kwanzas vale 1).`;
    if (a.senha !== a.confirmar) return "As senhas não coincidem.";
    return null;
  }

  async function seguinte(ev: FormEvent) {
    ev.preventDefault();
    if (atual === "dados") {
      const erro = conferir();
      if (erro) return toast.error(erro);
    }
    if (atual !== "rever") {
      setPasso(passo + 1);
      window.scrollTo({ top: 0 });
      return;
    }
    setBusy(true);
    try {
      await call("setup.inicializar", {
        empresa: e,
        admin: { nome: "Administrador", login: a.login, senha: a.senha },
        modo,
        ramo: escola ? "servicos" : "comercio",
        moedas_pos: escola ? [] : moedasEscolhidas(),
        base: escola ? { familias: [FAMILIA_ENSINO], artigos: artigosEscola(parseKz(propina), parseKz(matricula)) } : undefined,
      });
      toast.success("Sistema configurado. Inicie sessão.");
      onDone();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-full bg-[radial-gradient(ellipse_at_top_left,var(--color-brand-100),transparent_55%)] px-4 py-10 sm:px-6">
      <div className="mx-auto max-w-2xl">
        <div className="mb-6">
          <Logo className="mb-4 size-12 drop-shadow-sm" />
          <h1 className="text-[28px] font-semibold tracking-tight">Bem-vindo ao KanGest</h1>
          <p className="mt-1 text-ink-2">Três passos e começa a facturar. O resto ajusta-se depois em Configurações.</p>
        </div>

        <ol className="mb-5 flex flex-wrap gap-1.5" aria-label="Passos da instalação">
          {PASSOS.map((p, i) => (
            <li key={p}>
              <button
                type="button"
                onClick={() => i < passo && setPasso(i)}
                disabled={i > passo}
                aria-current={i === passo ? "step" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-full border px-3 py-1 text-[12.5px] transition",
                  i === passo ? "border-brand bg-brand text-white" : i < passo ? "border-brand/30 bg-surface text-ink hover:border-brand" : "border-line bg-surface/60 text-ink-3",
                )}
              >
                <span className={cn("grid size-4 place-items-center rounded-full text-[10.5px] font-semibold", i === passo ? "bg-white/25" : i < passo ? "bg-ok text-white" : "bg-sunken")}>
                  {i < passo ? <Check className="size-3" /> : i + 1}
                </span>
                {TITULOS[p]}
              </button>
            </li>
          ))}
        </ol>

        <form onSubmit={seguinte}>
          {atual === "modo" && (
            <Card className="p-6">
              <div className="mb-3 font-semibold">Que tipo de gestão vai fazer?</div>
              <div className="grid gap-2">
                {MODOS.map((m) => {
                  const sel = modo === m.id;
                  return (
                    <button
                      key={m.id}
                      type="button"
                      disabled={m.emBreve}
                      onClick={() => setModo(m.id)}
                      aria-pressed={sel}
                      className={cn(
                        "flex items-center gap-3 rounded-xl border p-3 text-left transition",
                        sel ? "border-brand bg-brand-50/60 ring-2 ring-brand/20" : "border-line hover:border-brand/40 hover:bg-sunken/60",
                        m.emBreve && "cursor-not-allowed opacity-60 hover:border-line hover:bg-transparent",
                      )}
                    >
                      <span className={cn("grid size-10 shrink-0 place-items-center rounded-lg", sel ? "bg-brand text-white" : "bg-sunken text-ink-2")}>
                        <m.icone className="size-5" />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-2 text-[15px] font-semibold">
                          {m.nome}
                          {m.emBreve && <Badge tone="accent">Em breve</Badge>}
                        </span>
                        <span className="block text-[12.5px] leading-snug text-ink-3">{m.descricao}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </Card>
          )}

          {atual === "dados" && (
            <div className="space-y-5">
              <Card className="p-6">
                <Titulo icone={escola ? <GraduationCap className="size-5 text-brand" /> : <Building2 className="size-5 text-brand" />}>{escola ? "Escola" : "Empresa"}</Titulo>
                <div className="grid grid-cols-2 gap-4">
                  <Field label={escola ? "Nome da escola" : "Nome / Razão social"} className="col-span-2">
                    <Input required autoFocus value={e.nome} onChange={(x) => setE({ ...e, nome: x.target.value })} />
                  </Field>
                  <Field label="NIF">
                    <Input required value={e.nif} onChange={(x) => setE({ ...e, nif: x.target.value })} />
                  </Field>
                  <Field label="Regime de IVA">
                    <Select value={e.regime_iva} onChange={(x) => setE({ ...e, regime_iva: x.target.value })}>
                      {Object.entries(REGIMES).map(([v, l]) => (
                        <option key={v} value={v}>
                          {l}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field label="Endereço">
                    <Input value={e.endereco} onChange={(x) => setE({ ...e, endereco: x.target.value })} />
                  </Field>
                  <Field label="Telefone">
                    <Input value={e.telefone} onChange={(x) => setE({ ...e, telefone: x.target.value })} />
                  </Field>
                </div>
              </Card>

              {escola ? (
                <Card className="p-6">
                  <Titulo icone={<Coins className="size-5 text-brand" />}>Propina</Titulo>
                  <p className="mb-4 text-[13px] text-ink-3">Isentas de IVA (art. 12.º, n.º 1, al. l) do CIVA — {MOTIVO_ENSINO}). Pode deixar em branco e definir depois em Artigos.</p>
                  <div className="grid grid-cols-2 gap-4">
                    <Field label="Propina mensal (Kz)">
                      <Input className="num text-right" inputMode="decimal" placeholder="a definir" value={propina} onChange={(x) => setPropina(x.target.value)} />
                    </Field>
                    <Field label="Matrícula (Kz)">
                      <Input className="num text-right" inputMode="decimal" placeholder="a definir" value={matricula} onChange={(x) => setMatricula(x.target.value)} />
                    </Field>
                  </div>
                </Card>
              ) : (
                <Card className="p-6">
                  <Titulo icone={<Coins className="size-5 text-brand" />}>Moedas no POS</Titulo>
                  <p className="mb-4 text-[13px] text-ink-3">O POS recebe sempre kwanzas. Marque só as moedas que aceita em numerário (o troco sai em kwanzas).</p>
                  <div className="grid gap-2 sm:grid-cols-2">
                    {MOEDAS_POS.map((m) => {
                      const marcada = m.codigo in moedasPos;
                      return (
                        <div key={m.codigo} className={cn("flex items-center gap-3 rounded-xl border px-3 py-2", marcada ? "border-brand/40 bg-brand-50/50" : "border-line")}>
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
              )}

              <Card className="p-6">
                <Titulo icone={<ShieldCheck className="size-5 text-brand" />}>Administrador</Titulo>
                <div className="grid gap-4 sm:grid-cols-3">
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
            </div>
          )}

          {atual === "rever" && (
            <Card className="p-6">
              <Titulo icone={<Check className="size-5 text-brand" />}>Confira antes de começar</Titulo>
              <div className="mt-2 text-[14px]">
                <Linha r="Tipo de gestão" v={modoPorId(modo).nome} />
                <Linha r={escola ? "Escola" : "Empresa"} v={e.nome} />
                <Linha r="NIF" v={e.nif} />
                <Linha r="Regime de IVA" v={REGIMES[e.regime_iva]} />
                {escola ? (
                  <>
                    <Linha r="Propina mensal" v={parseKz(propina) > 0 ? kz(parseKz(propina)) : "a definir"} />
                    <Linha r="Matrícula" v={parseKz(matricula) > 0 ? kz(parseKz(matricula)) : "a definir"} />
                  </>
                ) : (
                  <Linha
                    r="Moedas no POS"
                    v={moedasEscolhidas().length ? `Kwanza e ${moedasEscolhidas().map((m) => `${MOEDAS_POS.find((x) => x.codigo === m.codigo)?.nome} (${m.cambio.toLocaleString("pt-AO")} Kz)`).join(", ")}` : "Só kwanzas"}
                  />
                )}
                <Linha r="Administrador" v={a.login} />
              </div>
              <Button type="button" size="sm" variant="ghost" className="mt-2" onClick={() => setPasso(1)}>
                Alterar dados
              </Button>
            </Card>
          )}

          <div className="mt-6 flex items-center justify-between gap-3">
            {passo > 0 ? (
              <Button type="button" variant="secondary" onClick={() => setPasso(passo - 1)}>
                <ArrowLeft className="size-4" /> Anterior
              </Button>
            ) : (
              <span />
            )}
            {atual === "rever" ? (
              <Button size="lg" loading={busy}>
                <Check className="size-5" /> Concluir e começar
              </Button>
            ) : (
              <Button size="lg">
                Seguinte <ArrowRight className="size-5" />
              </Button>
            )}
          </div>
        </form>

        {atual === "modo" && (
          <Card className="mt-8 p-6">
            <div className="mb-2 flex items-center gap-2 font-semibold">
              <DatabaseBackup className="size-5 text-brand" /> Já tem uma cópia de segurança do KanGest?
            </div>
            <div className="flex items-end gap-2">
              <Field label="Caminho da cópia (.db ou .kgc)" className="flex-1">
                <Input className="font-mono" value={copia} onChange={(x) => setCopia(x.target.value)} placeholder="E:\Copias KanGest\kangest-….kgc" />
              </Field>
              <Button type="button" variant="secondary" disabled={!copia.trim()} onClick={() => setRestaurar(true)}>
                Restaurar
              </Button>
            </div>
          </Card>
        )}
      </div>
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

function Titulo({ icone, children }: { icone: ReactNode; children: ReactNode }) {
  return <div className="mb-3 flex items-center gap-2 font-semibold">{icone} {children}</div>;
}

function Linha({ r, v }: { r: string; v?: string }) {
  return (
    <div className="flex gap-3 border-b border-line py-1.5 last:border-0">
      <span className="w-36 shrink-0 text-ink-3">{r}</span>
      <span className="min-w-0 break-words">{v || "—"}</span>
    </div>
  );
}

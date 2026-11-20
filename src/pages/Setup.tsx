// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useState, type FormEvent, type ReactNode } from "react";
import { toast } from "sonner";
import { ArrowLeft, ArrowRight, Building2, Check, CheckCircle2, ClipboardCheck, Coins, DatabaseBackup, GraduationCap, Palette, Pencil, Plus, ShieldCheck, Store, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { Badge, Button, Card, Field, Input, Select, cn } from "@/components/ui";
import { Logo } from "@/components/Logo";
import { EscolherRamo } from "@/components/EscolherRamo";
import { EscolherCores } from "@/components/EscolherCores";
import { useAparencia } from "@/lib/aparencia";
import { PALETAS, type Cores } from "@/lib/cores";
import { ramoPorId } from "@/lib/ramos";
import { ARTIGOS_ESCOLA, FAMILIA_ENSINO, MODOS, MOTIVO_ENSINO, modoPorId, type ArtigoBase, type ModoId } from "@/lib/modos";
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

type Passo = "modo" | "empresa" | "actividade" | "aparencia" | "admin" | "rever";
const PASSOS: Passo[] = ["modo", "empresa", "actividade", "aparencia", "admin", "rever"];

/** Linha de propina ou emolumento na revisão: escolhida ou não, com o preço escrito. */
interface LinhaEscola extends ArtigoBase {
  chave: string;
  escolhido: boolean;
  precoTexto: string;
}

const linhasIniciais = (): LinhaEscola[] => ARTIGOS_ESCOLA.map((a) => ({ ...a, chave: a.referencia, escolhido: true, precoTexto: "" }));

export default function Setup({ onDone }: { onDone: () => void }) {
  const [modo, setModo] = useState<ModoId>("loja");
  const [e, setE] = useState({ nome: "", nif: "", endereco: "", localidade: "Luanda", provincia: "Luanda", telefone: "", email: "", regime_iva: "GERAL" });
  const [a, setA] = useState({ nome: "Administrador", login: "admin", senha: "", confirmar: "" });
  const [busy, setBusy] = useState(false);
  const [ramo, setRamo] = useState<string>("comercio");
  // famílias sugeridas pelo ramo que o utilizador desmarcou (as outras criam-se)
  const [semFamilias, setSemFamilias] = useState<string[]>([]);
  const [linhas, setLinhas] = useState<LinhaEscola[]>(linhasIniciais);
  const [cores, setCoresLocal] = useState<Cores>({ paleta: "petroleo" });
  // moedas aceites em numerário no POS, com o câmbio do dia (nenhuma: o POS só recebe kwanzas)
  const [moedasPos, setMoedasPos] = useState<Record<string, string>>({});
  const [passo, setPasso] = useState(0);
  const [visto, setVisto] = useState(0);
  // o jogo escolhido vê-se logo neste ecrã
  const escolherCores = (c: Cores) => {
    setCoresLocal(c);
    useAparencia.getState().setCores(c);
  };
  // computador novo: restaurar uma cópia de segurança em vez de configurar do zero
  const [copia, setCopia] = useState("");
  const [restaurar, setRestaurar] = useState(false);

  const escola = modo === "escola";
  const atual = PASSOS[passo];
  const ramoAtual = ramoPorId(escola ? "servicos" : ramo);
  const familias = escola ? [FAMILIA_ENSINO] : ramoAtual.familias.filter((f) => !semFamilias.includes(f));
  const escolhidas = linhas.filter((l) => l.escolhido && l.descricao.trim());
  const titulos: Record<Passo, string> = {
    modo: "Tipo de gestão",
    empresa: escola ? "Escola" : "Empresa",
    actividade: escola ? "Propinas e emolumentos" : "Actividade",
    aparencia: "Cores e moedas",
    admin: "Administrador",
    rever: "Rever e começar",
  };

  const moedasEscolhidas = () => Object.entries(moedasPos).map(([codigo, c]) => ({ codigo, cambio: Number(c.replace(/\s/g, "").replace(",", ".")) || 0 }));

  /** Confere o passo actual antes de avançar (os campos obrigatórios já os confere o browser). */
  function conferir(p: Passo): string | null {
    if (p === "actividade" && escola) {
      const refs = escolhidas.map((l) => l.referencia.trim().toUpperCase());
      if (refs.some((r) => !/^[A-Z0-9._/-]{1,20}$/.test(r))) return "Cada linha precisa de uma referência curta (letras, algarismos e - _ . /).";
      const dup = refs.find((r, i) => refs.indexOf(r) !== i);
      if (dup) return `A referência ${dup} aparece duas vezes.`;
      if (escolhidas.some((l) => l.precoTexto.trim() && parseKz(l.precoTexto) < 0)) return "Há um preço inválido.";
    }
    if (p === "aparencia") {
      const sem = moedasEscolhidas().find((m) => m.cambio <= 0);
      if (sem) return `Indique o câmbio do dia de ${MOEDAS_POS.find((x) => x.codigo === sem.codigo)?.nome ?? sem.codigo} (quantos kwanzas vale 1).`;
    }
    if (p === "admin" && a.senha !== a.confirmar) return "As senhas não coincidem.";
    return null;
  }

  function irPara(i: number) {
    if (i <= visto) setPasso(i);
  }

  async function seguinte(ev: FormEvent) {
    ev.preventDefault();
    const erro = conferir(atual);
    if (erro) return toast.error(erro);
    if (atual !== "rever") {
      const n = passo + 1;
      setPasso(n);
      setVisto((v) => Math.max(v, n));
      window.scrollTo({ top: 0 });
      return;
    }
    // revisão: confere tudo de novo (pode ter voltado atrás e mudado)
    for (const p of PASSOS) {
      const e2 = conferir(p);
      if (e2) {
        setPasso(PASSOS.indexOf(p));
        return toast.error(e2);
      }
    }
    setBusy(true);
    try {
      const base = escola
        ? {
            familias,
            artigos: escolhidas.map((l) => ({
              referencia: l.referencia.trim().toUpperCase(),
              descricao: l.descricao.trim(),
              familia: l.familia,
              preco: l.precoTexto.trim() ? parseKz(l.precoTexto) : 0,
              tipo: l.tipo,
              unidade: l.unidade,
              motivo_isencao: l.motivo_isencao,
            })),
          }
        : { familias, artigos: [] };
      await call("setup.inicializar", {
        empresa: e,
        admin: { nome: a.nome, login: a.login, senha: a.senha },
        modo,
        ramo: ramoAtual.id,
        cores,
        moedas_pos: moedasEscolhidas(),
        base,
      });
      toast.success("Sistema configurado. Inicie sessão.");
      onDone();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const mudarLinha = (chave: string, m: Partial<LinhaEscola>) => setLinhas((ls) => ls.map((l) => (l.chave === chave ? { ...l, ...m } : l)));
  function novaLinha() {
    const n = linhas.filter((l) => l.chave.startsWith("novo")).length + 1;
    setLinhas((ls) => [
      ...ls,
      { chave: `novo${Date.now()}`, referencia: `EMOL${n}`, descricao: "", familia: FAMILIA_ENSINO, preco: 0, tipo: "S", motivo_isencao: MOTIVO_ENSINO, escolhido: true, precoTexto: "" },
    ]);
  }

  return (
    <div className="min-h-full bg-[radial-gradient(ellipse_at_top_left,var(--color-brand-100),transparent_55%)] px-4 py-10 sm:px-6">
      <div className="mx-auto max-w-3xl">
        <div className="mb-6">
          <Logo className="mb-4 size-12 drop-shadow-sm" />
          <h1 className="text-[28px] font-semibold tracking-tight">Bem-vindo ao KanGest</h1>
          <p className="mt-1 text-ink-2">Escolha o tipo de gestão e ajuste tudo antes de começar. Pode alterar estes dados mais tarde.</p>
        </div>

        <ol className="mb-5 flex flex-wrap gap-1.5" aria-label="Passos da instalação">
          {PASSOS.map((p, i) => (
            <li key={p}>
              <button
                type="button"
                onClick={() => irPara(i)}
                disabled={i > visto}
                aria-current={i === passo ? "step" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-full border px-3 py-1 text-[12.5px] transition",
                  i === passo ? "border-brand bg-brand text-white" : i <= visto ? "border-brand/30 bg-surface text-ink hover:border-brand" : "border-line bg-surface/60 text-ink-3",
                )}
              >
                <span className={cn("grid size-4 place-items-center rounded-full text-[10.5px] font-semibold", i === passo ? "bg-white/25" : i < visto ? "bg-ok text-white" : "bg-sunken")}>
                  {i < visto && i !== passo ? <Check className="size-3" /> : i + 1}
                </span>
                {titulos[p]}
              </button>
            </li>
          ))}
        </ol>

        <form onSubmit={seguinte}>
          {atual === "modo" && (
            <Card className="p-6">
              <Titulo icone={<ClipboardCheck className="size-5 text-brand" />}>Que tipo de gestão vai fazer?</Titulo>
              <p className="mb-4 text-[13px] text-ink-3">Decide os menus e o que o KanGest prepara para começar. Pode mudar depois em Configurações → Empresa.</p>
              <div className="grid gap-3 sm:grid-cols-3">
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
                        "relative flex flex-col items-start gap-2 rounded-xl border p-4 text-left transition",
                        sel ? "border-brand bg-brand-50/60 ring-2 ring-brand/20" : "border-line hover:border-brand/40 hover:bg-sunken/60",
                        m.emBreve && "cursor-not-allowed opacity-60 hover:border-line hover:bg-transparent",
                      )}
                    >
                      {m.emBreve && (
                        <Badge tone="accent" className="absolute right-3 top-3">
                          Em breve
                        </Badge>
                      )}
                      <span className={cn("grid size-10 place-items-center rounded-lg", sel ? "bg-brand text-white" : "bg-sunken text-ink-2")}>
                        <m.icone className="size-5" />
                      </span>
                      <span className="text-[15px] font-semibold">{m.nome}</span>
                      <span className="text-[12.5px] leading-snug text-ink-3">{m.descricao}</span>
                    </button>
                  );
                })}
              </div>
              <ul className="mt-3 grid gap-1 rounded-xl bg-sunken/70 p-3 text-[12.5px] text-ink-2 sm:grid-cols-2">
                {modoPorId(modo).dicas.map((d) => (
                  <li key={d} className="flex items-start gap-1.5">
                    <CheckCircle2 className="mt-0.5 size-3.5 shrink-0 text-ok" /> {d}
                  </li>
                ))}
              </ul>
            </Card>
          )}

          {atual === "empresa" && (
            <Card className="p-6">
              <Titulo icone={escola ? <GraduationCap className="size-5 text-brand" /> : <Building2 className="size-5 text-brand" />}>
                {escola ? "Dados da escola" : "Dados da empresa"}
              </Titulo>
              <div className="mt-4 grid grid-cols-2 gap-4">
                <Field label={escola ? "Nome da escola / entidade titular" : "Nome / Razão social"} className="col-span-2">
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
          )}

          {atual === "actividade" && !escola && (
            <Card className="p-6">
              <Titulo icone={<Store className="size-5 text-brand" />}>Ramo de actividade</Titulo>
              <p className="mb-4 text-[13px] text-ink-3">O KanGest adapta os artigos e o POS ao seu negócio. Pode mudar depois em Configurações → Empresa.</p>
              <EscolherRamo valor={ramo} onChange={setRamo} />
              <div className="mt-5 text-[13.5px] font-medium">Famílias de artigos para começar</div>
              <p className="mb-2 text-[12.5px] text-ink-3">Desmarque as que não usa; pode criar outras depois em Tabelas.</p>
              <div className="flex flex-wrap gap-2">
                {ramoAtual.familias.map((f) => {
                  const marcada = !semFamilias.includes(f);
                  return (
                    <label key={f} className={cn("flex cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-[13px]", marcada ? "border-brand/40 bg-brand-50/50" : "border-line text-ink-3")}>
                      <input type="checkbox" checked={marcada} onChange={() => setSemFamilias((s) => (marcada ? [...s, f] : s.filter((x) => x !== f)))} />
                      {f}
                    </label>
                  );
                })}
              </div>
            </Card>
          )}

          {atual === "actividade" && escola && (
            <Card className="p-6">
              <Titulo icone={<GraduationCap className="size-5 text-brand" />}>Propinas e emolumentos</Titulo>
              <p className="mb-4 text-[13px] text-ink-3">
                Ficam na família «{FAMILIA_ENSINO}», como serviços isentos de IVA (art. 12.º, n.º 1, al. l) do CIVA — {MOTIVO_ENSINO}). Ponha os preços que já conhece; os outros
                definem-se depois em Artigos.
              </p>
              <div className="overflow-x-auto">
                <table className="w-full min-w-[520px] text-[13.5px]">
                  <thead>
                    <tr className="text-left text-[12px] text-ink-3">
                      <th className="w-8 pb-1.5" />
                      <th className="pb-1.5 font-medium">Descrição</th>
                      <th className="w-28 pb-1.5 font-medium">Referência</th>
                      <th className="w-36 pb-1.5 text-right font-medium">Preço (Kz)</th>
                      <th className="w-8 pb-1.5" />
                    </tr>
                  </thead>
                  <tbody>
                    {linhas.map((l) => (
                      <tr key={l.chave} className={cn("border-t border-line", !l.escolhido && "opacity-55")}>
                        <td className="py-1.5">
                          <input type="checkbox" aria-label={`Criar ${l.descricao || "esta linha"}`} checked={l.escolhido} onChange={(x) => mudarLinha(l.chave, { escolhido: x.target.checked })} />
                        </td>
                        <td className="py-1.5 pr-2">
                          <Input
                            className="h-9"
                            aria-label="Descrição"
                            required={l.escolhido}
                            placeholder="Ex.: Exame especial"
                            value={l.descricao}
                            onChange={(x) => mudarLinha(l.chave, { descricao: x.target.value })}
                          />
                        </td>
                        <td className="py-1.5 pr-2">
                          <Input className="h-9 font-mono uppercase" aria-label="Referência" value={l.referencia} maxLength={20} onChange={(x) => mudarLinha(l.chave, { referencia: x.target.value })} />
                        </td>
                        <td className="py-1.5">
                          <Input
                            className="num h-9 text-right"
                            aria-label={`Preço de ${l.descricao}`}
                            inputMode="decimal"
                            placeholder="a definir"
                            value={l.precoTexto}
                            onChange={(x) => mudarLinha(l.chave, { precoTexto: x.target.value })}
                          />
                        </td>
                        <td className="py-1.5 pl-1">
                          {l.chave.startsWith("novo") && (
                            <button type="button" title="Tirar esta linha" onClick={() => setLinhas((ls) => ls.filter((x) => x.chave !== l.chave))} className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-danger">
                              <Trash2 className="size-4" />
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={novaLinha}>
                <Plus className="size-4" /> Acrescentar emolumento
              </Button>
            </Card>
          )}

          {atual === "aparencia" && (
            <>
              <Card className="p-6">
                <Titulo icone={<Palette className="size-5 text-brand" />}>Cores do sistema</Titulo>
                <p className="mb-4 text-[13px] text-ink-3">O jogo de cores do menu, dos botões e do ecrã de entrada. Pode mudar depois em Configurações → Aparência.</p>
                <EscolherCores valor={cores} onChange={escolherCores} />
              </Card>
              <Card className="mt-5 p-6">
                <Titulo icone={<Coins className="size-5 text-brand" />}>Moedas no POS</Titulo>
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
            </>
          )}

          {atual === "admin" && (
            <Card className="p-6">
              <Titulo icone={<ShieldCheck className="size-5 text-brand" />}>Administrador</Titulo>
              <p className="mb-4 text-[13px] text-ink-3">Quem entra primeiro e cria os outros utilizadores.</p>
              <div className="grid grid-cols-2 gap-4">
                <Field label="Nome">
                  <Input required autoFocus value={a.nome} onChange={(x) => setA({ ...a, nome: x.target.value })} />
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
          )}

          {atual === "rever" && (
            <div className="space-y-4">
              <p className="text-[13.5px] text-ink-2">Confira tudo antes de começar. Use «Alterar» para voltar a um passo; nada fica gravado até concluir.</p>
              <Revisao titulo="Tipo de gestão" onAlterar={() => irPara(0)}>
                <span className="flex items-center gap-2">
                  {(() => {
                    const M = modoPorId(modo);
                    return (
                      <>
                        <M.icone className="size-4 text-brand" /> {M.nome}
                      </>
                    );
                  })()}
                </span>
              </Revisao>
              <Revisao titulo={escola ? "Escola" : "Empresa"} onAlterar={() => irPara(1)}>
                <Linha r="Nome" v={e.nome} />
                <Linha r="NIF" v={e.nif} />
                <Linha r="Regime de IVA" v={REGIMES[e.regime_iva]} />
                <Linha r="Morada" v={[e.endereco, e.localidade, e.provincia].filter(Boolean).join(", ")} />
                <Linha r="Contactos" v={[e.telefone, e.email].filter(Boolean).join(" · ")} />
              </Revisao>
              <Revisao titulo={escola ? "Propinas e emolumentos" : "Actividade"} onAlterar={() => irPara(2)}>
                {escola ? (
                  <>
                    <div className="mb-1.5 text-[12.5px] text-ink-3">
                      Família «{FAMILIA_ENSINO}» · isentos de IVA ({MOTIVO_ENSINO}) · {escolhidas.length} {escolhidas.length === 1 ? "artigo" : "artigos"}
                    </div>
                    <ul className="divide-y divide-line">
                      {escolhidas.map((l) => (
                        <li key={l.chave} className="flex items-center justify-between gap-3 py-1">
                          <span className="min-w-0 truncate">
                            {l.descricao} <span className="font-mono text-[11.5px] text-ink-3">{l.referencia.toUpperCase()}</span>
                          </span>
                          {l.precoTexto.trim() && parseKz(l.precoTexto) > 0 ? <span className="num shrink-0">{kz(parseKz(l.precoTexto))}</span> : <Badge tone="accent">a definir</Badge>}
                        </li>
                      ))}
                    </ul>
                  </>
                ) : (
                  <>
                    <Linha r="Ramo" v={ramoAtual.nome} />
                    <Linha r="Famílias" v={familias.length ? familias.join(", ") : "nenhuma (cria depois)"} />
                  </>
                )}
              </Revisao>
              <Revisao titulo="Cores e moedas" onAlterar={() => irPara(3)}>
                <Linha r="Cores" v={cores.paleta === "personalizada" ? "Personalizadas" : (PALETAS.find((p) => p.id === cores.paleta)?.nome ?? cores.paleta)} />
                <Linha
                  r="Moedas no POS"
                  v={
                    moedasEscolhidas().length
                      ? `Kwanza e ${moedasEscolhidas()
                          .map((m) => `${MOEDAS_POS.find((x) => x.codigo === m.codigo)?.nome} (${m.cambio.toLocaleString("pt-AO")} Kz)`)
                          .join(", ")}`
                      : "Só kwanzas"
                  }
                />
              </Revisao>
              <Revisao titulo="Administrador" onAlterar={() => irPara(4)}>
                <Linha r="Nome" v={a.nome} />
                <Linha r="Utilizador" v={a.login} />
                <Linha r="Senha" v={"•".repeat(Math.min(a.senha.length, 12))} />
              </Revisao>
              <div className="rounded-xl border border-dashed border-line-strong p-4 text-[13px] text-ink-2">
                <div className="mb-1 font-medium text-ink">O KanGest prepara também</div>
                <ul className="space-y-0.5">
                  <li>• As séries de documentos do ano, à medida que emite cada tipo de documento.</li>
                  <li>• O armazém principal, as taxas de IVA, os motivos de isenção e os modos de pagamento.</li>
                  <li>• O período de experiência da licença, que começa hoje.</li>
                </ul>
              </div>
            </div>
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
  return <div className="mb-1 flex items-center gap-2 font-semibold">{icone} {children}</div>;
}

/** Bloco da revisão final, com o botão para voltar ao passo. */
function Revisao({ titulo, onAlterar, children }: { titulo: string; onAlterar: () => void; children: ReactNode }) {
  return (
    <Card className="p-4">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-[13px] font-semibold uppercase tracking-wide text-ink-3">{titulo}</span>
        <Button type="button" size="sm" variant="ghost" onClick={onAlterar}>
          <Pencil className="size-3.5" /> Alterar
        </Button>
      </div>
      <div className="text-[14px]">{children}</div>
    </Card>
  );
}

function Linha({ r, v }: { r: string; v?: string }) {
  return (
    <div className="flex gap-3 py-0.5">
      <span className="w-32 shrink-0 text-ink-3">{r}</span>
      <span className="min-w-0 break-words">{v || "—"}</span>
    </div>
  );
}

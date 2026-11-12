import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { KeyRound, LockOpen, Plus, ShieldCheck, Trash2, UserCog, UsersRound } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora, hojeISO } from "@/lib/format";
import { GRUPOS_PERMISSOES, TODAS_PERMISSOES } from "@/lib/permissoes";
import { useSession } from "@/store/session";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface Utilizador {
  id: number;
  nome: string;
  login: string;
  perfil: string;
  perfil_descricao: string | null;
  ativo: number;
  permissoes: string[];
  permissoes_proprias: boolean;
  limite_desconto: number;
  tem_pin: boolean;
  mudar_senha: boolean;
  ultimo_acesso: string | null;
  bloqueado_ate: string | null;
  importado: boolean;
}

interface Perfil {
  codigo: string;
  descricao: string;
  nivel: number;
  sistema: boolean;
  personalizado: boolean;
  permissoes: string[];
  utilizadores: number;
}

interface Politica {
  senha_min: number;
  tentativas: number;
  bloqueio_min: number;
  inatividade_min: number;
  login_pin: boolean;
}

const VISTAS = [
  { id: "utilizadores", label: "Utilizadores", perm: "sistema.utilizadores" },
  { id: "perfis", label: "Perfis e permissões", perm: "sistema.utilizadores" },
  { id: "acessos", label: "Registo de acessos", perm: "sistema.auditoria" },
  { id: "auditoria", label: "Auditoria", perm: "sistema.auditoria" },
  { id: "seguranca", label: "Segurança", perm: "sistema.utilizadores" },
] as const;
type Vista = (typeof VISTAS)[number]["id"];

/** O escalão decide o ecrã inicial e o acesso ao painel; o resto vem das permissões. */
const NIVEIS: Record<number, string> = { 1: "Começa no POS", 2: "Painel de gestão", 3: "Acesso total" };

/** Módulo de utilizadores, perfis de acesso, registo de acessos e política de segurança. */
export default function Utilizadores() {
  const tem = useSession((s) => s.tem);
  const [sp, setSp] = useSearchParams();
  const visiveis = VISTAS.filter((v) => tem(v.perm));
  const pedida = visiveis.find((v) => v.id === sp.get("vista"));
  const vista: Vista = pedida?.id ?? visiveis[0]?.id ?? "utilizadores";

  return (
    <>
      <PageHeader title="Utilizadores e Permissões" subtitle="Contas, perfis de acesso, segurança da autenticação e registo de acessos" />
      <div className="mb-5 flex gap-1 border-b border-line">
        {visiveis.map((v) => (
          <button
            key={v.id}
            onClick={() => setSp({ vista: v.id })}
            className={cn("-mb-px whitespace-nowrap border-b-2 px-4 py-2.5", vista === v.id ? "border-brand font-medium text-ink" : "border-transparent text-ink-2 hover:text-ink")}
          >
            {v.label}
          </button>
        ))}
      </div>
      {vista === "utilizadores" && <ListaUtilizadores />}
      {vista === "perfis" && <Perfis />}
      {vista === "acessos" && <Registo tipo="acessos" />}
      {vista === "auditoria" && <Registo tipo="auditoria" />}
      {vista === "seguranca" && <Seguranca />}
    </>
  );
}

// ------------------------------------------------------------------ matriz de permissões

function MatrizPermissoes({ valor, onChange, desativado }: { valor: string[]; onChange: (p: string[]) => void; desativado?: boolean }) {
  const alternar = (p: string) => onChange(valor.includes(p) ? valor.filter((x) => x !== p) : [...valor, p]);
  return (
    <div className={cn("grid grid-cols-3 gap-x-6 gap-y-5", desativado && "pointer-events-none opacity-50")}>
      {GRUPOS_PERMISSOES.map((g) => {
        const ids = g.itens.map((i) => i.id);
        const todas = ids.every((i) => valor.includes(i));
        return (
          <div key={g.grupo}>
            <label className="mb-1.5 flex items-center gap-2 text-[12px] font-medium uppercase tracking-wide text-ink-2">
              <input
                type="checkbox"
                checked={todas}
                ref={(el) => {
                  if (el) el.indeterminate = !todas && ids.some((i) => valor.includes(i));
                }}
                onChange={() => onChange(todas ? valor.filter((x) => !ids.includes(x)) : [...new Set([...valor, ...ids])])}
              />
              {g.grupo}
            </label>
            {g.itens.map((p) => (
              <label key={p.id} className="flex items-center gap-2 py-0.5 pl-1 text-[13px]">
                <input type="checkbox" checked={valor.includes(p.id)} onChange={() => alternar(p.id)} />
                {p.label}
              </label>
            ))}
          </div>
        );
      })}
    </div>
  );
}

// ------------------------------------------------------------------ utilizadores

type Edicao = Partial<Utilizador> & { senha?: string; pin?: string; personalizar?: boolean; obrigarMudar?: boolean };

function estado(u: Utilizador) {
  if (!u.ativo) return <Badge>Inactivo</Badge>;
  if (u.bloqueado_ate) return <Badge tone="danger">Bloqueado até {u.bloqueado_ate.slice(11, 16)}</Badge>;
  if (u.mudar_senha) return <Badge tone="accent">Senha provisória</Badge>;
  return <Badge tone="ok">Activo</Badge>;
}

function ListaUtilizadores() {
  const qc = useQueryClient();
  const lista = useQuery({ queryKey: ["utilizadores"], queryFn: () => call<Utilizador[]>("utilizadores.listar") });
  const perfis = useQuery({ queryKey: ["perfis"], queryFn: () => call<Perfil[]>("perfis.listar") });
  const politica = useQuery({ queryKey: ["seguranca"], queryFn: () => call<Politica>("seguranca.obter") });
  const [u, setU] = useState<Edicao | null>(null);
  const [senhaNova, setSenhaNova] = useState<{ login: string; senha: string } | null>(null);
  const atualizar = () => qc.invalidateQueries({ queryKey: ["utilizadores"] });

  async function guardar() {
    if (!u) return;
    try {
      await call("utilizadores.guardar", {
        id: u.id,
        nome: u.nome ?? "",
        login: u.login ?? "",
        perfil: u.perfil ?? "CAIXA",
        ativo: u.ativo !== 0,
        senha: u.senha || undefined,
        pin: u.pin || undefined,
        limite_desconto: u.limite_desconto ?? 0,
        permissoes: u.personalizar ? (u.permissoes ?? []) : null,
        mudar_senha: u.senha ? !!u.obrigarMudar : undefined,
      });
      toast.success("Utilizador guardado.");
      setU(null);
      atualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function redefinir(x: Utilizador) {
    try {
      const r = await call<{ login: string; senha: string }>("utilizadores.redefinirSenha", { id: x.id });
      setSenhaNova(r);
      atualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function desbloquear(x: Utilizador) {
    try {
      await call("utilizadores.desbloquear", { id: x.id });
      toast.success(`${x.nome} pode voltar a entrar.`);
      atualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const perfilEscolhido = perfis.data?.find((p) => p.codigo === u?.perfil);

  return (
    <Card>
      <div className="flex items-center justify-between p-4">
        <span className="text-[13px] text-ink-2">
          {lista.data?.filter((x) => x.ativo).length ?? 0} activos · {lista.data?.length ?? 0} no total
        </span>
        <Button onClick={() => setU({ perfil: "CAIXA", ativo: 1, permissoes: [], limite_desconto: 0, personalizar: false, obrigarMudar: true })}>
          <Plus className="size-4" /> Novo utilizador
        </Button>
      </div>
      {lista.data?.length === 0 ? (
        <Empty icon={<UsersRound className="size-6" />} title="Sem utilizadores" />
      ) : (
        <Table className="tabela-compacta">
          <thead>
            <tr>
              <Th>Nome</Th>
              <Th>Utilizador</Th>
              <Th>Perfil</Th>
              <Th>PIN</Th>
              <Th>Último acesso</Th>
              <Th>Estado</Th>
              <Th className="text-right">Acções</Th>
            </tr>
          </thead>
          <tbody>
            {lista.data?.map((x) => (
              <tr key={x.id} className={cn("hover:bg-sunken/60", !x.ativo && "opacity-60")}>
                <Td>
                  <button className="text-left font-medium hover:underline" onClick={() => setU({ ...x, personalizar: x.permissoes_proprias, obrigarMudar: true })}>
                    {x.nome}
                  </button>
                  {x.importado && <div className="text-[11.5px] text-ink-3">importado do KIGEST</div>}
                </Td>
                <Td className="font-mono text-[12.5px]">{x.login}</Td>
                <Td>
                  <Badge tone={x.perfil === "ADMIN" ? "brand" : "neutral"}>{x.perfil_descricao ?? x.perfil}</Badge>
                  {x.permissoes_proprias && <Badge className="ml-1">personalizado</Badge>}
                </Td>
                <Td>{x.tem_pin ? <Badge tone="ok">Definido</Badge> : <span className="text-ink-3">—</span>}</Td>
                <Td className="text-ink-2">{x.ultimo_acesso ? dataHora(x.ultimo_acesso) : "nunca entrou"}</Td>
                <Td>{estado(x)}</Td>
                <Td>
                  <div className="flex justify-end gap-1">
                    {x.bloqueado_ate && (
                      <Button size="sm" variant="secondary" onClick={() => desbloquear(x)}>
                        <LockOpen className="size-4" /> Desbloquear
                      </Button>
                    )}
                    <Button size="sm" variant="ghost" onClick={() => redefinir(x)} title="Gera uma senha provisória que o utilizador tem de mudar ao entrar">
                      <KeyRound className="size-4" /> Redefinir senha
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setU({ ...x, personalizar: x.permissoes_proprias, obrigarMudar: true })}>
                      <UserCog className="size-4" /> Editar
                    </Button>
                  </div>
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}

      {u && (
        <Modal open onClose={() => setU(null)} title={u.id ? `Editar ${u.nome}` : "Novo utilizador"} width="max-w-4xl" footer={<Button onClick={guardar}>Guardar</Button>}>
          <div className="grid grid-cols-3 gap-4">
            <Field label="Nome" className="col-span-2">
              <Input autoFocus value={u.nome ?? ""} onChange={(e) => setU({ ...u, nome: e.target.value })} />
            </Field>
            <Field label="Utilizador (para entrar)">
              <Input value={u.login ?? ""} onChange={(e) => setU({ ...u, login: e.target.value })} />
            </Field>
            <Field label="Perfil de acesso" hint={perfilEscolhido ? `${NIVEIS[perfilEscolhido.nivel]} · ${perfilEscolhido.permissoes.length} permissões` : undefined}>
              <Select value={u.perfil} onChange={(e) => setU({ ...u, perfil: e.target.value })}>
                {perfis.data?.map((p) => (
                  <option key={p.codigo} value={p.codigo}>
                    {p.descricao}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={u.id ? "Nova senha (opcional)" : "Senha"} hint={`Mínimo ${politica.data?.senha_min ?? 6} caracteres`}>
              <Input type="password" autoComplete="new-password" value={u.senha ?? ""} onChange={(e) => setU({ ...u, senha: e.target.value })} />
            </Field>
            <Field label={u.tem_pin ? "Novo PIN (opcional)" : "PIN"} hint="4 a 8 dígitos, único; «-» remove">
              <Input inputMode="numeric" autoComplete="off" value={u.pin ?? ""} onChange={(e) => setU({ ...u, pin: e.target.value.replace(/[^\d-]/g, "").slice(0, 8) })} />
            </Field>
            <Field label="Limite de desconto (%)" hint="0 = sem limite">
              <Input type="number" min={0} max={100} className="num text-right" value={u.limite_desconto ?? 0} onChange={(e) => setU({ ...u, limite_desconto: Number(e.target.value) })} />
            </Field>
            <div className="col-span-2 flex flex-wrap items-end gap-x-6 gap-y-2 pb-2">
              {(!u.id || u.senha) && (
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={!!u.obrigarMudar} onChange={(e) => setU({ ...u, obrigarMudar: e.target.checked })} /> Obrigar a mudar a senha ao entrar
                </label>
              )}
              {u.id && (
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={u.ativo !== 0} onChange={(e) => setU({ ...u, ativo: e.target.checked ? 1 : 0 })} /> Conta activa
                </label>
              )}
            </div>
          </div>

          <div className="mt-6 rounded-xl border border-line p-4">
            <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
              <label className="flex items-center gap-2 font-medium">
                <input
                  type="checkbox"
                  checked={!!u.personalizar}
                  disabled={u.perfil === "ADMIN"}
                  onChange={(e) => setU({ ...u, personalizar: e.target.checked, permissoes: e.target.checked ? (u.permissoes?.length ? u.permissoes : (perfilEscolhido?.permissoes ?? [])) : u.permissoes })}
                />
                Permissões próprias deste utilizador
              </label>
              <span className="text-[12.5px] text-ink-3">
                {u.perfil === "ADMIN" ? "O Administrador tem sempre todas as permissões." : u.personalizar ? "Substituem as do perfil." : `Usa as do perfil ${perfilEscolhido?.descricao ?? ""}.`}
              </span>
            </div>
            <MatrizPermissoes
              valor={u.perfil === "ADMIN" ? TODAS_PERMISSOES : u.personalizar ? (u.permissoes ?? []) : (perfilEscolhido?.permissoes ?? [])}
              onChange={(p) => setU({ ...u, permissoes: p })}
              desativado={!u.personalizar || u.perfil === "ADMIN"}
            />
          </div>
        </Modal>
      )}

      {senhaNova && (
        <Modal open onClose={() => setSenhaNova(null)} title="Senha provisória" width="max-w-sm" footer={<Button onClick={() => setSenhaNova(null)}>Feito</Button>}>
          <p className="text-ink-2">
            Entregue esta senha a <b className="text-ink">{senhaNova.login}</b>. Só é mostrada agora; o utilizador tem de a mudar ao entrar.
          </p>
          <div className="mt-4 select-all rounded-lg border border-line bg-sunken py-3 text-center font-mono text-[22px] tracking-wider">{senhaNova.senha}</div>
        </Modal>
      )}
    </Card>
  );
}

// ------------------------------------------------------------------ perfis

function Perfis() {
  const qc = useQueryClient();
  const perfis = useQuery({ queryKey: ["perfis"], queryFn: () => call<Perfil[]>("perfis.listar") });
  const [p, setP] = useState<Partial<Perfil> & { novo?: boolean } | null>(null);
  const atualizar = () => {
    qc.invalidateQueries({ queryKey: ["perfis"] });
    qc.invalidateQueries({ queryKey: ["utilizadores"] });
  };

  async function guardar(repor = false) {
    if (!p) return;
    try {
      await call("perfis.guardar", { codigo: p.novo ? null : p.codigo, descricao: p.descricao ?? "", nivel: p.nivel ?? 1, permissoes: repor ? null : (p.permissoes ?? []) });
      toast.success(repor ? "Permissões de origem repostas." : "Perfil guardado.");
      setP(null);
      atualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function eliminar(codigo: string) {
    try {
      await call("perfis.eliminar", { codigo });
      toast.success("Perfil eliminado.");
      setP(null);
      atualizar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const admin = p?.codigo === "ADMIN";
  return (
    <>
      <div className="mb-4 flex items-center justify-between">
        <p className="max-w-2xl text-[13px] text-ink-2">
          Cada utilizador tem um perfil. As permissões do perfil aplicam-se a todos os seus utilizadores, salvo aos que tenham permissões próprias.
        </p>
        <Button onClick={() => setP({ novo: true, nivel: 1, permissoes: [] })}>
          <Plus className="size-4" /> Novo perfil
        </Button>
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
        {perfis.data?.map((x) => (
          <Card key={x.codigo} className="flex flex-col p-5">
            <div className="flex items-start justify-between gap-2">
              <div>
                <div className="font-semibold">{x.descricao}</div>
                <div className="font-mono text-[11.5px] text-ink-3">{x.codigo}</div>
              </div>
              <Badge tone={x.nivel === 3 ? "brand" : "neutral"}>{NIVEIS[x.nivel]}</Badge>
            </div>
            <div className="mt-3 text-[13px] text-ink-2">
              {x.permissoes.length} de {TODAS_PERMISSOES.length} permissões · {x.utilizadores} {x.utilizadores === 1 ? "utilizador" : "utilizadores"}
            </div>
            <div className="mt-1 flex flex-wrap gap-1">
              {x.sistema && <Badge>de origem</Badge>}
              {x.sistema && x.personalizado && <Badge tone="accent">alterado</Badge>}
            </div>
            <div className="mt-auto pt-4">
              <Button size="sm" variant="secondary" onClick={() => setP({ ...x })}>
                <ShieldCheck className="size-4" /> {x.codigo === "ADMIN" ? "Ver permissões" : "Editar permissões"}
              </Button>
            </div>
          </Card>
        ))}
      </div>

      {p && (
        <Modal
          open
          onClose={() => setP(null)}
          title={p.novo ? "Novo perfil" : `Perfil ${p.descricao}`}
          width="max-w-4xl"
          footer={
            admin ? (
              <Button onClick={() => setP(null)}>Fechar</Button>
            ) : (
              <>
                {!p.novo && !p.sistema && (
                  <Button variant="ghost" className="mr-auto text-danger" onClick={() => p.codigo && eliminar(p.codigo)}>
                    <Trash2 className="size-4" /> Eliminar
                  </Button>
                )}
                {p.sistema && p.personalizado && (
                  <Button variant="secondary" className="mr-auto" onClick={() => guardar(true)}>
                    Repor as permissões de origem
                  </Button>
                )}
                <Button onClick={() => guardar()}>Guardar</Button>
              </>
            )
          }
        >
          <div className="mb-5 grid grid-cols-3 gap-4">
            <Field label="Nome do perfil" className="col-span-2">
              <Input autoFocus={p.novo} disabled={p.sistema} value={p.descricao ?? ""} placeholder="Ex.: Armazém, Contabilista, Supervisor de caixa" onChange={(e) => setP({ ...p, descricao: e.target.value })} />
            </Field>
            <Field label="Ecrã inicial" hint="O painel mostra vendas e indicadores da empresa">
              <Select disabled={p.sistema} value={p.nivel ?? 1} onChange={(e) => setP({ ...p, nivel: Number(e.target.value) })}>
                <option value={1}>Ponto de venda (POS)</option>
                <option value={2}>Painel de gestão</option>
                {p.nivel === 3 && <option value={3}>Painel (administrador)</option>}
              </Select>
            </Field>
          </div>
          {admin && <p className="mb-4 text-[13px] text-ink-2">O Administrador tem sempre todas as permissões, incluindo as dos módulos que vierem a ser acrescentados.</p>}
          <MatrizPermissoes valor={admin ? TODAS_PERMISSOES : (p.permissoes ?? [])} onChange={(v) => setP({ ...p, permissoes: v })} desativado={admin} />
        </Modal>
      )}
    </>
  );
}

// ------------------------------------------------------------------ registo de acessos e auditoria

const EVENTOS: Record<string, { label: string; tom: "ok" | "danger" | "accent" | "neutral" | "brand" }> = {
  LOGIN: { label: "Entrada", tom: "ok" },
  LOGOUT: { label: "Saída", tom: "neutral" },
  FALHA: { label: "Tentativa falhada", tom: "danger" },
  BLOQUEIO: { label: "Conta bloqueada", tom: "danger" },
  DESBLOQUEIO: { label: "Conta desbloqueada", tom: "accent" },
  SENHA: { label: "Senha alterada", tom: "brand" },
  SENHA_REDEFINIDA: { label: "Senha redefinida", tom: "accent" },
  SESSAO_BLOQUEADA: { label: "Sessão bloqueada", tom: "neutral" },
  SESSAO_RETOMADA: { label: "Sessão retomada", tom: "neutral" },
};

interface LinhaRegisto {
  id: number;
  data_hora: string;
  utilizador: string | null;
  evento?: string;
  acao?: string;
  detalhe: string | null;
  posto?: string | null;
}

function Registo({ tipo }: { tipo: "acessos" | "auditoria" }) {
  const hoje = hojeISO();
  const [de, setDe] = useState(() => new Date(Date.now() - 6 * 86400000 - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10));
  const [ate, setAte] = useState(hoje);
  const [quem, setQuem] = useState<number | "">("");
  const [evento, setEvento] = useState("");
  const podeVerUtilizadores = useSession((s) => s.tem)("sistema.utilizadores");
  const utilizadores = useQuery({ queryKey: ["utilizadores"], queryFn: () => call<{ id: number; nome: string }[]>("utilizadores.listar"), enabled: podeVerUtilizadores });
  const q = useQuery({
    queryKey: [tipo, de, ate, quem, evento],
    queryFn: () => call<LinhaRegisto[]>(`${tipo}.listar`, { de, ate, utilizador_id: quem || null, evento: evento || null }),
    placeholderData: (p) => p,
  });
  useEffect(() => setEvento(""), [tipo]);
  const acoes = tipo === "auditoria" ? [...new Set((q.data ?? []).map((l) => l.acao ?? ""))].filter(Boolean).sort() : [];
  const falhas = tipo === "acessos" ? (q.data ?? []).filter((l) => l.evento === "FALHA").length : 0;

  return (
    <Card className={cn("transition-opacity", q.isFetching && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-3 border-b border-line p-4">
        <Input type="date" className="w-40" value={de} onChange={(e) => setDe(e.target.value)} />
        <span className="text-ink-3">até</span>
        <Input type="date" className="w-40" value={ate} onChange={(e) => setAte(e.target.value)} />
        {podeVerUtilizadores && (
          <Select className="w-52" value={quem} onChange={(e) => setQuem(e.target.value ? Number(e.target.value) : "")}>
            <option value="">Todos os utilizadores</option>
            {utilizadores.data?.map((u) => (
              <option key={u.id} value={u.id}>
                {u.nome}
              </option>
            ))}
          </Select>
        )}
        <Select className="w-52" value={evento} onChange={(e) => setEvento(e.target.value)}>
          <option value="">{tipo === "acessos" ? "Todos os eventos" : "Todas as acções"}</option>
          {tipo === "acessos"
            ? Object.entries(EVENTOS).map(([k, v]) => (
                <option key={k} value={k}>
                  {v.label}
                </option>
              ))
            : acoes.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
        </Select>
        <span className="ml-auto text-[13px] text-ink-2">
          {q.data?.length ?? 0} registos
          {falhas > 0 && <span className="ml-2 font-medium text-danger">· {falhas} tentativas falhadas</span>}
        </span>
      </div>
      {q.data?.length === 0 ? (
        <Empty icon={<ShieldCheck className="size-6" />} title="Sem registos no período" />
      ) : (
        <Table className="tabela-compacta max-h-[calc(100vh-300px)]">
          <thead>
            <tr>
              <Th>Data e hora</Th>
              <Th>Utilizador</Th>
              <Th>{tipo === "acessos" ? "Evento" : "Acção"}</Th>
              {tipo === "acessos" && <Th>Posto</Th>}
              <Th>Detalhe</Th>
            </tr>
          </thead>
          <tbody>
            {q.data?.map((l) => {
              const ev = l.evento ? EVENTOS[l.evento] : null;
              return (
                <tr key={l.id}>
                  <Td className="whitespace-nowrap text-ink-2">{dataHora(l.data_hora.replace(" ", "T"))}</Td>
                  <Td className="font-medium">{l.utilizador || <span className="text-ink-3">desconhecido</span>}</Td>
                  <Td>{ev ? <Badge tone={ev.tom}>{ev.label}</Badge> : <Badge>{l.evento ?? l.acao}</Badge>}</Td>
                  {tipo === "acessos" && <Td className="text-ink-2">{l.posto ?? "—"}</Td>}
                  <Td className="text-ink-2">{l.detalhe ?? "—"}</Td>
                </tr>
              );
            })}
          </tbody>
        </Table>
      )}
    </Card>
  );
}

// ------------------------------------------------------------------ política de segurança

function Seguranca() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["seguranca"], queryFn: () => call<Politica>("seguranca.obter") });
  const [p, setP] = useState<Politica | null>(null);
  useEffect(() => {
    if (q.data) setP(q.data);
  }, [q.data]);
  if (!p) return null;

  async function guardar() {
    try {
      await call("seguranca.guardar", p);
      toast.success("Política de segurança guardada.");
      qc.invalidateQueries({ queryKey: ["seguranca"] });
      qc.invalidateQueries({ queryKey: ["politica"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const num = (k: keyof Politica, rotulo: string, hint: string, min: number, max: number) => (
    <Field label={rotulo} hint={hint}>
      <Input type="number" min={min} max={max} className="num text-right" value={Number(p[k])} onChange={(e) => setP({ ...p, [k]: Number(e.target.value) })} />
    </Field>
  );
  return (
    <Card className="max-w-3xl p-6">
      <div className="grid grid-cols-2 gap-5">
        {num("senha_min", "Comprimento mínimo da senha", "Aplica-se às senhas novas e às mudadas", 4, 32)}
        {num("tentativas", "Tentativas antes de bloquear a conta", "0 = nunca bloqueia", 0, 20)}
        {num("bloqueio_min", "Minutos de bloqueio da conta", "Ou até o administrador a desbloquear", 1, 1440)}
        {num("inatividade_min", "Bloquear a sessão sem actividade (min)", "0 = nunca; a sessão retoma-se com a senha ou o PIN", 0, 480)}
      </div>
      <label className="mt-5 flex items-start gap-3 rounded-lg border border-line p-3">
        <input type="checkbox" className="mt-1" checked={p.login_pin} onChange={(e) => setP({ ...p, login_pin: e.target.checked })} />
        <span>
          <span className="font-medium">Permitir entrar só com o PIN</span>
          <span className="block text-[12.5px] text-ink-3">
            Útil nos terminais POS para trocar de operador depressa. Cada PIN é único; após 5 PIN errados o posto espera 5 minutos.
          </span>
        </span>
      </label>
      <div className="mt-6 flex justify-end">
        <Button onClick={guardar}>Guardar</Button>
      </div>
    </Card>
  );
}

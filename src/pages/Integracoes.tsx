import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { AlertTriangle, Ban, BellRing, Copy, FileJson, KeyRound, Monitor, Network, Pencil, Plug, Plus, RefreshCw, Send, ShieldCheck, Trash2 } from "lucide-react";
import { call } from "@/lib/api";
import { dataHora } from "@/lib/format";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface PermissaoApi {
  id: string;
  descricao: string;
  grupo: string;
}

interface ChaveApi {
  id: number;
  nome: string;
  prefixo: string;
  utilizador_id: number;
  utilizador: string;
  utilizador_ativo: number;
  posto_id: number | null;
  posto: string | null;
  permissoes: string[];
  sem_efeito: string[];
  ips: string | null;
  limite_minuto: number;
  expira_em: string | null;
  ativa: number;
  revogada_em: string | null;
  criado_em: string;
  criado_por: string | null;
  ultimo_uso: string | null;
  ultimo_ip: string | null;
  pedidos: number;
  documentos: number;
  estado: "ACTIVA" | "INACTIVA" | "EXPIRADA" | "REVOGADA";
}

interface EstadoApi {
  ativo: boolean;
  porta: number;
  rede: boolean;
  /** Cloudflare Access (opcional): domínio da equipa e AUD da aplicação. */
  cf_equipa: string;
  cf_aud: string;
  a_servir: number | null;
  erro: string | null;
  /** A API faz parte do plano Empresa (e da experiência); nos outros planos, do módulo «api» da licença. */
  licenca: { incluida: boolean; experiencia: boolean; plano: string; mensagem: string | null };
  enderecos: string[];
  modo_rede: string;
  permissoes: PermissaoApi[];
  chaves: ChaveApi[];
  utilizadores: { id: number; nome: string; login: string; perfil: string; permissoes: string[] }[];
  postos: { id: number; descricao: string; armazem_id: number; serie_prefixo: string }[];
}

interface PedidoApi {
  id: number;
  data_hora: string;
  chave_id: number | null;
  chave: string | null;
  prefixo: string | null;
  ip: string;
  metodo: string;
  caminho: string;
  estado: number;
  duracao_ms: number;
  erro: string | null;
}

interface EdicaoChave {
  id?: number;
  nome: string;
  utilizador_id: number;
  posto_id: number | null;
  permissoes: string[];
  ips: string;
  limite_minuto: number;
  expira_em: string;
  ativa: boolean;
}

/** Modelos de permissões para os usos mais comuns (o administrador ajusta depois). */
const MODELOS: { label: string; permissoes: string[] }[] = [
  {
    label: "Loja online",
    permissoes: ["api.artigos", "api.stock", "api.entidades", "api.documentos", "clientes", "vendas.fatura_recibo", "vendas.fatura", "vendas.proforma"],
  },
  { label: "Contabilidade", permissoes: ["api.artigos", "api.entidades", "api.documentos", "relatorios.ver", "sistema.saft"] },
  { label: "Só consulta (BI)", permissoes: ["api.artigos", "api.stock", "api.entidades", "api.documentos", "relatorios.ver"] },
  { label: "Verificador de preços", permissoes: ["api.artigos"] },
];

const ESTADOS: Record<ChaveApi["estado"], { label: string; tom: "ok" | "neutral" | "danger" | "accent" }> = {
  ACTIVA: { label: "Activa", tom: "ok" },
  INACTIVA: { label: "Desactivada", tom: "neutral" },
  EXPIRADA: { label: "Expirada", tom: "accent" },
  REVOGADA: { label: "Revogada", tom: "danger" },
};

async function copiar(texto: string, o_que = "Copiado") {
  try {
    await navigator.clipboard.writeText(texto);
    toast.success(`${o_que}.`);
  } catch {
    toast.error("Não foi possível copiar. Seleccione o texto e copie à mão.");
  }
}

/** API de integração: servidor, chaves de acesso e registo de pedidos (Sistema → API e Integrações). */
export default function Integracoes() {
  const qc = useQueryClient();
  const estado = useQuery({ queryKey: ["api-estado"], queryFn: () => call<EstadoApi>("api.estado") });
  const [chave, setChave] = useState<EdicaoChave | null>(null);
  const [segredo, setSegredo] = useState<{ nome: string; chave: string } | null>(null);
  const [revogar, setRevogar] = useState<ChaveApi | null>(null);
  const atualizar = () => {
    qc.invalidateQueries({ queryKey: ["api-estado"] });
    qc.invalidateQueries({ queryKey: ["api-pedidos"] });
  };
  const e = estado.data;

  const base = useMemo(() => {
    if (!e) return "";
    const host = e.rede ? (e.enderecos[0] ?? "IP-DESTA-MÁQUINA") : "127.0.0.1";
    return `http://${host}:${e.a_servir ?? e.porta}/api/v1`;
  }, [e]);

  function nova() {
    const sugestao = e?.utilizadores.find((u) => u.perfil !== "ADMIN") ?? e?.utilizadores[0];
    setChave({ nome: "", utilizador_id: sugestao?.id ?? 0, posto_id: null, permissoes: MODELOS[0].permissoes, ips: "", limite_minuto: 120, expira_em: "", ativa: true });
  }

  function editar(c: ChaveApi) {
    setChave({
      id: c.id,
      nome: c.nome,
      utilizador_id: c.utilizador_id,
      posto_id: c.posto_id,
      permissoes: c.permissoes,
      ips: c.ips ?? "",
      limite_minuto: c.limite_minuto,
      expira_em: c.expira_em ?? "",
      ativa: c.ativa === 1,
    });
  }

  async function guardarChave() {
    if (!chave) return;
    const dados = { ...chave, ips: chave.ips || null, expira_em: chave.expira_em || null };
    try {
      if (chave.id) {
        await call("api.chaveGuardar", dados);
        toast.success("Chave guardada.");
      } else {
        const r = await call<{ chave: string }>("api.chaveCriar", dados);
        setSegredo({ nome: chave.nome, chave: r.chave });
      }
      setChave(null);
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }

  async function renovar(c: ChaveApi) {
    try {
      const r = await call<{ chave: string }>("api.chaveRenovar", { id: c.id });
      setSegredo({ nome: c.nome, chave: r.chave });
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }

  async function confirmarRevogar() {
    if (!revogar) return;
    try {
      await call("api.chaveRevogar", { id: revogar.id });
      toast.success(`A chave «${revogar.nome}» foi revogada.`);
      setRevogar(null);
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }

  return (
    <>
      <PageHeader title="API e Integrações" subtitle="Ligue a loja online, a contabilidade, o BI e outras aplicações ao KanGest com chaves de acesso" />
      {!e ? (
        <Card className="max-w-3xl">
          <div className="p-6 text-ink-3">{estado.error ? (estado.error as Error).message : "A carregar…"}</div>
        </Card>
      ) : (
        <div className="space-y-5">
          {!e.licenca.incluida && (
            <Card className="max-w-4xl border-accent/40 bg-accent-50/60">
              <div className="flex items-start gap-3 p-4">
                <AlertTriangle className="mt-0.5 size-5 shrink-0 text-[#9a6514]" />
                <div className="space-y-2">
                  <p className="font-medium">A API de integração não faz parte do plano {e.licenca.plano}</p>
                  <p className="text-[13px] text-ink-2">{e.licenca.mensagem}</p>
                  <p className="text-[13px] text-ink-2">Pode preparar as chaves; os pedidos são recusados até a licença incluir a API.</p>
                  <Link to="/licenca" className="inline-block text-[13px] font-medium text-brand hover:underline">
                    Licença e Assinatura →
                  </Link>
                </div>
              </div>
            </Card>
          )}
          {e.licenca.experiencia && (
            <p className="max-w-4xl text-[12.5px] text-ink-3">
              Durante a experiência a API está disponível. Depois faz parte do plano Empresa; nos planos Essencial e Comércio acrescenta-se com o módulo API.
            </p>
          )}
          <Servidor e={e} base={base} onGuardado={atualizar} />

          <Card>
            <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4">
              <div className="flex items-center gap-2">
                <KeyRound className="size-4 text-ink-3" />
                <span className="font-medium">Chaves de acesso</span>
                <span className="text-[12.5px] text-ink-3">Cada chave age em nome de um utilizador, com as permissões escolhidas para ela.</span>
              </div>
              <Button size="sm" onClick={nova}>
                <Plus className="size-4" /> Nova chave
              </Button>
            </div>
            {e.chaves.length ? (
              <Table>
                <thead>
                  <tr>
                    <Th>Nome</Th>
                    <Th>Chave</Th>
                    <Th>Age como</Th>
                    <Th>Posto</Th>
                    <Th>Permissões</Th>
                    <Th>Último uso</Th>
                    <Th className="text-right">Pedidos</Th>
                    <Th className="text-right">Docs.</Th>
                    <Th>Estado</Th>
                    <Th />
                  </tr>
                </thead>
                <tbody>
                  {e.chaves.map((c) => (
                    <tr key={c.id} className={cn(c.estado === "REVOGADA" && "opacity-60")}>
                      <Td className="font-medium">
                        {c.nome}
                        {c.ips && <div className="text-[11.5px] font-normal text-ink-3">IPs: {c.ips}</div>}
                      </Td>
                      <Td className="font-mono text-[12.5px]">{c.prefixo}…</Td>
                      <Td>
                        {c.utilizador}
                        {!c.utilizador_ativo && <div className="text-[11.5px] text-danger">utilizador inactivo</div>}
                      </Td>
                      <Td className="text-ink-2">{c.posto ?? "Posto#1"}</Td>
                      <Td>
                        <span title={c.permissoes.join("\n")}>{c.permissoes.length}</span>
                        {!!c.sem_efeito.length && c.estado !== "REVOGADA" && (
                          <div className="flex items-center gap-1 text-[11.5px] text-[#9a6514]" title={`O utilizador não tem: ${c.sem_efeito.join(", ")}`}>
                            <AlertTriangle className="size-3" /> {c.sem_efeito.length} sem efeito
                          </div>
                        )}
                      </Td>
                      <Td className="text-ink-2">
                        {c.ultimo_uso ? dataHora(c.ultimo_uso) : "—"}
                        {c.ultimo_ip && <div className="font-mono text-[11.5px] text-ink-3">{c.ultimo_ip}</div>}
                      </Td>
                      <Td className="num text-right">{c.pedidos}</Td>
                      <Td className="num text-right">{c.documentos}</Td>
                      <Td>
                        <Badge tone={ESTADOS[c.estado].tom}>{ESTADOS[c.estado].label}</Badge>
                        {c.expira_em && c.estado !== "REVOGADA" && <div className="text-[11.5px] text-ink-3">até {c.expira_em.split("-").reverse().join("/")}</div>}
                      </Td>
                      <Td className="whitespace-nowrap text-right">
                        {c.estado !== "REVOGADA" && (
                          <>
                            <Button size="sm" variant="ghost" onClick={() => editar(c)} title="Nome, utilizador, posto, permissões, IPs, limite e validade">
                              <Pencil className="size-4" />
                            </Button>
                            <Button size="sm" variant="ghost" onClick={() => renovar(c)} title="Gera um segredo novo; o antigo deixa logo de funcionar">
                              <RefreshCw className="size-4" />
                            </Button>
                            <Button size="sm" variant="ghost" className="text-danger" onClick={() => setRevogar(c)} title="Revogar de vez">
                              <Ban className="size-4" />
                            </Button>
                          </>
                        )}
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            ) : (
              <Empty
                icon={<KeyRound className="size-6" />}
                title="Ainda não há chaves"
                text="Crie uma chave para cada sistema que se liga ao KanGest (loja online, contabilidade…). Assim pode revogar uma sem afectar as outras."
                action={
                  <Button onClick={nova}>
                    <Plus className="size-4" /> Nova chave
                  </Button>
                }
              />
            )}
          </Card>

          <Avisos />
          <Pedidos chaves={e.chaves} />
          <ComoLigar base={base} />
        </div>
      )}

      {chave && e && <EditarChave e={e} c={chave} set={setChave} onGuardar={guardarChave} onFechar={() => setChave(null)} />}

      {segredo && (
        <Modal
          open
          onClose={() => setSegredo(null)}
          title={`Chave «${segredo.nome}»`}
          width="max-w-xl"
          footer={
            <>
              <Button variant="secondary" onClick={() => copiar(segredo.chave, "Chave copiada")}>
                <Copy className="size-4" /> Copiar
              </Button>
              <Button onClick={() => setSegredo(null)}>Já guardei a chave</Button>
            </>
          }
        >
          <p className="text-ink-2">
            Copie a chave e guarde-a no sistema que se vai ligar. <b className="text-ink">Só é mostrada agora:</b> o KanGest guarda apenas uma impressão dela. Se a perder, use «Renovar segredo».
          </p>
          <div className="mt-4 select-all break-all rounded-lg border border-line bg-sunken p-3 font-mono text-[14px]">{segredo.chave}</div>
          <p className="mt-3 text-[12.5px] text-ink-3">
            Envie-a em cada pedido no cabeçalho <span className="font-mono">Authorization: Bearer {segredo.chave.slice(0, 11)}…</span>
          </p>
        </Modal>
      )}

      {revogar && (
        <Modal
          open
          onClose={() => setRevogar(null)}
          title="Revogar a chave?"
          width="max-w-md"
          footer={
            <>
              <Button variant="ghost" onClick={() => setRevogar(null)}>
                Cancelar
              </Button>
              <Button variant="danger" onClick={confirmarRevogar}>
                <Ban className="size-4" /> Revogar
              </Button>
            </>
          }
        >
          <p className="text-ink-2">
            A chave <b className="text-ink">{revogar.nome}</b> ({revogar.prefixo}…) deixa de funcionar já e não pode ser reactivada. O sistema que a usa fica sem acesso até receber uma chave nova.
          </p>
        </Modal>
      )}
    </>
  );
}

// ---------------------------------------------------------------- servidor

function Servidor({ e, base, onGuardado }: { e: EstadoApi; base: string; onGuardado: () => void }) {
  const [f, setF] = useState<{ ativo: boolean; porta: number; rede: boolean; cf_equipa: string; cf_aud: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const v = f ?? { ativo: e.ativo, porta: e.porta, rede: e.rede, cf_equipa: e.cf_equipa, cf_aud: e.cf_aud };

  async function guardar() {
    setBusy(true);
    try {
      await call("api.guardar", { ativo: v.ativo, porta: Number(v.porta) || 7800, rede: v.rede, cf_equipa: v.cf_equipa, cf_aud: v.cf_aud });
      toast.success(v.ativo ? "API ligada." : "API desligada.");
      setF(null);
      onGuardado();
    } catch (x) {
      toast.error((x as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const MODOS = [
    { rede: false, label: "Só desta máquina", icon: Monitor, texto: "Para usar com um túnel ou proxy com HTTPS nesta máquina (recomendado para a Internet)." },
    { rede: true, label: "Rede local", icon: Network, texto: "Outros computadores da rede ligam-se a esta máquina pelo IP." },
  ];

  return (
    <Card className="max-w-4xl">
      <div className="flex items-center gap-2 border-b border-line p-4">
        <Plug className="size-4 text-ink-3" />
        <span className="font-medium">Servidor da API</span>
        {e.a_servir ? <Badge tone="ok">a servir na porta {e.a_servir}</Badge> : <Badge>desligada</Badge>}
      </div>
      <div className="space-y-4 p-4">
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={v.ativo} onChange={(x) => setF({ ...v, ativo: x.target.checked })} />
          <span className="font-medium">Ligar a API de integração</span>
        </label>
        <div className="grid grid-cols-[160px_1fr] gap-4">
          <Field label="Porta" hint="7800 por omissão">
            <Input className="num" value={v.porta} onChange={(x) => setF({ ...v, porta: Number(x.target.value.replace(/\D/g, "")) || 0 })} />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            {MODOS.map((m) => (
              <label key={m.label} className={cn("flex cursor-pointer gap-3 rounded-xl border p-3", v.rede === m.rede ? "border-brand bg-brand-50/50" : "border-line hover:bg-sunken/60")}>
                <input type="radio" className="mt-1" checked={v.rede === m.rede} onChange={() => setF({ ...v, rede: m.rede })} />
                <div>
                  <div className="flex items-center gap-2 font-medium">
                    <m.icon className="size-4" /> {m.label}
                  </div>
                  <div className="text-[12.5px] text-ink-3">{m.texto}</div>
                </div>
              </label>
            ))}
          </div>
        </div>
        {e.erro && <p className="rounded-lg bg-danger-50 p-3 text-[13px] text-danger">{e.erro}</p>}
        {e.a_servir && (
          <div className="flex flex-wrap items-center gap-2 text-[13px]">
            <span className="text-ink-2">Endereço da API:</span>
            <span className="select-all rounded bg-sunken px-2 py-1 font-mono">{base}</span>
            <Button size="sm" variant="ghost" onClick={() => copiar(base, "Endereço copiado")}>
              <Copy className="size-4" />
            </Button>
          </div>
        )}
        <p className="text-[12.5px] text-ink-3">
          A API fala HTTP sem cifra (os postos em rede, esses, vão cifrados): na rede local está protegida pelas chaves. Para a Internet, escolha «Só desta máquina» e publique-a através de um túnel com HTTPS
          (por exemplo, Cloudflare Tunnel). Não encaminhe esta porta no router.
        </p>
        <div className="rounded-xl border border-line p-4">
          <div className="mb-1 flex items-center gap-2 font-medium">
            <ShieldCheck className="size-4 text-ink-3" /> Cloudflare Access (opcional)
            {e.cf_equipa && <Badge tone="ok">ligado</Badge>}
          </div>
          <p className="mb-3 text-[12.5px] text-ink-3">
            Com a API publicada por um túnel da Cloudflare, os pedidos que chegam pela Cloudflare só passam com a autorização do Access (por exemplo, um token de serviço de cada
            integração), além da chave de API. Os pedidos da rede local continuam só com a chave.
          </p>
          <div className="grid grid-cols-2 gap-4">
            <Field label="Domínio da equipa" hint="Zero Trust → Definições → Domínio da equipa">
              <Input className="font-mono" placeholder="empresa.cloudflareaccess.com" value={v.cf_equipa} onChange={(x) => setF({ ...v, cf_equipa: x.target.value })} />
            </Field>
            <Field label="AUD da aplicação" hint="Application Audience (AUD) Tag da aplicação do Access">
              <Input className="font-mono" value={v.cf_aud} onChange={(x) => setF({ ...v, cf_aud: x.target.value })} />
            </Field>
          </div>
        </div>
        <div className="flex gap-2">
          <Button loading={busy} onClick={guardar}>
            Guardar
          </Button>
          {f && (
            <Button variant="ghost" onClick={() => setF(null)}>
              Cancelar
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------- criar / editar chave

function EditarChave({ e, c, set, onGuardar, onFechar }: { e: EstadoApi; c: EdicaoChave; set: (c: EdicaoChave) => void; onGuardar: () => void; onFechar: () => void }) {
  const grupos = useMemo(() => {
    const g: Record<string, PermissaoApi[]> = {};
    e.permissoes.forEach((p) => (g[p.grupo] ??= []).push(p));
    return Object.entries(g);
  }, [e.permissoes]);
  const utilizador = e.utilizadores.find((u) => u.id === c.utilizador_id);
  // permissões do programa que o utilizador não tem: a chave também não as terá
  const semEfeito = c.permissoes.filter((p) => !p.startsWith("api.") && utilizador && !utilizador.permissoes.includes(p));
  const alternar = (id: string) => set({ ...c, permissoes: c.permissoes.includes(id) ? c.permissoes.filter((x) => x !== id) : [...c.permissoes, id] });

  return (
    <Modal
      open
      onClose={onFechar}
      title={c.id ? `Chave «${c.nome}»` : "Nova chave de API"}
      width="max-w-3xl"
      footer={
        <>
          <Button variant="ghost" onClick={onFechar}>
            Cancelar
          </Button>
          <Button onClick={onGuardar}>{c.id ? "Guardar" : "Criar chave"}</Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Nome" hint="O sistema que se liga (ex.: Loja online, Contabilidade)">
          <Input autoFocus value={c.nome} onChange={(x) => set({ ...c, nome: x.target.value })} />
        </Field>
        <Field label="Age em nome de" hint="Os documentos ficam com este operador. Use um utilizador próprio para a integração.">
          <Select value={c.utilizador_id} onChange={(x) => set({ ...c, utilizador_id: Number(x.target.value) })}>
            {e.utilizadores.map((u) => (
              <option key={u.id} value={u.id}>
                {u.nome} ({u.login})
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Posto" hint="Série e armazém dos documentos emitidos pela chave">
          <Select value={c.posto_id ?? ""} onChange={(x) => set({ ...c, posto_id: x.target.value ? Number(x.target.value) : null })}>
            <option value="">Posto#1 (por omissão)</option>
            {e.postos.map((p) => (
              <option key={p.id} value={p.id}>
                {p.descricao}
                {p.serie_prefixo ? ` — série ${p.serie_prefixo}` : ""}
              </option>
            ))}
          </Select>
        </Field>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Pedidos por minuto" hint="0 = sem limite">
            <Input className="num" value={c.limite_minuto} onChange={(x) => set({ ...c, limite_minuto: Number(x.target.value.replace(/\D/g, "")) || 0 })} />
          </Field>
          <Field label="Válida até" hint="Vazio = sem validade">
            <Input type="date" value={c.expira_em} onChange={(x) => set({ ...c, expira_em: x.target.value })} />
          </Field>
        </div>
        <Field label="IPs permitidos" hint="Vazio = qualquer. Ex.: 192.168.1.20, 10.0.0.0/8" className="col-span-2">
          <Input className="font-mono" value={c.ips} onChange={(x) => set({ ...c, ips: x.target.value })} placeholder="qualquer endereço" />
        </Field>
      </div>

      <div className="mt-5">
        <div className="mb-2 flex flex-wrap items-center gap-2">
          <span className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Permissões</span>
          <span className="text-[12px] text-ink-3">Modelos:</span>
          {MODELOS.map((m) => (
            <Button key={m.label} size="sm" variant="secondary" onClick={() => set({ ...c, permissoes: m.permissoes })}>
              {m.label}
            </Button>
          ))}
        </div>
        <div className="grid grid-cols-2 gap-x-6 gap-y-4 rounded-xl border border-line p-4">
          {grupos.map(([grupo, itens]) => (
            <div key={grupo}>
              <div className="mb-1.5 text-[12.5px] font-medium text-ink-2">{grupo}</div>
              {itens.map((p) => (
                <label key={p.id} className="flex items-center gap-2 py-0.5 text-[13px]">
                  <input type="checkbox" checked={c.permissoes.includes(p.id)} onChange={() => alternar(p.id)} />
                  <span className={cn(semEfeito.includes(p.id) && "text-[#9a6514]")}>{p.descricao}</span>
                </label>
              ))}
            </div>
          ))}
        </div>
        {!!semEfeito.length && (
          <p className="mt-2 flex items-center gap-1.5 text-[12.5px] text-[#9a6514]">
            <AlertTriangle className="size-3.5" /> {utilizador?.nome} não tem {semEfeito.length === 1 ? "uma das permissões escolhidas" : `${semEfeito.length} das permissões escolhidas`}: a
            chave também não as terá.
          </p>
        )}
      </div>

      {c.id && (
        <label className="mt-4 flex items-center gap-2">
          <input type="checkbox" checked={c.ativa} onChange={(x) => set({ ...c, ativa: x.target.checked })} />
          <span>Chave activa (desmarque para suspender sem revogar)</span>
        </label>
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------- avisos automáticos (webhooks)

interface WebhookApi {
  id: number;
  nome: string;
  url: string;
  eventos: string[];
  segredo: string;
  ativo: boolean;
  ultimo_envio: string | null;
  ultimo_estado: number | null;
  falhas_seguidas: number;
  pendentes: number;
  falhadas: number;
}
interface EntregaApi {
  id: number;
  estado: "PENDENTE" | "ENTREGUE" | "FALHADA";
  tentativas: number;
  proxima_em: string | null;
  codigo_http: number | null;
  erro: string | null;
  entregue_em: string | null;
  webhook: string;
  tipo: string;
  objecto_id: number | null;
  criado_em: string;
}
interface WebhooksDados {
  webhooks: WebhookApi[];
  entregas: EntregaApi[];
  eventos: { id: string; descricao: string }[];
}
type EdicaoAviso = { id?: number; nome: string; url: string; eventos: string[]; ativo: boolean };

const ESTADO_ENTREGA: Record<EntregaApi["estado"], { label: string; tom: "ok" | "accent" | "danger" }> = {
  ENTREGUE: { label: "Entregue", tom: "ok" },
  PENDENTE: { label: "Por enviar", tom: "accent" },
  FALHADA: { label: "Falhada", tom: "danger" },
};

/** Avisos automáticos: o KanGest avisa outros sistemas (loja online, ERP) por POST assinado. */
function Avisos() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["api-webhooks"], queryFn: () => call<WebhooksDados>("api.webhooks"), refetchInterval: 15000 });
  const [ed, setEd] = useState<EdicaoAviso | null>(null);
  const [segredo, setSegredo] = useState<{ nome: string; segredo: string } | null>(null);
  const [apagar, setApagar] = useState<WebhookApi | null>(null);
  const atualizar = () => qc.invalidateQueries({ queryKey: ["api-webhooks"] });
  const d = q.data;

  async function guardar() {
    if (!ed) return;
    try {
      const r = await call<{ segredo: string | null }>("api.webhookGuardar", ed);
      if (r.segredo) setSegredo({ nome: ed.nome, segredo: r.segredo });
      else toast.success("Aviso guardado.");
      setEd(null);
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  async function testar(w: WebhookApi) {
    try {
      const r = await call<{ aceite: boolean; codigo_http: number | null; erro: string | null }>("api.webhookTestar", { id: w.id });
      if (r.aceite) toast.success(`${w.nome}: o sistema recebeu o teste (HTTP ${r.codigo_http}).`);
      else toast.error(`${w.nome}: ${r.erro ?? "sem resposta"}`, { duration: 8000 });
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  async function renovar(w: WebhookApi) {
    try {
      const r = await call<{ segredo: string }>("api.webhookSegredo", { id: w.id });
      setSegredo({ nome: w.nome, segredo: r.segredo });
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  async function confirmarApagar() {
    if (!apagar) return;
    try {
      await call("api.webhookApagar", { id: apagar.id });
      toast.success(`Aviso «${apagar.nome}» apagado.`);
      setApagar(null);
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  async function reenviar(e: EntregaApi) {
    try {
      await call("api.entregaReenviar", { id: e.id });
      toast.success("Volta a ser enviado dentro de momentos.");
      atualizar();
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  const nomeEvento = (id: string) => d?.eventos.find((x) => x.id === id)?.descricao ?? id;

  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4">
        <div className="flex items-center gap-2">
          <BellRing className="size-4 text-ink-3" />
          <span className="font-medium">Avisos automáticos (webhooks)</span>
          <span className="text-[12.5px] text-ink-3">O KanGest avisa a loja online ou outro sistema quando há vendas, anulações e mudanças de stock ou de preços.</span>
        </div>
        <Button size="sm" onClick={() => setEd({ nome: "", url: "https://", eventos: ["documento.emitido", "stock.alterado", "artigo.alterado"], ativo: true })}>
          <Plus className="size-4" /> Novo aviso
        </Button>
      </div>
      {d?.webhooks.length ? (
        <Table>
          <thead>
            <tr>
              <Th>Nome</Th>
              <Th>Eventos</Th>
              <Th>Último envio</Th>
              <Th className="text-right">Por enviar</Th>
              <Th className="text-right">Falhados</Th>
              <Th>Estado</Th>
              <Th />
            </tr>
          </thead>
          <tbody>
            {d.webhooks.map((w) => (
              <tr key={w.id}>
                <Td>
                  <div className="font-medium">{w.nome}</div>
                  <div className="max-w-[320px] truncate font-mono text-[11.5px] text-ink-3" title={w.url}>
                    {w.url}
                  </div>
                </Td>
                <Td className="text-[12.5px] text-ink-2" title={w.eventos.map(nomeEvento).join("\n")}>
                  {w.eventos.join(", ")}
                </Td>
                <Td className="text-ink-2">
                  {w.ultimo_envio ? dataHora(w.ultimo_envio) : "—"}
                  {w.ultimo_estado !== null && (
                    <Badge tone={w.ultimo_estado >= 200 && w.ultimo_estado < 300 ? "ok" : "danger"} className="ml-2">
                      {w.ultimo_estado || "sem ligação"}
                    </Badge>
                  )}
                </Td>
                <Td className="num text-right">{w.pendentes}</Td>
                <Td className={cn("num text-right", w.falhadas > 0 && "text-danger")}>{w.falhadas}</Td>
                <Td>{w.ativo ? <Badge tone="ok">Activo</Badge> : <Badge>Parado</Badge>}</Td>
                <Td className="whitespace-nowrap text-right">
                  <Button size="sm" variant="ghost" onClick={() => testar(w)} title="Enviar já um aviso de teste">
                    <Send className="size-4" />
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setEd({ id: w.id, nome: w.nome, url: w.url, eventos: w.eventos, ativo: w.ativo })} title="Editar">
                    <Pencil className="size-4" />
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setSegredo({ nome: w.nome, segredo: w.segredo })} title="Ver o segredo da assinatura">
                    <KeyRound className="size-4" />
                  </Button>
                  <Button size="sm" variant="ghost" className="text-danger" onClick={() => setApagar(w)} title="Apagar">
                    <Trash2 className="size-4" />
                  </Button>
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      ) : (
        <p className="p-6 text-ink-3">{q.isLoading ? "A carregar…" : "Sem avisos. Crie um para a loja online saber das vendas e do stock sem ter de perguntar."}</p>
      )}

      {!!d?.entregas.length && (
        <>
          <div className="border-t border-line px-4 pt-4 text-[13px] font-medium">Entregas recentes</div>
          <Table className="max-h-[320px]">
            <thead>
              <tr>
                <Th>Evento</Th>
                <Th>Aviso</Th>
                <Th>Estado</Th>
                <Th className="text-right">Tentativas</Th>
                <Th>Resposta</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {d.entregas.map((x) => (
                <tr key={x.id}>
                  <Td className="whitespace-nowrap">
                    <span className="font-mono text-[12px]">{x.tipo}</span>
                    {x.objecto_id !== null && <span className="text-ink-3"> #{x.objecto_id}</span>}
                    <div className="text-[11.5px] text-ink-3">{dataHora(x.criado_em)}</div>
                  </Td>
                  <Td>{x.webhook}</Td>
                  <Td>
                    <Badge tone={ESTADO_ENTREGA[x.estado].tom}>{ESTADO_ENTREGA[x.estado].label}</Badge>
                    {x.estado === "PENDENTE" && x.proxima_em && <div className="text-[11.5px] text-ink-3">nova tentativa {dataHora(x.proxima_em)}</div>}
                  </Td>
                  <Td className="num text-right">{x.tentativas}</Td>
                  <Td className="max-w-[300px] truncate text-[12.5px] text-ink-2" title={x.erro ?? ""}>
                    {x.erro ?? (x.codigo_http ? `HTTP ${x.codigo_http}` : "—")}
                  </Td>
                  <Td className="text-right">
                    {x.estado !== "ENTREGUE" && (
                      <Button size="sm" variant="ghost" onClick={() => reenviar(x)} title="Enviar de novo já">
                        <RefreshCw className="size-4" />
                      </Button>
                    )}
                  </Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </>
      )}

      {ed && d && (
        <Modal
          open
          onClose={() => setEd(null)}
          title={ed.id ? `Aviso «${ed.nome}»` : "Novo aviso automático"}
          width="max-w-2xl"
          footer={
            <>
              <Button variant="ghost" onClick={() => setEd(null)}>
                Cancelar
              </Button>
              <Button onClick={guardar}>{ed.id ? "Guardar" : "Criar aviso"}</Button>
            </>
          }
        >
          <div className="space-y-4">
            <Field label="Nome" hint="O sistema avisado (ex.: Loja online)">
              <Input autoFocus value={ed.nome} onChange={(x) => setEd({ ...ed, nome: x.target.value })} />
            </Field>
            <Field label="Endereço que recebe os avisos" hint="POST com JSON, assinado com o segredo (ver docs/API.md)">
              <Input className="font-mono" value={ed.url} onChange={(x) => setEd({ ...ed, url: x.target.value })} />
            </Field>
            <div>
              <div className="mb-1.5 text-[12px] font-medium uppercase tracking-wide text-ink-2">Eventos</div>
              {d.eventos.map((ev) => (
                <label key={ev.id} className="flex items-center gap-2 py-0.5 text-[13px]">
                  <input
                    type="checkbox"
                    checked={ed.eventos.includes(ev.id)}
                    onChange={() => setEd({ ...ed, eventos: ed.eventos.includes(ev.id) ? ed.eventos.filter((y) => y !== ev.id) : [...ed.eventos, ev.id] })}
                  />
                  <span>{ev.descricao}</span>
                  <span className="font-mono text-[11.5px] text-ink-3">{ev.id}</span>
                </label>
              ))}
            </div>
            {ed.id && (
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={ed.ativo} onChange={(x) => setEd({ ...ed, ativo: x.target.checked })} />
                <span>Activo (desmarque para parar os avisos sem apagar)</span>
              </label>
            )}
          </div>
        </Modal>
      )}

      {segredo && (
        <Modal
          open
          onClose={() => setSegredo(null)}
          title={`Segredo do aviso «${segredo.nome}»`}
          width="max-w-xl"
          footer={
            <>
              <Button variant="secondary" onClick={() => copiar(segredo.segredo, "Segredo copiado")}>
                <Copy className="size-4" /> Copiar
              </Button>
              {d?.webhooks.find((w) => w.nome === segredo.nome) && (
                <Button variant="ghost" onClick={() => { const w = d.webhooks.find((x) => x.nome === segredo.nome); if (w) renovar(w); }}>
                  Gerar outro
                </Button>
              )}
              <Button onClick={() => setSegredo(null)}>Fechar</Button>
            </>
          }
        >
          <p className="text-ink-2">
            O sistema avisado usa este segredo para confirmar que o aviso veio do KanGest: cada pedido traz <span className="font-mono">X-KanGest-Assinatura: t=…,v1=…</span>, o
            HMAC-SHA256 de <span className="font-mono">«t.corpo»</span> com este segredo.
          </p>
          <div className="mt-4 select-all break-all rounded-lg border border-line bg-sunken p-3 font-mono text-[13px]">{segredo.segredo}</div>
        </Modal>
      )}

      {apagar && (
        <Modal
          open
          onClose={() => setApagar(null)}
          title="Apagar o aviso?"
          width="max-w-md"
          footer={
            <>
              <Button variant="ghost" onClick={() => setApagar(null)}>
                Cancelar
              </Button>
              <Button variant="danger" onClick={confirmarApagar}>
                <Trash2 className="size-4" /> Apagar
              </Button>
            </>
          }
        >
          <p className="text-ink-2">
            O sistema <b className="text-ink">{apagar.nome}</b> deixa de ser avisado e as entregas por fazer perdem-se.
          </p>
        </Modal>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------- registo de pedidos

function Pedidos({ chaves }: { chaves: ChaveApi[] }) {
  const [chaveId, setChaveId] = useState<number | null>(null);
  const [erros, setErros] = useState(false);
  const pedidos = useQuery({
    queryKey: ["api-pedidos", chaveId, erros],
    queryFn: () => call<PedidoApi[]>("api.pedidos", { chave_id: chaveId, erros, limite: 300 }),
    refetchInterval: 15000,
  });
  const tom = (s: number) => (s < 300 ? "ok" : s === 429 || s < 500 ? "accent" : "danger");

  return (
    <Card>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line p-4">
        <span className="font-medium">Registo de pedidos</span>
        <div className="flex items-center gap-3">
          <Select className="h-8 w-56 text-[13px]" value={chaveId ?? ""} onChange={(x) => setChaveId(x.target.value ? Number(x.target.value) : null)}>
            <option value="">Todas as chaves</option>
            {chaves.map((c) => (
              <option key={c.id} value={c.id}>
                {c.nome} ({c.prefixo}…)
              </option>
            ))}
          </Select>
          <label className="flex items-center gap-1.5 text-[13px]">
            <input type="checkbox" checked={erros} onChange={(x) => setErros(x.target.checked)} /> Só erros
          </label>
          <Button size="sm" variant="ghost" onClick={() => pedidos.refetch()}>
            <RefreshCw className="size-4" />
          </Button>
        </div>
      </div>
      {pedidos.data?.length ? (
        <Table className="max-h-[420px]">
          <thead>
            <tr>
              <Th>Data e hora</Th>
              <Th>Chave</Th>
              <Th>IP</Th>
              <Th>Pedido</Th>
              <Th>Resposta</Th>
              <Th className="text-right">ms</Th>
              <Th>Erro</Th>
            </tr>
          </thead>
          <tbody>
            {pedidos.data.map((p) => (
              <tr key={p.id}>
                <Td className="whitespace-nowrap text-ink-2">{dataHora(p.data_hora)}</Td>
                <Td>{p.chave ?? <span className="text-ink-3">—</span>}</Td>
                <Td className="font-mono text-[12px]">{p.ip}</Td>
                <Td className="font-mono text-[12px]">
                  <b>{p.metodo}</b> {p.caminho}
                </Td>
                <Td>
                  <Badge tone={tom(p.estado)}>{p.estado}</Badge>
                </Td>
                <Td className="num text-right text-ink-2">{p.duracao_ms}</Td>
                <Td className="max-w-[360px] truncate text-[12.5px] text-ink-2" title={p.erro ?? ""}>
                  {p.erro ?? ""}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      ) : (
        <p className="p-6 text-ink-3">{pedidos.isLoading ? "A carregar…" : "Sem pedidos registados."}</p>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------- exemplos

function ComoLigar({ base }: { base: string }) {
  const exemplos = [
    { titulo: "Confirmar a chave", cmd: `curl -H "Authorization: Bearer kg_…" ${base}` },
    { titulo: "Artigos alterados desde uma data (sincronizar a loja online)", cmd: `curl -H "Authorization: Bearer kg_…" "${base}/artigos?alterado_desde=2026-11-01&limite=500"` },
    {
      titulo: "Emitir uma Factura/Recibo (a mesma Idempotency-Key nunca emite duas vezes)",
      cmd: `curl -X POST ${base}/documentos -H "Authorization: Bearer kg_…" -H "Content-Type: application/json" -H "Idempotency-Key: encomenda-1001" \\
  -d '{"tipo":"FR","referencia":"ENC-1001","linhas":[{"referencia":"ARZ5","quantidade":2}],"pagamentos":[{"modo":"TB","valor":228000}]}'`,
    },
    { titulo: "SAF-T de um mês (contabilidade)", cmd: `curl -H "Authorization: Bearer kg_…" -o saft.xml "${base}/saft?de=2026-10-01&ate=2026-10-31"` },
  ];
  return (
    <Card>
      <div className="flex items-center gap-2 border-b border-line p-4">
        <FileJson className="size-4 text-ink-3" />
        <span className="font-medium">Como ligar</span>
      </div>
      <div className="space-y-3 p-4 text-[13px]">
        <p className="text-ink-2">
          Valores em cêntimos de Kz (1 140,00 Kz = <span className="font-mono">114000</span>). As listas vêm por páginas: repita o pedido com{" "}
          <span className="font-mono">desde_id</span> igual ao <span className="font-mono">seguinte</span> da resposta até <span className="font-mono">mais</span> ser falso. A
          descrição completa (para o Postman ou o Swagger) está em <span className="select-all font-mono">{base}/openapi.json</span>.
        </p>
        {exemplos.map((x) => (
          <div key={x.titulo}>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-ink-2">{x.titulo}</span>
              <Button size="sm" variant="ghost" onClick={() => copiar(x.cmd, "Exemplo copiado")}>
                <Copy className="size-4" />
              </Button>
            </div>
            <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded-lg bg-sunken p-3 font-mono text-[12px]">{x.cmd}</pre>
          </div>
        ))}
      </div>
    </Card>
  );
}

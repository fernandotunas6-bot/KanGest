import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CloudUpload, FileKey2, ListChecks, Plus, RefreshCw, Send } from "lucide-react";
import { call } from "@/lib/api";
import { data, dataHora, kz } from "@/lib/format";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";

interface Estado {
  ativo: boolean;
  ambiente: "HML" | "PROD";
  desde: string;
  utilizador: string;
  senha: boolean;
  numero_validacao: string;
  produto: string;
  versao: string;
  estabelecimento: string;
  chave_software: boolean;
  chave_contribuinte: boolean;
  nif: string;
  endereco: string;
  contagem: { estado: string; n: number }[];
  series_agt: { tipo_codigo: string; serie: string; ativa: number; padrao: number }[];
}
interface Envio {
  documento_id: number;
  numero_doc: string;
  data_emissao: string;
  entidade_nome: string;
  total: number;
  estado: "PENDENTE" | "ENVIADO" | "VALIDO" | "INVALIDO";
  request_id: string | null;
  erros: string | null;
  tentativas: number;
  corrige: string | null;
  enviado_em: string | null;
  verificado_em: string | null;
}

const ESTADOS: Record<Envio["estado"], { label: string; tom: "neutral" | "brand" | "ok" | "danger" }> = {
  PENDENTE: { label: "Por enviar", tom: "neutral" },
  ENVIADO: { label: "Em validação", tom: "brand" },
  VALIDO: { label: "Válido", tom: "ok" },
  INVALIDO: { label: "Rejeitado", tom: "danger" },
};
const TIPOS = [
  ["FT", "Factura"],
  ["FR", "Factura/Recibo"],
  ["NC", "Nota de Crédito"],
  ["ND", "Nota de Débito"],
  ["AF", "Autofacturação"],
  ["RC", "Recibo"],
] as const;

const lerErros = (e: string | null): string[] => {
  if (!e) return [];
  try {
    const v = JSON.parse(e);
    return Array.isArray(v) ? v.map(String) : [String(v)];
  } catch {
    return [e];
  }
};

/** Facturação Electrónica da AGT (Decreto Presidencial 71/25): configuração, séries e comunicação dos documentos. */
export default function FacturacaoElectronica() {
  const qc = useQueryClient();
  const estado = useQuery({ queryKey: ["fe-estado"], queryFn: () => call<Estado>("fe.estado") });
  const [filtro, setFiltro] = useState("");
  const envios = useQuery({ queryKey: ["fe-envios", filtro], queryFn: () => call<Envio[]>("fe.envios", { estado: filtro || null }) });
  const [busy, setBusy] = useState(false);
  const [serie, setSerie] = useState(false);
  const [corrigir, setCorrigir] = useState<Envio | null>(null);
  const [seriesAgt, setSeriesAgt] = useState<Record<string, unknown>[] | null>(null);
  const e = estado.data;
  const n = (k: string) => e?.contagem.find((c) => c.estado === k)?.n ?? 0;

  async function enviar() {
    setBusy(true);
    try {
      const r = await call<{ enviados: number; validados: number; avisos: string[] }>("fe.processar");
      if (r.avisos?.length) toast.error(r.avisos[0], { duration: 10000 });
      else toast.success(`${r.enviados} documento(s) enviado(s), ${r.validados} validado(s).`);
      qc.invalidateQueries({ queryKey: ["fe-estado"] });
      qc.invalidateQueries({ queryKey: ["fe-envios"] });
    } catch (x) {
      toast.error((x as Error).message);
    } finally {
      setBusy(false);
    }
  }
  async function reenviar(env: Envio) {
    try {
      await call("fe.reenviar", { documento_id: env.documento_id });
      toast.success(`${env.numero_doc} volta a ser enviado no próximo ciclo.`);
      qc.invalidateQueries({ queryKey: ["fe-envios"] });
    } catch (x) {
      toast.error((x as Error).message);
    }
  }
  async function consultarSeries() {
    try {
      setSeriesAgt(await call<Record<string, unknown>[]>("fe.listarSeries"));
    } catch (x) {
      toast.error((x as Error).message);
    }
  }

  return (
    <>
      <PageHeader
        title="Facturação Electrónica"
        subtitle="Comunicação das facturas e recibos à AGT em tempo real (Decreto Presidencial 71/25)"
        actions={
          e?.ativo && (
            <Button loading={busy} onClick={enviar}>
              <Send className="size-4" /> Enviar e consultar agora
            </Button>
          )
        }
      />
      {e && (
        <div className="mb-4 grid grid-cols-4 gap-3">
          {(["PENDENTE", "ENVIADO", "VALIDO", "INVALIDO"] as const).map((k) => (
            <button
              key={k}
              onClick={() => setFiltro(filtro === k ? "" : k)}
              className={cn("rounded-xl border bg-surface p-4 text-left transition hover:border-brand/40", filtro === k ? "border-brand" : "border-line")}
            >
              <div className="text-[12px] uppercase tracking-wide text-ink-3">{ESTADOS[k].label}</div>
              <div className={cn("num text-[22px] font-semibold", k === "INVALIDO" && n(k) > 0 && "text-danger")}>{n(k)}</div>
            </button>
          ))}
        </div>
      )}
      <div className="grid grid-cols-[minmax(0,1fr)_380px] gap-4">
        <Card>
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <div className="font-medium">Documentos comunicados {filtro && `· ${ESTADOS[filtro as Envio["estado"]].label}`}</div>
            <Button variant="ghost" size="sm" onClick={() => envios.refetch()}>
              <RefreshCw className="size-4" />
            </Button>
          </div>
          {!envios.data?.length ? (
            <Empty
              icon={<CloudUpload className="size-6" />}
              title="Sem documentos"
              text={e?.ativo ? "As facturas e recibos emitidos entram aqui e são enviados à AGT automaticamente." : "Ligue a facturação electrónica no painel ao lado."}
            />
          ) : (
            <Table className="max-h-[calc(100vh-330px)]">
              <thead>
                <tr>
                  <Th>Documento</Th>
                  <Th>Cliente</Th>
                  <Th className="text-right">Total</Th>
                  <Th>Estado na AGT</Th>
                  <Th />
                </tr>
              </thead>
              <tbody>
                {envios.data.map((v) => (
                  <tr key={v.documento_id} className="align-top">
                    <Td>
                      <div className="font-mono text-[12.5px] font-medium">{v.numero_doc}</div>
                      <div className="text-[12px] text-ink-3">
                        {data(v.data_emissao)}
                        {v.corrige ? ` · corrige ${v.corrige}` : ""}
                      </div>
                    </Td>
                    <Td className="text-ink-2">{v.entidade_nome}</Td>
                    <Td className="num text-right">{kz(v.total)}</Td>
                    <Td>
                      <Badge tone={ESTADOS[v.estado].tom}>{ESTADOS[v.estado].label}</Badge>
                      {v.request_id && <div className="mt-0.5 font-mono text-[11px] text-ink-3">pedido {v.request_id}</div>}
                      {lerErros(v.erros).map((x, i) => (
                        <div key={i} className={cn("mt-0.5 max-w-[340px] text-[12px]", v.estado === "INVALIDO" ? "text-danger" : "text-ink-3")}>
                          {x}
                        </div>
                      ))}
                    </Td>
                    <Td className="text-right">
                      {v.estado === "INVALIDO" ? (
                        <Button variant="secondary" size="sm" onClick={() => setCorrigir(v)}>
                          Corrigir
                        </Button>
                      ) : v.estado === "PENDENTE" && v.tentativas > 0 ? (
                        <Button variant="ghost" size="sm" onClick={() => reenviar(v)}>
                          Reenviar
                        </Button>
                      ) : null}
                    </Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Card>

        <div className="space-y-4">
          {e && (
            // repõe o formulário só quando a configuração gravada muda (não a cada envio)
            <Configuracao
              key={[e.ativo, e.ambiente, e.utilizador, e.senha, e.numero_validacao, e.estabelecimento, e.chave_software, e.chave_contribuinte].join("|")}
              e={e}
            />
          )}
          {e && (
            <Card className="p-5">
              <div className="mb-3 flex items-center justify-between">
                <div className="flex items-center gap-2 font-medium">
                  <ListChecks className="size-4 text-brand" /> Séries da AGT
                </div>
                <div className="flex gap-1">
                  <Button variant="ghost" size="sm" disabled={!e.chave_contribuinte} onClick={consultarSeries}>
                    Consultar
                  </Button>
                  <Button variant="secondary" size="sm" disabled={!e.chave_contribuinte} onClick={() => setSerie(true)}>
                    <Plus className="size-4" /> Pedir série
                  </Button>
                </div>
              </div>
              {!e.series_agt.length ? (
                <p className="text-[13px] text-ink-3">Com a facturação electrónica ligada, cada tipo de documento comunicado só se emite numa série pedida à AGT.</p>
              ) : (
                <ul className="space-y-1 text-[13px]">
                  {e.series_agt.map((s) => (
                    <li key={`${s.tipo_codigo}${s.serie}`} className="flex items-center justify-between">
                      <span>
                        <b>{s.tipo_codigo}</b> <span className="font-mono">{s.serie}</span>
                      </span>
                      {s.padrao ? <Badge tone="brand">Em uso</Badge> : !s.ativa ? <Badge>Bloqueada</Badge> : null}
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          )}
        </div>
      </div>

      {serie && <PedirSerie onClose={() => setSerie(false)} />}
      {corrigir && <Corrigir env={corrigir} onClose={() => setCorrigir(null)} />}
      {seriesAgt && (
        <Modal open onClose={() => setSeriesAgt(null)} title="Séries registadas na AGT" width="max-w-2xl">
          {!seriesAgt.length ? (
            <p className="text-ink-3">A AGT não tem séries registadas para este contribuinte e estabelecimento.</p>
          ) : (
            <Table>
              <thead>
                <tr>
                  <Th>Série</Th>
                  <Th>Tipo</Th>
                  <Th>Ano</Th>
                  <Th>Estado</Th>
                  <Th>Últ. documento</Th>
                </tr>
              </thead>
              <tbody>
                {seriesAgt.map((s, i) => (
                  <tr key={i}>
                    <Td className="font-mono">{String(s.seriesCode ?? "")}</Td>
                    <Td>{String(s.documentType ?? "")}</Td>
                    <Td>{String(s.seriesYear ?? "")}</Td>
                    <Td>{({ A: "Aberta", U: "Em uso", F: "Fechada" } as Record<string, string>)[String(s.seriesStatus)] ?? String(s.seriesStatus ?? "")}</Td>
                    <Td className="font-mono text-[12px]">{String(s.lastDocumentCreated ?? "—")}</Td>
                  </tr>
                ))}
              </tbody>
            </Table>
          )}
        </Modal>
      )}
    </>
  );
}

function Configuracao({ e }: { e: Estado }) {
  const qc = useQueryClient();
  const [f, setF] = useState({ ...e, senha_nova: "", chave_software_pem: "", chave_contribuinte_pem: "" });
  const [busy, setBusy] = useState(false);
  const set = (p: Partial<typeof f>) => setF((x) => ({ ...x, ...p }));

  async function gravar(ativo: boolean) {
    setBusy(true);
    try {
      await call("fe.guardar", {
        ativo,
        ambiente: f.ambiente,
        utilizador: f.utilizador,
        senha: f.senha_nova || null,
        numero_validacao: f.numero_validacao,
        produto: f.produto,
        estabelecimento: f.estabelecimento,
        chave_software: f.chave_software_pem || null,
        chave_contribuinte: f.chave_contribuinte_pem || null,
      });
      toast.success(ativo ? "Facturação electrónica ligada." : "Configuração gravada.");
      qc.invalidateQueries({ queryKey: ["fe-estado"] });
    } catch (x) {
      toast.error((x as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const chave = (rotulo: string, presente: boolean, campo: "chave_software_pem" | "chave_contribuinte_pem", dica: string) => (
    <Field label={rotulo} hint={dica}>
      <label className="flex cursor-pointer items-center gap-2 rounded-lg border border-dashed border-line px-3 py-2 text-[13px] text-ink-2 hover:bg-sunken">
        <FileKey2 className="size-4" />
        {f[campo] ? "Chave escolhida (grave para aplicar)" : presente ? "Carregada · substituir…" : "Escolher ficheiro PEM…"}
        <input type="file" accept=".pem,.key,.txt" className="hidden" onChange={async (x) => x.target.files?.[0] && set({ [campo]: await x.target.files[0].text() })} />
      </label>
    </Field>
  );

  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center justify-between">
        <div className="font-medium">Ligação à AGT</div>
        {e.ativo ? <Badge tone="ok">Ligada{e.desde ? ` desde ${data(e.desde)}` : ""}</Badge> : <Badge>Desligada</Badge>}
      </div>
      <div className="space-y-3">
        <Field label="Ambiente">
          <Select value={f.ambiente} onChange={(x) => set({ ambiente: x.target.value as "HML" | "PROD" })}>
            <option value="HML">Homologação (testes)</option>
            <option value="PROD">Produção</option>
          </Select>
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Utilizador (AGT)">
            <Input value={f.utilizador} onChange={(x) => set({ utilizador: x.target.value })} />
          </Field>
          <Field label="Senha">
            <Input type="password" value={f.senha_nova} placeholder={e.senha ? "•••••• (manter)" : ""} onChange={(x) => set({ senha_nova: x.target.value })} />
          </Field>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label="N.º de validação do software">
            <Input value={f.numero_validacao} onChange={(x) => set({ numero_validacao: x.target.value })} placeholder="Ex.: C_134" />
          </Field>
          <Field label="Estabelecimento">
            <Input value={f.estabelecimento} onChange={(x) => set({ estabelecimento: x.target.value })} />
          </Field>
        </div>
        {chave("Chave do produtor do software", e.chave_software, "chave_software_pem", "Assina o softwareInfo (registada no Portal do Parceiro)")}
        {chave("Chave do contribuinte", e.chave_contribuinte, "chave_contribuinte_pem", "Emitida pela AGT no Portal do Contribuinte; assina os documentos")}
        <p className="text-[12px] text-ink-3">
          NIF emissor {e.nif || "—"} · {e.produto} {e.versao} · {e.endereco}
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          <Button variant="secondary" loading={busy} onClick={() => gravar(e.ativo)}>
            Gravar
          </Button>
          {e.ativo ? (
            <Button variant="ghost" loading={busy} onClick={() => gravar(false)}>
              Desligar
            </Button>
          ) : (
            <Button loading={busy} onClick={() => gravar(true)}>
              Ligar facturação electrónica
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}

function PedirSerie({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const [tipo, setTipo] = useState("FR");
  const [ano, setAno] = useState(new Date().getFullYear());
  const [contingencia, setContingencia] = useState(false);
  const [busy, setBusy] = useState(false);
  async function pedir() {
    setBusy(true);
    try {
      const r = await call<{ serie: string }>("fe.solicitarSerie", { tipo, ano, contingencia });
      toast.success(`A AGT atribuiu a série ${r.serie}; passa a ser a série em uso para ${tipo}.`);
      qc.invalidateQueries({ queryKey: ["fe-estado"] });
      onClose();
    } catch (x) {
      toast.error((x as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title="Pedir série à AGT"
      width="max-w-md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={pedir}>
            Pedir série
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Documento">
          <Select value={tipo} onChange={(x) => setTipo(x.target.value)}>
            {TIPOS.map(([c, l]) => (
              <option key={c} value={c}>
                {c} · {l}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Ano" hint="Depois de 15/12 pode pedir para o ano seguinte">
          <Input className="num" type="number" value={ano} onChange={(x) => setAno(Number(x.target.value))} />
        </Field>
        <label className="col-span-2 flex items-center gap-2 text-ink-2">
          <input type="checkbox" checked={contingencia} onChange={(x) => setContingencia(x.target.checked)} /> Série de contingência
        </label>
      </div>
    </Modal>
  );
}

function Corrigir({ env, onClose }: { env: Envio; onClose: () => void }) {
  const qc = useQueryClient();
  const [numero, setNumero] = useState("");
  const [busy, setBusy] = useState(false);
  async function gravar() {
    setBusy(true);
    try {
      await call("fe.corrigir", { rejeitado_id: env.documento_id, numero_novo: numero });
      toast.success("O documento novo será enviado como correcção do rejeitado.");
      qc.invalidateQueries({ queryKey: ["fe-envios"] });
      onClose();
    } catch (x) {
      toast.error((x as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title={`Corrigir ${env.numero_doc}`}
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} disabled={!numero.trim()} onClick={gravar}>
            Associar correcção
          </Button>
        </>
      }
    >
      <div className="mb-3 space-y-1 text-[13px]">
        {lerErros(env.erros).map((x, i) => (
          <div key={i} className="text-danger">
            {x}
          </div>
        ))}
      </div>
      <p className="mb-4 text-ink-2">
        A AGT não aceita o mesmo número duas vezes. Emita um documento novo com os dados certos e indique aqui o seu número: é enviado como correcção deste (estado «C»).
        Enviado em {env.enviado_em ? dataHora(env.enviado_em) : "—"}.
      </p>
      <Field label="N.º do documento de correcção">
        <Input autoFocus className="font-mono" value={numero} onChange={(x) => setNumero(x.target.value)} placeholder="Ex.: FT FT2026AGT1N/15" />
      </Field>
    </Modal>
  );
}

// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { FileSpreadsheet, HandCoins, Plus, Printer, Search, Trash2, Users } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { kz, nifValido, parseKz, toInputKz } from "@/lib/format";
import { exportarExcel, kzXL } from "@/lib/csv";
import { imprimir } from "@/lib/print";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import type { Entidade } from "@/lib/types";
import { AbasFicha } from "@/components/Auditoria";
import { InputData } from "@/components/InputData";
import { Atalho, Badge, BotaoCancelar, Button, Card, Confirmar, Empty, Field, Input, Modal, PageHeader, Segmentado, Select, Table, Td, Th, cn, useOrdenacao } from "@/components/ui";
import { useJaUsado, useSujo } from "@/lib/ficha";

export default function Entidades({ tipo }: { tipo: "C" | "F" }) {
  const [q, setQ] = useState("");
  const [editar, setEditar] = useState<Partial<Entidade> | null>(null);
  // cada ficha nova é outra (o «Guardar e novo» abre uma limpa)
  const [vez, setVez] = useState(0);
  const [estado, setEstado] = useState<"activos" | "inactivos" | "todos">("activos");
  const [sel, setSel] = useState<Set<number>>(new Set());
  const qc = useQueryClient();
  const dq = useDebounce(q);
  const nav = useNavigate();
  const [sp, setSp] = useSearchParams();
  // ?id=… (pesquisa global, Ctrl+K): abre logo a ficha
  useEffect(() => {
    const id = Number(sp.get("id"));
    if (!id) return;
    setSp({}, { replace: true });
    if (id !== 1) call<Entidade>("entidades.obter", { id }).then(setEditar, (e: Error) => toast.error(e.message));
  }, [sp, setSp]);
  const nome = tipo === "C" ? "Clientes" : "Fornecedores";
  const lista = useQuery({
    queryKey: ["entidades", tipo, dq, estado !== "activos"],
    queryFn: () => call<Entidade[]>("entidades.listar", { tipo, q: dq, inativos: estado !== "activos" }),
  });
  const visiveis = estado === "inactivos" ? lista.data?.filter((e) => e.ativo === 0) : lista.data;
  const seleccionados = (visiveis ?? []).filter((e) => sel.has(e.id));
  const marcaveis = (visiveis ?? []).filter((e) => e.id !== 1);
  const todosMarcados = marcaveis.length > 0 && marcaveis.every((e) => sel.has(e.id));
  function marcar(id: number) {
    setSel((x) => {
      const n = new Set(x);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }
  function nova() {
    setVez((v) => v + 1);
    setEditar({ tipo, nivel_preco: 1 });
  }
  async function emLote(accao: "activar" | "desactivar") {
    try {
      const r = await call<{ alterados: number }>("entidades.lote", { ids: seleccionados.map((e) => e.id), accao });
      toast.success(`${r.alterados} ficha(s) ${accao === "activar" ? "reactivada(s)" : "desactivada(s)"}.`);
      setSel(new Set());
      qc.invalidateQueries({ queryKey: ["entidades"] });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  const { ordenados, cab } = useOrdenacao(visiveis, (e, k: "nome" | "nif" | "localidade" | "saldo") =>
    k === "nome" ? e.nome : k === "nif" ? e.nif : k === "localidade" ? e.localidade : (e.saldo ?? 0),
  );
  const empresa = useEmpresa();
  const linhas = visiveis ?? [];
  const totalDivida = linhas.reduce((s, e) => s + Math.max(0, e.saldo ?? 0), 0);
  function exportar() {
    exportarExcel(
      `${nome.toLowerCase()}.xlsx`,
      [["Nome", "NIF", "Telefone", "Email", "Endereço", "Localidade", "Província", "Saldo em dívida"], ...(ordenados ?? linhas).map((e) => [e.nome, e.nif, e.telefone ?? "", e.email ?? "", e.endereco ?? "", e.localidade ?? "", e.provincia ?? "", kzXL(e.saldo ?? 0)])],
      { titulo: nome },
    );
  }

  return (
    <>
      <PageHeader
        title={nome}
        subtitle={tipo === "C" ? "Fichas de clientes e conta corrente" : "Fichas de fornecedores"}
        actions={
          <>
            <Button variant="secondary" onClick={exportar} disabled={!linhas.length}>
              <FileSpreadsheet className="size-4" /> Excel
            </Button>
            <Button variant="secondary" onClick={imprimir} disabled={!linhas.length}>
              <Printer className="size-4" /> Imprimir
            </Button>
            {tipo === "C" && (
              <Button variant="secondary" onClick={() => nav("/recibos/novo")}>
                <HandCoins className="size-4" /> Emitir recibo
              </Button>
            )}
            <Button onClick={nova}>
              <Plus className="size-4" /> Novo {tipo === "C" ? "cliente" : "fornecedor"}
            </Button>
          </>
        }
      />
      <Card>
        <div className="flex flex-wrap gap-3 border-b border-line p-4">
          <div className="relative min-w-60 flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Pesquisar por nome, NIF, telefone ou email…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <Segmentado
            className="w-full sm:w-72"
            valor={estado}
            onChange={(v) => {
              setEstado(v);
              setSel(new Set());
            }}
            opcoes={[
              { id: "activos", label: "Activos" },
              { id: "inactivos", label: "Inactivos" },
              { id: "todos", label: "Todos" },
            ]}
          />
        </div>
        {seleccionados.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 border-b border-line bg-brand-50/60 px-4 py-2.5 text-[13px]">
            <span className="mr-2 font-medium text-brand-700">{seleccionados.length} seleccionado(s)</span>
            <Button size="sm" variant="secondary" onClick={() => emLote("activar")}>
              Reactivar
            </Button>
            <Button size="sm" variant="secondary" onClick={() => emLote("desactivar")}>
              Desactivar
            </Button>
            <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setSel(new Set())}>
              Limpar a selecção
            </Button>
          </div>
        )}
        {visiveis?.length === 0 ? (
          <Empty icon={<Users className="size-6" />} title={estado === "inactivos" ? `Sem ${nome.toLowerCase()} desactivados` : `Sem ${nome.toLowerCase()}`} />
        ) : (
          <Table className="max-h-[calc(100vh-304px)]">
            <thead>
              <tr>
                <Th className="w-8">
                  <input type="checkbox" aria-label="Seleccionar todos" checked={todosMarcados} onChange={() => setSel(todosMarcados ? new Set() : new Set(marcaveis.map((e) => e.id)))} />
                </Th>
                <Th {...cab("nome")}>Nome</Th>
                <Th {...cab("nif")}>NIF</Th>
                <Th>Contacto</Th>
                <Th {...cab("localidade")}>Localidade</Th>
                <Th className="text-right" {...cab("saldo")}>
                  Saldo em dívida
                </Th>
              </tr>
            </thead>
            <tbody>
              {ordenados?.map((e) => (
                <tr key={e.id} className={cn("cursor-pointer hover:bg-sunken/60", e.ativo === 0 && "opacity-60")} onClick={() => e.id !== 1 && setEditar(e)}>
                  <Td onClick={(x) => x.stopPropagation()}>
                    {e.id !== 1 && <input type="checkbox" aria-label={`Seleccionar ${e.nome}`} checked={sel.has(e.id)} onChange={() => marcar(e.id)} />}
                  </Td>
                  <Td className="font-medium">
                    {e.nome} {e.id === 1 && <Badge className="ml-2">Sistema</Badge>}
                    {e.ativo === 0 && <Badge className="ml-2">Inactivo</Badge>}
                  </Td>
                  <Td className="font-mono text-[12.5px]">{e.nif}</Td>
                  <Td className="text-ink-2">{[e.telefone, e.email].filter(Boolean).join(" · ") || "—"}</Td>
                  <Td className="text-ink-2">{e.localidade ?? "—"}</Td>
                  <Td className="num text-right">{(e.saldo ?? 0) > 0 ? <span className="font-medium text-accent-ink">{kz(e.saldo)}</span> : <span className="text-ink-3">—</span>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {editar && <EntidadeForm key={`${editar.id ?? "nova"}-${vez}`} inicial={editar} onClose={() => setEditar(null)} onNovo={nova} />}
      {!editar && (
      <PrintPortal>
        <div className="p-2 text-[11px] text-black">
          <CabecalhoEmpresa empresa={empresa} titulo={`Lista de ${nome}`} />
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th className="py-1">Nome</th>
                <th className="py-1">NIF</th>
                <th className="py-1">Contacto</th>
                <th className="py-1">Localidade</th>
                <th className="py-1 text-right">Saldo em dívida</th>
              </tr>
            </thead>
            <tbody>
              {linhas.map((e) => (
                <tr key={e.id} className="border-b border-dotted border-gray-300">
                  <td className="py-0.5">{e.nome}</td>
                  <td className="py-0.5">{e.nif}</td>
                  <td className="py-0.5">{[e.telefone, e.email].filter(Boolean).join(" · ")}</td>
                  <td className="py-0.5">{e.localidade ?? ""}</td>
                  <td className="py-0.5 text-right">{(e.saldo ?? 0) > 0 ? kz(e.saldo) : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="mt-2 flex justify-between font-bold">
            <span>{linhas.length} {tipo === "C" ? "clientes" : "fornecedores"}</span>
            {totalDivida > 0 && <span>Total em dívida: {kz(totalDivida)}</span>}
          </div>
        </div>
      </PrintPortal>
      )}
    </>
  );
}

function EntidadeForm({ inicial, onClose, onNovo }: { inicial: Partial<Entidade>; onClose: () => void; onNovo?: () => void }) {
  const [e, setE] = useState(inicial);
  const [limite, setLimite] = useState(toInputKz(inicial.limite_credito ?? 0));
  const [busy, setBusy] = useState(false);
  const [erroNome, setErroNome] = useState<string | null>(null);
  const [eliminar, setEliminar] = useState(false);
  // cada carregar em «Imprimir ficha» imprime outra vez (a ficha impressa espera pelos documentos em dívida)
  const [imprimirFicha, setImprimirFicha] = useState(0);
  const qc = useQueryClient();
  const set = (k: keyof Entidade, v: unknown) => setE((x) => ({ ...x, [k]: v }));
  const sujo = useSujo({ e, limite });
  // NIF que já está noutra ficha activa (avisa enquanto se escreve)
  const nifOutro = useJaUsado<{ id: number; nome: string }>("entidades.nifUsado", "nif", e.nif, e.id, inicial.nif);
  const nifOk = nifValido(e.nif, e.pais ?? "AO");
  // retenções na fonte (para a retenção habitual)
  const retencoes = useQuery({ queryKey: ["doc-auxiliares"], queryFn: () => call<{ retencoes: { codigo: string; descricao: string; percentagem: number }[] }>("documentos.auxiliares"), staleTime: 300_000 }).data?.retencoes ?? [];
  const vendedores = useQuery({ queryKey: ["vendedores"], queryFn: () => call<{ id: number; nome: string }[]>("rh.vendedores"), staleTime: 300_000 }).data ?? [];

  async function guardar(depois: "fechar" | "novo" = "fechar") {
    if (busy) return;
    if (!e.nome?.trim()) {
      setErroNome("Escreva o nome ou a razão social.");
      document.querySelector<HTMLInputElement>("[role=dialog] input[data-campo=nome]")?.focus();
      return;
    }
    if (nifOutro) {
      toast.error(`O NIF ${e.nif} já está na ficha «${nifOutro.nome}».`);
      return;
    }
    setBusy(true);
    try {
      await call("entidades.guardar", { ...e, limite_credito: parseKz(limite) });
      toast.success("Guardado.");
      qc.invalidateQueries({ queryKey: ["entidades"] });
      qc.invalidateQueries({ queryKey: ["auditoria-historico", "entidades"] });
      if (depois === "novo" && onNovo) onNovo();
      else onClose();
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open
      onClose={onClose}
      title={e.id ? e.nome ?? "Editar" : e.tipo === "F" ? "Novo fornecedor" : "Novo cliente"}
      sujo={sujo}
      onGuardar={() => guardar()}
      footer={
        <>
          {e.id ? (
            <span className="mr-auto flex gap-1">
              <Button variant="ghost" className="text-danger hover:bg-danger-50 hover:text-danger" onClick={() => setEliminar(true)}>
                <Trash2 className="size-4" /> Eliminar
              </Button>
              <Button
                variant="ghost"
                onClick={() => setImprimirFicha((v) => v + 1)}
              >
                <Printer className="size-4" /> Imprimir ficha
              </Button>
            </span>
          ) : null}
          <BotaoCancelar />
          {!e.id && onNovo && (
            <Button variant="secondary" loading={busy} onClick={() => guardar("novo")}>
              Guardar e novo
            </Button>
          )}
          <Button loading={busy} onClick={() => guardar()}>
            Guardar <Atalho tecla="S" />
          </Button>
        </>
      }
    >
      {eliminar && (
        <Confirmar
          titulo={`Eliminar «${e.nome ?? ""}»?`}
          confirmar="Eliminar"
          onClose={() => setEliminar(false)}
          onConfirmar={async () => {
            try {
              const r = await call<{ eliminado: boolean; documentos?: number }>("entidades.eliminar", { id: e.id });
              if (r.eliminado) toast.success("Ficha eliminada.");
              else
                toast.success("A ficha ficou desactivada", {
                  description: `${r.documentos ? `Tem ${r.documentos} documento(s)` : "Tem movimentos ligados"}: sai das listas e da escolha nos documentos. Pode reactivá-la em «Inactivos».`,
                  duration: 9000,
                });
              qc.invalidateQueries({ queryKey: ["entidades"] });
              onClose();
            } catch (err) {
              toast.error((err as Error).message);
            }
          }}
        >
          <p>Se a ficha nunca foi usada, é apagada.</p>
          <p>
            Se já tem documentos, avenças, viaturas ou pontos, fica só <strong className="text-ink">desactivada</strong>: sai das listas e da escolha nos documentos, os
            documentos antigos continuam com ela e pode reactivá-la em «Inactivos».
          </p>
          <p className="text-[13px] text-ink-3">Fica registado na auditoria.</p>
        </Confirmar>
      )}
      <AbasFicha entidade="entidades" id={e.id}>
        <div className="grid grid-cols-2 gap-4">
          <Field label="Nome / Razão social" className="col-span-2" erro={erroNome}>
            <Input
              autoFocus
              data-campo="nome"
              value={e.nome ?? ""}
              onChange={(x) => {
                set("nome", x.target.value);
                if (x.target.value.trim()) setErroNome(null);
              }}
            />
          </Field>
          <Field
            label="NIF"
            hint="Vazio = 999999999 (consumidor final)"
            erro={!nifOk ? "NIF inválido. Em Angola: 10 algarismos (empresas) ou o n.º do BI (ex.: 001234567LA041)." : nifOutro ? `Este NIF já está na ficha «${nifOutro.nome}».` : null}
          >
            <Input value={e.nif ?? ""} onChange={(x) => set("nif", x.target.value)} />
          </Field>
          <Field label="Tipo">
            <Select value={e.tipo} onChange={(x) => set("tipo", x.target.value)}>
              <option value="C">Cliente</option>
              <option value="F">Fornecedor</option>
              <option value="A">Cliente e fornecedor</option>
            </Select>
          </Field>
          <Field label="Endereço" className="col-span-2">
            <Input value={e.endereco ?? ""} onChange={(x) => set("endereco", x.target.value)} />
          </Field>
          <Field label="Localidade">
            <Input value={e.localidade ?? ""} onChange={(x) => set("localidade", x.target.value)} />
          </Field>
          <Field label="Província">
            <Input value={e.provincia ?? ""} onChange={(x) => set("provincia", x.target.value)} />
          </Field>
          <Field label="País" hint="Código de 2 letras (SAF-T)">
            <Input
              list="paises"
              className="uppercase"
              maxLength={2}
              value={e.pais ?? "AO"}
              onChange={(x) => set("pais", x.target.value.toUpperCase())}
            />
            <datalist id="paises">
              {[["AO", "Angola"], ["PT", "Portugal"], ["ZA", "África do Sul"], ["NA", "Namíbia"], ["CD", "RD Congo"], ["CN", "China"], ["BR", "Brasil"], ["AE", "Emirados Árabes"], ["US", "Estados Unidos"], ["IN", "Índia"], ["TR", "Turquia"]].map(([c, n]) => (
                <option key={c} value={c}>
                  {n}
                </option>
              ))}
            </datalist>
          </Field>
          <Field label="Telefone">
            <Input value={e.telefone ?? ""} onChange={(x) => set("telefone", x.target.value)} />
          </Field>
          <Field label="Email">
            <Input value={e.email ?? ""} onChange={(x) => set("email", x.target.value)} />
          </Field>
          {e.tipo !== "F" && (
            <>
              <Field label="Nível de preço">
                <Select value={e.nivel_preco ?? 1} onChange={(x) => set("nivel_preco", Number(x.target.value))}>
                  {[1, 2, 3, 4].map((n) => (
                    <option key={n} value={n}>
                      PVP {n}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label="Desconto habitual (%)">
                <Input type="number" className="num text-right" value={e.desconto ?? 0} onChange={(x) => set("desconto", Number(x.target.value))} />
              </Field>
              <Field label="Limite de crédito (Kz)">
                <Input className="num text-right" value={limite} onChange={(x) => setLimite(x.target.value)} />
              </Field>
            </>
          )}
          <Field label="Retenção na fonte habitual" hint="Vem escolhida nas facturas e nos recibos desta entidade (ex.: empresas que retêm o Imposto Industrial ao pagar)">
            <Select value={e.retencao ?? ""} onChange={(x) => set("retencao", x.target.value)}>
              <option value="">Nenhuma</option>
              {retencoes.map((r) => (
                <option key={r.codigo} value={r.codigo}>
                  {r.descricao} ({r.percentagem.toLocaleString("pt-PT")}%)
                </option>
              ))}
            </Select>
          </Field>
          {e.tipo !== "F" && vendedores.length > 0 && (
            <Field label="Vendedor habitual" hint="Vem escolhido nas vendas a este cliente (POS, facturas, avenças) e conta nas comissões">
              <Select value={e.vendedor_id ? String(e.vendedor_id) : ""} onChange={(x) => set("vendedor_id", x.target.value ? Number(x.target.value) : 0)}>
                <option value="">Nenhum</option>
                {vendedores.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.nome}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <div className="col-span-2 mt-2 border-t border-line pt-4 text-[12px] font-medium uppercase tracking-wide text-ink-2">Contactos e dados pessoais</div>
          <Field label={e.nif && /^\d{10}$/.test(e.nif) ? "Pessoa de contacto" : "Pessoa de contacto (se for empresa)"}>
            <Input value={e.pessoa_contacto ?? ""} maxLength={120} onChange={(x) => set("pessoa_contacto", x.target.value)} />
          </Field>
          <Field label="Telefone alternativo">
            <Input value={e.telefone2 ?? ""} maxLength={40} inputMode="tel" onChange={(x) => set("telefone2", x.target.value)} />
          </Field>
          <Field label="Data de nascimento" hint="Para particulares (aniversários e ficha)">
            <InputData value={e.data_nascimento ?? ""} onChange={(x) => set("data_nascimento", x.target.value)} />
          </Field>
          <Field label="Sexo">
            <Select value={e.sexo ?? ""} onChange={(x) => set("sexo", x.target.value)}>
              <option value="">—</option>
              <option value="F">Feminino</option>
              <option value="M">Masculino</option>
            </Select>
          </Field>
          <Field label="N.º do BI, passaporte ou cartão de residente">
            <Input value={e.documento_identificacao ?? ""} maxLength={40} onChange={(x) => set("documento_identificacao", x.target.value)} />
          </Field>
          <Field label="Nacionalidade">
            <Input value={e.nacionalidade ?? ""} maxLength={60} placeholder="Angolana" onChange={(x) => set("nacionalidade", x.target.value)} />
          </Field>
          <Field label="Profissão" className="col-span-2">
            <Input value={e.profissao ?? ""} maxLength={80} onChange={(x) => set("profissao", x.target.value)} />
          </Field>
        </div>
      </AbasFicha>
      {imprimirFicha > 0 && <FichaImpressa e={e} retencoes={retencoes} vez={imprimirFicha} />}
    </Modal>
  );
}

/** Ficha do cliente ou do fornecedor em A4: identificação, contactos, dados pessoais, condições, dívida e assinaturas. */
function FichaImpressa({ e, retencoes, vez }: { e: Partial<Entidade>; retencoes: { codigo: string; descricao: string; percentagem: number }[]; vez: number }) {
  const empresa = useEmpresa();
  const consulta = useQuery({
    queryKey: ["ficha-pendentes", e.id],
    queryFn: () => call<{ documentos: { id: number; numero_doc: string; data_emissao: string; data_vencimento: string | null; total: number; valor_pendente: number }[] }>("contas.pendentes", { tipo: e.tipo === "F" ? "F" : "C", entidade_id: e.id }),
    enabled: !!e.id,
  });
  const pendentes = consulta.data?.documentos ?? [];
  // imprime quando já tem os documentos em dívida (e outra vez a cada pedido)
  const pronto = !e.id || consulta.isSuccess || consulta.isError;
  useEffect(() => {
    if (pronto) imprimir();
  }, [pronto, vez]);
  const ret = retencoes.find((r) => r.codigo === e.retencao);
  const linhas: [string, string | null | undefined][][] = [
    [
      ["Nome / Razão social", e.nome],
      ["NIF", e.nif],
      ["Tipo", e.tipo === "F" ? "Fornecedor" : e.tipo === "A" ? "Cliente e fornecedor" : "Cliente"],
      ["Endereço", e.endereco],
      ["Localidade", [e.localidade, e.provincia, e.pais].filter(Boolean).join(" · ")],
    ],
    [
      ["Telefone", [e.telefone, e.telefone2].filter(Boolean).join(" · ")],
      ["Email", e.email],
      ["Pessoa de contacto", e.pessoa_contacto],
    ],
    [
      ["Data de nascimento", e.data_nascimento ? e.data_nascimento.split("-").reverse().join("/") : null],
      ["Sexo", e.sexo === "F" ? "Feminino" : e.sexo === "M" ? "Masculino" : null],
      ["BI / passaporte", e.documento_identificacao],
      ["Nacionalidade", e.nacionalidade],
      ["Profissão", e.profissao],
    ],
    [
      ["Nível de preço", e.nivel_preco ? `PVP ${e.nivel_preco}` : null],
      ["Desconto habitual", e.desconto ? `${e.desconto}%` : null],
      ["Limite de crédito", e.limite_credito ? kz(e.limite_credito) : null],
      ["Retenção na fonte habitual", ret ? `${ret.descricao} (${ret.percentagem}%)` : null],
    ],
  ];
  const titulos = ["Identificação", "Contactos", "Dados pessoais", "Condições comerciais"];
  const divida = pendentes.reduce((s, p) => s + p.valor_pendente, 0);
  return (
    <PrintPortal>
      <div className="p-2 text-[11.5px] text-black">
        <CabecalhoEmpresa empresa={empresa} titulo={e.tipo === "F" ? "Ficha de fornecedor" : "Ficha de cliente"} numero={e.id ? `N.º ${e.id}` : undefined} />
        <div className="grid grid-cols-2 gap-x-8 gap-y-4">
          {linhas.map((bloco, n) => {
            const cheias = bloco.filter(([, v]) => v);
            if (!cheias.length) return null;
            return (
              <div key={titulos[n]}>
                <div className="mb-1 border-b border-black pb-0.5 text-[10.5px] font-bold uppercase tracking-wide">{titulos[n]}</div>
                {cheias.map(([k, v]) => (
                  <div key={k} className="flex justify-between gap-3 border-b border-dotted border-gray-300 py-0.5">
                    <span className="text-gray-600">{k}</span>
                    <span className="text-right font-medium">{v}</span>
                  </div>
                ))}
              </div>
            );
          })}
        </div>
        {e.observacoes && (
          <div className="mt-4">
            <div className="mb-1 border-b border-black pb-0.5 text-[10.5px] font-bold uppercase tracking-wide">Observações</div>
            <p className="whitespace-pre-wrap">{e.observacoes}</p>
          </div>
        )}
        <div className="mt-5">
          <div className="mb-1 border-b border-black pb-0.5 text-[10.5px] font-bold uppercase tracking-wide">Situação a {new Date().toLocaleDateString("pt-PT")}</div>
          {pendentes.length ? (
            <table className="w-full border-collapse text-left">
              <thead>
                <tr className="border-b border-black">
                  <th className="py-1">Documento</th>
                  <th className="py-1">Emissão</th>
                  <th className="py-1">Vencimento</th>
                  <th className="py-1 text-right">Total</th>
                  <th className="py-1 text-right">Em dívida</th>
                </tr>
              </thead>
              <tbody>
                {pendentes.map((p) => (
                  <tr key={p.id} className="border-b border-dotted border-gray-300">
                    <td className="py-0.5">{p.numero_doc}</td>
                    <td className="py-0.5">{p.data_emissao.split("-").reverse().join("/")}</td>
                    <td className="py-0.5">{p.data_vencimento ? p.data_vencimento.split("-").reverse().join("/") : ""}</td>
                    <td className="py-0.5 text-right">{kz(p.total)}</td>
                    <td className="py-0.5 text-right">{kz(p.valor_pendente)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <p>Sem documentos em dívida.</p>
          )}
          <div className="mt-2 text-right font-bold">Total em dívida: {kz(divida)}</div>
        </div>
        <div className="mt-14 grid grid-cols-2 gap-16 text-center">
          <div className="border-t border-black pt-1">{e.tipo === "F" ? "O fornecedor" : "O cliente"}</div>
          <div className="border-t border-black pt-1">Pela empresa</div>
        </div>
      </div>
    </PrintPortal>
  );
}

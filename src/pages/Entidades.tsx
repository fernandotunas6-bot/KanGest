import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { FileSpreadsheet, HandCoins, Plus, Printer, Search, Users } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { kz, parseKz, toInputKz } from "@/lib/format";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { imprimir } from "@/lib/print";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";
import type { Entidade } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th } from "@/components/ui";

export default function Entidades({ tipo }: { tipo: "C" | "F" }) {
  const [q, setQ] = useState("");
  const [editar, setEditar] = useState<Partial<Entidade> | null>(null);
  const dq = useDebounce(q);
  const nav = useNavigate();
  const nome = tipo === "C" ? "Clientes" : "Fornecedores";
  const lista = useQuery({ queryKey: ["entidades", tipo, dq], queryFn: () => call<Entidade[]>("entidades.listar", { tipo, q: dq }) });
  const empresa = useEmpresa();
  const linhas = lista.data ?? [];
  const totalDivida = linhas.reduce((s, e) => s + Math.max(0, e.saldo ?? 0), 0);
  function exportar() {
    exportarFicheiro(
      `${nome.toLowerCase()}.csv`,
      csv([["Nome", "NIF", "Telefone", "Email", "Endereço", "Localidade", "Província", "Saldo em dívida"], ...linhas.map((e) => [e.nome, e.nif, e.telefone ?? "", e.email ?? "", e.endereco ?? "", e.localidade ?? "", e.provincia ?? "", csvKz(e.saldo ?? 0)])]),
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
              <FileSpreadsheet className="size-4" /> CSV
            </Button>
            <Button variant="secondary" onClick={imprimir} disabled={!linhas.length}>
              <Printer className="size-4" /> Imprimir
            </Button>
            {tipo === "C" && (
              <Button variant="secondary" onClick={() => nav("/recibos/novo")}>
                <HandCoins className="size-4" /> Emitir recibo
              </Button>
            )}
            <Button onClick={() => setEditar({ tipo, nivel_preco: 1 })}>
              <Plus className="size-4" /> Novo {tipo === "C" ? "cliente" : "fornecedor"}
            </Button>
          </>
        }
      />
      <Card>
        <div className="border-b border-line p-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Pesquisar por nome, NIF ou telefone…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<Users className="size-6" />} title={`Sem ${nome.toLowerCase()}`} />
        ) : (
          <Table className="max-h-[calc(100vh-260px)]">
            <thead>
              <tr>
                <Th>Nome</Th>
                <Th>NIF</Th>
                <Th>Contacto</Th>
                <Th>Localidade</Th>
                <Th className="text-right">Saldo em dívida</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((e) => (
                <tr key={e.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => e.id !== 1 && setEditar(e)}>
                  <Td className="font-medium">
                    {e.nome} {e.id === 1 && <Badge className="ml-2">Sistema</Badge>}
                  </Td>
                  <Td className="font-mono text-[12.5px]">{e.nif}</Td>
                  <Td className="text-ink-2">{[e.telefone, e.email].filter(Boolean).join(" · ") || "—"}</Td>
                  <Td className="text-ink-2">{e.localidade ?? "—"}</Td>
                  <Td className="num text-right">{(e.saldo ?? 0) > 0 ? <span className="font-medium text-[#9a6514]">{kz(e.saldo)}</span> : <span className="text-ink-3">—</span>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {editar && <EntidadeForm inicial={editar} onClose={() => setEditar(null)} />}
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
    </>
  );
}

function EntidadeForm({ inicial, onClose }: { inicial: Partial<Entidade>; onClose: () => void }) {
  const [e, setE] = useState(inicial);
  const [limite, setLimite] = useState(toInputKz(inicial.limite_credito ?? 0));
  const [busy, setBusy] = useState(false);
  const qc = useQueryClient();
  const set = (k: keyof Entidade, v: unknown) => setE((x) => ({ ...x, [k]: v }));

  async function guardar() {
    setBusy(true);
    try {
      await call("entidades.guardar", { ...e, limite_credito: parseKz(limite) });
      toast.success("Guardado.");
      qc.invalidateQueries({ queryKey: ["entidades"] });
      onClose();
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
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={guardar}>
            Guardar
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Nome / Razão social" className="col-span-2">
          <Input autoFocus value={e.nome ?? ""} onChange={(x) => set("nome", x.target.value)} />
        </Field>
        <Field label="NIF" hint="Vazio = 999999999 (consumidor final)">
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
      </div>
    </Modal>
  );
}

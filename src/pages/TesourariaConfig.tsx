import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { ArrowLeft, Banknote, Building2, Plus } from "lucide-react";
import { call } from "@/lib/api";
import { kz, parseKz, toInputKz } from "@/lib/format";
import { Badge, Button, Card, Field, Input, Modal, PageHeader, Select, Table, Td, Th } from "@/components/ui";
import type { Centro, Conta } from "./Tesouraria";

export default function TesourariaConfig() {
  const nav = useNavigate();
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<Conta[]>("tesouraria.contas") });
  const centros = useQuery({ queryKey: ["tes-centros"], queryFn: () => call<Centro[]>("tesouraria.centros") });
  const [conta, setConta] = useState<Partial<Conta> | null>(null);
  const [inicial, setInicial] = useState("0,00");
  const [centro, setCentro] = useState<Partial<Centro> | null>(null);

  async function gravarConta() {
    if (!conta) return;
    try {
      await call("tesouraria.contaGuardar", { ...conta, saldo_inicial: parseKz(inicial), ativo: conta.ativo !== 0 });
      toast.success("Conta guardada.");
      setConta(null);
      contas.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  async function gravarCentro() {
    if (!centro) return;
    try {
      await call("tesouraria.centroGuardar", { ...centro, ativo: centro.ativo !== 0 });
      toast.success("Centro de custo guardado.");
      setCentro(null);
      centros.refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <>
      <PageHeader
        title="Contas e Centros de Custo"
        subtitle="Caixas, contas bancárias e rubricas usadas no fluxo de caixa"
        actions={
          <Button variant="ghost" onClick={() => nav("/tesouraria")}>
            <ArrowLeft className="size-4" /> Fluxo de caixa
          </Button>
        }
      />
      <div className="grid grid-cols-2 gap-4">
        <Card>
          <div className="flex items-center justify-between px-5 pb-2 pt-4">
            <span className="font-semibold">Contas (caixa e bancos)</span>
            <Button
              size="sm"
              onClick={() => {
                setConta({ tipo: "BANCO", ativo: 1 });
                setInicial("0,00");
              }}
            >
              <Plus className="size-4" /> Nova conta
            </Button>
          </div>
          <Table>
            <thead>
              <tr>
                <Th>Conta</Th>
                <Th>Banco / IBAN</Th>
                <Th className="text-right">Saldo</Th>
              </tr>
            </thead>
            <tbody>
              {contas.data?.map((c) => (
                <tr
                  key={c.id}
                  className="cursor-pointer hover:bg-sunken/60"
                  onClick={() => {
                    setConta(c);
                    setInicial(toInputKz(c.saldo_inicial));
                  }}
                >
                  <Td>
                    <span className="inline-flex items-center gap-2 font-medium">
                      {c.tipo === "CAIXA" ? <Banknote className="size-4 text-[#9a6514]" /> : <Building2 className="size-4 text-brand" />}
                      {c.descricao}
                    </span>
                    {!c.ativo && <Badge tone="danger" className="ml-2">Inactiva</Badge>}
                  </Td>
                  <Td className="text-ink-2">{[c.banco, c.iban].filter(Boolean).join(" · ") || "—"}</Td>
                  <Td className="num text-right font-medium">{kz(c.saldo)}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </Card>

        <Card>
          <div className="flex items-center justify-between px-5 pb-2 pt-4">
            <span className="font-semibold">Centros de custo / rubricas</span>
            <Button size="sm" onClick={() => setCentro({ tipo: "DESPESA", ativo: 1 })}>
              <Plus className="size-4" /> Novo
            </Button>
          </div>
          <Table className="max-h-[60vh]">
            <thead>
              <tr>
                <Th>Descrição</Th>
                <Th>Tipo</Th>
              </tr>
            </thead>
            <tbody>
              {centros.data?.map((c) => (
                <tr key={c.id} className="cursor-pointer hover:bg-sunken/60" onClick={() => setCentro(c)}>
                  <Td className={c.ativo ? "" : "text-ink-3 line-through"}>{c.descricao}</Td>
                  <Td>{c.tipo === "DESPESA" ? <Badge tone="danger">Despesa</Badge> : <Badge tone="ok">Receita</Badge>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        </Card>
      </div>

      {conta && (
        <Modal open onClose={() => setConta(null)} title={conta.id ? "Editar conta" : "Nova conta"} width="max-w-lg" footer={<Button onClick={gravarConta}>Gravar</Button>}>
          <div className="grid grid-cols-2 gap-4">
            <Field label="Descrição da conta" className="col-span-2">
              <Input autoFocus value={conta.descricao ?? ""} onChange={(e) => setConta({ ...conta, descricao: e.target.value })} />
            </Field>
            <Field label="Tipo">
              <Select value={conta.tipo} onChange={(e) => setConta({ ...conta, tipo: e.target.value as Conta["tipo"] })}>
                <option value="CAIXA">Caixa</option>
                <option value="BANCO">Banco</option>
              </Select>
            </Field>
            <Field label="Saldo inicial (Kz)">
              <Input className="num text-right" value={inicial} onChange={(e) => setInicial(e.target.value)} />
            </Field>
            {conta.tipo === "BANCO" && (
              <>
                <Field label="Banco">
                  <Input value={conta.banco ?? ""} onChange={(e) => setConta({ ...conta, banco: e.target.value })} />
                </Field>
                <Field label="Agência">
                  <Input value={conta.agencia ?? ""} onChange={(e) => setConta({ ...conta, agencia: e.target.value })} />
                </Field>
                <Field label="N.º de conta">
                  <Input value={conta.numero ?? ""} onChange={(e) => setConta({ ...conta, numero: e.target.value })} />
                </Field>
                <Field label="IBAN">
                  <Input value={conta.iban ?? ""} onChange={(e) => setConta({ ...conta, iban: e.target.value.toUpperCase() })} />
                </Field>
              </>
            )}
            {conta.id && (
              <label className="col-span-2 flex items-center gap-2">
                <input type="checkbox" checked={conta.ativo !== 0} onChange={(e) => setConta({ ...conta, ativo: e.target.checked ? 1 : 0 })} /> Conta activa
              </label>
            )}
          </div>
        </Modal>
      )}

      {centro && (
        <Modal open onClose={() => setCentro(null)} title={centro.id ? "Editar centro de custo" : "Novo centro de custo"} width="max-w-md" footer={<Button onClick={gravarCentro}>Gravar</Button>}>
          <div className="space-y-4">
            <Field label="Descritivo">
              <Input autoFocus value={centro.descricao ?? ""} onChange={(e) => setCentro({ ...centro, descricao: e.target.value })} />
            </Field>
            <Field label="Tipo">
              <Select value={centro.tipo} onChange={(e) => setCentro({ ...centro, tipo: e.target.value as Centro["tipo"] })}>
                <option value="DESPESA">Despesa (pagamentos)</option>
                <option value="RECEITA">Receita (entradas)</option>
              </Select>
            </Field>
            {centro.id && (
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={centro.ativo !== 0} onChange={(e) => setCentro({ ...centro, ativo: e.target.checked ? 1 : 0 })} /> Activo
              </label>
            )}
          </div>
        </Modal>
      )}
    </>
  );
}

import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { FileSpreadsheet, Plus, Printer, Search, Trash2, UserRound, UsersRound } from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce } from "@/lib/hooks";
import { data, kz, parseKz, toInputKz } from "@/lib/format";
import type { Funcionario, TabelasRH } from "@/lib/types";
import { Badge, Button, Card, Empty, Field, Input, Modal, PageHeader, Select, Table, Td, Th, cn } from "@/components/ui";
import { imprimir } from "@/lib/print";
import { csv, csvKz, exportarFicheiro } from "@/lib/csv";
import { PrintPortal } from "@/components/DocumentoPrint";
import { CabecalhoEmpresa, useEmpresa } from "@/components/PrintCabecalho";

export function useTabelasRH() {
  return useQuery({ queryKey: ["rh-tabelas"], queryFn: () => call<TabelasRH>("rh.tabelas"), staleTime: 60_000 });
}

/** Subsídios fixos da ficha (rubricas 002–014). */
export const SUBSIDIOS: { campo: keyof Funcionario; label: string }[] = [
  { campo: "sub_alimentacao", label: "Subsídio de alimentação" },
  { campo: "sub_transporte", label: "Subsídio de transporte" },
  { campo: "sub_chefia", label: "Subsídio de chefia" },
  { campo: "sub_turno", label: "Subsídio de turno" },
  { campo: "abono_falhas", label: "Abono para falhas" },
  { campo: "sub_arrendamento", label: "Subsídio de arrendamento" },
  { campo: "sub_atavio", label: "Subsídio de atavio" },
  { campo: "sub_isencao_horario", label: "Isenção de horário" },
  { campo: "sub_deslocacao", label: "Subsídio de deslocação" },
];

const ABAS = ["Identificação", "Contactos", "Contrato", "Remuneração", "Banco", "Agregado familiar"] as const;

export default function Funcionarios() {
  const [q, setQ] = useState("");
  const [inativos, setInativos] = useState(false);
  const dq = useDebounce(q);
  const [editar, setEditar] = useState<number | "novo" | null>(null);
  const lista = useQuery({ queryKey: ["funcionarios", dq, inativos], queryFn: () => call<Funcionario[]>("rh.funcionarios", { q: dq, inativos }) });
  const massa = (lista.data ?? []).filter((f) => f.ativo).reduce((s, f) => s + f.salario_base + SUBSIDIOS.reduce((x, c) => x + (Number(f[c.campo]) || 0), 0), 0);
  const empresa = useEmpresa();
  // a lista impressa só sai quando se pede (o portal também serve a ficha individual)
  const [imprimirLista, setImprimirLista] = useState(false);
  const [agregados, setAgregados] = useState<AgregadoLinha[] | null>(null);
  function exportar() {
    exportarFicheiro(
      "funcionarios.csv",
      csv([
        ["N.º", "Nome", "NIF", "N.º INSS", "BI", "Departamento", "Função", "Categoria", "Contrato", "Admissão", "Saída", "Salário base", "Conta própria", "Estado"],
        ...(lista.data ?? []).map((f) => [
          f.numero ?? "", f.nome, f.nif ?? "", f.inss_numero ?? "", f.bi_numero ?? "", f.departamento ?? "", f.funcao ?? "", f.categoria ?? "", f.tipo_contrato ?? "",
          f.data_admissao ?? "", f.data_saida ?? "", csvKz(f.salario_base), f.independente ? "Sim" : "Não", f.ativo ? "Activo" : "Inactivo",
        ]),
      ]),
    );
  }

  return (
    <>
      <PageHeader
        title="Funcionários"
        subtitle={lista.data ? `${lista.data.filter((f) => f.ativo).length} activos · massa salarial fixa ${kz(massa)}` : "Fichas do pessoal"}
        actions={
          <>
            <Button variant="secondary" onClick={exportar} disabled={!lista.data?.length}>
              <FileSpreadsheet className="size-4" /> CSV
            </Button>
            <Button
              variant="secondary"
              onClick={() =>
                call<AgregadoLinha[]>("rh.agregados")
                  .then(setAgregados)
                  .catch((e) => toast.error((e as Error).message))
              }
              title="Agregado familiar de todos os funcionários"
            >
              <UsersRound className="size-4" /> Agregados
            </Button>
            <Button
              variant="secondary"
              disabled={!lista.data?.length}
              onClick={() => {
                setImprimirLista(true);
                imprimir();
                setTimeout(() => setImprimirLista(false), 4000);
              }}
            >
              <Printer className="size-4" /> Imprimir lista
            </Button>
            <Button onClick={() => setEditar("novo")}>
              <Plus className="size-4" /> Novo funcionário
            </Button>
          </>
        }
      />
      <Card>
        <div className="flex items-center gap-4 border-b border-line p-4">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-3" />
            <Input className="pl-9" placeholder="Nome, n.º, NIF ou BI…" value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <label className="flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={inativos} onChange={(e) => setInativos(e.target.checked)} /> Mostrar inactivos
          </label>
        </div>
        {lista.data?.length === 0 ? (
          <Empty icon={<UserRound className="size-6" />} title="Sem funcionários" text="Crie as fichas do pessoal para processar salários." />
        ) : (
          <Table className="max-h-[calc(100vh-260px)]">
            <thead>
              <tr>
                <Th>N.º</Th>
                <Th>Nome</Th>
                <Th>Departamento</Th>
                <Th>Função</Th>
                <Th>Admissão</Th>
                <Th className="text-right">Salário base</Th>
                <Th>Estado</Th>
              </tr>
            </thead>
            <tbody>
              {lista.data?.map((f) => (
                <tr key={f.id} className={cn("cursor-pointer hover:bg-sunken/60", !f.ativo && "opacity-55")} onClick={() => setEditar(f.id)}>
                  <Td className="font-mono text-[12.5px] text-ink-2">{f.numero ?? "—"}</Td>
                  <Td>
                    <div className="font-medium">{f.nome}</div>
                    <div className="text-[12px] text-ink-3">{[f.nif && `NIF ${f.nif}`, f.inss_numero && `INSS ${f.inss_numero}`].filter(Boolean).join(" · ")}</div>
                  </Td>
                  <Td className="text-ink-2">{f.departamento ?? "—"}</Td>
                  <Td className="text-ink-2">{f.funcao ?? "—"}</Td>
                  <Td className="text-ink-2">{data(f.data_admissao)}</Td>
                  <Td className="num text-right font-medium">
                    {f.independente ? (
                      <span title="Trabalhador por conta própria: honorário">
                        {kz(f.honorario)} <span className="text-[11px] font-normal text-ink-3">hon.</span>
                      </span>
                    ) : (
                      kz(f.salario_base)
                    )}
                  </Td>
                  <Td>{f.ativo ? <Badge tone="ok">Activo</Badge> : <Badge>Inactivo</Badge>}</Td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {editar !== null && <FuncionarioForm id={editar === "novo" ? null : editar} onClose={() => setEditar(null)} />}
      {agregados && <AgregadosModal linhas={agregados} empresa={empresa} onClose={() => setAgregados(null)} />}
      {imprimirLista && editar === null && (
        <PrintPortal>
          <div className="p-2 text-[11px] text-black">
            <CabecalhoEmpresa empresa={empresa} titulo={inativos ? "Lista Geral de Funcionários" : "Lista de Funcionários"} />
            <table className="w-full border-collapse text-left">
              <thead>
                <tr className="border-b border-black">
                  <th className="py-1">N.º</th>
                  <th className="py-1">Nome</th>
                  <th className="py-1">NIF / INSS</th>
                  <th className="py-1">Departamento</th>
                  <th className="py-1">Função</th>
                  <th className="py-1">Admissão</th>
                  <th className="py-1 text-right">Salário base</th>
                  <th className="py-1">Estado</th>
                </tr>
              </thead>
              <tbody>
                {(lista.data ?? []).map((f) => (
                  <tr key={f.id} className="border-b border-dotted border-gray-300">
                    <td className="py-0.5">{f.numero ?? ""}</td>
                    <td className="py-0.5">
                      {f.nome}
                      {f.independente ? " (conta própria)" : ""}
                    </td>
                    <td className="py-0.5">{[f.nif, f.inss_numero].filter(Boolean).join(" / ")}</td>
                    <td className="py-0.5">{f.departamento ?? ""}</td>
                    <td className="py-0.5">{f.funcao ?? ""}</td>
                    <td className="py-0.5">{f.data_admissao ? data(f.data_admissao) : ""}</td>
                    <td className="py-0.5 text-right">{kz(f.salario_base)}</td>
                    <td className="py-0.5">{f.ativo ? "Activo" : "Inactivo"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="mt-2 font-bold">
              {lista.data?.length ?? 0} funcionários · massa salarial fixa dos activos {kz(massa)}
            </div>
          </div>
        </PrintPortal>
      )}
    </>
  );
}

type Agregado = { key: string; nome: string; parentesco: string; data_nascimento: string };
const DINHEIRO: (keyof Funcionario)[] = ["salario_base", ...SUBSIDIOS.map((s) => s.campo)];

function FuncionarioForm({ id, onClose }: { id: number | null; onClose: () => void }) {
  const qc = useQueryClient();
  const tabs = useTabelasRH().data;
  const contas = useQuery({ queryKey: ["tes-contas"], queryFn: () => call<{ id: number; descricao: string }[]>("tesouraria.contas") }).data;
  const atual = useQuery({ queryKey: ["funcionario", id], queryFn: () => call<Funcionario>("rh.funcionario", { id }), enabled: !!id }).data;
  const [aba, setAba] = useState<(typeof ABAS)[number]>("Identificação");
  const [f, setF] = useState<Partial<Funcionario>>({ desconta_inss: 1, desconta_irt: 1, ativo: 1, tipo_contrato: "Indeterminado", nacionalidade: "Angolana" });
  const [kzs, setKzs] = useState<Record<string, string>>({});
  const [agregado, setAgregado] = useState<Agregado[]>([]);
  const [busy, setBusy] = useState(false);
  const [nova, setNova] = useState<{ tabela: "departamentos" | "funcoes" | "categorias"; campo: keyof Funcionario; label: string; texto: string } | null>(null);

  useEffect(() => {
    if (!atual) return;
    setF(atual);
    setKzs({ ...Object.fromEntries(DINHEIRO.map((c) => [c, toInputKz(Number(atual[c]) || 0)])), honorario: toInputKz(Number(atual.honorario) || 0) });
    setAgregado((atual.agregado ?? []).map((a) => ({ key: crypto.randomUUID(), nome: a.nome, parentesco: a.parentesco ?? "", data_nascimento: a.data_nascimento ?? "" })));
  }, [atual]);

  const set = (k: keyof Funcionario, v: unknown) => setF((x) => ({ ...x, [k]: v }));
  const txt = (k: keyof Funcionario, label: string, extra?: { type?: string; className?: string; placeholder?: string }) => (
    <Field label={label} className={extra?.className}>
      <Input type={extra?.type} placeholder={extra?.placeholder} value={(f[k] as string | null) ?? ""} onChange={(e) => set(k, e.target.value)} />
    </Field>
  );
  const dinheiro = (k: keyof Funcionario, label: string) => (
    <Field label={label}>
      <Input className="num text-right" value={kzs[k] ?? "0,00"} onChange={(e) => setKzs((x) => ({ ...x, [k]: e.target.value }))} />
    </Field>
  );
  const totalFixo = DINHEIRO.reduce((s, c) => s + parseKz(kzs[c] ?? "0"), 0);

  async function gravarNova() {
    if (!nova) return;
    try {
      const r = await call<{ id: number }>("rh.tabelaGuardar", { tabela: nova.tabela, descricao: nova.texto });
      await qc.invalidateQueries({ queryKey: ["rh-tabelas"] });
      set(nova.campo, r.id);
      setNova(null);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function guardar() {
    setBusy(true);
    try {
      const corpo: Record<string, unknown> = { ...f };
      delete corpo.departamento;
      delete corpo.funcao;
      delete corpo.categoria;
      delete corpo.criado_em;
      for (const c of DINHEIRO) corpo[c] = parseKz(kzs[c] ?? "0");
      corpo.desconta_inss = !!f.desconta_inss;
      corpo.desconta_irt = !!f.desconta_irt;
      corpo.ativo = f.ativo !== 0;
      corpo.independente = !!f.independente;
      corpo.honorario = parseKz(kzs.honorario ?? "0");
      corpo.tipo_honorario = f.tipo_honorario ?? "MENSAL";
      corpo.agregado = agregado.filter((a) => a.nome.trim()).map((a) => ({ nome: a.nome, parentesco: a.parentesco || null, data_nascimento: a.data_nascimento || null }));
      await call("rh.funcionarioGuardar", corpo);
      toast.success("Ficha guardada.");
      qc.invalidateQueries({ queryKey: ["funcionarios"] });
      qc.invalidateQueries({ queryKey: ["funcionario", id] });
      onClose();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open
      onClose={onClose}
      title={id ? f.nome ?? "Funcionário" : "Novo funcionário"}
      width="max-w-4xl"
      footer={
        <>
          {atual && (
            <Button variant="ghost" className="mr-auto" onClick={imprimir} title="Imprime a ficha gravada (FichaFuncionario do KIGEST)">
              <Printer className="size-4" /> Imprimir ficha
            </Button>
          )}
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button loading={busy} onClick={guardar}>
            Guardar
          </Button>
        </>
      }
    >
      {atual && (
        <PrintPortal>
          <FichaPrint f={atual} />
        </PrintPortal>
      )}
      <div className="mb-5 flex flex-wrap gap-1 rounded-lg bg-sunken p-1">
        {ABAS.map((a) => (
          <button key={a} onClick={() => setAba(a)} className={cn("rounded-md px-3 py-1.5 text-[13px]", aba === a ? "bg-surface font-medium shadow-sm" : "text-ink-2 hover:text-ink")}>
            {a}
          </button>
        ))}
      </div>

      {aba === "Identificação" && (
        <div className="grid grid-cols-4 gap-4">
          {txt("nome", "Nome completo", { className: "col-span-3" })}
          {txt("numero", "N.º mecanográfico")}
          <Field label="Sexo">
            <Select value={f.sexo ?? ""} onChange={(e) => set("sexo", e.target.value)}>
              <option value="">—</option>
              <option value="M">Masculino</option>
              <option value="F">Feminino</option>
            </Select>
          </Field>
          {txt("data_nascimento", "Data de nascimento", { type: "date" })}
          <Field label="Estado civil">
            <Select value={f.estado_civil ?? ""} onChange={(e) => set("estado_civil", e.target.value)}>
              <option value="">—</option>
              {["Solteiro(a)", "Casado(a)", "União de facto", "Divorciado(a)", "Viúvo(a)"].map((x) => (
                <option key={x}>{x}</option>
              ))}
            </Select>
          </Field>
          {txt("nacionalidade", "Nacionalidade")}
          {txt("naturalidade", "Naturalidade")}
          {txt("bi_numero", "N.º do BI / passaporte")}
          {txt("bi_local_emissao", "Local de emissão")}
          {txt("bi_validade", "Validade do BI", { type: "date" })}
          {txt("nif", "NIF")}
          {txt("inss_numero", "N.º de beneficiário INSS")}
        </div>
      )}

      {aba === "Contactos" && (
        <div className="grid grid-cols-4 gap-4">
          {txt("morada", "Morada", { className: "col-span-4" })}
          {txt("localidade", "Bairro / localidade")}
          {txt("municipio", "Município")}
          {txt("provincia", "Província")}
          <div />
          {txt("telefone", "Telefone")}
          {txt("email", "Email", { className: "col-span-2" })}
        </div>
      )}

      {aba === "Contrato" && (
        <div className="grid grid-cols-3 gap-4">
          {(
            [
              ["departamentos", "departamento_id", "Departamento"],
              ["funcoes", "funcao_id", "Função"],
              ["categorias", "categoria_id", "Categoria profissional"],
            ] as const
          ).map(([tabela, campo, label]) => (
            <Field key={campo} label={label}>
              <div className="flex gap-2">
                <Select value={(f[campo] as number | null) ?? ""} onChange={(e) => set(campo, e.target.value ? Number(e.target.value) : null)}>
                  <option value="">—</option>
                  {tabs?.[tabela].map((x) => (
                    <option key={x.id} value={x.id}>
                      {x.descricao}
                    </option>
                  ))}
                </Select>
                <Button variant="secondary" title={`Novo(a) ${label.toLowerCase()}`} onClick={() => setNova({ tabela, campo, label, texto: "" })}>
                  <Plus className="size-4" />
                </Button>
              </div>
            </Field>
          ))}
          <Field label="Tipo de contrato">
            <Select value={f.tipo_contrato ?? ""} onChange={(e) => set("tipo_contrato", e.target.value)}>
              {["Indeterminado", "Determinado", "Estágio", "Prestação de serviços"].map((x) => (
                <option key={x}>{x}</option>
              ))}
            </Select>
          </Field>
          {txt("data_admissao", "Data de admissão", { type: "date" })}
          {txt("data_saida", "Data de saída", { type: "date" })}
          {txt("motivo_saida", "Motivo de saída", { className: "col-span-3" })}
          <label className="col-span-3 flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={f.ativo !== 0} onChange={(e) => set("ativo", e.target.checked ? 1 : 0)} /> Funcionário activo (entra no processamento salarial)
          </label>
        </div>
      )}

      {aba === "Remuneração" && (
        <>
          <div className="mb-5 grid grid-cols-4 items-end gap-4 rounded-xl border border-line bg-sunken/50 p-4">
            <label className="col-span-2 flex h-10 items-center gap-2">
              <input type="checkbox" checked={!!f.independente} onChange={(e) => set("independente", e.target.checked ? 1 : 0)} />
              Trabalhador por conta própria (honorários, IRT Grupo B)
            </label>
            {f.independente ? (
              <>
                <Field label="Honorário (Kz)">
                  <Input className="num text-right" value={kzs.honorario ?? "0,00"} onChange={(e) => setKzs((x) => ({ ...x, honorario: e.target.value }))} />
                </Field>
                <Field label="Base do honorário">
                  <Select value={f.tipo_honorario ?? "MENSAL"} onChange={(e) => set("tipo_honorario", e.target.value)}>
                    <option value="MENSAL">Por mês</option>
                    <option value="DIA">Por dia</option>
                    <option value="HORA">Por hora</option>
                  </Select>
                </Field>
                {txt("local_trabalho", "Local de trabalho", { className: "col-span-4" })}
              </>
            ) : (
              <p className="col-span-2 text-[12.5px] text-ink-3">Os independentes não entram no processamento salarial; processam-se em Honorários e Prestações.</p>
            )}
          </div>
          <div className="grid grid-cols-4 gap-4">
            {dinheiro("salario_base", "Salário base (Kz)")}
            {SUBSIDIOS.map((s) => (
              <div key={s.campo}>{dinheiro(s.campo, s.label)}</div>
            ))}
          </div>
          <div className="mt-5 grid grid-cols-4 items-end gap-4 rounded-xl border border-line p-4">
            <label className="flex h-10 items-center gap-2">
              <input type="checkbox" checked={!!f.desconta_inss} onChange={(e) => set("desconta_inss", e.target.checked ? 1 : 0)} /> Desconta INSS
            </label>
            <label className="flex h-10 items-center gap-2">
              <input type="checkbox" checked={!!f.desconta_irt} onChange={(e) => set("desconta_irt", e.target.checked ? 1 : 0)} /> Desconta IRT
            </label>
            <Field label="Quota sindical (%)">
              <Input className="num text-right" type="number" min={0} max={10} step="0.1" value={f.desconto_sindicato ?? 0} onChange={(e) => set("desconto_sindicato", Number(e.target.value))} />
            </Field>
            <div className="text-right">
              <div className="text-[12px] uppercase tracking-wide text-ink-3">Remuneração fixa mensal</div>
              <div className="num text-[20px] font-semibold">{kz(totalFixo)}</div>
            </div>
          </div>
          <p className="mt-3 text-[12.5px] text-ink-3">
            Horas extra, faltas, prémios, subsídios de férias e de Natal e adiantamentos lançam-se mês a mês em Processamento salarial → Alterações do mês.
          </p>
        </>
      )}

      {aba === "Banco" && (
        <div className="grid grid-cols-3 gap-4">
          {txt("banco", "Banco", { placeholder: "BAI, BFA, BIC, BPC…" })}
          {txt("iban", "IBAN", { className: "col-span-2", placeholder: "AO06 …" })}
          <Field label="Conta da empresa para pagamento" className="col-span-2">
            <Select value={f.conta_empresa_id ?? ""} onChange={(e) => set("conta_empresa_id", e.target.value ? Number(e.target.value) : null)}>
              <option value="">—</option>
              {contas?.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.descricao}
                </option>
              ))}
            </Select>
          </Field>
        </div>
      )}

      {aba === "Agregado familiar" && (
        <div>
          <div className="space-y-2">
            {agregado.map((a) => (
              <div key={a.key} className="grid grid-cols-[minmax(0,1fr)_180px_170px_36px] gap-2">
                <Input placeholder="Nome" value={a.nome} onChange={(e) => setAgregado((xs) => xs.map((x) => (x.key === a.key ? { ...x, nome: e.target.value } : x)))} />
                <Select value={a.parentesco} onChange={(e) => setAgregado((xs) => xs.map((x) => (x.key === a.key ? { ...x, parentesco: e.target.value } : x)))}>
                  <option value="">Parentesco…</option>
                  {["Cônjuge", "Filho(a)", "Pai", "Mãe", "Outro"].map((p) => (
                    <option key={p}>{p}</option>
                  ))}
                </Select>
                <Input type="date" value={a.data_nascimento} onChange={(e) => setAgregado((xs) => xs.map((x) => (x.key === a.key ? { ...x, data_nascimento: e.target.value } : x)))} />
                <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => setAgregado((xs) => xs.filter((x) => x.key !== a.key))}>
                  <Trash2 className="size-4" />
                </button>
              </div>
            ))}
          </div>
          <Button variant="secondary" size="sm" className="mt-3" onClick={() => setAgregado((xs) => [...xs, { key: crypto.randomUUID(), nome: "", parentesco: "", data_nascimento: "" }])}>
            <Plus className="size-4" /> Adicionar familiar
          </Button>
          <p className="mt-3 text-[12.5px] text-ink-3">O número de dependentes da ficha é actualizado a partir desta lista.</p>
        </div>
      )}
      {nova && (
        <Modal
          open
          onClose={() => setNova(null)}
          title={`Novo(a) ${nova.label.toLowerCase()}`}
          width="max-w-md"
          footer={
            <>
              <Button variant="secondary" onClick={() => setNova(null)}>
                Cancelar
              </Button>
              <Button onClick={gravarNova}>Criar</Button>
            </>
          }
        >
          <Field label="Descrição">
            <Input autoFocus value={nova.texto} onChange={(e) => setNova({ ...nova, texto: e.target.value })} onKeyDown={(e) => e.key === "Enter" && gravarNova()} />
          </Field>
        </Modal>
      )}
    </Modal>
  );
}

/** Ficha de registo do funcionário para imprimir e assinar (FichaFuncionario do KIGEST). */
function FichaPrint({ f }: { f: Funcionario }) {
  const empresa = useEmpresa();
  const campo = (rotulo: string, valor: string | number | null | undefined) => (
    <div key={rotulo} className="flex gap-2 border-b border-dotted border-gray-300 py-0.5">
      <span className="w-40 shrink-0 text-gray-600">{rotulo}</span>
      <span className="font-medium">{valor === null || valor === undefined || valor === "" ? "—" : valor}</span>
    </div>
  );
  const seccao = (titulo: string) => <div className="mb-1 mt-4 border-b-2 border-black pb-0.5 text-[12px] font-bold uppercase">{titulo}</div>;
  const subsidios = SUBSIDIOS.filter((x) => Number(f[x.campo]) > 0);
  const totalFixo = Number(f.salario_base) + subsidios.reduce((s, x) => s + Number(f[x.campo]), 0);
  return (
    <div className="p-2 text-[11.5px] text-black">
      <CabecalhoEmpresa empresa={empresa} titulo="Ficha do Funcionário" numero={f.numero ? `N.º ${f.numero}` : undefined} />
      {seccao("Identificação")}
      <div className="grid grid-cols-2 gap-x-8">
        {campo("Nome", f.nome)}
        {campo("Sexo", f.sexo)}
        {campo("Data de nascimento", f.data_nascimento ? data(f.data_nascimento) : null)}
        {campo("Estado civil", f.estado_civil)}
        {campo("Nacionalidade", f.nacionalidade)}
        {campo("Naturalidade", f.naturalidade)}
        {campo("BI n.º", f.bi_numero)}
        {campo("Emitido em", f.bi_local_emissao)}
        {campo("Validade do BI", f.bi_validade ? data(f.bi_validade) : null)}
        {campo("NIF", f.nif)}
        {campo("N.º Segurança Social", f.inss_numero)}
      </div>
      {seccao("Contactos")}
      <div className="grid grid-cols-2 gap-x-8">
        {campo("Morada", f.morada)}
        {campo("Localidade", f.localidade)}
        {campo("Município", f.municipio)}
        {campo("Província", f.provincia)}
        {campo("Telefone", f.telefone)}
        {campo("Email", f.email)}
      </div>
      {seccao("Contrato")}
      <div className="grid grid-cols-2 gap-x-8">
        {campo("Departamento", f.departamento)}
        {campo("Função", f.funcao)}
        {campo("Categoria", f.categoria)}
        {campo("Tipo de contrato", f.tipo_contrato)}
        {campo("Admissão", f.data_admissao ? data(f.data_admissao) : null)}
        {campo("Saída", f.data_saida ? `${data(f.data_saida)}${f.motivo_saida ? ` (${f.motivo_saida})` : ""}` : null)}
        {campo("Local de trabalho", f.local_trabalho)}
        {campo("Situação", f.ativo ? "Activo" : "Inactivo")}
      </div>
      {seccao("Remuneração")}
      <div className="grid grid-cols-2 gap-x-8">
        {f.independente ? campo("Honorário", `${kz(f.honorario)} (${f.tipo_honorario === "DIA" ? "por dia" : f.tipo_honorario === "HORA" ? "por hora" : "mensal"})`) : campo("Salário base", kz(f.salario_base))}
        {subsidios.map((x) => campo(x.label, kz(Number(f[x.campo]))))}
        {!f.independente && campo("Total fixo mensal", kz(totalFixo))}
        {campo("Desconta INSS / IRT", `${f.desconta_inss ? "Sim" : "Não"} / ${f.desconta_irt ? "Sim" : "Não"}`)}
        {campo("Banco", f.banco)}
        {campo("IBAN", f.iban)}
      </div>
      {!!f.agregado?.length && (
        <>
          {seccao("Agregado familiar")}
          <table className="w-full border-collapse text-left">
            <thead>
              <tr className="border-b border-black">
                <th className="py-0.5">Nome</th>
                <th className="py-0.5">Parentesco</th>
                <th className="py-0.5">Data de nascimento</th>
              </tr>
            </thead>
            <tbody>
              {f.agregado.map((a, i) => (
                <tr key={i} className="border-b border-dotted border-gray-300">
                  <td className="py-0.5">{a.nome}</td>
                  <td className="py-0.5">{a.parentesco ?? "—"}</td>
                  <td className="py-0.5">{a.data_nascimento ? data(a.data_nascimento) : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      <div className="mt-14 grid grid-cols-2 gap-16 text-center">
        <div className="border-t border-black pt-1">O Funcionário</div>
        <div className="border-t border-black pt-1">A Entidade Empregadora</div>
      </div>
    </div>
  );
}

interface AgregadoLinha {
  numero: string | null;
  funcionario: string;
  ativo: number;
  nome: string;
  parentesco: string | null;
  data_nascimento: string | null;
}

/** Idade em anos completos numa data AAAA-MM-DD. */
function idade(nasc: string | null) {
  if (!nasc) return null;
  const n = new Date(nasc + "T12:00:00");
  const h = new Date();
  let a = h.getFullYear() - n.getFullYear();
  if (h.getMonth() < n.getMonth() || (h.getMonth() === n.getMonth() && h.getDate() < n.getDate())) a--;
  return a >= 0 ? a : null;
}

/** Lista de agregados familiares (ListaAgregadosFamiliar do KIGEST), com CSV e impressão. */
function AgregadosModal({ linhas, empresa, onClose }: { linhas: AgregadoLinha[]; empresa: ReturnType<typeof useEmpresa>; onClose: () => void }) {
  const [imprimindo, setImprimindo] = useState(false);
  const tabela = (
    <table className="w-full border-collapse text-left">
      <thead>
        <tr className="border-b border-line-strong text-[12px] uppercase tracking-wide text-ink-3">
          <th className="py-1.5 font-medium">Funcionário</th>
          <th className="py-1.5 font-medium">Nome</th>
          <th className="py-1.5 font-medium">Parentesco</th>
          <th className="py-1.5 font-medium">Nascimento</th>
          <th className="py-1.5 text-right font-medium">Idade</th>
        </tr>
      </thead>
      <tbody>
        {linhas.map((l, i) => (
          <tr key={i} className="border-b border-line">
            <td className="py-1.5">
              {l.funcionario}
              {!l.ativo ? " (inactivo)" : ""}
            </td>
            <td className="py-1.5 font-medium">{l.nome}</td>
            <td className="py-1.5">{l.parentesco ?? "—"}</td>
            <td className="py-1.5">{l.data_nascimento ? data(l.data_nascimento) : "—"}</td>
            <td className="py-1.5 text-right">{idade(l.data_nascimento) ?? "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
  return (
    <Modal
      open
      onClose={onClose}
      title="Agregado familiar dos funcionários"
      width="max-w-4xl"
      footer={
        <>
          <Button
            variant="secondary"
            disabled={!linhas.length}
            onClick={() =>
              exportarFicheiro(
                "agregados-familiares.csv",
                csv([["Funcionário", "Nome", "Parentesco", "Data de nascimento", "Idade"], ...linhas.map((l) => [l.funcionario, l.nome, l.parentesco ?? "", l.data_nascimento ?? "", idade(l.data_nascimento) ?? ""])]),
              )
            }
          >
            <FileSpreadsheet className="size-4" /> CSV
          </Button>
          <Button
            disabled={!linhas.length}
            onClick={() => {
              setImprimindo(true);
              imprimir();
              setTimeout(() => setImprimindo(false), 4000);
            }}
          >
            <Printer className="size-4" /> Imprimir
          </Button>
        </>
      }
    >
      {linhas.length ? tabela : <p className="py-8 text-center text-ink-3">Nenhum funcionário tem agregado familiar registado.</p>}
      {imprimindo && (
        <PrintPortal>
          <div className="p-2 text-[11px] text-black">
            <CabecalhoEmpresa empresa={empresa} titulo="Agregado Familiar" />
            <div className="mt-3">{tabela}</div>
            <div className="mt-2 font-bold">{linhas.length} pessoas</div>
          </div>
        </PrintPortal>
      )}
    </Modal>
  );
}

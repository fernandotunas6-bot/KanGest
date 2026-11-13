import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  ArrowLeft,
  Banknote,
  CreditCard,
  FileStack,
  FolderOpen,
  Boxes,
  Hash,
  Inbox,
  Keyboard,
  Lock,
  LogOut,
  Maximize2,
  Minimize2,
  Minus,
  MonitorSmartphone,
  PackageSearch,
  PauseCircle,
  Pencil,
  Percent,
  Plus,
  ScanBarcode,
  Search,
  Tag,
  Trash2,
  UserRound,
  Wallet,
  X,
  XCircle,
} from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { data, kz, kzNum, numero, parseKz, qtd } from "@/lib/format";
import type { Artigo, CaixaResumo, DocCompleto, DocResumo, Entidade, Lote, PostoOpcoes } from "@/lib/types";
import { useSession } from "@/store/session";
import { Badge, Button, Field, Input, Modal, cn } from "@/components/ui";
import DocumentoModal, { ImpressaoAutomatica } from "@/components/DocumentoModal";
import { PinModal, Segmento, TecladoModal, TextoModal } from "@/components/PosDialogs";
import { Logo } from "@/components/Logo";
import { bloquearSessao } from "@/components/Sessao";
import { AvisoLicenca } from "@/components/AvisoLicenca";
import { useLicenca } from "@/lib/licenca";
import { useEmpresa } from "@/components/PrintCabecalho";
import { abrirVisorEcra, ecraInteiro, linhasVisorColuna, ouvirVisorPronto, publicarVisor, type EstadoVisor } from "@/lib/equipamento";

interface Linha {
  key: string;
  artigo: Artigo;
  quantidade: number;
  preco: number; // sem IVA (cêntimos)
  desconto: number; // %
  descricao: string | null; // descrição de linha (F9)
  lote?: string | null; // lote escolhido (vazio = o que expira primeiro)
  manual?: boolean; // preço/desconto fixados manualmente ou vindos de proforma/suspenso
}

type DescGlobal = { tipo: "pct" | "valor"; valor: number }; // pct: %, valor: cêntimos
type LinhaGuardada = { artigo_id: number; quantidade: number; preco: number; desconto: number; descricao?: string | null };

const OPCOES_PADRAO: PostoOpcoes = {
  formato: "ticket",
  vias: 1,
  imprimir_auto: false,
  mostrar_opcoes_impressao: true,
  valor_minimo_impressao: 0,
  repetir_linhas: false,
  confirmar_cliente: false,
};

const CONSUMIDOR: Entidade = {
  id: 1, tipo: "C", nome: "Consumidor Final", nif: "999999999", endereco: null, localidade: null, provincia: null,
  telefone: null, email: null, limite_credito: 0, desconto: 0, nivel_preco: 1,
};

const uid = () => crypto.randomUUID();
const precoNivel = (a: Artigo, nivel: number) => [a.preco1, a.preco2, a.preco3, a.preco4][nivel - 1] || a.preco1;
const comIvaArt = (preco: number, a: Artigo) => Math.round(preco * (1 + a.taxa_pct / 100));

/** Mesmas regras de arredondamento do backend (documentos::emitir). */
function calcular(linhas: Linha[], g: DescGlobal) {
  const gp = g.tipo === "pct" ? g.valor : 0;
  const base = linhas.map((l) => {
    const bruto = Math.round(l.quantidade * l.preco);
    const pct = 100 - ((100 - l.desconto) * (100 - gp)) / 100;
    const desc = Math.round((bruto * pct) / 100);
    return { bruto, desc, liquido: bruto - desc };
  });
  if (g.tipo === "valor" && g.valor > 0 && base.length) {
    const tot = base.reduce((s, b) => s + b.liquido, 0);
    const valor = Math.min(g.valor, tot);
    let restante = valor;
    base.forEach((b, i) => {
      const quota = i === base.length - 1 ? restante : Math.round((valor * b.liquido) / (tot || 1));
      const d = Math.min(quota, b.liquido, restante);
      restante -= d;
      b.liquido -= d;
      b.desc += d;
    });
  }
  const calc = base.map((b, i) => {
    const iva = Math.round((b.liquido * linhas[i].artigo.taxa_pct) / 100);
    return { ...b, iva, total: b.liquido + iva };
  });
  const t = calc.reduce(
    (s, c, i) => ({ bruto: s.bruto + c.bruto, desc: s.desc + c.desc, iva: s.iva + c.iva, total: s.total + c.total, itens: s.itens + linhas[i].quantidade }),
    { bruto: 0, desc: 0, iva: 0, total: 0, itens: 0 },
  );
  const maxDesc = calc.reduce((m, c) => Math.max(m, c.bruto ? (c.desc * 100) / c.bruto : 0), 0);
  return { linhas: calc, ...t, maxDesc };
}

type Teclado = { modo: "qtd" | "preco" | "desc" | "global" } | null;
type PedidoPin = { permissao: string; descricao: string; desconto?: number; acao: () => void } | null;

export default function Pos() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const { sessao, pode, tem, setSessao } = useSession();
  const tabelas = useTabelas();
  const opcoes: PostoOpcoes = { ...OPCOES_PADRAO, ...(tabelas.data?.posto?.opcoes ?? {}) };
  const termica = !!opcoes.escpos?.ativo;

  const [busca, setBusca] = useState("");
  const [familia, setFamilia] = useState<number | null>(null);
  const [linhas, setLinhas] = useState<Linha[]>([]);
  const [sel, setSel] = useState<number | null>(null);
  const [cliente, setCliente] = useState<Entidade>(CONSUMIDOR);
  const [descGlobal, setDescGlobal] = useState<DescGlobal>({ tipo: "pct", valor: 0 });
  const [origem, setOrigem] = useState<{ id: number; numero: string } | null>(null);
  const [pin, setPin] = useState<string | null>(null);

  const [pagar, setPagar] = useState(false);
  const licenca = useLicenca();
  const [escolherCliente, setEscolherCliente] = useState<"" | "lista" | "novo">("");
  const [editarCliente, setEditarCliente] = useState(false);
  const [confirmarCliente, setConfirmarCliente] = useState(false);
  const [teclado, setTeclado] = useState<Teclado>(null);
  const [descricaoLinha, setDescricaoLinha] = useState(false);
  const [escolherLote, setEscolherLote] = useState(false);
  const [suspender, setSuspender] = useState(false);
  const [verSuspensos, setVerSuspensos] = useState(false);
  const [verProformas, setVerProformas] = useState(false);
  const [pedidoPin, setPedidoPin] = useState<PedidoPin>(null);
  const [docEmitido, setDocEmitido] = useState<number | null>(null);
  const [impressaoAuto, setImpressaoAuto] = useState<number | null>(null);
  const buscaRef = useRef<HTMLInputElement>(null);
  const q = useDebounce(busca, 200);
  // equipamento: venda acabada mostrada no visor, ecrã inteiro e teclado no ecrã (terminais tácteis)
  const [fimVenda, setFimVenda] = useState<{ documento: string; total: number; pago: number; troco: number } | null>(null);
  const [cheio, setCheio] = useState(false);
  const [tecladoEcra, setTecladoEcra] = useState(false);
  const empresa = useEmpresa();

  const caixa = useQuery({ queryKey: ["caixa"], queryFn: () => call<CaixaResumo | null>("caixa.estado") });
  const artigos = useQuery({
    queryKey: ["pos-artigos", q, familia],
    queryFn: () => call<Artigo[]>("artigos.listar", { q, familia_id: familia, limite: 120 }),
  });

  const focar = () => setTimeout(() => buscaRef.current?.focus(), 30);
  const totais = useMemo(() => calcular(linhas, descGlobal), [linhas, descGlobal]);
  const idx = sel ?? (linhas.length ? linhas.length - 1 : null); // linha activa (seleccionada ou a última)
  const linhaAtiva = idx !== null ? linhas[idx] : null;
  const algumModal = pagar || !!escolherCliente || editarCliente || confirmarCliente || !!teclado || descricaoLinha || escolherLote || suspender || verSuspensos || verProformas || !!pedidoPin || !!docEmitido;

  /** Executa a acção se o operador tiver permissão (e limite de desconto); caso contrário pede o PIN de um supervisor. */
  function comPermissao(permissao: string, descricao: string, acao: () => void, desconto?: number) {
    const limite = sessao?.limite_desconto ?? 0;
    const limiteOk = desconto === undefined || limite <= 0 || desconto <= limite + 1e-9;
    if (tem(permissao) && limiteOk) return acao();
    setPedidoPin({ permissao, descricao, desconto, acao });
  }

  const adicionar = useCallback(
    (a: Artigo, quantidade = 1) => {
      setLinhas((ls) => {
        const i = opcoes.repetir_linhas ? -1 : ls.findIndex((l) => l.artigo.id === a.id && !l.manual);
        if (i >= 0) {
          const n = [...ls];
          n[i] = { ...n[i], quantidade: +(n[i].quantidade + quantidade).toFixed(3) };
          setSel(i);
          return n;
        }
        setSel(ls.length);
        return [...ls, { key: uid(), artigo: a, quantidade, preco: precoNivel(a, cliente.nivel_preco), desconto: Math.max(cliente.desconto || 0, a.promo_atual || 0), descricao: null }];
      });
    },
    [cliente, opcoes.repetir_linhas],
  );

  // Enter no campo de pesquisa: leitor de código de barras / referência. Suporta "3*codigo".
  async function onEnter() {
    const txt = busca.trim();
    if (!txt) return;
    const m = txt.match(/^(\d+(?:[.,]\d+)?)\*(.+)$/);
    const quantidade = m ? Number(m[1].replace(",", ".")) : 1;
    const codigo = m ? m[2] : txt;
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo });
      if (a) {
        adicionar(a, quantidade);
        setBusca("");
      } else if (artigos.data?.length === 1) {
        adicionar(artigos.data[0], quantidade);
        setBusca("");
      } else {
        toast.warning(`Artigo "${codigo}" não encontrado.`);
      }
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  function mudarCliente(c: Entidade) {
    setCliente(c);
    // reaplica nível de preço e desconto do cliente às linhas não alteradas manualmente
    setLinhas((ls) => ls.map((l) => (l.manual ? l : { ...l, preco: precoNivel(l.artigo, c.nivel_preco), desconto: Math.max(c.desconto || 0, l.artigo.promo_atual || 0) })));
  }

  function limpar() {
    setLinhas([]);
    setSel(null);
    setCliente(CONSUMIDOR);
    setDescGlobal({ tipo: "pct", valor: 0 });
    setOrigem(null);
    setPin(null);
    focar();
  }

  // ---------------- operações sobre linhas ----------------
  const atualizar = (i: number, patch: Partial<Linha>) => setLinhas((ls) => ls.map((l, j) => (j === i ? { ...l, ...patch } : l)));

  function apagarLinha(i: number | null = idx) {
    if (i === null || !linhas[i]) return;
    comPermissao("pos.eliminar_linha", "Apagar linha", () => {
      setLinhas((ls) => ls.filter((_, j) => j !== i));
      setSel(null);
      focar();
    });
  }

  function abrirTeclado(modo: NonNullable<Teclado>["modo"]) {
    if (modo !== "global" && !linhaAtiva) return toast.info("Adicione ou seleccione uma linha.");
    if (modo === "global" && !linhas.length) return;
    setTeclado({ modo });
  }

  function aplicarTeclado(valor: number) {
    const modo = teclado?.modo;
    const i = idx;
    setTeclado(null);
    if (!modo) return;
    if (modo === "qtd" && i !== null) {
      if (valor <= 0) return toast.error("A quantidade tem de ser positiva.");
      comPermissao("pos.quantidade", "Alterar quantidade", () => atualizar(i, { quantidade: valor }));
    } else if (modo === "preco" && i !== null) {
      const l = linhas[i];
      const preco = Math.round((valor * 100) / (1 + l.artigo.taxa_pct / 100));
      comPermissao("pos.alterar_preco", "Alterar preço", () => atualizar(i, { preco, manual: true }));
    } else if (modo === "desc" && i !== null) {
      if (valor > 100) return toast.error("Desconto inválido.");
      comPermissao("pos.desconto", `Desconto de ${valor}%`, () => atualizar(i, { desconto: valor, manual: true }), valor);
    } else if (modo === "global") {
      const g: DescGlobal = descGlobal.tipo === "pct" ? { tipo: "pct", valor } : { tipo: "valor", valor: Math.round(valor * 100) };
      if (g.tipo === "pct" && g.valor > 100) return toast.error("Desconto inválido.");
      const efetivo = calcular(linhas, g).maxDesc;
      comPermissao("pos.desconto", "Desconto global", () => setDescGlobal(g), efetivo);
    }
    focar();
  }

  // ---------------- suspensos e proformas ----------------
  async function carregarLinhas(origemLinhas: LinhaGuardada[]) {
    const arts = await call<Artigo[]>("artigos.porIds", { ids: origemLinhas.map((l) => l.artigo_id) });
    const mapa = new Map(arts.map((a) => [a.id, a]));
    const em_falta = origemLinhas.filter((l) => !mapa.has(l.artigo_id)).length;
    if (em_falta) toast.warning(`${em_falta} artigo(s) já não existem e foram ignorados.`);
    setLinhas(
      origemLinhas
        .filter((l) => mapa.has(l.artigo_id))
        .map((l) => ({ key: uid(), artigo: mapa.get(l.artigo_id)!, quantidade: l.quantidade, preco: l.preco, desconto: l.desconto, descricao: l.descricao ?? null, manual: true })),
    );
    setSel(null);
  }

  async function carregarCliente(id: number | null | undefined) {
    setCliente(id && id !== 1 ? await call<Entidade>("entidades.obter", { id }) : CONSUMIDOR);
  }

  async function gravarSuspenso(descricao: string) {
    try {
      await call("suspensos.guardar", {
        entidade_id: cliente.id,
        entidade_nome: cliente.nome,
        descricao,
        total: totais.total,
        dados: {
          linhas: linhas.map((l) => ({ artigo_id: l.artigo.id, quantidade: l.quantidade, preco: l.preco, desconto: l.desconto, descricao: l.descricao })),
          desc_global: descGlobal,
          origem,
        },
      });
      toast.success("Documento suspenso.");
      setSuspender(false);
      limpar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function abrirSuspenso(id: number) {
    if (linhas.length) return toast.error("Termine ou suspenda a venda actual primeiro.");
    try {
      const s = await call<{
        entidade_id: number | null;
        dados: { linhas: LinhaGuardada[]; desc_global?: DescGlobal; origem?: { id: number; numero: string } | null };
      }>("suspensos.abrir", { id });
      await carregarLinhas(s.dados.linhas);
      setDescGlobal(s.dados.desc_global ?? { tipo: "pct", valor: 0 });
      setOrigem(s.dados.origem ?? null);
      await carregarCliente(s.entidade_id);
      setVerSuspensos(false);
      focar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function abrirProforma(id: number) {
    if (linhas.length) return toast.error("Termine ou suspenda a venda actual primeiro.");
    try {
      const d = await call<DocCompleto>("documentos.obter", { id });
      await carregarLinhas(
        d.linhas.map((l) => ({ artigo_id: l.artigo_id, quantidade: l.quantidade, preco: l.preco_unitario, desconto: l.desconto_pct, descricao: l.descricao })),
      );
      await carregarCliente(d.documento.entidade_id);
      setOrigem({ id: d.documento.id, numero: d.documento.numero_doc });
      setVerProformas(false);
      focar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  // ---------------- pagamento ----------------
  function abrirPagamento() {
    if (!linhas.length) return;
    if (caixa.isSuccess && !caixa.data) return toast.error("Abra o caixa antes de vender.");
    if (licenca.data && !licenca.data.pode_emitir) return toast.error(licenca.data.mensagem);
    if (opcoes.confirmar_cliente && cliente.id === 1) return setConfirmarCliente(true);
    setPagar(true);
  }

  function aposEmissao(d: DocCompleto) {
    setPagar(false);
    limpar();
    // o visor mostra o total, o valor recebido e o troco durante uns segundos
    setFimVenda({ documento: d.documento.numero_doc, total: d.documento.total, pago: d.documento.valor_entregue || d.documento.total, troco: d.documento.troco });
    qc.invalidateQueries({ queryKey: ["caixa"] });
    qc.invalidateQueries({ queryKey: ["pos-artigos"] });
    const id = d.documento.id;
    const imprime = opcoes.imprimir_auto && d.documento.total >= (opcoes.valor_minimo_impressao || 0);
    if (opcoes.mostrar_opcoes_impressao) {
      setDocEmitido(id);
    } else if (termica) {
      // ESC/POS: o talão (e a gaveta) saem directamente, sem diálogo do sistema
      const chamada = imprime ? call("impressao.talao", { id, vias: opcoes.vias }) : call("impressao.gaveta", { documento_id: id });
      chamada.catch((e) => toast.error((e as Error).message));
    } else if (imprime) {
      setImpressaoAuto(id);
    }
  }

  async function abrirGaveta() {
    try {
      await call("impressao.gaveta");
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  // ---------------- equipamento: visor de cliente e ecrã inteiro ----------------
  const modoVisor = opcoes.visor?.modo ?? "nenhum";
  const estadoVisor: EstadoVisor = useMemo(() => {
    const base = {
      empresa: { nome: empresa?.designacao_comercial || empresa?.nome || "", logo: empresa?.logo ?? null },
      mensagem1: opcoes.visor?.mensagem1 ?? "",
      mensagem2: opcoes.visor?.mensagem2 ?? "",
    };
    if (!linhas.length) {
      return fimVenda
        ? { ...base, fase: "fim", linhas: [], ultima: null, total: fimVenda.total, itens: 0, desconto: 0, pago: fimVenda.pago, troco: fimVenda.troco, documento: fimVenda.documento }
        : { ...base, fase: "livre", linhas: [], ultima: null, total: 0, itens: 0, desconto: 0 };
    }
    const ls = totais.linhas.map((c, i) => ({ descricao: linhas[i].descricao || linhas[i].artigo.descricao, quantidade: linhas[i].quantidade, total: c.total }));
    // desconto como o cliente o vê: com IVA
    const desconto = totais.linhas.reduce((s, c, i) => s + Math.round(c.desc * (1 + linhas[i].artigo.taxa_pct / 100)), 0);
    const ativa = Math.min(idx ?? ls.length - 1, ls.length - 1);
    return { ...base, fase: pagar ? "pagar" : "venda", linhas: ls, ultima: ls[ativa] ?? null, ativa, total: totais.total, itens: totais.itens, desconto };
  }, [empresa, opcoes.visor?.mensagem1, opcoes.visor?.mensagem2, linhas, totais, idx, pagar, fimVenda]);
  const estadoVisorRef = useRef(estadoVisor);
  estadoVisorRef.current = estadoVisor;

  // a venda acabada fica 10 s no visor; uma venda nova substitui-a logo
  useEffect(() => {
    if (!fimVenda) return;
    if (linhas.length) return setFimVenda(null);
    const t = setTimeout(() => setFimVenda(null), 10_000);
    return () => clearTimeout(t);
  }, [fimVenda, linhas.length]);

  // visor de coluna (porta série): só envia quando as linhas mudam; um aviso por sessão se falhar
  const visorEnviado = useRef("");
  const visorAvisado = useRef(false);
  useEffect(() => {
    if (modoVisor === "ecra") {
      publicarVisor(estadoVisor);
      return;
    }
    if (modoVisor !== "serie") return;
    const [linha1, linha2] = linhasVisorColuna(estadoVisor, opcoes.visor?.colunas ?? 20);
    const chave = `${linha1}\n${linha2}`;
    if (chave === visorEnviado.current) return;
    const t = setTimeout(() => {
      visorEnviado.current = chave;
      call("impressao.visor", { linha1, linha2 }).catch((e) => {
        if (!visorAvisado.current) toast.error((e as Error).message);
        visorAvisado.current = true;
      });
    }, 120);
    return () => clearTimeout(t);
  }, [estadoVisor, modoVisor, opcoes.visor?.colunas]);

  // segundo ecrã: abre ao entrar no POS e responde quando a janela do cliente pede o estado
  useEffect(() => {
    if (modoVisor !== "ecra") return;
    abrirVisorEcra().catch((e) => toast.error(`Ecrã do cliente: ${(e as Error).message}`));
    const parar = ouvirVisorPronto(() => publicarVisor(estadoVisorRef.current));
    return () => {
      parar();
      publicarVisor({ ...estadoVisorRef.current, fase: "livre", linhas: [], ultima: null, total: 0, itens: 0, desconto: 0 });
    };
  }, [modoVisor]);

  // ecrã inteiro automático nos terminais dedicados (e sai dele ao deixar o POS)
  useEffect(() => {
    if (!opcoes.pos_ecra_inteiro) return;
    ecraInteiro(true).then(setCheio).catch(() => {});
    return () => {
      ecraInteiro(false).catch(() => {});
    };
  }, [opcoes.pos_ecra_inteiro]);

  async function alternarEcraInteiro() {
    try {
      setCheio(await ecraInteiro());
    } catch (e) {
      toast.error((e as Error).message);
    }
    focar();
  }

  // ---------------- atalhos (iguais ao KIGEST POS) ----------------
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (algumModal) return;
      // Leitor de código de barras (teclado USB) ou teclado com o foco fora dos campos: o que se escreve
      // vai para a pesquisa, para não se perder uma leitura depois de tocar numa linha ou num botão.
      const alvo = e.target as HTMLElement | null;
      const emCampo = !!alvo && (["INPUT", "TEXTAREA", "SELECT"].includes(alvo.tagName) || alvo.isContentEditable);
      if (!emCampo && e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && buscaRef.current) {
        e.preventDefault();
        const inp = buscaRef.current;
        const v = inp.value + e.key;
        inp.focus();
        inp.value = v;
        inp.setSelectionRange(v.length, v.length);
        setBusca(v);
        return;
      }
      if (!emCampo && e.key === "Enter" && buscaRef.current?.value.trim()) {
        e.preventDefault();
        onEnter();
        return;
      }
      const acoes: Record<string, () => void> = {
        F2: () => buscaRef.current?.focus(),
        F3: () => abrirTeclado("qtd"),
        F4: () => setEscolherCliente("lista"),
        F5: () => (cliente.id === 1 ? setEscolherCliente("novo") : setEditarCliente(true)),
        F6: () => abrirTeclado("desc"),
        F7: () => abrirTeclado("preco"),
        F8: () => abrirTeclado("global"),
        F9: () => linhaAtiva && setDescricaoLinha(true),
        F10: () => linhas.length && (tem("pos.suspenso") ? setSuspender(true) : toast.error("Sem permissão para suspender documentos.")),
        F11: () => (tem("pos.suspenso") ? setVerSuspensos(true) : toast.error("Sem permissão para documentos suspensos.")),
        F12: () => abrirPagamento(),
      };
      if (acoes[e.key]) {
        e.preventDefault();
        acoes[e.key]();
        return;
      }
      if (termica && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "g") {
        e.preventDefault();
        abrirGaveta();
        return;
      }
      const naPesquisa = document.activeElement === buscaRef.current;
      if (e.key === "Delete" && (!naPesquisa || !busca)) {
        e.preventDefault();
        apagarLinha();
      } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && naPesquisa && !busca && linhas.length) {
        e.preventDefault();
        const atual = idx ?? 0;
        setSel(Math.max(0, Math.min(linhas.length - 1, atual + (e.key === "ArrowDown" ? 1 : -1))));
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  });

  const caixaFechado = caixa.isSuccess && !caixa.data;

  async function sair() {
    await call("auth.logout");
    qc.clear();
    setSessao(null);
  }

  const teclas: { k: string; label: string; icon: ReactNode; on: () => void; tom?: string }[] = [
    { k: "F3", label: "Quantidade", icon: <Hash className="size-4" />, on: () => abrirTeclado("qtd") },
    { k: "F6", label: "Desc. linha", icon: <Percent className="size-4" />, on: () => abrirTeclado("desc") },
    { k: "F7", label: "Preço", icon: <Tag className="size-4" />, on: () => abrirTeclado("preco") },
    { k: "F8", label: "Desc. global", icon: <Percent className="size-4" />, on: () => abrirTeclado("global") },
    { k: "F9", label: "Descrição", icon: <Pencil className="size-4" />, on: () => linhaAtiva && setDescricaoLinha(true) },
    { k: "", label: "Lote", icon: <Boxes className="size-4" />, on: () => (linhaAtiva?.artigo.controla_lote ? setEscolherLote(true) : toast.info("Este artigo não é controlado por lotes.")) },
    { k: "Del", label: "Apagar linha", icon: <Trash2 className="size-4" />, on: () => apagarLinha() },
    { k: "F10", label: "Suspender", icon: <PauseCircle className="size-4" />, on: () => linhas.length && setSuspender(true) },
    { k: "F11", label: "Suspensos", icon: <FolderOpen className="size-4" />, on: () => setVerSuspensos(true) },
    { k: "", label: "Notas / Proformas", icon: <FileStack className="size-4" />, on: () => setVerProformas(true) },
  ];
  if (termica) {
    teclas.push({ k: "Ctrl+G", label: "Gaveta", icon: <Inbox className="size-4" />, on: abrirGaveta });
  }

  return (
    <div className="flex h-full flex-col bg-bg">
      {/* Barra superior */}
      <header className="flex h-14 shrink-0 items-center gap-4 border-b border-line bg-surface px-4">
        {pode("GESTOR") ? (
          <Button variant="ghost" size="sm" onClick={() => nav("/")}>
            <ArrowLeft className="size-4" /> Gestão
          </Button>
        ) : (
          <Button variant="ghost" size="sm" onClick={() => nav("/documentos")}>
            Documentos
          </Button>
        )}
        <div className="flex items-center gap-2">
          <Logo className="size-8" />
          <span className="font-semibold">Ponto de Venda</span>
          <Badge tone="neutral">{tabelas.data?.posto?.descricao ?? "Posto"}</Badge>
        </div>
        <div className="ml-auto flex items-center gap-3">
          {caixa.data ? (
            <Badge tone="ok">
              <Wallet className="mr-1 size-3.5" /> Caixa aberto · {caixa.data.documentos} vendas
            </Badge>
          ) : (
            <Badge tone="danger">Caixa fechado</Badge>
          )}
          <Button variant="secondary" size="sm" onClick={() => nav("/caixa")}>
            Caixa
          </Button>
          {modoVisor === "ecra" && (
            <Button
              variant="ghost"
              size="sm"
              title="Abrir o ecrã do cliente"
              onClick={() => abrirVisorEcra().then(() => publicarVisor(estadoVisorRef.current)).catch((e) => toast.error((e as Error).message))}
            >
              <MonitorSmartphone className="size-4" />
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={alternarEcraInteiro} title={cheio ? "Sair do ecrã inteiro" : "Ecrã inteiro"}>
            {cheio ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
          </Button>
          <span className="text-[13px] text-ink-2">{sessao?.nome}</span>
          <Button variant="ghost" size="sm" onClick={bloquearSessao} title="Bloquear o posto (retoma-se com a senha ou o PIN; a venda fica como está)">
            <Lock className="size-4" />
          </Button>
          <Button variant="ghost" size="sm" onClick={sair} title="Terminar sessão">
            <LogOut className="size-4" />
          </Button>
        </div>
      </header>
      <AvisoLicenca compacto className="mx-4 mt-3" />

      <div className="flex min-h-0 flex-1">
        {/* Catálogo */}
        <section className="flex min-w-0 flex-1 flex-col p-4">
          <div className="relative">
            <ScanBarcode className="absolute left-4 top-1/2 size-5 -translate-y-1/2 text-ink-3" />
            <input
              ref={buscaRef}
              autoFocus
              value={busca}
              onChange={(e) => setBusca(e.target.value)}
              onKeyDown={(e) => {
                // os leitores terminam a leitura com Enter ou, alguns, com Tab
                if (e.key === "Enter" || (e.key === "Tab" && busca.trim())) {
                  e.preventDefault();
                  onEnter();
                }
              }}
              placeholder="Ler código de barras ou pesquisar artigo…"
              className="h-14 w-full rounded-xl border border-line-strong bg-surface pl-12 pr-14 text-[16px] shadow-card outline-none focus:border-brand focus:ring-4 focus:ring-brand/15"
            />
            <button
              type="button"
              onClick={() => {
                setTecladoEcra((x) => !x);
                focar();
              }}
              title="Teclado no ecrã (terminais tácteis)"
              aria-pressed={tecladoEcra}
              className={cn(
                "absolute right-2 top-1/2 grid size-10 -translate-y-1/2 place-items-center rounded-lg transition",
                tecladoEcra ? "bg-brand text-white" : "text-ink-3 hover:bg-sunken hover:text-ink",
              )}
            >
              <Keyboard className="size-5" />
            </button>
          </div>
          {tecladoEcra && (
            <TecladoEcra
              onTecla={(k) => {
                setBusca((b) => (k === "⌫" ? b.slice(0, -1) : k === "Limpar" ? "" : b + (k === "Espaço" ? " " : k)));
                focar();
              }}
              onEnter={() => {
                onEnter();
                focar();
              }}
              onFechar={() => {
                setTecladoEcra(false);
                focar();
              }}
            />
          )}

          <div className="mt-3 flex gap-2 overflow-x-auto pb-1">
            <Chip ativo={familia === null} onClick={() => setFamilia(null)}>
              Todos
            </Chip>
            {tabelas.data?.familias.map((f) => (
              <Chip key={f.id} ativo={familia === f.id} onClick={() => setFamilia(f.id)}>
                {f.descricao}
              </Chip>
            ))}
          </div>

          <div className="mt-3 min-h-0 flex-1 overflow-y-auto pr-1">
            {artigos.data?.length === 0 ? (
              <div className="grid h-full place-items-center text-center text-ink-3">
                <div>
                  <PackageSearch className="mx-auto mb-2 size-8" />
                  Nenhum artigo encontrado.
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-3">
                {artigos.data?.map((a) => {
                  const semStock = a.controla_stock === 1 && a.stock <= 0 && a.permite_negativo !== 1;
                  return (
                    <button
                      key={a.id}
                      onClick={() => {
                        adicionar(a);
                        focar();
                      }}
                      disabled={semStock}
                      className="group flex h-[118px] flex-col rounded-xl border border-line bg-surface p-3 text-left shadow-card transition hover:-translate-y-0.5 hover:border-brand/50 hover:shadow-md disabled:opacity-45 disabled:hover:translate-y-0"
                    >
                      <span className="line-clamp-2 text-[13.5px] font-medium leading-snug">{a.descricao}</span>
                      <span className="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-ink-3">
                        {a.referencia}
                        {!!a.promo_atual && <span className="rounded bg-accent-50 px-1.5 py-px font-medium text-[#9a6514]">−{numero(a.promo_atual, 2)}%</span>}
                      </span>
                      <div className="mt-auto flex items-end justify-between">
                        <span className="num text-[15px] font-semibold text-brand-700">{kzNum(comIvaArt(precoNivel(a, cliente.nivel_preco), a))}</span>
                        {a.controla_stock === 1 && (
                          <span className={cn("num text-[11px]", a.stock <= a.qtd_minima ? "text-danger" : "text-ink-3")}>
                            {qtd(a.stock)} {a.unidade}
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {/* Barra de funções (teclas do KIGEST POS) */}
          <div className="mt-3 grid grid-cols-[repeat(auto-fit,minmax(68px,1fr))] gap-2">
            {teclas.map((t) => (
              <button
                key={t.label}
                onClick={t.on}
                className="flex flex-col items-center justify-center gap-1 rounded-xl border border-line bg-surface px-1 py-2 text-[11.5px] text-ink-2 shadow-card transition hover:border-brand/50 hover:text-ink"
              >
                {t.icon}
                <span className="text-center leading-tight">{t.label}</span>
              </button>
            ))}
          </div>
        </section>

        {/* Carrinho */}
        <aside className="flex w-[420px] shrink-0 flex-col border-l border-line bg-surface">
          <div className="flex items-center gap-3 border-b border-line px-5 py-3.5">
            <button onClick={() => setEscolherCliente("lista")} className="flex min-w-0 flex-1 items-center gap-3 text-left">
              <div className="grid size-9 shrink-0 place-items-center rounded-full bg-brand-50 text-brand">
                <UserRound className="size-[18px]" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{cliente.nome}</div>
                <div className="text-[12px] text-ink-3">NIF {cliente.nif}</div>
              </div>
            </button>
            <button
              onClick={() => (cliente.id === 1 ? setEscolherCliente("novo") : setEditarCliente(true))}
              className="rounded-md p-1.5 text-ink-3 hover:bg-sunken hover:text-ink"
              title="Alterar dados do cliente"
            >
              <Pencil className="size-4" />
            </button>
          </div>

          {(origem || descGlobal.valor > 0 || pin) && (
            <div className="flex flex-wrap gap-1.5 border-b border-line px-5 py-2">
              {origem && (
                <Badge tone="brand">
                  A facturar {origem.numero}
                  <button className="ml-1" onClick={() => setOrigem(null)}>
                    <X className="size-3" />
                  </button>
                </Badge>
              )}
              {descGlobal.valor > 0 && (
                <Badge tone="accent">
                  Desc. global {descGlobal.tipo === "pct" ? `${descGlobal.valor}%` : kz(descGlobal.valor)}
                  <button className="ml-1" onClick={() => setDescGlobal({ tipo: "pct", valor: 0 })}>
                    <X className="size-3" />
                  </button>
                </Badge>
              )}
              {pin && <Badge tone="neutral">Autorizado por supervisor</Badge>}
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-y-auto">
            {linhas.length === 0 ? (
              <div className="grid h-full place-items-center px-8 text-center text-ink-3">
                <div>
                  <Search className="mx-auto mb-2 size-7" />
                  Leia um código de barras ou toque num artigo para começar.
                </div>
              </div>
            ) : (
              <ul>
                {linhas.map((l, i) => {
                  const c = totais.linhas[i];
                  return (
                    <li key={l.key} onClick={() => setSel(i)} className={cn("cursor-pointer border-b border-line/70 px-5 py-3", idx === i && "bg-brand-50/60")}>
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <div className="truncate font-medium">{l.descricao || l.artigo.descricao}</div>
                          <div className="num text-[12px] text-ink-3">
                            {kzNum(comIvaArt(l.preco, l.artigo))} / {l.artigo.unidade ?? "UN"} · IVA {l.artigo.taxa_pct}%
                            {l.desconto ? ` · \u2212${numero(l.desconto, 2)}%` : ""}
                          </div>
                          {!!l.artigo.controla_lote && (
                            <button
                              className="mt-0.5 text-[12px] text-accent hover:underline"
                              onClick={(e) => {
                                e.stopPropagation();
                                setSel(i);
                                setEscolherLote(true);
                              }}
                            >
                              {l.lote ? `Lote ${l.lote}` : "Lote automático (expira primeiro)"}
                            </button>
                          )}
                        </div>
                        <div className="num shrink-0 font-semibold">{kzNum(c?.total)}</div>
                      </div>
                      <div className="mt-2 flex items-center gap-2">
                        <button
                          className="grid size-7 place-items-center rounded-md border border-line hover:bg-sunken"
                          onClick={(e) => {
                            e.stopPropagation();
                            if (l.quantidade > 1) comPermissao("pos.quantidade", "Alterar quantidade", () => atualizar(i, { quantidade: +(l.quantidade - 1).toFixed(3) }));
                          }}
                        >
                          <Minus className="size-3.5" />
                        </button>
                        <button
                          className="num h-7 min-w-14 rounded-md border border-line px-2 text-center hover:bg-sunken"
                          onClick={(e) => {
                            e.stopPropagation();
                            setSel(i);
                            setTeclado({ modo: "qtd" });
                          }}
                        >
                          {qtd(l.quantidade)}
                        </button>
                        <button
                          className="grid size-7 place-items-center rounded-md border border-line hover:bg-sunken"
                          onClick={(e) => {
                            e.stopPropagation();
                            atualizar(i, { quantidade: +(l.quantidade + 1).toFixed(3) });
                          }}
                        >
                          <Plus className="size-3.5" />
                        </button>
                        <button
                          className="ml-auto rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger"
                          onClick={(e) => {
                            e.stopPropagation();
                            apagarLinha(i);
                          }}
                        >
                          <Trash2 className="size-4" />
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          <div className="border-t border-line bg-bg/50 px-5 py-4">
            <div className="space-y-1 text-[13px] text-ink-2">
              <Linha2 k={`Subtotal (${qtd(totais.itens)} ${totais.itens === 1 ? "item" : "itens"})`} v={kz(totais.bruto)} />
              {totais.desc > 0 && <Linha2 k="Descontos" v={`\u2212${kz(totais.desc)}`} />}
              <Linha2 k="IVA" v={kz(totais.iva)} />
            </div>
            <div className="mt-2 flex items-baseline justify-between">
              <span className="text-ink-2">Total</span>
              <span className="num text-[30px] font-semibold tracking-tight">{kz(totais.total)}</span>
            </div>
            <div className="mt-3 grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-2">
              <Button variant="secondary" size="lg" onClick={limpar} disabled={!linhas.length}>
                <XCircle className="size-4" /> Cancelar
              </Button>
              <Button size="lg" variant="accent" className="text-[16px]" onClick={abrirPagamento} disabled={!linhas.length || caixaFechado}>
                <Banknote className="size-5" /> Pagar
              </Button>
            </div>
          </div>
        </aside>
      </div>

      <AbrirCaixa open={caixaFechado} onOpened={() => { caixa.refetch(); focar(); }} />

      <ClienteModal
        modo={escolherCliente}
        onClose={() => { setEscolherCliente(""); focar(); }}
        onSelect={(c) => { mudarCliente(c); setEscolherCliente(""); focar(); }}
      />

      {editarCliente && (
        <EditarClienteModal
          cliente={cliente}
          onClose={() => { setEditarCliente(false); focar(); }}
          onSaved={(c) => { setCliente(c); setEditarCliente(false); focar(); }}
        />
      )}

      <Modal
        open={confirmarCliente}
        onClose={() => setConfirmarCliente(false)}
        title="Confirmar cliente"
        width="max-w-md"
        footer={
          <>
            <Button variant="secondary" onClick={() => { setConfirmarCliente(false); setEscolherCliente("lista"); }}>
              Escolher cliente
            </Button>
            <Button autoFocus onClick={() => { setConfirmarCliente(false); setPagar(true); }}>
              Continuar como Consumidor Final
            </Button>
          </>
        }
      >
        <p className="text-ink-2">A venda está em nome de <b className="text-ink">Consumidor Final</b>. Deseja identificar o cliente (nome e NIF)?</p>
      </Modal>

      <TecladoModal
        open={!!teclado}
        titulo={{ qtd: "Quantidade", preco: "Alterar preço (com IVA)", desc: "Desconto de linha", global: "Desconto global" }[teclado?.modo ?? "qtd"]}
        sufixo={teclado?.modo === "qtd" ? linhaAtiva?.artigo.unidade ?? "" : teclado?.modo === "desc" || (teclado?.modo === "global" && descGlobal.tipo === "pct") ? "%" : "Kz"}
        ajuda={
          teclado?.modo === "preco"
            ? `Preço actual: ${kz(linhaAtiva ? comIvaArt(linhaAtiva.preco, linhaAtiva.artigo) : 0)}`
            : teclado?.modo === "desc" || teclado?.modo === "global"
              ? sessao?.limite_desconto
                ? `O seu limite de desconto é ${sessao.limite_desconto}%.`
                : undefined
              : undefined
        }
        valorInicial={
          !teclado || !linhaAtiva
            ? descGlobal.tipo === "pct"
              ? String(descGlobal.valor || "")
              : String(descGlobal.valor / 100 || "")
            : teclado.modo === "qtd"
              ? String(linhaAtiva.quantidade).replace(".", ",")
              : teclado.modo === "preco"
                ? (comIvaArt(linhaAtiva.preco, linhaAtiva.artigo) / 100).toFixed(2).replace(".", ",")
                : teclado.modo === "desc"
                  ? String(linhaAtiva.desconto || "")
                  : descGlobal.tipo === "pct"
                    ? String(descGlobal.valor || "")
                    : String(descGlobal.valor / 100 || "")
        }
        extra={
          teclado?.modo === "global" ? (
            <Segmento
              valor={descGlobal.tipo}
              opcoes={[
                { id: "pct", label: "Percentagem (%)" },
                { id: "valor", label: "Numerário (Kz)" },
              ]}
              onChange={(t) => setDescGlobal({ tipo: t, valor: 0 })}
            />
          ) : undefined
        }
        onConfirm={aplicarTeclado}
        onClose={() => { setTeclado(null); focar(); }}
      />

      {escolherLote && linhaAtiva && (
        <LoteModal
          artigo={linhaAtiva.artigo}
          armazem={tabelas.data?.armazem_id}
          quantidade={linhaAtiva.quantidade}
          atual={linhaAtiva.lote ?? null}
          onPick={(lote) => {
            if (idx !== null) atualizar(idx, { lote });
            setEscolherLote(false);
            focar();
          }}
          onClose={() => {
            setEscolherLote(false);
            focar();
          }}
        />
      )}

      <TextoModal
        open={descricaoLinha}
        titulo="Descrição da linha"
        label="Descrição impressa no documento"
        valorInicial={linhaAtiva?.descricao || linhaAtiva?.artigo.descricao || ""}
        onConfirm={(v) => {
          if (idx !== null) atualizar(idx, { descricao: v.trim() || null });
          setDescricaoLinha(false);
          focar();
        }}
        onClose={() => { setDescricaoLinha(false); focar(); }}
      />

      <TextoModal
        open={suspender}
        titulo="Suspender documento"
        label="Identificação (cliente, mesa, observação)"
        valorInicial={cliente.id === 1 ? "" : cliente.nome}
        onConfirm={gravarSuspenso}
        onClose={() => { setSuspender(false); focar(); }}
      />

      {verSuspensos && <SuspensosModal onClose={() => { setVerSuspensos(false); focar(); }} onAbrir={abrirSuspenso} />}
      {verProformas && <ProformasModal onClose={() => { setVerProformas(false); focar(); }} onAbrir={abrirProforma} />}

      <PinModal
        pedido={pedidoPin}
        onClose={() => { setPedidoPin(null); focar(); }}
        onOk={(p) => {
          if (pedidoPin?.permissao === "pos.desconto" || pedidoPin?.permissao === "pos.alterar_preco") setPin(p);
          pedidoPin?.acao();
          setPedidoPin(null);
          focar();
        }}
      />

      {pagar && (
        <Pagamento
          total={totais.total}
          cliente={cliente}
          linhas={linhas}
          descGlobal={descGlobal}
          origemId={origem?.id ?? null}
          pin={pin}
          modos={tabelas.data?.modos_pagamento ?? []}
          onClose={() => { setPagar(false); focar(); }}
          onEmitido={aposEmissao}
        />
      )}

      <DocumentoModal id={docEmitido} formatoInicial={opcoes.formato} onClose={() => { setDocEmitido(null); focar(); }} />
      {impressaoAuto && (
        <ImpressaoAutomatica id={impressaoAuto} formato={opcoes.formato} vias={opcoes.vias} onDone={() => setImpressaoAuto(null)} />
      )}
    </div>
  );
}

/** Teclado no ecrã para pesquisar artigos em terminais só tácteis (sem teclado físico). */
function TecladoEcra({ onTecla, onEnter, onFechar }: { onTecla: (k: string) => void; onEnter: () => void; onFechar: () => void }) {
  const linhas = ["1234567890", "QWERTYUIOP", "ASDFGHJKLÇ", "ZXCVBNM-*."];
  const tecla = "grid h-11 min-w-0 place-items-center rounded-lg border border-line bg-surface text-[15px] font-medium shadow-card active:scale-95 active:bg-brand-50";
  return (
    <div className="mt-2 rounded-xl border border-line bg-sunken p-2">
      {linhas.map((l) => (
        <div key={l} className="mb-1.5 grid grid-cols-10 gap-1.5">
          {l.split("").map((k) => (
            <button key={k} type="button" className={tecla} onClick={() => onTecla(k)}>
              {k}
            </button>
          ))}
        </div>
      ))}
      <div className="grid grid-cols-[1fr_4fr_1.5fr_2fr_1fr] gap-1.5">
        <button type="button" className={tecla} onClick={() => onTecla("Limpar")}>
          Limpar
        </button>
        <button type="button" className={tecla} onClick={() => onTecla("Espaço")}>
          Espaço
        </button>
        <button type="button" className={tecla} onClick={() => onTecla("⌫")} aria-label="Apagar">
          ⌫
        </button>
        <button type="button" className={cn(tecla, "border-brand bg-brand text-white active:bg-brand-700")} onClick={onEnter}>
          Adicionar
        </button>
        <button type="button" className={tecla} onClick={onFechar} aria-label="Fechar o teclado">
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}

function Linha2({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <span>{k}</span>
      <span className="num">{v}</span>
    </div>
  );
}

function Chip({ ativo, onClick, children }: { ativo: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "shrink-0 rounded-full border px-4 py-1.5 text-[13px] transition",
        ativo ? "border-brand bg-brand text-white" : "border-line-strong bg-surface text-ink-2 hover:border-brand/50",
      )}
    >
      {children}
    </button>
  );
}

// ------------------------------------------------------------------ Pagamento
function Pagamento({
  total,
  cliente,
  linhas,
  descGlobal,
  origemId,
  pin,
  modos,
  onClose,
  onEmitido,
}: {
  total: number;
  cliente: Entidade;
  linhas: Linha[];
  descGlobal: DescGlobal;
  origemId: number | null;
  pin: string | null;
  modos: { codigo: string; descricao: string }[];
  onClose: () => void;
  onEmitido: (d: DocCompleto) => void;
}) {
  const [tipo, setTipo] = useState<"FR" | "FT">("FR");
  const [valores, setValores] = useState<Record<string, string>>({ NU: (total / 100).toFixed(2).replace(".", ",") });
  const [busy, setBusy] = useState(false);
  const nuRef = useRef<HTMLInputElement>(null);

  const pago = Object.values(valores).reduce((s, v) => s + parseKz(v || "0"), 0);
  const falta = Math.max(0, total - pago);
  const troco = Math.max(0, pago - total);

  useEffect(() => {
    nuRef.current?.select();
  }, []);

  async function emitir() {
    if (tipo === "FR" && falta > 0) return toast.error(`Falta pagar ${kz(falta)}.`);
    setBusy(true);
    try {
      const d = await call<DocCompleto>("documentos.emitir", {
        tipo,
        entidade_id: cliente.id,
        linhas: linhas.map((l) => ({
          artigo_id: l.artigo.id,
          quantidade: l.quantidade,
          preco_unitario: l.preco,
          desconto_pct: l.desconto,
          descricao: l.descricao ?? undefined,
          lote: l.lote || undefined,
        })),
        desconto_global_pct: descGlobal.tipo === "pct" && descGlobal.valor > 0 ? descGlobal.valor : undefined,
        desconto_global_valor: descGlobal.tipo === "valor" && descGlobal.valor > 0 ? descGlobal.valor : undefined,
        doc_origem_id: origemId ?? undefined,
        autorizacao_pin: pin ?? undefined,
        pagamentos: tipo === "FR" ? Object.entries(valores).map(([modo, v]) => ({ modo, valor: parseKz(v || "0") })) : [],
      });
      toast.success(`${d.documento.numero_doc} emitido.`);
      onEmitido(d);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  const rapidos = [total, Math.ceil(total / 100000) * 100000, Math.ceil(total / 500000) * 500000, Math.ceil(total / 1000000) * 1000000]
    .filter((v, i, a) => v >= total && a.indexOf(v) === i)
    .slice(0, 4);

  return (
    <Modal
      open
      onClose={onClose}
      title="Pagamento"
      width="max-w-2xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Voltar
          </Button>
          <Button size="lg" variant="accent" loading={busy} onClick={emitir} disabled={tipo === "FR" && falta > 0}>
            Emitir {tipo === "FR" ? "Factura/Recibo" : "Factura"}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          emitir();
        }}
      >
        <div className="mb-5 flex items-center justify-between rounded-xl bg-brand-700 px-5 py-4 text-white">
          <div>
            <div className="text-[13px] text-white/70">Total a pagar</div>
            <div className="num text-[32px] font-semibold tracking-tight">{kz(total)}</div>
          </div>
          <div className="text-right text-[13px] text-white/80">
            {cliente.nome}
            <br />
            NIF {cliente.nif}
          </div>
        </div>

        <Segmento
          valor={tipo}
          opcoes={[
            { id: "FR", label: "Pronto pagamento (FR)" },
            { id: "FT", label: "A crédito (FT)" },
          ]}
          onChange={setTipo}
        />

        {tipo === "FR" ? (
          <div className="grid grid-cols-2 gap-6">
            {/* uma linha por forma de pagamento: cabe no ecrã sem rolar até ao botão de emitir */}
            <div className="space-y-2">
              {modos
                .filter((m) => m.codigo !== "CS" || cliente.id !== 1)
                .map((m) => (
                  <label key={m.codigo} className="grid grid-cols-[minmax(0,1fr)_150px] items-center gap-3">
                    <span className="flex min-w-0 items-center gap-2 text-[13px] text-ink-2">
                      {m.codigo === "NU" ? <Banknote className="size-4 shrink-0 text-ink-3" /> : <CreditCard className="size-4 shrink-0 text-ink-3" />}
                      <span className="truncate" title={m.descricao}>
                        {m.descricao}
                      </span>
                    </span>
                    <Input
                      ref={m.codigo === "NU" ? nuRef : undefined}
                      className={cn("num h-9 text-right", m.codigo === "NU" && "font-medium")}
                      value={valores[m.codigo] ?? ""}
                      placeholder="0,00"
                      onChange={(e) => setValores({ ...valores, [m.codigo]: e.target.value })}
                      onFocus={(e) => e.target.select()}
                    />
                  </label>
                ))}
            </div>
            <div className="flex flex-col gap-3">
              <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Numerário rápido</div>
              <div className="grid grid-cols-2 gap-2">
                {rapidos.map((v) => (
                  <Button type="button" key={v} variant="secondary" size="lg" className="num" onClick={() => setValores({ NU: (v / 100).toFixed(2).replace(".", ",") })}>
                    {kzNum(v)}
                  </Button>
                ))}
              </div>
              <div className="mt-auto space-y-2 rounded-xl border border-line p-4">
                <Linha2 k="Recebido" v={kz(pago)} />
                <div className={cn("flex justify-between", falta > 0 ? "text-danger" : "text-ink-3")}>
                  <span>Em falta</span>
                  <span className="num">{kz(falta)}</span>
                </div>
                <div className="flex items-baseline justify-between border-t border-line pt-2">
                  <span className="font-medium">Troco</span>
                  <span className="num text-[24px] font-semibold text-ok">{kz(troco)}</span>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="rounded-xl border border-line bg-sunken/60 p-4 text-ink-2">
            {cliente.id === 1
              ? "A venda a crédito exige um cliente identificado. Feche esta janela e escolha o cliente."
              : `O valor fica em dívida na conta corrente de ${cliente.nome}. Pode liquidar mais tarde com um Recibo.`}
          </div>
        )}
        <button type="submit" hidden />
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------------ Cliente
function ClienteModal({ modo, onClose, onSelect }: { modo: "" | "lista" | "novo"; onClose: () => void; onSelect: (c: Entidade) => void }) {
  const [q, setQ] = useState("");
  const [novo, setNovo] = useState(false);
  const [f, setF] = useState({ nome: "", nif: "", telefone: "", endereco: "" });
  const dq = useDebounce(q);
  const lista = useQuery({ queryKey: ["clientes-pos", dq], queryFn: () => call<Entidade[]>("entidades.listar", { tipo: "C", q: dq }), enabled: !!modo });
  useEffect(() => setNovo(modo === "novo"), [modo]);

  async function criar() {
    try {
      const { id } = await call<{ id: number }>("entidades.guardar", { ...f, tipo: "C" });
      onSelect(await call<Entidade>("entidades.obter", { id }));
      setNovo(false);
      setF({ nome: "", nif: "", telefone: "", endereco: "" });
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  return (
    <Modal open={!!modo} onClose={onClose} title={novo ? "Novo cliente" : "Cliente"} width="max-w-xl">
      {!novo ? (
        <>
          <div className="flex gap-2">
            <Input autoFocus placeholder="Pesquisar por nome, NIF ou telefone…" value={q} onChange={(e) => setQ(e.target.value)} />
            <Button variant="secondary" onClick={() => setNovo(true)}>
              <Plus className="size-4" /> Novo
            </Button>
          </div>
          <ul className="mt-3 max-h-[50vh] overflow-y-auto">
            {lista.data?.map((c) => (
              <li key={c.id}>
                <button onClick={() => onSelect(c)} className="flex w-full items-center justify-between rounded-lg px-3 py-2.5 text-left hover:bg-sunken">
                  <span>
                    <span className="font-medium">{c.nome}</span>
                    <span className="ml-2 text-[12px] text-ink-3">NIF {c.nif}</span>
                  </span>
                  {(c.saldo ?? 0) > 0 && <Badge tone="accent">Dívida {kz(c.saldo)}</Badge>}
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <div className="space-y-4">
          <Field label="Nome">
            <Input autoFocus value={f.nome} onChange={(e) => setF({ ...f, nome: e.target.value })} />
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field label="NIF">
              <Input value={f.nif} onChange={(e) => setF({ ...f, nif: e.target.value })} />
            </Field>
            <Field label="Telefone">
              <Input value={f.telefone} onChange={(e) => setF({ ...f, telefone: e.target.value })} />
            </Field>
          </div>
          <Field label="Endereço">
            <Input value={f.endereco} onChange={(e) => setF({ ...f, endereco: e.target.value })} />
          </Field>
          <div className="flex justify-end gap-2">
            <Button variant="secondary" onClick={() => (modo === "novo" ? onClose() : setNovo(false))}>
              <X className="size-4" /> Cancelar
            </Button>
            <Button onClick={criar}>Criar e seleccionar</Button>
          </div>
        </div>
      )}
    </Modal>
  );
}

/** F5 — Alterar dados do cliente seleccionado. */
function EditarClienteModal({ cliente, onClose, onSaved }: { cliente: Entidade; onClose: () => void; onSaved: (c: Entidade) => void }) {
  const [f, setF] = useState({ ...cliente });
  async function guardar() {
    try {
      await call("entidades.guardar", f);
      onSaved(await call<Entidade>("entidades.obter", { id: cliente.id }));
      toast.success("Cliente actualizado.");
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal
      open
      onClose={onClose}
      title="Alterar dados do cliente"
      width="max-w-lg"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
          <Button onClick={guardar}>Guardar</Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-4">
        <Field label="Nome" className="col-span-2">
          <Input autoFocus value={f.nome} onChange={(e) => setF({ ...f, nome: e.target.value })} />
        </Field>
        <Field label="NIF">
          <Input value={f.nif} onChange={(e) => setF({ ...f, nif: e.target.value })} />
        </Field>
        <Field label="Telefone">
          <Input value={f.telefone ?? ""} onChange={(e) => setF({ ...f, telefone: e.target.value })} />
        </Field>
        <Field label="Endereço" className="col-span-2">
          <Input value={f.endereco ?? ""} onChange={(e) => setF({ ...f, endereco: e.target.value })} />
        </Field>
        <Field label="Email" className="col-span-2">
          <Input value={f.email ?? ""} onChange={(e) => setF({ ...f, email: e.target.value })} />
        </Field>
      </div>
    </Modal>
  );
}

// ------------------------------------------------------------------ Suspensos (F11)
function SuspensosModal({ onClose, onAbrir }: { onClose: () => void; onAbrir: (id: number) => void }) {
  const { data, refetch } = useQuery({
    queryKey: ["suspensos"],
    queryFn: () => call<{ id: number; entidade_nome: string | null; descricao: string | null; total: number; criado_em: string; utilizador: string }[]>("suspensos.listar"),
  });
  async function eliminar(id: number) {
    try {
      await call("suspensos.eliminar", { id });
      refetch();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  return (
    <Modal open onClose={onClose} title="Documentos suspensos" width="max-w-2xl">
      {!data?.length ? (
        <p className="py-8 text-center text-ink-3">Não há documentos suspensos neste posto.</p>
      ) : (
        <ul className="max-h-[60vh] divide-y divide-line overflow-y-auto">
          {data.map((s) => (
            <li key={s.id} className="flex items-center gap-3 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{s.descricao || s.entidade_nome || `Suspenso #${s.id}`}</div>
                <div className="text-[12px] text-ink-3">
                  {s.entidade_nome} · {s.criado_em.slice(0, 16)} · {s.utilizador}
                </div>
              </div>
              <span className="num font-medium">{kz(s.total)}</span>
              <Button size="sm" onClick={() => onAbrir(s.id)}>
                Abrir
              </Button>
              <button className="rounded-md p-1.5 text-ink-3 hover:bg-danger-50 hover:text-danger" onClick={() => eliminar(s.id)} title="Eliminar">
                <Trash2 className="size-4" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

// ------------------------------------------------------------------ Notas e proformas
function ProformasModal({ onClose, onAbrir }: { onClose: () => void; onAbrir: (id: number) => void }) {
  const [q, setQ] = useState("");
  const dq = useDebounce(q);
  const { data: lista } = useQuery({
    queryKey: ["pos-proformas", dq],
    queryFn: () => call<DocResumo[]>("documentos.listar", { classe: "PROFORMA", estado: "N", q: dq, limite: 100 }),
  });
  return (
    <Modal open onClose={onClose} title="Notas e proformas por facturar" width="max-w-2xl">
      <Input autoFocus placeholder="Pesquisar por número ou cliente…" value={q} onChange={(e) => setQ(e.target.value)} />
      {!lista?.length ? (
        <p className="py-8 text-center text-ink-3">Não há proformas ou orçamentos por facturar.</p>
      ) : (
        <ul className="mt-3 max-h-[55vh] divide-y divide-line overflow-y-auto">
          {lista.map((d) => (
            <li key={d.id}>
              <button onClick={() => onAbrir(d.id)} className="flex w-full items-center gap-3 rounded-lg px-2 py-2.5 text-left hover:bg-sunken">
                <div className="min-w-0 flex-1">
                  <div className="font-medium">{d.numero_doc}</div>
                  <div className="text-[12px] text-ink-3">
                    {d.entidade_nome} · {data(d.data_emissao)}
                  </div>
                </div>
                <span className="num font-medium">{kz(d.total)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

// ------------------------------------------------------------------ Abrir caixa
function AbrirCaixa({ open, onOpened }: { open: boolean; onOpened: () => void }) {
  const [valor, setValor] = useState("0,00");
  const [busy, setBusy] = useState(false);
  async function abrir() {
    setBusy(true);
    try {
      await call("caixa.abrir", { valor_inicial: parseKz(valor) });
      toast.success("Caixa aberto.");
      onOpened();
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      open={open}
      onClose={() => {}}
      title="Abertura de caixa"
      width="max-w-md"
      footer={
        <Button loading={busy} onClick={abrir}>
          Abrir caixa
        </Button>
      }
    >
      <p className="mb-4 text-ink-2">Indique o fundo de maneio (numerário inicial na gaveta).</p>
      <Field label="Valor inicial (Kz)">
        <Input autoFocus className="num text-right text-lg" value={valor} onChange={(e) => setValor(e.target.value)} onKeyDown={(e) => e.key === "Enter" && abrir()} />
      </Field>
    </Modal>
  );
}

/** Escolha do lote da linha (KIGEST POS: seleccionar lote na venda). Sem escolha, sai o que expira primeiro. */
function LoteModal({
  artigo,
  armazem,
  quantidade,
  atual,
  onPick,
  onClose,
}: {
  artigo: Artigo;
  armazem?: number;
  quantidade: number;
  atual: string | null;
  onPick: (lote: string | null) => void;
  onClose: () => void;
}) {
  const lotes = useQuery({ queryKey: ["lotes-artigo", artigo.id, armazem], queryFn: () => call<Lote[]>("lotes.listar", { artigo_id: artigo.id, armazem_id: armazem }) });
  return (
    <Modal open onClose={onClose} title={`Lote · ${artigo.descricao}`} width="max-w-lg">
      <div className="space-y-2">
        <button
          className={cn("w-full rounded-xl border px-4 py-3 text-left hover:bg-sunken", !atual ? "border-brand bg-brand-50/60" : "border-line")}
          onClick={() => onPick(null)}
        >
          <div className="font-medium">Automático</div>
          <div className="text-[12.5px] text-ink-3">Sai primeiro o lote com a validade mais próxima</div>
        </button>
        {lotes.isLoading && <p className="py-4 text-center text-ink-3">A carregar…</p>}
        {lotes.data?.length === 0 && <p className="py-4 text-center text-ink-3">Sem lotes com stock neste armazém.</p>}
        {lotes.data?.map((l) => {
          const curto = l.quantidade < quantidade;
          const expirado = l.dias_validade !== null && l.dias_validade < 0;
          return (
            <button
              key={l.id}
              disabled={expirado}
              className={cn(
                "flex w-full items-center justify-between rounded-xl border px-4 py-3 text-left hover:bg-sunken disabled:cursor-not-allowed disabled:opacity-50",
                atual === l.lote ? "border-brand bg-brand-50/60" : "border-line",
              )}
              onClick={() => onPick(l.lote)}
            >
              <div>
                <div className="font-medium">Lote {l.lote}</div>
                <div className={cn("text-[12.5px]", expirado ? "text-danger" : l.dias_validade !== null && l.dias_validade <= l.aviso_validade_dias ? "text-accent" : "text-ink-3")}>
                  {l.validade ? `Validade ${data(l.validade)}${expirado ? " · expirado" : l.dias_validade !== null ? ` · ${l.dias_validade} dia(s)` : ""}` : "Sem validade"}
                </div>
              </div>
              <div className={cn("num text-right", curto && "text-danger")}>
                {qtd(l.quantidade)} {artigo.unidade ?? "UN"}
                {curto && <div className="text-[11.5px]">menos do que a linha</div>}
              </div>
            </button>
          );
        })}
      </div>
    </Modal>
  );
}

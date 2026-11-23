// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  ArrowLeft,
  Banknote,
  ChevronLeft,
  ChevronRight,
  CreditCard,
  FileStack,
  FolderOpen,
  Boxes,
  Hash,
  Inbox,
  Keyboard,
  LayoutGrid,
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
  ShoppingBasket,
  Star,
  Tag,
  Trash2,
  UserRound,
  Wallet,
  X,
  XCircle,
} from "lucide-react";
import { call } from "@/lib/api";
import { useDebounce, useTabelas } from "@/lib/hooks";
import { data, hojeISO, kz, kzNum, nifValido, numero, parseKz, qtd } from "@/lib/format";
import { bip, camaraDisponivel, useLeitor } from "@/lib/leitor";
import { BotaoCamara, CodigoDesconhecido } from "@/components/Leitor";
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
import { useAparencia } from "@/lib/aparencia";
import { atributosDe, notaDaLinha, ramoPorId, resumoAtributos } from "@/lib/ramos";
import { useMiniaturas } from "@/lib/fotos";
import { useMeuPerfil } from "@/lib/perfil";
import { Avatar } from "@/components/MeuPerfil";
import { abrirVisorEcra, ecraInteiro, linhasVisorColuna, ouvirVisorPronto, publicarVisor, type EstadoVisor } from "@/lib/equipamento";

interface Linha {
  key: string;
  artigo: Artigo;
  quantidade: number;
  preco: number; // sem IVA (cêntimos)
  pvp?: number | null; // com IVA (cêntimos): o valor mestre com «preços com IVA incluído»
  regra?: string | null; // promoção por regras aplicada (ex.: «Leve 3 pague 2»)
  desconto: number; // %
  descricao: string | null; // descrição de linha (F9)
  lote?: string | null; // lote escolhido (vazio = o que expira primeiro)
  manual?: boolean; // preço/desconto fixados manualmente ou vindos de proforma/suspenso
}

type DescGlobal = { tipo: "pct" | "valor"; valor: number }; // pct: %, valor: cêntimos
type LinhaGuardada = { artigo_id: number; quantidade: number; preco: number; pvp?: number | null; desconto: number; descricao?: string | null };

const OPCOES_PADRAO: PostoOpcoes = {
  formato: "ticket",
  vias: 1,
  // venda rápida: imprime sozinho e volta logo à venda seguinte
  imprimir_auto: true,
  mostrar_opcoes_impressao: false,
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
type SaldoPontos = { pontos: number; valor: number; pode_pagar: boolean };
const semIvaArt = (pvp: number, a: Artigo) => Math.round((pvp * 100) / (100 + a.taxa_pct));
/** PVP com IVA do nível de preço: o guardado, se ainda bater com o preço sem IVA; senão, calculado (como o servidor). */
const pvpNivel = (a: Artigo, nivel: number) => {
  const n = [a.preco1, a.preco2, a.preco3, a.preco4][nivel - 1] ? nivel : 1;
  const preco = precoNivel(a, n);
  const g = [a.pvp1, a.pvp2, a.pvp3, a.pvp4][n - 1];
  return g != null && semIvaArt(g, a) === preco ? g : comIvaArt(preco, a);
};

type CalcLinha = { bruto: number; desc: number; liquido: number; iva: number; total: number };

function resumir(linhas: Linha[], calc: CalcLinha[]) {
  const t = calc.reduce(
    (s, c, i) => ({ bruto: s.bruto + c.bruto, desc: s.desc + c.desc, iva: s.iva + c.iva, total: s.total + c.total, itens: s.itens + linhas[i].quantidade }),
    { bruto: 0, desc: 0, iva: 0, total: 0, itens: 0 },
  );
  const maxDesc = calc.reduce((m, c) => Math.max(m, c.bruto ? (c.desc * 100) / c.bruto : 0), 0);
  return { linhas: calc, ...t, maxDesc };
}

/** «Preços com IVA incluído»: a linha parte do PVP (quantidade × PVP − desconto) e o IVA sai do total. */
function calcularComIva(linhas: Linha[], g: DescGlobal) {
  const gp = g.tipo === "pct" ? g.valor : 0;
  const base = linhas.map((l) => {
    const brutoG = Math.round(l.quantidade * (l.pvp ?? comIvaArt(l.preco, l.artigo)));
    const pct = 100 - ((100 - l.desconto) * (100 - gp)) / 100;
    const descG = Math.round((brutoG * pct) / 100);
    return { descG, total: brutoG - descG };
  });
  if (g.tipo === "valor" && g.valor > 0 && base.length) {
    const tot = base.reduce((s, b) => s + b.total, 0);
    const valor = Math.min(g.valor, tot);
    let restante = valor;
    base.forEach((b, i) => {
      const quota = i === base.length - 1 ? restante : Math.round((valor * b.total) / (tot || 1));
      const d = Math.min(quota, b.total, restante);
      restante -= d;
      b.total -= d;
      b.descG += d;
    });
  }
  const calc = base.map((b, i) => {
    const t = linhas[i].artigo.taxa_pct;
    const iva = Math.round((b.total * t) / (100 + t));
    const desc = Math.round((b.descG * 100) / (100 + t));
    return { bruto: b.total - iva + desc, desc, liquido: b.total - iva, iva, total: b.total };
  });
  return resumir(linhas, calc);
}

/** Mesmas regras de arredondamento do backend (documentos::emitir). */
function calcular(linhas: Linha[], g: DescGlobal, comIva = false) {
  if (comIva) return calcularComIva(linhas, g);
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
  return resumir(linhas, calc);
}

type Teclado = { modo: "qtd" | "preco" | "desc" | "global" } | null;
type PedidoPin = { permissao: string; descricao: string; desconto?: number; acao: () => void } | null;

type Cupao = { codigo: string; tipo: "PCT" | "VALOR"; valor: number; descricao: string | null };

/** Uma venda em curso (há várias em paralelo: «Venda 2/3», como nos terminais com fila). */
type VendaAberta = {
  linhas: Linha[];
  sel: number | null;
  cliente: Entidade;
  descGlobal: DescGlobal;
  origem: { id: number; numero: string } | null;
  pin: string | null;
  cupao: Cupao | null;
  /** nome dado à venda (ex.: «Mesa 4», «Sr. João») */
  nome?: string | null;
};
const MAX_VENDAS = 12;

export default function Pos() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const { sessao, pode, tem, setSessao } = useSession();
  const fotoPerfil = useMeuPerfil().data?.foto;
  const tabelas = useTabelas();
  const opcoes: PostoOpcoes = { ...OPCOES_PADRAO, ...(tabelas.data?.posto?.opcoes ?? {}) };
  const termica = !!opcoes.escpos?.ativo;
  const politicaStock = tabelas.data?.sem_stock ?? "nao";

  const [busca, setBusca] = useState("");
  const [familia, setFamilia] = useState<number | null>(null);
  // relógio da barra (os terminais POS ficam muitas vezes em ecrã inteiro, sem o relógio do sistema)
  const [agora, setAgora] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setAgora(new Date()), 15_000);
    return () => clearInterval(t);
  }, []);
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
  const [cupao, setCupao] = useState<Cupao | null>(null);
  const [pedirCupao, setPedirCupao] = useState(false);
  const [docEmitido, setDocEmitido] = useState<number | null>(null);
  const [impressaoAuto, setImpressaoAuto] = useState<number | null>(null);
  const buscaRef = useRef<HTMLInputElement>(null);
  const q = useDebounce(busca, 200);
  // equipamento: venda acabada mostrada no visor, ecrã inteiro e teclado no ecrã (terminais tácteis)
  const [fimVenda, setFimVenda] = useState<{ documento: string; total: number; pago: number; troco: number } | null>(null);
  const [cheio, setCheio] = useState(false);
  const [tecladoEcra, setTecladoEcra] = useState(false);
  const empresa = useEmpresa();
  // ecrã táctil: a pesquisa fica com o foco (o leitor de códigos escreve nela) sem abrir o teclado do
  // sistema; abre-se ao tocar no campo. No telemóvel, artigos e venda são dois separadores.
  const tactil = useAparencia((a) => a.tactil);
  const [tecladoSistema, setTecladoSistema] = useState(false);
  const [vista, setVista] = useState<"artigos" | "venda">("artigos");
  // ramo de actividade: dados do artigo no botão, receita médica, n.º de série, mesas com nome
  const ramo = ramoPorId(tabelas.data?.ramo);
  const [pedirSerie, setPedirSerie] = useState<{ artigo: Artigo; quantidade: number } | null>(null);
  const [nomeVenda, setNomeVenda] = useState<string | null>(null);
  const [renomear, setRenomear] = useState(false);
  // leitor: código que nenhum artigo tem (juntá-lo a um artigo ou criar o artigo)
  const [desconhecido, setDesconhecido] = useState<string | null>(null);

  const caixa = useQuery({ queryKey: ["caixa"], queryFn: () => call<CaixaResumo | null>("caixa.estado") });
  const artigos = useQuery({
    queryKey: ["pos-artigos", q, familia],
    queryFn: () => call<Artigo[]>("artigos.listar", { q, familia_id: familia, limite: 120 }),
  });

  const miniaturas = useMiniaturas(artigos.data);
  const focar = () => setTimeout(() => buscaRef.current?.focus(), 30);
  const comIva = !!tabelas.data?.precos_com_iva;
  // fidelização: saldo de pontos do cliente identificado
  const fid = tabelas.data?.fidelizacao;
  const pontos = useQuery({
    queryKey: ["pontos", cliente.id],
    queryFn: () => call<SaldoPontos>("fidelizacao.saldo", { entidade_id: cliente.id }),
    enabled: !!fid?.ativa && cliente.id !== 1,
  });
  const totais = useMemo(() => calcular(linhas, descGlobal, comIva), [linhas, descGlobal, comIva]);
  const idx = sel ?? (linhas.length ? linhas.length - 1 : null); // linha activa (seleccionada ou a última)
  const linhaAtiva = idx !== null ? linhas[idx] : null;
  const algumModal = !!desconhecido || !!pedirSerie || renomear || pagar || !!escolherCliente || editarCliente || confirmarCliente || !!teclado || descricaoLinha || escolherLote || suspender || verSuspensos || verProformas || !!pedidoPin || !!docEmitido || pedirCupao;

  /** Executa a acção se o operador tiver permissão (e limite de desconto); caso contrário pede o PIN de um supervisor. */
  function comPermissao(permissao: string, descricao: string, acao: () => void, desconto?: number) {
    const limite = sessao?.limite_desconto ?? 0;
    const limiteOk = desconto === undefined || limite <= 0 || desconto <= limite + 1e-9;
    if (tem(permissao) && limiteOk) return acao();
    setPedidoPin({ permissao, descricao, desconto, acao });
  }

  const adicionar = useCallback(
    (a: Artigo, quantidade = 1, descricao: string | null = null, separada = false) => {
      setLinhas((ls) => {
        const i = opcoes.repetir_linhas || separada ? -1 : ls.findIndex((l) => l.artigo.id === a.id && !l.manual && l.descricao === descricao);
        if (i >= 0) {
          const n = [...ls];
          n[i] = { ...n[i], quantidade: +(n[i].quantidade + quantidade).toFixed(3) };
          setSel(i);
          return n;
        }
        setSel(ls.length);
        return [
          ...ls,
          { key: uid(), artigo: a, quantidade, preco: precoNivel(a, cliente.nivel_preco), pvp: pvpNivel(a, cliente.nivel_preco), desconto: Math.max(cliente.desconto || 0, a.promo_atual || 0), descricao },
        ];
      });
    },
    [cliente, opcoes.repetir_linhas],
  );

  /** Lança o artigo com as regras do ramo: aviso de receita, n.º de série (uma linha por unidade) e garantia na descrição. */
  /** Juntar mais `quantidade` passa a existência deste posto? (artigos com stock; os kits confere-os o servidor) */
  function faltaStock(a: Artigo, quantidade: number) {
    if (a.controla_stock !== 1 || a.tipo !== "P" || a.composto === 1) return false;
    const noCarrinho = linhas.filter((l) => l.artigo.id === a.id).reduce((s, l) => s + l.quantidade, 0);
    return noCarrinho + quantidade > a.stock + 1e-9;
  }

  /** Junta o artigo à venda; sem stock, conforme a política da empresa: recusa, pede autorização ou segue a ficha. */
  function vender(a: Artigo, quantidade = 1): boolean {
    if (faltaStock(a, quantidade) && !(politicaStock === "artigo" && a.permite_negativo === 1)) {
      const ha = qtd(Math.max(0, a.stock));
      if (politicaStock !== "autorizar") {
        bip(false);
        toast.error(`Sem stock de «${a.descricao}»: há ${ha}.`, { description: "Registe primeiro a entrada (compra ou entrada de stock), ou acerte-o no inventário." });
        return false;
      }
      if (!tem("stock.sem_stock")) {
        comPermissao("stock.sem_stock", `Vender «${a.descricao}» sem stock (há ${ha})`, () => juntar(a, quantidade));
        return true;
      }
      toast.warning(`«${a.descricao}» vendido sem stock (há ${ha}).`);
    }
    juntar(a, quantidade);
    return true;
  }

  function juntar(a: Artigo, quantidade: number) {
    const at = atributosDe(a);
    if (at.receita) toast.warning(`${a.descricao}: sujeito a receita médica — confirme a receita.`, { duration: 7000 });
    if (at.pede_serie) {
      setPedirSerie({ artigo: a, quantidade: Math.max(1, Math.round(quantidade)) });
      return;
    }
    const nota = notaDaLinha(a);
    adicionar(a, quantidade, nota ? `${a.descricao} · ${nota}` : null);
  }

  // Enter no campo de pesquisa: leitor de código de barras / referência. Suporta "3*codigo".
  async function onEnter() {
    const txt = busca.trim();
    if (!txt) return;
    if (await lerCodigo(txt, true)) setBusca("");
  }

  /** Um código lido (leitor, câmara ou Enter na pesquisa); «3*código» vende 3. Devolve se o tratou. */
  async function lerCodigo(txt: string, daPesquisa = false): Promise<boolean> {
    const m = txt.match(/^(\d+(?:[.,]\d+)?)\*(.+)$/);
    const quantidade = m ? Number(m[1].replace(",", ".")) : 1;
    const codigo = (m ? m[2] : txt).trim();
    try {
      const a = await call<Artigo | null>("artigos.codigo", { codigo });
      if (a) {
        // GS1 (DataMatrix dos medicamentos): com a validade passada não entra na venda
        const validade = a.gs1?.validade;
        const dataPt = (v: string) => v.split("-").reverse().join("/");
        if (validade && validade < hojeISO().slice(0, validade.length)) {
          bip(false);
          toast.error(`${a.descricao}: a validade passou (${dataPt(validade)}). Não foi juntado à venda.`, {
            description: `${a.gs1?.lote ? `Lote ${a.gs1.lote}. ` : ""}Para o vender mesmo assim, escolha-o na lista de artigos.`,
            duration: 12000,
          });
          return true;
        }
        // embalagem (ex.: pack de 12) ou etiqueta de balança: a quantidade vem no código
        if (vender(a, a.quantidade_lida ? +(quantidade * a.quantidade_lida).toFixed(6) : quantidade)) bip(true);
        if (a.etiqueta) toast.info(`${a.descricao}: ${a.etiqueta.tipo === "PESO" ? `${qtd(a.etiqueta.valor)} kg` : kz(a.etiqueta.valor)} (etiqueta da balança)`);
        if (a.gs1?.lote || validade) toast.info(`${a.descricao}: ${[a.gs1?.lote && `lote ${a.gs1.lote}`, validade && `validade ${dataPt(validade)}`].filter(Boolean).join(", ")}`);
        return true;
      }
      // texto escrito à mão com um só resultado na lista (só se a lista já for deste texto: com o leitor, o Enter
      // chega antes de a pesquisa responder e a lista ainda seria a da pesquisa anterior)
      if (daPesquisa && q.trim() === txt && !artigos.isFetching && artigos.data?.length === 1) {
        vender(artigos.data[0], quantidade);
        return true;
      }
      bip(false);
      if (/^\S{4,}$/.test(codigo) && /\d/.test(codigo)) {
        setDesconhecido(codigo);
        return true;
      }
      toast.warning(`Artigo "${codigo}" não encontrado.`);
    } catch (e) {
      toast.error((e as Error).message);
    }
    return false;
  }
  // leitura com o cursor noutro campo (quantidade, desconto…): vai para a venda e não fica nesse campo
  useLeitor((l) => void lerCodigo(l.codigo), !algumModal);

  function mudarCliente(c: Entidade) {
    setCliente(c);
    // reaplica nível de preço e desconto do cliente às linhas não alteradas manualmente
    setLinhas((ls) =>
      ls.map((l) => (l.manual ? l : { ...l, preco: precoNivel(l.artigo, c.nivel_preco), pvp: pvpNivel(l.artigo, c.nivel_preco), desconto: Math.max(c.desconto || 0, l.artigo.promo_atual || 0) })),
    );
  }

  function limpar() {
    setLinhas([]);
    setSel(null);
    setCliente(CONSUMIDOR);
    setDescGlobal({ tipo: "pct", valor: 0 });
    setCupao(null);
    setOrigem(null);
    setPin(null);
    setNomeVenda(null);
    focar();
  }

  // ---------------- vendas em paralelo (a actual vive nos estados acima; as outras ficam em `abertas`) ----------------
  const [abertas, setAbertas] = useState<(VendaAberta | null)[]>([null]);
  const [atual, setAtual] = useState(0);
  const fotografia = (): VendaAberta => ({ linhas, sel, cliente, descGlobal, origem, pin, cupao, nome: nomeVenda });

  function carregarVenda(v: VendaAberta | null) {
    setLinhas(v?.linhas ?? []);
    setSel(v?.sel ?? null);
    setCliente(v?.cliente ?? CONSUMIDOR);
    setDescGlobal(v?.descGlobal ?? { tipo: "pct", valor: 0 });
    setOrigem(v?.origem ?? null);
    setPin(v?.pin ?? null);
    setCupao(v?.cupao ?? null);
    setNomeVenda(v?.nome ?? null);
    focar();
  }

  function irParaVenda(destino: number) {
    if (destino === atual || destino < 0 || destino >= abertas.length) return;
    const lista = [...abertas];
    const v = lista[destino];
    lista[destino] = null;
    let alvo = destino;
    // uma venda vazia não fica à espera
    if (linhas.length) lista[atual] = fotografia();
    else {
      lista.splice(atual, 1);
      if (alvo > atual) alvo--;
    }
    setAbertas(lista);
    setAtual(alvo);
    carregarVenda(v);
  }

  function novaVenda() {
    if (!linhas.length && !ramo.vendasComNome) return toast.info("Esta venda ainda está vazia: use-a.");
    if (!linhas.length) return setRenomear(true);
    if (abertas.length >= MAX_VENDAS) return toast.error(`No máximo ${MAX_VENDAS} vendas abertas ao mesmo tempo.`);
    const lista = [...abertas];
    lista[atual] = fotografia();
    lista.push(null);
    setAbertas(lista);
    setAtual(lista.length - 1);
    carregarVenda(null);
    setVista("artigos");
    // restauração, salão: cada venda em paralelo é uma mesa / um cliente com nome
    if (ramo.vendasComNome) {
      setNomeVenda(`${ramo.vendasComNome} ${lista.length}`);
      setRenomear(true);
    }
  }

  /** A venda actual acabou (emitida, suspensa ou cancelada): fica uma venda nova no fim; as outras continuam abertas. */
  function terminarVenda() {
    limpar();
    if (abertas.length > 1) {
      const lista = abertas.filter((_, i) => i !== atual);
      lista.push(null);
      setAbertas(lista);
      setAtual(lista.length - 1);
      toast.info(`${lista.length - 1} ${lista.length === 2 ? "venda continua aberta" : "vendas continuam abertas"} — use ‹ › para voltar.`);
    }
  }

  // As outras vendas abertas ficam guardadas neste aparelho (a actual já se guarda em «venda em curso
  // gravada», mais abaixo): recarregar a página ou fechar o browser por engano não as perde.
  const chaveVendas = tabelas.data?.posto ? `kangest.pos.vendas.${tabelas.data.posto.id}.${sessao?.login ?? ""}` : null;
  const restaurado = useRef<string | null>(null);
  const abertasRef = useRef(abertas);
  abertasRef.current = abertas;
  useEffect(() => {
    if (!chaveVendas || restaurado.current === chaveVendas) return;
    restaurado.current = chaveVendas;
    let g: { abertas: (VendaAberta | null)[]; atual: number } | null = null;
    try {
      g = JSON.parse(localStorage.getItem(chaveVendas) ?? "null");
    } catch {
      g = null;
    }
    if (!g?.abertas?.length) return;
    const a = Math.min(Math.max(0, g.atual), g.abertas.length - 1);
    const outras = g.abertas.map((x, i) => (i === a ? null : x));
    if (!outras.some((v) => v?.linhas.length)) return;
    (async () => {
      // os artigos vêm frescos do servidor (stock, lotes); preço, desconto e quantidade ficam os da venda
      const ids = [...new Set(outras.flatMap((v) => v?.linhas.map((l) => l.artigo.id) ?? []))];
      const arts = await call<Artigo[]>("artigos.porIds", { ids });
      const mapa = new Map(arts.map((x) => [x.id, x]));
      if (abertasRef.current.length !== 1) return; // entretanto já se abriram vendas novas
      const frescas = outras.map((v) => (v ? { ...v, pin: null, linhas: v.linhas.filter((l) => mapa.has(l.artigo.id)).map((l) => ({ ...l, artigo: mapa.get(l.artigo.id)! })) } : v));
      setAbertas(frescas);
      setAtual(a);
      const n = frescas.filter((v) => v?.linhas.length).length;
      toast.info(n === 1 ? "Há outra venda aberta à espera (‹ ›)." : `Há ${n} vendas abertas à espera (‹ ›).`);
    })().catch(() => undefined);
  }, [chaveVendas]);
  useEffect(() => {
    if (!chaveVendas || restaurado.current !== chaveVendas) return;
    // a autorização do supervisor (PIN) nunca fica gravada
    const outras = abertas.map((v, i) => (i === atual || !v ? null : { ...v, pin: null }));
    try {
      if (outras.some((v) => v?.linhas.length)) localStorage.setItem(chaveVendas, JSON.stringify({ abertas: outras, atual }));
      else localStorage.removeItem(chaveVendas);
    } catch {
      /* armazenamento cheio ou indisponível */
    }
  }, [chaveVendas, abertas, atual]);

  // ---------------- promoções por regras (o servidor calcula; aplicam-se às linhas não alteradas à mão) ----------------
  const chavePromo = linhas.map((l) => `${l.artigo.id}:${l.quantidade}:${l.manual ? 1 : 0}`).join("|");
  useEffect(() => {
    if (!linhas.length) return;
    const t = setTimeout(async () => {
      try {
        const r = await call<{ desconto: number; regra: string | null }[]>("promocoes.calcular", { linhas: linhas.map((l) => ({ artigo_id: l.artigo.id, quantidade: l.quantidade })) });
        setLinhas((ls) =>
          ls.map((l, i) => {
            if (l.manual || !r[i] || ls.length !== r.length) return l;
            const base = Math.max(cliente.desconto || 0, l.artigo.promo_atual || 0);
            const desconto = Math.max(base, r[i].desconto);
            const regra = r[i].desconto > base ? r[i].regra : null;
            return desconto === l.desconto && regra === (l.regra ?? null) ? l : { ...l, desconto, regra };
          }),
        );
      } catch {
        // sem regras ou sem ligação: fica o desconto que já havia
      }
    }, 250);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chavePromo, cliente.id]);

  // ---------------- cupão de desconto ----------------
  async function aplicarCupao(codigo: string) {
    if (!codigo.trim()) return;
    try {
      const c = await call<{ codigo: string; tipo: "PCT" | "VALOR"; valor: number; descricao: string | null }>("cupoes.validar", { codigo, total: totais.total });
      setCupao(c);
      setDescGlobal(c.tipo === "PCT" ? { tipo: "pct", valor: c.valor } : { tipo: "valor", valor: Math.round(c.valor) });
      setPedirCupao(false);
      toast.success(`Cupão ${c.codigo} aplicado${c.descricao ? ` (${c.descricao})` : ""}.`);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
  function retirarCupao() {
    setCupao(null);
    setDescGlobal({ tipo: "pct", valor: 0 });
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
      comPermissao("pos.alterar_preco", "Alterar preço", () => atualizar(i, { preco, pvp: Math.round(valor * 100), manual: true }));
    } else if (modo === "desc" && i !== null) {
      if (valor > 100) return toast.error("Desconto inválido.");
      comPermissao("pos.desconto", `Desconto de ${valor}%`, () => atualizar(i, { desconto: valor, manual: true }), valor);
    } else if (modo === "global") {
      const g: DescGlobal = descGlobal.tipo === "pct" ? { tipo: "pct", valor } : { tipo: "valor", valor: Math.round(valor * 100) };
      if (g.tipo === "pct" && g.valor > 100) return toast.error("Desconto inválido.");
      const efetivo = calcular(linhas, g, comIva).maxDesc;
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
        .map((l) => ({ key: uid(), artigo: mapa.get(l.artigo_id)!, quantidade: l.quantidade, preco: l.preco, pvp: l.pvp ?? null, desconto: l.desconto, descricao: l.descricao ?? null, manual: true })),
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
          linhas: linhas.map((l) => ({ artigo_id: l.artigo.id, quantidade: l.quantidade, preco: l.preco, pvp: l.pvp ?? null, desconto: l.desconto, descricao: l.descricao })),
          desc_global: descGlobal,
          origem,
        },
      });
      toast.success("Documento suspenso.");
      setSuspender(false);
      terminarVenda();
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
        d.linhas.map((l) => ({ artigo_id: l.artigo_id, quantidade: l.quantidade, preco: l.preco_unitario, pvp: l.pvp_unitario ?? null, desconto: l.desconto_pct, descricao: l.descricao })),
      );
      await carregarCliente(d.documento.entidade_id);
      setOrigem({ id: d.documento.id, numero: d.documento.numero_doc });
      setVerProformas(false);
      focar();
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  // ---------------- venda em curso gravada (falha de energia, programa fechado) ----------------
  const chaveCarrinho = `kangest.carrinho.${tabelas.data?.posto?.id ?? 1}`;
  const recuperado = useRef(false);
  useEffect(() => {
    if (recuperado.current || !tabelas.data) return;
    recuperado.current = true;
    let guardado: {
      linhas: (LinhaGuardada & { lote?: string | null; manual?: boolean })[];
      cliente: number | null;
      desc_global: DescGlobal;
      origem: { id: number; numero: string } | null;
      nome?: string | null;
    } | null = null;
    try {
      guardado = JSON.parse(localStorage.getItem(chaveCarrinho) ?? "null");
    } catch {
      guardado = null;
    }
    if (!guardado?.linhas?.length || linhas.length) return;
    (async () => {
      const arts = await call<Artigo[]>("artigos.porIds", { ids: guardado.linhas.map((l) => l.artigo_id) });
      const mapa = new Map(arts.map((a) => [a.id, a]));
      setLinhas(
        guardado.linhas
          .filter((l) => mapa.has(l.artigo_id))
          .map((l) => ({ key: uid(), artigo: mapa.get(l.artigo_id)!, quantidade: l.quantidade, preco: l.preco, pvp: l.pvp ?? null, desconto: l.desconto, descricao: l.descricao ?? null, lote: l.lote ?? null, manual: !!l.manual })),
      );
      setDescGlobal(guardado.desc_global ?? { tipo: "pct", valor: 0 });
      setOrigem(guardado.origem ?? null);
      setNomeVenda(guardado.nome ?? null);
      await carregarCliente(guardado.cliente);
      toast.info(`Recuperada a venda que estava em curso (${guardado.linhas.length} linha(s)).`);
    })().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabelas.data]);
  useEffect(() => {
    if (!recuperado.current) return;
    try {
      if (!linhas.length) localStorage.removeItem(chaveCarrinho);
      else
        localStorage.setItem(
          chaveCarrinho,
          JSON.stringify({
            linhas: linhas.map((l) => ({ artigo_id: l.artigo.id, quantidade: l.quantidade, preco: l.preco, pvp: l.pvp ?? null, desconto: l.desconto, descricao: l.descricao, lote: l.lote ?? null, manual: !!l.manual })),
            cliente: cliente.id === 1 ? null : cliente.id,
            desc_global: descGlobal,
            origem,
            nome: nomeVenda,
          }),
        );
    } catch {
      // armazenamento indisponível: a venda continua só em memória
    }
  }, [linhas, cliente, descGlobal, origem, nomeVenda, chaveCarrinho]);

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
    terminarVenda();
    setVista("artigos");
    // o visor mostra o total, o valor recebido e o troco durante uns segundos
    setFimVenda({ documento: d.documento.numero_doc, total: d.documento.total, pago: d.documento.valor_entregue || d.documento.total, troco: d.documento.troco });
    qc.invalidateQueries({ queryKey: ["caixa"] });
    qc.invalidateQueries({ queryKey: ["pos-artigos"] });
    qc.invalidateQueries({ queryKey: ["pontos"] });
    // a existência mudou: o mapa de stock e a lista de artigos mostram-na já
    qc.invalidateQueries({ queryKey: ["stock"] });
    qc.invalidateQueries({ queryKey: ["artigos"] });
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
      if (e.altKey && !e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        e.preventDefault();
        irParaVenda(atual + (e.key === "ArrowLeft" ? -1 : 1));
        return;
      }
      if (e.altKey && !e.ctrlKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        novaVenda();
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
    { k: "F8", label: "Desc. global", icon: <Percent className="size-4" />, on: () => (cupao ? toast.info("Retire o cupão para dar outro desconto global.") : abrirTeclado("global")) },
    { k: "", label: "Cupão", icon: <Tag className="size-4" />, on: () => (linhas.length ? setPedirCupao(true) : toast.info("Adicione artigos antes do cupão.")) },
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
      <header className="flex h-14 shrink-0 items-center gap-2 border-b border-line bg-surface px-2 sm:gap-4 sm:px-4">
        {pode("GESTOR") ? (
          <Button variant="ghost" size="sm" onClick={() => nav("/")} title="Voltar à gestão" className="max-sm:px-2">
            <ArrowLeft className="size-4" /> <span className="max-sm:hidden">Gestão</span>
          </Button>
        ) : (
          <Button variant="ghost" size="sm" onClick={() => nav("/documentos")}>
            Documentos
          </Button>
        )}
        <div className="flex min-w-0 items-center gap-2">
          <Logo className="size-8 max-sm:hidden" />
          <span className="hidden whitespace-nowrap font-semibold xl:inline">Ponto de Venda</span>
          <Badge tone="neutral" className="max-w-32 shrink-0 truncate">
            {tabelas.data?.posto?.descricao ?? "Posto"}
          </Badge>
        </div>
        <div className="ml-auto flex items-center gap-1 sm:gap-3">
          {caixa.data ? (
            <Badge tone="ok" className="max-sm:hidden">
              <Wallet className="mr-1 size-3.5" /> Caixa aberto · {caixa.data.documentos} vendas
            </Badge>
          ) : (
            <Badge tone="danger">Caixa fechado</Badge>
          )}
          <Button variant="secondary" size="sm" onClick={() => nav("/caixa")} title="Caixa">
            <Wallet className="size-4 sm:hidden" /> <span className="max-sm:hidden">Caixa</span>
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
          <Button variant="ghost" size="sm" onClick={alternarEcraInteiro} title={cheio ? "Sair do ecrã inteiro" : "Ecrã inteiro"} className="max-sm:hidden">
            {cheio ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
          </Button>
          <span className="num hidden text-[13px] text-ink-2 lg:inline" title={agora.toLocaleDateString("pt-PT", { weekday: "long", day: "numeric", month: "long" })}>
            {agora.toLocaleDateString("pt-PT", { weekday: "short", day: "2-digit", month: "2-digit" })} · <b className="font-semibold text-ink">{agora.toLocaleTimeString("pt-PT", { hour: "2-digit", minute: "2-digit" })}</b>
          </span>
          <span className="flex items-center gap-2 text-[13px] text-ink-2">
            <span className="max-sm:hidden" title={sessao?.nome}>
              <Avatar nome={sessao?.nome} foto={fotoPerfil} className="size-7 text-[12px]" />
            </span>
            <span className="hidden max-w-40 truncate xl:inline">{sessao?.nome}</span>
          </span>
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
        <section className={cn("flex min-w-0 flex-1 flex-col p-3 sm:p-4", vista === "venda" && "max-md:hidden")}>
          <div className="relative">
            <ScanBarcode className="absolute left-4 top-1/2 size-5 -translate-y-1/2 text-ink-3" />
            <input
              ref={buscaRef}
              data-leitor
              autoFocus
              // no ecrã táctil o teclado do sistema só abre quando se toca no campo
              inputMode={tactil && !tecladoSistema ? "none" : "search"}
              enterKeyHint="search"
              onPointerDown={() => tactil && setTecladoSistema(true)}
              onBlur={() => setTecladoSistema(false)}
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
              className={cn(camaraDisponivel() ? "pr-24" : "pr-14", "h-12 w-full rounded-xl border border-line-strong bg-surface pl-12 text-[16px] shadow-card outline-none focus:border-brand focus:ring-4 focus:ring-brand/15 sm:h-14")}
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
            {camaraDisponivel() && (
              <BotaoCamara
                onLer={(c) => {
                  void lerCodigo(c);
                  focar();
                }}
                className="absolute right-12 top-1/2 grid size-10 -translate-y-1/2 place-items-center rounded-lg text-ink-3 transition hover:bg-sunken hover:text-ink"
              />
            )}
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
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-[repeat(auto-fill,minmax(150px,1fr))] sm:gap-3 xl:grid-cols-[repeat(auto-fill,minmax(170px,1fr))]">
                {artigos.data?.map((a) => {
                  const semStock = a.controla_stock === 1 && a.tipo === "P" && a.composto !== 1 && a.stock <= 0;
                  // com «só com autorização» o artigo continua a poder tocar-se (pede o PIN); com «nunca» não
                  const bloqueado = semStock && (politicaStock === "nao" || (politicaStock === "artigo" && a.permite_negativo !== 1));
                  return (
                    <button
                      key={a.id}
                      onClick={() => {
                        vender(a);
                        focar();
                      }}
                      disabled={bloqueado}
                      className="group relative flex h-[104px] flex-col rounded-xl border border-line bg-surface p-2.5 text-left sm:h-[118px] sm:p-3 shadow-card transition hover:-translate-y-0.5 hover:border-brand/50 hover:shadow-md active:translate-y-0 active:scale-[0.98] disabled:bg-sunken/60 disabled:hover:translate-y-0 disabled:hover:border-line disabled:hover:shadow-card"
                    >
                      {semStock && <span className="absolute right-2 top-2 z-[1] rounded-md bg-danger-50 px-1.5 py-0.5 text-[10.5px] font-medium text-danger">Sem stock</span>}
                      <span className="flex min-w-0 items-start gap-2">
                        <span className="line-clamp-2 min-w-0 flex-1 text-[13.5px] font-medium leading-snug">{a.descricao}</span>
                        {miniaturas[a.id] && <img src={miniaturas[a.id]} alt="" className={cn("size-11 shrink-0 rounded-lg object-cover", semStock && "opacity-50")} loading="lazy" />}
                      </span>
                      <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[11.5px] text-ink-3">
                        <span className="truncate">{resumoAtributos(a, ramo) || a.referencia}</span>
                        {!!a.promo_atual && <span className="rounded bg-accent-50 px-1.5 py-px font-medium text-accent-ink">−{numero(a.promo_atual, 2)}%</span>}
                      </span>
                      <div className="mt-auto flex items-end justify-between gap-2">
                        <span className={cn("num text-[16px] font-semibold", semStock ? "text-ink-3" : "text-brand-700")}>{kzNum(pvpNivel(a, cliente.nivel_preco))}</span>
                        {a.tipo === "S" ? (
                          <span className="text-[11px] text-ink-3">Serviço</span>
                        ) : (
                          a.controla_stock === 1 && (
                            <span
                              className={cn("num inline-flex items-center gap-1 text-[11px]", a.stock <= 0 ? "text-danger" : a.stock <= a.qtd_minima ? "text-accent-ink" : "text-ink-3")}
                              title={a.stock <= a.qtd_minima ? "No mínimo ou abaixo" : "Em stock"}
                            >
                              <span className={cn("size-1.5 rounded-full", a.stock <= 0 ? "bg-danger" : a.stock <= a.qtd_minima ? "bg-accent" : "bg-ok")} />
                              {qtd(a.stock)} {a.unidade ?? "un"}
                            </span>
                          )
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {/* Barra de funções (teclas do KIGEST POS) */}
          <div className="mt-3 grid grid-cols-[repeat(auto-fit,minmax(68px,1fr))] gap-2 max-md:hidden [@media(max-height:760px)]:mt-2 [@media(max-height:760px)]:gap-1.5">
            {teclas.map((t) => (
              <button
                key={t.label}
                onClick={t.on}
                title={t.k ? `${t.label} (${t.k})` : t.label}
                className="relative flex flex-col items-center justify-center gap-1 rounded-xl border border-line bg-surface px-1 pb-2 pt-3.5 text-[11.5px] text-ink-2 shadow-card transition hover:border-brand/50 hover:text-ink active:scale-[0.97] [@media(max-height:760px)]:gap-0.5 [@media(max-height:760px)]:pb-1.5 [@media(max-height:760px)]:pt-3"
              >
                {t.k && <kbd className="absolute right-1 top-1 px-1 py-px text-[9.5px] text-ink-3">{t.k}</kbd>}
                {t.icon}
                <span className="text-center leading-tight">{t.label}</span>
              </button>
            ))}
          </div>
          {/* telemóvel: a venda em curso fica numa barra em baixo */}
          <div className="mt-2 grid grid-cols-[minmax(0,1fr)_auto] gap-2 md:hidden">
            <button
              onClick={() => setVista("venda")}
              className="flex min-w-0 items-center gap-2.5 rounded-xl border border-line bg-surface px-3 py-2.5 text-left shadow-card active:scale-[0.99]"
            >
              <span className="relative grid size-9 shrink-0 place-items-center rounded-lg bg-brand-50 text-brand">
                <ShoppingBasket className="size-5" />
                {linhas.length > 0 && (
                  <span className="num absolute -right-1.5 -top-1.5 grid min-w-5 place-items-center rounded-full bg-accent px-1 text-[11px] font-semibold text-on-accent">{linhas.length}</span>
                )}
              </span>
              <span className="min-w-0 leading-tight">
                <span className="block truncate text-[12px] text-ink-3">
                  {nomeVenda ?? "Venda"} {atual + 1}/{abertas.length} · {qtd(totais.itens)} {totais.itens === 1 ? "item" : "itens"}
                </span>
                <span className="num block truncate text-[17px] font-semibold">{kz(totais.total)}</span>
              </span>
            </button>
            <Button size="lg" variant="accent" className="h-auto px-5 text-[16px]" onClick={abrirPagamento} disabled={!linhas.length || caixaFechado}>
              <Banknote className="size-5" /> Pagar
            </Button>
          </div>
        </section>

        {/* Carrinho */}
        <aside
          className={cn(
            "flex w-full shrink-0 flex-col border-line bg-surface md:w-[340px] md:border-l lg:w-[380px] xl:w-[420px]",
            vista === "artigos" && "max-md:hidden",
          )}
        >
          {/* vendas em paralelo: ‹ Venda 2/3 › e + para abrir outra (Alt+← / Alt+→ / Alt+N) */}
          <div className="flex items-center gap-1 border-b border-line px-2 py-1.5 sm:px-3">
            <Button variant="ghost" size="sm" className="px-2 md:hidden" onClick={() => setVista("artigos")} title="Voltar aos artigos">
              <LayoutGrid className="size-4" /> Artigos
            </Button>
            <button
              className="grid size-9 place-items-center rounded-lg text-ink-2 hover:bg-sunken disabled:opacity-30"
              disabled={atual === 0}
              onClick={() => irParaVenda(atual - 1)}
              aria-label="Venda anterior"
              title="Venda anterior (Alt+←)"
            >
              <ChevronLeft className="size-5" />
            </button>
            <button
              className="num min-w-[86px] max-w-44 truncate rounded-md px-1.5 py-1 text-center text-[13px] font-medium hover:bg-sunken"
              aria-live="polite"
              onClick={() => setRenomear(true)}
              title="Dar um nome a esta venda (mesa, cliente…)"
            >
              {nomeVenda ? `${nomeVenda} · ${atual + 1}/${abertas.length}` : `Venda ${atual + 1}/${abertas.length}`}
            </button>
            <button
              className="grid size-9 place-items-center rounded-lg text-ink-2 hover:bg-sunken disabled:opacity-30"
              disabled={atual >= abertas.length - 1}
              onClick={() => irParaVenda(atual + 1)}
              aria-label="Venda seguinte"
              title="Venda seguinte (Alt+→)"
            >
              <ChevronRight className="size-5" />
            </button>
            <Button variant="secondary" size="sm" className="ml-auto" onClick={novaVenda} title="Abrir outra venda em paralelo (Alt+N): esta fica à espera">
              <Plus className="size-4" /> Nova
            </Button>
          </div>
          <div className="flex items-center gap-3 border-b border-line px-3 py-3 sm:px-5 sm:py-3.5">
            <button onClick={() => setEscolherCliente("lista")} className="flex min-w-0 flex-1 items-center gap-3 text-left">
              <div className="grid size-9 shrink-0 place-items-center rounded-full bg-brand-50 text-brand">
                <UserRound className="size-[18px]" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{cliente.nome}</div>
                <div className="flex items-center gap-2 text-[12px] text-ink-3">
                  NIF {cliente.nif}
                  {pontos.data && cliente.id !== 1 && (
                    <span className="inline-flex items-center gap-0.5 font-medium text-accent-ink" title={`Vale ${kz(pontos.data.valor)}`}>
                      <Star className="size-3" />
                      {numero(pontos.data.pontos, 0)} pontos
                    </span>
                  )}
                </div>
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
                  <div className="mx-auto mb-3 grid size-12 place-items-center rounded-2xl bg-brand-50 text-brand">
                    <ScanBarcode className="size-6" />
                  </div>
                  <p className="font-medium text-ink-2">Leia um código de barras ou toque num artigo.</p>
                  <ul className={cn("mx-auto mt-4 w-fit space-y-1.5 text-left text-[12.5px]", tactil && "hidden")}>
                    {(
                      [
                        ["Enter", "ler o código (3*código = 3 unidades)"],
                        ["F2", "lista de artigos"],
                        ["F4", "escolher o cliente"],
                        ["F12", "pagamento"],
                      ] as const
                    ).map(([k, d]) => (
                      <li key={k} className="flex items-center gap-2">
                        <kbd className="min-w-[44px] text-center">{k}</kbd> {d}
                      </li>
                    ))}
                  </ul>
                </div>
              </div>
            ) : (
              <ul>
                {linhas.map((l, i) => {
                  const c = totais.linhas[i];
                  return (
                    <li key={l.key} onClick={() => setSel(i)} className={cn("relative cursor-pointer border-b border-line/70 px-3 py-2.5 sm:px-5", idx === i && "bg-brand-50/60")}>
                      {idx === i && <span className="absolute inset-y-0 left-0 w-1 bg-brand" />}
                      <div className="flex items-start gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="line-clamp-2 break-words font-medium">
                            <span className="num mr-1.5 text-[12px] font-normal text-ink-3">{i + 1}.</span>
                            {l.descricao || l.artigo.descricao}
                          </div>
                          <div className="num text-[12px] text-ink-3">
                            {kzNum(l.pvp ?? comIvaArt(l.preco, l.artigo))} / {l.artigo.unidade ?? "UN"} · IVA {l.artigo.taxa_pct}%
                            {l.desconto ? ` · \u2212${numero(l.desconto, 2)}%` : ""}
                          </div>
                          {l.regra && <div className="mt-0.5 inline-block rounded bg-accent-50 px-1.5 py-px text-[11.5px] font-medium text-accent-ink">{l.regra}</div>}
                          {/* preço líquido (já com os descontos) abaixo do custo médio: só aviso, a venda continua possível */}
                          {!!c && l.artigo.custo_medio > 0 && l.quantidade > 0 && c.liquido < Math.round(l.artigo.custo_medio * l.quantidade) && (
                            <div className="mt-0.5 text-[12px] font-medium text-danger">Abaixo do custo médio ({kzNum(l.artigo.custo_medio)} s/ IVA)</div>
                          )}
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
                        {/* total e quantidade à direita: cada linha ocupa duas alturas em vez de três */}
                        <div className="flex shrink-0 flex-col items-end gap-1.5">
                          <div className="num font-semibold">{kzNum(c?.total)}</div>
                          <div className="flex items-center gap-1.5">
                        <button
                          className="grid size-8 place-items-center rounded-md border border-line bg-surface hover:bg-sunken"
                          aria-label="Menos uma unidade"
                          onClick={(e) => {
                            e.stopPropagation();
                            if (l.quantidade > 1) comPermissao("pos.quantidade", "Alterar quantidade", () => atualizar(i, { quantidade: +(l.quantidade - 1).toFixed(3) }));
                          }}
                        >
                          <Minus className="size-3.5" />
                        </button>
                        <button
                          className="num h-8 min-w-12 rounded-md border border-line bg-surface px-2 text-center font-medium hover:bg-sunken"
                          aria-label="Quantidade (abre o teclado)"
                          onClick={(e) => {
                            e.stopPropagation();
                            setSel(i);
                            setTeclado({ modo: "qtd" });
                          }}
                        >
                          {qtd(l.quantidade)}
                        </button>
                        <button
                          className="grid size-8 place-items-center rounded-md border border-line bg-surface hover:bg-sunken"
                          aria-label="Mais uma unidade"
                          onClick={(e) => {
                            e.stopPropagation();
                            atualizar(i, { quantidade: +(l.quantidade + 1).toFixed(3) });
                          }}
                        >
                          <Plus className="size-3.5" />
                        </button>
                        <button
                          className="ml-1 grid size-8 place-items-center rounded-md text-ink-3 hover:bg-danger-50 hover:text-danger"
                          aria-label="Apagar esta linha"
                          title="Apagar esta linha"
                          onClick={(e) => {
                            e.stopPropagation();
                            apagarLinha(i);
                          }}
                        >
                          <Trash2 className="size-4" />
                        </button>
                          </div>
                        </div>
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          <div className="flex gap-1.5 overflow-x-auto border-t border-line px-2 py-2 md:hidden">
            {teclas.map((t) => (
              <button
                key={t.label}
                onClick={t.on}
                className="flex min-w-[72px] shrink-0 flex-col items-center gap-1 rounded-lg border border-line bg-surface px-2 py-2 text-[11px] text-ink-2 active:scale-95"
              >
                {t.icon}
                <span className="whitespace-nowrap leading-tight">{t.label}</span>
              </button>
            ))}
          </div>
          <div className="border-t border-line bg-bg/50 px-3 py-3 sm:px-5 sm:py-4">
            <div className="space-y-1 text-[13px] text-ink-2">
              <Linha2 k={`Subtotal (${qtd(totais.itens)} ${totais.itens === 1 ? "item" : "itens"})`} v={kz(totais.bruto)} />
              {totais.desc > 0 && <Linha2 k="Descontos" v={`\u2212${kz(totais.desc)}`} />}
              {cupao && (
                <div className="flex items-center justify-between rounded-md bg-accent-50 px-2 py-1 text-accent-ink">
                  <span>
                    Cupão <b>{cupao.codigo}</b> ({cupao.tipo === "PCT" ? `\u2212${numero(cupao.valor, 2)}%` : `\u2212${kz(cupao.valor)}`})
                  </span>
                  <button className="text-[12px] underline" onClick={retirarCupao}>
                    retirar
                  </button>
                </div>
              )}
              <Linha2 k="IVA" v={kz(totais.iva)} />
            </div>
            <div className="mt-2 flex items-baseline justify-between">
              <span className="text-ink-2">Total</span>
              <span className="num text-[30px] font-semibold tracking-tight">{kz(totais.total)}</span>
            </div>
            <div className="mt-3 grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-2">
              <Button variant="secondary" size="lg" className="px-3" onClick={terminarVenda} disabled={!linhas.length} title="Cancelar a venda em curso">
                <XCircle className="size-4" /> Cancelar
              </Button>
              <Button size="lg" variant="accent" className="text-[16px]" onClick={abrirPagamento} disabled={!linhas.length || caixaFechado} title="Pagamento (F12)">
                <Banknote className="size-5" /> Pagar
                <kbd className="ml-1 border-on-accent/25 bg-white/35 text-[10.5px] text-on-accent">F12</kbd>
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
            ? `Preço actual: ${kz(linhaAtiva ? (linhaAtiva.pvp ?? comIvaArt(linhaAtiva.preco, linhaAtiva.artigo)) : 0)}`
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
                ? ((linhaAtiva.pvp ?? comIvaArt(linhaAtiva.preco, linhaAtiva.artigo)) / 100).toFixed(2).replace(".", ",")
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
        open={pedirCupao}
        titulo="Cupão de desconto"
        label="Código do cupão"
        valorInicial=""
        onConfirm={aplicarCupao}
        onClose={() => { setPedirCupao(false); focar(); }}
      />

      <TextoModal
        open={!!pedirSerie}
        titulo="N.º de série / IMEI"
        label={pedirSerie ? `${pedirSerie.artigo.descricao}${pedirSerie.quantidade > 1 ? ` (faltam ${pedirSerie.quantidade})` : ""}` : ""}
        valorInicial=""
        onConfirm={(v) => {
          if (!pedirSerie) return;
          if (!v.trim()) return toast.error("Leia ou escreva o n.º de série (fica na factura).");
          const { artigo, quantidade } = pedirSerie;
          adicionar(artigo, 1, `${artigo.descricao} · ${notaDaLinha(artigo, v)}`, true);
          // uma linha por unidade: cada uma com o seu número
          setPedirSerie(quantidade > 1 ? { artigo, quantidade: quantidade - 1 } : null);
          if (quantidade <= 1) focar();
        }}
        onClose={() => {
          setPedirSerie(null);
          focar();
        }}
      />

      <TextoModal
        open={renomear}
        titulo={ramo.vendasComNome ? `${ramo.vendasComNome} / nome da venda` : "Nome da venda"}
        label="Ex.: Mesa 4, Balcão, Sr. João (vazio = sem nome)"
        valorInicial={nomeVenda ?? ""}
        onConfirm={(v) => {
          setNomeVenda(v.trim() || null);
          setRenomear(false);
          focar();
        }}
        onClose={() => {
          setRenomear(false);
          focar();
        }}
      />

      <TextoModal
        open={suspender}
        titulo="Suspender documento"
        label="Identificação (cliente, mesa, observação)"
        valorInicial={nomeVenda ?? (cliente.id === 1 ? "" : cliente.nome)}
        onConfirm={gravarSuspenso}
        onClose={() => { setSuspender(false); focar(); }}
      />

      {verSuspensos && <SuspensosModal onClose={() => { setVerSuspensos(false); focar(); }} onAbrir={abrirSuspenso} />}
      {desconhecido && (
        <CodigoDesconhecido
          codigo={desconhecido}
          onAssociado={(a) => vender(a, a.quantidade_lida ?? 1)}
          onClose={() => {
            setDesconhecido(null);
            focar();
          }}
        />
      )}
      {verProformas && <ProformasModal onClose={() => { setVerProformas(false); focar(); }} onAbrir={abrirProforma} />}

      <PinModal
        pedido={pedidoPin}
        onClose={() => { setPedidoPin(null); focar(); }}
        onOk={(p) => {
          if (pedidoPin && ["pos.desconto", "pos.alterar_preco", "stock.sem_stock"].includes(pedidoPin.permissao)) setPin(p);
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
          cupao={cupao?.codigo ?? null}
          origemId={origem?.id ?? null}
          pin={pin}
          modos={tabelas.data?.modos_pagamento ?? []}
          pontos={cliente.id !== 1 && pontos.data?.pode_pagar ? { ...pontos.data, valor_ponto: fid?.valor_ponto ?? 0 } : null}
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
    <div className="flex justify-between gap-3">
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
  cupao,
  origemId,
  pin,
  modos,
  pontos,
  onClose,
  onEmitido,
}: {
  total: number;
  cliente: Entidade;
  linhas: Linha[];
  descGlobal: DescGlobal;
  cupao: string | null;
  origemId: number | null;
  pin: string | null;
  modos: { codigo: string; descricao: string }[];
  /** saldo de pontos do cliente, quando pode pagar com eles */
  pontos: (SaldoPontos & { valor_ponto: number }) | null;
  onClose: () => void;
  onEmitido: (d: DocCompleto) => void;
}) {
  const [tipo, setTipo] = useState<"FR" | "FT">("FR");
  const [valores, setValores] = useState<Record<string, string>>({ NU: (total / 100).toFixed(2).replace(".", ",") });
  // numerário noutras moedas: só as escolhidas para o POS (na instalação ou em Tabelas → Moedas), com câmbio do dia;
  // o troco sai em kwanzas
  const moedas = useQuery({ queryKey: ["moedas"], queryFn: () => call<{ codigo: string; simbolo: string; cambio: number; ativo: number; pos?: number }[]>("moedas.listar"), staleTime: 300_000 })
    .data?.filter((m) => m.codigo !== "AOA" && m.ativo && m.pos && m.cambio > 0) ?? [];
  const [estrangeiras, setEstrangeiras] = useState<Record<string, string>>({});
  const emMoeda = moedas.reduce((s, m) => s + Math.round(parseKz(estrangeiras[m.codigo] || "0") * m.cambio), 0);
  const [busy, setBusy] = useState(false);
  const nuRef = useRef<HTMLInputElement>(null);

  const pago = Object.values(valores).reduce((s, v) => s + parseKz(v || "0"), 0) + emMoeda;
  const falta = Math.max(0, total - pago);
  const troco = Math.max(0, pago - total);
  // pagamento com pontos (SAF-T «DE»): até ao saldo e ao total; o resto fica em numerário
  const emPontos = parseKz(valores.DE || "0");
  const pontosUsados = pontos && pontos.valor_ponto > 0 ? Math.ceil(emPontos / pontos.valor_ponto) : 0;
  function usarPontos() {
    if (!pontos) return;
    const outros = Object.entries(valores).filter(([m]) => m !== "DE" && m !== "NU").reduce((s, [, v]) => s + parseKz(v || "0"), 0);
    const usar = Math.max(0, Math.min(pontos.valor, total - outros));
    const fmt = (c: number) => (c / 100).toFixed(2).replace(".", ",");
    setValores({ ...valores, DE: fmt(usar), NU: fmt(Math.max(0, total - outros - usar)) });
    // o foco volta ao numerário: Enter emite (e não volta a carregar no botão)
    requestAnimationFrame(() => nuRef.current?.select());
  }

  useEffect(() => {
    nuRef.current?.select();
  }, []);

  async function emitir() {
    if (tipo === "FR" && falta > 0) return toast.error(`Falta pagar ${kz(falta)}.`);
    if (tipo === "FR" && pontos && emPontos > pontos.valor) return toast.error(`O cliente só tem ${kz(pontos.valor)} em pontos.`);
    setBusy(true);
    try {
      const d = await call<DocCompleto>("documentos.emitir", {
        tipo,
        entidade_id: cliente.id,
        linhas: linhas.map((l) => ({
          artigo_id: l.artigo.id,
          quantidade: l.quantidade,
          preco_unitario: l.preco,
          // com «preços com IVA incluído» o servidor parte do PVP da linha
          pvp_unitario: l.pvp ?? undefined,
          desconto_pct: l.desconto,
          descricao: l.descricao ?? undefined,
          lote: l.lote || undefined,
        })),
        desconto_global_pct: descGlobal.tipo === "pct" && descGlobal.valor > 0 ? descGlobal.valor : undefined,
        desconto_global_valor: descGlobal.tipo === "valor" && descGlobal.valor > 0 ? descGlobal.valor : undefined,
        doc_origem_id: origemId ?? undefined,
        autorizacao_pin: pin ?? undefined,
        cupao: cupao ?? undefined,
        pagamentos:
          tipo === "FR"
            ? [
                ...Object.entries(valores).map(([modo, v]) => ({ modo, valor: parseKz(v || "0") })),
                ...moedas.filter((m) => parseKz(estrangeiras[m.codigo] || "0") > 0).map((m) => ({ modo: "NU", valor: 0, moeda: m.codigo, valor_moeda: parseKz(estrangeiras[m.codigo]) })),
              ]
            : [],
      });
      toast.success(`${d.documento.numero_doc} emitido.`);
      onEmitido(d);
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  // numerário rápido (o que falta depois dos pontos): o valor certo e o que o cliente costuma entregar — as notas de
  // 500, 1 000, 1 500, 2 000, 5 000 e 10 000 Kz e o total arredondado a essas notas; só o que chega, até 6 botões
  const restante = Math.max(0, total - emPontos);
  const NOTAS_KZ = [500, 1000, 1500, 2000, 5000, 10000];
  const rapidos =
    restante > 0
      ? [restante, ...[500, 1000, 2000, 5000, 10000].map((n) => Math.ceil(restante / (n * 100)) * n * 100), ...NOTAS_KZ.map((n) => n * 100)]
          .filter((v) => v >= restante)
          .sort((x, y) => x - y)
          .filter((v, i, a) => a.indexOf(v) === i)
          .slice(0, 6)
      : [];

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
            <kbd className="ml-1 border-on-accent/25 bg-white/35 text-[10.5px] text-on-accent">Enter</kbd>
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
        <div className="mb-4 flex items-center justify-between gap-3 rounded-xl bg-brand-700 px-4 py-3 text-white sm:mb-5 sm:px-5 sm:py-4">
          <div>
            <div className="text-[13px] text-white/70">Total a pagar</div>
            <div className="num text-[26px] font-semibold tracking-tight sm:text-[32px]">{kz(total)}</div>
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
          <div className="grid gap-4 sm:grid-cols-2 sm:gap-6">
            {/* uma linha por forma de pagamento: cabe no ecrã sem rolar até ao botão de emitir */}
            <div className="space-y-2">
              {modos
                // compensação de saldo: só para o cliente identificado que tem crédito (adiantamentos, notas de crédito)
                .filter((m) => m.codigo !== "CS" || (cliente.id !== 1 && (cliente.credito ?? 0) > 0))
                .map((m) => (
                  <label key={m.codigo} className="grid grid-cols-[minmax(0,1fr)_minmax(110px,150px)] items-center gap-3">
                    <span className="flex min-w-0 items-center gap-2 text-[13px] text-ink-2">
                      {m.codigo === "NU" ? <Banknote className="size-4 shrink-0 text-ink-3" /> : <CreditCard className="size-4 shrink-0 text-ink-3" />}
                      <span className="line-clamp-2 leading-tight" title={m.descricao}>
                        {m.codigo === "CS" ? `Crédito do cliente (disponível ${kz(cliente.credito ?? 0)})` : m.descricao}
                      </span>
                    </span>
                    <Input
                      ref={m.codigo === "NU" ? nuRef : undefined}
                      className={cn("num h-9 text-right", m.codigo === "NU" && "font-medium")}
                      inputMode="decimal"
                      value={valores[m.codigo] ?? ""}
                      placeholder="0,00"
                      onChange={(e) => setValores({ ...valores, [m.codigo]: e.target.value })}
                      onFocus={(e) => e.target.select()}
                    />
                  </label>
                ))}
              {pontos && (
                <div className="mt-3 rounded-xl border border-accent/40 bg-accent-50/60 p-3">
                  <div className="mb-2 flex items-center justify-between gap-2 text-[13px]">
                    <span className="flex items-center gap-1.5 font-medium text-accent-ink">
                      <Star className="size-4" />
                      {numero(pontos.pontos, 0)} pontos · {kz(pontos.valor)}
                    </span>
                    <Button type="button" size="sm" variant="secondary" onClick={usarPontos}>
                      Usar pontos
                    </Button>
                  </div>
                  <label className="grid grid-cols-[minmax(0,1fr)_150px] items-center gap-3">
                    <span className="text-[13px] text-ink-2">{pontosUsados > 0 ? `Pago com ${numero(pontosUsados, 0)} pontos` : "Pago com pontos"}</span>
                    <Input
                      className="num h-9 text-right"
                      value={valores.DE ?? ""}
                      placeholder="0,00"
                      onChange={(e) => setValores({ ...valores, DE: e.target.value })}
                      onFocus={(e) => e.target.select()}
                    />
                  </label>
                </div>
              )}
            </div>
            <div className="flex flex-col gap-3">
              <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Numerário rápido</div>
              <div className="grid grid-cols-3 gap-2">
                {rapidos.map((v) => (
                  <Button type="button" key={v} variant="secondary" size="lg" className="num" onClick={() => setValores({ ...(emPontos > 0 ? { DE: valores.DE } : {}), NU: (v / 100).toFixed(2).replace(".", ",") })}>
                    {kzNum(v)}
                  </Button>
                ))}
              </div>
              {moedas.length > 0 && (
                <div className="space-y-2 rounded-xl border border-line p-3">
                  <div className="text-[12px] font-medium uppercase tracking-wide text-ink-2">Noutra moeda (troco em Kz)</div>
                  {moedas.map((m) => {
                    const v = parseKz(estrangeiras[m.codigo] || "0");
                    return (
                      <div key={m.codigo}>
                        <label className="grid grid-cols-[52px_minmax(0,1fr)] items-center gap-2">
                          <span className="text-[13px] font-medium">{m.simbolo || m.codigo}</span>
                          <Input
                            className="num h-9 text-right"
                            inputMode="decimal"
                            placeholder="0,00"
                            value={estrangeiras[m.codigo] ?? ""}
                            onChange={(e) => {
                              setEstrangeiras({ ...estrangeiras, [m.codigo]: e.target.value });
                              // o numerário em Kz que estava sugerido deixa de ser preciso
                              if (valores.NU && parseKz(valores.NU) === total) setValores({ ...valores, NU: "" });
                            }}
                            onFocus={(e) => e.target.select()}
                          />
                        </label>
                        <div className="mt-1 flex items-center justify-between gap-2 pl-[60px] text-[11.5px] text-ink-3">
                          <span>
                            ×{m.cambio.toLocaleString("pt-PT", { minimumFractionDigits: 2 })} = {kz(Math.round(v * m.cambio))}
                          </span>
                          <span className="flex gap-1">
                            {[20, 50, 100].map((n) => (
                              <button
                                key={n}
                                type="button"
                                className="rounded border border-line px-1.5 py-0.5 hover:bg-sunken"
                                onClick={() => {
                                  setEstrangeiras({ ...estrangeiras, [m.codigo]: String(n) });
                                  if (valores.NU && parseKz(valores.NU) === total) setValores({ ...valores, NU: "" });
                                }}
                              >
                                {n}
                              </button>
                            ))}
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
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
            <Field label="NIF" hint={nifValido(f.nif) ? undefined : "10 algarismos ou n.º do BI (ex.: 001234567LA041)"}>
              <Input value={f.nif} onChange={(e) => setF({ ...f, nif: e.target.value })} className={cn(!nifValido(f.nif) && "border-danger")} />
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
        <Field label="NIF" hint={nifValido(f.nif) ? undefined : "10 algarismos ou n.º do BI (ex.: 001234567LA041)"}>
          <Input value={f.nif} onChange={(e) => setF({ ...f, nif: e.target.value })} className={cn(!nifValido(f.nif) && "border-danger")} />
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

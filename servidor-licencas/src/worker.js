// Servidor de licenças do KanGest — Cloudflare Worker + D1, sem dependências.
//
// Público (chamado pelo KanGest):
//   POST /v1/registo    {nif, empresa, email, telefone, maquina, versao}  → envia um código de 6 algarismos
//   POST /v1/confirmar  {nif, maquina, codigo}                            → regista e devolve a licença (experiência)
//   POST /v1/renovar    {nif, maquina, versao}                            → devolve a licença mais recente
//   GET  /v1/planos                                                       → planos e preços (para o site)
// Painel do produtor (Authorization: Bearer ADMIN_TOKEN):
//   GET /admin (página)   GET /admin/api/resumo   GET /admin/api/cliente?nif=
//   POST /admin/api/pagamento · emitir · plano · cliente · maquina · codigo
//
// Variáveis: DB (D1), CHAVE_PRIVADA (secreto, PEM pkcs8 Ed25519), ADMIN_TOKEN (secreto),
//            RESEND_API_KEY + EMAIL_REMETENTE (opcionais: sem eles, o código aparece no painel).
// As licenças têm o mesmo formato do scripts/licencas/kangest-licencas.mjs: «KG1.<json base64url>.<assinatura>».
import { PAINEL } from "./painel.js";

const VERSAO = "1.0.0";
const DIAS_EXPERIENCIA = 30;
const TOLERANCIA = 15;
const CODIGO_MINUTOS = 30;
const MAX_PEDIDOS_HORA = 5;
const MAX_TENTATIVAS = 5;
const PLANOS_POSTOS = { ESSENCIAL: 1, COMERCIO: 3, EMPRESA: 0 };

// ------------------------------------------------------------------ utilitários

const enc = new TextEncoder();

function b64url(bytes) {
  const b = new Uint8Array(bytes);
  let s = "";
  for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function sha256(texto) {
  const h = await crypto.subtle.digest("SHA-256", enc.encode(texto));
  return [...new Uint8Array(h)].map((x) => x.toString(16).padStart(2, "0")).join("");
}

/** Dia de hoje em Luanda (UTC+1), AAAA-MM-DD. */
export const hoje = () => new Date(Date.now() + 3600_000).toISOString().slice(0, 10);
const agora = () => new Date().toISOString().slice(0, 19) + "Z";

function somarDias(dia, n) {
  const d = new Date(dia + "T12:00:00Z");
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/** Último dia de um período de N meses que começa em `dia` (13/11 + 1 mês → 12/12). */
function somarMeses(dia, meses) {
  const d = new Date(dia + "T12:00:00Z");
  d.setUTCMonth(d.getUTCMonth() + meses);
  d.setUTCDate(d.getUTCDate() - 1);
  return d.toISOString().slice(0, 10);
}

const dmy = (d) => (d ? `${d.slice(8, 10)}/${d.slice(5, 7)}/${d.slice(0, 4)}` : "—");
const eDia = (d) => typeof d === "string" && /^\d{4}-\d{2}-\d{2}$/.test(d) && !isNaN(Date.parse(d + "T00:00:00Z"));

const json = (dados, estado = 200, extra = {}) =>
  new Response(JSON.stringify(dados), { status: estado, headers: { "content-type": "application/json; charset=utf-8", ...extra } });
const erro = (mensagem, estado = 400) => json({ ok: false, erro: mensagem }, estado);

class Falha extends Error {
  constructor(mensagem, estado = 400) {
    super(mensagem);
    this.estado = estado;
  }
}

const limpaNif = (v) => String(v ?? "").replace(/[^0-9a-z]/gi, "").toUpperCase();
const limpaMaquina = (v) => String(v ?? "").trim().toUpperCase();
const texto = (v, max = 120) => String(v ?? "").trim().replace(/\s+/g, " ").slice(0, max);

function validarNif(nif) {
  if (!/^[0-9A-Z]{9,14}$/.test(nif)) throw new Falha("O NIF não é válido.");
  return nif;
}
function validarMaquina(m) {
  if (!/^[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}$/.test(m)) throw new Falha("Identificador do computador inválido. Actualize o KanGest.");
  return m;
}

function mascararEmail(email) {
  const [u, d] = String(email).split("@");
  if (!d) return "";
  return `${u.slice(0, 2)}${"•".repeat(Math.max(1, u.length - 2))}@${d}`;
}

// ------------------------------------------------------------------ assinatura

const chaves = new Map();
async function chavePrivada(env) {
  const pem = env.CHAVE_PRIVADA;
  if (!pem) throw new Falha("O servidor não tem a chave de assinatura configurada.", 500);
  if (!chaves.has(pem)) {
    const der = Uint8Array.from(atob(pem.replace(/-----[^-]+-----|\s/g, "")), (c) => c.charCodeAt(0));
    chaves.set(pem, await crypto.subtle.importKey("pkcs8", der, { name: "Ed25519" }, false, ["sign"]));
  }
  return chaves.get(pem);
}

async function assinar(env, lic) {
  const corpo = b64url(enc.encode(JSON.stringify(lic)));
  const assinatura = await crypto.subtle.sign({ name: "Ed25519" }, await chavePrivada(env), enc.encode(corpo));
  return `KG1.${corpo}.${b64url(assinatura)}`;
}

function idLicenca() {
  const b = crypto.getRandomValues(new Uint8Array(5));
  return "LIC-" + [...b].map((x) => x.toString(16).padStart(2, "0")).join("").toUpperCase();
}

/** Emite (ou reaproveita, se já existe uma igual) e regista uma licença. */
async function emitir(env, { nif, empresa, plano, postos, inicio, fim, tolerancia = TOLERANCIA, maquina = null, modulos = [], origem }) {
  const igual = await env.DB.prepare(
    `SELECT id, chave FROM licencas WHERE nif = ? AND empresa = ? AND IFNULL(maquina,'') = ? AND plano = ? AND postos = ? AND inicio = ? AND fim = ?
       AND tolerancia = ? ORDER BY emitida_em DESC LIMIT 1`,
  ).bind(nif, empresa, maquina ?? "", plano, postos, inicio, fim, tolerancia).first();
  if (igual) return { id: igual.id, chave: igual.chave };
  const lic = { v: 1, id: idLicenca(), nif, empresa, plano, postos, modulos, inicio, fim, tolerancia, emitida: hoje(), ...(maquina ? { maquina } : {}) };
  const chave = await assinar(env, lic);
  await env.DB.prepare(
    `INSERT INTO licencas (id, nif, empresa, maquina, plano, postos, inicio, fim, tolerancia, emitida_em, origem, chave) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)`,
  ).bind(lic.id, nif, empresa, maquina, plano, postos, inicio, fim, tolerancia, agora(), origem, chave).run();
  return { id: lic.id, chave };
}

// ------------------------------------------------------------------ licença de um cliente

async function postosDoPlano(env, plano) {
  const p = await env.DB.prepare("SELECT postos FROM planos WHERE codigo = ?").bind(plano).first();
  return p ? p.postos : PLANOS_POSTOS[plano] ?? 1;
}

/** O que o cliente tem direito neste computador: assinatura paga, experiência ou nada. */
async function licencaActual(env, cliente, maquina) {
  const dia = hoje();
  if (cliente.pago_ate && cliente.plano) {
    const limite = somarDias(cliente.pago_ate, TOLERANCIA);
    if (dia > limite) {
      return { licenca: null, estado: "EXPIRADA", fim: cliente.pago_ate, mensagem: `A assinatura terminou a ${dmy(cliente.pago_ate)}. Para continuar a emitir, faça o pagamento da renovação.` };
    }
    const ultimo = await env.DB.prepare("SELECT MIN(registado_em) AS d FROM pagamentos WHERE nif = ?").bind(cliente.nif).first();
    const inicio = (ultimo?.d ?? cliente.criado_em).slice(0, 10);
    const postos = cliente.postos ?? (await postosDoPlano(env, cliente.plano));
    const l = await emitir(env, { nif: cliente.nif, empresa: cliente.empresa, plano: cliente.plano, postos, inicio, fim: cliente.pago_ate, maquina, origem: "RENOVACAO" });
    return { licenca: l.chave, id: l.id, estado: "ACTIVA", plano: cliente.plano, fim: cliente.pago_ate, mensagem: `Licença ${cliente.plano} válida até ${dmy(cliente.pago_ate)}.` };
  }
  if (cliente.experiencia_ate && dia <= cliente.experiencia_ate) {
    const inicio = cliente.criado_em.slice(0, 10);
    const l = await emitir(env, { nif: cliente.nif, empresa: cliente.empresa, plano: "EXPERIENCIA", postos: 0, inicio, fim: cliente.experiencia_ate, tolerancia: 0, maquina, origem: "EXPERIENCIA" });
    return { licenca: l.chave, id: l.id, estado: "EXPERIENCIA", plano: "EXPERIENCIA", fim: cliente.experiencia_ate, mensagem: `Experiência registada até ${dmy(cliente.experiencia_ate)}. Para continuar depois, escolha um plano.` };
  }
  return {
    licenca: null,
    estado: "EXPIRADA",
    fim: cliente.experiencia_ate,
    mensagem: `A experiência do NIF ${cliente.nif} terminou a ${dmy(cliente.experiencia_ate)}. Para continuar a emitir, escolha um plano e faça o pagamento.`,
  };
}

// ------------------------------------------------------------------ email

async function enviarCodigo(env, { email, empresa, codigo }) {
  if (!env.RESEND_API_KEY || !env.EMAIL_REMETENTE || !email) return false;
  const corpo = {
    from: env.EMAIL_REMETENTE,
    to: [email],
    subject: `KanGest: código de confirmação ${codigo}`,
    text: `Olá,\n\nO código para confirmar o registo de ${empresa} no KanGest é:\n\n    ${codigo}\n\nEscreva-o no KanGest, em Sistema → Licença e Assinatura. É válido durante ${CODIGO_MINUTOS} minutos.\n\nSe não pediu este código, ignore esta mensagem.\n\nKanGest`,
  };
  try {
    const r = await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: { authorization: `Bearer ${env.RESEND_API_KEY}`, "content-type": "application/json" },
      body: JSON.stringify(corpo),
    });
    return r.ok;
  } catch {
    return false;
  }
}

// ------------------------------------------------------------------ API pública

async function registo(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const maquina = validarMaquina(limpaMaquina(c.maquina));
  const empresa = texto(c.empresa);
  const email = texto(c.email, 160).toLowerCase();
  const telefone = texto(c.telefone, 30);
  if (empresa.length < 2) throw new Falha("Indique o nome da empresa.");
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/.test(email)) throw new Falha("Indique um email válido: é para lá que vai o código de confirmação.");

  const maq = await env.DB.prepare("SELECT bloqueada FROM maquinas WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  if (maq?.bloqueada) throw new Falha("Este computador foi bloqueado para este NIF. Contacte o suporte do KanGest.", 403);
  if (!maq) {
    const cli = await env.DB.prepare("SELECT maquinas_max FROM clientes WHERE nif = ?").bind(nif).first();
    if (cli) {
      const n = await env.DB.prepare("SELECT COUNT(*) AS n FROM maquinas WHERE nif = ? AND bloqueada = 0").bind(nif).first();
      if (n.n >= cli.maquinas_max) {
        throw new Falha(`O NIF ${nif} já tem o KanGest registado em ${n.n} ${n.n === 1 ? "computador" : "computadores"}. Os postos de caixa ligam-se ao servidor e não precisam de registo; para outra instalação, contacte o suporte.`, 409);
      }
    }
  }

  const antes = await env.DB.prepare("SELECT pedidos, criado_em FROM codigos WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  const recente = antes && Date.now() - Date.parse(antes.criado_em) < 3600_000;
  if (recente && antes.pedidos >= MAX_PEDIDOS_HORA) throw new Falha("Pediu demasiados códigos. Espere uma hora e tente de novo.", 429);

  const n = crypto.getRandomValues(new Uint32Array(1))[0] % 1_000_000;
  const codigo = String(n).padStart(6, "0");
  const enviado = await enviarCodigo(env, { email, empresa, codigo });
  await env.DB.prepare(
    `INSERT INTO codigos (nif, maquina, empresa, email, telefone, codigo_hash, codigo_aberto, expira_em, tentativas, pedidos, criado_em)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 1, ?9)
     ON CONFLICT(nif, maquina) DO UPDATE SET empresa = ?3, email = ?4, telefone = ?5, codigo_hash = ?6, codigo_aberto = ?7, expira_em = ?8,
       tentativas = 0, pedidos = ?10, criado_em = ?11`,
  ).bind(
    nif, maquina, empresa, email, telefone,
    await sha256(`${nif}:${maquina}:${codigo}`),
    enviado ? null : codigo,
    new Date(Date.now() + CODIGO_MINUTOS * 60_000).toISOString(),
    agora(),
    recente ? antes.pedidos + 1 : 1,
    recente ? antes.criado_em : agora(),
  ).run();

  return {
    ok: true,
    enviado,
    email: mascararEmail(email),
    mensagem: enviado
      ? `Enviámos um código de 6 algarismos para ${mascararEmail(email)}. É válido durante ${CODIGO_MINUTOS} minutos.`
      : "O pedido ficou registado. O suporte do KanGest vai contactá-lo com o código de confirmação.",
  };
}

async function confirmar(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const maquina = validarMaquina(limpaMaquina(c.maquina));
  const codigo = String(c.codigo ?? "").replace(/\D/g, "");
  const p = await env.DB.prepare("SELECT * FROM codigos WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  if (!p) throw new Falha("Não há nenhum pedido de registo pendente para este computador. Peça um código novo.", 404);
  if (Date.parse(p.expira_em) < Date.now()) throw new Falha("O código expirou. Peça um código novo.", 410);
  if (p.tentativas >= MAX_TENTATIVAS) throw new Falha("Demasiadas tentativas erradas. Peça um código novo.", 429);
  if ((await sha256(`${nif}:${maquina}:${codigo}`)) !== p.codigo_hash) {
    await env.DB.prepare("UPDATE codigos SET tentativas = tentativas + 1 WHERE nif = ? AND maquina = ?").bind(nif, maquina).run();
    const restam = MAX_TENTATIVAS - p.tentativas - 1;
    throw new Falha(restam > 0 ? `Código errado. ${restam === 1 ? "Resta 1 tentativa" : `Restam ${restam} tentativas`}.` : "Código errado. Peça um código novo.");
  }

  const t = agora();
  let cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cliente) {
    await env.DB.prepare("INSERT INTO clientes (nif, empresa, email, telefone, criado_em, experiencia_ate) VALUES (?,?,?,?,?,?)")
      .bind(nif, p.empresa, p.email, p.telefone, t, somarDias(hoje(), DIAS_EXPERIENCIA - 1)).run();
  } else {
    await env.DB.prepare("UPDATE clientes SET empresa = ?, email = ?, telefone = COALESCE(NULLIF(?, ''), telefone) WHERE nif = ?")
      .bind(p.empresa, p.email, p.telefone, nif).run();
  }
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO maquinas (nif, maquina, primeiro_contacto, ultimo_contacto, versao) VALUES (?1, ?2, ?3, ?3, ?4)
       ON CONFLICT(nif, maquina) DO UPDATE SET ultimo_contacto = ?3, versao = COALESCE(?4, versao)`,
    ).bind(nif, maquina, t, texto(c.versao, 20) || null),
    env.DB.prepare("DELETE FROM codigos WHERE nif = ? AND maquina = ?").bind(nif, maquina),
  ]);
  cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  return { ok: true, ...(await licencaActual(env, cliente, maquina)) };
}

async function renovar(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const maquina = validarMaquina(limpaMaquina(c.maquina));
  const maq = await env.DB.prepare("SELECT bloqueada FROM maquinas WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  if (!maq) throw new Falha("Este computador ainda não está registado. Registe-o em Sistema → Licença e Assinatura.", 404);
  if (maq.bloqueada) throw new Falha("Este computador foi bloqueado para este NIF. Contacte o suporte do KanGest.", 403);
  await env.DB.prepare("UPDATE maquinas SET ultimo_contacto = ?, versao = COALESCE(?, versao) WHERE nif = ? AND maquina = ?")
    .bind(agora(), texto(c.versao, 20) || null, nif, maquina).run();
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  return { ok: true, ...(await licencaActual(env, cliente, maquina)) };
}

async function planosPublicos(env) {
  const { results } = await env.DB.prepare("SELECT codigo, nome, postos, preco_mes, preco_ano FROM planos WHERE activo = 1 ORDER BY ordem").all();
  return { ok: true, moeda: "AOA", planos: results.map((p) => ({ ...p, preco_mes: p.preco_mes / 100, preco_ano: p.preco_ano / 100 })) };
}

// ------------------------------------------------------------------ painel do produtor

function autorizado(env, req) {
  const dado = (req.headers.get("authorization") ?? "").replace(/^Bearer\s+/i, "");
  const certo = env.ADMIN_TOKEN ?? "";
  if (certo.length < 24 || dado.length !== certo.length) return false;
  let dif = 0;
  for (let i = 0; i < certo.length; i++) dif |= certo.charCodeAt(i) ^ dado.charCodeAt(i);
  return dif === 0;
}

const estadoCliente = (c, dia = hoje()) =>
  c.pago_ate
    ? c.pago_ate >= dia ? "ACTIVA" : somarDias(c.pago_ate, TOLERANCIA) >= dia ? "TOLERANCIA" : "EXPIRADA"
    : c.experiencia_ate && c.experiencia_ate >= dia ? "EXPERIENCIA" : "EXPIRADA";

async function resumo(env) {
  const [planos, clientes, codigos, pagamentos] = await env.DB.batch([
    env.DB.prepare("SELECT * FROM planos ORDER BY ordem"),
    env.DB.prepare(
      `SELECT c.*, (SELECT COUNT(*) FROM maquinas m WHERE m.nif = c.nif AND m.bloqueada = 0) AS maquinas,
              (SELECT MAX(ultimo_contacto) FROM maquinas m WHERE m.nif = c.nif) AS ultimo_contacto
       FROM clientes c ORDER BY c.criado_em DESC LIMIT 500`,
    ),
    env.DB.prepare("SELECT nif, maquina, empresa, email, telefone, codigo_aberto, expira_em, tentativas, criado_em FROM codigos ORDER BY criado_em DESC LIMIT 100"),
    env.DB.prepare("SELECT p.*, c.empresa FROM pagamentos p LEFT JOIN clientes c ON c.nif = p.nif ORDER BY p.id DESC LIMIT 50"),
  ]);
  const dia = hoje();
  return {
    ok: true,
    hoje: dia,
    email_configurado: !!(env.RESEND_API_KEY && env.EMAIL_REMETENTE),
    planos: planos.results,
    clientes: clientes.results.map((c) => ({ ...c, estado: estadoCliente(c, dia) })),
    codigos: codigos.results.map((c) => ({ ...c, expirado: Date.parse(c.expira_em) < Date.now() })),
    pagamentos: pagamentos.results,
  };
}

async function fichaCliente(env, nif) {
  nif = validarNif(limpaNif(nif));
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cliente) throw new Falha("Cliente não encontrado.", 404);
  const [maquinas, licencas, pagamentos] = await env.DB.batch([
    env.DB.prepare("SELECT * FROM maquinas WHERE nif = ? ORDER BY ultimo_contacto DESC").bind(nif),
    env.DB.prepare("SELECT id, maquina, plano, postos, inicio, fim, tolerancia, emitida_em, origem, chave FROM licencas WHERE nif = ? ORDER BY emitida_em DESC LIMIT 50").bind(nif),
    env.DB.prepare("SELECT * FROM pagamentos WHERE nif = ? ORDER BY id DESC").bind(nif),
  ]);
  return { ok: true, cliente: { ...cliente, estado: estadoCliente(cliente) }, maquinas: maquinas.results, licencas: licencas.results, pagamentos: pagamentos.results };
}

async function registarPagamento(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cliente) throw new Falha("Cliente não encontrado: o cliente regista-se primeiro no KanGest (ou emita uma licença manual).", 404);
  const plano = String(c.plano ?? "").toUpperCase();
  const p = await env.DB.prepare("SELECT * FROM planos WHERE codigo = ?").bind(plano).first();
  if (!p) throw new Falha("Plano desconhecido.");
  const meses = Math.trunc(Number(c.meses));
  if (!(meses >= 1 && meses <= 36)) throw new Falha("Indique entre 1 e 36 meses.");
  const valor = Math.round(Number(c.valor ?? 0) * 100);
  if (!(valor >= 0)) throw new Falha("Valor inválido.");
  const metodo = ["TRANSFERENCIA", "MULTICAIXA", "NUMERARIO", "OUTRO"].includes(c.metodo) ? c.metodo : "OUTRO";
  const dia = hoje();
  // continua a partir do fim pago, se ainda não passou; senão começa hoje
  const desde = cliente.pago_ate && cliente.pago_ate >= dia ? somarDias(cliente.pago_ate, 1) : dia;
  const pagoAte = somarMeses(desde, meses);
  await env.DB.batch([
    env.DB.prepare("INSERT INTO pagamentos (nif, plano, meses, valor, metodo, referencia, pago_ate, registado_em) VALUES (?,?,?,?,?,?,?,?)")
      .bind(nif, plano, meses, valor, metodo, texto(c.referencia, 80) || null, pagoAte, agora()),
    env.DB.prepare("UPDATE clientes SET plano = ?, pago_ate = ? WHERE nif = ?").bind(plano, pagoAte, nif),
  ]);
  // licenças já prontas para cada computador (o KanGest recebe-as sozinho na próxima renovação)
  const { results: maquinas } = await env.DB.prepare("SELECT maquina FROM maquinas WHERE nif = ? AND bloqueada = 0").bind(nif).all();
  const actualizado = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  const licencas = [];
  for (const m of maquinas) licencas.push({ maquina: m.maquina, ...(await licencaActual(env, actualizado, m.maquina)) });
  return { ok: true, pago_ate: pagoAte, licencas, mensagem: `Pagamento registado: ${p.nome} até ${dmy(pagoAte)}.` };
}

async function emitirManual(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const empresa = texto(c.empresa);
  if (empresa.length < 2) throw new Falha("Indique o nome da empresa.");
  const plano = String(c.plano ?? "").toUpperCase();
  const p = await env.DB.prepare("SELECT * FROM planos WHERE codigo = ?").bind(plano).first();
  if (!p) throw new Falha("Plano desconhecido.");
  const inicio = c.inicio || hoje();
  if (!eDia(inicio)) throw new Falha("Data de início inválida.");
  const fim = c.fim || somarMeses(inicio, Math.trunc(Number(c.meses ?? 12)));
  if (!eDia(fim) || fim < inicio) throw new Falha("Data de fim inválida.");
  const maquina = c.maquina ? validarMaquina(limpaMaquina(c.maquina)) : null;
  const postos = c.postos === undefined || c.postos === "" ? p.postos : Math.max(0, Math.trunc(Number(c.postos)));
  const tolerancia = c.tolerancia === undefined || c.tolerancia === "" ? TOLERANCIA : Math.max(0, Math.trunc(Number(c.tolerancia)));
  const l = await emitir(env, { nif, empresa, plano, postos, inicio, fim, tolerancia, maquina, origem: "PAINEL" });
  return { ok: true, ...l, mensagem: `Licença ${l.id} emitida: ${empresa}, ${p.nome}, ${dmy(inicio)} a ${dmy(fim)}.` };
}

async function guardarPlano(env, c) {
  const codigo = String(c.codigo ?? "").toUpperCase();
  if (!/^[A-Z][A-Z0-9_]{2,19}$/.test(codigo) || codigo === "EXPERIENCIA") throw new Falha("Código de plano inválido.");
  const nome = texto(c.nome, 40);
  if (!nome) throw new Falha("Indique o nome do plano.");
  const n = (v) => Math.max(0, Math.round(Number(v ?? 0) * 100));
  await env.DB.prepare(
    `INSERT INTO planos (codigo, nome, postos, preco_mes, preco_ano, activo, ordem) VALUES (?1,?2,?3,?4,?5,?6,?7)
     ON CONFLICT(codigo) DO UPDATE SET nome = ?2, postos = ?3, preco_mes = ?4, preco_ano = ?5, activo = ?6, ordem = ?7`,
  ).bind(codigo, nome, Math.max(0, Math.trunc(Number(c.postos ?? 1))), n(c.preco_mes), n(c.preco_ano), c.activo === false ? 0 : 1, Math.trunc(Number(c.ordem ?? 9))).run();
  return { ok: true, mensagem: `Plano ${nome} guardado.` };
}

async function guardarCliente(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const cli = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cli) throw new Falha("Cliente não encontrado.", 404);
  if (c.experiencia_ate !== undefined && c.experiencia_ate !== null && !eDia(c.experiencia_ate)) throw new Falha("Data de experiência inválida.");
  await env.DB.prepare(
    `UPDATE clientes SET empresa = ?, email = ?, telefone = ?, notas = ?, maquinas_max = ?, postos = ?, experiencia_ate = ? WHERE nif = ?`,
  ).bind(
    texto(c.empresa ?? cli.empresa) || cli.empresa,
    texto(c.email ?? cli.email ?? "", 160) || null,
    texto(c.telefone ?? cli.telefone ?? "", 30) || null,
    texto(c.notas ?? cli.notas ?? "", 1000) || null,
    Math.max(1, Math.trunc(Number(c.maquinas_max ?? cli.maquinas_max))),
    c.postos === "" || c.postos === null ? null : c.postos === undefined ? cli.postos : Math.max(0, Math.trunc(Number(c.postos))),
    c.experiencia_ate === undefined ? cli.experiencia_ate : c.experiencia_ate,
    nif,
  ).run();
  return { ok: true, mensagem: "Cliente actualizado." };
}

async function guardarMaquina(env, c) {
  const nif = validarNif(limpaNif(c.nif));
  const maquina = validarMaquina(limpaMaquina(c.maquina));
  const r = await env.DB.prepare("UPDATE maquinas SET bloqueada = ? WHERE nif = ? AND maquina = ?").bind(c.bloqueada ? 1 : 0, nif, maquina).run();
  if (!r.meta?.changes) throw new Falha("Computador não encontrado.", 404);
  return { ok: true, mensagem: c.bloqueada ? "Computador bloqueado: deixa de receber licenças." : "Computador desbloqueado." };
}

async function apagarCodigo(env, c) {
  await env.DB.prepare("DELETE FROM codigos WHERE nif = ? AND maquina = ?").bind(limpaNif(c.nif), limpaMaquina(c.maquina)).run();
  return { ok: true, mensagem: "Pedido apagado." };
}

// ------------------------------------------------------------------ encaminhamento

const CORS = { "access-control-allow-origin": "*", "access-control-allow-methods": "GET, OPTIONS", "access-control-max-age": "86400" };

async function corpoJson(req) {
  if (Number(req.headers.get("content-length") ?? 0) > 16_384) throw new Falha("Pedido demasiado grande.", 413);
  try {
    const c = await req.json();
    if (c && typeof c === "object" && !Array.isArray(c)) return c;
  } catch { /* em baixo */ }
  throw new Falha("Pedido inválido.");
}

async function tratar(req, env) {
  const url = new URL(req.url);
  const caminho = url.pathname.replace(/\/+$/, "") || "/";
  const m = req.method;

  if (m === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
  if (caminho === "/" && m === "GET") return json({ servico: "Licenças do KanGest", versao: VERSAO });
  if (caminho === "/v1/planos" && m === "GET") return json(await planosPublicos(env), 200, { ...CORS, "cache-control": "public, max-age=300" });
  if (m === "POST" && caminho === "/v1/registo") return json(await registo(env, await corpoJson(req)));
  if (m === "POST" && caminho === "/v1/confirmar") return json(await confirmar(env, await corpoJson(req)));
  if (m === "POST" && caminho === "/v1/renovar") return json(await renovar(env, await corpoJson(req)));

  if (caminho === "/admin" && m === "GET") {
    return new Response(PAINEL, {
      headers: {
        "content-type": "text/html; charset=utf-8",
        "cache-control": "no-store",
        "x-frame-options": "DENY",
        "referrer-policy": "no-referrer",
        "content-security-policy": "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
      },
    });
  }
  if (caminho.startsWith("/admin/api/")) {
    if (!autorizado(env, req)) return erro("Acesso negado.", 401);
    const accao = caminho.slice("/admin/api/".length);
    if (m === "GET" && accao === "resumo") return json(await resumo(env));
    if (m === "GET" && accao === "cliente") return json(await fichaCliente(env, url.searchParams.get("nif")));
    if (m === "POST") {
      const c = await corpoJson(req);
      const f = { pagamento: registarPagamento, emitir: emitirManual, plano: guardarPlano, cliente: guardarCliente, maquina: guardarMaquina, codigo: apagarCodigo }[accao];
      if (f) return json(await f(env, c));
    }
  }
  return erro("Não encontrado.", 404);
}

export default {
  async fetch(req, env) {
    try {
      return await tratar(req, env);
    } catch (e) {
      if (e instanceof Falha) return erro(e.message, e.estado);
      console.error(e);
      return erro("Erro interno do servidor de licenças. Tente mais tarde.", 500);
    }
  },
};

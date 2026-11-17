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
//            RESEND_API_KEY + EMAIL_REMETENTE (opcionais: sem eles, o código aparece no painel),
//            ACCESS_EQUIPA + ACCESS_AUD (opcionais: o painel só responde a quem passou pelo Cloudflare Access).
// Protecção: limite de pedidos por IP e por hora (tabela «limites»), Cloudflare Access à frente do painel,
//            cabeçalhos de segurança em todas as respostas.
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

// ------------------------------------------------------------------ protecção

/** Pedidos por IP e por hora. O registo envia emails; a confirmação testa códigos; o painel conta só as falhas. */
export const LIMITES = { registo: 20, confirmar: 60, renovar: 600, admin: 30, web: 60 };
const janelaHora = () => new Date().toISOString().slice(0, 13);

/** Conta um pedido do IP (CF-Connecting-IP) e recusa acima do limite. Sem IP (testes locais), não limita. */
async function contar(env, req, tipo) {
  const ip = req.headers.get("cf-connecting-ip");
  if (!ip) return 0;
  const r = await env.DB.prepare(
    `INSERT INTO limites (chave, janela, n) VALUES (?1, ?2, 1)
     ON CONFLICT(chave) DO UPDATE SET n = CASE WHEN janela = ?2 THEN n + 1 ELSE 1 END, janela = ?2
     RETURNING n`,
  ).bind(`${tipo}:${ip}`, janelaHora()).first();
  // de vez em quando, esquece as horas passadas
  if (Math.random() < 0.02) await env.DB.prepare("DELETE FROM limites WHERE janela < ?").bind(janelaHora()).run();
  return r?.n ?? 0;
}

async function limitar(env, req, tipo) {
  if ((await contar(env, req, tipo)) > LIMITES[tipo]) throw new Falha("Demasiados pedidos deste endereço. Tente daqui a uma hora.", 429);
}

/** O IP já passou o limite (sem contar este pedido)? */
async function excedido(env, req, tipo) {
  const ip = req.headers.get("cf-connecting-ip");
  if (!ip) return false;
  const r = await env.DB.prepare("SELECT n FROM limites WHERE chave = ? AND janela = ?").bind(`${tipo}:${ip}`, janelaHora()).first();
  return (r?.n ?? 0) >= LIMITES[tipo];
}

const b64urlBytes = (s) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4)), (c) => c.charCodeAt(0));
let chavesAccess = null; // { equipa, quando, chaves: Map(kid → CryptoKey) }
let renovadasEm = 0;

/** Chaves públicas do Cloudflare Access da equipa (uma hora em memória). */
async function chavesDoAccess(env, renovar = false) {
  const equipa = env.ACCESS_EQUIPA;
  if (!renovar && chavesAccess?.equipa === equipa && Date.now() - chavesAccess.quando < 3600_000) return chavesAccess.chaves;
  const r = await fetch(`https://${equipa}/cdn-cgi/access/certs`);
  if (!r.ok) throw new Falha("Não foi possível confirmar o Cloudflare Access.", 503);
  const { keys = [] } = await r.json();
  const chaves = new Map();
  for (const k of keys) {
    if (k.kty !== "RSA") continue;
    chaves.set(k.kid ?? "", await crypto.subtle.importKey("jwk", { kty: "RSA", n: k.n, e: k.e, alg: "RS256", ext: true }, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]));
  }
  chavesAccess = { equipa, quando: Date.now(), chaves };
  return chaves;
}

/**
 * Cloudflare Access à frente do painel: com ACCESS_EQUIPA e ACCESS_AUD, o pedido tem de trazer o JWT do Access
 * (cabeçalho Cf-Access-Jwt-Assertion ou cookie CF_Authorization) assinado pela equipa, para esta aplicação e válido.
 */
export async function confirmarAccess(env, req) {
  if (!env.ACCESS_EQUIPA || !env.ACCESS_AUD) return true;
  const cookie = (req.headers.get("cookie") ?? "").split(";").map((x) => x.trim()).find((x) => x.startsWith("CF_Authorization="));
  const token = req.headers.get("cf-access-jwt-assertion") ?? cookie?.slice("CF_Authorization=".length);
  if (!token) return false;
  try {
    const [h, p, s, resto] = token.split(".");
    if (!h || !p || !s || resto !== undefined) return false;
    const cab = JSON.parse(new TextDecoder().decode(b64urlBytes(h)));
    if (cab.alg !== "RS256") return false;
    let chaves = await chavesDoAccess(env);
    // chave nova (a Cloudflare roda-as): volta a buscá-las, no máximo de 5 em 5 minutos
    if (!chaves.has(cab.kid ?? "") && Date.now() - renovadasEm > 300_000) {
      renovadasEm = Date.now();
      chaves = await chavesDoAccess(env, true);
    }
    const chave = chaves.get(cab.kid ?? "");
    if (!chave || !(await crypto.subtle.verify("RSASSA-PKCS1-v1_5", chave, b64urlBytes(s), enc.encode(`${h}.${p}`)))) return false;
    const c = JSON.parse(new TextDecoder().decode(b64urlBytes(p)));
    const aud = Array.isArray(c.aud) ? c.aud : [c.aud];
    const agoraS = Date.now() / 1000;
    return aud.includes(env.ACCESS_AUD) && c.iss === `https://${env.ACCESS_EQUIPA}` && typeof c.exp === "number" && c.exp + 60 > agoraS && !(c.nbf > agoraS + 60);
  } catch (e) {
    if (e instanceof Falha) throw e;
    return false;
  }
}

/** Cabeçalhos de segurança em todas as respostas. */
const SEGURANCA = {
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
  "strict-transport-security": "max-age=31536000; includeSubDomains",
  "permissions-policy": "camera=(), microphone=(), geolocation=(), payment=()",
  "cross-origin-opener-policy": "same-origin",
};

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
  // segredo do computador: só se mostra agora; confirmar de novo (código por email) troca-o
  const segredo = b64url(crypto.getRandomValues(new Uint8Array(32)));
  await env.DB.prepare(
    `INSERT INTO segredos_maquina (nif, maquina, segredo_hash, criado_em) VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT(nif, maquina) DO UPDATE SET segredo_hash = ?3, criado_em = ?4`,
  ).bind(nif, maquina, await sha256(`segredo:${segredo}`), t).run();
  cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  return { ok: true, segredo, ...(await licencaActual(env, cliente, maquina)) };
}

/** Comparação em tempo constante (resumos com o mesmo tamanho). */
function iguais(a, b) {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length) return false;
  let dif = 0;
  for (let i = 0; i < a.length; i++) dif |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return dif === 0;
}

/**
 * Computador registado e com o segredo certo. Os computadores registados antes de haver segredos não o têm:
 * renovam como antes (`exigir` falso), mas para o acesso pela Internet têm de confirmar de novo o registo.
 */
async function autenticarMaquina(env, c, exigir = true) {
  const nif = validarNif(limpaNif(c.nif));
  const maquina = validarMaquina(limpaMaquina(c.maquina));
  const maq = await env.DB.prepare("SELECT bloqueada FROM maquinas WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  if (!maq) throw new Falha("Este computador ainda não está registado. Registe-o em Sistema → Licença e Assinatura.", 404);
  if (maq.bloqueada) throw new Falha("Este computador foi bloqueado para este NIF. Contacte o suporte do KanGest.", 403);
  const s = await env.DB.prepare("SELECT segredo_hash FROM segredos_maquina WHERE nif = ? AND maquina = ?").bind(nif, maquina).first();
  if (!s) {
    if (exigir) throw new Falha("Confirme de novo o registo deste computador (Sistema → Licença e Assinatura → Registar) para activar esta função.", 401);
    return { nif, maquina };
  }
  if (!iguais(await sha256(`segredo:${String(c.segredo ?? "")}`), s.segredo_hash)) {
    throw new Falha("O registo deste computador não é válido. Confirme-o de novo em Sistema → Licença e Assinatura.", 401);
  }
  return { nif, maquina };
}

async function renovar(env, c) {
  const { nif, maquina } = await autenticarMaquina(env, c, false);
  await env.DB.prepare("UPDATE maquinas SET ultimo_contacto = ?, versao = COALESCE(?, versao) WHERE nif = ? AND maquina = ?")
    .bind(agora(), texto(c.versao, 20) || null, nif, maquina).run();
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  return { ok: true, ...(await licencaActual(env, cliente, maquina)) };
}

async function planosPublicos(env) {
  const { results } = await env.DB.prepare("SELECT codigo, nome, postos, preco_mes, preco_ano FROM planos WHERE activo = 1 ORDER BY ordem").all();
  return { ok: true, moeda: "AOA", planos: results.map((p) => ({ ...p, preco_mes: p.preco_mes / 100, preco_ano: p.preco_ano / 100 })) };
}

// ------------------------------------------------------------------ acesso pela Internet (subdomínios)
//
// Cada empresa escolhe um nome (loja → https://loja.<WEB_DOMINIO>). O servidor cria, em nome do produtor, um túnel da
// Cloudflare até ao KanGest servidor da loja (http://localhost:<porta>) e o registo DNS; o KanGest recebe o token do
// túnel e corre o cloudflared. Configuração (docs/CLOUDFLARE.md): WEB_DOMINIO, CF_CONTA, CF_ZONA e o segredo
// CF_API_TOKEN (Cloudflare Tunnel · Edit na conta, DNS · Edit na zona). Sem ela, o acesso web diz que não está disponível.

const NOMES_RESERVADOS = new Set(["www", "api", "app", "admin", "painel", "licencas", "kangest", "mail", "email", "smtp", "ftp", "ns1", "ns2",
  "suporte", "ajuda", "site", "status", "demo", "teste", "dev", "blog", "loja", "cdn", "static", "web", "portal", "login", "conta"]);

function validarNome(v) {
  const nome = String(v ?? "").trim().toLowerCase();
  if (!/^[a-z0-9](?:[a-z0-9-]{1,28}[a-z0-9])$/.test(nome) || nome.includes("--")) {
    throw new Falha("O nome tem de ter 3 a 30 letras minúsculas, algarismos ou hífenes (sem começar nem acabar em hífen).");
  }
  if (NOMES_RESERVADOS.has(nome)) throw new Falha(`O nome «${nome}» está reservado. Escolha outro (por exemplo, o nome da empresa).`);
  return nome;
}

const webConfigurado = (env) => Boolean(env.WEB_DOMINIO && env.CF_CONTA && env.CF_ZONA && env.CF_API_TOKEN);

/** Pedido à API da Cloudflare com o token do produtor. Devolve o `result`; os erros passam a Falha 502. */
async function cf(env, metodo, caminho, corpo) {
  const r = await fetch("https://api.cloudflare.com/client/v4" + caminho, {
    method: metodo,
    headers: { authorization: `Bearer ${env.CF_API_TOKEN}`, "content-type": "application/json" },
    body: corpo === undefined ? undefined : JSON.stringify(corpo),
  });
  let d = null;
  try { d = await r.json(); } catch { /* em baixo */ }
  if (!r.ok || !d?.success) {
    const msg = d?.errors?.map((e) => `${e.code}: ${e.message}`).join("; ") || `HTTP ${r.status}`;
    throw new Falha(`A Cloudflare recusou o pedido (${msg}). Tente mais tarde ou contacte o suporte do KanGest.`, 502);
  }
  return d.result;
}

async function webEstado(env, c) {
  const { nif, maquina } = await autenticarMaquina(env, c);
  const s = await env.DB.prepare("SELECT * FROM subdominios WHERE nif = ?").bind(nif).first();
  return {
    ok: true,
    disponivel: webConfigurado(env),
    dominio: env.WEB_DOMINIO || null,
    nome: s?.nome ?? null,
    endereco: s ? `https://${s.nome}.${env.WEB_DOMINIO}` : null,
    neste_computador: s ? s.maquina === maquina : null,
  };
}

async function webActivar(env, c) {
  const { nif, maquina } = await autenticarMaquina(env, c);
  if (!webConfigurado(env)) throw new Falha("O acesso pela Internet ainda não está disponível. Contacte o suporte do KanGest.", 503);
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cliente || estadoCliente(cliente) === "EXPIRADA") {
    throw new Falha("A assinatura terminou: renove-a para activar o acesso pela Internet.", 402);
  }
  const nome = validarNome(c.nome);
  const porta = Number(c.porta ?? 7799);
  if (!Number.isInteger(porta) || porta < 1024 || porta > 65535) throw new Falha("Porta inválida.");
  const existente = await env.DB.prepare("SELECT * FROM subdominios WHERE nif = ?").bind(nif).first();
  if (existente && existente.maquina !== maquina) {
    throw new Falha(`O acesso pela Internet desta empresa está activo noutro computador (${existente.maquina}). Desactive-o primeiro.`, 409);
  }
  const dono = await env.DB.prepare("SELECT nif FROM subdominios WHERE nome = ?").bind(nome).first();
  if (dono && dono.nif !== nif) throw new Falha(`O endereço ${nome}.${env.WEB_DOMINIO} já está em uso. Escolha outro nome.`, 409);

  const conta = `/accounts/${env.CF_CONTA}`;
  const host = `${nome}.${env.WEB_DOMINIO}`;
  const tunel = existente?.tunel_id ?? (await cf(env, "POST", `${conta}/cfd_tunnel`, { name: `kangest-${nif}-${Date.now()}`, config_src: "cloudflare" })).id;
  await cf(env, "PUT", `${conta}/cfd_tunnel/${tunel}/configurations`, {
    config: { ingress: [{ hostname: host, service: `http://localhost:${porta}` }, { service: "http_status:404" }] },
  });
  let dns = existente?.dns_id ?? null;
  if (!existente || existente.nome !== nome || !dns) {
    const novo = await cf(env, "POST", `/zones/${env.CF_ZONA}/dns_records`, {
      type: "CNAME", name: host, content: `${tunel}.cfargotunnel.com`, proxied: true, ttl: 1, comment: `KanGest NIF ${nif}`,
    });
    if (dns) await cf(env, "DELETE", `/zones/${env.CF_ZONA}/dns_records/${dns}`).catch(() => {});
    dns = novo.id;
  }
  const token = await cf(env, "GET", `${conta}/cfd_tunnel/${tunel}/token`);
  const t = agora();
  await env.DB.prepare(
    `INSERT INTO subdominios (nif, nome, maquina, tunel_id, dns_id, porta, criado_em, actualizado_em) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
     ON CONFLICT(nif) DO UPDATE SET nome = ?2, maquina = ?3, tunel_id = ?4, dns_id = ?5, porta = ?6, actualizado_em = ?7`,
  ).bind(nif, nome, maquina, tunel, dns, porta, t).run();
  return { ok: true, nome, endereco: `https://${host}`, token };
}

/** Desliga o acesso pela Internet. Qualquer computador registado da empresa o pode fazer (ex.: o servidor avariou). */
async function webDesactivar(env, c) {
  const { nif } = await autenticarMaquina(env, c);
  const s = await env.DB.prepare("SELECT * FROM subdominios WHERE nif = ?").bind(nif).first();
  if (!s) return { ok: true, mensagem: "O acesso pela Internet já estava desligado." };
  if (webConfigurado(env)) {
    const conta = `/accounts/${env.CF_CONTA}`;
    if (s.dns_id) await cf(env, "DELETE", `/zones/${env.CF_ZONA}/dns_records/${s.dns_id}`).catch(() => {});
    await cf(env, "DELETE", `${conta}/cfd_tunnel/${s.tunel_id}/connections`).catch(() => {});
    await cf(env, "DELETE", `${conta}/cfd_tunnel/${s.tunel_id}`).catch(() => {});
  }
  await env.DB.prepare("DELETE FROM subdominios WHERE nif = ?").bind(nif).run();
  return { ok: true, mensagem: `O endereço ${s.nome}.${env.WEB_DOMINIO ?? ""} foi desligado.` };
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
  if (m === "POST" && caminho === "/v1/registo") {
    await limitar(env, req, "registo");
    return json(await registo(env, await corpoJson(req)));
  }
  if (m === "POST" && caminho === "/v1/confirmar") {
    await limitar(env, req, "confirmar");
    return json(await confirmar(env, await corpoJson(req)));
  }
  if (m === "POST" && caminho === "/v1/renovar") {
    await limitar(env, req, "renovar");
    return json(await renovar(env, await corpoJson(req)));
  }
  if (m === "POST" && caminho.startsWith("/v1/web/")) {
    const f = { estado: webEstado, activar: webActivar, desactivar: webDesactivar }[caminho.slice("/v1/web/".length)];
    if (f) {
      await limitar(env, req, "web");
      return json(await f(env, await corpoJson(req)));
    }
  }

  // painel: Cloudflare Access (se configurado) e, na API, o token; as falhas contam por IP
  if (caminho === "/admin" || caminho.startsWith("/admin/")) {
    if (await excedido(env, req, "admin")) return erro("Demasiadas tentativas falhadas deste endereço. Tente daqui a uma hora.", 429);
    if (!(await confirmarAccess(env, req))) {
      await contar(env, req, "admin");
      return erro("Acesso reservado: entre pelo Cloudflare Access.", 403);
    }
  }
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
    if (!autorizado(env, req)) {
      await contar(env, req, "admin");
      return erro("Acesso negado.", 401);
    }
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

async function responder(req, env) {
  try {
    return await tratar(req, env);
  } catch (e) {
    if (e instanceof Falha) return erro(e.message, e.estado);
    console.error(e);
    return erro("Erro interno do servidor de licenças. Tente mais tarde.", 500);
  }
}

export default {
  async fetch(req, env) {
    const r = await responder(req, env);
    const h = new Headers(r.headers);
    for (const [k, v] of Object.entries(SEGURANCA)) if (!h.has(k)) h.set(k, v);
    return new Response(r.body, { status: r.status, statusText: r.statusText, headers: h });
  },
};

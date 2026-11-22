// KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
// © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

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
//            ACCESS_EQUIPA + ACCESS_AUD (opcionais: o painel só responde a quem passou pelo Cloudflare Access),
//            ADMIN_ORIGENS (opcional: origens da área do produtor no site; por omissão https://kangest.pages.dev),
//            ADMIN_EMAIL + ADMIN_SENHA (opcionais: entrada no painel com email e senha; a senha só como resumo PBKDF2,
//            gerado com `node servidor-licencas/resumo-senha.mjs`).
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
export const LIMITES = { registo: 20, confirmar: 60, renovar: 600, admin: 30, web: 60, portal: 120, conta: 30, contaFalhas: 20 };
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
  await guardarEstado(env, nif, maquina, c);
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  // a equipa gerida no site, se o KanGest estiver atrás (KanGest 1.4+ diz que versão tem em estado.equipa_versao)
  const equipa = c?.estado && typeof c.estado === "object" ? await equipaParaKangest(env, nif, maquina, c.estado.equipa_versao) : null;
  return { ok: true, ...(await licencaActual(env, cliente, maquina)), ...(equipa ? { equipa_gerida: equipa } : {}) };
}

const dataOuNull = (v) => (/^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$/.test(String(v ?? "")) ? String(v) : null);

/** A equipa que o KanGest manda: só os campos conhecidos, com limites (o resto, se viesse, não se guarda). */
export function limparEquipa(e) {
  if (!e || typeof e !== "object" || Array.isArray(e)) return null;
  const lista = (v, n) => (Array.isArray(v) ? v.slice(0, n).filter((x) => x && typeof x === "object") : []);
  const data = (v) => dataOuNull(String(v ?? "").replace(" ", "T"));
  return {
    utilizadores: lista(e.utilizadores, 500).map((u) => ({
      login: texto(u.login, 40), nome: texto(u.nome, 80), perfil: texto(u.perfil, 20), ativo: u.ativo === true,
      loja: texto(u.loja, 60) || null, dois_passos: u.dois_passos === true, ultimo_acesso: data(u.ultimo_acesso),
    })),
    perfis: lista(e.perfis, 50).map((p) => ({ codigo: texto(p.codigo, 20), descricao: texto(p.descricao, 60) })),
    lojas: lista(e.lojas, 200).map((l) => ({ nome: texto(l.nome, 60), ativo: l.ativo === true })),
    postos: lista(e.postos, 200).map((p) => ({ descricao: texto(p.descricao, 60), loja: texto(p.loja, 60) || null })),
  };
}

/** O estado que o KanGest manda na verificação (versões antigas não mandam nada: fica o que havia). */
export async function guardarEstado(env, nif, maquina, c) {
  const e = c?.estado;
  if (!e || typeof e !== "object" || Array.isArray(e)) return;
  const lic = e.licenca && typeof e.licenca === "object" ? e.licenca : {};
  const cop = e.copias && typeof e.copias === "object" ? e.copias : {};
  await env.DB.prepare(
    `INSERT INTO instalacoes (nif, maquina, recebido_em, versao, modo, licenca_estado, licenca_fim, copia_ultima, copia_externa_ultima, copia_externa_erro)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT (nif, maquina) DO UPDATE SET recebido_em = excluded.recebido_em, versao = excluded.versao, modo = excluded.modo,
       licenca_estado = excluded.licenca_estado, licenca_fim = excluded.licenca_fim, copia_ultima = excluded.copia_ultima,
       copia_externa_ultima = excluded.copia_externa_ultima, copia_externa_erro = excluded.copia_externa_erro`,
  )
    .bind(nif, maquina, agora(), texto(c.versao, 20) || null, texto(e.modo, 12) || null, texto(lic.estado, 20) || null,
      dataOuNull(lic.fim), dataOuNull(cop.ultima), dataOuNull(cop.ultima_externa), cop.erro_externa ? 1 : 0)
    .run();
  const equipa = limparEquipa(e.equipa);
  if (equipa) {
    await env.DB.prepare(
      `INSERT INTO equipas (nif, maquina, recebido_em, dados) VALUES (?, ?, ?, ?)
       ON CONFLICT (nif, maquina) DO UPDATE SET recebido_em = excluded.recebido_em, dados = excluded.dados`,
    ).bind(nif, maquina, agora(), JSON.stringify(equipa)).run();
  }
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

// ------------------------------------------------------------------ verificação de humano (Cloudflare Turnstile)
//
// Na entrada pela Internet dos KanGest com endereço <nome>.<WEB_DOMINIO>, como no Claude: o widget no browser e a
// confirmação aqui, porque o segredo não vai para os computadores das lojas. Segredos TURNSTILE_SITE_KEY (público) e
// TURNSTILE_SECRET_KEY, de um widget do Cloudflare com o domínio WEB_DOMINIO. Sem eles, nada se exige.

const SITEVERIFY = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
const humanoConfigurado = (env) => Boolean(env.TURNSTILE_SITE_KEY && env.TURNSTILE_SECRET_KEY && env.WEB_DOMINIO);

/** A chave pública do widget, para o KanGest a pôr no ecrã de entrada (null = não se exige). */
function humanoChave(env) {
  return { ok: true, site_key: humanoConfigurado(env) ? env.TURNSTILE_SITE_KEY : null, dominio: env.WEB_DOMINIO || null };
}

/**
 * Confere o token do widget com o Cloudflare. `humano: false` só quando o Cloudflare diz que o token não vale (ou não
 * veio); sem resposta do Cloudflare, ou com a nossa chave recusada, deixa passar e diz `degradado` (uma loja que não
 * entra porque o serviço de outra empresa caiu é pior do que um robô a mais durante esse tempo).
 */
export async function humanoVerificar(env, c) {
  if (!humanoConfigurado(env)) return { ok: true, humano: true, degradado: true };
  const token = String(c?.token ?? "").trim();
  if (!token || token.length > 2048) return { ok: true, humano: false, motivo: "missing-input-response" };
  const corpo = new URLSearchParams({ secret: env.TURNSTILE_SECRET_KEY, response: token });
  const ip = String(c?.ip ?? "").trim();
  if (/^[0-9a-f.:]{3,45}$/i.test(ip)) corpo.set("remoteip", ip);
  try {
    const r = await fetch(SITEVERIFY, { method: "POST", body: corpo, signal: AbortSignal.timeout(5000) });
    if (!r.ok) return { ok: true, humano: true, degradado: true };
    const d = await r.json().catch(() => null);
    if (!d) return { ok: true, humano: true, degradado: true };
    if (d.success === true) return { ok: true, humano: true };
    const codigos = Array.isArray(d["error-codes"]) ? d["error-codes"] : [];
    if (codigos.some((x) => x === "invalid-input-secret" || x === "missing-input-secret")) {
      console.error("Turnstile: a chave secreta foi recusada", codigos.join(","));
      return { ok: true, humano: true, degradado: true };
    }
    return { ok: true, humano: false, motivo: codigos.join(",") || "rejeitado" };
  } catch {
    return { ok: true, humano: true, degradado: true };
  }
}

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

/**
 * Página de entrada comum (site → Entrar): a partir do nome que a empresa escolheu, diz o endereço do KanGest web dela
 * e se o computador da loja está ligado (estado do túnel na Cloudflare). Público: só revela o que o DNS já revela.
 */
async function webEndereco(env, dado) {
  if (!webConfigurado(env)) throw new Falha("O acesso ao KanGest pela Internet ainda não está disponível.", 503);
  const nome = String(dado ?? "").trim().toLowerCase();
  if (!/^[a-z0-9-]{3,30}$/.test(nome)) throw new Falha("Escreva o endereço da empresa: 3 a 30 letras, algarismos ou hífenes.");
  const s = await env.DB.prepare("SELECT nome, tunel_id FROM subdominios WHERE nome = ?").bind(nome).first();
  if (!s) throw new Falha(`Não há nenhuma empresa com o endereço ${nome}.${env.WEB_DOMINIO}. Confirme-o com o administrador do KanGest da empresa.`, 404);
  let ligado = null;
  try {
    const t = await cf(env, "GET", `/accounts/${env.CF_CONTA}/cfd_tunnel/${s.tunel_id}`);
    ligado = t?.status === "healthy" || t?.status === "degraded";
  } catch { /* sem estado: a página deixa entrar na mesma */ }
  return { ok: true, nome: s.nome, endereco: `https://${s.nome}.${env.WEB_DOMINIO}`, ligado };
}

// ------------------------------------------------------------------ painel do produtor

// ------------------------------------------------------------------ entrada no painel com email e senha
//
// ADMIN_EMAIL e ADMIN_SENHA (segredos): o email do produtor e o resumo da senha, «pbkdf2-sha256$<iterações>$<sal>$<resumo>»
// (base64url). A senha nunca fica no código nem no servidor. A entrada dá uma sessão de 12 horas, «s1.<dados>.<assinatura>»,
// assinada com HMAC-SHA256 pela ADMIN_TOKEN (que continua a valer como chave de emergência).

const SESSAO_HORAS = 12;
const deB64url = (s) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4)), (c) => c.charCodeAt(0));

/** Comparação em tempo constante de bytes (a «iguais» de cima compara texto). */
function iguaisBytes(a, b) {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i];
  return d === 0;
}

/** PBKDF2-SHA256 (o mesmo que o resumo-senha.mjs produz). */
export async function resumoSenha(senha, sal, iteracoes = 100_000) {
  const chave = await crypto.subtle.importKey("raw", enc.encode(senha), "PBKDF2", false, ["deriveBits"]);
  return new Uint8Array(await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt: sal, iterations: iteracoes }, chave, 256));
}

async function senhaCerta(env, senha) {
  const [alg, it, sal, res] = String(env.ADMIN_SENHA ?? "").trim().split("$");
  const iteracoes = Number(it);
  if (alg !== "pbkdf2-sha256" || !sal || !res || !(iteracoes >= 10_000 && iteracoes <= 100_000)) return false;
  return iguaisBytes(await resumoSenha(senha, deB64url(sal), iteracoes), deB64url(res));
}

async function assinarSessao(env, texto) {
  const k = await crypto.subtle.importKey("raw", enc.encode(env.ADMIN_TOKEN ?? ""), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(await crypto.subtle.sign("HMAC", k, enc.encode(texto)));
}

/** Como no Claude: «Chrome (Mac OS X)», «Safari (iPhone)». */
export function descreverAparelho(ua) {
  const u = String(ua ?? "");
  if (!u) return "Aparelho desconhecido";
  const nav = /Edg\//.test(u) ? "Edge" : /OPR\//.test(u) ? "Opera" : /Firefox\/|FxiOS/.test(u) ? "Firefox" : /Chrome\/|CriOS/.test(u) ? "Chrome" : /Safari\//.test(u) ? "Safari" : "Browser";
  const so = /iPhone/.test(u) ? "iPhone" : /iPad/.test(u) ? "iPad" : /Android/.test(u) ? "Android" : /Windows/.test(u) ? "Windows" : /Mac OS X|Macintosh/.test(u) ? "Mac OS X" : /Linux/.test(u) ? "Linux" : "";
  return so ? `${nav} (${so})` : nav;
}

/** Local aproximado dito pela Cloudflare (cidade, região, país). */
function localDe(req) {
  const cf = req?.cf ?? {};
  return [cf.city, cf.region, cf.country].filter(Boolean).join(", ") || "Desconhecido";
}

export async function entrarPainel(env, c, req) {
  const certo = String(env.ADMIN_EMAIL ?? "").trim().toLowerCase();
  if (!certo || !env.ADMIN_SENHA || (env.ADMIN_TOKEN ?? "").length < 24) {
    throw new Falha("A entrada com email e senha ainda não está configurada neste servidor: use a chave de administração.", 503);
  }
  const email = String(c?.email ?? "").trim().toLowerCase();
  const senha = String(c?.senha ?? "");
  // a senha confere-se sempre (mesmo com o email errado), para o tempo de resposta não dizer qual dos dois falhou
  const senhaOk = senha.length > 0 && senha.length <= 256 && (await senhaCerta(env, senha));
  if (!senhaOk || !iguaisBytes(enc.encode(email), enc.encode(certo))) throw new Falha("Email ou senha incorrectos.", 401);
  const expira = Date.now() + SESSAO_HORAS * 3_600_000;
  // cada entrada fica registada, para «Onde fez login» e para a poder terminar
  const sid = b64url(crypto.getRandomValues(new Uint8Array(16)));
  await env.DB.prepare("INSERT INTO sessoes_painel (id, email, criada_em, ultima_em, aparelho, local) VALUES (?, ?, ?, ?, ?, ?)")
    .bind(sid, certo, agora(), agora(), descreverAparelho(req?.headers.get("user-agent")).slice(0, 60), localDe(req).slice(0, 80))
    .run();
  const dados = b64url(enc.encode(JSON.stringify({ e: certo, exp: expira, s: sid })));
  return { ok: true, token: `s1.${dados}.${await assinarSessao(env, dados)}`, email: certo, expira: new Date(expira).toISOString() };
}

/** A sessão do painel, se a assinatura confere e não expirou. */
async function sessaoPainel(env, token) {
  const [v, dados, ass] = token.split(".");
  if (v !== "s1" || !dados || !ass || (env.ADMIN_TOKEN ?? "").length < 24) return null;
  if (!iguaisBytes(enc.encode(await assinarSessao(env, dados)), enc.encode(ass))) return null;
  try {
    const d = JSON.parse(new TextDecoder().decode(deB64url(dados)));
    if (!(typeof d.exp === "number" && d.exp > Date.now() && d.e === String(env.ADMIN_EMAIL ?? "").trim().toLowerCase())) return null;
    // a sessão tem de continuar registada e não terminada («Sair em todos os aparelhos»)
    const s = await env.DB.prepare("SELECT ultima_em, terminada_em FROM sessoes_painel WHERE id = ?").bind(String(d.s ?? "")).first();
    if (!s || s.terminada_em) return null;
    if (Date.parse(s.ultima_em) < Date.now() - 5 * 60_000) {
      await env.DB.prepare("UPDATE sessoes_painel SET ultima_em = ? WHERE id = ?").bind(agora(), d.s).run();
    }
    return d;
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------ contas das empresas (portal do cliente)
//
// Como as definições da conta do Claude: o dono da empresa entra no site (kangest.pages.dev/conta) com o email do
// registo e uma senha, e vê a licença, os pagamentos, os computadores e onde tem sessão. A senha cria-se (ou repõe-se)
// com um código de 6 algarismos enviado para o email do registo; sem envio de email configurado, o produtor vê-o no
// painel e dá-o por telefone, como no registo. Sessões de 30 dias, «c1.<dados>.<assinatura>», registadas em
// sessoes_conta (aparelho e local, sem o IP); mudar ou repor a senha termina as outras.

const CONTA_DIAS = 30;
const CONTA_ITERACOES = 100_000;
const MSG_CODIGO = "Se este email for o de uma empresa registada no KanGest, enviámos um código de 6 algarismos para definir a senha. Sem email, peça-o ao suporte do KanGest.";

async function hmac(chave, texto) {
  const k = await crypto.subtle.importKey("raw", enc.encode(chave), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(await crypto.subtle.sign("HMAC", k, enc.encode(texto)));
}
const chaveConta = (env) => `conta|${env.ADMIN_TOKEN ?? ""}`;

async function resumoNovo(senha) {
  const sal = crypto.getRandomValues(new Uint8Array(16));
  return `pbkdf2-sha256$${CONTA_ITERACOES}$${b64url(sal)}$${b64url(await resumoSenha(senha, sal, CONTA_ITERACOES))}`;
}

async function confereResumo(guardado, senha) {
  const [alg, it, sal, res] = String(guardado ?? "").split("$");
  const iteracoes = Number(it);
  if (alg !== "pbkdf2-sha256" || !sal || !res || !(iteracoes >= 10_000 && iteracoes <= 100_000)) return false;
  return iguaisBytes(await resumoSenha(senha, deB64url(sal), iteracoes), deB64url(res));
}

function validarSenhaNova(senha) {
  const s = String(senha ?? "");
  if (s.length < 10 || s.length > 128) throw new Falha("A senha tem de ter entre 10 e 128 caracteres.");
  if (!/[A-Za-z]/.test(s) || !/[0-9]/.test(s)) throw new Falha("A senha tem de ter letras e algarismos.");
  return s;
}

async function enviarCodigoConta(env, { email, empresa, codigo }) {
  if (!env.RESEND_API_KEY || !env.EMAIL_REMETENTE) return false;
  try {
    const r = await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: { authorization: `Bearer ${env.RESEND_API_KEY}`, "content-type": "application/json" },
      body: JSON.stringify({
        from: env.EMAIL_REMETENTE, to: [email], subject: `KanGest: código ${codigo} para a conta de ${empresa}`,
        text: `Olá,\n\nO código para definir a senha da conta de ${empresa} no site do KanGest é:\n\n    ${codigo}\n\nÉ válido durante ${CODIGO_MINUTOS} minutos.\n\nSe não pediu este código, ignore esta mensagem: a sua conta continua como estava.\n\nKanGest`,
      }),
    });
    return r.ok;
  } catch {
    return false;
  }
}

/** Criar a conta ou repor a senha: um código para o email do registo. A resposta é sempre a mesma. */
export async function contaPedirCodigo(env, c) {
  const email = String(c?.email ?? "").trim().toLowerCase();
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || email.length > 120) throw new Falha("Escreva um email válido.");
  const cli = await env.DB.prepare("SELECT nif, empresa FROM clientes WHERE lower(email) = ? ORDER BY criado_em LIMIT 1").bind(email).first();
  if (!cli) return { ok: true, mensagem: MSG_CODIGO };
  const antes = await env.DB.prepare("SELECT criado_em FROM codigos_conta WHERE email = ?").bind(email).first();
  if (antes && Date.parse(antes.criado_em) > Date.now() - 60_000) return { ok: true, mensagem: MSG_CODIGO };
  const codigo = String(100_000 + (crypto.getRandomValues(new Uint32Array(1))[0] % 900_000));
  const enviado = await enviarCodigoConta(env, { email, empresa: cli.empresa, codigo });
  const expira = new Date(Date.now() + CODIGO_MINUTOS * 60_000).toISOString().slice(0, 19) + "Z";
  await env.DB.prepare(
    `INSERT INTO codigos_conta (email, nif, codigo_hash, codigo_aberto, expira_em, tentativas, criado_em) VALUES (?, ?, ?, ?, ?, 0, ?)
     ON CONFLICT (email) DO UPDATE SET nif = excluded.nif, codigo_hash = excluded.codigo_hash, codigo_aberto = excluded.codigo_aberto,
       expira_em = excluded.expira_em, tentativas = 0, criado_em = excluded.criado_em`,
  ).bind(email, cli.nif, await sha256(`conta:${email}:${codigo}`), enviado ? null : codigo, expira, agora()).run();
  return { ok: true, mensagem: MSG_CODIGO };
}

/** Com o código certo, define a senha (cria a conta, se ainda não havia) e termina as sessões abertas. */
export async function contaDefinirSenha(env, c) {
  const email = String(c?.email ?? "").trim().toLowerCase();
  const codigo = String(c?.codigo ?? "").replace(/\D/g, "");
  const senha = validarSenhaNova(c?.senha);
  const r = await env.DB.prepare("SELECT * FROM codigos_conta WHERE email = ?").bind(email).first();
  if (!r || Date.parse(r.expira_em) < Date.now() || r.tentativas >= MAX_TENTATIVAS) {
    throw new Falha("O código expirou ou não é válido. Peça outro.", 400);
  }
  if (!iguais(await sha256(`conta:${email}:${codigo}`), r.codigo_hash)) {
    await env.DB.prepare("UPDATE codigos_conta SET tentativas = tentativas + 1 WHERE email = ?").bind(email).run();
    throw new Falha("Código errado.", 400);
  }
  const resumo = await resumoNovo(senha);
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO contas (nif, email, senha, criada_em, actualizada_em) VALUES (?, ?, ?, ?, ?)
       ON CONFLICT (nif) DO UPDATE SET email = excluded.email, senha = excluded.senha, actualizada_em = excluded.actualizada_em`,
    ).bind(r.nif, email, resumo, agora(), agora()),
    env.DB.prepare("DELETE FROM codigos_conta WHERE email = ?").bind(email),
    env.DB.prepare("UPDATE sessoes_conta SET terminada_em = ? WHERE nif = ? AND terminada_em IS NULL").bind(agora(), r.nif),
  ]);
  return { ok: true, mensagem: "Senha definida. Já pode entrar." };
}

export async function contaEntrar(env, c, req) {
  const email = String(c?.email ?? "").trim().toLowerCase();
  const senha = String(c?.senha ?? "");
  const conta = await env.DB.prepare("SELECT nif, email, senha FROM contas WHERE email = ?").bind(email).first();
  // sem conta, confere-se na mesma um resumo (o tempo de resposta não diz se o email existe)
  const ok = (await confereResumo(conta?.senha ?? `pbkdf2-sha256$${CONTA_ITERACOES}$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`, senha)) && !!conta;
  if (!ok || senha.length > 256) throw new Falha("Email ou senha incorrectos.", 401);
  const expira = Date.now() + CONTA_DIAS * 86_400_000;
  const sid = b64url(crypto.getRandomValues(new Uint8Array(16)));
  await env.DB.prepare("INSERT INTO sessoes_conta (id, nif, criada_em, ultima_em, aparelho, local) VALUES (?, ?, ?, ?, ?, ?)")
    .bind(sid, conta.nif, agora(), agora(), descreverAparelho(req?.headers.get("user-agent")).slice(0, 60), localDe(req).slice(0, 80))
    .run();
  const dados = b64url(enc.encode(JSON.stringify({ n: conta.nif, exp: expira, s: sid })));
  return { ok: true, token: `c1.${dados}.${await hmac(chaveConta(env), dados)}`, email: conta.email, expira: new Date(expira).toISOString() };
}

/** A sessão da conta deste pedido (assinatura, validade e registo não terminado), ou null. */
async function sessaoConta(env, req) {
  const token = (req.headers.get("authorization") ?? "").replace(/^Bearer\s+/i, "");
  const [v, dados, ass] = token.split(".");
  if (v !== "c1" || !dados || !ass || (env.ADMIN_TOKEN ?? "").length < 24) return null;
  if (!iguaisBytes(enc.encode(await hmac(chaveConta(env), dados)), enc.encode(ass))) return null;
  try {
    const d = JSON.parse(new TextDecoder().decode(deB64url(dados)));
    if (!(typeof d.exp === "number" && d.exp > Date.now())) return null;
    const s = await env.DB.prepare("SELECT nif, ultima_em, terminada_em FROM sessoes_conta WHERE id = ?").bind(String(d.s ?? "")).first();
    if (!s || s.terminada_em || s.nif !== d.n) return null;
    if (Date.parse(s.ultima_em) < Date.now() - 5 * 60_000) {
      await env.DB.prepare("UPDATE sessoes_conta SET ultima_em = ? WHERE id = ?").bind(agora(), d.s).run();
    }
    return { nif: d.n, sid: d.s, expira: d.exp };
  } catch {
    return null;
  }
}

/** Tudo o que a conta mostra: empresa, licença, pagamentos, computadores (com a licença e as cópias de cada um). */
async function contaResumo(env, s) {
  const cli = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(s.nif).first();
  if (!cli) throw new Falha("Empresa não encontrada.", 404);
  const [conta, maquinas, licencas, pagamentos, planos, equipas] = await env.DB.batch([
    env.DB.prepare("SELECT email, criada_em FROM contas WHERE nif = ?").bind(s.nif),
    env.DB.prepare(
      `SELECT m.maquina, m.versao, m.primeiro_contacto, m.ultimo_contacto, m.bloqueada, i.modo, i.licenca_estado, i.licenca_fim,
              i.copia_ultima, i.copia_externa_ultima, i.copia_externa_erro, i.recebido_em
       FROM maquinas m LEFT JOIN instalacoes i ON i.nif = m.nif AND i.maquina = m.maquina WHERE m.nif = ? ORDER BY m.ultimo_contacto DESC`,
    ).bind(s.nif),
    env.DB.prepare("SELECT id, maquina, plano, postos, inicio, fim, emitida_em, origem, chave FROM licencas WHERE nif = ? ORDER BY emitida_em DESC LIMIT 20").bind(s.nif),
    env.DB.prepare("SELECT registado_em, plano, meses, valor, metodo, referencia, pago_ate FROM pagamentos WHERE nif = ? ORDER BY id DESC LIMIT 50").bind(s.nif),
    env.DB.prepare("SELECT codigo, nome, postos, preco_mes, preco_ano FROM planos WHERE activo = 1 ORDER BY ordem"),
    env.DB.prepare("SELECT maquina, recebido_em, dados FROM equipas WHERE nif = ? ORDER BY recebido_em DESC LIMIT 1").bind(s.nif),
  ]);
  const c = conta.results[0] ?? {};
  const eq = equipas.results[0];
  return {
    ok: true,
    hoje: hoje(),
    empresa: {
      nif: cli.nif, nome: cli.empresa, email: c.email ?? cli.email, telefone: cli.telefone, estado: estadoCliente(cli),
      plano: cli.plano, pago_ate: cli.pago_ate, experiencia_ate: cli.experiencia_ate, maquinas_max: cli.maquinas_max, conta_desde: c.criada_em,
    },
    maquinas: maquinas.results, licencas: licencas.results, pagamentos: pagamentos.results, planos: planos.results,
    equipa: eq ? { ...JSON.parse(eq.dados), maquina: eq.maquina, recebido_em: eq.recebido_em } : null,
  };
}

async function contaSessoes(env, s) {
  const limite = new Date(Date.now() - CONTA_DIAS * 86_400_000).toISOString().slice(0, 19) + "Z";
  const { results } = await env.DB.prepare(
    "SELECT id, aparelho, local, criada_em, ultima_em FROM sessoes_conta WHERE nif = ? AND terminada_em IS NULL AND criada_em >= ? ORDER BY ultima_em DESC LIMIT 50",
  ).bind(s.nif, limite).all();
  return { ok: true, sessoes: results.map((x) => ({ ...x, actual: x.id === s.sid })).sort((a, b) => b.actual - a.actual) };
}

async function contaTerminar(env, s, c, outras) {
  const r = outras
    ? await env.DB.prepare("UPDATE sessoes_conta SET terminada_em = ? WHERE nif = ? AND terminada_em IS NULL AND id <> ?").bind(agora(), s.nif, s.sid).run()
    : await env.DB.prepare("UPDATE sessoes_conta SET terminada_em = ? WHERE nif = ? AND id = ? AND terminada_em IS NULL").bind(agora(), s.nif, String(c?.id ?? "")).run();
  const n = r.meta?.changes ?? 0;
  return { ok: true, terminadas: n, mensagem: n ? `${n} ${n === 1 ? "sessão terminada" : "sessões terminadas"}.` : "Não havia sessões para terminar." };
}

async function contaPerfil(env, s, c) {
  const telefone = texto(c?.telefone, 30);
  if (telefone && !/^[0-9 +()-]{6,30}$/.test(telefone)) throw new Falha("O telefone só pode ter algarismos, espaços e + ( ) -.");
  await env.DB.prepare("UPDATE clientes SET telefone = ? WHERE nif = ?").bind(telefone || null, s.nif).run();
  return { ok: true, mensagem: "Dados guardados." };
}

async function contaMudarSenha(env, s, c) {
  const conta = await env.DB.prepare("SELECT senha FROM contas WHERE nif = ?").bind(s.nif).first();
  if (!conta || !(await confereResumo(conta.senha, String(c?.actual ?? "")))) throw new Falha("A senha actual está errada.", 400);
  const nova = validarSenhaNova(c?.nova);
  await env.DB.batch([
    env.DB.prepare("UPDATE contas SET senha = ?, actualizada_em = ? WHERE nif = ?").bind(await resumoNovo(nova), agora(), s.nif),
    env.DB.prepare("UPDATE sessoes_conta SET terminada_em = ? WHERE nif = ? AND terminada_em IS NULL AND id <> ?").bind(agora(), s.nif, s.sid),
  ]);
  return { ok: true, mensagem: "Senha mudada. As sessões nos outros aparelhos terminaram." };
}

// ------------------------------------------------------------------ equipa gerida no site (fase 3b)

const PERFIS_BASE = ["ADMIN", "GESTOR", "CAIXA"];
const ALFABETO_SENHA = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";

function senhaProvisoria() {
  const b = crypto.getRandomValues(new Uint8Array(12));
  let s = "";
  for (const x of b) s += ALFABETO_SENHA[x % ALFABETO_SENHA.length];
  return s.slice(0, 4) + "-" + s.slice(4, 8) + "-" + s.slice(8, 12);
}

async function subirVersao(env, nif) {
  await env.DB.prepare(
    `INSERT INTO equipa_versoes (nif, versao, actualizado_em) VALUES (?, 1, ?)
     ON CONFLICT (nif) DO UPDATE SET versao = versao + 1, actualizado_em = excluded.actualizado_em`,
  ).bind(nif, agora()).run();
}

/** O que o KanGest comunicou (perfis e lojas válidos) e o que está gerido no site. */
async function equipaGerida(env, nif) {
  const [f, l, p, v, inv] = await env.DB.batch([
    env.DB.prepare("SELECT id, login, nome, perfil, loja, ativo, senha_provisoria IS NOT NULL AS senha_pendente FROM funcionarios WHERE nif = ? ORDER BY ativo DESC, nome").bind(nif),
    env.DB.prepare("SELECT id, nome, ativo FROM lojas_geridas WHERE nif = ? ORDER BY nome").bind(nif),
    env.DB.prepare("SELECT id, descricao, loja FROM postos_geridos WHERE nif = ? ORDER BY descricao").bind(nif),
    env.DB.prepare("SELECT versao, actualizado_em, aplicada, aplicada_em, aplicada_maquina FROM equipa_versoes WHERE nif = ?").bind(nif),
    env.DB.prepare("SELECT dados FROM equipas WHERE nif = ? ORDER BY recebido_em DESC LIMIT 1").bind(nif),
  ]);
  const comunicada = inv.results[0] ? JSON.parse(inv.results[0].dados) : null;
  return {
    funcionarios: f.results.map((x) => ({ ...x, ativo: x.ativo === 1, senha_pendente: x.senha_pendente === 1 })),
    lojas: l.results.map((x) => ({ ...x, ativo: x.ativo === 1 })),
    postos: p.results,
    versao: v.results[0] ?? { versao: 0, aplicada: 0 },
    perfis: comunicada?.perfis?.length ? comunicada.perfis.map((x) => x.codigo) : PERFIS_BASE,
    lojas_conhecidas: [...new Set([...(comunicada?.lojas ?? []).map((x) => x.nome), ...l.results.map((x) => x.nome)])],
    logins_no_programa: (comunicada?.utilizadores ?? []).map((u) => u.login.toLowerCase()),
  };
}

async function guardarFuncionario(env, nif, c) {
  const g = await equipaGerida(env, nif);
  const login = String(c?.login ?? "").trim().toLowerCase();
  const nome = texto(c?.nome, 80);
  const perfil = texto(c?.perfil, 20).toUpperCase();
  const loja = texto(c?.loja, 60) || null;
  if (!/^[a-z0-9._-]{2,40}$/.test(login)) throw new Falha("O login tem de ter 2 a 40 letras minúsculas, algarismos, pontos, hífenes ou sublinhados.");
  if (nome.length < 2) throw new Falha("Escreva o nome do funcionário.");
  if (!g.perfis.includes(perfil)) throw new Falha(`Perfil desconhecido. Use um destes: ${g.perfis.join(", ")}.`);
  if (loja && !g.lojas_conhecidas.includes(loja)) throw new Falha("Loja desconhecida: crie-a primeiro em «Lojas e postos».");
  const ativo = c?.ativo === false ? 0 : 1;
  const existe = c?.id ? g.funcionarios.find((x) => x.id === String(c.id)) : null;
  if (c?.id && !existe) throw new Falha("Funcionário não encontrado.", 404);
  const outro = g.funcionarios.find((x) => x.login === login && x.id !== existe?.id);
  if (outro) throw new Falha(`Já há um funcionário com o login «${login}».`);
  // novo: senha provisória, a não ser que já exista no programa com este login (aí mantém a senha que tem)
  const adoptar = !existe && g.logins_no_programa.includes(login);
  const senha = !existe && !adoptar ? senhaProvisoria() : null;
  const resumo = senha ? await resumoNovo(senha) : null;
  if (existe) {
    await env.DB.prepare("UPDATE funcionarios SET login = ?, nome = ?, perfil = ?, loja = ?, ativo = ?, actualizado_em = ? WHERE nif = ? AND id = ?")
      .bind(login, nome, perfil, loja, ativo, agora(), nif, existe.id).run();
  } else {
    await env.DB.prepare("INSERT INTO funcionarios (nif, id, login, nome, perfil, loja, ativo, senha_provisoria, criado_em, actualizado_em) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
      .bind(nif, b64url(crypto.getRandomValues(new Uint8Array(9))), login, nome, perfil, loja, ativo, resumo, agora(), agora()).run();
  }
  await subirVersao(env, nif);
  return {
    ok: true, senha_provisoria: senha,
    mensagem: existe ? "Funcionário guardado. Chega ao KanGest na próxima verificação."
      : adoptar ? `«${login}» já existe no programa: passa a ser gerido aqui e mantém a senha que tem.`
      : "Funcionário criado. Dê-lhe a senha provisória: no primeiro acesso ao KanGest tem de a mudar.",
  };
}

async function reporSenhaFuncionario(env, nif, c) {
  const f = await env.DB.prepare("SELECT id, login FROM funcionarios WHERE nif = ? AND id = ?").bind(nif, String(c?.id ?? "")).first();
  if (!f) throw new Falha("Funcionário não encontrado.", 404);
  const senha = senhaProvisoria();
  await env.DB.prepare("UPDATE funcionarios SET senha_provisoria = ?, actualizado_em = ? WHERE nif = ? AND id = ?").bind(await resumoNovo(senha), agora(), nif, f.id).run();
  await subirVersao(env, nif);
  return { ok: true, senha_provisoria: senha, mensagem: `Senha provisória nova para «${f.login}». Vale depois da próxima verificação do KanGest.` };
}

async function guardarLoja(env, nif, c) {
  const nome = texto(c?.nome, 60);
  if (nome.length < 2) throw new Falha("Escreva o nome da loja.");
  const ativo = c?.ativo === false ? 0 : 1;
  const id = c?.id ? String(c.id) : b64url(crypto.getRandomValues(new Uint8Array(9)));
  const dup = await env.DB.prepare("SELECT id FROM lojas_geridas WHERE nif = ? AND nome = ? AND id <> ?").bind(nif, nome, id).first();
  if (dup) throw new Falha(`Já há uma loja «${nome}».`);
  await env.DB.prepare(
    `INSERT INTO lojas_geridas (nif, id, nome, ativo, actualizado_em) VALUES (?, ?, ?, ?, ?)
     ON CONFLICT (nif, id) DO UPDATE SET nome = excluded.nome, ativo = excluded.ativo, actualizado_em = excluded.actualizado_em`,
  ).bind(nif, id, nome, ativo, agora()).run();
  await subirVersao(env, nif);
  return { ok: true, mensagem: "Loja guardada. Chega ao KanGest na próxima verificação." };
}

async function guardarPosto(env, nif, c) {
  const g = await equipaGerida(env, nif);
  const descricao = texto(c?.descricao, 60);
  const loja = texto(c?.loja, 60) || null;
  if (descricao.length < 2) throw new Falha("Escreva o nome do posto (ex.: Caixa 2).");
  if (!loja || !g.lojas_conhecidas.includes(loja)) throw new Falha("Escolha a loja do posto.");
  const id = c?.id ? String(c.id) : b64url(crypto.getRandomValues(new Uint8Array(9)));
  const dup = await env.DB.prepare("SELECT id FROM postos_geridos WHERE nif = ? AND descricao = ? AND id <> ?").bind(nif, descricao, id).first();
  if (dup) throw new Falha(`Já há um posto «${descricao}».`);
  await env.DB.prepare(
    `INSERT INTO postos_geridos (nif, id, descricao, loja, actualizado_em) VALUES (?, ?, ?, ?, ?)
     ON CONFLICT (nif, id) DO UPDATE SET descricao = excluded.descricao, loja = excluded.loja, actualizado_em = excluded.actualizado_em`,
  ).bind(nif, id, descricao, loja, agora()).run();
  await subirVersao(env, nif);
  return { ok: true, mensagem: "Posto guardado. Chega ao KanGest na próxima verificação." };
}

/**
 * Na verificação: regista a versão que o KanGest diz ter e, se estiver atrás, devolve a equipa gerida para aplicar.
 * Quando o KanGest já tem a versão actual, os resumos das senhas provisórias apagam-se (já não fazem falta aqui).
 */
export async function equipaParaKangest(env, nif, maquina, versaoKangest) {
  const v = await env.DB.prepare("SELECT versao, aplicada FROM equipa_versoes WHERE nif = ?").bind(nif).first();
  if (!v) return null;
  const tem = Number.isInteger(versaoKangest) ? versaoKangest : 0;
  if (tem >= v.versao) {
    if (v.aplicada < v.versao) {
      await env.DB.batch([
        env.DB.prepare("UPDATE equipa_versoes SET aplicada = ?, aplicada_em = ?, aplicada_maquina = ? WHERE nif = ?").bind(v.versao, agora(), maquina, nif),
        env.DB.prepare("UPDATE funcionarios SET senha_provisoria = NULL WHERE nif = ?").bind(nif),
      ]);
    }
    return null;
  }
  const [f, l, p] = await env.DB.batch([
    env.DB.prepare("SELECT id, login, nome, perfil, loja, ativo, senha_provisoria FROM funcionarios WHERE nif = ? ORDER BY login").bind(nif),
    env.DB.prepare("SELECT id, nome, ativo FROM lojas_geridas WHERE nif = ? ORDER BY nome").bind(nif),
    env.DB.prepare("SELECT id, descricao, loja FROM postos_geridos WHERE nif = ? ORDER BY descricao").bind(nif),
  ]);
  return {
    versao: v.versao,
    funcionarios: f.results.map((x) => ({ ...x, ativo: x.ativo === 1 })),
    lojas: l.results.map((x) => ({ ...x, ativo: x.ativo === 1 })),
    postos: p.results,
  };
}

/** As rotas /v1/conta/* (o portal do cliente). */
async function rotasConta(env, req, m, accao) {
  if (m === "POST" && accao === "codigo") {
    await limitar(env, req, "conta");
    return json(await contaPedirCodigo(env, await corpoJson(req)));
  }
  if (m === "POST" && accao === "definir-senha") {
    await limitar(env, req, "conta");
    return json(await contaDefinirSenha(env, await corpoJson(req)));
  }
  if (m === "POST" && accao === "entrar") {
    if (await excedido(env, req, "contaFalhas")) return erro("Demasiadas tentativas falhadas deste endereço. Tente daqui a uma hora.", 429);
    try {
      return json(await contaEntrar(env, await corpoJson(req), req));
    } catch (e) {
      if (e instanceof Falha && e.estado === 401) await contar(env, req, "contaFalhas");
      throw e;
    }
  }
  const s = await sessaoConta(env, req);
  if (!s) return erro("A sessão terminou. Entre outra vez.", 401);
  if (m === "GET" && accao === "resumo") return json(await contaResumo(env, s));
  if (m === "GET" && accao === "sessoes") return json(await contaSessoes(env, s));
  if (m === "POST" && accao === "sessoes/terminar") return json(await contaTerminar(env, s, await corpoJson(req), false));
  if (m === "POST" && accao === "sessoes/terminar-outras") return json(await contaTerminar(env, s, null, true));
  if (m === "POST" && accao === "perfil") return json(await contaPerfil(env, s, await corpoJson(req)));
  if (m === "POST" && accao === "senha") return json(await contaMudarSenha(env, s, await corpoJson(req)));
  if (m === "POST" && accao === "sair") return json(await contaTerminar(env, s, { id: s.sid }, false));
  if (m === "GET" && accao === "equipa") return json({ ok: true, ...(await equipaGerida(env, s.nif)) });
  if (m === "POST" && accao === "equipa/funcionario") return json(await guardarFuncionario(env, s.nif, await corpoJson(req)));
  if (m === "POST" && accao === "equipa/repor-senha") return json(await reporSenhaFuncionario(env, s.nif, await corpoJson(req)));
  if (m === "POST" && accao === "equipa/loja") return json(await guardarLoja(env, s.nif, await corpoJson(req)));
  if (m === "POST" && accao === "equipa/posto") return json(await guardarPosto(env, s.nif, await corpoJson(req)));
  return erro("Não encontrado.", 404);
}

/** «Onde fez login»: as sessões do painel ainda abertas (a actual primeiro). */
async function sessoesPainel(env, actual) {
  const limite = new Date(Date.now() - SESSAO_HORAS * 3_600_000).toISOString().slice(0, 19) + "Z";
  const { results } = await env.DB.prepare(
    "SELECT id, aparelho, local, criada_em, ultima_em FROM sessoes_painel WHERE terminada_em IS NULL AND criada_em >= ? ORDER BY ultima_em DESC LIMIT 50",
  ).bind(limite).all();
  return { ok: true, sessoes: results.map((s) => ({ ...s, actual: s.id === actual })).sort((a, b) => b.actual - a.actual) };
}

async function autorizado(env, req) {
  const dado = (req.headers.get("authorization") ?? "").replace(/^Bearer\s+/i, "");
  if (dado.startsWith("s1.")) return Boolean(await sessaoPainel(env, dado));
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
  const [planos, clientes, codigos, pagamentos, semCopia] = await env.DB.batch([
    env.DB.prepare("SELECT * FROM planos ORDER BY ordem"),
    env.DB.prepare(
      `SELECT c.*, (SELECT COUNT(*) FROM maquinas m WHERE m.nif = c.nif AND m.bloqueada = 0) AS maquinas,
              (SELECT MAX(ultimo_contacto) FROM maquinas m WHERE m.nif = c.nif) AS ultimo_contacto
       FROM clientes c ORDER BY c.criado_em DESC LIMIT 500`,
    ),
    env.DB.prepare(
      `SELECT nif, maquina, empresa, email, telefone, codigo_aberto, expira_em, tentativas, criado_em, 'registo' AS tipo FROM codigos
       UNION ALL
       SELECT k.nif, '' AS maquina, c.empresa, k.email, c.telefone, k.codigo_aberto, k.expira_em, k.tentativas, k.criado_em, 'conta' AS tipo
       FROM codigos_conta k LEFT JOIN clientes c ON c.nif = k.nif
       ORDER BY criado_em DESC LIMIT 100`,
    ),
    env.DB.prepare("SELECT p.*, c.empresa FROM pagamentos p LEFT JOIN clientes c ON c.nif = p.nif ORDER BY p.id DESC LIMIT 50"),
    // computadores activos (contacto nos últimos 30 dias) sem cópia local há mais de 7 dias, ou com a cópia externa a falhar
    env.DB.prepare(
      `SELECT i.nif FROM instalacoes i JOIN maquinas m ON m.nif = i.nif AND m.maquina = i.maquina
       WHERE m.bloqueada = 0 AND m.ultimo_contacto >= ? AND (i.copia_ultima IS NULL OR i.copia_ultima < ? OR i.copia_externa_erro = 1)`,
    ).bind(somarDias(hoje(), -30), somarDias(hoje(), -7)),
  ]);
  const dia = hoje();
  return {
    ok: true,
    hoje: dia,
    email_configurado: !!(env.RESEND_API_KEY && env.EMAIL_REMETENTE),
    planos: planos.results,
    clientes: clientes.results.map((c) => ({ ...c, estado: estadoCliente(c, dia), sem_copia: semCopia.results.some((x) => x.nif === c.nif) })),
    sem_copia: semCopia.results.length,
    codigos: codigos.results.map((c) => ({ ...c, expirado: Date.parse(c.expira_em) < Date.now() })),
    pagamentos: pagamentos.results,
  };
}

async function fichaCliente(env, nif) {
  nif = validarNif(limpaNif(nif));
  const cliente = await env.DB.prepare("SELECT * FROM clientes WHERE nif = ?").bind(nif).first();
  if (!cliente) throw new Falha("Cliente não encontrado.", 404);
  const [maquinas, licencas, pagamentos] = await env.DB.batch([
    env.DB.prepare(
      `SELECT m.*, i.recebido_em, i.modo, i.licenca_estado, i.licenca_fim, i.copia_ultima, i.copia_externa_ultima, i.copia_externa_erro
       FROM maquinas m LEFT JOIN instalacoes i ON i.nif = m.nif AND i.maquina = m.maquina
       WHERE m.nif = ? ORDER BY m.ultimo_contacto DESC`,
    ).bind(nif),
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

  // a área do produtor no site (kangest.pages.dev/admin.html) usa a mesma API: o pedido prévio do browser não leva o token
  if (m === "OPTIONS" && (caminho.startsWith("/admin/api/") || caminho.startsWith("/v1/conta/"))) {
    const corsAdmin = origemAdmin(env, req);
    return corsAdmin ? new Response(null, { status: 204, headers: corsAdmin }) : erro("Origem não autorizada.", 403);
  }
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
  if (m === "GET" && caminho === "/v1/web/humano") return json(humanoChave(env));
  if (m === "GET" && caminho === "/v1/web/endereco") {
    const cabec = { ...CORS, "cache-control": "no-store" };
    try {
      await limitar(env, req, "portal");
      return json(await webEndereco(env, url.searchParams.get("nome")), 200, cabec);
    } catch (e) {
      if (!(e instanceof Falha)) throw e;
      return json({ ok: false, erro: e.message }, e.estado, cabec);
    }
  }
  if (m === "POST" && caminho.startsWith("/v1/web/")) {
    const f = { estado: webEstado, activar: webActivar, desactivar: webDesactivar, humano: humanoVerificar }[caminho.slice("/v1/web/".length)];
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
  // portal do cliente (conta da empresa no site)
  if (caminho.startsWith("/v1/conta/")) return rotasConta(env, req, m, caminho.slice("/v1/conta/".length));
  // entrada com email e senha (antes da autorização: é ela que a dá); as falhas contam para o bloqueio por IP
  if (caminho === "/admin/api/entrar" && m === "POST") {
    try {
      return json(await entrarPainel(env, await corpoJson(req), req));
    } catch (e) {
      if (e instanceof Falha && e.estado === 401) await contar(env, req, "admin");
      throw e;
    }
  }
  if (caminho.startsWith("/admin/api/")) {
    if (!(await autorizado(env, req))) {
      await contar(env, req, "admin");
      return erro("Acesso negado.", 401);
    }
    const accao = caminho.slice("/admin/api/".length);
    if (m === "GET" && accao === "resumo") return json(await resumo(env));
    const sessaoActual = await sessaoPainel(env, (req.headers.get("authorization") ?? "").replace(/^Bearer\s+/i, ""));
    if (m === "GET" && accao === "sessao") {
      return json({ ok: true, email: sessaoActual?.e ?? null, expira: sessaoActual ? new Date(sessaoActual.exp).toISOString() : null, chave: !sessaoActual });
    }
    if (m === "GET" && accao === "sessoes") return json(await sessoesPainel(env, sessaoActual?.s));
    if (m === "POST" && (accao === "sessoes/terminar" || accao === "sessoes/terminar-outras")) {
      const c = await corpoJson(req);
      const r = accao === "sessoes/terminar"
        ? await env.DB.prepare("UPDATE sessoes_painel SET terminada_em = ? WHERE id = ? AND terminada_em IS NULL").bind(agora(), String(c.id ?? "")).run()
        : await env.DB.prepare("UPDATE sessoes_painel SET terminada_em = ? WHERE terminada_em IS NULL AND id <> ?").bind(agora(), String(sessaoActual?.s ?? "")).run();
      const n = r.meta?.changes ?? 0;
      return json({ ok: true, terminadas: n, mensagem: n ? `${n} ${n === 1 ? "sessão terminada" : "sessões terminadas"}.` : "Não havia sessões para terminar." });
    }
    if (m === "GET" && accao === "cliente") return json(await fichaCliente(env, url.searchParams.get("nif")));
    if (m === "POST") {
      const c = await corpoJson(req);
      const f = { pagamento: registarPagamento, emitir: emitirManual, plano: guardarPlano, cliente: guardarCliente, maquina: guardarMaquina, codigo: apagarCodigo }[accao];
      if (f) return json(await f(env, c));
    }
  }
  return erro("Não encontrado.", 404);
}

/**
 * Cabeçalhos CORS para a área do produtor no site, se o pedido vier de uma origem autorizada (ADMIN_ORIGENS, por omissão
 * o site do KanGest). O token continua a ser exigido em cada pedido; isto só deixa o browser fazê-lo a partir do site.
 */
export function origemAdmin(env, req) {
  const origem = req.headers.get("origin");
  if (!origem) return null;
  const permitidas = String(env.ADMIN_ORIGENS || "https://kangest.pages.dev").split(",").map((o) => o.trim().replace(/\/$/, "")).filter(Boolean);
  if (!permitidas.includes(origem)) return null;
  return {
    "access-control-allow-origin": origem,
    "access-control-allow-methods": "GET, POST, OPTIONS",
    "access-control-allow-headers": "authorization, content-type",
    "access-control-max-age": "600",
    vary: "origin",
  };
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
    const p = new URL(req.url).pathname;
    const cors = p.startsWith("/admin/api/") || p.startsWith("/v1/conta/") ? origemAdmin(env, req) : null;
    if (cors) for (const [k, v] of Object.entries(cors)) h.set(k, v);
    for (const [k, v] of Object.entries(SEGURANCA)) if (!h.has(k)) h.set(k, v);
    return new Response(r.body, { status: r.status, statusText: r.statusText, headers: h });
  },
};

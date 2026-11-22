// Prova de fumo do servidor de licenças em produção: node servidor-licencas/teste/fumo.mjs [endereço]
// Faz o que um cliente faz, com uma empresa de teste (NIF «FUMO…»), e no fim apaga tudo o que criou:
//   registo → código → confirmação → licença assinada (verificada com a chave pública do KanGest) → renovação com o
//   estado e a equipa → conta da empresa (código, senha, entrada, resumo) → equipa gerida (loja e funcionário) → a
//   renovação leva a equipa ao KanGest → aplicada → conta nova criada no site antes de instalar («por registar», e a
//   experiência começa no registo do programa) → ficha na área do produtor → entrada do produtor recusa senha errada.
// Diz também se o encaminhamento no Supabase (para os KanGest 1.4.0) chega a este servidor.
// O endereço por omissão é o web.licencas do site-fonte/config.json. Precisa da chave do produtor
// (~/.kangest-licencas/servidor/admin-token.txt ou KANGEST_ADMIN_TOKEN) e, para apagar a empresa de teste, do token da
// Cloudflare (~/.kangest-licencas/cloudflare-token.txt ou CLOUDFLARE_API_TOKEN, com «D1 · Edit»). Só mostra resultados e tempos.
import { createPublicKey, verify } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { ligarD1 } from "../copia-d1.mjs";

const CONFIG = JSON.parse(readFileSync(new URL("../../site-fonte/config.json", import.meta.url), "utf8"));
const URL_BASE = (process.argv[2] ?? CONFIG.web.licencas).replace(/\/+$/, "");
const ENCAMINHAMENTO_SUPABASE = "https://fissktkxilseymadzppn.supabase.co/functions/v1/kangest";
const ler = (f) => (existsSync(f) ? readFileSync(f, "utf8").trim() : "");
const ADMIN = process.env.KANGEST_ADMIN_TOKEN || ler(join(homedir(), ".kangest-licencas/servidor/admin-token.txt"));
const CLOUDFLARE = process.env.CLOUDFLARE_API_TOKEN || ler(join(homedir(), ".kangest-licencas/cloudflare-token.txt"));
const EMAIL_PRODUTOR = ler(join(homedir(), ".kangest-licencas/servidor/admin-email.txt"));
if (!ADMIN) {
  console.error("Falta a chave do produtor (~/.kangest-licencas/servidor/admin-token.txt ou KANGEST_ADMIN_TOKEN).");
  process.exit(2);
}

// as chaves públicas com que o KanGest confirma as licenças
const fonte = readFileSync(new URL("../../src-tauri/src/services/licenca.rs", import.meta.url), "utf8");
const CHAVES = [...fonte.match(/CHAVES_PUBLICAS: &\[&str\] = &\[([^\]]+)\]/)[1].matchAll(/"([\w-]+)"/g)].map(([, x]) =>
  createPublicKey({ key: { kty: "OKP", crv: "Ed25519", x }, format: "jwk" }),
);
const assinada = (lic) => {
  const [v, corpo, ass] = String(lic).split(".");
  return v === "KG1" && CHAVES.some((k) => verify(null, Buffer.from(corpo), k, Buffer.from(ass, "base64url")));
};

const n = String(Date.now()).slice(-8);
const NIF = `FUMO${n}`;
const MAQ = `F0F0-${n.slice(0, 4)}-${n.slice(4)}`;
const EMAIL = `fumo-${n}@kangest.invalid`;
const SENHA = `Prova-de-fumo-${n}`;
// a conta criada no site antes de instalar: outra empresa de teste
const NIF_NOVA = `FUMO${n}N`;
const MAQ_NOVA = `F0F1-${n.slice(0, 4)}-${n.slice(4)}`;
const EMAIL_NOVA = `fumo-nova-${n}@kangest.invalid`;
let falhas = 0;

async function pedir(metodo, caminho, corpo, token, base = URL_BASE) {
  const t0 = Date.now();
  const r = await fetch(base + caminho, {
    method: metodo,
    headers: { "content-type": "application/json", "user-agent": "KanGest-prova-de-fumo", ...(token ? { authorization: `Bearer ${token}` } : {}) },
    body: corpo ? JSON.stringify(corpo) : undefined,
    redirect: "manual",
  });
  const texto = await r.text();
  let json = null;
  try { json = JSON.parse(texto); } catch { /* não é JSON */ }
  return { estado: r.status, corpo: json ?? texto, ms: Date.now() - t0, cabecalhos: r.headers };
}
function passo(nome, ok, r, detalhe = "") {
  if (!ok) falhas++;
  const erro = !ok && r?.corpo?.erro ? ` — ${r.corpo.erro}` : "";
  console.log(`${ok ? "✓" : "✗"} ${nome}${r ? ` (${r.estado}, ${r.ms} ms)` : ""}${detalhe ? ` — ${detalhe}` : ""}${erro}`);
  return ok;
}
const codigoAberto = async (tipo, nif = NIF) =>
  (await pedir("GET", "/admin/api/resumo", null, ADMIN)).corpo?.codigos?.find((c) => c.tipo === tipo && c.nif === nif)?.codigo_aberto;

async function apagar() {
  if (!CLOUDFLARE) {
    console.log(`! Sem o token da Cloudflare: a empresa de teste ${NIF} fica na D1. Apague-a à mão.`);
    return;
  }
  try {
    const d1 = await ligarD1({ token: CLOUDFLARE, conta: process.env.CLOUDFLARE_ACCOUNT_ID });
    await d1.consultar("DELETE FROM codigos_conta WHERE nif IN (?1, ?2) OR email IN (?3, ?4)", [NIF, NIF_NOVA, EMAIL, EMAIL_NOVA]);
    await d1.consultar("DELETE FROM pedidos_conta WHERE nif IN (?1, ?2) OR email IN (?3, ?4)", [NIF, NIF_NOVA, EMAIL, EMAIL_NOVA]);
    for (const t of ["funcionarios", "lojas_geridas", "postos_geridos", "equipa_versoes", "equipas", "sessoes_conta", "segredos_maquina",
      "contas", "licencas", "instalacoes", "codigos", "pagamentos", "maquinas", "subdominios", "clientes"]) {
      await d1.consultar(`DELETE FROM ${t} WHERE nif IN (?1, ?2)`, [NIF, NIF_NOVA]);
    }
    const [{ restam }] = await d1.consultar(
      `SELECT (SELECT COUNT(*) FROM clientes WHERE nif IN (?1, ?2)) + (SELECT COUNT(*) FROM licencas WHERE nif IN (?1, ?2))
            + (SELECT COUNT(*) FROM codigos_conta WHERE email IN (?3, ?4)) + (SELECT COUNT(*) FROM pedidos_conta WHERE email IN (?3, ?4)) AS restam`,
      [NIF, NIF_NOVA, EMAIL, EMAIL_NOVA],
    );
    passo(`empresas de teste ${NIF} e ${NIF_NOVA} apagadas`, restam === 0, null);
  } catch (e) {
    passo(`apagar a empresa de teste ${NIF}`, false, null, e.message);
  }
}

console.log(`Prova de fumo em ${URL_BASE}\n`);
try {
  let r = await pedir("GET", "/");
  passo("o serviço responde", r.estado === 200 && r.corpo?.servico === "Licenças do KanGest", r);
  r = await pedir("GET", "/v1/planos");
  passo("planos", r.estado === 200 && r.corpo?.planos?.length > 0, r, r.corpo?.planos?.map((p) => `${p.codigo} ${p.preco_mes ? p.preco_mes + " Kz/mês" : "sem preço"}`).join(", "));

  // registo como o KanGest o faz
  r = await pedir("POST", "/v1/registo", { nif: NIF, empresa: "Prova de fumo (apagar)", email: EMAIL, telefone: "900 000 000", maquina: MAQ, versao: "1.4.0" });
  passo("registo do computador", r.estado === 200 && r.corpo?.ok, r, r.corpo?.enviado ? "código enviado por email" : "sem envio de email: o código fica na área do produtor");
  const codigo = await codigoAberto("registo");
  passo("o código aparece na área do produtor", /^\d{6}$/.test(codigo ?? ""), null);
  r = await pedir("POST", "/v1/confirmar", { nif: NIF, maquina: MAQ, codigo, versao: "1.4.0" });
  const conf = r.corpo ?? {};
  passo("confirmação: licença de experiência", r.estado === 200 && conf.estado === "EXPERIENCIA", r);
  passo("a licença está assinada com a chave que o KanGest conhece", assinada(conf.licenca), null);

  // verificação periódica, com o estado e a equipa que o KanGest comunica
  const estado = (equipaVersao) => ({
    modo: "loja", licenca: { estado: "EXPERIENCIA", fim: conf.fim }, copias: { ultima: new Date().toISOString().slice(0, 19) },
    equipa: { utilizadores: [{ login: "admin", nome: "Administrador", perfil: "ADMIN", ativo: true }], perfis: [{ codigo: "ADMIN" }, { codigo: "CAIXA" }], lojas: [{ nome: "Loja Fumo", ativo: true }], postos: [] },
    equipa_versao: equipaVersao,
  });
  r = await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.0", estado: estado(0) });
  passo("renovação: a mesma licença", r.estado === 200 && r.corpo?.id === conf.id, r);
  r = await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: "errado", versao: "1.4.0" });
  passo("renovação com o segredo errado é recusada", r.estado === 401, r);

  // conta da empresa no site
  r = await pedir("POST", "/v1/conta/codigo", { email: EMAIL });
  passo("conta: pedir o código", r.estado === 200, r);
  const codigoConta = await codigoAberto("conta");
  r = await pedir("POST", "/v1/conta/definir-senha", { email: EMAIL, codigo: codigoConta, senha: SENHA });
  passo("conta: definir a senha", r.estado === 200, r);
  r = await pedir("POST", "/v1/conta/entrar", { email: EMAIL, senha: SENHA });
  const token = r.corpo?.token;
  passo("conta: entrar", r.estado === 200 && /^c1\./.test(token ?? ""), r);
  r = await pedir("GET", "/v1/conta/resumo", null, token);
  passo("conta: resumo com o computador e a equipa comunicada", r.estado === 200 && r.corpo?.maquinas?.[0]?.maquina === MAQ && r.corpo?.equipa?.utilizadores?.length === 1, r);

  // equipa gerida no site → chega ao KanGest na verificação seguinte
  r = await pedir("POST", "/v1/conta/equipa/funcionario", { login: "fumo", nome: "Funcionária de Teste", perfil: "CAIXA", loja: "Loja Fumo" }, token);
  passo("conta: novo funcionário com senha provisória", r.estado === 200 && /-/.test(r.corpo?.senha_provisoria ?? ""), r);
  r = await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.0", estado: estado(0) });
  const g = r.corpo?.equipa_gerida;
  passo("a renovação leva a equipa gerida ao KanGest", r.estado === 200 && g?.versao > 0 && g.funcionarios?.some((f) => f.login === "fumo" && /^pbkdf2-sha256\$/.test(f.senha_provisoria ?? "")), r);
  r = await pedir("POST", "/v1/renovar", { nif: NIF, maquina: MAQ, segredo: conf.segredo, versao: "1.4.0", estado: estado(g?.versao ?? 0) });
  passo("com a versão aplicada, já não a volta a mandar", r.estado === 200 && !r.corpo?.equipa_gerida, r);
  r = await pedir("GET", "/v1/conta/equipa", null, token);
  passo("conta: a equipa aparece como aplicada", r.estado === 200 && r.corpo?.versao?.aplicada === g?.versao, r);
  r = await pedir("POST", "/v1/conta/sair", {}, token);
  passo("conta: sair", r.estado === 200 && (await pedir("GET", "/v1/conta/resumo", null, token)).estado === 401, r);

  // conta nova no site, antes de instalar: fica «por registar» até o programa se registar com esse NIF
  r = await pedir("POST", "/v1/conta/criar", { empresa: "Prova de fumo, conta nova (apagar)", nif: NIF_NOVA, email: EMAIL_NOVA });
  passo("conta nova: pedir o código", r.estado === 200 && r.corpo?.ok, r);
  const codigoNova = await codigoAberto("conta-nova", NIF_NOVA);
  passo("conta nova: o pedido aparece na área do produtor", /^\d{6}$/.test(codigoNova ?? ""), null);
  r = await pedir("POST", "/v1/conta/criar/confirmar", { email: EMAIL_NOVA, codigo: codigoNova, senha: SENHA });
  const tokenNova = r.corpo?.token;
  passo("conta nova: criada, com a sessão aberta", r.estado === 200 && /^c1\./.test(tokenNova ?? ""), r);
  r = await pedir("GET", "/v1/conta/resumo", null, tokenNova);
  passo("conta nova: por registar, sem experiência a contar", r.estado === 200 && r.corpo?.empresa?.estado === "POR_REGISTAR" && !r.corpo?.empresa?.experiencia_ate, r);
  await pedir("POST", "/v1/registo", { nif: NIF_NOVA, empresa: "Prova de fumo, conta nova (apagar)", email: EMAIL_NOVA, maquina: MAQ_NOVA, versao: "1.4.1" });
  r = await pedir("POST", "/v1/confirmar", { nif: NIF_NOVA, maquina: MAQ_NOVA, codigo: await codigoAberto("registo", NIF_NOVA), versao: "1.4.1" });
  passo("conta nova: o registo do programa começa a experiência", r.estado === 200 && r.corpo?.estado === "EXPERIENCIA" && assinada(r.corpo?.licenca), r);
  r = await pedir("GET", "/v1/conta/resumo", null, tokenNova);
  passo("conta nova: a conta continua, já em experiência", r.estado === 200 && r.corpo?.empresa?.estado === "EXPERIENCIA", r);

  // área do produtor
  r = await pedir("GET", `/admin/api/cliente?nif=${NIF}`, null, ADMIN);
  passo("produtor: ficha do cliente com o estado da instalação", r.estado === 200 && r.corpo?.maquinas?.[0]?.modo === "loja", r);
  if (EMAIL_PRODUTOR) {
    r = await pedir("POST", "/admin/api/entrar", { email: EMAIL_PRODUTOR, senha: `errada-${n}` });
    passo("produtor: a entrada com email e senha recusa a senha errada", r.estado === 401, r);
  }

  // o encaminhamento no Supabase (os KanGest 1.4.0 falam com ele) chega a este servidor: a empresa de teste só existe aqui
  if (URL_BASE !== ENCAMINHAMENTO_SUPABASE) {
    r = await pedir("GET", `/admin/api/cliente?nif=${NIF}`, null, ADMIN, ENCAMINHAMENTO_SUPABASE).catch(() => null);
    passo("o encaminhamento no Supabase (KanGest 1.4.0) chega a este servidor", r?.estado === 200 && r.corpo?.maquinas?.[0]?.maquina === MAQ, r);
  }
} catch (e) {
  passo(`erro inesperado: ${e.message}`, false, null);
} finally {
  await apagar();
}

console.log(falhas ? `\n${falhas} falha(s).` : "\nTudo a funcionar.");
process.exitCode = falhas ? 1 : 0;

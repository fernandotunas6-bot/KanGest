#!/usr/bin/env node
// Licenças do KanGest (ferramenta do produtor). Sem dependências: usa o Ed25519 do Node.
//
//   node scripts/licencas/kangest-licencas.mjs chaves
//       cria o par de chaves em ~/.kangest-licencas/ (se ainda não existir) e mostra a chave pública,
//       que vai em src-tauri/src/services/licenca.rs (CHAVES_PUBLICAS)
//
//   node scripts/licencas/kangest-licencas.mjs emitir --nif 5417189230 --empresa "Comércio Kianda, Lda" \
//       --plano COMERCIO --postos 3 --meses 12 [--inicio 2026-11-13] [--ate 2027-11-12] [--maquina 3F9A-12BC-77D0] \
//       [--tolerancia 15] [--modulos rh,oficina]
//       mostra a licença (KG1.…) para o cliente colar em Sistema → Licença e Assinatura
//
//   node scripts/licencas/kangest-licencas.mjs ver KG1.…
//       mostra o conteúdo de uma licença e confirma a assinatura
//
// A CHAVE PRIVADA NUNCA VAI PARA O REPOSITÓRIO (é público). Faça cópia de ~/.kangest-licencas/ num sítio seguro:
// sem ela não se emitem licenças que as instalações existentes aceitem.
import { createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign, verify } from "node:crypto";
import { appendFileSync, chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const PASTA = process.env.KANGEST_LICENCAS ?? join(homedir(), ".kangest-licencas");
const PRIVADA = join(PASTA, "privada.pem");
const REGISTO = join(PASTA, "emitidas.csv");
const PLANOS = { ESSENCIAL: 1, COMERCIO: 3, EMPRESA: 0 }; // postos por omissão (0 = sem limite)

const b64url = (b) => Buffer.from(b).toString("base64url");
const publicaRaw = (chave) => createPublicKey(chave).export({ format: "der", type: "spki" }).subarray(-32);

function args(lista) {
  const o = {};
  for (let i = 0; i < lista.length; i++) {
    if (lista[i].startsWith("--")) o[lista[i].slice(2)] = lista[i + 1]?.startsWith("--") || lista[i + 1] === undefined ? true : lista[++i];
  }
  return o;
}
const isoDia = (d) => d.toISOString().slice(0, 10);
function somarMeses(dia, meses) {
  const d = new Date(dia + "T12:00:00Z");
  d.setUTCMonth(d.getUTCMonth() + meses);
  d.setUTCDate(d.getUTCDate() - 1);
  return isoDia(d);
}

const [comando, ...resto] = process.argv.slice(2);

if (comando === "chaves") {
  mkdirSync(PASTA, { recursive: true, mode: 0o700 });
  if (!existsSync(PRIVADA)) {
    const { privateKey } = generateKeyPairSync("ed25519");
    writeFileSync(PRIVADA, privateKey.export({ format: "pem", type: "pkcs8" }), { mode: 0o600 });
    chmodSync(PRIVADA, 0o600);
    console.log(`Chave privada criada em ${PRIVADA} (guarde uma cópia num sítio seguro).`);
  } else {
    console.log(`A chave privada já existe em ${PRIVADA}.`);
  }
  const pub = b64url(publicaRaw(createPrivateKey(readFileSync(PRIVADA))));
  writeFileSync(join(PASTA, "publica.txt"), pub + "\n");
  console.log(`Chave pública (para CHAVES_PUBLICAS no licenca.rs): ${pub}`);
} else if (comando === "emitir") {
  const a = args(resto);
  if (!existsSync(PRIVADA)) throw new Error("Não há chave privada: corra primeiro «chaves».");
  const nif = String(a.nif ?? "").replace(/\D/g, "");
  if (!nif) throw new Error("Indique --nif.");
  if (!a.empresa) throw new Error("Indique --empresa.");
  const plano = String(a.plano ?? "COMERCIO").toUpperCase();
  if (!(plano in PLANOS)) throw new Error(`Plano desconhecido: ${plano} (${Object.keys(PLANOS).join(", ")}).`);
  const inicio = a.inicio ?? isoDia(new Date());
  const fim = a.ate ?? somarMeses(inicio, Number(a.meses ?? 1));
  const lic = {
    v: 1,
    id: "LIC-" + randomBytes(5).toString("hex").toUpperCase(),
    nif,
    empresa: String(a.empresa),
    plano,
    postos: a.postos !== undefined ? Number(a.postos) : PLANOS[plano],
    modulos: a.modulos ? String(a.modulos).split(",").map((m) => m.trim()).filter(Boolean) : [],
    inicio,
    fim,
    tolerancia: Number(a.tolerancia ?? 15),
    emitida: isoDia(new Date()),
    ...(a.maquina ? { maquina: String(a.maquina).toUpperCase() } : {}),
  };
  const corpo = b64url(JSON.stringify(lic));
  const assinatura = b64url(sign(null, Buffer.from(corpo), createPrivateKey(readFileSync(PRIVADA))));
  const chave = `KG1.${corpo}.${assinatura}`;
  appendFileSync(REGISTO, `${lic.emitida};${lic.id};${nif};"${lic.empresa.replace(/"/g, "'")}";${plano};${lic.postos};${inicio};${fim}\n`);
  console.log(`Licença ${lic.id} · ${lic.empresa} (NIF ${nif}) · ${plano} · ${lic.postos || "sem limite de"} postos · ${inicio} a ${fim}\n`);
  console.log(chave);
} else if (comando === "ver") {
  const chave = (resto[0] ?? "").replace(/\s+/g, "");
  const [pref, corpo, assinatura] = chave.split(".");
  if (pref !== "KG1" || !corpo || !assinatura) throw new Error("Não é uma licença do KanGest (KG1.…).");
  const lic = JSON.parse(Buffer.from(corpo, "base64url").toString());
  let valida = "sem chave privada para confirmar";
  if (existsSync(PRIVADA)) {
    valida = verify(null, Buffer.from(corpo), createPublicKey(createPrivateKey(readFileSync(PRIVADA))), Buffer.from(assinatura, "base64url")) ? "assinatura válida" : "ASSINATURA INVÁLIDA";
  }
  console.log(JSON.stringify(lic, null, 2), `\n${valida}`);
} else {
  console.log("Uso: kangest-licencas.mjs chaves | emitir --nif … --empresa … [--plano …] [--meses N | --ate AAAA-MM-DD] | ver KG1.…");
}

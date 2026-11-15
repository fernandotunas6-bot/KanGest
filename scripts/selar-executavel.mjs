// Selo de integridade do executável (beforeBundleCommand do Tauri): corre depois de compilar e antes de empacotar,
// assina o resumo do executável com a chave do produtor e grava src-tauri/selo/selo.json, que vai com a aplicação.
// Ver docs/SEGURANCA.md.
//
// Sem KANGEST_SELO_PRIVADA (compilações locais, ou antes de criar os segredos no GitHub), o selo fica vazio e o
// KanGest funciona como até aqui («sem selo»). Com KANGEST_SELO_PUBLICA e sem a privada, pára: a compilação exigiria
// um selo que não pode ter, e nenhuma instalação poderia facturar.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const raiz = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauri = join(raiz, "src-tauri");
const saida = join(tauri, "selo", "selo.json");
const vazio = () => writeFileSync(saida, JSON.stringify({ v: 1, nota: "Compilação sem selo (não oficial)." }, null, 2) + "\n");

const privada = (process.env.KANGEST_SELO_PRIVADA || "").trim();
const publica = (process.env.KANGEST_SELO_PUBLICA || "").trim();
if (!privada || !publica) {
  if (publica) {
    console.error("Selo: KANGEST_SELO_PUBLICA está definida e KANGEST_SELO_PRIVADA não. Defina as duas ou nenhuma.");
    process.exit(1);
  }
  vazio();
  console.log("Selo: sem as chaves KANGEST_SELO_*, o executável fica sem selo (compilação não oficial).");
  process.exit(0);
}

const alvo = process.env.TAURI_ENV_TARGET_TRIPLE || "";
const nome = alvo.includes("windows") || (!alvo && process.platform === "win32") ? "kigest.exe" : "kigest";
const perfil = process.env.TAURI_ENV_DEBUG === "true" ? "debug" : "release";
const candidatos = [alvo && join(tauri, "target", alvo, perfil, nome), join(tauri, "target", perfil, nome)]
  .filter((c) => c && existsSync(c))
  .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs); // o que acabou de ser compilado
if (!candidatos.length) {
  console.error(`Selo: não encontrei o executável compilado (${nome}, alvo «${alvo}»).`);
  process.exit(1);
}
const versao = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(tauri, "Cargo.toml"), "utf8"))?.[1];
if (!versao) {
  console.error("Selo: não encontrei a versão em src-tauri/Cargo.toml.");
  process.exit(1);
}

// a ferramenta corre nesta máquina, mesmo quando o KanGest é compilado para outra arquitectura
const env = { ...process.env };
delete env.CARGO_BUILD_TARGET;
execFileSync(
  "cargo",
  ["run", "--quiet", "--release", "--manifest-path", join(raiz, "scripts", "selo", "Cargo.toml"), "--", "assinar", candidatos[0], saida, versao, alvo || process.platform],
  { stdio: "inherit", env },
);

# Publicar actualizações do KanGest

O KanGest verifica actualizações em **Sistema → Sobre o KanGest → Actualizações**. A origem é um endereço (`https://…`) ou uma pasta partilhada na rede (`\\SERVIDOR\KanGest`). Cada posto actualiza-se a si próprio; antes de instalar é feita uma cópia de segurança da base em `copias/kangest-antes-de-<versão>.db`.

## 1. Gerar os instaladores

```bash
npm run tauri build      # Windows: …/bundle/nsis/KanGest_0.2.0_x64-setup.exe · macOS: …/bundle/dmg/KanGest_0.2.0_aarch64.dmg
```

## 2. (Recomendado) Assinar os pacotes

Gere uma vez o par de chaves e guarde a chave privada fora do repositório:

```bash
npx tauri signer generate -w ~/.kigest/actualizacoes.key
npx tauri signer sign -f ~/.kigest/actualizacoes.key KanGest_0.2.0_x64-setup.exe   # cria KanGest_0.2.0_x64-setup.exe.sig
```

Na compilação oficial, a **chave pública vem dentro do programa** (variável `KANGEST_CHAVE_ACTUALIZACOES` do repositório,
ver [CERTIFICACAO-AGT.md](CERTIFICACAO-AGT.md)): a assinatura passa a ser obrigatória e a chave não se pode trocar no
ecrã, por isso uma origem adulterada não consegue instalar nada. Nas compilações de desenvolvimento, pode-se colar a
chave pública em cada instalação (campo «Chave pública de assinatura»).

O SHA-256 conferido no descarregamento fica ao lado do pacote e volta a conferir-se antes de abrir o instalador: um
ficheiro trocado entretanto não se instala.

## 3. Manifesto `kangest-atualizacao.json`

**Na publicação pelo GitHub Actions o manifesto é gerado sozinho** (trabalho «manifesto das actualizações»: assina os
três instaladores e anexa `kangest-atualizacao.json`, `SHA256SUMS.txt` e os `.sig` à release). Nesse caso a origem a
indicar nos postos é `https://github.com/fernandotunas6-bot/KanGest/releases/latest/download`.

Para uma pasta partilhada, copie para lá os instaladores com os `.sig` e gere o manifesto com
`python3 scripts/manifesto-atualizacao.py <pasta> <versão> [docs/versoes/v<versão>.md]`, ou escreva-o à mão,
na mesma pasta (ou endereço) dos instaladores:

```json
{
  "versao": "0.2.0",
  "data": "2026-12-01",
  "notas": "Facturação electrónica da AGT; correcções nos relatórios.",
  "pacotes": {
    "windows-x86_64": { "ficheiro": "KanGest_0.2.0_x64-setup.exe", "sha256": "…", "assinatura": "<conteúdo do .sig>" },
    "macos-aarch64":  { "ficheiro": "KanGest_0.2.0_aarch64.dmg",  "sha256": "…", "assinatura": "<conteúdo do .sig>" },
    "macos-x86_64":   { "ficheiro": "KanGest_0.2.0_x64.dmg",      "sha256": "…", "assinatura": "<conteúdo do .sig>" }
  }
}
```

O SHA-256 obtém-se com `shasum -a 256 <ficheiro>` (macOS) ou `certutil -hashfile <ficheiro> SHA256` (Windows). O pacote é sempre conferido pelo SHA-256; a assinatura é obrigatória quando a instalação tem chave pública configurada.

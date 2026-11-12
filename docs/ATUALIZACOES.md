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

Cole a **chave pública** em cada instalação (campo «Chave pública de assinatura»). A partir daí, só se instalam pacotes assinados com essa chave.

## 3. Manifesto `kangest-atualizacao.json`

Na mesma pasta (ou endereço) dos instaladores:

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

## Qual instalador escolher

| Computador | Ficheiro |
|---|---|
| Windows 10 ou 11 (64 bits) | `KanGest_{{VERSAO}}_x64-setup.exe` |
| Mac com Apple Silicon (M1, M2, M3, M4) | `KanGest_{{VERSAO}}_aarch64.dmg` |
| Mac com processador Intel | `KanGest_{{VERSAO}}_x64.dmg` |

- Guia de instalação, postos em rede e actualizações: [docs/INSTALACAO.md](https://github.com/fernandotunas6-bot/KanGest/blob/main/docs/INSTALACAO.md).
- Para conferir um ficheiro descarregado, compare o SHA-256 com o de `SHA256SUMS.txt`: `certutil -hashfile <ficheiro> SHA256` (Windows) ou `shasum -a 256 <ficheiro>` (macOS).
- Actualizações automáticas: em Sistema → Sobre o KanGest → Actualizações, origem `https://github.com/fernandotunas6-bot/KanGest/releases/latest/download` (manifesto `kangest-atualizacao.json`).

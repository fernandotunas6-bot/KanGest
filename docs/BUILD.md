# Build e instalação do KanGest

Versão actual: **1.0.0**. O logótipo está em `src/assets/kigest-icone.svg`. Com um Chrome de depuração aberto (ver `scripts/auditoria/README.md`):

- ícones da aplicação: `node scripts/icone.mjs` e depois `npx tauri icon src-tauri/icons/app-icon.png -o src-tauri/icons`;
- imagens do instalador de Windows (cabeçalho 150×57 e lateral 164×314, BMP de 24 bits): `node scripts/instalador-imagens.mjs`.

## Instalador para Windows

O instalador é um `.exe` (NSIS) em português, com o logótipo do KanGest nas páginas de boas-vindas e de fim e no cabeçalho. Instala para o utilizador actual, sem pedir administrador, e cria o atalho no menu Iniciar. Inclui também o instalador do WebView2, para o caso raro de o Windows não o ter.

### Opção A — no próprio PC Windows (recomendado para a migração)

1. Instalar uma vez:
   - [Rust](https://rustup.rs) (escolher o toolchain MSVC; o instalador oferece as «Build Tools do Visual Studio» — aceitar o «Desktop development with C++»);
   - [Node.js 22 LTS](https://nodejs.org).
2. Copiar a pasta `kigest-next` para o PC (sem `node_modules` nem `src-tauri/target`).
3. Numa linha de comandos na pasta:

   ```bat
   npm ci
   npm run tauri build -- --bundles nsis
   ```

4. O instalador fica em `src-tauri\target\release\bundle\nsis\KanGest_1.0.0_x64-setup.exe`.

A primeira compilação demora (descarrega e compila as bibliotecas); as seguintes são rápidas.

### Opção B — GitHub Actions

Com o projecto num repositório GitHub, o workflow `.github/workflows/build.yml` gera o instalador de Windows e os `.dmg` de macOS (Apple Silicon e Intel) ao publicar uma etiqueta `v1.0.0`, ou manualmente em *Actions → Build → Run workflow*. Os ficheiros ficam nos «Artifacts» da execução e num rascunho de *Release*.

## macOS

```bash
npm ci
CI=true npm run tauri build -- --bundles app,dmg
```

`src-tauri/target/release/bundle/macos/KanGest.app` e `bundle/dmg/KanGest_1.0.0_x64.dmg`. Sem certificado de programador Apple, o macOS pede para abrir com o botão direito → «Abrir» na primeira vez.

## Migração dos dados do KIGEST

A base do KIGEST (`KigestGC`) está no SQL Server do computador **DESKTOP-G26LDJM** (ver `Conexao.ini`). Há dois caminhos; o primeiro funciona em qualquer caso.

### A. Ficheiro exportado (KanGest noutro computador, ou sem rede entre os dois)

1. No KanGest: **Sistema → Importar dados do KIGEST → Guardar o ExportarKIGEST**. Ficam nas Transferências `ExportarKIGEST.bat`, `ExportarKIGEST.ps1` e o LEIA-ME (também em `ferramentas/exportar-kigest/`).
2. Copiar os dois ficheiros para o DESKTOP-G26LDJM (pen), fechar o GC e o POS, e fazer duplo clique em `ExportarKIGEST.bat`. Não é preciso instalar nada: usa o PowerShell do Windows, lê o `Conexao.ini`, só lê a base e grava `KIGEST-exportacao-AAAAMMDD-HHMM.kigest` no Ambiente de Trabalho.
3. Trazer o `.kigest` para o computador do KanGest. Na mesma página, em **Ficheiro exportado**, escolher o ficheiro (a lista mostra os das Transferências, do Ambiente de Trabalho, dos Documentos e das pens) ou arrastá-lo para a janela.
4. **Analisar ficheiro** → **Simular** → rever a conferência e os avisos → **Importar dados**.

### B. Ligação directa (KanGest instalado no próprio DESKTOP-G26LDJM)

1. Instalar `KanGest_1.0.0_x64-setup.exe` e criar a empresa e o administrador (são substituídos pelos dados do KIGEST).
2. **Sistema → Importar dados do KIGEST → Ligação directa ao SQL Server → Ler Conexao.ini** (servidor `localhost`).
3. **Ligar e analisar** → **Simular** → **Importar dados**.

Nos dois casos é feita uma cópia de segurança do KanGest antes de importar, o relatório final mostra a conferência KIGEST × KanGest e as senhas provisórias dos utilizadores, e a importação pode ser repetida sem duplicar.

Depois, rever: Sistema → Sobre (contagens), Tesouraria (saldos das contas), Contas a Receber, Stock e Funcionários.

## Depois de instalar

- **Certificação AGT:** Sistema → Configurações → Certificação AGT (n.º do certificado e chave do produtor).
- **Facturação electrónica:** Sistema → Facturação Electrónica (ver `FACTURACAO-ELECTRONICA.md`).
- **Postos em rede:** um PC como servidor, os outros como postos (Configurações → Rede).
- **Actualizações:** Sobre → Actualizações (ver `ATUALIZACOES.md`).

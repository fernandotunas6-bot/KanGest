# Instalar o KanGest

Guia para quem instala o KanGest num computador da loja ou do escritório. Para compilar os instaladores, ver
[BUILD.md](BUILD.md); para publicar uma versão, [ATUALIZACOES.md](ATUALIZACOES.md).

## 1. Requisitos

| | |
|---|---|
| Windows | Windows 10 ou 11, 64 bits |
| macOS | macOS 10.15 (Catalina) ou mais recente; Apple Silicon ou Intel |
| Ecrã | Pelo menos 1024 × 680 (o tamanho mínimo da janela; o POS foi verificado em 1024×680, 1280×720, 1366×768 e 1920×1080) |
| Memória e disco | Recomendado: 4 GB de memória (8 GB no computador que serve os postos em rede) e 1 GB livre para o programa, a base de dados e as cópias de segurança |
| Internet | Não é precisa para vender. Serve para a licença, as actualizações, a Facturação Electrónica da AGT, o email e a API pela Cloudflare |

## 2. Qual instalador

Na página [Releases](https://github.com/fernandotunas6-bot/KanGest/releases) (ou no site):

| Computador | Ficheiro |
|---|---|
| Windows 10 ou 11 | `KanGest_<versão>_x64-setup.exe` |
| Mac com Apple Silicon (M1, M2, M3, M4) | `KanGest_<versão>_aarch64.dmg` |
| Mac com processador Intel | `KanGest_<versão>_x64.dmg` |

No Mac, o tipo de processador está em  → Acerca deste Mac («Chip Apple…» ou «Processador Intel…»).

Para confirmar que o ficheiro descarregado está inteiro e é o publicado, compare o SHA-256 com o de `SHA256SUMS.txt`
(na mesma release):

```bat
certutil -hashfile KanGest_1.2.0_x64-setup.exe SHA256
```

```bash
shasum -a 256 KanGest_1.2.0_aarch64.dmg
```

## 3. Windows

1. Duplo clique no `…_x64-setup.exe`. O instalador está em português e **não pede administrador**: instala só para o
   utilizador do Windows que o corre e cria o atalho no menu Iniciar. Num computador partilhado por vários utilizadores
   do Windows, instale em cada um que vá usar o KanGest.
2. Se o Windows mostrar «O Windows protegeu o seu PC» (instalador sem certificado de assinatura), carregue em
   **Mais informações → Executar mesmo assim**.
3. O KanGest usa o WebView2 do Windows (já vem no Windows 11 e no Windows 10 actualizado). Se faltar, o instalador
   descarrega-o: nesse caso é preciso Internet durante a instalação.

Os dados ficam em `%APPDATA%\ao.kigest.app` (base `kigest.db`, cópias em `copias\`). Desinstalar (Definições → Aplicações)
**não apaga os dados**, a não ser que marque a opção de apagar os dados da aplicação.

## 4. macOS

1. Abra o `.dmg` e arraste o **KanGest** para **Aplicações**.
2. Sem assinatura da Apple, na primeira vez: botão direito no KanGest → **Abrir** → **Abrir** (ou Definições do Sistema →
   Privacidade e Segurança → «Abrir mesmo assim»).

Os dados ficam em `~/Library/Application Support/ao.kigest.app`.

## 5. Primeiro arranque

- **Empresa nova:** o assistente pede os dados da empresa (nome, NIF) e o administrador. A experiência de 30 dias começa
  aqui; para a prender ao NIF e receber a licença sozinha, registe o KanGest em Sistema → Licença e Assinatura
  ([LICENCAS.md](LICENCAS.md)).
- **Computador novo, com uma cópia de segurança:** no assistente, «Já tem uma cópia de segurança do KanGest?» restaura a
  cópia (`.db`, ou `.kgc` cifrada com senha) em vez de configurar do zero ([COPIAS.md](COPIAS.md)).
- **Vindo do KIGEST:** Sistema → Importar dados do KIGEST ([MIGRACAO-KIGEST.md](MIGRACAO-KIGEST.md)).

## 6. Vários postos (rede local)

1. Instale o KanGest em todos os computadores.
2. No computador que guarda os dados: Configurações → Rede → **Servidor**, com uma chave de rede longa. Se a firewall do
   Windows perguntar, autorize as **redes privadas** (porta 7799).
3. Em cada posto: Configurações → Rede → **Posto em rede**, com o endereço do servidor e a mesma chave.
4. Actualize sempre **primeiro o servidor e depois os postos**: postos e servidor têm de ter a mesma versão.

Pormenores em [REDE.md](REDE.md). A API de integração usa outra porta (7800) e chaves próprias ([API.md](API.md)).

## 7. Actualizações

Em Sistema → Sobre o KanGest → **Actualizações**, indique a origem
`https://github.com/fernandotunas6-bot/KanGest/releases/latest/download` (ou a pasta partilhada onde a empresa guarda os
instaladores). O KanGest lê o manifesto `kangest-atualizacao.json`, descarrega o instalador do seu sistema, confere o
SHA-256 e a assinatura, faz uma cópia de segurança da base e abre o instalador.

## 8. Problemas comuns

| Sintoma | O que fazer |
|---|---|
| «O Windows protegeu o seu PC» | Mais informações → Executar mesmo assim (instalador ainda sem certificado) |
| No Mac, «não é possível abrir porque o programador não pode ser verificado» | Botão direito → Abrir, uma vez |
| Um posto diz «versão antiga» ao ligar ao servidor | Actualize o servidor e o posto para a mesma versão |
| O posto não encontra o servidor | Mesmo endereço e porta nos dois; firewall do servidor a aceitar a porta 7799 em redes privadas |
| «Este KanGest foi alterado depois de publicado» | Reinstale a partir da página oficial ([SEGURANCA.md](SEGURANCA.md)) |
| Restaurar diz «versão mais recente» | Actualize primeiro o KanGest deste computador |

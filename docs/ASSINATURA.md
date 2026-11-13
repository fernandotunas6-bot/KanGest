# Certificação dos instaladores (assinatura de código)

Sem assinatura, o Windows mostra «O Windows protegeu o seu PC» e o macOS diz que não pode verificar o programador.
Para os avisos desaparecerem, os instaladores têm de ser assinados com certificados de programador da Apple e de uma
autoridade de certificação aceite pela Microsoft. **Os certificados compram-se em nome da empresa (ou da pessoa) que
distribui o KanGest; ninguém os pode comprar por si.**

A compilação (`.github/workflows/build.yml`) já está preparada: assim que os segredos abaixo existirem no repositório
do GitHub, a próxima versão (`git tag v1.x.y && git push origin v1.x.y`) sai assinada. Até lá:

- **macOS:** assinatura «ad-hoc» (evita o «está danificado» nos Mac Apple Silicon; o aviso do programador continua);
- **Windows:** sem assinatura.

O site (`site-fonte/paginas/baixar.html`) explica como abrir o KanGest enquanto não há certificados.

## macOS — Apple Developer Program

Custo: **99 USD por ano**. Dá o certificado «Developer ID Application» e a notarização (a Apple verifica o programa e
o macOS deixa de avisar).

1. Inscrever em <https://developer.apple.com/programs/enroll/>:
   - **como empresa** (o nome da empresa aparece como programador): precisa de um número **D-U-N-S** (gratuito, pedido
     em <https://developer.apple.com/enroll/duns-lookup/>; pode levar 1–2 semanas) e de poder assinar pela empresa;
   - **como pessoa**: mais rápido, mas aparece o nome da pessoa.
2. Num Mac, criar o certificado: Xcode → Settings → Accounts → Manage Certificates → «+» → **Developer ID
   Application** (só o titular da conta o pode criar). Alternativa: developer.apple.com → Certificates → «+».
3. Exportar o certificado com a chave: Acesso a Porta-chaves → «Os meus certificados» → botão direito em
   «Developer ID Application: …» → Exportar → formato `.p12`, com uma palavra-passe.
4. Converter para texto: `base64 -i certificado.p12 | pbcopy` (fica copiado).
5. Palavra-passe de aplicação para a notarização: <https://account.apple.com> → Iniciar sessão e segurança →
   Palavras-passe específicas de aplicações → «KanGest CI».
6. O **Team ID** (10 caracteres) está em developer.apple.com → Account → Membership details.

Segredos a criar no GitHub:

| Segredo | Valor |
|---|---|
| `APPLE_CERTIFICATE` | o texto base64 do `.p12` (passo 4) |
| `APPLE_CERTIFICATE_PASSWORD` | a palavra-passe do `.p12` (passo 3) |
| `APPLE_ID` | o email da conta Apple de programador |
| `APPLE_PASSWORD` | a palavra-passe de aplicação (passo 5) |
| `APPLE_TEAM_ID` | o Team ID (passo 6) |

## Windows — certificado de assinatura na nuvem (SSL.com eSigner)

O serviço barato da Microsoft (Azure Artifact Signing) **não aceita empresas de Angola**: só EUA, Canadá, União
Europeia e Reino Unido. Fica uma autoridade de certificação com assinatura na nuvem que funcione no GitHub; a
compilação está preparada para o **SSL.com eSigner** (pela ferramenta jsign, que também serve para DigiCert KeyLocker,
Certum, Google Cloud KMS e outros; ver `scripts/assinar-windows.ps1`).

1. Comprar em <https://www.ssl.com/> um **Code Signing Certificate** (OV, em nome da empresa; o EV é mais caro e,
   desde 2024, já não tira o aviso mais depressa) com **eSigner** (assinatura na nuvem, sem pen USB). Ver os preços
   actuais no site.
2. Validação da empresa: a SSL.com pede documentos (certidão comercial, NIF, comprovativo de morada, telefone
   verificável). Os dados têm de bater certo com o nome que vai aparecer como editor.
3. Depois de emitido, na página da encomenda: **activar o eSigner**, escolher um PIN e guardar o **segredo TOTP**
   (o texto base64 que aparece com o código QR) e o **Credential ID** do certificado.
4. Ver no plano do eSigner quantas assinaturas por mês inclui: cada versão assina alguns ficheiros (o programa, o
   instalador e, em geral, o desinstalador).

Segredos a criar no GitHub:

| Segredo | Valor |
|---|---|
| `WINDOWS_ASSINATURA_CONTA` | `utilizador\|palavra-passe` da conta SSL.com |
| `WINDOWS_ASSINATURA_SEGREDO` | o segredo TOTP (passo 3) |
| `WINDOWS_ASSINATURA_CERTIFICADO` | o Credential ID (passo 3) |
| `WINDOWS_ASSINATURA_TIPO` | só se não for o eSigner (ex.: `DIGICERTONE`, `GOOGLECLOUD`) |

Mesmo assinado, o Windows pode ainda avisar nas primeiras transferências: o SmartScreen ganha confiança com o número
de pessoas que instalam. A diferença é que aparece o nome da empresa em vez de «Editor desconhecido».

## Criar os segredos no GitHub

Quem administra o repositório `fernandotunas6-bot/KanGest`: Settings → Secrets and variables → Actions →
**New repository secret**, um por linha das tabelas. Ou, no terminal, com o `gh` dessa conta:

```bash
gh secret set APPLE_CERTIFICATE -R fernandotunas6-bot/KanGest < certificado.b64
gh secret set APPLE_CERTIFICATE_PASSWORD -R fernandotunas6-bot/KanGest
```

Os segredos nunca vão para ficheiros do repositório (é público).

## Confirmar

1. Testar sem publicar: `git push origin HEAD:build/assinatura` compila e guarda os instaladores na execução
   (Actions → Build → artefactos). Os passos «Confirmar a assinatura» mostram quem assinou.
2. Publicar: subir a versão (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) e criar a tag.
3. No Mac: `spctl -a -vv /Applications/KanGest.app` deve dizer `accepted` e `source=Notarized Developer ID`.
   No Windows: propriedades do instalador → Assinaturas digitais.

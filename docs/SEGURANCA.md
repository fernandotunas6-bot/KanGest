# Segurança: cifra e protecção do programa

O que está cifrado ou selado, como se activa e, sobretudo, até onde vai. Nenhuma protecção que corre no computador do
cliente é inquebrável: quem controla o computador pode examinar e alterar o programa. O objectivo é que alterar o
KanGest ou os seus dados dê muito trabalho, deixe rasto e não funcione às cegas.

| O quê | Protecção | Onde |
|---|---|---|
| Ligação entre os postos e o servidor (rede local) | AES-256-GCM, chave derivada da chave de rede (PBKDF2-HMAC-SHA256), contra repetições | [REDE.md](REDE.md) |
| Segredos na base (chave de assinatura AGT de desenvolvimento, senha e chaves da Facturação Electrónica, senha do email, segredos dos avisos) | AES-256-GCM, chave mestra fora da base (`kangest.chave`) | abaixo, §4 |
| Datas da licença na base (início da experiência, última data vista) | Selo HMAC-SHA256 com chave fora da base | abaixo, §1 |
| Executável do KanGest | Selo de integridade Ed25519 posto na compilação oficial | abaixo, §2 |
| Depuradores (engenharia reversa com o programa a correr) | A emissão pára com um depurador ligado | abaixo, §3 |
| Licenças | Assinatura Ed25519 do produtor | [LICENCAS.md](LICENCAS.md) |
| API de integração pela Internet | Chaves de API (guardadas só em SHA-256), Cloudflare Tunnel e Access (JWT RS256) | [API.md](API.md), [CLOUDFLARE.md](CLOUDFLARE.md) |
| Entrada pela Internet (subdomínio) | Verificação em dois passos (TOTP) por utilizador, que o administrador pode exigir; PIN recusado | [UTILIZADORES.md](UTILIZADORES.md) |
| Avisos automáticos (webhooks) | Assinatura HMAC-SHA256 de cada aviso | [API.md](API.md) |

## 1. Selo dos dados da licença

O período de experiência conta a partir de `licenca.instalada_em` e o relógio é vigiado com `licenca.ultimo_visto`,
ambos na tabela `config`. Antes, bastava mudar a primeira data na base (com um editor de SQLite) para ganhar mais 30
dias, ou apagar a segunda para esconder um relógio atrasado.

Agora as duas datas ficam também seladas em `licenca.selo` (HMAC-SHA256, com uma chave que não está na base e que não
aparece inteira no executável):

- **Mexer na tabela à mão só pode piorar**: a experiência conta da data mais antiga (a da tabela ou a do selo) e o
  relógio compara-se com a utilização mais recente. A data verdadeira volta a ser escrita na tabela.
- **Selo apagado ou inventado**: valem os registos da própria base — o primeiro e o último registo da auditoria, a
  criação dos utilizadores e os documentos emitidos (os importados de outro programa não contam). Fica uma linha na
  auditoria: «Os dados da licença foram alterados fora do KanGest: repostos a partir dos registos da base».
- **Primeira vez**: numa base nova, ou vinda de uma versão anterior, o selo é criado com o que estiver na tabela e fica
  registado na auditoria («Selo dos dados da licença criado»).
- Mudar a base para outro computador (cópia de segurança, computador novo) não estraga o selo.

## 2. Selo de integridade do executável

Na compilação oficial (GitHub Actions), entre compilar e empacotar, o `scripts/selar-executavel.mjs` calcula o resumo
SHA-256 do executável e assina-o com a chave privada do produtor. O selo (`selo.json`) vai dentro do instalador, ao lado
do programa. A chave pública fica compilada no próprio executável.

Ao arrancar, o KanGest volta a calcular o resumo do seu executável e confirma a assinatura. Se não bater certo — o
executável foi alterado, por exemplo para saltar a verificação da licença, ou o selo foi trocado ou apagado — o KanGest
passa a **«Programa alterado»**: abre, consulta, imprime e exporta o SAF-T, mas não emite documentos até ser reinstalado
a partir do site oficial. O estado aparece em **Sistema → Licença e Assinatura → Integridade do programa**.

O resumo conta o conteúdo de todas as secções do executável (código, constantes — incluindo as chaves públicas das
licenças —, dados), o endereço de cada secção e o ponto de entrada. Não conta o que muda legitimamente depois de
compilar: a assinatura de código do Windows (Authenticode) e do macOS (codesign) e o tipo de pacote que o Tauri escreve
no executável ao empacotar. O código está em `src-tauri/src/selo/` e é o mesmo que a ferramenta da compilação usa.

### Activar (uma vez)

1. Gere o par de chaves num computador de confiança:

   ```bash
   cargo run --manifest-path scripts/selo/Cargo.toml -- chaves
   ```

2. No GitHub: **Settings → Secrets and variables → Actions → New repository secret**, crie `KANGEST_SELO_PRIVADA` e
   `KANGEST_SELO_PUBLICA` com os dois valores. Guarde a chave privada também num cofre de senhas: não se mostra a
   ninguém nem vai para o código.
3. A partir da compilação seguinte, cada instalador sai selado. A GitHub Actions confirma o selo dos executáveis já
   empacotados antes de publicar (passo «Confirmar o selo de integridade») e falha se não bater certo.

Sem os dois segredos, os instaladores saem sem selo e o KanGest funciona como até aqui («Sem selo (compilação não
oficial)»). Com só um deles, a compilação pára. Mudar de chave: gere outro par e troque os dois segredos; as versões
já instaladas continuam com o seu selo.

## 3. Depuradores

Nas compilações oficiais, se houver um depurador ligado ao KanGest (x64dbg, WinDbg, lldb…), a emissão de documentos
pára com a mensagem «Há uma ferramenta de depuração ligada ao KanGest» e volta quando a ferramenta é fechada. É a forma
mais comum de estudar o programa a correr para descobrir onde está a verificação da licença.

## 4. Segredos guardados na base

A chave privada que assina os documentos nas compilações de desenvolvimento (AGT), a senha e as chaves da Facturação
Electrónica, a senha do email e os segredos dos avisos automáticos vão cifrados com AES-256-GCM
(`src-tauri/src/segredos.rs`): quem abre a base com um editor de SQLite, ou lê uma cópia de segurança, vê `kgs1:…` em
vez da chave ou da senha. Na compilação oficial, a chave do produtor e as credenciais da Facturação Electrónica nem
sequer vão para a base: vêm dentro do programa ([CERTIFICACAO-AGT.md](CERTIFICACAO-AGT.md)).

- A chave mestra fica no **cofre do sistema**: no Windows, no Gestor de Credenciais, protegido pela DPAPI, ligado à
  conta do utilizador do sistema. No macOS, no Porta-chaves, mas **só nas compilações assinadas com o certificado da
  Apple** (o `build.yml` define `KANGEST_COFRE_MACOS=1`): o Porta-chaves só entrega a chave sem perguntar à mesma
  aplicação assinada, e com a assinatura «ad-hoc» cada actualização seria outra aplicação, por isso o Mac pediria a
  senha e «Permitir/Negar» depois de cada actualização. Sem o certificado, no macOS a chave fica no ficheiro, como
  antes; uma chave que já esteja no Porta-chaves continua a ler-se dele. O ficheiro `kangest.chave`, na pasta de dados, guarda só o
  identificador (`cofre1:…`), não a chave. Uma cópia da base, ou da pasta de dados inteira, sozinha, não revela os
  segredos. Os valores antigos, em texto, são cifrados no arranque.
- Instalações anteriores, com a chave no `kangest.chave`, passam-na sozinhas para o cofre no primeiro arranque; a chave
  é a mesma, por isso os segredos já cifrados continuam a abrir. O ficheiro só troca a chave pelo identificador depois
  de a ler de volta do cofre. Se o cofre não estiver disponível, a chave continua no ficheiro, como antes; se o cofre
  recusar o acesso, o programa abre e pede de novo os segredos, sem criar outra chave por cima.
- Noutros sistemas (Linux, testes) a chave continua no ficheiro.
- Restaurar a base noutro computador obriga a introduzir de novo esses segredos (o programa avisa) e a renovar o
  segredo de cada aviso automático em **Sistema → API e Integrações**. Os documentos já assinados continuam
  verificáveis: a chave pública de cada chave de assinatura fica registada.

## 5. Base de dados cifrada

A base de dados (`kigest.db`) vai cifrada com o **SQLCipher** (AES-256, cada página com o seu código de autenticação):
quem abre o ficheiro com um editor de SQLite, ou o copia, não vê documentos, clientes nem artigos. A chave da base
deriva da chave mestra (SHA-256 com um rótulo próprio, `segredos::chave_base`), por isso fica onde a chave mestra fica:
no cofre do sistema no Windows, e no ficheiro `kangest.chave` no macOS sem certificado da Apple.

- **Bases antigas, em claro:** cifram-se sozinhas no primeiro arranque (`sqlcipher_export` para um ficheiro ao lado,
  conferido tabela a tabela, que depois substitui o original; um corte a meio deixa o original intacto).
- **Cópias locais** (`copias/`, feitas com `VACUUM INTO`): saem cifradas com a mesma chave; restauram-se neste
  computador.
- **Cópia exportada e cópias externas (pen, OneDrive):** saem **portáteis**, em claro, e as externas cifradas com a
  senha das cópias se estiver definida. São estas que salvam a loja quando o computador avaria, por isso têm de abrir
  noutro computador, onde a chave deste não existe. Defina a senha das cópias externas.
- **Mudar de computador:** use uma cópia externa ou exportada e **Restaurar**. Copiar a pasta de dados não chega no
  Windows: a chave ficou no cofre do utilizador do computador antigo. Uma base que não abre com a chave deste computador
  fica de parte (`kigest.db.ilegivel-…`, nunca apagada) e o KanGest arranca com uma base nova, onde se restaura a cópia.
- **Suporte:** para analisar uma base, use a cópia exportada (em claro); `kigest.db` só abre com a chave.

## 6. Limites (o que estas protecções não fazem)

- **Quem altera o executável também pode retirar as verificações.** O selo, o selo da licença e a detecção de
  depurador obrigam a perceber e alterar várias partes do programa compilado (Rust optimizado, sem símbolos), mas não
  o tornam impossível. O que realmente não se falsifica são as licenças (só o produtor tem a chave privada) e a cadeia
  de assinaturas dos documentos.
- **As cópias portáteis vão em claro sem a senha das cópias.** A cópia exportada e as externas sem senha lêem-se com
  um editor de SQLite: guarde-as com cuidado, ou defina a senha das cópias externas.
- **No macOS sem certificado da Apple,** a chave fica no `kangest.chave`, ao lado da base: quem copia a pasta de dados
  inteira leva as duas. No Windows a chave está no cofre do utilizador.
- **Os segredos cifrados não resistem a quem entra na conta do utilizador no computador**: o cofre do sistema entrega
  a chave a programas que correm nessa conta. Protege contra quem copia a pasta de dados ou o disco, ou entra com
  outra conta do sistema. Contra quem tem a própria conta só uma senha que não está no computador — é o caso das
  cópias de segurança cifradas com senha.
- **A chave de assinatura dos documentos (AGT)** é gerada na base até haver o certificado do produtor. Com o
  certificado, a chave do produtor passa a ir no programa e merece o mesmo cuidado que a chave do selo.
- **Uma base nova recomeça a experiência** se não estiver registada no servidor de licenças: o registo online prende a
  experiência ao NIF (uma vez por NIF), e é essa a defesa.

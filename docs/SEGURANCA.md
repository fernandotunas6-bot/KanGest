# Segurança: cifra e protecção do programa

O que está cifrado ou selado, como se activa e, sobretudo, até onde vai. Nenhuma protecção que corre no computador do
cliente é inquebrável: quem controla o computador pode examinar e alterar o programa. O objectivo é que alterar o
KanGest ou os seus dados dê muito trabalho, deixe rasto e não funcione às cegas.

| O quê | Protecção | Onde |
|---|---|---|
| Ligação entre os postos e o servidor (rede local) | AES-256-GCM, chave derivada da chave de rede (Argon2id), contra repetições | [REDE.md](REDE.md#cifra-da-ligação) |
| Segredos na base (chave de assinatura AGT, senha e chaves da Facturação Electrónica, segredos dos avisos) | AES-256-GCM (cofre) | abaixo, §4 |
| Cópias de segurança para levar (pen, nuvem, email) | AES-256-GCM com chave da senha (Argon2id), por blocos autenticados | abaixo, §5 |
| Datas da licença na base (início da experiência, última data vista) | Selo HMAC-SHA256 com chave fora da base | abaixo, §1 |
| Executável do KanGest | Selo de integridade Ed25519 posto na compilação oficial | abaixo, §2 |
| Depuradores (engenharia reversa com o programa a correr) | A emissão pára com um depurador ligado | abaixo, §3 |
| Licenças | Assinatura Ed25519 do produtor | [LICENCAS.md](LICENCAS.md) |
| API de integração pela Internet | Chaves de API (guardadas só em SHA-256), Cloudflare Tunnel e Access (JWT RS256) | [API.md](API.md), [CLOUDFLARE.md](CLOUDFLARE.md) |
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

## 4. Segredos guardados na base (cofre)

A chave privada que assina os documentos (AGT), a senha e as duas chaves privadas da Facturação Electrónica e os
segredos dos avisos automáticos estavam em texto na base. Agora vão cifrados com AES-256-GCM (`src-tauri/src/cofre.rs`):
quem abre a base com um editor de SQLite, ou lê uma cópia de segurança, vê `KGC1.…` em vez da chave ou da senha.

- A chave da cifra junta uma chave do programa (que não está inteira no executável) e o sal da própria base
  (`cofre.sal`). Por isso a base continua a abrir noutro computador com o KanGest (cópia de segurança restaurada,
  computador novo) e não precisa de nenhuma senha no dia-a-dia.
- Cada valor só abre no campo onde foi guardado: copiar a chave AGT para o campo da senha não a revela.
- As bases de versões anteriores são cifradas no primeiro arranque desta versão. **Não volte depois a uma versão
  anterior**: ela não leria os segredos cifrados (a chave AGT e as da Facturação Electrónica).

## 5. Cópias de segurança cifradas com senha

Em **Configurações → Sistema → Cópia cifrada…**, o KanGest grava na pasta Transferências uma cópia
`kangest-copia-AAAAMMDD-HHMMSS.kgc` cifrada com a senha escolhida (pelo menos 8 caracteres; uma frase serve melhor).
É a cópia certa para levar numa pen, guardar na nuvem ou mandar por email: **sem a senha ninguém a abre — nem o
KanGest nem o suporte**. Não há forma de recuperar uma senha esquecida.

- A chave sai da senha por **Argon2id** (64 MiB de memória, 3 passagens), o que torna lento adivinhar senhas às cegas.
- O conteúdo vai em **AES-256-GCM em blocos de 1 MiB**, cada um com um número único que inclui a sua posição e se é o
  último. Uma cópia cortada, com blocos trocados, com um byte mexido ou com o cabeçalho alterado não abre.
- A cópia fica completa e consistente mesmo com o programa em uso (`VACUUM INTO`); o ficheiro temporário em claro é
  apagado logo a seguir.

**Restaurar** (também em Configurações → Sistema, só para quem gere as configurações): o KanGest procura as cópias
cifradas (`.kgc`), as manuais (`.db`) e as automáticas neste computador e nas pens. Escolhida a cópia (e a senha, se for
cifrada), o KanGest abre-a, confirma que é uma base do KanGest inteira (verificação de integridade do SQLite) e de uma
versão que sabe abrir, e mostra a empresa e o número de documentos. A troca faz-se ao **fechar e voltar a abrir o
programa**, com a base ainda fechada; a base que estava em uso fica guardada em `copias/kangest-antes-do-restauro-….db`
(com tudo o que estivesse no registo WAL). Até reabrir, pode desistir. Em rede, o restauro faz-se no servidor.

As cópias automáticas diárias (`copias/`, 14 dias) continuam sem senha: ficam no mesmo computador que a base.

## 6. Limites (o que estas protecções não fazem)

- **Quem altera o executável também pode retirar as verificações.** O selo, o selo da licença e a detecção de
  depurador obrigam a perceber e alterar várias partes do programa compilado (Rust optimizado, sem símbolos), mas não
  o tornam impossível. O que realmente não se falsifica são as licenças (só o produtor tem a chave privada) e a cadeia
  de assinaturas dos documentos.
- **O resto da base não está cifrado.** Os segredos vão no cofre, mas os documentos, os clientes e os artigos lêem-se
  com um editor de SQLite por quem tiver acesso ao computador. As cópias de segurança devem ser guardadas com o mesmo
  cuidado. Cifrar a base inteira (SQLCipher) obriga a uma senha ou a uma chave guardada no sistema (DPAPI no Windows,
  Porta-chaves no macOS) e a um cuidado especial ao restaurar noutro computador: fica como passo seguinte.
- **O cofre não resiste a quem estuda o programa**: a chave do programa está no executável, embora partida. Contra
  isso só uma senha que não está no computador — é o caso das cópias de segurança cifradas com senha (§5).
- **A chave de assinatura dos documentos (AGT)** é gerada na base até haver o certificado do produtor. Com o
  certificado, a chave do produtor passa a ir no programa e merece o mesmo cuidado que a chave do selo.
- **Uma base nova recomeça a experiência** se não estiver registada no servidor de licenças: o registo online prende a
  experiência ao NIF (uma vez por NIF), e é essa a defesa.

# Segurança: cifra e protecção do programa

O que está cifrado ou selado, como se activa e, sobretudo, até onde vai. Nenhuma protecção que corre no computador do
cliente é inquebrável: quem controla o computador pode examinar e alterar o programa. O objectivo é que alterar o
KanGest ou os seus dados dê muito trabalho, deixe rasto e não funcione às cegas.

| O quê | Protecção | Onde |
|---|---|---|
| Ligação entre os postos e o servidor (rede local) | AES-256-GCM, chave derivada da chave de rede (Argon2id), contra repetições | [REDE.md](REDE.md#cifra-da-ligação) |
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

## 4. Limites (o que estas protecções não fazem)

- **Quem altera o executável também pode retirar as verificações.** O selo, o selo da licença e a detecção de
  depurador obrigam a perceber e alterar várias partes do programa compilado (Rust optimizado, sem símbolos), mas não
  o tornam impossível. O que realmente não se falsifica são as licenças (só o produtor tem a chave privada) e a cadeia
  de assinaturas dos documentos.
- **A base de dados não está cifrada.** Quem tem acesso ao computador lê a base (`kigest.db`). As cópias de segurança
  devem ser guardadas com o mesmo cuidado.
- **A chave de assinatura dos documentos (AGT) está na base**, gerada na primeira emissão, até haver o certificado do
  produtor. Com o certificado, a chave do produtor passa a ir no programa e merece o mesmo cuidado que a chave do selo.
- **Uma base nova recomeça a experiência** se não estiver registada no servidor de licenças: o registo online prende a
  experiência ao NIF (uma vez por NIF), e é essa a defesa.

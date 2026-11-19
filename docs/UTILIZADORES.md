# Utilizadores, permissões e autenticação

O módulo está em **Sistema → Utilizadores e Permissões**. Tem cinco separadores:

| Separador | Para quê | Quem vê |
|---|---|---|
| Utilizadores | Criar e editar contas, perfil, PIN, limite de desconto, permissões próprias, redefinir senha, desbloquear, repor a verificação em dois passos | permissão *Utilizadores e permissões* |
| Perfis e permissões | Perfis de acesso (Administrador, Gestor, Caixa e os criados pela empresa) e a matriz de permissões de cada um | idem |
| Registo de acessos | Entradas, saídas, tentativas falhadas, bloqueios, mudanças de senha, por posto | permissão *Registo de acessos e auditoria* |
| Auditoria | Operações registadas (emissões, anulações, importações, alterações de configuração…) | idem |
| Segurança | Política de senhas, bloqueio por tentativas, bloqueio por inactividade, entrada por PIN, dois passos pela Internet | *Utilizadores e permissões* |

O menu **Sistema → Registo de Acessos e Auditoria** abre directamente o registo de acessos.

## Perfis de acesso

Cada utilizador tem um perfil. As permissões do perfil aplicam-se a todos os seus utilizadores, excepto aos que tenham
**permissões próprias** (caixa «Permissões próprias deste utilizador» na ficha).

| Perfil | Escalão | Permissões de origem |
|---|---|---|
| Administrador | 3 | Todas, sempre (incluindo as de módulos novos). Não se pode editar. |
| Gestor | 2 | Todas excepto as de administração: utilizadores, configurações, actualizações, facturação electrónica, importação, parametrização, certificação, auditoria, configuração de RH e API e integrações. |
| Caixa (POS) | 1 | Facturas, facturas-recibo, proformas, clientes, recibos; no POS: quantidade, apagar linha, suspender, fecho e relatório de caixa. |

- Os perfis de origem podem ter as permissões alteradas; o botão **Repor as permissões de origem** volta à lista inicial.
- É possível criar perfis novos (ex.: *Armazém*, *Contabilista*, *Supervisor de caixa*) e escolher o **ecrã inicial**:
  o ponto de venda (como o Caixa) ou o painel de gestão (como o Gestor). O que cada um pode fazer vem só das permissões.
- Um perfil só se elimina se não tiver utilizadores.
- Tem de existir sempre pelo menos um Administrador activo.

## Senhas

- Comprimento mínimo configurável (por omissão 6). Senhas com um só carácter repetido são recusadas.
- **Senha provisória:** os utilizadores criados pelo administrador (com a opção «Obrigar a mudar a senha ao entrar»),
  os importados do KIGEST e os que tiveram a senha redefinida têm de escolher uma senha nova no primeiro acesso.
  Até lá não conseguem fazer mais nada.
- **Redefinir senha** gera uma senha provisória aleatória, mostrada uma única vez ao administrador.
- Cada utilizador pode mudar a sua senha no ícone da chave, por baixo do menu.

## Bloqueios

- **Conta:** ao fim de N tentativas falhadas seguidas (por omissão 5) a conta fica bloqueada durante M minutos
  (por omissão 15), ou até o administrador a desbloquear na lista de utilizadores. A mensagem de erro indica quantas
  tentativas restam. Com N = 0 nunca bloqueia.
- **Sessão:** o botão do cadeado (no menu lateral e na barra do POS) tapa o ecrã sem perder nada: a venda em curso,
  formulários e filtros ficam como estavam. Retoma-se com a senha ou o PIN do mesmo utilizador, ou
  **Trocar de utilizador**. Com «Bloquear a sessão sem actividade» > 0 a sessão bloqueia-se sozinha.
  Enquanto está bloqueada, o servidor recusa qualquer operação dessa sessão (também nos postos em rede).

## PIN

- 4 a 8 dígitos, único por pessoa (identifica quem autoriza).
- Serve para autorizações de supervisor no POS (desconto acima do limite, alterar preço, apagar linha…) e para retomar
  a sessão bloqueada.
- Com **Permitir entrar só com o PIN** activo, o ecrã de entrada mostra um teclado numérico — útil nos terminais
  tácteis para trocar de operador depressa. Após 5 PIN errados no mesmo posto, a entrada por PIN fica suspensa 5 minutos.

## Verificação em dois passos

Cada pessoa pode ligá-la na sua conta (menu do utilizador → **Verificação em dois passos**). Depois da senha, o
KanGest pede o código de 6 algarismos que uma aplicação de autenticação do telemóvel (Google Authenticator,
Microsoft Authenticator, Authy…) mostra e muda de 30 em 30 segundos (TOTP, RFC 6238). Quem souber a senha não
entra sem o telemóvel.

- Para ligar: ler o código QR (ou escrever a chave), escrever o código e a senha. Aparecem **8 códigos de
  recuperação**, cada um serve uma vez para entrar sem o telemóvel; não voltam a ser mostrados.
- Para desligar: senha e um código (da aplicação ou de recuperação).
- Quem perdeu o telemóvel e os códigos: o administrador usa **Repor dois passos** na lista de utilizadores; a pessoa
  volta a entrar só com a senha e liga-a de novo.
- Um código já usado não serve outra vez; aceita-se o código anterior e o seguinte (relógio do telemóvel um pouco
  desacertado). Códigos errados contam como tentativas falhadas para o bloqueio da conta.
- A chave guarda-se cifrada na base (como os outros segredos, [SEGURANCA.md](SEGURANCA.md) §4) e os códigos de
  recuperação só em SHA-256.
- Em **Segurança**, **Exigir a verificação em dois passos a quem entra pela Internet** recusa, pelo subdomínio
  (túnel da Cloudflare), as contas que não a tenham ligada. Na rede da loja nada muda. Pela Internet o PIN nunca
  serve para entrar.

## Esqueci-me da senha

No ecrã de entrada, **Esqueci-me da senha** abre a recuperação em três passos: o utilizador, o código da aplicação de
autenticação (ou um código de recuperação, que fica gasto) e a senha nova, com a política de senhas.

- Só serve a quem tem a **verificação em dois passos** ligada; os outros pedem ao administrador uma senha provisória
  (**Redefinir senha**). A mensagem de erro é a mesma em todos os casos — não diz se o utilizador existe nem se tem
  os dois passos.
- Um código errado conta como tentativa falhada para o bloqueio da conta; com a conta bloqueada não se recupera.
- O código conferido dá **10 minutos** para escolher a senha, uma só vez. A senha nova deixa a conta desbloqueada e
  sem senha provisória; os dois passos continuam ligados.
- Só nos postos da loja (rede local): **pela Internet a senha não se recupera**.
- Fica no registo: `Recuperação da senha` (código conferido) e `Senha alterada` («recuperada com a verificação em dois
  passos»); um código de recuperação usado fica também na auditoria.

## Ecrã de entrada

À esquerda, a imagem do ecrã de entrada com o logótipo e três cartões de novidades; à direita, «Bem-vindo ao
KanGest» com o utilizador e a senha (o olho mostra o que se
escreveu; os campos vazios dizem «Este campo é obrigatório»), **Ocultar valores** e **Esqueci-me da senha**. Em baixo,
as **Perguntas frequentes** (entrar, aparelhos e rede, segurança), **Contacte-nos** e o site.

**Ocultar valores** fica guardado no aparelho: no painel, os valores em kwanzas aparecem como `••••••` (e o eixo do
gráfico sem montantes), para quem está do outro lado do balcão não os ver. No painel, **Mostrar valores** volta a
mostrá-los.

**Configurações → Aparência** (permissão de configurar o sistema), para todos os postos e browsers da empresa:

- **Cores do sistema:** sete jogos prontos (o verde-petróleo do KanGest, azul-marinho, azul petróleo e ouro, vinho e
  dourado, verde-floresta, grafite e laranja, violeta) ou um personalizado com duas cores (a principal — menu, botões,
  ligações — e a de destaque — Abrir POS, Pagar, avisos). Os tons claros, escuros e os do tema escuro calculam-se
  sozinhos, e uma cor demasiado clara escurece o necessário para o texto se ler. Escolhe-se também na instalação. O
  tema claro ou escuro continua a ser de cada pessoa.
- **Imagem do ecrã de entrada:** é a do KanGest e **não se troca** — fotografia n.º 9363112 do Pexels (licença
  Pexels, uso comercial gratuito), editada: fato cinzento, gravata laranja e colunas claras
  (`scripts/compor-imagem-entrada.py`).

## O meu perfil

No menu do utilizador (canto superior direito), **O meu perfil**: cada pessoa muda a sua **fotografia** (do
computador ou, no telemóvel, com a câmara; corta-se ao quadrado e reduz-se a 256 px), o **nome**, o **email** e o
**telefone**. A fotografia aparece no topo, no POS, no ecrã de sessão bloqueada e na lista de utilizadores do
administrador. O utilizador (login), o perfil, a loja e as permissões continuam a ser do administrador. A mudança fica
no registo de acessos (`Perfil alterado`, com o nome antigo e o novo).

## Registo

Ficam no **Registo de acessos**: `Entrada`, `Saída`, `Tentativa falhada` (com o utilizador escrito, mesmo que não exista),
`Conta bloqueada`, `Conta desbloqueada`, `Senha alterada`, `Senha redefinida`, `Sessão bloqueada`, `Sessão retomada`,
com data, hora e posto. A **Auditoria** guarda as operações (quem criou ou alterou utilizadores e perfis, mudou a
política de segurança, emitiu ou anulou documentos, importou dados…).

### Auditoria: o antes e o depois

Cada vez que alguém cria, altera, elimina ou anula uma ficha — artigos, clientes e fornecedores, utilizadores, perfis,
famílias, armazéns, taxas, moedas, séries, contas, centros de custo, tipos de documento, modos de pagamento, postos,
promoções e cupões, funcionários, viaturas e ordens da oficina, chaves da API, documentos anulados, a ficha da empresa
— fica uma linha com **o que estava e o que ficou**: numa criação a ficha nova, numa alteração só os campos que
mudaram, numa eliminação a ficha como estava. As configurações (segurança, opções de venda, email, facturação
electrónica, cores…) ficam chave a chave. Uma gravação sem mudanças não fica. Cada linha diz também quem, quando, em
que posto e por que via (este computador, rede da loja, pela Internet ou API de integração).

Senhas, PIN, segredos dos dois passos, chaves privadas e senhas de email **nunca** vão para o registo: aparecem como
«•••» (e «••• (novo)» quando mudaram). As fotografias aparecem só como «imagem».

Em **Utilizadores e Permissões → Auditoria** filtra-se por período, utilizador, acção, ficha, via e texto (procura no
detalhe e nos valores); clicar numa linha mostra o antes e o depois, clicar na ficha mostra todo o histórico dela.
Exporta para Excel e imprime. Nas fichas de artigos, clientes e fornecedores e utilizadores há o separador
**Histórico** com a mesma informação (para quem tem a permissão *Registo de acessos e auditoria*).

### Registo à prova de adulteração

O registo **só cresce**: o KanGest não deixa apagar nem alterar linhas da auditoria (nem por engano, nem por um erro do
programa). Cada linha leva um **selo**: o SHA-256 do selo da linha anterior com o conteúdo desta — uma cadeia, como a
dos documentos certificados. Se alguém mudar ou apagar uma linha directamente na base de dados, fora do KanGest, a
cadeia parte-se nesse ponto; se apagar as últimas linhas, o KanGest dá pela falta ao abrir ou na operação seguinte e
deixa-o escrito no próprio registo (`Registo mexido por fora`).

O topo do separador **Auditoria** diz se o registo está íntegro (quantas linhas, de quando a quando) e, se não, qual
a linha e o que se passou; o botão **Verificar integridade** volta a conferir tudo.

## Para quem programa

- Catálogo de permissões: `PERMISSOES` em `src-tauri/src/state.rs` e `GRUPOS_PERMISSOES` em `src/lib/permissoes.ts`
  (têm de ter as mesmas chaves). Permissões só do administrador: `SO_ADMIN`.
- Cada operação do servidor verifica `st.exigir_perm("…")`. As permissões efectivas vêm de
  `permissoes_efectivas(perfil, próprias)`.
- Métodos sem sessão completa: `setup.*`, `auth.login`, `auth.loginPin`, `auth.logout`, `auth.sessao`,
  `auth.alterarSenha`, `auth.desbloquear`, `seguranca.politica`. Todos os outros são recusados com a sessão bloqueada
  ou com senha provisória.
- Tabelas: `perfis`, `acessos`; colunas de segurança em `utilizadores` (migração `0016_utilizadores_permissoes.sql`).
- Auditoria: `services/auditoria.rs` (o mapa método → ficha em `alvo`, o antes e o depois, a listagem, o histórico) e
  `src/auditoria.rs` (o selo em cadeia, `selar`, `verificar`). O `dispatch` lê a ficha antes, corre o método e regista
  o que mudou; no fim de cada pedido sela as linhas novas. Um método novo que grava uma ficha entra em `alvo`; um
  campo secreto novo tem de ser apanhado por `secreto`. Migração `0037_auditoria_seria.sql` (colunas `entidade`,
  `registo_id`, `antes`, `depois`, `posto_id`, `origem`, `selo` e os gatilhos que não deixam apagar nem alterar).

## Utilizadores de uma só loja (servidor central)

Num servidor central com várias livrarias (cada uma é um armazém, com os seus postos), cada utilizador pode ficar
ligado a **uma loja**: Sistema → Utilizadores → a ficha → **Loja**. Com uma loja:

- só entra nos **postos dessa loja** (num posto de outra loja, o login diz de que loja é o utilizador);
- vê só os **documentos** dessa loja (a lista e a abertura de um documento);
- os **relatórios** (vendas, síntese, stock) e o **painel** inicial são os dessa loja, peça o filtro que pedir.

«Todas as lojas» (o normal) é como antes. Os administradores vêem sempre todas. As chaves da API agem pelo posto
delas, não pela loja do utilizador.

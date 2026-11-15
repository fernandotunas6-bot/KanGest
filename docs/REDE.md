# Vários postos ligados ao mesmo servidor

No KIGEST original todos os postos apontavam para o mesmo SQL Server. Aqui o papel do servidor
é feito por uma das máquinas do KanGest: ela guarda a base de dados e serve as outras pela rede local.

## Como ligar

1. **Na máquina que guarda os dados** (a do escritório ou a caixa principal):
   Configurações → Rede → **Servidor**. Escolha a porta (7799 por omissão) e uma **chave de rede**
   (mínimo 6 caracteres; use pelo menos 12, sem palavras do dicionário — é dela que sai a chave da cifra). Guarde. O ecrã mostra o endereço IP a indicar aos outros postos.
   Se a firewall perguntar, autorize o KanGest a aceitar ligações na rede local.
2. **Em cada um dos outros postos**: Configurações → Rede → **Posto em rede**, com o endereço do
   servidor, a mesma porta e a **mesma chave**. Guarde — a configuração só é aceite se o servidor
   responder. Depois use *Testar ligação* sempre que quiser confirmar.
3. Em Configurações → Postos e impressão, dê a cada posto a sua descrição, armazém e prefixo de
   série, e em cada máquina use *Usar nesta máquina*.

## O que fica em cada lado

| Fica no servidor | Fica em cada posto |
|---|---|
| Base de dados (artigos, stock, documentos, caixa, conta corrente, RH) | Impressora térmica e gaveta |
| Numeração e cadeia de assinaturas AGT | Ficheiros exportados (CSV) gravados na própria máquina |
| Cópias de segurança (passam a guardar o trabalho de todos) | A escolha de qual é o posto (`posto.json`) |
| SAF-T e relatórios | |

Cada posto tem a **sua sessão**: quem entra num posto não entra nos outros, e cada documento sai
na série do posto que o emitiu (ex.: `FR P22026/15`). A numeração nunca salta porque a emissão
continua a ser serializada num só lugar — o servidor.

## Limites e cuidados

- A ligação é **cifrada** (ver abaixo). Mesmo assim, **não exponha a porta à Internet** nem a encaminhe no router.
- **Sem rede não há venda** no posto cliente: os dados estão no servidor. Quem precisa de vender
  sempre deve ficar em modo *Sozinho*.
- O servidor só abre (ou muda de porta) ao arrancar a aplicação: depois de mudar a porta, feche e
  reabra o KanGest no servidor.
- Uma sessão de posto é esquecida depois de 12 horas sem pedidos.
- A base de dados **não** deve ser partilhada por uma pasta de rede: é para isso que existe este modo.
- Para ligar **outros sistemas** (loja online, contabilidade, BI) não se usa a chave de rede: a API de integração tem
  porta própria (7800) e chaves de acesso por sistema, com permissões — ver [API.md](API.md).
- Mudar a chave de rede no servidor vale logo; os postos deixam de entrar até terem a chave nova.

## Cifra da ligação

Tudo o que passa entre os postos e o servidor vai cifrado. Até à versão 1.1.0 ia em texto: quem estivesse na mesma
rede (Wi-Fi da loja, por exemplo) podia ler a chave de rede, as senhas de entrada e os documentos.

- **AES-256-GCM** em cada pedido e em cada resposta. A chave da cifra sai da chave de rede por **Argon2id**
  (19 MiB, 2 passagens) com um **sal** aleatório que o servidor cria na primeira vez e guarda no `rede.json`.
- **A chave de rede nunca viaja.** O posto pede ao servidor o sal (público) e deriva a mesma chave; um pedido feito
  com outra chave é recusado («Chave de rede inválida»).
- **Nada se lê nem se altera pelo caminho**: método, dados, senhas, token da sessão e a resposta vão cifrados e
  autenticados; um byte mexido e o pedido é recusado.
- **Um pedido copiado da rede não se repete** (por exemplo, para emitir a mesma factura outra vez): cada pedido leva
  a hora e um número único, que o servidor recusa se já o viu ou se tiver mais de 5 minutos.
- **A resposta só serve ao pedido a que responde** (vai ligada ao número único do pedido) e o posto recusa respostas
  sem cifra que digam «ok».
- **Relógios desacertados não param a venda**: o posto acerta-se pela hora do servidor e repete o pedido.

Postos e servidor têm de estar na mesma versão: um posto antigo recebe «Este posto tem uma versão antiga do KanGest
(ligação sem cifra). Actualize-o.»; um posto novo ligado a um servidor antigo recebe «O servidor tem uma versão antiga
do KanGest: actualize-o primeiro». Actualize primeiro o servidor e depois os postos.

O que a cifra não resolve: quem souber a chave de rede (ou tiver acesso ao computador do servidor e ao `rede.json`)
consegue ligar-se como um posto. Escolha uma chave longa e mude-a quando sair alguém que a conhecia.

# Vários postos ligados ao mesmo servidor

No KIGEST original todos os postos apontavam para o mesmo SQL Server. Aqui o papel do servidor
é feito por uma das máquinas do KanGest: ela guarda a base de dados e serve as outras pela rede local.

## Como ligar

1. **Na máquina que guarda os dados** (a do escritório ou a caixa principal):
   Configurações → Rede → **Servidor**. Escolha a porta (7799 por omissão) e uma **chave de rede**
   (mínimo 6 caracteres). Guarde. O ecrã mostra o endereço IP a indicar aos outros postos.
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

- A ligação é **HTTP em texto na rede local**, protegida pela chave de rede — como o SQL Server do
  KIGEST, que também falava em claro na LAN. **Não exponha a porta à Internet** nem a encaminhe no router.
- **Sem rede não há venda** no posto cliente: os dados estão no servidor. Quem precisa de vender
  sempre deve ficar em modo *Sozinho*.
- O servidor só abre (ou muda de porta) ao arrancar a aplicação: depois de mudar a porta, feche e
  reabra o KanGest no servidor.
- Uma sessão de posto é esquecida depois de 12 horas sem pedidos.
- A base de dados **não** deve ser partilhada por uma pasta de rede: é para isso que existe este modo.

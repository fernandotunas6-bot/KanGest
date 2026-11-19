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

## IPv4 e IPv6

O servidor ouve nos **dois ao mesmo tempo**, na mesma porta (dupla pilha): os postos, os telemóveis no browser e
as integrações ligam-se pelo endereço que tiverem. O ecrã do servidor mostra o IPv4 e, havendo, o IPv6 da
máquina (o código QR usa o primeiro).

- No posto, o endereço do servidor pode ser IPv4 (`192.168.1.10`), IPv6 (`2001:db8::10`, ou `[2001:db8::10]:7799`
  com a porta) ou um nome (`servidor.loja.ao`). Um nome com os dois tipos de endereço tenta os dois.
- No browser, um IPv6 escreve-se entre parênteses rectos: `http://[2001:db8::10]:7799`.
- A API (Sistema → API e Integrações) e as impressoras de rede (`fd00::50` ou `[fd00::50]:9100`) aceitam o mesmo.
  Com a API «só desta máquina», abre em `127.0.0.1` e em `::1`, para o túnel chegar por qualquer um.
- Os registos, os bloqueios por chave errada e o reconhecimento do túnel na própria máquina tratam um IPv4 que chega
  pela porta IPv6 (`::ffff:192.168.1.5`) como o IPv4 que é.
- Sem IPv6 no computador (desligado no sistema), o servidor abre só em IPv4, como antes.

Na firewall, autorize o KanGest nos dois (no Windows, a regra da aplicação cobre IPv4 e IPv6). Com IPv6 público,
os computadores ficam alcançáveis da Internet sem NAT: não publique a porta 7799 sem o túnel e o Access
([CLOUDFLARE.md](CLOUDFLARE.md)).

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

## Segurança da ligação (KanGest 1.2)

- O corpo de cada pedido e de cada resposta vai **cifrado e autenticado** (AES-256-GCM). A chave vem da chave de rede
  (PBKDF2-HMAC-SHA256, 100 000 iterações, com um sal guardado no servidor, em `rede.json`). A **chave de rede nunca
  passa na rede**: quem a não tiver não consegue ler nem forjar pedidos.
- Cada pedido leva a hora do posto e um identificador único: um pedido capturado não se pode repetir, e um posto com o
  relógio desacertado mais de 15 minutos é avisado para acertar a data e a hora.
- Um endereço que erre a chave 10 vezes fica bloqueado 10 minutos.
- Chaves novas têm pelo menos 10 caracteres; no servidor, **Gerar chave** cria uma de 20 caracteres. Uma chave antiga
  mais curta continua a funcionar, com um aviso para a trocar.
- Só quem tem a permissão *Configurações* vê a chave de rede no ecrã.
- Os postos e o servidor têm de estar **todos na versão 1.2** (ou superior): um posto antigo recebe a mensagem para se
  actualizar, e um servidor antigo é detectado pelo posto novo.

## Limites e cuidados

- Mesmo cifrada, a porta é para a rede local: **não a exponha à Internet** nem a encaminhe no router. Para trabalhar
  de fora da loja use o túnel da Cloudflare **com o Access à frente** ([WEB-PWA.md](WEB-PWA.md)).
- A mesma porta serve a aplicação ao **browser** (telemóveis, tablets, terminais Android, iPad): ver
  [WEB-PWA.md](WEB-PWA.md). Desliga-se em Configurações → Rede → «Permitir postos pelo browser».
- **Sem rede não há venda** no posto cliente: os dados estão no servidor. Quem precisa de vender
  sempre deve ficar em modo *Sozinho*.
- O servidor só abre (ou muda de porta) ao arrancar a aplicação: depois de mudar a porta, feche e
  reabra o KanGest no servidor.
- Uma sessão de posto é esquecida depois de 12 horas sem pedidos.
- A base de dados **não** deve ser partilhada por uma pasta de rede: é para isso que existe este modo.
- Para ligar **outros sistemas** (loja online, contabilidade, BI) não se usa a chave de rede: a API de integração tem
  porta própria (7800) e chaves de acesso por sistema, com permissões — ver [API.md](API.md).

# Equipamento do posto de venda

O KanGest trabalha com o equipamento habitual de um posto de facturação: terminal táctil, leitor de código de barras, impressora térmica, gaveta do dinheiro e visor de cliente. Tudo se configura por posto em **Configurações → Postos e impressão → Configurar**, na máquina onde o equipamento está ligado.

## Terminal táctil

- O POS está desenhado para ecrãs a partir de 1024 × 768 (15″) e funciona também em 1280 × 1024 e 1920 × 1080. Os botões têm pelo menos 44 px, o tamanho de um dedo.
- **Telemóveis, tablets e terminais Android/iOS** (no browser, [WEB-PWA.md](WEB-PWA.md)): o POS adapta-se ao ecrã. Num
  telemóvel, os artigos ocupam o ecrã e a venda fica numa barra em baixo («Venda 1/2 · 3 itens · total» e **Pagar**);
  toca-se na barra para ver e alterar as linhas. Num tablet ou terminal deitado (ex.: Sunmi 1366 × 768) fica como no
  computador. Em ecrãs estreitos o menu da gestão abre como gaveta (☰).
- **Modo táctil** (menu do utilizador → *Ecrã táctil*: Automático, Ligado, Desligado): em ecrãs tácteis, botões,
  campos e linhas das tabelas ficam com altura para o dedo; o campo de pesquisa do POS fica pronto para o leitor
  de códigos **sem abrir o teclado do sistema** — o teclado só aparece ao tocar no campo.
- **Vendas em paralelo**: **+ Nova** (Alt+N) abre outra venda e deixa a actual à espera; **‹ ›** (Alt+← / Alt+→)
  passa de uma para a outra — «Venda 2/3». Quando uma venda é emitida, suspensa ou cancelada, as outras continuam
  abertas. Ficam guardadas no aparelho: recarregar a página ou fechar o programa por engano não as perde (a
  autorização do supervisor não fica guardada).
- **Teclado no ecrã:** o botão do teclado no campo de pesquisa abre um teclado para procurar artigos sem teclado físico. «Adicionar» lança o artigo encontrado.
- **Ecrã inteiro:** o botão no topo do POS alterna o ecrã inteiro. Para os terminais dedicados, active «Abrir o POS em ecrã inteiro» em **Venda**.
- Os operadores de caixa entram directamente no POS.

## Leitor de código de barras

Serve qualquer leitor USB ou sem fios (Bluetooth, 2,4 GHz) que funcione como teclado, que é o modo de origem da
maioria. O KanGest reconhece a leitura pela velocidade: o leitor escreve muito mais depressa do que uma pessoa.

**Onde lê**

- **POS:**
  - a leitura entra na venda mesmo que o operador tenha tocado numa linha, num botão ou noutro campo antes de ler;
  - `3*código` vende 3 unidades.
- **Documentos** (compras, entradas e saídas de stock, facturas…): junta a linha, mesmo com o cursor numa quantidade
  ou nas observações (o que o leitor lá escreveu é tirado).
- **Inventário:** cada leitura soma 1 à contagem do artigo (ou as unidades da embalagem lida) e marca a linha.
- **Etiquetas:** junta o artigo.
- **Lista de artigos:** abre o artigo lido; se nenhum tiver o código, abre uma ficha nova já com ele.
- **Ficha do artigo:** avisa quando o último algarismo de um EAN/UPC não bate com os outros (código mal escrito ou
  mal lido).

**Que códigos reconhece**

- O código de barras, a referência e os outros códigos do artigo (embalagens: uma caixa de 12 vende 12).
- As etiquetas da balança (peso ou preço).
- UPC-A de 12 algarismos e EAN-13 com o zero à frente (um encontra o outro), e GTIN-14 das caixas.
- ISBN com hífenes ou de 10 algarismos (os livros guardam-se em ISBN-13).
- **GS1** (o DataMatrix dos medicamentos e o GS1-128 das caixas):
  - procura pelo GTIN;
  - no POS, mostra o lote e a validade e **não junta à venda um artigo com a validade passada**;
  - nas entradas de stock, preenche o lote e a validade da linha.
- O prefixo que alguns leitores acrescentam («]E0», «]C1», «]d2»…) é ignorado.

**Código desconhecido no POS**

O POS mostra o código e o tipo e oferece duas saídas:

- **Juntar ao artigo:** é outra embalagem ou outro código de um artigo que já existe. Diz-se quantas unidades o
  código vende e o leitor passa a reconhecê-lo. Fica na auditoria.
- **Criar artigo novo:** abre a ficha com o código.

**Som**

Um bip agudo quando o artigo existe e dois graves quando não existe.

**Configurar e testar** (em **Configurações → Postos e impressão → Configurar**, valem só para o aparelho)

- Ligar ou desligar o som.
- Marcar **Leitor sem fios**: os leitores sem fios mandam os caracteres mais devagar e o KanGest dá-lhes mais
  tempo.
- O campo **Testar o leitor** mostra:
  - o código e o tipo, e se o dígito de controlo está certo;
  - os milissegundos por carácter e como o leitor acaba a leitura (Enter, Tab ou nada);
  - o artigo encontrado.

O leitor deve acabar a leitura com **Enter**. Os que acabam com **Tab** também são aceites, e os que não mandam nada
no fim são reconhecidos quando param de escrever.

**Câmara**

Nos telemóveis e tablets Android (Chrome), o botão da câmara na pesquisa do POS lê:

- EAN, UPC, Code 128, Code 39 e ITF;
- DataMatrix e QR.

No iPhone e no Windows o browser ainda não sabe ler códigos com a câmara, e o botão não aparece.

## Impressora térmica (ESC/POS)

Em **Impressora térmica (ESC/POS) e gaveta**, active «Imprimir directamente»:

| Ligação | Destino |
|---|---|
| Fila de impressão local | Windows: nome da partilha da impressora (partilhe-a com um nome sem espaços, ex. `POS80`). macOS: nome da fila. |
| Impressora de rede | Endereço IP (porta 9100), ex. `192.168.1.50` |
| Porta / dispositivo | `\\.\COM3`, `LPT1` ou `/dev/usb/lp0` |

- **Largura do papel:** 80 mm com 42 colunas (fonte normal); 80 mm com 48 colunas (letra pequena, fonte B); 58 mm com 32 colunas. O «Papel do talão» em **Impressão** acerta a largura dos dois tipos de talão.
- **Logótipo:** «Imprimir o logótipo da empresa» converte o logótipo da ficha da empresa para preto e branco, à largura do papel, sempre que se grava o posto. A pré-visualização mostra como vai sair.
- O talão leva todos os elementos fiscais do A4: empresa e NIF, documento e via, cliente e NIF, documento de origem e motivo (notas de crédito e de débito), vencimento (facturas a crédito), linhas com IVA, totais, pagamentos e troco, quadro do IVA, motivos de isenção, os 4 caracteres da assinatura com o n.º do certificado e, na Facturação Electrónica, o QR code da AGT.
- «Gravar e testar impressão» imprime um talão de teste com acentos, colunas e corte.

Sem impressão directa, o talão sai pelo diálogo de impressão do sistema, em 80 ou 58 mm.

## Gaveta do dinheiro

Ligada à impressora térmica (cabo RJ11). Abre em todas as vendas, só com numerário ou nunca. A maioria usa o pino 2. No POS, o botão «Gaveta» (Ctrl+G) abre-a e exige a permissão de movimentos de caixa.

## Pagamento noutra moeda

O POS recebe sempre kwanzas. No pagamento, a caixa «Noutra moeda (troco em Kz)» mostra **só as moedas aceites no
POS**:

- escolhem-se na instalação ou em **Tabelas → Moedas**, na coluna «Aceite no POS» ou na ficha da moeda;
- cada moeda tem de estar activa e com o câmbio do dia;
- as outras moedas activas continuam a servir nas compras e nos documentos em moeda estrangeira, mas não ocupam o
  pagamento do POS;
- sem nenhuma marcada, a caixa não aparece.

O troco sai em kwanzas. No fecho de caixa, o que entrou noutra moeda aparece à parte, na moeda («Em USD (na
gaveta)»).

Numa base que já existia, fica marcado só o dólar, se tiver câmbio.

## Visor de cliente

Em **Visor de cliente**:

- **Visor de coluna (série/USB):** os visores VFD de 2 linhas × 20. Indique a porta (`COM3` no Windows, ver no Gestor de Dispositivos; `/dev/cu.usbserial-…` no macOS) e a velocidade (quase sempre 9600). Nos comandos, escolha «Genérico (CD5220)», que serve a maioria dos visores, ou «Epson (ESC/POS)». Mostra:
  - à espera: as duas mensagens de boas-vindas;
  - na venda: o último artigo com o valor e, na segunda linha, o total;
  - no pagamento: «TOTAL A PAGAR»;
  - no fim: o troco e «Obrigado! Volte sempre».

  «Gravar e testar o visor» escreve «KanGest - visor OK» e uma régua de colunas na segunda linha.
- **Segundo ecrã:** nos terminais de ecrã duplo, o KanGest abre o ecrã do cliente em ecrã inteiro no segundo monitor ao entrar no POS. Mostra o logótipo, os artigos, o total em grande, o total a pagar e, no fim, o valor recebido e o troco. Com um só monitor abre numa janela. O botão do monitor no topo do POS volta a abri-lo se for fechado.

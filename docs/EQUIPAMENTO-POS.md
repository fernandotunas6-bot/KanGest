# Equipamento do posto de venda

O KanGest trabalha com o equipamento habitual de um posto de facturação: terminal táctil, leitor de código de barras, impressora térmica, gaveta do dinheiro e visor de cliente. Tudo se configura por posto em **Configurações → Postos e impressão → Configurar**, na máquina onde o equipamento está ligado.

## Terminal táctil

- O POS está desenhado para ecrãs a partir de 1024 × 768 (15″) e funciona também em 1280 × 1024 e 1920 × 1080. Os botões têm pelo menos 44 px, o tamanho de um dedo.
- **Teclado no ecrã:** o botão do teclado no campo de pesquisa abre um teclado para procurar artigos sem teclado físico. «Adicionar» lança o artigo encontrado.
- **Ecrã inteiro:** o botão no topo do POS alterna o ecrã inteiro. Para os terminais dedicados, active «Abrir o POS em ecrã inteiro» em **Venda**.
- Os operadores de caixa entram directamente no POS.

## Leitor de código de barras

Qualquer leitor USB que funcione como teclado serve (é o modo de origem da maioria).

- A leitura entra sempre na pesquisa, mesmo que o operador tenha tocado numa linha ou num botão antes de ler.
- O leitor deve terminar a leitura com **Enter**. Os que terminam com **Tab** também são aceites.
- Procura pelo código de barras e pela referência do artigo. `3*código` lança 3 unidades.

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

## Visor de cliente

Em **Visor de cliente**:

- **Visor de coluna (série/USB):** os visores VFD de 2 linhas × 20. Indique a porta (`COM3` no Windows, ver no Gestor de Dispositivos; `/dev/cu.usbserial-…` no macOS) e a velocidade (quase sempre 9600). Nos comandos, escolha «Genérico (CD5220)», que serve a maioria dos visores, ou «Epson (ESC/POS)». Mostra:
  - à espera: as duas mensagens de boas-vindas;
  - na venda: o último artigo com o valor e, na segunda linha, o total;
  - no pagamento: «TOTAL A PAGAR»;
  - no fim: o troco e «Obrigado! Volte sempre».

  «Gravar e testar o visor» escreve «KanGest - visor OK» e uma régua de colunas na segunda linha.
- **Segundo ecrã:** nos terminais de ecrã duplo, o KanGest abre o ecrã do cliente em ecrã inteiro no segundo monitor ao entrar no POS. Mostra o logótipo, os artigos, o total em grande, o total a pagar e, no fim, o valor recebido e o troco. Com um só monitor abre numa janela. O botão do monitor no topo do POS volta a abri-lo se for fechado.

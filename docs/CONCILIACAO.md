# Conciliação bancária

Em **Tesouraria › Conciliação Bancária** confere-se uma conta do banco com o extracto que o banco envia. A permissão
é a do fluxo de caixa (`tesouraria.fluxo`).

## Como se faz

1. No site ou na aplicação do banco, exporte o extracto do período em Excel (`.xlsx`, `.xls`), LibreOffice (`.ods`)
   ou CSV.
2. Escolha a conta e **Importar extracto**. O KanGest reconhece as colunas pelos títulos: data, descrição, débito e
   crédito (ou um só valor com sinal, ou um valor com «D/C») e saldo. As linhas sem data, como o saldo anterior e os
   totais, ficam de fora. Se alguma coluna estiver errada, corrige-se na janela antes de importar.
3. Cada linha do extracto casa sozinha com os movimentos da conta no KanGest:
   - um movimento com o mesmo valor e a data mais próxima, até 4 dias de diferença (o banco lança às vezes uns dias
     depois);
   - senão, os movimentos de um dia que somam o valor da linha. O banco liquida numa só linha as vendas com cartão
     (Multicaixa) do dia, por isso tenta-se primeiro por modo de pagamento e depois o dia inteiro.
4. O que sobra fica à vista dos dois lados:
   - **Casar**: escolhem-se os movimentos que fazem a linha. Se a soma não bater (por exemplo, a comissão que o banco
     descontou no TPA), **Casar e lançar a diferença** cria o movimento da diferença.
   - **Lançar**: uma linha que não existe no KanGest (comissões, juros, um depósito esquecido) passa a movimento de
     tesouraria, com a data e o valor do extracto.
   - Os movimentos do KanGest que o banco ainda não mostra (um cheque por descontar, um depósito do último dia)
     ficam na coluna da direita.
5. **Fechar conciliação** deixa-a como está, para consulta.

O topo mostra o saldo segundo o banco (a coluna «Saldo» do extracto), o saldo da conta no KanGest no último dia do
extracto e a diferença. Os movimentos por conciliar explicam essa diferença.

## Regras

- A conciliação **só aponta**: não altera nenhum documento nem movimento. Os lançamentos (a linha, a diferença)
  passam pelo serviço da tesouraria, com a permissão, o **fecho de períodos** e a auditoria dele.
- Um movimento só se concilia uma vez, em todas as conciliações (`conciliacao_pares.fluxo` é único).
- O mesmo extracto não entra duas vezes: mesma conta, mesmo período, mesmas linhas e mesmo total.
- Importar, casar, lançar, desfazer, fechar e apagar ficam no registo de auditoria.
- Uma conciliação aberta pode apagar-se (o extracto errado). Os movimentos lançados a partir dela continuam na
  tesouraria e anulam-se lá.

## Para quem programa

`src-tauri/src/services/conciliacao.rs` tem a leitura do extracto, que reaproveita a das folhas da importação
(`importar_folha::ler`, `numero`, `normalizar`), o casamento automático e as operações
`conciliacao.ler|criar|listar|obter|casar|lancar|descasar|fechar|eliminar`. As tabelas estão na migração `0050`. Cada
movimento do fluxo identifica-se pela chave de `vw_fluxo`:

- `D<id>`: pagamento de um documento;
- `C<id>`: movimento de caixa;
- `T<id>`: movimento de tesouraria.

O teste é `conciliacao::testes::conciliacao_por_extracto`.

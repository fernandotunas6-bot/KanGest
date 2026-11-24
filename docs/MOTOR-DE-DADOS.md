# Motor central de dados

O construtor de relatórios, os relatórios prontos do painel e a qualidade dos dados usam o mesmo motor
(`src-tauri/src/services/dados/`). É um motor **só de leitura**: consulta o que os módulos já gravam, com as
permissões que já existem. Não grava documentos, stock nem fichas. As correcções fazem-se nos ecrãs de sempre, com a
auditoria deles.

## Peças

| Peça | Ficheiro | O que faz |
|---|---|---|
| Fontes | `dados/fontes.rs` | Registo `FONTES`: o que se pode consultar, com que permissão, e os campos de cada fonte com a expressão SQL fixa |
| Consultas | `dados/consulta.rs` | `dados.fontes` e `dados.consultar`: campos, filtros, agrupamentos, agregados, ordenação por várias colunas, total geral |
| Modelos | `dados/modelos.rs` + migração `0048` | `dados.modelos`, `dados.modeloGuardar`, `dados.modeloEliminar`: relatórios guardados (do utilizador ou partilhados), auditados como as outras fichas |
| Verificações | `dados/verificacoes.rs` | `dados.verificar`: registo `VERIFICACOES` da qualidade dos dados, com exemplos e o ecrã onde se corrige |
| Ecrã | `src/lib/dados.ts`, `src/lib/construtor.ts`, `src/components/dados/Grelha.tsx` | Tipos, formatação, Excel/CSV, períodos, relatórios prontos, comparação de períodos, grelha |
| Páginas | `src/pages/ConstrutorRelatorios.tsx`, `src/pages/QualidadeDados.tsx` | Construtor (`/relatorios/construtor`) e qualidade dos dados (`/qualidade`) |

## Regras de segurança

- **Só expressões do registo.** O SQL monta-se com as expressões de `FONTES`, com os operadores
  `= <> > >= < <= contem comeca entre em vazio naovazio` e com as funções `soma media min max contar`. Nada vem como
  texto do ecrã. As colunas têm aliases internos (`c0`, `c1`…).
- **Valores sempre como parâmetros.** Um filtro com `x' OR '1'='1` procura esse texto. Num `contem`, o `%` procura-se
  como carácter.
- **A permissão é a da fonte.** Cada fonte tem uma permissão de `state::PERMISSOES` (`relatorios.ver`,
  `tesouraria.fluxo`, `compras.fatura`). Uma fonte sem permissão não aparece em `dados.fontes` e `dados.consultar`
  recusa-a.
- **Utilizador de uma loja.** `Fonte::loja` é a expressão do armazém de cada linha. Com `st.loja()` (utilizador
  limitado a uma loja), junta-se `AND <loja> = ?` antes dos filtros. A loja vem da sessão, nunca do pedido, por isso um
  filtro pedido no ecrã não a contorna. A tesouraria não é por loja, como o fluxo de caixa. Nas verificações, `{LOJA}`
  marca o sítio da mesma condição.
- **Limites.** 1 000 linhas por omissão e 5 000 no máximo (o Excel e o CSV), 200 valores num filtro `em`. O total geral
  conta todas as linhas que passam nos filtros, não só as mostradas.

## Totais que batem certo

As fontes de vendas usam as regras dos relatórios fixos (`relatorios::VENDAS` e `relatorios::SINAL`):

- facturas, facturas-recibo e notas de débito somam;
- notas de crédito subtraem;
- anulados ficam de fora.

As compras usam a regra do mapa de fornecedor (notas de crédito de fornecedores a subtrair). As contas a receber usam o
critério das contas correntes (documentos normais com valor em dívida). Os testes comparam o motor com os relatórios
fixos para o mesmo período:

- `consultas_seguras_e_certas`: vendas, famílias, compras, stock e contas a receber;
- `so_a_loja_do_utilizador`: a regra da loja;
- `modelos_de_relatorio`: os modelos e a auditoria;
- `encontra_os_problemas`: as verificações.

## Acrescentar uma fonte

1. Uma entrada em `FONTES` com:
   - `id`;
   - `rotulo`;
   - `descricao`;
   - `permissao` (uma das que já existem);
   - `de` (tabelas e um `WHERE` com a condição base);
   - `data` (o campo do período, ou vazio num retrato de agora);
   - `loja` (a expressão do armazém, ou vazio);
   - os campos.
2. Cada campo tem um tipo: `Texto`, `Numero`, `Dinheiro` (cêntimos), `Data` (AAAA-MM-DD) ou `Logico`. Um número que
   não se soma (preço unitário, taxa, dias) é `nao_somavel`. Se a loja aparece num campo, o id do campo é `loja`.
3. O teste `registo_coerente` confirma:
   - que não há ids repetidos;
   - que o `WHERE` existe;
   - que a permissão existe;
   - que o campo de data é uma data;
   - que uma fonte com armazém tem a expressão da loja.

   O teste `consultas_seguras_e_certas` corre cada fonte com todos os campos e agrupada por cada campo de texto.
4. No ecrã, um ponto de partida em `PADRAO` (`ConstrutorRelatorios.tsx`) e, se fizer sentido, um relatório pronto em
   `PRONTOS` (`src/lib/construtor.ts`).

## Acrescentar uma verificação

1. Uma entrada em `VERIFICACOES` com:
   - `id`;
   - `titulo`;
   - `texto` (porque importa e como se corrige);
   - `gravidade` (`erro` ou `aviso`);
   - `permissao`;
   - `contar` (uma linha com o número de casos);
   - `exemplos` (até 20 linhas com `rotulo` e `detalhe`);
   - `corrigir` (o ecrã onde se corrige);
   - `loja`.
2. No `detalhe`, `{kz:cêntimos}`, `{qtd:número}` e `{data:AAAA-MM-DD}` saem no formato do programa
   (1 234,56 Kz, 31/01/2020).
3. Acrescente o caso ao teste `encontra_os_problemas`.

## Relatórios prontos e painel

`PRONTOS` (`src/lib/construtor.ts`) tem definições com nome, iguais às que um modelo guarda. Cada utilizador só vê os
das fontes que pode consultar. Para abrir um pronto: `/relatorios/construtor?pronto=<id>&periodo=<período>`, com
`linkPronto(id, periodo)`. O painel usa-o nos indicadores (`Stat` com `onAnalisar`), só para quem tem
`relatorios.ver`.

## Comparação de períodos

No resumo, o construtor faz uma segunda consulta com o período comparado (`periodoComparado`) e junta-a grupo a grupo
(`juntarComparacao`). Ao lado de cada valor aparecem o valor anterior (`<id>#anterior`) e a variação em percentagem
(`<id>#var`).

O período comparado escolhe-se assim:
- o mês (ou o mês até hoje) compara com o mês anterior até ao mesmo dia;
- o ano compara com o ano anterior;
- outro intervalo compara com os mesmos dias imediatamente antes (hoje com ontem);
- «ano anterior» são as mesmas datas um ano antes;
- o fim do mês e o 29 de Fevereiro encostam ao último dia do mês.

Não se compara um agrupamento por data, mês ou ano, porque os grupos de um período não existem no outro. As colunas da
comparação não vêm do motor e por isso não se ordenam no servidor.

# Regras de aprovação

Em **Sistema › Regras de Aprovação** o administrador escolhe que operações precisam do PIN de quem pode aprovar. A
página pede a permissão `sistema.utilizadores`. As regras **chegam desligadas**: até se ligarem, tudo funciona como
antes.

## As quatro regras

| Regra | Aplica-se a | Quem aprova (permissão) |
|---|---|---|
| Anular documentos | anular qualquer documento; opcionalmente, só os de dias anteriores | «Aprovar anulações de documentos» (`aprovar.anulacao`) |
| Saídas de tesouraria | pagamentos e saídas na tesouraria, pagamentos a fornecedores (VRC), devoluções de dinheiro a clientes, lançamentos de saída da conciliação bancária | «Aprovar saídas de tesouraria, pagamentos e devoluções» (`aprovar.saidas`) |
| Vales de caixa | saídas de dinheiro da gaveta do POS | «Aprovar vales de caixa» (`aprovar.vales`) |
| Notas de crédito e regularizações | notas de crédito (NC) emitidas e regularizações de conta corrente | «Aprovar notas de crédito e regularizações» (`aprovar.creditos`) |

Cada regra tem um valor **«A partir de»**: abaixo dele, a operação não pede o PIN. Vazio quer dizer qualquer valor.
As entradas de dinheiro e as transferências entre contas da empresa nunca pedem o PIN.

## Como funciona

- Quando uma regra se aplica, abre-se o pedido de **PIN**. Quem tem a permissão de aprovar dessa regra escreve o seu
  PIN, e a operação continua. Pode ser o próprio utilizador, se tiver a permissão.
- O gestor tem as quatro permissões por omissão. O operador de caixa não tem nenhuma. Mudam-se em **Utilizadores e
  Permissões**, no grupo «Aprovar com o PIN».
- Uma regra só se liga se houver alguém que a possa aprovar, com a permissão **e** com um PIN definido. Assim não
  fica nenhuma operação impossível de fazer.
- Um PIN errado, ou de quem não pode aprovar, é recusado no próprio pedido, para se tentar outro. **Cancelar** não faz
  a operação.
- Cada aprovação fica registada, e vê-se em baixo na mesma página: quando, a operação, o documento ou movimento, o
  valor, quem pediu e quem aprovou. O documento aprovado (nota de crédito, pagamento) mostra também quem o autorizou.
- Os pedidos da **API de integração** (loja online) não passam pelas regras: não há quem escreva um PIN, e as chaves
  de acesso têm as suas próprias permissões.

## Para quem programa

A regra está num só sítio, `src-tauri/src/services/aprovacoes.rs`:

- `aprovacoes::exigir(st, Regra::…, valor, data, operacao, pin)` diz se a regra se aplica e, se sim, exige o PIN. Se
  faltar o PIN, devolve um erro com a frase `PEDE_PIN`. Se o PIN servir, devolve a aprovação.
- `aprovacoes::registar(&mut tx, aprovacao, referencia)` grava a aprovação na mesma transacção da operação.

As operações recebem o PIN no campo `autorizacao_pin`. Do lado do ecrã, `call` (`src/lib/api.ts`) reconhece a frase
`PEDE_PIN` e abre o diálogo `components/AprovacaoPin.tsx`, montado uma só vez na raiz da aplicação. Depois repete a
mesma chamada com o PIN. Nenhum ecrã precisa de código próprio para isto. As tabelas estão na migração `0051`. O
teste é `aprovacoes::testes::regras_de_aprovacao`.

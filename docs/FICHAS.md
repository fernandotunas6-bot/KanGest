# Fichas: criar, alterar, eliminar

Como funcionam as fichas de **artigos**, **clientes** e **fornecedores** (e, no que é comum, as outras janelas de
edição). Tudo o que muda fica na auditoria, com o antes e o depois (ver `UTILIZADORES.md`, «Auditoria»).

## Eliminar com segurança

O botão **Eliminar** da ficha pergunta antes e explica o que vai acontecer:

- uma ficha que **nunca foi usada** é apagada;
- uma ficha **já usada** fica só **desactivada**: sai das listas, das vendas, da pesquisa e da escolha nos
  documentos, mas os documentos antigos continuam a mostrá-la.
  - Um artigo conta como usado se tiver documentos, movimentos de stock, lotes, avenças, ordens da oficina ou
    promoções, ou se entrar num kit.
  - Uma entidade conta como usada se tiver documentos, avenças, viaturas, pontos ou outra ligação.

As desactivadas encontram-se no filtro **Inactivos** da lista. Reactivam-se:

- os artigos, na ficha («Artigo activo») ou em lote;
- os clientes e os fornecedores, em lote («Reactivar»).

Uma entidade não se reactiva se o NIF dela já estiver noutra ficha activa.

O «Consumidor Final» não se elimina nem se desactiva.

## Validações

- Os campos obrigatórios (descrição do artigo, nome da entidade) ficam a vermelho com a explicação, e o cursor vai
  para eles.
- **Enquanto se escreve**, o KanGest avisa se:
  - a referência ou o código de barras já está noutro artigo (diz qual, e se está desactivado);
  - o NIF já está noutra ficha activa;
  - o NIF não é válido em Angola.
- Fechar uma ficha com **alterações por guardar** (Esc, «X», clicar fora ou «Cancelar») pergunta antes. Há três
  respostas: continuar a editar, sair sem guardar ou guardar.

## Listas

As listas de artigos, clientes e fornecedores têm:

- a pesquisa: nos artigos, por descrição, referência, código de barras ou atributos do ramo; nas entidades, por
  nome, NIF, telefone ou email;
- o estado (**Activos**, **Inactivos**, **Todos**);
- a ordenação por coluna;
- a exportação para Excel.

**Seleccionar várias linhas** (as caixas à esquerda) mostra as acções em lote:

- **Artigos**:
  - activar ou desactivar;
  - mudar de família (a subfamília fica vazia);
  - **ajustar preços** numa percentagem. Mexe só no PVP 1 com IVA: o preço sem IVA acompanha e os outros níveis
    não mudam. O PVP novo pode ser arredondado a 1, 5, 10, 50, 100 ou 500 Kz, e a janela mostra uma amostra antes
    de aplicar.
- **Clientes e fornecedores**: reactivar ou desactivar.

Cada ficha alterada em lote fica na auditoria.

## Atalhos

- **Ctrl+S** (⌘S no Mac) guarda a ficha aberta. Funciona nos artigos, nas entidades e nos utilizadores.
- **Esc** fecha a janela, e pergunta antes se houver alterações por guardar.
- **Guardar e novo**, nas fichas novas, guarda e abre logo outra em branco, para lançar várias de seguida.
- **Clonar**, nos artigos, cria outro parecido.

## Para quem programa

- Servidor:
  - `artigos.eliminar`, `artigos.lote`, `artigos.codigoUsado` (`services/catalogo.rs`);
  - `entidades.eliminar`, `entidades.lote`, `entidades.nifUsado` e o filtro `inativos` de `entidades.listar`
    (`services/entidades.rs`).
  - Eliminar tenta apagar numa transacção. Se uma chave estrangeira recusar (uma ligação que não se previu),
    desfaz e desactiva.
- Auditoria:
  - as acções em lote registam cada ficha com `auditoria::ficha` e `auditoria::registar_ficha`;
  - «eliminar» que desactiva fica como `DESACTIVAR`.
- Interface (`components/ui`):
  - `Modal` com `sujo` (pergunta antes de fechar) e `onGuardar` (Ctrl+S);
  - `BotaoCancelar` (fecha com a mesma pergunta), `Confirmar`, `Atalho`, e `Field` com `erro` e `aviso`;
  - só a janela de cima responde ao teclado.
- Interface (`lib/ficha.ts`): `useSujo` e `useJaUsado`, que pergunta ao servidor enquanto se escreve.

# Utilizadores, permissões e autenticação

O módulo está em **Sistema → Utilizadores e Permissões**. Tem cinco separadores:

| Separador | Para quê | Quem vê |
|---|---|---|
| Utilizadores | Criar e editar contas, perfil, PIN, limite de desconto, permissões próprias, redefinir senha, desbloquear | permissão *Utilizadores e permissões* |
| Perfis e permissões | Perfis de acesso (Administrador, Gestor, Caixa e os criados pela empresa) e a matriz de permissões de cada um | idem |
| Registo de acessos | Entradas, saídas, tentativas falhadas, bloqueios, mudanças de senha, por posto | permissão *Registo de acessos e auditoria* |
| Auditoria | Operações registadas (emissões, anulações, importações, alterações de configuração…) | idem |
| Segurança | Política de senhas, bloqueio por tentativas, bloqueio por inactividade, entrada por PIN | *Utilizadores e permissões* |

O menu **Sistema → Registo de Acessos e Auditoria** abre directamente o registo de acessos.

## Perfis de acesso

Cada utilizador tem um perfil. As permissões do perfil aplicam-se a todos os seus utilizadores, excepto aos que tenham
**permissões próprias** (caixa «Permissões próprias deste utilizador» na ficha).

| Perfil | Escalão | Permissões de origem |
|---|---|---|
| Administrador | 3 | Todas, sempre (incluindo as de módulos novos). Não se pode editar. |
| Gestor | 2 | Todas excepto as de administração: utilizadores, configurações, actualizações, facturação electrónica, importação, parametrização, certificação, auditoria e configuração de RH. |
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

## Registo

Ficam no **Registo de acessos**: `Entrada`, `Saída`, `Tentativa falhada` (com o utilizador escrito, mesmo que não exista),
`Conta bloqueada`, `Conta desbloqueada`, `Senha alterada`, `Senha redefinida`, `Sessão bloqueada`, `Sessão retomada`,
com data, hora e posto. A **Auditoria** guarda as operações (quem criou ou alterou utilizadores e perfis, mudou a
política de segurança, emitiu ou anulou documentos, importou dados…).

## Para quem programa

- Catálogo de permissões: `PERMISSOES` em `src-tauri/src/state.rs` e `GRUPOS_PERMISSOES` em `src/lib/permissoes.ts`
  (têm de ter as mesmas chaves). Permissões só do administrador: `SO_ADMIN`.
- Cada operação do servidor verifica `st.exigir_perm("…")`. As permissões efectivas vêm de
  `permissoes_efectivas(perfil, próprias)`.
- Métodos sem sessão completa: `setup.*`, `auth.login`, `auth.loginPin`, `auth.logout`, `auth.sessao`,
  `auth.alterarSenha`, `auth.desbloquear`, `seguranca.politica`. Todos os outros são recusados com a sessão bloqueada
  ou com senha provisória.
- Tabelas: `perfis`, `acessos`; colunas de segurança em `utilizadores` (migração `0016_utilizadores_permissoes.sql`).

# Base de dados

O KanGest guarda tudo num ficheiro SQLite cifrado com o SQLCipher (`kigest.db`, na pasta de dados: ver
[INSTALACAO.md](INSTALACAO.md) e [SEGURANCA.md](SEGURANCA.md) §5). Para o abrir num editor de SQLite, use a cópia
exportada (em claro), não o ficheiro em uso. Num
conjunto de postos em rede, só o servidor tem a base; os postos falam com ele ([REDE.md](REDE.md)).

## Como está configurada

| | |
|---|---|
| Registo | WAL (leituras não esperam pelas escritas), `synchronous = NORMAL` |
| Integridade | chaves estrangeiras ligadas em todas as ligações; documentos assinados protegidos por gatilhos na própria base |
| Escritas | transacções `BEGIN IMMEDIATE` (`db::transacao`): uma segunda gravação espera pela primeira (`busy_timeout` de 10 s) em vez de falhar com «database is locked» |
| Segredos | cifrados (chave mestra fora da base, `kangest.chave`): ver [SEGURANCA.md](SEGURANCA.md) |
| Cópias | automáticas, externas e cifradas, verificadas com `PRAGMA integrity_check`: ver [COPIAS.md](COPIAS.md) |

## Migrações

As alterações ao esquema estão em `src-tauri/migrations/` (`0001_…sql`, `0002_…sql`, …) e aplicam-se sozinhas ao abrir
a base, por ordem; o SQLite guarda as aplicadas em `_sqlx_migrations`.

- Uma migração já publicada **nunca se altera**: as correcções vão numa migração nova.
- Antes de actualizar, o KanGest faz uma cópia da base (`copias/kangest-antes-de-<versão>.db`).
- Uma cópia feita por uma versão mais recente (com migrações que esta não conhece) não se restaura: a base deixaria de
  abrir.

## Índices e desempenho

Os índices das tabelas que crescem com o uso (documentos, linhas, pagamentos, movimentos de stock e de tesouraria)
foram escolhidos medindo as consultas numa base com **150 000 documentos** (~3 anos de uma loja), com o plano de cada
consulta (`EXPLAIN QUERY PLAN`). A migração `0028_indices.sql` explica cada um. Resultados (chamadas reais do programa):

| Chamada | Antes | Depois |
|---|---|---|
| Estado do caixa (POS, a cada venda) | 200 ms | 1,3 ms |
| Painel | 511 ms | 69 ms |
| Contas correntes: pendentes | 207 ms | 5 ms |
| Documentos de um cliente | 115 ms | 1,3 ms |
| Saldos das contas da tesouraria | 400 ms | 139 ms |
| Vendas por dia / artigo / família (ano) | 97 / 139 / 135 ms | 46 / 92 / 83 ms |
| Total das 24 chamadas medidas | ~3 000 ms | ~1 400 ms |

Regras que saíram desta medição, para as consultas novas:

- **Filtros opcionais montados à medida** (`sqlx::QueryBuilder`), não `(? IS NULL OR coluna = ?)`: com esta forma o
  SQLite não sabe, ao preparar, que filtro vem preenchido e não usa os índices (era o que fazia a lista de documentos).
- **Um período de vendas** filtra por `tipo_codigo` e `data_emissao`: o índice `(tipo_codigo, data_emissao)` lê só o
  período. Sem ele, o SQLite escolhia o índice da numeração e lia todas as vendas desde sempre.
- **Documentos por pagar** com `estado = 'N' AND valor_pendente > 0` escritos assim na consulta: é o que permite usar o
  índice parcial dos pendentes.
- **Somas por grupo numa só passagem** (`GROUP BY` e `LEFT JOIN`), não uma subconsulta por linha (os saldos das contas
  liam todos os movimentos uma vez por conta).
- Uma junção a partir do conjunto pequeno (os documentos de uma sessão), não a partir da tabela grande (todos os
  pagamentos).

### Medir de novo

```bash
cd src-tauri && cargo test --lib servidor_demo -- --ignored --nocapture      # base de demonstração em /tmp/kigest-auditoria
python3 scripts/base-volume.py /tmp/kigest-auditoria/kigest.db /tmp/volume.db 150000
KANGEST_BASE=/tmp/volume.db cargo test --release --lib volume_tempos -- --ignored --nocapture
```

`volume_tempos` (em `src-tauri/src/services/tests.rs`) mede 24 chamadas: painel, listas de documentos, relatórios de
vendas, síntese, tesouraria, contas correntes, clientes, stock e caixa.

## Verificações

- **Cadeia de assinaturas dos documentos**: Configurações → Certificação AGT → «Verificar agora».
- **Cada cópia de segurança** é verificada logo depois de criada (`PRAGMA integrity_check`) e, ao restaurar, também a
  versão das migrações.
- Recálculo de stock (Gestão de Stock → Recálculo) compara as existências com os movimentos.

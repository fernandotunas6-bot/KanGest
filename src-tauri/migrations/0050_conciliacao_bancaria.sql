-- Conciliação bancária (decisão do dono, 2026-11-24: importar o extracto). Cada linha do extracto casa com um ou mais
-- movimentos do fluxo de caixa da conta (vw_fluxo, pela chave: D = pagamento de documento, C = caixa, T = tesouraria).
-- Várias vendas com cartão do mesmo dia costumam entrar no banco numa só linha. Um movimento só se concilia uma vez
-- (fluxo UNIQUE).
-- A conciliação não altera nada do que concilia: só aponta. O que falta no KanGest (comissões, juros) lança-se pela
-- tesouraria, com as regras de sempre (permissão, fecho de períodos, auditoria).
CREATE TABLE conciliacoes (
    id INTEGER PRIMARY KEY,
    conta_id INTEGER NOT NULL REFERENCES contas(id),
    ficheiro TEXT NOT NULL,
    de TEXT NOT NULL,
    ate TEXT NOT NULL,
    -- saldo final segundo o banco (a coluna «Saldo» do extracto), quando a há
    saldo_banco INTEGER,
    estado TEXT NOT NULL DEFAULT 'ABERTA' CHECK (estado IN ('ABERTA', 'FECHADA')),
    criado_em TEXT NOT NULL,
    utilizador_id INTEGER REFERENCES utilizadores(id),
    fechado_em TEXT
);
CREATE INDEX idx_conciliacoes_conta ON conciliacoes(conta_id, de);

CREATE TABLE conciliacao_linhas (
    id INTEGER PRIMARY KEY,
    conciliacao_id INTEGER NOT NULL REFERENCES conciliacoes(id) ON DELETE CASCADE,
    -- a linha no ficheiro (como o Excel a numera)
    linha INTEGER NOT NULL,
    data TEXT NOT NULL,
    descricao TEXT NOT NULL,
    -- cêntimos: positivo entra na conta, negativo sai
    valor INTEGER NOT NULL,
    -- como casou: auto (pelo valor e pela data), manual, lancado (movimento criado a partir da linha); NULL por casar
    casado TEXT CHECK (casado IN ('auto', 'manual', 'lancado'))
);
CREATE INDEX idx_conciliacao_linhas ON conciliacao_linhas(conciliacao_id);

CREATE TABLE conciliacao_pares (
    linha_id INTEGER NOT NULL REFERENCES conciliacao_linhas(id) ON DELETE CASCADE,
    fluxo TEXT NOT NULL UNIQUE,
    PRIMARY KEY (linha_id, fluxo)
);

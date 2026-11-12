-- Paridade com o KIGEST POS: permissões por operação + PIN de supervisor, configuração
-- de postos, vendas suspensas, logótipo da empresa e rubricas nos movimentos de caixa.
-- (ALTER TABLE ... ADD COLUMN sem REFERENCES: o SQLite não permite FK com DEFAULT não nulo.)

ALTER TABLE utilizadores ADD COLUMN pin_hash TEXT;
ALTER TABLE utilizadores ADD COLUMN permissoes TEXT;               -- JSON ["pos.desconto", ...]; NULL = padrão do perfil
ALTER TABLE utilizadores ADD COLUMN limite_desconto REAL NOT NULL DEFAULT 0;  -- 0 = sem limite

ALTER TABLE postos ADD COLUMN armazem_id INTEGER NOT NULL DEFAULT 1;
ALTER TABLE postos ADD COLUMN serie_prefixo TEXT NOT NULL DEFAULT '';
ALTER TABLE postos ADD COLUMN opcoes TEXT;                          -- JSON (impressão, repetir linhas, …)

ALTER TABLE empresa ADD COLUMN logo TEXT;                            -- data URL (PNG/JPEG)

ALTER TABLE documentos ADD COLUMN autorizado_por INTEGER;            -- supervisor que autorizou preço/desconto

ALTER TABLE caixa_movimentos ADD COLUMN rubrica TEXT;
ALTER TABLE caixa_movimentos ADD COLUMN modo_codigo TEXT;

CREATE TABLE vendas_suspensas (
    id INTEGER PRIMARY KEY,
    posto_id INTEGER,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id),
    entidade_id INTEGER,
    entidade_nome TEXT,
    descricao TEXT,
    total INTEGER NOT NULL DEFAULT 0,
    dados TEXT NOT NULL,                                             -- JSON do carrinho
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

UPDATE postos SET opcoes = '{"formato":"ticket","vias":1,"imprimir_auto":false,"mostrar_opcoes_impressao":true,"valor_minimo_impressao":0,"repetir_linhas":false,"confirmar_cliente":false}'
WHERE opcoes IS NULL;

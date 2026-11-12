-- Lotes e validades, artigos compostos (kits), moedas/câmbio e custos adicionais de compra.

-- ------------------------------------------------------------------ lotes
ALTER TABLE artigos ADD COLUMN controla_lote INTEGER NOT NULL DEFAULT 0;
ALTER TABLE artigos ADD COLUMN aviso_validade_dias INTEGER NOT NULL DEFAULT 30;
ALTER TABLE artigos ADD COLUMN composto INTEGER NOT NULL DEFAULT 0;

CREATE TABLE lotes (
    id INTEGER PRIMARY KEY,
    artigo_id INTEGER NOT NULL REFERENCES artigos(id),
    armazem_id INTEGER NOT NULL REFERENCES armazens(id),
    lote TEXT NOT NULL,
    data_fabrico TEXT,
    validade TEXT,
    quantidade REAL NOT NULL DEFAULT 0,
    custo INTEGER NOT NULL DEFAULT 0,
    ultima_entrada TEXT,
    ultima_saida TEXT,
    ativo INTEGER NOT NULL DEFAULT 1,
    UNIQUE (artigo_id, armazem_id, lote)
);
CREATE INDEX idx_lotes_validade ON lotes(artigo_id, armazem_id, validade);

-- Movimentos por lote (permitem anular documentos com exactidão)
CREATE TABLE lote_movimentos (
    id INTEGER PRIMARY KEY,
    lote_id INTEGER NOT NULL REFERENCES lotes(id),
    documento_id INTEGER NOT NULL REFERENCES documentos(id),
    quantidade REAL NOT NULL
);

ALTER TABLE documento_linhas ADD COLUMN lote TEXT;
ALTER TABLE documento_linhas ADD COLUMN validade TEXT;

-- ------------------------------------------------------------------ artigos compostos (kits)
CREATE TABLE artigo_componentes (
    artigo_id INTEGER NOT NULL REFERENCES artigos(id) ON DELETE CASCADE,
    componente_id INTEGER NOT NULL REFERENCES artigos(id),
    quantidade REAL NOT NULL,
    PRIMARY KEY (artigo_id, componente_id)
);

-- ------------------------------------------------------------------ moedas e custos de compra
CREATE TABLE moedas (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL,
    simbolo TEXT NOT NULL,
    cambio REAL NOT NULL DEFAULT 0,          -- Kz por 1 unidade
    ativo INTEGER NOT NULL DEFAULT 1
);
INSERT INTO moedas (codigo, descricao, simbolo, cambio) VALUES
    ('AOA', 'Kwanza', 'Kz', 1), ('USD', 'Dólar americano', 'US$', 0), ('EUR', 'Euro', '€', 0),
    ('ZAR', 'Rand sul-africano', 'R', 0), ('CNY', 'Yuan chinês', '¥', 0);

ALTER TABLE documentos ADD COLUMN moeda TEXT NOT NULL DEFAULT 'AOA';
ALTER TABLE documentos ADD COLUMN cambio REAL NOT NULL DEFAULT 1;
ALTER TABLE documentos ADD COLUMN total_moeda INTEGER;               -- total na moeda estrangeira (cêntimos)

CREATE TABLE tipos_custo (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL
);
INSERT INTO tipos_custo (codigo, descricao) VALUES
    ('FRETE', 'Frete'), ('SEGURO', 'Seguro'), ('DESALF', 'Desalfandegamento'),
    ('TRANSP', 'Transporte interno'), ('DESPACHO', 'Despachante'), ('OUTROS', 'Outros custos');

-- Custos adicionais de uma compra (entram no custo de aquisição dos artigos, não no total do documento)
CREATE TABLE documento_custos (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL REFERENCES documentos(id) ON DELETE CASCADE,
    tipo TEXT NOT NULL REFERENCES tipos_custo(codigo),
    descricao TEXT,
    valor INTEGER NOT NULL                     -- em Kz (cêntimos)
);

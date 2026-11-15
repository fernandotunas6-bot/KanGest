-- Promoções por regras (além do desconto entre datas da ficha do artigo) e cupões de desconto.
CREATE TABLE promocao_regras (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL,
    tipo TEXT NOT NULL,                  -- LEVE_PAGUE | QUANTIDADE | HORARIO
    artigo_id INTEGER REFERENCES artigos(id),
    familia_id INTEGER REFERENCES familias(id),  -- sem artigo nem família: todos os artigos
    leve REAL,                           -- LEVE_PAGUE: leve N…
    pague REAL,                          -- …pague M
    quantidade_min REAL,                 -- QUANTIDADE: a partir de X unidades
    desconto_pct REAL,                   -- QUANTIDADE e HORARIO
    hora_inicio TEXT,                    -- HORARIO: HH:MM
    hora_fim TEXT,
    dias_semana TEXT,                    -- ex.: "12345" (1 = segunda … 7 = domingo); vazio = todos
    data_inicio TEXT,
    data_fim TEXT,
    ativo INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE cupoes (
    id INTEGER PRIMARY KEY,
    codigo TEXT NOT NULL UNIQUE,
    descricao TEXT,
    tipo TEXT NOT NULL,                  -- PCT | VALOR
    valor REAL NOT NULL,                 -- % ou cêntimos (com IVA)
    valor_minimo INTEGER NOT NULL DEFAULT 0,   -- compra mínima (cêntimos, com IVA)
    data_inicio TEXT,
    data_fim TEXT,
    utilizacoes_max INTEGER,             -- vazio = sem limite
    utilizacoes INTEGER NOT NULL DEFAULT 0,
    ativo INTEGER NOT NULL DEFAULT 1
);

ALTER TABLE documentos ADD COLUMN cupao TEXT;

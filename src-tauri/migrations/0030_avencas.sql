-- Avenças: contratos de prestação de serviços facturados periodicamente (contabilidade, segurança,
-- limpeza, manutenção, mensalidades…). Cada período gera uma factura (FT) na data em que se factura.

CREATE TABLE avencas (
    id INTEGER PRIMARY KEY,
    entidade_id INTEGER NOT NULL REFERENCES entidades(id),
    descricao TEXT NOT NULL,
    periodicidade TEXT NOT NULL DEFAULT 'MENSAL' CHECK (periodicidade IN ('MENSAL', 'TRIMESTRAL', 'SEMESTRAL', 'ANUAL')),
    -- próxima data a facturar (AAAA-MM-DD); avança um período por cada factura emitida
    proxima TEXT NOT NULL,
    -- última data a facturar (vazio = sem fim)
    fim TEXT,
    prazo TEXT,                                   -- prazo de pagamento (prazos_pagamento.codigo)
    observacoes TEXT,                             -- texto da factura; {periodo} = período facturado
    vendedor_id INTEGER REFERENCES funcionarios(id),
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    alterado_em TEXT
);
CREATE INDEX idx_avencas_proxima ON avencas(proxima) WHERE ativo = 1;

CREATE TABLE avenca_linhas (
    id INTEGER PRIMARY KEY,
    avenca_id INTEGER NOT NULL REFERENCES avencas(id) ON DELETE CASCADE,
    artigo_id INTEGER NOT NULL REFERENCES artigos(id),
    descricao TEXT,                               -- vazio = a do artigo; {periodo} = período facturado
    quantidade REAL NOT NULL DEFAULT 1 CHECK (quantidade > 0),
    preco INTEGER,                                -- sem IVA, cêntimos (vazio = preço do artigo)
    desconto REAL NOT NULL DEFAULT 0 CHECK (desconto BETWEEN 0 AND 100)
);
CREATE INDEX idx_avenca_linhas ON avenca_linhas(avenca_id);

-- um período facturado uma só vez: o registo entra antes da factura e guarda o documento emitido
CREATE TABLE avenca_emissoes (
    id INTEGER PRIMARY KEY,
    avenca_id INTEGER NOT NULL REFERENCES avencas(id),
    periodo TEXT NOT NULL,                        -- AAAA-MM-DD do início do período facturado
    documento_id INTEGER REFERENCES documentos(id),
    emitido_em TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    UNIQUE (avenca_id, periodo)
);

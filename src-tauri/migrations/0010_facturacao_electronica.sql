-- Facturação Electrónica da AGT (Decreto Presidencial 71/25): comunicação das facturas
-- e recibos aos serviços sigt/fe da AGT, séries atribuídas pela AGT.

CREATE TABLE fe_envios (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL UNIQUE REFERENCES documentos(id),
    estado TEXT NOT NULL DEFAULT 'PENDENTE',  -- PENDENTE | ENVIADO | VALIDO | INVALIDO
    request_id TEXT,
    erros TEXT,                               -- JSON [{codigo, descricao}]
    tentativas INTEGER NOT NULL DEFAULT 0,
    corrige TEXT,                             -- n.º do documento rejeitado que este corrige (documentStatus C)
    ambiente TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    enviado_em TEXT,
    verificado_em TEXT
);
CREATE INDEX idx_fe_estado ON fe_envios(estado);

ALTER TABLE series ADD COLUMN fe_agt INTEGER NOT NULL DEFAULT 0;  -- série atribuída pela AGT (solicitarSerie)

INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('fe.ativo', '0'),
    ('fe.ambiente', 'HML'),
    ('fe.estabelecimento', 'SEDE'),
    ('fe.produto', 'KiGest');

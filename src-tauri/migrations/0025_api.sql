-- API de integração: outros sistemas (loja online, contabilidade, BI, apps) ligam-se ao KanGest
-- com chaves de acesso. Cada chave age em nome de um utilizador, com as permissões escolhidas para ela.

CREATE TABLE api_chaves (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,                       -- ex.: «Loja online», «Contabilidade»
    prefixo TEXT NOT NULL UNIQUE,             -- início da chave (identifica-a sem a revelar)
    hash TEXT NOT NULL,                       -- SHA-256 (hex) da chave completa; a chave não se guarda
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id),
    posto_id INTEGER REFERENCES postos(id),   -- série e armazém dos documentos (NULL = posto 1)
    permissoes TEXT NOT NULL DEFAULT '[]',    -- JSON ["api.artigos", "vendas.fatura_recibo", ...]
    ips TEXT,                                 -- IPs ou redes permitidos (ex.: 192.168.1.0/24), separados por vírgulas
    limite_minuto INTEGER NOT NULL DEFAULT 120, -- pedidos por minuto (0 = sem limite)
    expira_em TEXT,                           -- AAAA-MM-DD (vazio = sem validade)
    ativa INTEGER NOT NULL DEFAULT 1,
    revogada_em TEXT,
    criado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    criado_por INTEGER REFERENCES utilizadores(id),
    ultimo_uso TEXT,
    ultimo_ip TEXT,
    pedidos INTEGER NOT NULL DEFAULT 0
);

-- Registo dos pedidos feitos à API (guardado 90 dias)
CREATE TABLE api_pedidos (
    id INTEGER PRIMARY KEY,
    data_hora TEXT NOT NULL,
    chave_id INTEGER,
    ip TEXT,
    metodo TEXT NOT NULL,
    caminho TEXT NOT NULL,
    estado INTEGER NOT NULL,                  -- código HTTP da resposta
    duracao_ms INTEGER NOT NULL DEFAULT 0,
    erro TEXT
);
CREATE INDEX idx_api_pedidos_data ON api_pedidos(data_hora);
CREATE INDEX idx_api_pedidos_chave ON api_pedidos(chave_id, id);

-- Pedido de emissão repetido com a mesma «Idempotency-Key»: devolve o documento já emitido em vez de outro.
CREATE TABLE api_idempotencia (
    chave_id INTEGER NOT NULL,
    idem TEXT NOT NULL,
    documento_id INTEGER NOT NULL REFERENCES documentos(id),
    criado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    PRIMARY KEY (chave_id, idem)
);

-- Documentos emitidos pela API: que chave os emitiu (vendas por canal).
ALTER TABLE documentos ADD COLUMN api_chave_id INTEGER;
CREATE INDEX idx_docs_api_chave ON documentos(api_chave_id);

-- Sincronização: última alteração de artigos e entidades (consultas «alterado_desde» da API).
-- Mantida por gatilhos, sem mexer no código que grava artigos e entidades.
ALTER TABLE artigos ADD COLUMN alterado_em TEXT;
ALTER TABLE entidades ADD COLUMN alterado_em TEXT;
UPDATE artigos SET alterado_em = replace(criado_em, ' ', 'T');
UPDATE entidades SET alterado_em = replace(criado_em, ' ', 'T');
CREATE INDEX idx_artigos_alterado ON artigos(alterado_em);
CREATE INDEX idx_entidades_alterado ON entidades(alterado_em);

-- (os gatilhos não se disparam a si próprios: recursive_triggers está desligado no SQLite por omissão)
CREATE TRIGGER trg_artigos_criado AFTER INSERT ON artigos
BEGIN
    UPDATE artigos SET alterado_em = strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime') WHERE id = NEW.id;
END;
CREATE TRIGGER trg_artigos_alterado AFTER UPDATE ON artigos WHEN NEW.alterado_em IS OLD.alterado_em
BEGIN
    UPDATE artigos SET alterado_em = strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime') WHERE id = NEW.id;
END;
CREATE TRIGGER trg_entidades_criada AFTER INSERT ON entidades
BEGIN
    UPDATE entidades SET alterado_em = strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime') WHERE id = NEW.id;
END;
CREATE TRIGGER trg_entidades_alterada AFTER UPDATE ON entidades WHEN NEW.alterado_em IS OLD.alterado_em
BEGIN
    UPDATE entidades SET alterado_em = strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime') WHERE id = NEW.id;
END;

-- Servidor da API: desligado por omissão.
INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('api.ativo', '0'),
    ('api.porta', '7800'),
    ('api.rede', '1');

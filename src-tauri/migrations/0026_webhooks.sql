-- Avisos automáticos da API (webhooks): o KanGest avisa outros sistemas (loja online, ERP…) por POST assinado
-- (HMAC-SHA256) quando há documentos emitidos ou anulados e mudanças de stock ou de artigos.

CREATE TABLE api_webhooks (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    url TEXT NOT NULL,
    eventos TEXT NOT NULL DEFAULT '[]',       -- JSON ["documento.emitido", "stock.alterado", …]
    segredo TEXT NOT NULL,                    -- chave da assinatura HMAC-SHA256
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    ultimo_envio TEXT,
    ultimo_estado INTEGER,                    -- código HTTP da última resposta (0 = sem ligação)
    falhas_seguidas INTEGER NOT NULL DEFAULT 0
);

-- Eventos por avisar. O aviso leva o estado no momento do envio (o documento, o stock, o artigo).
CREATE TABLE api_eventos (
    id INTEGER PRIMARY KEY,
    tipo TEXT NOT NULL,
    objecto_id INTEGER,
    criado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now', 'localtime')),
    distribuido INTEGER NOT NULL DEFAULT 0,   -- já tem as entregas criadas
    pendente INTEGER NOT NULL DEFAULT 1       -- ainda há entregas por fazer
);
CREATE INDEX idx_api_eventos_distr ON api_eventos(distribuido, tipo, objecto_id);
CREATE INDEX idx_api_eventos_pend ON api_eventos(pendente, criado_em);

-- Uma entrega por evento e webhook, com novas tentativas em caso de falha.
CREATE TABLE api_entregas (
    id INTEGER PRIMARY KEY,
    webhook_id INTEGER NOT NULL REFERENCES api_webhooks(id) ON DELETE CASCADE,
    evento_id INTEGER NOT NULL REFERENCES api_eventos(id) ON DELETE CASCADE,
    estado TEXT NOT NULL DEFAULT 'PENDENTE',  -- PENDENTE | ENTREGUE | FALHADA
    tentativas INTEGER NOT NULL DEFAULT 0,
    proxima_em TEXT,
    codigo_http INTEGER,
    erro TEXT,
    entregue_em TEXT,
    UNIQUE (webhook_id, evento_id)
);
CREATE INDEX idx_api_entregas_estado ON api_entregas(estado, proxima_em);

-- Gatilhos: só registam eventos quando há webhooks activos (sem eles, nada muda no funcionamento).
CREATE TRIGGER trg_evento_documento AFTER INSERT ON documentos
WHEN NEW.importado = 0 AND EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('documento.emitido', NEW.id);
END;

CREATE TRIGGER trg_evento_anulacao AFTER UPDATE OF estado ON documentos
WHEN NEW.estado = 'A' AND OLD.estado <> 'A' AND EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('documento.anulado', NEW.id);
END;

-- stock e artigos: um evento por artigo enquanto o anterior não foi distribuído (vários movimentos, um aviso)
CREATE TRIGGER trg_evento_stock AFTER INSERT ON movimentos_stock
WHEN EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
  AND NOT EXISTS (SELECT 1 FROM api_eventos WHERE tipo = 'stock.alterado' AND objecto_id = NEW.artigo_id AND distribuido = 0)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('stock.alterado', NEW.artigo_id);
END;

CREATE TRIGGER trg_evento_artigo_novo AFTER INSERT ON artigos
WHEN EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
  AND NOT EXISTS (SELECT 1 FROM api_eventos WHERE tipo = 'artigo.alterado' AND objecto_id = NEW.id AND distribuido = 0)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('artigo.alterado', NEW.id);
END;

CREATE TRIGGER trg_evento_artigo AFTER UPDATE ON artigos
WHEN EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
  AND NOT EXISTS (SELECT 1 FROM api_eventos WHERE tipo = 'artigo.alterado' AND objecto_id = NEW.id AND distribuido = 0)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('artigo.alterado', NEW.id);
END;

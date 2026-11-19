-- Avisos de stock também quando muda o que está reservado: uma nota de encomenda (NE) emitida, anulada ou facturada
-- muda o disponível (existência − reservado) sem mexer na existência. A loja online vende o disponível, por isso
-- precisa do aviso (o aviso leva o reservado e o disponível de cada armazém).
CREATE TRIGGER trg_evento_reserva_linha AFTER INSERT ON documento_linhas
WHEN NEW.artigo_id IS NOT NULL
  AND EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
  AND (SELECT tipo_codigo FROM documentos WHERE id = NEW.documento_id) = 'NE'
  AND NOT EXISTS (SELECT 1 FROM api_eventos WHERE tipo = 'stock.alterado' AND objecto_id = NEW.artigo_id AND distribuido = 0)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id) VALUES ('stock.alterado', NEW.artigo_id);
END;

CREATE TRIGGER trg_evento_reserva_estado AFTER UPDATE OF estado ON documentos
WHEN NEW.tipo_codigo = 'NE' AND NEW.estado <> OLD.estado AND EXISTS (SELECT 1 FROM api_webhooks WHERE ativo = 1)
BEGIN
    INSERT INTO api_eventos (tipo, objecto_id)
    SELECT DISTINCT 'stock.alterado', l.artigo_id FROM documento_linhas l
    WHERE l.documento_id = NEW.id AND l.artigo_id IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM api_eventos e WHERE e.tipo = 'stock.alterado' AND e.objecto_id = l.artigo_id AND e.distribuido = 0);
END;

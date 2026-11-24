-- Fecho de períodos (decisão do dono, 2026-11-24): o administrador fecha um mês que já terminou e, a partir daí, nada
-- com data desse mês se anula nem se lança (documentos, movimentos de tesouraria, compras com a data do fornecedor,
-- regularizações de conta corrente). Os serviços verificam e explicam (services/periodos.rs); estes gatilhos são a
-- segunda barreira, para qualquer caminho que escape aos serviços. Reabrir apaga a linha; fechar e reabrir ficam na
-- auditoria, com o motivo.
CREATE TABLE periodos_fechados (
    mes TEXT PRIMARY KEY CHECK (mes GLOB '[0-9][0-9][0-9][0-9]-[0-1][0-9]'),
    descricao TEXT NOT NULL,
    fechado_em TEXT NOT NULL,
    utilizador_id INTEGER REFERENCES utilizadores(id)
);

CREATE TRIGGER trg_periodo_fechado_documento
BEFORE UPDATE OF estado ON documentos
WHEN NEW.estado = 'A' AND OLD.estado <> 'A'
  AND EXISTS (SELECT 1 FROM periodos_fechados WHERE mes = substr(OLD.data_emissao, 1, 7))
BEGIN
    SELECT RAISE(ABORT, 'O mês deste documento está fechado: corrija-o com uma nota de crédito.');
END;

CREATE TRIGGER trg_periodo_fechado_movimento
BEFORE UPDATE OF anulado ON movimentos_tesouraria
WHEN NEW.anulado = 1 AND OLD.anulado = 0
  AND EXISTS (SELECT 1 FROM periodos_fechados WHERE mes = substr(OLD.data, 1, 7))
BEGIN
    SELECT RAISE(ABORT, 'O mês deste movimento está fechado.');
END;

CREATE TRIGGER trg_periodo_fechado_regularizacao
BEFORE UPDATE OF anulado ON cc_regularizacoes
WHEN NEW.anulado = 1 AND OLD.anulado = 0
  AND EXISTS (SELECT 1 FROM periodos_fechados WHERE mes = substr(OLD.data, 1, 7))
BEGIN
    SELECT RAISE(ABORT, 'O mês desta regularização está fechado.');
END;

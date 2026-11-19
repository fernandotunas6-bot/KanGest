-- Auditoria séria: o que mudou em cada ficha (antes e depois), de que posto e por que via, e um selo em cadeia que
-- denuncia linhas alteradas ou apagadas fora do KanGest (src/auditoria.rs).
ALTER TABLE auditoria ADD COLUMN entidade TEXT;     -- tabela da ficha: artigos, entidades, utilizadores, config, ...
ALTER TABLE auditoria ADD COLUMN registo_id TEXT;   -- chave da ficha (id ou código)
ALTER TABLE auditoria ADD COLUMN antes TEXT;        -- JSON: os campos como estavam (numa alteração, só os que mudaram)
ALTER TABLE auditoria ADD COLUMN depois TEXT;       -- JSON: os campos como ficaram
ALTER TABLE auditoria ADD COLUMN posto_id INTEGER;
ALTER TABLE auditoria ADD COLUMN origem TEXT;       -- local, rede, internet ou api
ALTER TABLE auditoria ADD COLUMN selo TEXT;         -- SHA-256 do selo anterior e desta linha (escreve-se uma vez)

CREATE INDEX IF NOT EXISTS idx_auditoria_ficha ON auditoria(entidade, registo_id);
CREATE INDEX IF NOT EXISTS idx_auditoria_utilizador ON auditoria(utilizador_id);
CREATE INDEX IF NOT EXISTS idx_auditoria_acao ON auditoria(acao);

-- O registo só cresce: não se apagam linhas e, de cada linha, só se escreve o selo, e uma vez.
CREATE TRIGGER auditoria_nao_se_apaga BEFORE DELETE ON auditoria
BEGIN
    SELECT RAISE(ABORT, 'O registo de auditoria não se apaga.');
END;

CREATE TRIGGER auditoria_nao_se_altera BEFORE UPDATE ON auditoria
WHEN OLD.selo IS NOT NULL
  OR NEW.id IS NOT OLD.id
  OR NEW.data_hora IS NOT OLD.data_hora
  OR NEW.utilizador_id IS NOT OLD.utilizador_id
  OR NEW.acao IS NOT OLD.acao
  OR NEW.detalhe IS NOT OLD.detalhe
  OR NEW.entidade IS NOT OLD.entidade
  OR NEW.registo_id IS NOT OLD.registo_id
  OR NEW.antes IS NOT OLD.antes
  OR NEW.depois IS NOT OLD.depois
  OR NEW.posto_id IS NOT OLD.posto_id
  OR NEW.origem IS NOT OLD.origem
BEGIN
    SELECT RAISE(ABORT, 'O registo de auditoria não se altera.');
END;

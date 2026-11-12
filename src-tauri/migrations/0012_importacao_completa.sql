-- Importação completa do KIGEST: origem dos registos de tesouraria e de caixa (reimportar não duplica).
ALTER TABLE contas ADD COLUMN id_origem TEXT;                 -- "FC:<ID>" (tbFluxoCaixaContas) / "BC:<Id>" (tbBancosContas)
ALTER TABLE movimentos_tesouraria ADD COLUMN id_origem TEXT;  -- movimento do fluxo de caixa do KIGEST
ALTER TABLE caixa_sessoes ADD COLUMN id_origem TEXT;          -- tbCaixaDiaria.Referencia
ALTER TABLE rh_processamentos ADD COLUMN importado INTEGER NOT NULL DEFAULT 0;
ALTER TABLE rh_honorarios ADD COLUMN importado INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_mov_tes_origem ON movimentos_tesouraria(id_origem);

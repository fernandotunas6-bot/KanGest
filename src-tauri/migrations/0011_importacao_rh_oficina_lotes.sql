-- Importação do KIGEST: funcionários, viaturas e ordens de serviço, lotes com saldo inicial.
ALTER TABLE funcionarios ADD COLUMN id_origem TEXT;   -- Id em tbRhFuncionarios
ALTER TABLE of_viaturas ADD COLUMN id_origem TEXT;    -- IdViatura no KIGEST
-- Saldo do lote na importação (não vem de nenhum documento); o recálculo soma-o aos movimentos.
ALTER TABLE lotes ADD COLUMN saldo_inicial REAL NOT NULL DEFAULT 0;
CREATE INDEX idx_func_origem ON funcionarios(id_origem);
CREATE INDEX idx_viat_origem ON of_viaturas(id_origem);

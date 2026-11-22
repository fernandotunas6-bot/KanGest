-- Equipa gerida no site (conta da empresa em kangest.pages.dev/conta): o identificador de cada funcionário, loja e posto
-- no site. NULL = só existe no programa (o site não lhe mexe).
ALTER TABLE utilizadores ADD COLUMN conta_id TEXT;
ALTER TABLE armazens ADD COLUMN conta_id TEXT;
ALTER TABLE postos ADD COLUMN conta_id TEXT;

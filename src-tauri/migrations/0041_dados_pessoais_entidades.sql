-- Dados pessoais e de contacto da ficha do cliente e do fornecedor (e na ficha impressa).
ALTER TABLE entidades ADD COLUMN pessoa_contacto TEXT;
ALTER TABLE entidades ADD COLUMN telefone2 TEXT;
ALTER TABLE entidades ADD COLUMN data_nascimento TEXT;      -- AAAA-MM-DD
ALTER TABLE entidades ADD COLUMN sexo TEXT;                 -- M | F
ALTER TABLE entidades ADD COLUMN documento_identificacao TEXT;  -- n.º do BI, do passaporte ou do cartão de residente
ALTER TABLE entidades ADD COLUMN nacionalidade TEXT;
ALTER TABLE entidades ADD COLUMN profissao TEXT;

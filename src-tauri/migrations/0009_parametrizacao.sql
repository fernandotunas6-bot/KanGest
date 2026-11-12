-- Parametrização de documentos (Fr_Parametrizacao), Opções de Venda (Fr_ConfigContas)
-- e Definição de Faturas e Recibos (opções do impresso).

-- ------------------------------------------------------------------ tipos de documento
ALTER TABLE tipos_documento ADD COLUMN validade_dias INTEGER NOT NULL DEFAULT 0;   -- proformas/orçamentos: válido por N dias
ALTER TABLE tipos_documento ADD COLUMN repetir_linhas INTEGER NOT NULL DEFAULT 1;  -- «Duplicar Linha»: 0 = soma a quantidade
ALTER TABLE tipos_documento ADD COLUMN prefixo_extenso TEXT;                       -- texto antes do valor por extenso
ALTER TABLE tipos_documento ADD COLUMN texto_rodape TEXT;                          -- texto próprio do tipo no impresso
ALTER TABLE tipos_documento ADD COLUMN formato TEXT NOT NULL DEFAULT 'A4';         -- A4 | A5

UPDATE tipos_documento SET repetir_linhas = COALESCE((SELECT CAST(valor AS INTEGER) FROM config WHERE chave = 'vendas.repetir_linhas'), 1) WHERE grupo = 'VENDAS';
UPDATE tipos_documento SET repetir_linhas = COALESCE((SELECT CAST(valor AS INTEGER) FROM config WHERE chave = 'compras.repetir_linhas'), 1) WHERE grupo = 'COMPRAS';

-- ------------------------------------------------------------------ opções do impresso
INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('doc.mostrar_bancos', '1'),
    ('doc.valor_extenso', '0'),
    ('doc.assinaturas', '0'),
    ('doc.mostrar_operador', '1');

-- Vários códigos de barras por artigo (ex.: a embalagem de 12 com código próprio vende 12 unidades)
-- e etiquetas de balança (EAN-13 com o código do artigo e o peso ou o preço).
CREATE TABLE artigo_codigos (
    id INTEGER PRIMARY KEY,
    artigo_id INTEGER NOT NULL REFERENCES artigos(id) ON DELETE CASCADE,
    codigo TEXT NOT NULL UNIQUE,
    descricao TEXT,
    quantidade REAL NOT NULL DEFAULT 1        -- unidades que este código vende (embalagem)
);
CREATE INDEX idx_artigo_codigos_artigo ON artigo_codigos(artigo_id);

-- código do artigo na balança (PLU) que vem dentro das etiquetas EAN-13
ALTER TABLE artigos ADD COLUMN codigo_balanca TEXT;
CREATE INDEX idx_artigos_balanca ON artigos(codigo_balanca);

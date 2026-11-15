-- Vários códigos de barras por artigo (embalagens) e artigos a peso (etiquetas da balança).

-- Códigos extra de um artigo: outra embalagem (caixa de 12 → quantidade 12) ou outro código do mesmo artigo (1).
CREATE TABLE artigo_codigos (
    id INTEGER PRIMARY KEY,
    artigo_id INTEGER NOT NULL REFERENCES artigos(id) ON DELETE CASCADE,
    codigo TEXT NOT NULL UNIQUE,
    quantidade REAL NOT NULL DEFAULT 1,
    descricao TEXT
);
CREATE INDEX idx_artigo_codigos_artigo ON artigo_codigos(artigo_id);

-- Código do artigo na balança (PLU): as etiquetas EAN-13 da balança trazem-no com o peso ou o preço.
ALTER TABLE artigos ADD COLUMN codigo_balanca TEXT;
CREATE INDEX idx_artigos_balanca ON artigos(codigo_balanca);

-- Formato das etiquetas: 2 algarismos de prefixo + código do artigo + valor + dígito de controlo (13 ao todo).
INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('balanca.ativo', '0'),
    ('balanca.prefixos', '20,21,22,23,24,25,26,27,28,29'),
    ('balanca.digitos_codigo', '5'),
    ('balanca.tipo', 'PESO'),      -- PESO (o valor é o peso) | PRECO (o valor é o preço a pagar)
    ('balanca.decimais', '3');     -- casas decimais do valor: peso em gramas → 3; preço em Kz → 0; em cêntimos → 2

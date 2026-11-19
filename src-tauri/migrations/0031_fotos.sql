-- Fotografias dos artigos: uma por artigo, em duas medidas (miniatura para a grelha do POS e imagem para a ficha e a
-- API). Ficam numa tabela à parte para as listas de artigos continuarem leves.
CREATE TABLE artigo_fotos (
    artigo_id INTEGER PRIMARY KEY REFERENCES artigos(id) ON DELETE CASCADE,
    miniatura TEXT NOT NULL,      -- data URL (webp/jpeg), ~160 px
    imagem TEXT NOT NULL,         -- data URL (webp/jpeg), até 800 px
    alterado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now', 'localtime'))
);

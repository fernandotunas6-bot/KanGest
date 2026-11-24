-- Motivos de isenção conforme a tabela de códigos do SAF-T (AO) da AGT. O texto vai para a factura e para o SAF-T
-- (TaxExemptionReason).
--
-- 1. Três textos citavam o artigo errado do CIVA (vinham de uma versão antiga da tabela). O código é o mesmo, só o
--    texto muda, e os documentos guardam o código, por isso as reimpressões e o SAF-T passam a citar o artigo certo:
--      M20 · alínea k) do n.º 1 do artigo 12.º (produtos petrolíferos);
--      M80 · alínea a) do n.º 1 do artigo 14.º (importação de bens cuja transmissão interna é isenta);
--      M91 · alínea b) do n.º 1 do artigo 16.º (bens para zonas francas, entrepostos e lojas francas).
-- 2. Os códigos que faltavam acrescentam-se (exportações M31–M38, importações M81–M86, regimes aduaneiros M90 e
--    M92–M94). Os que já existem ficam como estão.
UPDATE motivos_isencao SET descricao = 'Isento nos termos da alínea k) do nº1 do artigo 12.º do CIVA' WHERE codigo = 'M20';
UPDATE motivos_isencao SET descricao = 'Isento nos termos da alínea a) do nº1 do artigo 14.º do CIVA' WHERE codigo = 'M80';
UPDATE motivos_isencao SET descricao = 'Isento nos termos da alínea b) do nº1 do artigo 16.º do CIVA' WHERE codigo = 'M91';

INSERT OR IGNORE INTO motivos_isencao (codigo, descricao) VALUES
    ('M31', 'Isento nos termos da alínea b) do artigo 15.º do CIVA'),
    ('M32', 'Isento nos termos da alínea c) do artigo 15.º do CIVA'),
    ('M33', 'Isento nos termos da alínea d) do artigo 15.º do CIVA'),
    ('M34', 'Isento nos termos da alínea e) do artigo 15.º do CIVA'),
    ('M35', 'Isento nos termos da alínea f) do artigo 15.º do CIVA'),
    ('M36', 'Isento nos termos da alínea g) do artigo 15.º do CIVA'),
    ('M37', 'Isento nos termos da alínea h) do artigo 15.º do CIVA'),
    ('M38', 'Isento nos termos da alínea i) do artigo 15.º do CIVA'),
    ('M81', 'Isento nos termos da alínea b) do nº1 do artigo 14.º do CIVA'),
    ('M82', 'Isento nos termos da alínea c) do nº1 do artigo 14.º do CIVA'),
    ('M83', 'Isento nos termos da alínea d) do nº1 do artigo 14.º do CIVA'),
    ('M84', 'Isento nos termos da alínea e) do nº1 do artigo 14.º do CIVA'),
    ('M85', 'Isento nos termos da alínea a) do nº2 do artigo 14.º do CIVA'),
    ('M86', 'Isento nos termos da alínea b) do nº2 do artigo 14.º do CIVA'),
    ('M90', 'Isento nos termos da alínea a) do nº1 do artigo 16.º do CIVA'),
    ('M92', 'Isento nos termos da alínea c) do nº1 do artigo 16.º do CIVA'),
    ('M93', 'Isento nos termos da alínea d) do nº1 do artigo 16.º do CIVA'),
    ('M94', 'Isento nos termos da alínea e) do nº1 do artigo 16.º do CIVA');

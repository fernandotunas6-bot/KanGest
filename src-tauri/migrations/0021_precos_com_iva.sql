-- Preços com IVA incluído (retalho): o PVP com IVA de cada nível passa a ser guardado. Com a opção
-- «vendas.precos_com_iva» ligada, as vendas calculam-se a partir dele: 3 × 1 000,00 Kz dá 3 000,00 Kz
-- (e não 2 999,99 Kz, como acontecia ao partir do preço sem IVA arredondado ao cêntimo).
ALTER TABLE artigos ADD COLUMN pvp1 INTEGER;
ALTER TABLE artigos ADD COLUMN pvp2 INTEGER;
ALTER TABLE artigos ADD COLUMN pvp3 INTEGER;
ALTER TABLE artigos ADD COLUMN pvp4 INTEGER;
UPDATE artigos SET
    pvp1 = CAST(ROUND(preco1 * (100 + COALESCE((SELECT percentagem FROM taxas_iva t WHERE t.id = artigos.taxa_iva_id), 0)) / 100.0) AS INTEGER),
    pvp2 = CAST(ROUND(preco2 * (100 + COALESCE((SELECT percentagem FROM taxas_iva t WHERE t.id = artigos.taxa_iva_id), 0)) / 100.0) AS INTEGER),
    pvp3 = CAST(ROUND(preco3 * (100 + COALESCE((SELECT percentagem FROM taxas_iva t WHERE t.id = artigos.taxa_iva_id), 0)) / 100.0) AS INTEGER),
    pvp4 = CAST(ROUND(preco4 * (100 + COALESCE((SELECT percentagem FROM taxas_iva t WHERE t.id = artigos.taxa_iva_id), 0)) / 100.0) AS INTEGER);

-- preço unitário com IVA de cada linha vendida nesse modo (as notas de crédito partem dele)
ALTER TABLE documento_linhas ADD COLUMN pvp_unitario INTEGER;

-- Promoção do artigo, como no KIGEST (tbArtigos.Promocao, PromoData_ini, PromoData_Fim, Desconto):
-- entre as datas, a venda aplica este desconto automaticamente e sem pedir autorização.
ALTER TABLE artigos ADD COLUMN promo_desconto REAL NOT NULL DEFAULT 0;   -- % (0 = sem promoção)
ALTER TABLE artigos ADD COLUMN promo_inicio TEXT;                         -- AAAA-MM-DD (vazio = desde já)
ALTER TABLE artigos ADD COLUMN promo_fim TEXT;                            -- AAAA-MM-DD (vazio = sem fim)

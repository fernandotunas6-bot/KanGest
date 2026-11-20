-- Moedas aceites em numerário no POS (pagamento noutra moeda, troco em kwanzas). Só aparecem no pagamento do POS as
-- escolhidas na instalação ou em Tabelas → Moedas; numa base que já existe fica o dólar, se tiver câmbio.
ALTER TABLE moedas ADD COLUMN pos INTEGER NOT NULL DEFAULT 0;
UPDATE moedas SET pos = 1 WHERE codigo = 'USD' AND ativo = 1 AND cambio > 1;

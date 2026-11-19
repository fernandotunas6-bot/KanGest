-- Pagamento em moeda estrangeira no POS (ex.: cliente paga em USD, troco em Kz). O valor da linha continua em Kz (é o
-- que conta para o documento e para o SAF-T, com o meio de pagamento NU); guarda-se também o que se recebeu na moeda e
-- o câmbio usado, para o talão e para a conferência da gaveta por moeda.
ALTER TABLE documento_pagamentos ADD COLUMN moeda TEXT;            -- vazio = kwanzas
ALTER TABLE documento_pagamentos ADD COLUMN valor_moeda INTEGER;   -- recebido na moeda (cêntimos)
ALTER TABLE documento_pagamentos ADD COLUMN cambio REAL;           -- Kz por 1 unidade da moeda

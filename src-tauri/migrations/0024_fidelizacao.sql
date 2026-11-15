-- Fidelização de clientes: pontos ganhos nas compras e usados como meio de pagamento.
-- No SAF-T (AO) o pagamento com pontos é o mecanismo «DE» (dinheiro electrónico: cartões de fidelidade ou de pontos).
INSERT INTO modos_pagamento (codigo, descricao, ordem) VALUES ('DE', 'Pontos de fidelização', 8);

CREATE TABLE fidelizacao_movimentos (
    id INTEGER PRIMARY KEY,
    entidade_id INTEGER NOT NULL REFERENCES entidades(id),
    documento_id INTEGER REFERENCES documentos(id),   -- vazio nos ajustes manuais
    data_hora TEXT NOT NULL,
    tipo TEXT NOT NULL,          -- GANHO | RESGATE | ESTORNO (nota de crédito) | DEVOLUCAO (pontos devolvidos por nota de crédito) | ANULACAO | AJUSTE
    pontos INTEGER NOT NULL,     -- positivos entram no saldo, negativos saem
    valor INTEGER NOT NULL DEFAULT 0,  -- cêntimos: a compra que deu os pontos ou o valor pago com eles
    descricao TEXT,
    utilizador_id INTEGER REFERENCES utilizadores(id)
);
CREATE INDEX idx_fidelizacao_entidade ON fidelizacao_movimentos(entidade_id);
CREATE INDEX idx_fidelizacao_documento ON fidelizacao_movimentos(documento_id);

-- O pagamento com pontos não movimenta dinheiro (como a compensação de saldo)
DROP VIEW IF EXISTS vw_fluxo;
CREATE VIEW vw_fluxo AS
SELECT 'D' || p.id AS chave, d.data_emissao AS data, d.data_hora AS data_hora, m.conta_id AS conta_id,
       CASE WHEN t.grupo = 'COMPRAS' THEN 'SAIDA' ELSE 'ENTRADA' END AS tipo,
       p.valor AS valor, d.numero_doc AS documento, d.entidade_nome AS descricao, m.descricao AS rubrica,
       'DOCUMENTO' AS origem, d.id AS origem_id, 0 AS anulado
FROM documento_pagamentos p
JOIN documentos d ON d.id = p.documento_id
JOIN tipos_documento t ON t.codigo = d.tipo_codigo
JOIN modos_pagamento m ON m.codigo = p.modo_codigo
WHERE d.estado <> 'A' AND d.importado = 0 AND p.modo_codigo NOT IN ('CS', 'DE')
UNION ALL
SELECT 'C' || cm.id, substr(cm.data_hora, 1, 10), cm.data_hora, COALESCE(m.conta_id, 1), cm.tipo, cm.valor,
       NULL, cm.descricao, COALESCE(cm.rubrica, 'Caixa POS'), 'CAIXA', cm.id, 0
FROM caixa_movimentos cm
LEFT JOIN modos_pagamento m ON m.codigo = COALESCE(cm.modo_codigo, 'NU')
UNION ALL
SELECT 'T' || mt.id, mt.data, mt.data_hora, mt.conta_id, mt.tipo, mt.valor,
       mt.documento, mt.descricao, cc.descricao, 'TESOURARIA', mt.id, mt.anulado
FROM movimentos_tesouraria mt
LEFT JOIN centros_custo cc ON cc.id = mt.centro_custo_id;

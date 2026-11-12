-- Importação de dados do KIGEST (SQL Server): marcas de origem e histórico.

ALTER TABLE documentos ADD COLUMN importado INTEGER NOT NULL DEFAULT 0;   -- 1 = histórico do KIGEST (só leitura)
ALTER TABLE artigos ADD COLUMN id_origem TEXT;                              -- IdArtigo no KIGEST
ALTER TABLE entidades ADD COLUMN id_origem TEXT;                            -- "C:12" / "F:3" no KIGEST
ALTER TABLE utilizadores ADD COLUMN id_origem TEXT;

CREATE TABLE importacoes (
    id INTEGER PRIMARY KEY,
    data_hora TEXT NOT NULL,
    origem TEXT NOT NULL,          -- servidor/base
    relatorio TEXT NOT NULL        -- JSON com contagens e avisos
);

-- Os pagamentos dos documentos históricos não entram no fluxo de caixa do sistema novo
-- (o dinheiro desses dias já foi tratado no KIGEST).
DROP VIEW vw_fluxo;
CREATE VIEW vw_fluxo AS
SELECT 'D' || p.id AS chave, d.data_emissao AS data, d.data_hora AS data_hora, m.conta_id AS conta_id,
       CASE WHEN t.grupo = 'COMPRAS' THEN 'SAIDA' ELSE 'ENTRADA' END AS tipo,
       p.valor AS valor, d.numero_doc AS documento, d.entidade_nome AS descricao, m.descricao AS rubrica,
       'DOCUMENTO' AS origem, d.id AS origem_id, 0 AS anulado
FROM documento_pagamentos p
JOIN documentos d ON d.id = p.documento_id
JOIN tipos_documento t ON t.codigo = d.tipo_codigo
JOIN modos_pagamento m ON m.codigo = p.modo_codigo
WHERE d.estado <> 'A' AND d.importado = 0
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

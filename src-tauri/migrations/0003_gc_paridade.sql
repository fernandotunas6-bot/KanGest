-- Paridade com o KIGEST GC: todos os tipos de documento (vendas, compras, stock),
-- campos do formulário de documento, retenção na fonte, prazos de pagamento e tesouraria
-- (contas, centros de custo, movimentos, fluxo de caixa).

-- ---------------------------------------------------------------- tipos de documento
ALTER TABLE tipos_documento ADD COLUMN entidade TEXT;                       -- 'C' cliente | 'F' fornecedor | NULL (interno)
ALTER TABLE tipos_documento ADD COLUMN credito INTEGER NOT NULL DEFAULT 0;  -- nota de crédito: exige documento de origem
ALTER TABLE tipos_documento ADD COLUMN externo INTEGER NOT NULL DEFAULT 0;  -- documento do fornecedor (tem n.º externo)
ALTER TABLE tipos_documento ADD COLUMN grupo TEXT NOT NULL DEFAULT 'VENDAS'; -- VENDAS | COMPRAS | STOCK
ALTER TABLE tipos_documento ADD COLUMN ordem INTEGER NOT NULL DEFAULT 0;

UPDATE tipos_documento SET entidade = 'C', grupo = 'VENDAS' WHERE codigo IN ('FR','FT','NC','ND','RC','PP','OR','GT');
UPDATE tipos_documento SET credito = 1 WHERE codigo = 'NC';
UPDATE tipos_documento SET descricao = 'V/Factura', entidade = 'F', grupo = 'COMPRAS', externo = 1, conta_corrente = 1 WHERE codigo = 'FC';
UPDATE tipos_documento SET grupo = 'STOCK' WHERE codigo IN ('ES','SS','TS');
UPDATE tipos_documento SET descricao = 'Documento de Stock (Entrada)' WHERE codigo = 'ES';

INSERT INTO tipos_documento (codigo, descricao, classe, stock, conta_corrente, pago_no_acto, assinado, vias, entidade, credito, externo, grupo) VALUES
    ('NE',  'Nota de Encomenda',                 'PROFORMA',   0,  0, 0, 1, 1, 'C', 0, 0, 'VENDAS'),
    ('AF',  'Factura/Recibo (Autofacturação)',   'COMPRA',     1,  0, 1, 1, 2, 'F', 0, 0, 'COMPRAS'),
    ('VFR', 'V/Factura/Recibo',                  'COMPRA',     1,  0, 1, 0, 1, 'F', 0, 1, 'COMPRAS'),
    ('VNC', 'V/Nota de Crédito',                 'COMPRA',    -1, -1, 0, 0, 1, 'F', 1, 1, 'COMPRAS'),
    ('VND', 'V/Nota de Débito',                  'COMPRA',     0,  1, 0, 0, 1, 'F', 0, 1, 'COMPRAS'),
    ('VNE', 'Encomenda a Fornecedor',            'COMPRA_DOC', 0,  0, 0, 0, 1, 'F', 0, 0, 'COMPRAS'),
    ('VOR', 'V/Orçamento',                       'COMPRA_DOC', 0,  0, 0, 0, 1, 'F', 0, 1, 'COMPRAS'),
    ('VGT', 'V/Guia de Transporte',              'COMPRA_DOC', 0,  0, 0, 0, 1, 'F', 0, 1, 'COMPRAS'),
    ('VRC', 'Pagamento a Fornecedor',            'PAGAMENTO',  0, -1, 1, 0, 1, 'F', 0, 0, 'COMPRAS'),
    ('CI',  'Consumo Interno',                   'STOCK',     -1,  0, 0, 0, 1, NULL, 0, 0, 'STOCK'),
    ('IN',  'Inventário (Regularização)',        'STOCK',      3,  0, 0, 0, 1, NULL, 0, 0, 'STOCK');

UPDATE tipos_documento SET ordem = CASE codigo
    WHEN 'FT' THEN 1 WHEN 'FR' THEN 2 WHEN 'PP' THEN 3 WHEN 'NC' THEN 4 WHEN 'ND' THEN 5 WHEN 'NE' THEN 6
    WHEN 'OR' THEN 7 WHEN 'GT' THEN 8 WHEN 'RC' THEN 9 WHEN 'AF' THEN 10
    WHEN 'FC' THEN 20 WHEN 'VFR' THEN 21 WHEN 'VNC' THEN 22 WHEN 'VND' THEN 23 WHEN 'VNE' THEN 24
    WHEN 'VOR' THEN 25 WHEN 'VGT' THEN 26 WHEN 'VRC' THEN 27
    WHEN 'ES' THEN 40 WHEN 'SS' THEN 41 WHEN 'TS' THEN 42 WHEN 'CI' THEN 43 WHEN 'IN' THEN 44 ELSE 99 END;

-- ---------------------------------------------------------------- campos do documento
ALTER TABLE documentos ADD COLUMN doc_externo TEXT;          -- n.º do documento do fornecedor (compras)
ALTER TABLE documentos ADD COLUMN motivo TEXT;               -- motivo de emissão (NC / ND)
ALTER TABLE documentos ADD COLUMN referencia TEXT;           -- V/ Referência (requisição, obra…)
ALTER TABLE documentos ADD COLUMN prazo TEXT;                -- prazo de pagamento
ALTER TABLE documentos ADD COLUMN transporte TEXT;           -- JSON: carga/descarga, viatura, motorista (guias)
ALTER TABLE documentos ADD COLUMN entidade_localidade TEXT;

-- Prazos de pagamento (tbzconfigTiposPagamento)
CREATE TABLE prazos_pagamento (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL,
    dias INTEGER NOT NULL DEFAULT 0
);
INSERT INTO prazos_pagamento (codigo, descricao, dias) VALUES
    ('PP', 'Pronto pagamento', 0), ('15', '15 dias', 15), ('30', '30 dias', 30),
    ('45', '45 dias', 45), ('60', '60 dias', 60), ('90', '90 dias', 90);

-- Retenção na fonte (ZconfigRetencao): taxas por omissão — confirmar as taxas em vigor.
CREATE TABLE retencoes (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL,
    percentagem REAL NOT NULL,
    base TEXT NOT NULL DEFAULT 'LIQUIDO',   -- LIQUIDO (base tributável) | IVA (IVA cativo)
    ativo INTEGER NOT NULL DEFAULT 1
);
INSERT INTO retencoes (codigo, descricao, percentagem, base) VALUES
    ('II',  'Imposto Industrial',                       6.5, 'LIQUIDO'),
    ('IRT', 'Imposto sobre o Rendimento do Trabalho',   6.5, 'LIQUIDO'),
    ('IS',  'Imposto de Selo',                          1,   'LIQUIDO'),
    ('IPU', 'Imposto Predial Urbano',                   15,  'LIQUIDO'),
    ('IAC', 'Imposto sobre a Aplicação de Capitais',    10,  'LIQUIDO'),
    ('IVA', 'IVA Cativo',                               50,  'IVA');

CREATE TABLE documento_retencoes (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL REFERENCES documentos(id) ON DELETE CASCADE,
    codigo TEXT NOT NULL,
    descricao TEXT NOT NULL,
    percentagem REAL NOT NULL,
    base INTEGER NOT NULL,
    valor INTEGER NOT NULL
);

-- ---------------------------------------------------------------- tesouraria
CREATE TABLE contas (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL,
    tipo TEXT NOT NULL DEFAULT 'CAIXA',      -- CAIXA | BANCO
    banco TEXT,
    agencia TEXT,
    numero TEXT,
    iban TEXT,
    saldo_inicial INTEGER NOT NULL DEFAULT 0,
    ativo INTEGER NOT NULL DEFAULT 1
);
INSERT INTO contas (id, descricao, tipo) VALUES (1, 'Caixa Principal', 'CAIXA'), (2, 'Banco', 'BANCO');

-- Em que conta entra/sai o dinheiro de cada modo de pagamento
ALTER TABLE modos_pagamento ADD COLUMN conta_id INTEGER NOT NULL DEFAULT 1;
UPDATE modos_pagamento SET conta_id = 2 WHERE codigo IN ('MB', 'TB', 'CC', 'CH');

CREATE TABLE centros_custo (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL,
    tipo TEXT NOT NULL DEFAULT 'DESPESA',    -- DESPESA | RECEITA
    ativo INTEGER NOT NULL DEFAULT 1
);
INSERT INTO centros_custo (descricao, tipo) VALUES
    ('Água e electricidade', 'DESPESA'), ('Combustível', 'DESPESA'), ('Salários', 'DESPESA'),
    ('Renda', 'DESPESA'), ('Comunicações', 'DESPESA'), ('Material de escritório', 'DESPESA'),
    ('Manutenção', 'DESPESA'), ('Impostos e taxas', 'DESPESA'), ('Outras despesas', 'DESPESA'),
    ('Reforço de caixa', 'RECEITA'), ('Outras receitas', 'RECEITA');

CREATE TABLE movimentos_tesouraria (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL,
    data_hora TEXT NOT NULL,
    conta_id INTEGER NOT NULL REFERENCES contas(id),
    tipo TEXT NOT NULL,                      -- ENTRADA | SAIDA
    valor INTEGER NOT NULL,
    descricao TEXT NOT NULL,
    centro_custo_id INTEGER REFERENCES centros_custo(id),
    entidade_id INTEGER REFERENCES entidades(id),
    documento TEXT,
    transferencia INTEGER,                   -- id partilhado pelos dois lados de uma transferência/depósito
    anulado INTEGER NOT NULL DEFAULT 0,
    motivo_anulacao TEXT,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id)
);
CREATE INDEX idx_mov_tes_data ON movimentos_tesouraria(data);

-- Fluxo de caixa = pagamentos de documentos + movimentos do caixa do POS + movimentos de tesouraria
CREATE VIEW vw_fluxo AS
SELECT 'D' || p.id AS chave, d.data_emissao AS data, d.data_hora AS data_hora, m.conta_id AS conta_id,
       CASE WHEN t.grupo = 'COMPRAS' THEN 'SAIDA' ELSE 'ENTRADA' END AS tipo,
       p.valor AS valor, d.numero_doc AS documento, d.entidade_nome AS descricao, m.descricao AS rubrica,
       'DOCUMENTO' AS origem, d.id AS origem_id, 0 AS anulado
FROM documento_pagamentos p
JOIN documentos d ON d.id = p.documento_id
JOIN tipos_documento t ON t.codigo = d.tipo_codigo
JOIN modos_pagamento m ON m.codigo = p.modo_codigo
WHERE d.estado <> 'A'
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

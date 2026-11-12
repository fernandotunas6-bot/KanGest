-- Restante do KIGEST GC: séries de documentos, recuperação de documentos manuais, SAF-T do Estado,
-- adiantamentos de clientes e créditos em conta corrente, vendedores, endereços de entrega,
-- regularizações, trabalhadores por conta própria e prestações sociais.

-- ------------------------------------------------------------------ séries de documentos (Fr_SeriesDocumento)
ALTER TABLE series ADD COLUMN descricao TEXT;
ALTER TABLE series ADD COLUMN data_inicio TEXT;
ALTER TABLE series ADD COLUMN data_fim TEXT;
ALTER TABLE series ADD COLUMN padrao INTEGER NOT NULL DEFAULT 0;      -- série por defeito do tipo de documento
ALTER TABLE series ADD COLUMN manual INTEGER NOT NULL DEFAULT 0;      -- série de documentos manuais (recuperação)
ALTER TABLE series ADD COLUMN posto_id INTEGER REFERENCES postos(id); -- NULL = qualquer posto
ALTER TABLE series ADD COLUMN data_ultimo TEXT;                       -- data do último documento (ordem cronológica)
UPDATE series SET
    data_ultimo = (SELECT MAX(d.data_emissao) FROM documentos d WHERE d.tipo_codigo = series.tipo_codigo AND d.serie = series.serie),
    descricao = COALESCE(descricao, 'Série ' || serie);

-- ------------------------------------------------------------------ documentos
-- Origem para o SAF-T (SourceBilling / SourcePayment): P produzido na aplicação, I integrado, M manual (recuperação).
ALTER TABLE documentos ADD COLUMN origem TEXT NOT NULL DEFAULT 'P';
UPDATE documentos SET origem = 'I' WHERE importado = 1;
ALTER TABLE documentos ADD COLUMN doc_manual TEXT;                   -- n.º do documento manual recuperado
-- Crédito a favor da entidade ainda por utilizar (notas de crédito não compensadas, adiantamentos).
ALTER TABLE documentos ADD COLUMN credito_disponivel INTEGER NOT NULL DEFAULT 0;
-- Vendedor (Fr_SelecionarFuncionarioCliente) e endereço de entrega (Fr_EnderecosExtras).
ALTER TABLE documentos ADD COLUMN vendedor_id INTEGER;
ALTER TABLE documentos ADD COLUMN endereco_entrega TEXT;
CREATE INDEX idx_doc_vendedor ON documentos(vendedor_id);

-- ------------------------------------------------------------------ entidades
ALTER TABLE entidades ADD COLUMN estado INTEGER NOT NULL DEFAULT 0;  -- entidade pública (SAF-T "Faturação para o Estado")
ALTER TABLE entidades ADD COLUMN observacoes TEXT;

CREATE TABLE entidade_enderecos (
    id INTEGER PRIMARY KEY,
    entidade_id INTEGER NOT NULL REFERENCES entidades(id),
    descricao TEXT NOT NULL,
    endereco TEXT NOT NULL,
    localidade TEXT,
    provincia TEXT,
    pais TEXT NOT NULL DEFAULT 'AO',
    ativo INTEGER NOT NULL DEFAULT 1
);

-- ------------------------------------------------------------------ adiantamentos e compensação
INSERT INTO tipos_documento (codigo, descricao, classe, stock, conta_corrente, pago_no_acto, assinado, vias, entidade, credito, externo, grupo, ordem) VALUES
    ('RA', 'Recibo de Adiantamento', 'ADIANTAMENTO', 0, -1, 1, 0, 2, 'C', 0, 0, 'VENDAS', 11);
INSERT INTO modos_pagamento (codigo, descricao, ordem) VALUES ('CS', 'Compensação de saldo (adiantamento / crédito)', 9);

-- Utilização de créditos (adiantamentos, notas de crédito) num recibo
CREATE TABLE credito_utilizacoes (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL REFERENCES documentos(id),  -- quem consome (recibo)
    credito_id INTEGER NOT NULL REFERENCES documentos(id),    -- crédito consumido (RA / NC)
    valor INTEGER NOT NULL
);

-- Devolução de dinheiro de um crédito (Fr_DevolucaoDinheiro): pela tesouraria ou pelo caixa do POS
CREATE TABLE credito_devolucoes (
    id INTEGER PRIMARY KEY,
    credito_id INTEGER NOT NULL REFERENCES documentos(id),
    movimento_id INTEGER REFERENCES movimentos_tesouraria(id),
    caixa_movimento_id INTEGER REFERENCES caixa_movimentos(id),
    valor INTEGER NOT NULL,
    data TEXT NOT NULL
);

-- Regularização de conta corrente (Fr_RegularizacaoContaCorrente)
CREATE TABLE cc_regularizacoes (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL,
    data_hora TEXT NOT NULL,
    entidade_id INTEGER NOT NULL REFERENCES entidades(id),
    grupo TEXT NOT NULL DEFAULT 'VENDAS', -- VENDAS (cliente) | COMPRAS (fornecedor)
    tipo TEXT NOT NULL,                  -- CREDITO (abate dívida) | DEBITO (acerta saldo a favor)
    valor INTEGER NOT NULL,
    justificacao TEXT NOT NULL,
    anulado INTEGER NOT NULL DEFAULT 0,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id)
);
CREATE TABLE cc_regularizacao_docs (
    regularizacao_id INTEGER NOT NULL REFERENCES cc_regularizacoes(id),
    documento_id INTEGER NOT NULL REFERENCES documentos(id),
    valor INTEGER NOT NULL
);

-- Pagamentos por compensação não movimentam dinheiro
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
WHERE d.estado <> 'A' AND d.importado = 0 AND p.modo_codigo <> 'CS'
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

-- ------------------------------------------------------------------ RH: trabalhadores por conta própria (honorários)
ALTER TABLE funcionarios ADD COLUMN independente INTEGER NOT NULL DEFAULT 0;
ALTER TABLE funcionarios ADD COLUMN honorario INTEGER NOT NULL DEFAULT 0;          -- valor base (cêntimos)
ALTER TABLE funcionarios ADD COLUMN tipo_honorario TEXT NOT NULL DEFAULT 'MENSAL'; -- MENSAL | DIA | HORA
ALTER TABLE funcionarios ADD COLUMN local_trabalho TEXT;

CREATE TABLE rh_honorarios (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    data TEXT NOT NULL,
    ano INTEGER NOT NULL,
    mes INTEGER NOT NULL,
    tipo TEXT NOT NULL,                 -- MENSAL | DIA | HORA
    valor_base INTEGER NOT NULL,
    quantidade REAL NOT NULL,
    iliquido INTEGER NOT NULL,
    irt_pct REAL NOT NULL,
    irt INTEGER NOT NULL,
    liquido INTEGER NOT NULL,
    descricao TEXT,
    estado TEXT NOT NULL DEFAULT 'PROCESSADO', -- PROCESSADO | ANULADO
    motivo_anulacao TEXT,
    pago INTEGER NOT NULL DEFAULT 0,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id)
);
CREATE INDEX idx_rh_hon_periodo ON rh_honorarios(ano, mes);

-- ------------------------------------------------------------------ RH: prestações sociais (Fr_PrestacoesSociais)
CREATE TABLE rh_prestacoes (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    tipo TEXT NOT NULL,                 -- Maternidade, Doença, Acidente de trabalho…
    data_inicio TEXT NOT NULL,
    data_fim TEXT NOT NULL,
    liquido_mensal INTEGER NOT NULL DEFAULT 0,
    meses INTEGER NOT NULL DEFAULT 1,
    suspensao INTEGER NOT NULL DEFAULT 1, -- suspende o processamento do salário no período
    observacoes TEXT,
    data TEXT NOT NULL,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id)
);

INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('rh.irt_independentes', '6.5'),
    ('rh.faltas_desconta_alimentacao', '1'),
    ('rh.faltas_desconta_transporte', '1'),
    ('vendas.repetir_linhas', '1'),
    ('compras.repetir_linhas', '1');

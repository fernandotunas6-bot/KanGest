-- KiGest — esquema inicial (SQLite)
-- Valores monetários em cêntimos (INTEGER). Quantidades em REAL.
-- Modelo inspirado no KIGEST GC/POS (tbArtigos, tbDocumentosCabessalho/Detalhe,
-- tbDocsModoPagamentos, tbContasCorrente, tbzconfig*).

PRAGMA foreign_keys = ON;

CREATE TABLE config (
    chave TEXT PRIMARY KEY,
    valor TEXT NOT NULL
);

CREATE TABLE empresa (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    nome TEXT NOT NULL,
    designacao_comercial TEXT,
    nif TEXT NOT NULL,
    endereco TEXT,
    localidade TEXT,
    provincia TEXT,
    telefone TEXT,
    email TEXT,
    regime_iva TEXT NOT NULL DEFAULT 'GERAL',  -- GERAL | SIMPLIFICADO | EXCLUSAO
    moeda TEXT NOT NULL DEFAULT 'AOA',
    rodape TEXT,
    bancos TEXT                                -- JSON [{banco, conta, iban}]
);

CREATE TABLE utilizadores (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    login TEXT NOT NULL UNIQUE COLLATE NOCASE,
    senha_hash TEXT NOT NULL,
    perfil TEXT NOT NULL DEFAULT 'CAIXA',      -- ADMIN | GESTOR | CAIXA
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE TABLE postos (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL
);

CREATE TABLE armazens (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    ativo INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE familias (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL
);

CREATE TABLE subfamilias (
    id INTEGER PRIMARY KEY,
    familia_id INTEGER NOT NULL REFERENCES familias(id),
    descricao TEXT NOT NULL
);

CREATE TABLE unidades (
    id INTEGER PRIMARY KEY,
    descricao TEXT NOT NULL,
    abreviatura TEXT NOT NULL
);

-- Taxas de IVA (Angola: 14% normal, 7%/5% reduzidas, 0% isento)
CREATE TABLE taxas_iva (
    id INTEGER PRIMARY KEY,
    codigo TEXT NOT NULL,          -- NOR | INT | RED | ISE | OUT (SAF-T TaxCode)
    descricao TEXT NOT NULL,
    percentagem REAL NOT NULL,
    ativo INTEGER NOT NULL DEFAULT 1
);

-- Motivos de isenção (SAF-T AO TaxExemptionCode: M00..M99)
CREATE TABLE motivos_isencao (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL
);

CREATE TABLE artigos (
    id INTEGER PRIMARY KEY,
    referencia TEXT NOT NULL UNIQUE,
    codigo_barras TEXT,
    descricao TEXT NOT NULL,
    descricao_curta TEXT,
    tipo TEXT NOT NULL DEFAULT 'P',             -- P produto | S serviço (SAF-T ProductType)
    familia_id INTEGER REFERENCES familias(id),
    subfamilia_id INTEGER REFERENCES subfamilias(id),
    unidade_id INTEGER REFERENCES unidades(id),
    taxa_iva_id INTEGER NOT NULL REFERENCES taxas_iva(id),
    motivo_isencao TEXT REFERENCES motivos_isencao(codigo),
    custo INTEGER NOT NULL DEFAULT 0,           -- último preço de custo
    custo_medio INTEGER NOT NULL DEFAULT 0,
    preco1 INTEGER NOT NULL DEFAULT 0,          -- preços de venda SEM IVA (4 níveis como no KIGEST)
    preco2 INTEGER NOT NULL DEFAULT 0,
    preco3 INTEGER NOT NULL DEFAULT 0,
    preco4 INTEGER NOT NULL DEFAULT 0,
    qtd_minima REAL NOT NULL DEFAULT 0,
    qtd_maxima REAL NOT NULL DEFAULT 0,
    controla_stock INTEGER NOT NULL DEFAULT 1,
    permite_negativo INTEGER NOT NULL DEFAULT 0,
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX idx_artigos_cobar ON artigos(codigo_barras);
CREATE INDEX idx_artigos_desc ON artigos(descricao);

CREATE TABLE stock (
    artigo_id INTEGER NOT NULL REFERENCES artigos(id),
    armazem_id INTEGER NOT NULL REFERENCES armazens(id),
    quantidade REAL NOT NULL DEFAULT 0,
    ultima_entrada TEXT,
    ultima_saida TEXT,
    PRIMARY KEY (artigo_id, armazem_id)
);

CREATE TABLE entidades (
    id INTEGER PRIMARY KEY,
    tipo TEXT NOT NULL DEFAULT 'C',             -- C cliente | F fornecedor | A ambos
    nome TEXT NOT NULL,
    nif TEXT NOT NULL DEFAULT '999999999',
    endereco TEXT,
    localidade TEXT,
    provincia TEXT,
    pais TEXT NOT NULL DEFAULT 'AO',
    telefone TEXT,
    email TEXT,
    limite_credito INTEGER NOT NULL DEFAULT 0,
    desconto REAL NOT NULL DEFAULT 0,
    nivel_preco INTEGER NOT NULL DEFAULT 1,
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX idx_entidades_nome ON entidades(nome);

-- Modos de pagamento (SAF-T PaymentMechanism)
CREATE TABLE modos_pagamento (
    codigo TEXT PRIMARY KEY,                    -- NU, CD, CC, TB, MB, CH, OU
    descricao TEXT NOT NULL,
    ativo INTEGER NOT NULL DEFAULT 1,
    ordem INTEGER NOT NULL DEFAULT 0
);

-- Tipos de documento (equivalente a tbzconfigTiposDocumentos)
CREATE TABLE tipos_documento (
    codigo TEXT PRIMARY KEY,                    -- FT, FR, NC, ND, PP, OR, GT, RC, FC, ES, SS, TS
    descricao TEXT NOT NULL,
    classe TEXT NOT NULL,                       -- VENDA | COMPRA | STOCK | RECIBO | PROFORMA | TRANSPORTE
    stock INTEGER NOT NULL DEFAULT 0,           -- -1 saída | 0 nada | 1 entrada | 2 transferência
    conta_corrente INTEGER NOT NULL DEFAULT 0,  -- 1 débito cliente | -1 crédito | 0 nada
    pago_no_acto INTEGER NOT NULL DEFAULT 0,    -- FR/VD: recebimento imediato
    assinado INTEGER NOT NULL DEFAULT 0,        -- assinatura AGT (RSA-SHA1)
    vias INTEGER NOT NULL DEFAULT 1,
    ativo INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE series (
    id INTEGER PRIMARY KEY,
    tipo_codigo TEXT NOT NULL REFERENCES tipos_documento(codigo),
    serie TEXT NOT NULL,
    ultimo_numero INTEGER NOT NULL DEFAULT 0,
    ultimo_hash TEXT NOT NULL DEFAULT '',
    ativa INTEGER NOT NULL DEFAULT 1,
    UNIQUE (tipo_codigo, serie)
);

CREATE TABLE documentos (
    id INTEGER PRIMARY KEY,
    tipo_codigo TEXT NOT NULL REFERENCES tipos_documento(codigo),
    serie TEXT NOT NULL,
    numero INTEGER NOT NULL,
    numero_doc TEXT NOT NULL UNIQUE,            -- ex.: "FR 2026/15"
    data_emissao TEXT NOT NULL,                 -- YYYY-MM-DD
    data_hora TEXT NOT NULL,                    -- YYYY-MM-DDTHH:MM:SS (SystemEntryDate)
    data_vencimento TEXT,
    entidade_id INTEGER REFERENCES entidades(id),
    entidade_nome TEXT NOT NULL,
    entidade_nif TEXT NOT NULL,
    entidade_endereco TEXT,
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id),
    posto_id INTEGER REFERENCES postos(id),
    armazem_id INTEGER REFERENCES armazens(id),
    armazem_destino_id INTEGER REFERENCES armazens(id),
    estado TEXT NOT NULL DEFAULT 'N',           -- N normal | A anulado | F faturado (proformas)
    motivo_anulacao TEXT,
    doc_origem_id INTEGER REFERENCES documentos(id),
    total_bruto INTEGER NOT NULL DEFAULT 0,     -- soma qtd*preço (sem IVA, sem desconto)
    total_desconto INTEGER NOT NULL DEFAULT 0,
    total_liquido INTEGER NOT NULL DEFAULT 0,   -- base tributável
    total_iva INTEGER NOT NULL DEFAULT 0,
    total_retencao INTEGER NOT NULL DEFAULT 0,
    total INTEGER NOT NULL DEFAULT 0,           -- GrossTotal
    valor_entregue INTEGER NOT NULL DEFAULT 0,
    troco INTEGER NOT NULL DEFAULT 0,
    valor_pendente INTEGER NOT NULL DEFAULT 0,  -- em dívida (conta corrente)
    hash TEXT NOT NULL DEFAULT '',
    hash_controlo TEXT NOT NULL DEFAULT '',
    caixa_sessao_id INTEGER,
    observacoes TEXT,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    UNIQUE (tipo_codigo, serie, numero)
);
CREATE INDEX idx_docs_data ON documentos(data_emissao);
CREATE INDEX idx_docs_entidade ON documentos(entidade_id);

CREATE TABLE documento_linhas (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL REFERENCES documentos(id) ON DELETE CASCADE,
    linha INTEGER NOT NULL,
    artigo_id INTEGER REFERENCES artigos(id),
    referencia TEXT NOT NULL,
    descricao TEXT NOT NULL,
    quantidade REAL NOT NULL,
    unidade TEXT,
    preco_unitario INTEGER NOT NULL,            -- sem IVA
    desconto_pct REAL NOT NULL DEFAULT 0,
    valor_desconto INTEGER NOT NULL DEFAULT 0,
    taxa_iva_id INTEGER REFERENCES taxas_iva(id),
    taxa_iva_pct REAL NOT NULL DEFAULT 0,
    motivo_isencao TEXT,
    valor_liquido INTEGER NOT NULL,             -- base da linha
    valor_iva INTEGER NOT NULL,
    total INTEGER NOT NULL,
    custo INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_linhas_doc ON documento_linhas(documento_id);
CREATE INDEX idx_linhas_artigo ON documento_linhas(artigo_id);

CREATE TABLE documento_pagamentos (
    id INTEGER PRIMARY KEY,
    documento_id INTEGER NOT NULL REFERENCES documentos(id) ON DELETE CASCADE,
    modo_codigo TEXT NOT NULL REFERENCES modos_pagamento(codigo),
    valor INTEGER NOT NULL
);

-- Recibos: liquidação de documentos em dívida
CREATE TABLE recibo_documentos (
    id INTEGER PRIMARY KEY,
    recibo_id INTEGER NOT NULL REFERENCES documentos(id) ON DELETE CASCADE,
    documento_id INTEGER NOT NULL REFERENCES documentos(id),
    valor INTEGER NOT NULL
);

CREATE TABLE movimentos_stock (
    id INTEGER PRIMARY KEY,
    data_hora TEXT NOT NULL,
    artigo_id INTEGER NOT NULL REFERENCES artigos(id),
    armazem_id INTEGER NOT NULL REFERENCES armazens(id),
    quantidade REAL NOT NULL,                   -- + entrada, - saída
    saldo REAL NOT NULL,                        -- existência após o movimento
    custo INTEGER NOT NULL DEFAULT 0,
    documento_id INTEGER REFERENCES documentos(id),
    descricao TEXT
);
CREATE INDEX idx_movstock_artigo ON movimentos_stock(artigo_id, data_hora);

CREATE TABLE caixa_sessoes (
    id INTEGER PRIMARY KEY,
    posto_id INTEGER REFERENCES postos(id),
    utilizador_id INTEGER NOT NULL REFERENCES utilizadores(id),
    aberta_em TEXT NOT NULL,
    fechada_em TEXT,
    valor_inicial INTEGER NOT NULL DEFAULT 0,
    valor_contado INTEGER,
    estado TEXT NOT NULL DEFAULT 'ABERTA'       -- ABERTA | FECHADA
);

CREATE TABLE caixa_movimentos (
    id INTEGER PRIMARY KEY,
    sessao_id INTEGER NOT NULL REFERENCES caixa_sessoes(id),
    data_hora TEXT NOT NULL,
    tipo TEXT NOT NULL,                         -- ENTRADA | SAIDA
    valor INTEGER NOT NULL,
    descricao TEXT
);

CREATE TABLE auditoria (
    id INTEGER PRIMARY KEY,
    data_hora TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    utilizador_id INTEGER,
    acao TEXT NOT NULL,
    detalhe TEXT
);

-- ===================== Dados base =====================
INSERT INTO postos (id, descricao) VALUES (1, 'Posto#1');
INSERT INTO armazens (id, nome) VALUES (1, 'Principal');
INSERT INTO familias (id, descricao) VALUES (1, 'Geral');
INSERT INTO subfamilias (id, familia_id, descricao) VALUES (1, 1, 'Geral');
INSERT INTO unidades (id, descricao, abreviatura) VALUES
    (1, 'Unidade', 'UN'), (2, 'Quilograma', 'KG'), (3, 'Litro', 'LT'),
    (4, 'Caixa', 'CX'), (5, 'Metro', 'MT'), (6, 'Serviço', 'SV');

INSERT INTO taxas_iva (id, codigo, descricao, percentagem) VALUES
    (1, 'NOR', 'IVA 14%', 14), (2, 'ISE', 'Isento', 0),
    (3, 'RED', 'IVA 7%', 7), (4, 'RED', 'IVA 5%', 5), (5, 'OUT', 'IVA 1%', 1);

INSERT INTO motivos_isencao (codigo, descricao) VALUES
    ('M00', 'Regime Transitório'),
    ('M02', 'Transmissão de bens e serviço não sujeita'),
    ('M04', 'IVA – Regime de Não Sujeição'),
    ('M10', 'Isento nos termos da alínea a) do nº1 do artigo 12.º do CIVA'),
    ('M11', 'Isento nos termos da alínea b) do nº1 do artigo 12.º do CIVA'),
    ('M12', 'Isento nos termos da alínea c) do nº1 do artigo 12.º do CIVA'),
    ('M13', 'Isento nos termos da alínea d) do nº1 do artigo 12.º do CIVA'),
    ('M14', 'Isento nos termos da alínea e) do nº1 do artigo 12.º do CIVA'),
    ('M20', 'Isento nos termos da alínea a) do nº1 do artigo 14.º do CIVA'),
    ('M30', 'Isento nos termos da alínea a) do artigo 15.º do CIVA'),
    ('M80', 'Isento nos termos da alínea a) do nº1 do artigo 9.º do CIVA'),
    ('M91', 'Isento nos termos do artigo 13.º do CIVA'),
    ('M99', 'Outros motivos de isenção');

INSERT INTO modos_pagamento (codigo, descricao, ordem) VALUES
    ('NU', 'Numerário', 1), ('MB', 'Multicaixa (TPA)', 2), ('TB', 'Transferência Bancária', 3),
    ('CC', 'Cartão de Crédito', 4), ('CH', 'Cheque', 5), ('OU', 'Outros', 6);

INSERT INTO tipos_documento (codigo, descricao, classe, stock, conta_corrente, pago_no_acto, assinado, vias) VALUES
    ('FR', 'Factura/Recibo',          'VENDA',      -1,  0, 1, 1, 1),
    ('FT', 'Factura',                 'VENDA',      -1,  1, 0, 1, 2),
    ('NC', 'Nota de Crédito',         'VENDA',       1, -1, 0, 1, 2),
    ('ND', 'Nota de Débito',          'VENDA',       0,  1, 0, 1, 2),
    ('RC', 'Recibo',                  'RECIBO',      0, -1, 1, 1, 2),
    ('PP', 'Factura Proforma',        'PROFORMA',    0,  0, 0, 1, 1),
    ('OR', 'Orçamento',               'PROFORMA',    0,  0, 0, 1, 1),
    ('GT', 'Guia de Transporte',      'TRANSPORTE',  0,  0, 0, 1, 3),
    ('FC', 'Factura de Compra',       'COMPRA',      1,  0, 0, 0, 1),
    ('ES', 'Entrada de Stock',        'STOCK',       1,  0, 0, 0, 1),
    ('SS', 'Saída de Stock',          'STOCK',      -1,  0, 0, 0, 1),
    ('TS', 'Transferência de Stock',  'STOCK',       2,  0, 0, 0, 1);

INSERT INTO entidades (id, tipo, nome, nif) VALUES (1, 'C', 'Consumidor Final', '999999999');

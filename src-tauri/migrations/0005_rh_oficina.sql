-- Recursos Humanos (salários com IRT e INSS) e Oficina (viaturas e ordens de serviço).
-- Valores em cêntimos. As taxas legais ficam em tabelas editáveis: CONFIRMAR antes de processar salários reais.

-- ======================================================================== RH
CREATE TABLE rh_departamentos (id INTEGER PRIMARY KEY, descricao TEXT NOT NULL);
CREATE TABLE rh_funcoes (id INTEGER PRIMARY KEY, descricao TEXT NOT NULL);
CREATE TABLE rh_categorias (id INTEGER PRIMARY KEY, descricao TEXT NOT NULL);
INSERT INTO rh_departamentos (descricao) VALUES ('Administração'), ('Comercial'), ('Armazém'), ('Oficina');
INSERT INTO rh_funcoes (descricao) VALUES ('Gerente'), ('Vendedor'), ('Operador de caixa'), ('Fiel de armazém'), ('Mecânico'), ('Motorista');
INSERT INTO rh_categorias (descricao) VALUES ('Quadro superior'), ('Quadro médio'), ('Técnico'), ('Operário qualificado'), ('Operário não qualificado');

CREATE TABLE funcionarios (
    id INTEGER PRIMARY KEY,
    numero TEXT,
    nome TEXT NOT NULL,
    sexo TEXT,
    data_nascimento TEXT,
    estado_civil TEXT,
    nacionalidade TEXT DEFAULT 'Angolana',
    naturalidade TEXT,
    bi_numero TEXT,
    bi_local_emissao TEXT,
    bi_validade TEXT,
    nif TEXT,
    inss_numero TEXT,
    morada TEXT,
    localidade TEXT,
    municipio TEXT,
    provincia TEXT,
    telefone TEXT,
    email TEXT,
    departamento_id INTEGER REFERENCES rh_departamentos(id),
    funcao_id INTEGER REFERENCES rh_funcoes(id),
    categoria_id INTEGER REFERENCES rh_categorias(id),
    tipo_contrato TEXT NOT NULL DEFAULT 'INDETERMINADO',   -- INDETERMINADO | DETERMINADO | PRESTACAO
    data_admissao TEXT,
    data_saida TEXT,
    motivo_saida TEXT,
    salario_base INTEGER NOT NULL DEFAULT 0,
    sub_alimentacao INTEGER NOT NULL DEFAULT 0,
    sub_transporte INTEGER NOT NULL DEFAULT 0,
    sub_chefia INTEGER NOT NULL DEFAULT 0,
    sub_turno INTEGER NOT NULL DEFAULT 0,
    abono_falhas INTEGER NOT NULL DEFAULT 0,
    sub_arrendamento INTEGER NOT NULL DEFAULT 0,
    sub_atavio INTEGER NOT NULL DEFAULT 0,
    sub_isencao_horario INTEGER NOT NULL DEFAULT 0,
    sub_deslocacao INTEGER NOT NULL DEFAULT 0,
    desconta_inss INTEGER NOT NULL DEFAULT 1,
    desconta_irt INTEGER NOT NULL DEFAULT 1,
    desconto_sindicato REAL NOT NULL DEFAULT 0,            -- % do salário base
    dependentes INTEGER NOT NULL DEFAULT 0,
    banco TEXT,
    iban TEXT,
    conta_empresa_id INTEGER,                              -- conta de tesouraria que paga o salário
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

CREATE TABLE rh_agregado (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id) ON DELETE CASCADE,
    nome TEXT NOT NULL,
    parentesco TEXT,
    data_nascimento TEXT
);

-- Rubricas de remuneração e de desconto. Nos descontos, incide_* = 1 significa que reduz a base (faltas).
CREATE TABLE rh_rubricas (
    codigo TEXT PRIMARY KEY,
    descricao TEXT NOT NULL,
    tipo TEXT NOT NULL,                       -- REMUNERACAO | DESCONTO
    incide_inss INTEGER NOT NULL DEFAULT 1,
    incide_irt INTEGER NOT NULL DEFAULT 1,
    isento_irt_ate INTEGER NOT NULL DEFAULT 0, -- parte isenta de IRT por mês (cêntimos)
    mapa TEXT,                                -- coluna no Mapa de Remunerações (IRT)
    ordem INTEGER NOT NULL DEFAULT 0
);
INSERT INTO rh_rubricas (codigo, descricao, tipo, incide_inss, incide_irt, isento_irt_ate, mapa, ordem) VALUES
    ('001', 'Salário base',                    'REMUNERACAO', 1, 1, 0,       'salario_base',    1),
    ('002', 'Subsídio de alimentação',         'REMUNERACAO', 1, 1, 3000000, 'sub_alimentacao', 2),
    ('003', 'Subsídio de transporte',          'REMUNERACAO', 1, 1, 3000000, 'sub_transporte',  3),
    ('004', 'Subsídio de férias',              'REMUNERACAO', 1, 1, 0,       'sub_ferias',      4),
    ('005', 'Subsídio de Natal',               'REMUNERACAO', 1, 1, 0,       'sub_natal',       5),
    ('006', 'Horas extraordinárias',           'REMUNERACAO', 1, 1, 0,       'horas_extra',     6),
    ('007', 'Abono de família',                'REMUNERACAO', 0, 0, 0,       'abono_familia',   7),
    ('008', 'Subsídio de chefia',              'REMUNERACAO', 1, 1, 0,       'outros',          8),
    ('009', 'Subsídio de turno',               'REMUNERACAO', 1, 1, 0,       'outros',          9),
    ('010', 'Abono para falhas',               'REMUNERACAO', 1, 1, 0,       'abono_falhas',    10),
    ('011', 'Subsídio de arrendamento',        'REMUNERACAO', 1, 1, 0,       'sub_renda',       11),
    ('012', 'Subsídio de atavio',              'REMUNERACAO', 1, 1, 0,       'sub_atavio',      12),
    ('013', 'Subsídio de isenção de horário',  'REMUNERACAO', 1, 1, 0,       'outros',          13),
    ('014', 'Subsídio de deslocação',          'REMUNERACAO', 1, 1, 0,       'outros',          14),
    ('015', 'Prémio de produtividade',         'REMUNERACAO', 1, 1, 0,       'premios',         15),
    ('016', 'Gratificação / bónus',            'REMUNERACAO', 1, 1, 0,       'premios',         16),
    ('017', 'Retroactivos',                    'REMUNERACAO', 1, 1, 0,       'outros',          17),
    ('018', 'Reembolso de despesas',           'REMUNERACAO', 0, 0, 0,       'reembolso',       18),
    ('101', 'Faltas injustificadas',           'DESCONTO',    1, 1, 0,       'faltas',          101),
    ('102', 'Adiantamento salarial',           'DESCONTO',    0, 0, 0,       NULL,              102),
    ('103', 'Quotização sindical',             'DESCONTO',    0, 0, 0,       NULL,              103),
    ('104', 'Outros descontos',                'DESCONTO',    0, 0, 0,       NULL,              104);

-- Tabela de IRT do Grupo A (Lei n.º 14/25 — OGE 2026). Valores em cêntimos. CONFIRMAR a tabela em vigor.
CREATE TABLE rh_irt_escaloes (
    id INTEGER PRIMARY KEY,
    de INTEGER NOT NULL,          -- matéria colectável a partir de (inclusive)
    ate INTEGER,                  -- até (inclusive); NULL = sem limite
    parcela_fixa INTEGER NOT NULL,
    taxa REAL NOT NULL,           -- %
    excesso INTEGER NOT NULL      -- sobre o excesso de
);
INSERT INTO rh_irt_escaloes (id, de, ate, parcela_fixa, taxa, excesso) VALUES
    (1,  0,          15000000,   0,         0,    0),
    (2,  15000001,   20000000,   1250000,   16,   15000000),
    (3,  20000001,   30000000,   3125000,   18,   20000000),
    (4,  30000001,   50000000,   4925000,   19,   30000000),
    (5,  50000001,   100000000,  8725000,   20,   50000000),
    (6,  100000001,  150000000,  18725000,  21,   100000000),
    (7,  150000001,  200000000,  29225000,  22,   150000000),
    (8,  200000001,  250000000,  40225000,  23,   200000000),
    (9,  250000001,  500000000,  51725000,  24,   250000000),
    (10, 500000001,  1000000000, 111725000, 24.5, 500000000),
    (11, 1000000001, NULL,       234225000, 25,   1000000000);

INSERT INTO config (chave, valor) VALUES
    ('rh.inss_trabalhador', '3'), ('rh.inss_empregador', '8'), ('rh.dias_uteis', '22'),
    ('rh.horas_dia', '8'), ('rh.acrescimo_horas_extra', '50'), ('rh.tabela_confirmada', '0');

CREATE TABLE rh_alteracoes (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    ano INTEGER NOT NULL,
    mes INTEGER NOT NULL,
    rubrica TEXT NOT NULL REFERENCES rh_rubricas(codigo),
    quantidade REAL NOT NULL DEFAULT 0,       -- dias de falta, horas extra, % de subsídio…
    valor INTEGER NOT NULL DEFAULT 0,         -- valor fixo (cêntimos); 0 = calcular pela quantidade
    descricao TEXT,
    data TEXT,
    processado INTEGER NOT NULL DEFAULT 0,
    adiantamento_id INTEGER
);
CREATE INDEX idx_rh_alt ON rh_alteracoes(ano, mes);

CREATE TABLE rh_adiantamentos (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    data TEXT NOT NULL,
    valor INTEGER NOT NULL,
    parcelas INTEGER NOT NULL DEFAULT 1,
    ano_inicio INTEGER NOT NULL,
    mes_inicio INTEGER NOT NULL,
    descricao TEXT,
    conta_id INTEGER,
    utilizador_id INTEGER NOT NULL
);

CREATE TABLE rh_processamentos (
    id INTEGER PRIMARY KEY,
    ano INTEGER NOT NULL,
    mes INTEGER NOT NULL,
    data TEXT NOT NULL,
    estado TEXT NOT NULL DEFAULT 'PROCESSADO',   -- PROCESSADO | ANULADO
    utilizador_id INTEGER NOT NULL,
    total_liquido INTEGER NOT NULL DEFAULT 0,
    pago INTEGER NOT NULL DEFAULT 0,
    motivo_anulacao TEXT
);
CREATE UNIQUE INDEX ux_rh_proc ON rh_processamentos(ano, mes) WHERE estado = 'PROCESSADO';

CREATE TABLE rh_recibos (
    id INTEGER PRIMARY KEY,
    processamento_id INTEGER NOT NULL REFERENCES rh_processamentos(id) ON DELETE CASCADE,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    numero TEXT, nome TEXT, nif TEXT, inss_numero TEXT, departamento TEXT, funcao TEXT, categoria TEXT,
    provincia TEXT, municipio TEXT, banco TEXT, iban TEXT,
    salario_base INTEGER NOT NULL DEFAULT 0,
    total_remuneracoes INTEGER NOT NULL DEFAULT 0,
    base_inss INTEGER NOT NULL DEFAULT 0,
    inss_trabalhador INTEGER NOT NULL DEFAULT 0,
    inss_empregador INTEGER NOT NULL DEFAULT 0,
    base_irt INTEGER NOT NULL DEFAULT 0,
    irt INTEGER NOT NULL DEFAULT 0,
    total_descontos INTEGER NOT NULL DEFAULT 0,
    liquido INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE rh_recibo_linhas (
    id INTEGER PRIMARY KEY,
    recibo_id INTEGER NOT NULL REFERENCES rh_recibos(id) ON DELETE CASCADE,
    rubrica TEXT NOT NULL,
    descricao TEXT NOT NULL,
    tipo TEXT NOT NULL,
    quantidade REAL,
    valor INTEGER NOT NULL
);

CREATE TABLE rh_ferias (
    id INTEGER PRIMARY KEY,
    funcionario_id INTEGER NOT NULL REFERENCES funcionarios(id),
    ano INTEGER NOT NULL,
    data_inicio TEXT NOT NULL,
    data_fim TEXT NOT NULL,
    dias INTEGER NOT NULL,
    estado TEXT NOT NULL DEFAULT 'MARCADA',   -- MARCADA | GOZADA | ANULADA
    observacoes TEXT
);

-- ======================================================================== Oficina
CREATE TABLE of_viaturas (
    id INTEGER PRIMARY KEY,
    entidade_id INTEGER REFERENCES entidades(id),
    matricula TEXT,
    marca TEXT NOT NULL,
    modelo TEXT,
    cor TEXT,
    ano INTEGER,
    chassi TEXT,
    motor TEXT,
    combustivel TEXT,
    categoria TEXT,
    km INTEGER NOT NULL DEFAULT 0,
    observacoes TEXT,
    ativo INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
CREATE INDEX idx_of_viat_mat ON of_viaturas(matricula);

CREATE TABLE of_ordens (
    id INTEGER PRIMARY KEY,
    numero TEXT NOT NULL UNIQUE,
    data_entrada TEXT NOT NULL,
    viatura_id INTEGER NOT NULL REFERENCES of_viaturas(id),
    entidade_id INTEGER REFERENCES entidades(id),
    km_entrada INTEGER,
    km_saida INTEGER,
    referencia TEXT,
    avaria TEXT,
    diagnostico TEXT,
    tecnico TEXT,
    estado TEXT NOT NULL DEFAULT 'ABERTA',    -- ABERTA | EM_CURSO | CONCLUIDA | FACTURADA | CANCELADA
    data_conclusao TEXT,
    documento_id INTEGER REFERENCES documentos(id),
    observacoes TEXT,
    utilizador_id INTEGER NOT NULL
);
CREATE TABLE of_ordem_linhas (
    id INTEGER PRIMARY KEY,
    ordem_id INTEGER NOT NULL REFERENCES of_ordens(id) ON DELETE CASCADE,
    artigo_id INTEGER REFERENCES artigos(id),
    descricao TEXT NOT NULL,
    quantidade REAL NOT NULL DEFAULT 1,
    mecanico TEXT
);

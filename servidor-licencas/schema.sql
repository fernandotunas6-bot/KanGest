-- Servidor de licenças do KanGest (Cloudflare D1 / SQLite). Valores em cêntimos de kwanza.

-- Planos à venda. Os preços ficam a 0 até serem definidos no painel.
CREATE TABLE IF NOT EXISTS planos (
    codigo TEXT PRIMARY KEY,
    nome TEXT NOT NULL,
    postos INTEGER NOT NULL,          -- 0 = sem limite
    preco_mes INTEGER NOT NULL DEFAULT 0,
    preco_ano INTEGER NOT NULL DEFAULT 0,
    activo INTEGER NOT NULL DEFAULT 1,
    ordem INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO planos (codigo, nome, postos, ordem) VALUES
    ('ESSENCIAL', 'Essencial', 1, 1),
    ('COMERCIO', 'Comércio', 3, 2),
    ('EMPRESA', 'Empresa', 0, 3);

-- Uma empresa = um NIF. A experiência dá-se uma única vez por NIF.
CREATE TABLE IF NOT EXISTS clientes (
    nif TEXT PRIMARY KEY,
    empresa TEXT NOT NULL,
    email TEXT,
    telefone TEXT,
    criado_em TEXT NOT NULL,
    experiencia_ate TEXT,             -- último dia da experiência
    plano TEXT REFERENCES planos(codigo),
    pago_ate TEXT,                    -- último dia pago da assinatura
    postos INTEGER,                   -- NULL = os do plano
    maquinas_max INTEGER NOT NULL DEFAULT 2, -- instalações (servidores) permitidas para este NIF
    notas TEXT
);

-- Computadores onde o KanGest está activado (o código do pedido: KG-<NIF>-<máquina>).
CREATE TABLE IF NOT EXISTS maquinas (
    nif TEXT NOT NULL REFERENCES clientes(nif),
    maquina TEXT NOT NULL,
    primeiro_contacto TEXT NOT NULL,
    ultimo_contacto TEXT NOT NULL,
    versao TEXT,
    bloqueada INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (nif, maquina)
);

-- Códigos de confirmação do registo (guardados só como resumo SHA-256).
CREATE TABLE IF NOT EXISTS codigos (
    nif TEXT NOT NULL,
    maquina TEXT NOT NULL,
    empresa TEXT NOT NULL,
    email TEXT,
    telefone TEXT,
    codigo_hash TEXT NOT NULL,
    codigo_aberto TEXT,               -- só quando o email não foi enviado: o painel mostra-o para o dar ao cliente
    expira_em TEXT NOT NULL,
    tentativas INTEGER NOT NULL DEFAULT 0,
    pedidos INTEGER NOT NULL DEFAULT 1,
    criado_em TEXT NOT NULL,
    PRIMARY KEY (nif, maquina)
);

CREATE TABLE IF NOT EXISTS licencas (
    id TEXT PRIMARY KEY,
    nif TEXT NOT NULL,
    empresa TEXT NOT NULL,
    maquina TEXT,
    plano TEXT NOT NULL,
    postos INTEGER NOT NULL,
    inicio TEXT NOT NULL,
    fim TEXT NOT NULL,
    tolerancia INTEGER NOT NULL,
    emitida_em TEXT NOT NULL,
    origem TEXT NOT NULL,             -- EXPERIENCIA · RENOVACAO · PAINEL
    chave TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_licencas_nif ON licencas(nif, maquina, fim);

CREATE TABLE IF NOT EXISTS pagamentos (
    id INTEGER PRIMARY KEY,
    nif TEXT NOT NULL REFERENCES clientes(nif),
    plano TEXT NOT NULL,
    meses INTEGER NOT NULL,
    valor INTEGER NOT NULL,
    metodo TEXT NOT NULL,             -- TRANSFERENCIA · MULTICAIXA · NUMERARIO · OUTRO
    referencia TEXT,
    pago_ate TEXT NOT NULL,           -- a assinatura ficou paga até este dia
    registado_em TEXT NOT NULL
);

-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Backend central do KanGest no Supabase: as tabelas do servidor de licenças (servidor-licencas/schema.sql), arrumadas
-- por esquema. Tabelas e colunas com os mesmos nomes da D1: a função kangest corre o mesmo worker.js e os dados copiam-se
-- directamente. Ver docs/ARQUITETURA.md.
--
-- Convenções: datas em texto ISO 8601 (UTC) e dias em AAAA-MM-DD; dinheiro em cêntimos de kwanza (bigint);
-- indicadores 0/1 (integer). Só a função kangest mexe nestes dados, pela ligação directa ao Postgres: os esquemas não
-- se expõem na API REST, as tabelas têm RLS sem políticas e anon/authenticated não têm privilégios (no fim).

CREATE SCHEMA empresas;
CREATE SCHEMA licencas;
CREATE SCHEMA equipa;
CREATE SCHEMA seguranca;
CREATE SCHEMA web;

COMMENT ON SCHEMA empresas IS 'A empresa (NIF), a conta do dono no site e os planos à venda.';
COMMENT ON SCHEMA licencas IS 'Computadores registados e o seu estado, licenças emitidas, pagamentos e códigos de registo.';
COMMENT ON SCHEMA equipa IS 'Equipa gerida no site (funcionários, lojas, postos) e a que cada KanGest comunica.';
COMMENT ON SCHEMA seguranca IS 'Sessões, códigos da conta, segredos dos computadores e limite de pedidos por IP.';
COMMENT ON SCHEMA web IS 'Acesso pela Internet: um subdomínio por empresa, servido por um túnel até ao KanGest da loja.';

-- ============================================================================================== empresas

CREATE TABLE empresas.planos (
    codigo text PRIMARY KEY,
    nome text NOT NULL,
    postos integer NOT NULL,                        -- 0 = sem limite
    preco_mes bigint NOT NULL DEFAULT 0,            -- cêntimos; 0 até ser definido no painel
    preco_ano bigint NOT NULL DEFAULT 0,
    activo integer NOT NULL DEFAULT 1 CHECK (activo IN (0, 1)),
    ordem integer NOT NULL DEFAULT 0
);
COMMENT ON TABLE empresas.planos IS 'Planos à venda.';
INSERT INTO empresas.planos (codigo, nome, postos, ordem) VALUES
    ('ESSENCIAL', 'Essencial', 1, 1),
    ('COMERCIO', 'Comércio', 3, 2),
    ('EMPRESA', 'Empresa', 0, 3)
ON CONFLICT (codigo) DO NOTHING;

CREATE TABLE empresas.clientes (
    nif text PRIMARY KEY,
    empresa text NOT NULL,
    email text,
    telefone text,
    criado_em text NOT NULL,
    experiencia_ate text,                           -- último dia da experiência (dá-se uma vez por NIF)
    plano text REFERENCES empresas.planos (codigo),
    pago_ate text,                                  -- último dia pago da assinatura
    postos integer,                                 -- NULL = os do plano
    maquinas_max integer NOT NULL DEFAULT 2,        -- instalações (servidores) permitidas para este NIF
    notas text
);
COMMENT ON TABLE empresas.clientes IS 'Uma empresa = um NIF.';

CREATE TABLE empresas.contas (
    nif text PRIMARY KEY,
    email text NOT NULL UNIQUE,
    senha text NOT NULL,                            -- só o resumo PBKDF2
    criada_em text NOT NULL,
    actualizada_em text NOT NULL
);
COMMENT ON TABLE empresas.contas IS 'Conta do dono da empresa no site (/conta.html): email do registo e senha.';

-- ============================================================================================== licencas

CREATE TABLE licencas.maquinas (
    nif text NOT NULL REFERENCES empresas.clientes (nif),
    maquina text NOT NULL,
    primeiro_contacto text NOT NULL,
    ultimo_contacto text NOT NULL,
    versao text,
    bloqueada integer NOT NULL DEFAULT 0 CHECK (bloqueada IN (0, 1)),
    PRIMARY KEY (nif, maquina)
);
COMMENT ON TABLE licencas.maquinas IS 'Computadores onde o KanGest está activado (código do pedido KG-<NIF>-<máquina>).';

CREATE TABLE licencas.instalacoes (
    nif text NOT NULL,
    maquina text NOT NULL,
    recebido_em text NOT NULL,
    versao text,
    modo text,                                      -- loja | escola
    licenca_estado text,                            -- como o KanGest a vê (ACTIVA, EXPERIENCIA, TOLERANCIA, EXPIRADA…)
    licenca_fim text,
    copia_ultima text,                              -- última cópia local (hora do computador)
    copia_externa_ultima text,
    copia_externa_erro integer NOT NULL DEFAULT 0 CHECK (copia_externa_erro IN (0, 1)),
    PRIMARY KEY (nif, maquina)
);
COMMENT ON TABLE licencas.instalacoes IS 'Estado que cada KanGest envia na verificação: só datas e indicadores, nunca dados da empresa.';

CREATE TABLE licencas.licencas (
    id text PRIMARY KEY,
    nif text NOT NULL,
    empresa text NOT NULL,
    maquina text,
    plano text NOT NULL,
    postos integer NOT NULL,
    inicio text NOT NULL,
    fim text NOT NULL,
    tolerancia integer NOT NULL,
    emitida_em text NOT NULL,
    origem text NOT NULL,                           -- EXPERIENCIA · RENOVACAO · PAINEL
    chave text NOT NULL                             -- KG1.<dados>.<assinatura Ed25519>
);
CREATE INDEX idx_licencas_nif ON licencas.licencas (nif, maquina, fim);
COMMENT ON TABLE licencas.licencas IS 'Licenças emitidas.';

CREATE TABLE licencas.pagamentos (
    id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    nif text NOT NULL REFERENCES empresas.clientes (nif),
    plano text NOT NULL,
    meses integer NOT NULL,
    valor bigint NOT NULL,                          -- cêntimos
    metodo text NOT NULL,                           -- TRANSFERENCIA · MULTICAIXA · NUMERARIO · OUTRO
    referencia text,
    pago_ate text NOT NULL,                         -- a assinatura ficou paga até este dia
    registado_em text NOT NULL
);
CREATE INDEX idx_pagamentos_nif ON licencas.pagamentos (nif, id);
COMMENT ON TABLE licencas.pagamentos IS 'Pagamentos registados pelo produtor.';

CREATE TABLE licencas.codigos (
    nif text NOT NULL,
    maquina text NOT NULL,
    empresa text NOT NULL,
    email text,
    telefone text,
    codigo_hash text NOT NULL,                      -- só o resumo SHA-256
    codigo_aberto text,                             -- só quando o email não foi enviado: o produtor dá-o ao cliente
    expira_em text NOT NULL,
    tentativas integer NOT NULL DEFAULT 0,
    pedidos integer NOT NULL DEFAULT 1,
    criado_em text NOT NULL,
    PRIMARY KEY (nif, maquina)
);
COMMENT ON TABLE licencas.codigos IS 'Códigos de confirmação do registo de um computador.';

-- ============================================================================================== equipa

CREATE TABLE equipa.funcionarios (
    nif text NOT NULL,
    id text NOT NULL,                               -- no KanGest: utilizadores.conta_id
    login text NOT NULL,
    nome text NOT NULL,
    perfil text NOT NULL,
    loja text,
    ativo integer NOT NULL DEFAULT 1 CHECK (ativo IN (0, 1)),
    senha_provisoria text,                          -- só o resumo PBKDF2; apaga-se quando o KanGest a aplica
    criado_em text NOT NULL,
    actualizado_em text NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, login)
);
COMMENT ON TABLE equipa.funcionarios IS 'Funcionários geridos na conta da empresa. Nunca se apagam: desactivar tira o acesso.';

CREATE TABLE equipa.lojas_geridas (
    nif text NOT NULL,
    id text NOT NULL,                               -- no KanGest: armazens.conta_id
    nome text NOT NULL,
    ativo integer NOT NULL DEFAULT 1 CHECK (ativo IN (0, 1)),
    actualizado_em text NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, nome)
);
COMMENT ON TABLE equipa.lojas_geridas IS 'Lojas geridas na conta da empresa.';

CREATE TABLE equipa.postos_geridos (
    nif text NOT NULL,
    id text NOT NULL,                               -- no KanGest: postos.conta_id
    descricao text NOT NULL,
    loja text,
    actualizado_em text NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, descricao)
);
COMMENT ON TABLE equipa.postos_geridos IS 'Postos (caixas) geridos na conta da empresa.';

CREATE TABLE equipa.equipa_versoes (
    nif text PRIMARY KEY,
    versao integer NOT NULL,                        -- sobe a cada alteração no site
    actualizado_em text NOT NULL,
    aplicada integer NOT NULL DEFAULT 0,            -- a versão que o KanGest já aplicou
    aplicada_em text,
    aplicada_maquina text
);
COMMENT ON TABLE equipa.equipa_versoes IS 'Versão da equipa gerida: o KanGest que está atrás recebe-a na verificação.';

CREATE TABLE equipa.equipas (
    nif text NOT NULL,
    maquina text NOT NULL,
    recebido_em text NOT NULL,
    dados text NOT NULL,                            -- JSON: login, nome, perfil, loja, activo; nunca senhas nem contactos
    PRIMARY KEY (nif, maquina)
);
COMMENT ON TABLE equipa.equipas IS 'A equipa, as lojas e os postos que o KanGest de cada computador comunicou.';

-- ============================================================================================== seguranca

CREATE TABLE seguranca.sessoes_painel (
    id text PRIMARY KEY,
    email text NOT NULL,
    criada_em text NOT NULL,
    ultima_em text NOT NULL,
    aparelho text,                                  -- do User-Agent
    local text,                                     -- aproximado; o IP não se guarda
    terminada_em text
);
COMMENT ON TABLE seguranca.sessoes_painel IS 'Sessões do produtor (área do produtor no site).';

CREATE TABLE seguranca.sessoes_conta (
    id text PRIMARY KEY,
    nif text NOT NULL,
    criada_em text NOT NULL,
    ultima_em text NOT NULL,
    aparelho text,
    local text,
    terminada_em text
);
CREATE INDEX idx_sessoes_conta_nif ON seguranca.sessoes_conta (nif, terminada_em);
COMMENT ON TABLE seguranca.sessoes_conta IS 'Sessões do dono da empresa (conta da empresa no site).';

CREATE TABLE seguranca.codigos_conta (
    email text PRIMARY KEY,
    nif text NOT NULL,
    codigo_hash text NOT NULL,                      -- só o resumo SHA-256
    codigo_aberto text,                             -- só enquanto o envio de email não estiver configurado
    expira_em text NOT NULL,
    tentativas integer NOT NULL DEFAULT 0,
    criado_em text NOT NULL
);
COMMENT ON TABLE seguranca.codigos_conta IS 'Códigos de 6 algarismos para criar ou repor a senha da conta da empresa.';

CREATE TABLE seguranca.segredos_maquina (
    nif text NOT NULL,
    maquina text NOT NULL,
    segredo_hash text NOT NULL,                     -- só o resumo SHA-256
    criado_em text NOT NULL,
    PRIMARY KEY (nif, maquina)
);
COMMENT ON TABLE seguranca.segredos_maquina IS 'Segredo de cada computador registado: autentica a renovação e o acesso pela Internet.';

CREATE TABLE seguranca.limites (
    chave text PRIMARY KEY,                         -- «registo:<IP>»
    janela text NOT NULL,                           -- hora UTC (AAAA-MM-DDTHH)
    n integer NOT NULL DEFAULT 0
);
COMMENT ON TABLE seguranca.limites IS 'Pedidos por IP e por hora (registo, confirmação, renovação, falhas de entrada).';

-- ============================================================================================== web

CREATE TABLE web.subdominios (
    nif text PRIMARY KEY,
    nome text NOT NULL UNIQUE,
    maquina text NOT NULL,                          -- computador onde o túnel corre
    tunel_id text NOT NULL,
    dns_id text,
    porta integer NOT NULL,
    criado_em text NOT NULL,
    actualizado_em text NOT NULL
);
COMMENT ON TABLE web.subdominios IS 'Um subdomínio por empresa (ex.: loja.kangest.ao), com o túnel da Cloudflare até ao KanGest.';

-- ============================================================================================== acesso

-- RLS sem políticas: pela API do Supabase (anon, authenticated) não se lê nem escreve nada. A função kangest liga-se
-- como dona das tabelas, a quem o RLS não se aplica.
ALTER TABLE empresas.planos ENABLE ROW LEVEL SECURITY;
ALTER TABLE empresas.clientes ENABLE ROW LEVEL SECURITY;
ALTER TABLE empresas.contas ENABLE ROW LEVEL SECURITY;
ALTER TABLE licencas.maquinas ENABLE ROW LEVEL SECURITY;
ALTER TABLE licencas.instalacoes ENABLE ROW LEVEL SECURITY;
ALTER TABLE licencas.licencas ENABLE ROW LEVEL SECURITY;
ALTER TABLE licencas.pagamentos ENABLE ROW LEVEL SECURITY;
ALTER TABLE licencas.codigos ENABLE ROW LEVEL SECURITY;
ALTER TABLE equipa.funcionarios ENABLE ROW LEVEL SECURITY;
ALTER TABLE equipa.lojas_geridas ENABLE ROW LEVEL SECURITY;
ALTER TABLE equipa.postos_geridos ENABLE ROW LEVEL SECURITY;
ALTER TABLE equipa.equipa_versoes ENABLE ROW LEVEL SECURITY;
ALTER TABLE equipa.equipas ENABLE ROW LEVEL SECURITY;
ALTER TABLE seguranca.sessoes_painel ENABLE ROW LEVEL SECURITY;
ALTER TABLE seguranca.sessoes_conta ENABLE ROW LEVEL SECURITY;
ALTER TABLE seguranca.codigos_conta ENABLE ROW LEVEL SECURITY;
ALTER TABLE seguranca.segredos_maquina ENABLE ROW LEVEL SECURITY;
ALTER TABLE seguranca.limites ENABLE ROW LEVEL SECURITY;
ALTER TABLE web.subdominios ENABLE ROW LEVEL SECURITY;

-- e nenhum privilégio para os papéis da API, nem nas tabelas que vierem depois
REVOKE ALL ON SCHEMA empresas, licencas, equipa, seguranca, web FROM PUBLIC, anon, authenticated;
REVOKE ALL ON ALL TABLES IN SCHEMA empresas, licencas, equipa, seguranca, web FROM PUBLIC, anon, authenticated;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA empresas, licencas, equipa, seguranca, web FROM PUBLIC, anon, authenticated;
ALTER DEFAULT PRIVILEGES IN SCHEMA empresas, licencas, equipa, seguranca, web REVOKE ALL ON TABLES FROM PUBLIC, anon, authenticated;
ALTER DEFAULT PRIVILEGES IN SCHEMA empresas, licencas, equipa, seguranca, web REVOKE ALL ON SEQUENCES FROM PUBLIC, anon, authenticated;
ALTER DEFAULT PRIVILEGES IN SCHEMA empresas, licencas, equipa, seguranca, web REVOKE ALL ON FUNCTIONS FROM PUBLIC, anon, authenticated;

-- KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Servidor de licenças do KanGest (Cloudflare D1 / SQLite). Valores em cêntimos de kwanza.
-- Arrumado por área, como em docs/ARQUITETURA.md: empresas, licenças, equipa, segurança e web. Só CREATE … IF NOT EXISTS:
-- publicar outra vez cria o que falta e não mexe no que existe.

-- ============================== empresas: a empresa (NIF), a conta do dono no site e os planos à venda

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

-- Contas das empresas no site (portal do cliente, como as definições da conta do Claude): o dono entra com o email do
-- registo e uma senha (só o resumo PBKDF2). A senha cria-se ou repõe-se com um código de 6 algarismos.
CREATE TABLE IF NOT EXISTS contas (
    nif TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    senha TEXT NOT NULL,
    criada_em TEXT NOT NULL,
    actualizada_em TEXT NOT NULL
);

-- ============================== licenças: computadores registados e o seu estado, licenças emitidas, pagamentos e códigos de registo

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

-- Estado de cada instalação, enviado pelo KanGest em cada verificação (/v1/renovar, no máximo de 12 em 12 horas):
-- versão, modo de gestão, licença e cópias de segurança. Só datas e indicadores, nunca caminhos nem dados da empresa.
-- Aparece na ficha do cliente (servidor e área do produtor no site).
CREATE TABLE IF NOT EXISTS instalacoes (
    nif TEXT NOT NULL,
    maquina TEXT NOT NULL,
    recebido_em TEXT NOT NULL,
    versao TEXT,
    modo TEXT,                        -- loja | escola
    licenca_estado TEXT,              -- como o KanGest a vê (ACTIVA, EXPERIENCIA, TOLERANCIA, EXPIRADA…)
    licenca_fim TEXT,                 -- AAAA-MM-DD
    copia_ultima TEXT,                -- última cópia local (AAAA-MM-DDTHH:MM:SS, hora do computador)
    copia_externa_ultima TEXT,        -- última cópia na pasta externa (pen, disco, pasta partilhada)
    copia_externa_erro INTEGER NOT NULL DEFAULT 0,  -- 1 = a última cópia externa falhou
    PRIMARY KEY (nif, maquina)
);

-- Licenças emitidas («KG1.<dados>.<assinatura Ed25519>»).
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

-- Pagamentos registados pelo produtor.
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

-- ============================== equipa: a equipa gerida no site e a que cada KanGest comunica

-- Equipa gerida no site (fase 3b): os funcionários, as lojas e os postos que o dono quer no KanGest. Cada alteração sobe
-- a versão; o KanGest diz na verificação que versão tem e, se estiver atrás, recebe a equipa e aplica-a (nunca apaga:
-- desactivar é que tira o acesso). A senha provisória só vai como resumo PBKDF2 e apaga-se quando o KanGest a aplica.
CREATE TABLE IF NOT EXISTS funcionarios (
    nif TEXT NOT NULL,
    id TEXT NOT NULL,
    login TEXT NOT NULL,
    nome TEXT NOT NULL,
    perfil TEXT NOT NULL,
    loja TEXT,
    ativo INTEGER NOT NULL DEFAULT 1,
    senha_provisoria TEXT,
    criado_em TEXT NOT NULL,
    actualizado_em TEXT NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, login)
);

CREATE TABLE IF NOT EXISTS lojas_geridas (
    nif TEXT NOT NULL,
    id TEXT NOT NULL,
    nome TEXT NOT NULL,
    ativo INTEGER NOT NULL DEFAULT 1,
    actualizado_em TEXT NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, nome)
);

CREATE TABLE IF NOT EXISTS postos_geridos (
    nif TEXT NOT NULL,
    id TEXT NOT NULL,
    descricao TEXT NOT NULL,
    loja TEXT,
    actualizado_em TEXT NOT NULL,
    PRIMARY KEY (nif, id),
    UNIQUE (nif, descricao)
);

CREATE TABLE IF NOT EXISTS equipa_versoes (
    nif TEXT PRIMARY KEY,
    versao INTEGER NOT NULL,
    actualizado_em TEXT NOT NULL,
    aplicada INTEGER NOT NULL DEFAULT 0,
    aplicada_em TEXT,
    aplicada_maquina TEXT
);

-- A equipa, as lojas e os postos que o KanGest de cada computador comunicou (só login, nome, perfil, loja, activo, dois
-- passos e último acesso; nunca senhas nem contactos), para a secção «Equipa» da conta da empresa.
CREATE TABLE IF NOT EXISTS equipas (
    nif TEXT NOT NULL,
    maquina TEXT NOT NULL,
    recebido_em TEXT NOT NULL,
    dados TEXT NOT NULL,
    PRIMARY KEY (nif, maquina)
);

-- ============================== segurança: sessões, códigos da conta, segredos dos computadores e limite de pedidos por IP

-- Sessões do painel do produtor (entrada com email e senha), para «Onde fez login» e «Sair em todos os aparelhos».
-- O aparelho vem do User-Agent e o local aproximado da Cloudflare; o IP não se guarda.
CREATE TABLE IF NOT EXISTS sessoes_painel (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    criada_em TEXT NOT NULL,
    ultima_em TEXT NOT NULL,
    aparelho TEXT,
    local TEXT,
    terminada_em TEXT
);

-- Sessões do dono da empresa na conta do site (aparelho e local aproximado; o IP não se guarda).
CREATE TABLE IF NOT EXISTS sessoes_conta (
    id TEXT PRIMARY KEY,
    nif TEXT NOT NULL,
    criada_em TEXT NOT NULL,
    ultima_em TEXT NOT NULL,
    aparelho TEXT,
    local TEXT,
    terminada_em TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessoes_conta_nif ON sessoes_conta (nif, terminada_em);

-- Códigos de 6 algarismos para criar ou repor a senha da conta da empresa (só o resumo SHA-256).
CREATE TABLE IF NOT EXISTS codigos_conta (
    email TEXT PRIMARY KEY,
    nif TEXT NOT NULL,
    codigo_hash TEXT NOT NULL,
    codigo_aberto TEXT,               -- só enquanto o envio de email não estiver configurado (o produtor dá-o)
    expira_em TEXT NOT NULL,
    tentativas INTEGER NOT NULL DEFAULT 0,
    criado_em TEXT NOT NULL
);

-- Segredo de cada computador registado (só o resumo SHA-256), dado ao confirmar o registo. Autentica os pedidos
-- seguintes desse computador (renovação, acesso pela Internet): o NIF e o código do computador não são segredos.
CREATE TABLE IF NOT EXISTS segredos_maquina (
    nif TEXT NOT NULL,
    maquina TEXT NOT NULL,
    segredo_hash TEXT NOT NULL,
    criado_em TEXT NOT NULL,
    PRIMARY KEY (nif, maquina)
);

-- Limite de pedidos por IP e por hora (registo, confirmação, renovação, tentativas falhadas no painel).
CREATE TABLE IF NOT EXISTS limites (
    chave TEXT PRIMARY KEY,           -- «registo:41.63.1.2»
    janela TEXT NOT NULL,             -- hora UTC (AAAA-MM-DDTHH)
    n INTEGER NOT NULL DEFAULT 0
);

-- ============================== web: acesso pela Internet (um subdomínio por empresa)

-- Acesso pela Internet: um subdomínio por empresa (ex.: loja.kangest.ao), servido por um túnel da Cloudflare até ao
-- KanGest servidor da loja. O túnel e o registo DNS são criados pelo servidor de licenças, em nome do produtor.
CREATE TABLE IF NOT EXISTS subdominios (
    nif TEXT PRIMARY KEY,
    nome TEXT NOT NULL UNIQUE,
    maquina TEXT NOT NULL,            -- computador onde o túnel corre
    tunel_id TEXT NOT NULL,
    dns_id TEXT,
    porta INTEGER NOT NULL,
    criado_em TEXT NOT NULL,
    actualizado_em TEXT NOT NULL
);

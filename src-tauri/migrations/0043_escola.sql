-- KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Gestão escolar (tipo de gestão «escola»), só a parte financeira: o aluno é o cliente que paga (a sua
-- conta corrente), a turma diz o valor da propina e a matrícula liga os dois num ano lectivo. A propina
-- mensal de cada matrícula é uma avença (avencas.rs): uma factura por mês, sem repetir, com a dívida em
-- documentos.valor_pendente como em qualquer conta corrente.

CREATE TABLE anos_lectivos (
    id INTEGER PRIMARY KEY,
    designacao TEXT NOT NULL UNIQUE,              -- ex.: «2026/2027»
    inicio TEXT NOT NULL,                         -- AAAA-MM-01: o mês da primeira propina
    fim TEXT NOT NULL,                            -- AAAA-MM-01: o mês da última propina
    corrente INTEGER NOT NULL DEFAULT 0,          -- o ano que os ecrãs abrem por omissão
    CHECK (fim >= inicio)
);

CREATE TABLE turmas (
    id INTEGER PRIMARY KEY,
    ano_lectivo_id INTEGER NOT NULL REFERENCES anos_lectivos(id),
    classe TEXT NOT NULL,                         -- ex.: «7.ª classe»
    turma TEXT NOT NULL DEFAULT 'A',
    propina INTEGER NOT NULL DEFAULT 0 CHECK (propina >= 0),   -- propina mensal, cêntimos
    ativo INTEGER NOT NULL DEFAULT 1,
    UNIQUE (ano_lectivo_id, classe, turma)
);

CREATE TABLE matriculas (
    id INTEGER PRIMARY KEY,
    entidade_id INTEGER NOT NULL REFERENCES entidades(id),   -- o aluno: o cliente das facturas
    turma_id INTEGER NOT NULL REFERENCES turmas(id),
    ano_lectivo_id INTEGER NOT NULL REFERENCES anos_lectivos(id),
    data TEXT NOT NULL,
    desconto REAL NOT NULL DEFAULT 0 CHECK (desconto BETWEEN 0 AND 100),  -- bolsa ou desconto na propina, %
    avenca_id INTEGER REFERENCES avencas(id),     -- a propina mensal (sem ela: bolsa integral)
    estado TEXT NOT NULL DEFAULT 'ACTIVA' CHECK (estado IN ('ACTIVA', 'ANULADA')),
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
-- uma só matrícula viva por aluno e ano lectivo (a anulada não conta)
CREATE UNIQUE INDEX idx_matricula_ano ON matriculas(entidade_id, ano_lectivo_id) WHERE estado = 'ACTIVA';
CREATE INDEX idx_matriculas_turma ON matriculas(turma_id);

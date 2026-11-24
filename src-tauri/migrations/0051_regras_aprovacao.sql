-- Regras de aprovação (decisão do dono, 2026-11-24). Cada regra liga-se à parte e pede o PIN de quem tenha a
-- permissão de aprovar dessa regra (pode ser o próprio utilizador, se a tiver). Chegam desligadas: até o administrador
-- as ligar, tudo funciona como antes.
--   anular   · anular documentos (opcional: só os de dias anteriores)
--   saidas   · saídas de tesouraria, pagamentos a fornecedores e devoluções de dinheiro a clientes
--   vales    · vales de caixa (saídas de dinheiro da gaveta do POS)
--   creditos · notas de crédito e regularizações de conta corrente
CREATE TABLE regras_aprovacao (
    regra TEXT PRIMARY KEY CHECK (regra IN ('anular', 'saidas', 'vales', 'creditos')),
    ativo INTEGER NOT NULL DEFAULT 0,
    -- cêntimos: a regra aplica-se a partir deste valor (0 = qualquer valor)
    valor_minimo INTEGER NOT NULL DEFAULT 0,
    -- só na regra «anular»: 1 = só os documentos de dias anteriores
    so_dias_anteriores INTEGER NOT NULL DEFAULT 0
);
INSERT INTO regras_aprovacao (regra) VALUES ('anular'), ('saidas'), ('vales'), ('creditos');

-- Cada aprovação dada: quem pediu, quem aprovou com o PIN, o quê e quanto.
CREATE TABLE aprovacoes (
    id INTEGER PRIMARY KEY,
    regra TEXT NOT NULL,
    operacao TEXT NOT NULL,
    -- o documento ou o movimento (ex.: «NC A/3», «Tesouraria n.º 15», «Vale n.º 8»)
    referencia TEXT,
    valor INTEGER NOT NULL,
    pedido_por INTEGER REFERENCES utilizadores(id),
    aprovado_por INTEGER NOT NULL REFERENCES utilizadores(id),
    criado_em TEXT NOT NULL
);
CREATE INDEX idx_aprovacoes_data ON aprovacoes(criado_em);

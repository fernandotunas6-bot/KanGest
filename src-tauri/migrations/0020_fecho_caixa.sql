-- Fecho de caixa: n.º sequencial do fecho (relatório Z) por posto, contagem por notas e moedas,
-- valores contados por forma de pagamento (talão do TPA, transferências) e fecho cego.
ALTER TABLE caixa_sessoes ADD COLUMN numero_z INTEGER;
ALTER TABLE caixa_sessoes ADD COLUMN contagem TEXT;          -- JSON {"5000": 3, "1000": 2, ...} (Kz → quantidade)
ALTER TABLE caixa_sessoes ADD COLUMN contado_modos TEXT;     -- JSON {"MB": 1234500, ...} (cêntimos)
ALTER TABLE caixa_sessoes ADD COLUMN cego INTEGER NOT NULL DEFAULT 0;
ALTER TABLE caixa_sessoes ADD COLUMN fechada_por INTEGER REFERENCES utilizadores(id);
ALTER TABLE caixa_sessoes ADD COLUMN observacoes TEXT;

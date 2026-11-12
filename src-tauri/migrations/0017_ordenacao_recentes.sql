-- As listas mostram primeiro o mais recente pela data e hora (não pela ordem de registo,
-- que no histórico importado do KIGEST não é cronológica).
CREATE INDEX IF NOT EXISTS idx_docs_data_hora ON documentos(data_hora);
CREATE INDEX IF NOT EXISTS idx_caixa_aberta ON caixa_sessoes(aberta_em);

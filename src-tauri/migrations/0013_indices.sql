-- Índices para bases grandes (importação do KIGEST e consultas diárias):
-- procuras pela origem do KIGEST e pelas tabelas de ligação dos documentos.
CREATE INDEX IF NOT EXISTS idx_artigos_origem ON artigos(id_origem);
CREATE INDEX IF NOT EXISTS idx_entidades_origem ON entidades(id_origem);
CREATE INDEX IF NOT EXISTS idx_utilizadores_origem ON utilizadores(id_origem);
CREATE INDEX IF NOT EXISTS idx_doc_pag_doc ON documento_pagamentos(documento_id);
CREATE INDEX IF NOT EXISTS idx_rec_docs_recibo ON recibo_documentos(recibo_id);
CREATE INDEX IF NOT EXISTS idx_rec_docs_doc ON recibo_documentos(documento_id);
CREATE INDEX IF NOT EXISTS idx_doc_ret_doc ON documento_retencoes(documento_id);
CREATE INDEX IF NOT EXISTS idx_movstock_doc ON movimentos_stock(documento_id);
CREATE INDEX IF NOT EXISTS idx_lote_mov_lote ON lote_movimentos(lote_id);
CREATE INDEX IF NOT EXISTS idx_lote_mov_doc ON lote_movimentos(documento_id);
CREATE INDEX IF NOT EXISTS idx_cred_util_doc ON credito_utilizacoes(documento_id);
CREATE INDEX IF NOT EXISTS idx_cred_util_cred ON credito_utilizacoes(credito_id);
CREATE INDEX IF NOT EXISTS idx_docs_origem ON documentos(doc_origem_id);
CREATE INDEX IF NOT EXISTS idx_rh_recibo_proc ON rh_recibos(processamento_id);
CREATE INDEX IF NOT EXISTS idx_rh_rec_linhas ON rh_recibo_linhas(recibo_id);
CREATE INDEX IF NOT EXISTS idx_of_ordem_linhas ON of_ordem_linhas(ordem_id);
CREATE INDEX IF NOT EXISTS idx_caixa_mov_sessao ON caixa_movimentos(sessao_id);

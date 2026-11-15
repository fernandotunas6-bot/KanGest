-- Índices para as consultas mais usadas. Medidos numa base com 150 000 documentos (~3 anos de uma loja), com o teste
-- volume_tempos (src-tauri/src/services/tests.rs) e o plano de cada consulta (EXPLAIN QUERY PLAN):
--
-- • vendas de um período (painel, relatórios, síntese): sem estatísticas, o SQLite escolhia o índice
--   (tipo_codigo, serie, numero) e lia todas as vendas desde sempre; com (tipo_codigo, data_emissao) lê só o período.
--   Vendas do mês no painel: 60 ms → 1 ms.
CREATE INDEX IF NOT EXISTS idx_doc_tipo_data ON documentos(tipo_codigo, data_emissao);

-- • documentos de uma sessão de caixa (estado do caixa no POS a cada venda, fecho, lista de fechos): lia a tabela toda.
CREATE INDEX IF NOT EXISTS idx_doc_caixa_sessao ON documentos(caixa_sessao_id) WHERE caixa_sessao_id IS NOT NULL;

-- • contas correntes, vencidos e antiguidade dos saldos: só os documentos por pagar (poucos), por cliente e vencimento.
--   Pendentes de clientes: 207 ms → 5 ms.
CREATE INDEX IF NOT EXISTS idx_doc_pendentes ON documentos(entidade_id, data_vencimento) WHERE estado = 'N' AND valor_pendente > 0;

-- • proformas e documentos a vencer
CREATE INDEX IF NOT EXISTS idx_doc_vencimento ON documentos(data_vencimento) WHERE data_vencimento IS NOT NULL;

-- • extracto e saldo de uma conta da tesouraria
CREATE INDEX IF NOT EXISTS idx_mov_tes_conta ON movimentos_tesouraria(conta_id, data);

-- • avisos automáticos: ao apagar eventos antigos, as entregas apagam-se em cascata (sem índice, uma leitura de toda a
--   tabela por cada evento apagado)
CREATE INDEX IF NOT EXISTS idx_api_entregas_evento ON api_entregas(evento_id);

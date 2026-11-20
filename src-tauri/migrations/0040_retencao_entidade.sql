-- Retenção na fonte habitual de uma entidade (ex.: o cliente retém 6,5% de Imposto Industrial nos serviços): vem
-- escolhida nas facturas e nos recibos dessa entidade (o código é o da tabela retencoes; NULL = nenhuma).
ALTER TABLE entidades ADD COLUMN retencao TEXT;

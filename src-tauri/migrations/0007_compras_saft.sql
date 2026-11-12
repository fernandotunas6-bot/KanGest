-- SAF-T de aquisição (PurchaseInvoices): dados do documento do fornecedor.
-- Hash = 4 caracteres de controlo impressos na factura do fornecedor ("0" quando não aplicável).
ALTER TABLE documentos ADD COLUMN doc_externo_hash TEXT;
-- Data do documento do fornecedor (pode ser anterior à data de registo).
ALTER TABLE documentos ADD COLUMN doc_externo_data TEXT;
-- Tipo de dedução do IVA (MFI, INV, OBC, SERV, IMPT, SCE). Vazio = automático pelo tipo de artigo.
ALTER TABLE documentos ADD COLUMN deducao_tipo TEXT;

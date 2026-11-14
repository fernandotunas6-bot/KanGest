-- Versão da chave com que cada documento foi assinado (HashControl do SAF-T): "0" enquanto o programa
-- não tinha certificado, depois a versão da chave do produtor. Fica fixa depois de gravada.
ALTER TABLE documentos ADD COLUMN hash_versao TEXT;

CREATE TRIGGER trg_doc_hash_versao
BEFORE UPDATE OF hash_versao ON documentos
WHEN OLD.hash_versao IS NOT NULL AND NEW.hash_versao IS NOT OLD.hash_versao AND OLD.importado = 0
BEGIN
    SELECT RAISE(ABORT, 'Documento assinado: a versão da chave não pode ser alterada.');
END;

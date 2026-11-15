-- Protecção dos documentos assinados pelo KanGest (cadeia AGT).
-- Os dados que entram na assinatura ou no impresso fiscal deixam de poder ser alterados ou apagados
-- por engano (ou por um editor de SQLite). A verificação de integridade (Certificação AGT) confirma
-- que estes triggers existem e refaz a cadeia de assinaturas.
-- O histórico importado do KIGEST (importado = 1) fica de fora: tem as assinaturas do sistema antigo e
-- o importador acerta-o ao repetir a importação.

CREATE TRIGGER trg_doc_assinado_upd
BEFORE UPDATE OF tipo_codigo, serie, numero, numero_doc, data_emissao, data_hora, entidade_id, entidade_nome, entidade_nif,
    entidade_endereco, entidade_localidade, doc_origem_id, total_bruto, total_desconto, total_liquido, total_iva, total_retencao,
    total, moeda, cambio, hash, hash_controlo ON documentos
WHEN OLD.hash <> '' AND OLD.importado = 0 AND (
       NEW.tipo_codigo IS NOT OLD.tipo_codigo OR NEW.serie IS NOT OLD.serie OR NEW.numero IS NOT OLD.numero
    OR NEW.numero_doc IS NOT OLD.numero_doc OR NEW.data_emissao IS NOT OLD.data_emissao OR NEW.data_hora IS NOT OLD.data_hora
    OR NEW.entidade_id IS NOT OLD.entidade_id OR NEW.entidade_nome IS NOT OLD.entidade_nome OR NEW.entidade_nif IS NOT OLD.entidade_nif
    OR NEW.entidade_endereco IS NOT OLD.entidade_endereco OR NEW.entidade_localidade IS NOT OLD.entidade_localidade
    OR NEW.doc_origem_id IS NOT OLD.doc_origem_id OR NEW.total_bruto IS NOT OLD.total_bruto OR NEW.total_desconto IS NOT OLD.total_desconto
    OR NEW.total_liquido IS NOT OLD.total_liquido OR NEW.total_iva IS NOT OLD.total_iva OR NEW.total_retencao IS NOT OLD.total_retencao
    OR NEW.total IS NOT OLD.total OR NEW.moeda IS NOT OLD.moeda OR NEW.cambio IS NOT OLD.cambio
    OR NEW.hash IS NOT OLD.hash OR NEW.hash_controlo IS NOT OLD.hash_controlo)
BEGIN
    SELECT RAISE(ABORT, 'Documento assinado: os dados fiscais não podem ser alterados. Corrija com uma nota de crédito ou de débito.');
END;

CREATE TRIGGER trg_doc_anulado
BEFORE UPDATE OF estado ON documentos
WHEN OLD.estado = 'A' AND NEW.estado <> 'A' AND OLD.importado = 0
BEGIN
    SELECT RAISE(ABORT, 'Um documento anulado não pode voltar a ficar válido.');
END;

CREATE TRIGGER trg_doc_assinado_del
BEFORE DELETE ON documentos
WHEN OLD.hash <> '' AND OLD.importado = 0
BEGIN
    SELECT RAISE(ABORT, 'Documento assinado: não pode ser apagado. Anule-o com motivo.');
END;

CREATE TRIGGER trg_linha_assinada_upd
BEFORE UPDATE OF documento_id, linha, artigo_id, referencia, descricao, quantidade, unidade, preco_unitario, desconto_pct,
    valor_desconto, taxa_iva_id, taxa_iva_pct, motivo_isencao, valor_liquido, valor_iva, total ON documento_linhas
WHEN EXISTS (SELECT 1 FROM documentos d WHERE d.id = OLD.documento_id AND d.hash <> '' AND d.importado = 0)
BEGIN
    SELECT RAISE(ABORT, 'Linha de um documento assinado: não pode ser alterada.');
END;

CREATE TRIGGER trg_linha_assinada_del
BEFORE DELETE ON documento_linhas
WHEN EXISTS (SELECT 1 FROM documentos d WHERE d.id = OLD.documento_id AND d.hash <> '' AND d.importado = 0)
BEGIN
    SELECT RAISE(ABORT, 'Linha de um documento assinado: não pode ser apagada.');
END;

CREATE TRIGGER trg_pagamento_assinado_upd
BEFORE UPDATE ON documento_pagamentos
WHEN EXISTS (SELECT 1 FROM documentos d WHERE d.id = OLD.documento_id AND d.hash <> '' AND d.importado = 0)
BEGIN
    SELECT RAISE(ABORT, 'Pagamento de um documento assinado: não pode ser alterado.');
END;

CREATE TRIGGER trg_pagamento_assinado_del
BEFORE DELETE ON documento_pagamentos
WHEN EXISTS (SELECT 1 FROM documentos d WHERE d.id = OLD.documento_id AND d.hash <> '' AND d.importado = 0)
BEGIN
    SELECT RAISE(ABORT, 'Pagamento de um documento assinado: não pode ser apagado.');
END;

CREATE TRIGGER trg_serie_recuar
BEFORE UPDATE OF ultimo_numero, ultimo_hash ON series
WHEN NEW.ultimo_numero < OLD.ultimo_numero
  OR (NEW.ultimo_numero = OLD.ultimo_numero AND OLD.ultimo_numero > 0 AND NEW.ultimo_hash IS NOT OLD.ultimo_hash)
BEGIN
    SELECT RAISE(ABORT, 'A numeração e a cadeia de assinaturas de uma série não podem recuar.');
END;

CREATE TRIGGER trg_serie_del
BEFORE DELETE ON series
WHEN OLD.ultimo_numero > 0
BEGIN
    SELECT RAISE(ABORT, 'Uma série com documentos não pode ser apagada.');
END;

-- chaves públicas usadas antes (para verificar documentos antigos depois de mudar de chave)
CREATE TABLE IF NOT EXISTS agt_chaves (
    id INTEGER PRIMARY KEY,
    chave_publica TEXT NOT NULL UNIQUE,
    origem TEXT NOT NULL,                       -- DESENVOLVIMENTO | IMPORTADA | PRODUTOR
    usada_ate TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);

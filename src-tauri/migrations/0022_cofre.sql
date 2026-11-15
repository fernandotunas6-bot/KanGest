-- Cofre dos segredos guardados na base (chave de assinatura AGT, senha e chaves da Facturação Electrónica, segredos dos
-- avisos automáticos): cifrados com AES-256-GCM (src-tauri/src/cofre.rs). O sal torna a chave da cifra própria de cada
-- base; não é secreto. Os valores antigos, em texto, são cifrados pelo programa no arranque.
INSERT OR IGNORE INTO config (chave, valor) VALUES ('cofre.sal', lower(hex(randomblob(16))));

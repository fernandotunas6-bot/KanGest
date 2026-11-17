-- Verificação em dois passos (TOTP, RFC 6238: Google Authenticator, Microsoft Authenticator…) por utilizador.
ALTER TABLE utilizadores ADD COLUMN totp_segredo TEXT;      -- cifrado (segredos.rs); NULL = sem dois passos
ALTER TABLE utilizadores ADD COLUMN totp_ultimo INTEGER;    -- último período de 30 s aceite: um código não serve duas vezes
ALTER TABLE utilizadores ADD COLUMN totp_recuperacao TEXT;  -- JSON com os resumos SHA-256 dos códigos de recuperação por usar

-- Exigir os dois passos a quem entra pela Internet (acesso por subdomínio); na rede da loja, só a quem os ligou.
INSERT OR IGNORE INTO config (chave, valor) VALUES ('seguranca.dois_passos_internet', '0');

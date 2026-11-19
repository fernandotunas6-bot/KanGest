-- O perfil de cada utilizador, que ele próprio muda (menu do utilizador → O meu perfil): fotografia (data URL JPEG,
-- 256 px) e contactos. O nome já existia; o utilizador (login), o perfil e as permissões continuam com o administrador.
ALTER TABLE utilizadores ADD COLUMN foto TEXT;
ALTER TABLE utilizadores ADD COLUMN email TEXT;
ALTER TABLE utilizadores ADD COLUMN telefone TEXT;

-- A loja de cada utilizador (servidor central com várias livrarias): com um armazém, o utilizador só entra nos postos
-- desse armazém e só vê os documentos, os relatórios e o painel dessa loja. Vazio = todas as lojas (como antes).
ALTER TABLE utilizadores ADD COLUMN armazem_id INTEGER REFERENCES armazens(id);

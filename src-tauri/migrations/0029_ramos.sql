-- Ramos de actividade (Configurações → Empresa → Ramo): o perfil do negócio — livraria, farmácia, moda,
-- electrónica, restauração, prestação de serviços… — e os dados próprios de cada ramo nos artigos
-- (ISBN, autor e editora; princípio activo e receita; tamanho e cor; marca, modelo, n.º de série e garantia…).
-- O ramo escolhido fica em config('empresa.ramo').

-- objecto JSON com os dados do ramo, ex.: {"autor": "Pepetela", "editora": "Mayamba", "ano": 2021}
ALTER TABLE artigos ADD COLUMN atributos TEXT;

-- unidades que os ramos novos usam (serviços à hora, avenças ao mês, calçado aos pares, obra ao m²)
INSERT INTO unidades (descricao, abreviatura) SELECT 'Hora', 'H' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'H');
INSERT INTO unidades (descricao, abreviatura) SELECT 'Dia', 'DIA' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'DIA');
INSERT INTO unidades (descricao, abreviatura) SELECT 'Mês', 'MES' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'MES');
INSERT INTO unidades (descricao, abreviatura) SELECT 'Par', 'PAR' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'PAR');
INSERT INTO unidades (descricao, abreviatura) SELECT 'Metro quadrado', 'M2' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'M2');
INSERT INTO unidades (descricao, abreviatura) SELECT 'Pacote', 'PCT' WHERE NOT EXISTS (SELECT 1 FROM unidades WHERE abreviatura = 'PCT');

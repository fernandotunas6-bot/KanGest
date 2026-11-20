-- Tipo de inventário de cada artigo, para o SAF-T de inventários (Circular n.º 05/GACA/GJ/DSF/AGT/2026):
-- M mercadorias, P matérias-primas, subsidiárias e de consumo, A produtos acabados e intermédios,
-- S subprodutos, desperdícios e refugos, T produtos e trabalhos em curso. Até aqui todos eram mercadorias.
ALTER TABLE artigos ADD COLUMN tipo_inventario TEXT NOT NULL DEFAULT 'M';

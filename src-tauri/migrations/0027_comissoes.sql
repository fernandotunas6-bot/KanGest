-- Comissões de vendedores: percentagem sobre as vendas sem IVA de cada vendedor (o funcionário escolhido no
-- documento). Uma família pode ter a sua própria percentagem, que substitui a do vendedor nas linhas dessa família.
CREATE TABLE comissao_taxas (
    tipo TEXT NOT NULL CHECK (tipo IN ('VENDEDOR', 'FAMILIA')),
    ref_id INTEGER NOT NULL,                  -- funcionarios.id ou familias.id
    pct REAL NOT NULL CHECK (pct >= 0 AND pct <= 100),
    PRIMARY KEY (tipo, ref_id)
);

-- Rubrica do processamento de salários: remuneração sujeita a INSS e a IRT.
INSERT OR IGNORE INTO rh_rubricas (codigo, descricao, tipo, incide_inss, incide_irt, isento_irt_ate, mapa, ordem)
VALUES ('019', 'Comissões de vendas', 'REMUNERACAO', 1, 1, 0, 'outros', 19);

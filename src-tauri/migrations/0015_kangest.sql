-- O produto passou a chamar-se KanGest: nome enviado à AGT na Facturação Electrónica (productId)
UPDATE config SET valor = 'KanGest' WHERE chave = 'fe.produto' AND valor = 'KiGest';

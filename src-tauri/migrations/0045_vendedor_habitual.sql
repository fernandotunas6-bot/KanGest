-- KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Vendedor habitual do cliente: entra nas vendas desse cliente que não indicam outro vendedor
-- (e daí nas comissões), no POS, nas facturas, nas avenças e na API.
ALTER TABLE entidades ADD COLUMN vendedor_id INTEGER REFERENCES funcionarios(id);

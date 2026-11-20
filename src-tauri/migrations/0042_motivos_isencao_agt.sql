-- KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Motivos de isenção do artigo 12.º do CIVA que faltavam na tabela (lista de códigos do SAF-T (AO) da AGT).
-- O M21 é o do ensino (al. l) do n.º 1): é o motivo dos artigos de propinas e emolumentos do modo escola.
-- Só acrescenta: os códigos que já existem ficam como estão (os documentos emitidos referem-nos).
INSERT OR IGNORE INTO motivos_isencao (codigo, descricao) VALUES
    ('M15', 'Isento nos termos da alínea f) do nº1 do artigo 12.º do CIVA'),
    ('M16', 'Isento nos termos da alínea g) do nº1 do artigo 12.º do CIVA'),
    ('M17', 'Isento nos termos da alínea h) do nº1 do artigo 12.º do CIVA'),
    ('M18', 'Isento nos termos da alínea i) do nº1 do artigo 12.º do CIVA'),
    ('M19', 'Isento nos termos da alínea j) do nº1 do artigo 12.º do CIVA'),
    ('M21', 'Isento nos termos da alínea l) do nº1 do artigo 12.º do CIVA'),
    ('M22', 'Isento nos termos da alínea m) do artigo 12.º do CIVA'),
    ('M23', 'Isento nos termos da alínea n) do artigo 12.º do CIVA'),
    ('M24', 'Isento nos termos da alínea o) do artigo 12.º do CIVA');

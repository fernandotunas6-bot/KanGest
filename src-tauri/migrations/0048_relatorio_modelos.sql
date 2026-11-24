-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Modelos do construtor de relatórios: a definição (fonte, período, colunas ou agrupamento, filtros, ordenação,
-- gráfico) em JSON. É de quem o criou; «partilhado» mostra-o a toda a equipa (que o vê se tiver a permissão da fonte).
CREATE TABLE relatorio_modelos (
    id INTEGER PRIMARY KEY,
    nome TEXT NOT NULL,
    fonte TEXT NOT NULL,
    definicao TEXT NOT NULL,
    utilizador_id INTEGER REFERENCES utilizadores(id),
    partilhado INTEGER NOT NULL DEFAULT 0,
    criado_em TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    alterado_em TEXT
);
CREATE INDEX idx_relatorio_modelos_dono ON relatorio_modelos(utilizador_id);

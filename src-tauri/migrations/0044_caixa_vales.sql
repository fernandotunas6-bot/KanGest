-- KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Vales de caixa: uma entrada ou saída de dinheiro com várias linhas (ex.: combustível, água, transporte),
-- a pessoa a quem se entregou (ou de quem se recebeu) e quem a registou, para imprimir o talão com assinatura.
ALTER TABLE caixa_movimentos ADD COLUMN beneficiario TEXT;
ALTER TABLE caixa_movimentos ADD COLUMN utilizador_id INTEGER REFERENCES utilizadores(id);

CREATE TABLE caixa_movimento_linhas (
    id INTEGER PRIMARY KEY,
    movimento_id INTEGER NOT NULL REFERENCES caixa_movimentos(id),
    descricao TEXT NOT NULL,
    rubrica TEXT,
    valor INTEGER NOT NULL CHECK (valor > 0)       -- cêntimos
);
CREATE INDEX idx_caixa_movimento_linhas ON caixa_movimento_linhas(movimento_id);

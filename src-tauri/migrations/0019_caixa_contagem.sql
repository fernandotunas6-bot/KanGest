-- Fecho de caixa: contagem por notas e moedas e fecho cego (o operador conta sem ver o valor esperado).
ALTER TABLE caixa_sessoes ADD COLUMN contagem TEXT;   -- JSON [{"valor": 500000, "quantidade": 3}] (valor da nota/moeda em cêntimos)

-- Notas e moedas do kwanza mostradas na contagem (em Kz, separadas por vírgulas; editáveis em Opções de Venda).
INSERT OR IGNORE INTO config (chave, valor) VALUES
    ('caixa.fecho_cego', '0'),
    ('caixa.notas', '5000,2000,1000,500,200'),
    ('caixa.moedas', '200,100,50,20,10,5,2,1,0.5');

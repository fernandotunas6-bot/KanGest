-- © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

-- Venda rápida no POS: no fim da venda o documento imprime-se sozinho e o posto volta logo à venda seguinte, em vez de
-- abrir a pré-visualização (mais um clique para imprimir e outro para fechar). Só muda os postos que ainda têm as
-- opções que vinham por omissão; quem quiser a pré-visualização volta a ligá-la em Configurar posto.
UPDATE postos
SET opcoes = json_set(opcoes, '$.imprimir_auto', json('true'), '$.mostrar_opcoes_impressao', json('false'))
WHERE json_valid(opcoes)
  AND json_extract(opcoes, '$.imprimir_auto') = 0
  AND json_extract(opcoes, '$.mostrar_opcoes_impressao') = 1;

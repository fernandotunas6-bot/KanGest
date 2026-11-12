# Página de vendas (KanGest, SIGA Plus, equipamento POS, projectos)

Página estática, sem dependências: abre-se directamente (`index.html`) ou publica-se em qualquer alojamento
(GitHub Pages, o alojamento do domínio, Cloudflare Pages…). Tudo o que usa está nesta pasta.

```
site/
  index.html          a página (HTML, CSS e o pouco JavaScript que precisa)
  assets/             logótipos (KanGest, SIGA Plus) e imagens
```

## Antes de publicar: contactos

No fim do `index.html`, no bloco `CONFIG`:

| Campo | Para quê |
|---|---|
| `email` | Email de vendas. Os pedidos de download chegam a este email e a pessoa recebe automaticamente os links (serviço FormSubmit). No primeiro pedido o FormSubmit envia um email de confirmação para este endereço: é preciso confirmar uma vez. |
| `whatsapp` | Número com indicativo e só algarismos (ex.: `244923000000`). Activa o botão «Falar por WhatsApp» e os «Pedir orçamento». |
| `downloads` | Links dos instaladores. Estão os da versão 1.0.0; numa versão nova, mude o número nos três links. |

Sem email configurado, o formulário mostra logo os links na página (não se perde nenhuma venda).

## Imagens

- As imagens do KanGest foram tiradas com dados de exemplo (empresa fictícia), nunca com dados de clientes.
- O equipamento usa ilustrações próprias; os modelos (Epson, Xprinter, Honeywell, Zebra) são só exemplos de
  equipamento ESC/POS e USB compatível. Os preços estão «sob consulta».
- As imagens do SIGA Plus e da Livraria Adventista são fotografias das páginas públicas `portal-siga.com` e
  `livrariaadventista.ao`. Antes de publicar, confirme com o cliente que pode mostrar a loja como referência.

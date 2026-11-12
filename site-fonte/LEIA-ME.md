# Site do KanGest

Fontes do site publicado em https://kangest.pages.dev (cópia em https://fernandotunas6-bot.github.io/KanGest/).
A pasta `site/` é gerada a partir desta: não se edita à mão.

```
site-fonte/
  config.json       versão, links dos instaladores, contactos de vendas, endereços do SIGA e da livraria
  base.html         cabeçalho, menu e rodapé comuns a todas as páginas
  estilo.css        estilos (visual do KanGest)
  site.js           menu do telemóvel, separadores, formulários de download e de contacto
  paginas/          uma página por ficheiro (1.ª linha: comentário com título, descrição e menu activo)
  assets/           logótipos, ecrãs do KanGest (dados de exemplo) e ilustrações do equipamento
```

## Gerar e publicar

```bash
python3 scripts/gerar-site.py                                        # gera site/
CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-site-cloudflare.py   # publica no Cloudflare Pages (projecto kangest)
```

A cópia no GitHub Pages actualiza-se enviando `site/` para o ramo `gh-pages`
(`git subtree split --prefix site`, ver o README do repositório).

## Contactos de vendas

Em `config.json`, `contacto.email` e `contacto.whatsapp` (só algarismos, com o indicativo: `244…`). Com o email, os
pedidos de download e de contacto chegam a esse endereço pelo FormSubmit e quem pede o download recebe os links
automaticamente (o FormSubmit pede uma confirmação no primeiro envio). Sem contactos, a página de contacto mostra
alternativas em vez do formulário, e o pedido de download mostra os links na própria página.

## Nova versão do KanGest

Em `config.json`, mude `versao`, os três links de `downloads` e os tamanhos; gere e publique.

## Imagens

- Os ecrãs do KanGest foram tirados com a empresa fictícia de demonstração, nunca com dados de clientes.
- O equipamento usa ilustrações próprias; os modelos indicados são exemplos de equipamento ESC/POS e USB compatível.
- As imagens do SIGA Plus e da Livraria Adventista são fotografias das páginas públicas `www.portal-siga.com` e
  `livrariaadventista.ao`.

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

## Página «Aderir» (planos e pedido de adesão)

`paginas/aderir.html`: os passos da adesão, os três planos (Essencial, Comércio, Empresa) e o formulário com a empresa,
o NIF, os postos, o contacto, a actividade, o programa actual e o que mais precisa. Não há pagamentos no site.

- **Preços**: vêm do servidor de licenças (`GET {web.licencas}/v1/planos`), definidos no painel do produtor em
  **Planos e preços**. Com o preço a zero, o cartão diz «Sob consulta». Mudar o preço no painel muda o site sem gerar
  nada de novo.
- **Para onde vai o pedido**: com `contacto.email`, chega por email (FormSubmit, com o assunto «Adesão ao KanGest:
  empresa (plano)»); sem email, abre o WhatsApp de `contacto.whatsapp` com o pedido já escrito.
- `aderir.html?plano=essencial` (ou `comercio`, `empresa`) chega com o plano escolhido.

## Página de entrada (KanGest web)

`paginas/entrar.html` e o botão **Entrar** do topo: o nome da empresa leva ao KanGest web dela. Em `config.json`,
`web.licencas` é o servidor de licenças que dá os endereços (também entra na CSP, em `connect-src`) e `web.dominio` o
domínio dos endereços das empresas (vazio até haver domínio). Ver [docs/CLOUDFLARE.md](../docs/CLOUDFLARE.md) §3.

## Nova versão do KanGest

Em `config.json`, mude `versao` e os tamanhos; gere e publique. Os links de `downloads` levam `{versao}` e apontam
para o próprio site (`/actualizacoes/…`): ao publicar, a GitHub Actions (`site.yml`) copia da release `v<versão>` os
instaladores e o manifesto das actualizações para `site/actualizacoes` (não se guardam no repositório) e confirma no
fim que o site os serve. Por isso a release tem de existir antes de se publicar o site com a versão nova.

## Imagens

- Os ecrãs do KanGest foram tirados com a empresa fictícia de demonstração, nunca com dados de clientes.
- Para os tirar de novo (interface nova), com o servidor de demonstração e o frontend a correr
  (ver [scripts/auditoria/README.md](../scripts/auditoria/README.md)):
  `node scripts/auditoria/capturas-site.cjs` (1440×900) e `python3 scripts/auditoria/miniaturas-site.py` (720×450).
  O site só se gera e publica com a versão nova, para mostrar o que se descarrega.
- O equipamento usa ilustrações próprias; os modelos indicados são exemplos de equipamento ESC/POS e USB compatível.
- As imagens do SIGA Plus e da Livraria Adventista são fotografias das páginas públicas `www.portal-siga.com` e
  `livrariaadventista.ao`.

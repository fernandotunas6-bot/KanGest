# Modo web e aplicação instalável (PWA)

O KanGest corre em **Windows e macOS** (aplicação instalada) e, a partir do servidor da loja, em **qualquer
browser**: telemóveis e tablets **Android** e **iPhone/iPad**, terminais POS Android (Sunmi, PAX, iMin…),
Chromebooks e outros computadores — sem instalar nada. É o mesmo programa, com os mesmos ecrãs, permissões e
documentos certificados; os dados ficam só no servidor.

| Onde | Como | Instalação |
|---|---|---|
| Windows 10/11 | Aplicação KanGest | Instalador `.exe` ([INSTALACAO.md](INSTALACAO.md)) |
| macOS (Apple Silicon e Intel) | Aplicação KanGest | `.dmg` |
| Android (telemóvel, tablet, terminal POS) | Chrome / Samsung Internet / Edge | Nenhuma — ou «Instalar aplicação» (PWA, com HTTPS) |
| iPhone / iPad | Safari | Nenhuma — ou «Partilhar → Adicionar ao ecrã principal» |
| Linux, ChromeOS, outro PC | Qualquer browser actual | Nenhuma |

## Como funciona

1. Um computador da loja é o **servidor** (Configurações → Rede → Servidor). Por omissão, o servidor também
   entrega a aplicação ao browser: **«Permitir postos pelo browser»** (pode desligar-se).
2. No aparelho, abre-se `http://IP-DO-SERVIDOR:7799` — ou aponta-se a câmara ao **código QR** que o servidor
   mostra em Configurações → Rede.
3. Na primeira vez pede-se a **chave de rede** e o **n.º do posto**. Depois entra-se com utilizador e senha (ou PIN)
   como em qualquer posto.

Os ficheiros da aplicação (HTML, JavaScript, CSS, ícones) vão embutidos no executável do servidor e saem dele — o
browser recebe sempre a versão do servidor, e uma actualização do servidor chega a todos os aparelhos ao
recarregar a página.

### Segurança

- Os ficheiros da aplicação não têm dados. Cada pedido de dados vai **cifrado e autenticado** com a chave de rede
  (o mesmo protocolo dos postos instalados — [REDE.md](REDE.md)): AES-256-GCM, com a chave derivada por
  PBKDF2-HMAC-SHA256 (100 000 iterações) e o sal do servidor, hora e identificador único em cada pedido.
- A **chave de rede não fica guardada** no aparelho, só a chave de cifra derivada dela. «Desligar este aparelho»
  (Configurações → Rede, no browser) apaga-a.
- Num endereço `http://` da rede local o browser não dá o WebCrypto; o KanGest usa a mesma cifra em JavaScript
  (bibliotecas auditadas `@noble/ciphers` e `@noble/hashes`). Em HTTPS usa o WebCrypto do browser.
- Mudar a chave de rede no servidor desliga todos os aparelhos (voltam a pedir a chave).
- Cada aparelho tem a sua sessão no servidor (como os postos); a sessão mantém-se ao recarregar a página.
- O servidor responde com `Content-Security-Policy` (só código do próprio servidor), `X-Frame-Options: DENY`,
  `nosniff` e sem `Referer`.
- O que mexe na máquina onde corre — configuração de rede e de Internet, actualizações, gravar ficheiros no disco,
  abrir o browser do sistema — **não se faz pela rede**: o servidor recusa esses pedidos de outro posto ou do
  browser.

### O que muda no browser

| Função | Na aplicação instalada | No browser |
|---|---|---|
| Exportar CSV / Excel | Grava na pasta Transferências do computador | Descarrega pelo browser para o aparelho |
| SAF-T, cópia de segurança exportada | Grava no servidor | Gera no servidor e descarrega para o aparelho |
| Talão (impressora térmica ESC/POS) | Impressora do posto | Impressora do posto, pelo servidor: de rede (IP:9100) ou ligada ao servidor |
| Talão / A4 sem impressora ESC/POS | Diálogo de impressão | Diálogo de impressão do browser (nos terminais Sunmi, a impressora interna aparece como impressora do Android) |
| WhatsApp, email | WhatsApp do sistema | Separador novo / aplicação WhatsApp do telemóvel |
| Rede, Internet, actualizações | Configura-se aqui | Só no computador servidor |

## Instalar como aplicação (PWA)

Com **HTTPS**, o KanGest instala-se como aplicação: ícone próprio no ecrã principal, abre em ecrã inteiro sem a
barra do browser, atalhos para o POS, Artigos e Documentos, e a «casca» da aplicação fica guardada no aparelho
(abre depressa e, sem rede, explica que não chega ao servidor em vez da página de erro do browser). Os **dados nunca
ficam guardados** no aparelho: o service worker (`sw.js`) não toca nos pedidos `/rpc`.

- **Android / Chrome / Edge**: menu ⋮ → **Instalar aplicação**, ou o botão **Instalar como aplicação** em
  Configurações → Rede.
- **iPhone / iPad**: Safari → **Partilhar** → **Adicionar ao ecrã principal** (funciona também por `http://` na rede
  local, em ecrã inteiro).
- Sem HTTPS (rede local por `http://`), o Android cria um atalho no ecrã principal; o resto funciona igual.

### HTTPS e acesso fora da loja (túnel da Cloudflare)

O túnel da Cloudflare dá HTTPS e um endereço próprio (ex.: `https://loja.empresa.ao`) sem abrir portas no router —
é também o caminho para usar o KanGest fora da loja (dono a ver vendas em casa, vendedor externo, segunda loja):

1. Zero Trust → **Networks → Tunnels** → o túnel da loja (ver [CLOUDFLARE.md](CLOUDFLARE.md)) → **Public hostname**:
   `loja.empresa.ao` → Service **HTTP** → `localhost:7799`.
2. **Obrigatório**: Zero Trust → **Access → Applications → Self-hosted** para `loja.empresa.ao`, com uma política
   **Allow** só para os emails da equipa (código de uso único por email, ou o Google/Microsoft da empresa). Assim
   ninguém chega sequer à página sem passar pela Cloudflare; a chave de rede e a senha de cada utilizador continuam
   a ser pedidas depois.
3. Regras recomendadas: **WAF** a bloquear países onde a empresa não trabalha, **Rate limiting** em `/rpc`
   (ex.: 600 pedidos/minuto por IP), **Bot Fight Mode**, **Always Use HTTPS** e TLS mínimo 1.2.

> Sem o Access à frente, não publique a porta 7799 na Internet.

## Terminais POS Android (ex.: Sunmi)

- Abra o Chrome do terminal no endereço do servidor e instale (ou adicione ao ecrã principal). Em
  **Postos e impressão**, crie um posto para o terminal (série, armazém, caixa) e use «Usar neste aparelho».
- **Impressora interna**: se o terminal tiver o serviço de impressão do fabricante (aparece como impressora do
  Android), o talão sai pelo diálogo de impressão — escolha o formato **talão 58 mm** ou **80 mm** no posto.
  Em alternativa, uma impressora térmica de rede (IP) configurada no posto imprime directamente, pelo servidor.
- **Leitor de códigos** do terminal: funciona como teclado — lê-se directamente no campo de pesquisa do POS.

## Comparação com POS só na nuvem

Programas como o Vendus correm na nuvem: trabalham no browser (Chrome) e em aplicações para Android e iOS, mas
**precisam sempre de Internet** — sem ela não se vende. No KanGest o servidor é um computador da loja: os
aparelhos ligam-se pela **rede local (Wi-Fi), mesmo sem Internet**, e a Internet só é precisa para a facturação
electrónica, emails e o acesso de fora (pelo túnel).

## Para programadores

- Servidor: `src-tauri/src/web.rs` (ficheiros, MIME, gzip, cabeçalhos) e `src-tauri/src/rede.rs` (`GET /rede/info`,
  `POST /rpc`, `GET /` com `Accept: text/html` → aplicação; sem esse cabeçalho → informação JSON, para os postos de
  versões anteriores).
- Browser: `src/lib/api.ts` (ligação, cifra, métodos que no browser são locais), `src/pages/LigarWeb.tsx`,
  `src/lib/pwa.ts`, `public/manifest.webmanifest`, `public/sw.js`, `public/icons/`.
- Testar no browser sem compilar a aplicação: `npm run build`, depois
  `KANGEST_WEB_DIR=$PWD/dist cargo test --manifest-path src-tauri/Cargo.toml servidor_demo -- --ignored --nocapture`
  e abrir `http://127.0.0.1:7799` (chave `auditoria123`, utilizador `admin`, senha `1234`).

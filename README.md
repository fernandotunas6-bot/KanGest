# KanGest — Gestão Comercial e POS (Tauri 2)

Sistema de facturação, stock, caixa e ponto de venda para Windows e macOS, inspirado no KIGEST GC/POS (Delphi/SQL Server).
A análise do sistema antigo e o roteiro estão em [docs/ANALISE-KIGEST.md](docs/ANALISE-KIGEST.md).

## Baixar

- **Página do produto:** https://kangest.pages.dev (cópia em https://fernandotunas6-bot.github.io/KanGest/)
- **Windows 10/11 (64 bits):** [KanGest_1.2.0_x64-setup.exe](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.2.0/KanGest_1.2.0_x64-setup.exe)
- **macOS Apple Silicon (M1–M4):** [KanGest_1.2.0_aarch64.dmg](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.2.0/KanGest_1.2.0_aarch64.dmg)
- **macOS Intel:** [KanGest_1.2.0_x64.dmg](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.2.0/KanGest_1.2.0_x64.dmg)
- **Todas as versões:** https://github.com/fernandotunas6-bot/KanGest/releases

Outros produtos na mesma página: [SIGA Plus](https://www.portal-siga.com) (gestão escolar) e o projecto da
[Livraria Adventista](https://livrariaadventista.ao). O site tem as fontes em [site-fonte/](site-fonte/) (páginas, estilos, `config.json` com versão, downloads e contactos)
e gera-se com `python3 scripts/gerar-site.py` para [site/](site/). Publica-se no Cloudflare Pages (projecto `kangest`) com
`CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-site-cloudflare.py`, ou sozinho a cada mudança no `main` (`.github/workflows/site.yml`,
com o segredo `CLOUDFLARE_API_TOKEN`; ver [docs/CLOUDFLARE.md](docs/CLOUDFLARE.md)); a cópia no GitHub Pages vem do ramo `gh-pages`.

## Requisitos

- Node.js 22 e npm
- Rust (stable) — https://rustup.rs
- **Windows:** Microsoft C++ Build Tools e WebView2 (já vem no Windows 10/11)
- **macOS:** Xcode Command Line Tools (`xcode-select --install`)

## Desenvolvimento

```bash
npm install
npm run tauri dev
```

Na primeira execução aparece o assistente de configuração (empresa + administrador).
A base de dados SQLite fica na pasta de dados da aplicação:

- Windows: `%APPDATA%\ao.kigest.app\kigest.db`
- macOS: `~/Library/Application Support/ao.kigest.app/kigest.db`

## Gerar instaladores

```bash
npm ci
npm run tauri build -- --bundles nsis          # Windows → src-tauri/target/release/bundle/nsis/KanGest_1.2.0_x64-setup.exe
CI=true npm run tauri build -- --bundles app,dmg   # macOS → src-tauri/target/release/bundle/dmg/*.dmg
```

Cada instalador tem de ser gerado no respectivo sistema operativo ou no GitHub Actions (`.github/workflows/build.yml`).
O passo a passo, incluindo a instalação no PC do KIGEST e a importação dos dados, está em [docs/BUILD.md](docs/BUILD.md).
O guia para quem instala (requisitos, qual instalador, postos em rede, actualizações e problemas comuns) está em
[docs/INSTALACAO.md](docs/INSTALACAO.md). A base de dados (configuração, migrações, índices e como medir o desempenho)
está em [docs/BASE-DE-DADOS.md](docs/BASE-DE-DADOS.md).
A comparação entre o KIGEST e o KanGest (funções e dados) está em [docs/COMPARACAO-KIGEST.md](docs/COMPARACAO-KIGEST.md).
O equipamento do posto de venda (impressora térmica, gaveta, leitor, visor de cliente, terminal táctil) está em [docs/EQUIPAMENTO-POS.md](docs/EQUIPAMENTO-POS.md).
Os **ramos de actividade** (livraria, farmácia, moda, electrónica, restauração, serviços, oficina…) estão em [docs/RAMOS.md](docs/RAMOS.md).
O **modo web** (telemóveis e tablets Android e iPhone/iPad, terminais POS Android, outros computadores — no browser, sem
instalar, e instalável como aplicação PWA) está em [docs/WEB-PWA.md](docs/WEB-PWA.md).
A **API de integração** (loja online, contabilidade, BI e outras aplicações, com chaves de acesso) está em [docs/API.md](docs/API.md)
e a especificação OpenAPI em [docs/openapi.json](docs/openapi.json). O estudo do que falta face aos grandes POS está em
[docs/ANALISE-POS-MERCADO.md](docs/ANALISE-POS-MERCADO.md). A protecção com a Cloudflare (túnel e Access para a API, servidor de
licenças e site) está em [docs/CLOUDFLARE.md](docs/CLOUDFLARE.md). A cifra e a protecção do programa (ligação entre postos,
selo da licença e do executável) estão em [docs/SEGURANCA.md](docs/SEGURANCA.md). O **motor central de dados** (construtor
de relatórios, relatórios prontos, comparação de períodos e qualidade dos dados) está em [docs/MOTOR-DE-DADOS.md](docs/MOTOR-DE-DADOS.md).
A **conciliação bancária** (extracto do banco × movimentos da conta) está em [docs/CONCILIACAO.md](docs/CONCILIACAO.md).
As **regras de aprovação** (o PIN de quem pode aprovar anulações, saídas, vales e notas de crédito) estão em [docs/APROVACOES.md](docs/APROVACOES.md).

## Testes

```bash
cd src-tauri && cargo test     # inclui a verificação da assinatura AGT
npm run typecheck
```

O CI (`.github/workflows/ci.yml`) corre em cada push e pull request: `npm run typecheck`, `cargo clippy -- -D warnings`
e `cargo test`, também numa compilação com uma chave de produtor de teste. No `build.yml`, um teste a falhar trava a
publicação dos instaladores.

## Atalhos do POS (iguais ao KIGEST POS)

| Tecla | Acção |
|---|---|
| Enter | Ler código de barras / referência (`3*código` = 3 unidades) |
| F2 | Lista de artigos (pesquisa) |
| F3 | Quantidade (teclado numérico) |
| F4 | Escolher cliente |
| F5 | Alterar dados do cliente |
| F6 | Desconto de linha |
| F7 | Alterar preço |
| F8 | Desconto global (% ou Kz) |
| F9 | Descrição de linha |
| Del | Apagar linha |
| F10 / F11 | Suspender documento / abrir suspensos |
| F12 | Pagamento |
| ↑ / ↓ | Seleccionar linha (com a pesquisa vazia) |

Operações sem permissão (preço, desconto acima do limite, apagar linha…) pedem o **PIN de um supervisor**.
A matriz de cobertura face ao KIGEST original está em [docs/COBERTURA.md](docs/COBERTURA.md).
A comparação com o Primavera, o Odoo e outros sistemas (interface, lacunas e roteiro) está em
[docs/ANALISE-ERP-REFERENCIA.md](docs/ANALISE-ERP-REFERENCIA.md).

## Atalhos da gestão

| Tecla | Acção |
|---|---|
| Ctrl+K (⌘K no Mac) | Pesquisar ou ir para: ecrãs, documentos, clientes, NIF e artigos |
| ↑ / ↓ e Enter | Escolher e abrir um resultado da pesquisa |
| Esc | Fechar a pesquisa, menus e janelas |

## Estrutura

```
src/                     React (páginas, componentes, impressão)
src-tauri/src/
  lib.rs                 arranque, comando rpc
  rede.rs                postos em rede (servidor / cliente na rede local)
  api.rs                 API de integração para outros sistemas (HTTP, chaves de acesso)
  services/              auth, catálogo, entidades, documentos, caixa, relatórios
  agt.rs                 assinatura RSA-SHA1 encadeada
  db.rs                  SQLite (WAL) + migrações
src-tauri/migrations/    esquema e dados base (IVA, isenções, tipos de documento)
```

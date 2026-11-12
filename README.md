# KanGest — Gestão Comercial e POS (Tauri 2)

Sistema de facturação, stock, caixa e ponto de venda para Windows e macOS, inspirado no KIGEST GC/POS (Delphi/SQL Server).
A análise do sistema antigo e o roteiro estão em [docs/ANALISE-KIGEST.md](docs/ANALISE-KIGEST.md).

## Baixar

- **Página do produto:** https://kangest.pages.dev (cópia em https://fernandotunas6-bot.github.io/KanGest/)
- **Windows 10/11 (64 bits):** [KanGest_1.0.0_x64-setup.exe](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.0.0/KanGest_1.0.0_x64-setup.exe)
- **macOS Apple Silicon (M1–M4):** [KanGest_1.0.0_aarch64.dmg](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.0.0/KanGest_1.0.0_aarch64.dmg)
- **macOS Intel:** [KanGest_1.0.0_x64.dmg](https://github.com/fernandotunas6-bot/KanGest/releases/download/v1.0.0/KanGest_1.0.0_x64.dmg)
- **Todas as versões:** https://github.com/fernandotunas6-bot/KanGest/releases

Outros produtos na mesma página: [SIGA Plus](https://www.portal-siga.com) (gestão escolar) e o projecto da
[Livraria Adventista](https://livrariaadventista.ao). A página está em [site/](site/) e publica-se no Cloudflare Pages (projecto `kangest`) com
`CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-site-cloudflare.py`; a cópia no GitHub Pages vem do ramo `gh-pages`.

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
npm run tauri build -- --bundles nsis          # Windows → src-tauri/target/release/bundle/nsis/KanGest_1.0.0_x64-setup.exe
CI=true npm run tauri build -- --bundles app,dmg   # macOS → src-tauri/target/release/bundle/dmg/*.dmg
```

Cada instalador tem de ser gerado no respectivo sistema operativo ou no GitHub Actions (`.github/workflows/build.yml`).
O passo a passo, incluindo a instalação no PC do KIGEST e a importação dos dados, está em [docs/BUILD.md](docs/BUILD.md).
A comparação entre o KIGEST e o KanGest (funções e dados) está em [docs/COMPARACAO-KIGEST.md](docs/COMPARACAO-KIGEST.md).
O equipamento do posto de venda (impressora térmica, gaveta, leitor, visor de cliente, terminal táctil) está em [docs/EQUIPAMENTO-POS.md](docs/EQUIPAMENTO-POS.md).

## Testes

```bash
cd src-tauri && cargo test     # inclui a verificação da assinatura AGT
npm run typecheck
```

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

## Estrutura

```
src/                     React (páginas, componentes, impressão)
src-tauri/src/
  lib.rs                 arranque, comando rpc
  services/              auth, catálogo, entidades, documentos, caixa, relatórios
  agt.rs                 assinatura RSA-SHA1 encadeada
  db.rs                  SQLite (WAL) + migrações
src-tauri/migrations/    esquema e dados base (IVA, isenções, tipos de documento)
```

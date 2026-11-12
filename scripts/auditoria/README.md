# Auditoria visual

Permite ver e fotografar todos os ecrãs do KanGest num browser sem janela, com dados de exemplo,
sem a aplicação desktop e sem permissões de gravação de ecrã.

1. Servidor de demonstração (base temporária com dados de exemplo, porta 7799):

   ```bash
   cd src-tauri && cargo test --offline servidor_demo -- --ignored --nocapture
   ```

2. Frontend ligado a esse servidor (porta 1422):

   ```bash
   VITE_RPC_HTTP=/rpc VITE_RPC_CHAVE=auditoria123 VITE_RPC_TOKEN=auditoria npx vite --config scripts/auditoria/vite.config.mjs
   ```

3. Chrome sem janela com porta de depuração:

   ```bash
   "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --remote-debugging-port=9333 \
     --user-data-dir=/tmp/kigest-chrome --hide-scrollbars --window-size=1366,820 about:blank &
   ```

4. Capturas (e erros da consola de cada ecrã):

   ```bash
   node scripts/auditoria/capturas.mjs /tmp/aud "/=painel" "/pos=pos@texto:Arroz" "/configuracoes?aba=rede=rede"
   W=1280 H=720 node scripts/auditoria/capturas.mjs /tmp/aud "/pos=pos-pequeno"
   python3 scripts/auditoria/folha.py /tmp/aud/folha.png /tmp/aud/painel.png /tmp/aud/pos.png
   ```

Para ver como uma página sai na impressora, termine as acções com `@impressao` (ex.: `"/documentos=nota@texto:FR 2026/6@clique:Nota de entrega@impressao"`).

O caminho HTTP do frontend (`src/lib/api.ts`) só existe em desenvolvimento e só com estas variáveis.

# Protecção com a Cloudflare

O KanGest usa a Cloudflare em três sítios. Uma parte da protecção já está no código e vai com cada versão; a outra
liga-se no painel da Cloudflare (dash.cloudflare.com e one.dash.cloudflare.com, para o Zero Trust).

| Serviço | Recursos da Cloudflare | Já no código | Ligar no painel |
|---|---|---|---|
| Site (`kangest.pages.dev`) | Pages, HTTPS, Web Analytics | Cabeçalhos de segurança (`_headers`): CSP com o hash do único script em linha, HSTS, `nosniff`, sem `iframe` | Domínio próprio, Web Analytics |
| Servidor de licenças (`kangest-licencas…workers.dev`) | Workers, D1, Access | Limite de pedidos por IP, Cloudflare Access à frente do painel, cabeçalhos de segurança | Domínio próprio, aplicação do Access para `/admin` |
| API do KanGest de cada cliente | Tunnel, Access (Zero Trust), WAF, Rate Limiting | Validação do JWT do Access, IP real do cliente, chaves de API | Túnel, aplicação do Access com tokens de serviço, regras WAF |
| KanGest no browser fora da loja (PWA) | Tunnel, Access (email/IdP), WAF, Rate Limiting | Pedidos cifrados com a chave de rede, CSP, sessão por aparelho | Hostname para `localhost:7799`, aplicação do Access para a equipa ([WEB-PWA.md](WEB-PWA.md)) |

## 1. API do KanGest pela Internet (túnel + Access)

O túnel liga o computador do KanGest à Cloudflare **de dentro para fora**: não se abre nenhuma porta no router e a
ligação é cifrada (HTTPS do lado de fora). O Access decide quem passa antes de o pedido chegar ao KanGest, e o
KanGest volta a confirmar essa autorização.

### No KanGest

**Sistema → API e Integrações**: ligue a API, porta 7800, acesso **«Só desta máquina»** e crie uma chave para cada
integração ([API.md](API.md)).

### Túnel

1. Zero Trust → **Networks → Tunnels → Create a tunnel** → *Cloudflared*, com o nome da loja (ex.: `kangest-loja`).
2. Instale o conector no computador do KanGest com o comando que o painel mostra (instala-se como serviço do sistema e
   arranca sozinho):
   - Windows (como administrador): `cloudflared.exe service install <TOKEN>`
   - macOS: `sudo cloudflared service install <TOKEN>`
3. **Public hostname**: `api.empresa.ao` (um domínio que esteja na Cloudflare) → Service **HTTP** → `localhost:7800`.

### Access (só passam as integrações autorizadas)

1. Zero Trust → **Access → Service Auth → Service Tokens → Create**: um token por integração (ex.: «Loja online»).
   Guarde o *Client ID* e o *Client Secret*.
2. Zero Trust → **Access → Applications → Add → Self-hosted**: domínio `api.empresa.ao`. Política com a acção
   **Service Auth**, a incluir os tokens de serviço das integrações.
3. Copie o **Application Audience (AUD) Tag** da aplicação e o **domínio da equipa** (Zero Trust → Settings →
   *Team domain*, ex.: `empresa.cloudflareaccess.com`) para **API e Integrações → Cloudflare Access** no KanGest.

A partir daí, cada pedido que chega pela Cloudflare tem de trazer o JWT do Access, confirmado pelo KanGest
(assinatura RS256 com as chaves públicas da equipa, AUD, emissor e validade), **além** da chave de API. Os pedidos da
rede local continuam só com a chave. A integração envia:

```bash
curl https://api.empresa.ao/api/v1 \
  -H "CF-Access-Client-Id: <id do token de serviço>" \
  -H "CF-Access-Client-Secret: <segredo do token de serviço>" \
  -H "Authorization: Bearer kg_…"
```

### Regras da zona (domínio da empresa)

- **WAF → Custom rules**: bloquear em `api.empresa.ao` o que não comece por `/api/v1`; se as integrações forem só de
  Angola, bloquear os outros países.
- **WAF → Rate limiting rules**: por exemplo, 300 pedidos por minuto por IP em `/api/v1/` (o KanGest também limita
  por chave).
- **Security → Bots**: *Bot Fight Mode*.
- **SSL/TLS**: *Always Use HTTPS*, *Minimum TLS Version* 1.2, HSTS.

O KanGest regista o IP real de quem pede (`CF-Connecting-IP`) e usa-o nos IPs permitidos de cada chave e no
bloqueio por chaves erradas; o Access tem o seu próprio registo (Zero Trust → Logs).

## 2. Servidor de licenças

**Já no código** (`servidor-licencas/src/worker.js`):

- Limite de pedidos por IP e por hora, guardado no D1 (tabela `limites`): 20 pedidos de registo (cada um envia um
  email), 60 confirmações de código, 600 renovações e 30 tentativas falhadas no painel; depois, `429`.
- Cloudflare Access à frente do painel: com `ACCESS_EQUIPA` e `ACCESS_AUD` definidos, `/admin` e `/admin/api/…` só
  respondem a pedidos com o JWT do Access (cabeçalho ou cookie `CF_Authorization`), e o token do painel continua a
  ser pedido. As rotas públicas (`/v1/…`), usadas pelo KanGest, não mudam.
- Cabeçalhos de segurança em todas as respostas (HSTS, `nosniff`, `no-referrer`, CSP no painel).

**No painel da Cloudflare:**

1. Workers → `kangest-licencas` → **Settings → Domains & Routes → Add custom domain**: ex. `licencas.kangest.ao`.
2. Zero Trust → **Access → Applications → Add → Self-hosted**: domínio `licencas.kangest.ao`, caminho `admin`.
   Política *Allow* com o email do produtor (código por email ou conta Google).
3. Publique com o AUD e a equipa:

   ```bash
   CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-licencas.py --access-equipa empresa.cloudflareaccess.com --access-aud <AUD>
   ```

   O painel passa a abrir em `https://licencas.kangest.ao/admin` depois de entrar pelo Access; pelo endereço
   `workers.dev` deixa de abrir.

## 3. Site

- `python3 scripts/gerar-site.py` gera `site/_headers` com a CSP (o hash SHA-256 do script de configuração em linha,
  o formsubmit.co dos formulários e a Web Analytics da Cloudflare), HSTS, `X-Frame-Options: DENY`, `nosniff`,
  `Referrer-Policy` e `Permissions-Policy`. O `publicar-site-cloudflare.py` envia-o como configuração do Pages.
- No painel: Pages → `kangest` → **Metrics → Web Analytics** (sem cookies; a CSP já o permite) e **Custom domains**.
- **Publicação automática** (`.github/workflows/site.yml`): cada push no `main` que mude `site-fonte/`, `site/` ou os
  scripts do site gera o site e publica-o no Pages; também se corre à mão em **Actions → Site → Run workflow**. No fim
  confirma que `kangest.pages.dev` mostra a versão do `site-fonte/config.json` e os cabeçalhos de segurança.
  Activa-se uma vez: crie um token em **My Profile → API Tokens** com «Account · Cloudflare Pages · Edit» e
  «Account · Account Settings · Read» e guarde-o no GitHub em **Settings → Secrets and variables → Actions** como
  `CLOUDFLARE_API_TOKEN`. Sem ele, o workflow só avisa.

## 4. Próximos recursos

- **R2**: cópia de segurança cifrada na nuvem (o KanGest envia a cópia diária, cifrada no computador do cliente).
- **Turnstile** no formulário de contacto do site (precisa de um ponto que confirme o token antes de enviar).
- **Queues** e Workers para distribuir os avisos automáticos da API a muitos destinos (hoje saem do próprio KanGest).
- **Email Routing** para os endereços do domínio.

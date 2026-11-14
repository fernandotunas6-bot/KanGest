# Chave do produtor, certificado AGT e Facturação Electrónica

O software certificado assina cada documento com a **chave privada do produtor** (RSA-SHA1, Decreto Executivo 74/19);
a AGT confirma as assinaturas com a chave pública entregue na certificação. A mesma ideia vale para a Facturação
Electrónica: as credenciais dos serviços e a chave de software são do produtor, não de cada cliente.

Por isso estes dados **não ficam na base de dados dos clientes** (nem nas cópias de segurança): vêm dentro do programa,
na compilação oficial, a partir de segredos do GitHub Actions.

## O que vai em cada sítio

| Dado | Onde fica | Segredo / variável do GitHub |
|---|---|---|
| Chave privada RSA do produtor (assinatura dos documentos) | dentro do programa | `KANGEST_AGT_CHAVE` (PEM) |
| N.º do certificado AGT | dentro do programa | `KANGEST_AGT_CERTIFICADO` |
| Versão da chave (HashControl do SAF-T) | dentro do programa (por omissão `1`) | `KANGEST_AGT_VERSAO_CHAVE` |
| Utilizador e senha dos serviços da FE | dentro do programa | `KANGEST_FE_UTILIZADOR`, `KANGEST_FE_SENHA` |
| Chave de software da FE (≥ 2048 bits) | dentro do programa | `KANGEST_FE_CHAVE_SOFTWARE` (PEM) |
| N.º de validação do software na FE | dentro do programa | `KANGEST_FE_NUMERO_VALIDACAO` |
| Chave pública das actualizações (minisign) | dentro do programa | variável `KANGEST_CHAVE_ACTUALIZACOES` |
| Só assinar com a chave e o certificado do produtor | dentro do programa | variável `KANGEST_EXIGIR_CERTIFICACAO` = `1` |
| Chave do **contribuinte** (FE) e estabelecimento | na instalação do cliente (cifrada) | — |

Os valores ficam ofuscados no executável (não aparecem numa procura de texto). Não é cifra: quem desmontar o programa
consegue lá chegar — é a protecção habitual dos programas certificados. O que muda é que a chave deixa de estar à vista
de qualquer pessoa que abra a base de dados ou uma cópia de segurança.

## Configurar a compilação oficial

No GitHub: *Settings → Secrets and variables → Actions*.

```bash
gh secret set KANGEST_AGT_CHAVE -R fernandotunas6-bot/KanGest < produtor-agt.pem
gh secret set KANGEST_AGT_CERTIFICADO -R fernandotunas6-bot/KanGest --body "123"
gh secret set KANGEST_FE_UTILIZADOR -R fernandotunas6-bot/KanGest
gh secret set KANGEST_FE_SENHA -R fernandotunas6-bot/KanGest
gh secret set KANGEST_FE_CHAVE_SOFTWARE -R fernandotunas6-bot/KanGest < produtor-fe.pem
gh secret set KANGEST_FE_NUMERO_VALIDACAO -R fernandotunas6-bot/KanGest --body "C_134"
gh variable set KANGEST_CHAVE_ACTUALIZACOES -R fernandotunas6-bot/KanGest --body "RWQ…"
```

Numa compilação local, as mesmas variáveis (ou `KANGEST_AGT_CHAVE_FICHEIRO=/caminho/chave.pem` e
`KANGEST_FE_CHAVE_SOFTWARE_FICHEIRO=…`) antes de `npm run tauri build`.

Depois da certificação, defina a variável `KANGEST_EXIGIR_CERTIFICACAO=1`: essa versão recusa assinar documentos se, por
engano, for compilada sem a chave ou sem o certificado.

## Instalações que já existiam

Ao arrancar pela primeira vez uma versão com a chave do produtor:

1. a chave privada que estava na base de dados (de desenvolvimento ou carregada à mão) é **apagada**;
2. a chave pública dela fica arquivada (tabela `agt_chaves`), para a verificação de integridade continuar a confirmar os
   documentos que ela assinou;
3. os documentos novos passam a ser assinados com a chave do produtor; a cadeia de cada série continua (a mensagem
   assinada inclui sempre o hash do documento anterior).

Em *Configurações → Certificação AGT* aparece a origem da chave (produtor, carregada ou de desenvolvimento) e o n.º do
certificado, que deixa de se poder alterar quando vem com o programa.

## HashControl

Cada documento guarda a versão da chave com que foi assinado (`documentos.hash_versao`): `0` enquanto o programa não tinha
certificado, depois a versão da chave do produtor. O SAF-T usa esse valor documento a documento, por isso uma exportação
que junte documentos de antes e de depois da certificação fica correcta.

## Mudar de chave

Se a chave do produtor for comprometida: gera-se outra, entrega-se a pública à AGT, actualiza-se o segredo
`KANGEST_AGT_CHAVE`, sobe-se `KANGEST_AGT_VERSAO_CHAVE` (ex.: `2`) e publica-se uma versão nova. As instalações passam a
assinar com a chave nova; a pública de cada chave do produtor usada fica registada em `agt_chaves` logo no primeiro
documento, por isso a verificação de integridade continua a confirmar os documentos antigos.

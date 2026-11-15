# Cópias de segurança

Em **Configurações → Sistema** (permissão *Cópia de segurança*).

## Quando se fazem

| Momento | Notas |
|---|---|
| Ao abrir o programa | Se a última cópia é de outro dia ou mais antiga do que a frequência escolhida |
| De N em N horas, com o programa aberto | Por omissão de 4 em 4 horas (1, 2, 4, 8, 12 ou 24, ou só ao abrir) |
| No fecho de caixa | Sempre |
| Antes de uma actualização, de uma importação ou de um restauro | Sempre |
| «Fazer cópia agora» | Manual |

As cópias fazem-se na máquina que guarda a base de dados (modo *Sozinho* ou *Servidor*). Um posto em rede não faz cópias:
os dados estão no servidor.

Cada cópia é feita com o programa em uso (`VACUUM INTO`) e **verificada** logo a seguir (`PRAGMA integrity_check` e as
tabelas do KanGest). Guardam-se as últimas N (por omissão 30) em `copias/`, na pasta de dados.

## Fora deste computador

Indique uma **pasta externa**: uma pen (`E:\Copias KanGest`), um disco de rede (`\\SERVIDOR\Copias`) ou a pasta do OneDrive
ou do Google Drive (que a sincroniza para a nuvem). Cada cópia é gravada também lá, com o NIF da empresa no nome.

- Se a pen não estiver ligada, a cópia local faz-se na mesma e o ecrã mostra o erro da pasta externa.
- **Cifrar as cópias da pasta externa** (opcional): ficam em `.kgc`, cifradas com AES-256-GCM e uma senha (PBKDF2). Protege
  os dados se a pen se perder. **Sem a senha, a cópia não se restaura noutro computador** — guarde-a num sítio seguro.

## Restaurar

- **Configurações → Sistema → Cópias guardadas → Restaurar** (ou indique o caminho de um ficheiro `.db`/`.kgc`).
- A cópia é verificada; a base actual é guardada em `copias/kangest-antes-do-restauro-…db`; o KanGest reinicia e troca a
  base antes de a abrir. As versões mais antigas da base são actualizadas pelas migrações ao abrir.
- Uma cópia feita por uma **versão mais recente** do KanGest é recusada («Actualize este KanGest antes de a restaurar»):
  tem migrações que esta versão não conhece e, depois da troca, a base deixaria de abrir.
- **Computador novo:** no ecrã de configuração inicial há «Já tem uma cópia de segurança do KanGest?» para restaurar em vez
  de configurar do zero.
- Os segredos guardados na instalação (chave do contribuinte da FE, senhas) estão cifrados com a chave mestra do
  computador onde foram gravados: noutro computador é preciso introduzi-los de novo (ver
  [CERTIFICACAO-AGT.md](CERTIFICACAO-AGT.md)).
- O restauro faz-se na máquina que guarda a base de dados; num posto em rede não está disponível.

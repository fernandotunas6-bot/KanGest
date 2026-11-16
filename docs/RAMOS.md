# Ramos de actividade

O KanGest adapta-se ao negócio. Em **Configurações → Empresa → Ramo de actividade** (ou logo no primeiro arranque)
escolhe-se o ramo; a partir daí os artigos pedem os dados próprios desse ramo, o POS mostra-os e usa-os, e a
pesquisa encontra-os. Mudar de ramo não apaga nada: os dados já guardados ficam nos artigos.

| Ramo | Dados de cada artigo | O que muda no programa |
|---|---|---|
| Comércio geral / loja | Marca | — |
| Supermercado, mercearia, minimercado | Marca, origem | Sugere as famílias habituais; balança, vários códigos por artigo e lotes/validades já existem |
| Livraria e papelaria | **ISBN** (no código de barras), autor(es), editora, ano, edição, colecção, idioma, páginas | O ISBN é conferido (algarismo de controlo) e o ISBN-10 passa a ISBN-13 — o mesmo do código de barras do livro; pesquisa por autor, editora ou ISBN; o botão do livro no POS mostra «autor · editora» |
| Farmácia e parafarmácia | Princípio activo (DCI), dosagem, forma, laboratório, **sujeito a receita** | Artigos novos com lotes e validades; pesquisa pelo princípio activo; o POS avisa quando o medicamento exige receita |
| Moda, vestuário e calçado | Marca, modelo, tamanho, cor, colecção, material | **Gerar variantes**: um artigo por tamanho × cor (ex.: `CAM01-M-AZUL`), cada um com o seu stock, preço e IVA do modelo; pesquisa por modelo, tamanho ou cor |
| Electrónica, informática e telemóveis | Marca, modelo, **garantia (meses)**, **pedir n.º de série / IMEI** | Na venda o POS pede o n.º de série de cada unidade (uma linha por unidade) e escreve na linha da factura «S/N … · Garantia 12 meses» |
| Ferragens e materiais de construção | Marca, medida, material | Sugere as famílias habituais |
| Peças auto e acessórios | Marca, ref. do fabricante (OEM), aplicação (viaturas) | Pesquisa pela referência do fabricante ou pela viatura |
| Restauração, café, bar e pastelaria | Alergénios | Pratos novos sem controlo de stock; cada **mesa** é uma venda em paralelo com nome («Mesa 4») |
| Prestação de serviços | Duração habitual | Artigos novos como serviço, à hora; **avenças** (facturação periódica) |
| Salão de beleza, barbearia, estética, clínica | Duração | Artigos novos como serviço; cada cliente em atendimento é uma venda com nome |
| Oficina auto | Marca, ref. do fabricante, aplicação | Módulo Oficina (viaturas, ordens de reparação) |
| Distribuição e venda por grosso | Marca, embalagem | Sugere as famílias habituais; níveis de preço e códigos por embalagem já existem |

**Criar famílias sugeridas** (no mesmo ecrã) cria as famílias habituais do ramo que ainda não existam.

## Como funciona por dentro

- O ramo fica em `config('empresa.ramo')`; a lista dos ramos e dos seus campos está em `src/lib/ramos.ts` (ecrã) e os
  identificadores aceites em `src-tauri/src/services/ramos.rs`.
- Os dados de ramo ficam em `artigos.atributos` (objecto JSON pequeno: texto, número ou sim). A pesquisa dos artigos
  procura nos **valores** (não nos nomes dos campos) com `json_each`.
- O mesmo código de barras (ou ISBN) não pode ficar em dois artigos.
- A API de integração devolve e aceita `atributos` ([API.md](API.md)).

## Avenças (prestação de serviços)

**Vendas → Avenças (facturação periódica)**: contratos de clientes com serviço contínuo — contabilidade, segurança,
limpeza, manutenção, mensalidades de escolas e ginásios, aluguer de equipamento.

- Cada avença tem o cliente, os serviços (artigos) com quantidade, preço (vazio = o do artigo) e desconto, a
  periodicidade (mensal, trimestral, semestral, anual), a **próxima factura** e, se quiser, a data em que termina.
- O painel avisa «N avenças por facturar» quando chega a data. **Facturar** emite uma factura (FT) por período
  chegado — com a data do dia (as facturas certificadas não se emitem com datas passadas) e o período na descrição
  da linha e nas observações (`{periodo}` → «Novembro 2026», «Novembro 2026 a Janeiro 2027»…). Se houver vários
  períodos em atraso, saem todos, um a um (até 12 de cada vez).
- Um período **nunca se factura duas vezes**: antes de cada factura fica reservado (`avenca_emissoes`, único por avença
  e período); se a factura falhar, a reserva sai e o período fica por facturar.
- O ecrã mostra a **receita recorrente** por mês. Apagar uma avença já facturada só a desactiva (o histórico fica).

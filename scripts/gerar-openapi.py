"""Gera docs/openapi.json, a especificação da API de integração (servida em GET /api/v1/openapi.json).

    python3 scripts/gerar-openapi.py            # escreve docs/openapi.json
    python3 scripts/gerar-openapi.py saida.json

As rotas estão em src-tauri/src/api.rs (rotear); este ficheiro descreve-as. O executável inclui o JSON ao compilar.
"""
import json, pathlib, sys

def ref(n): return {"$ref": f"#/components/schemas/{n}"}
def pref(n): return {"$ref": f"#/components/parameters/{n}"}
def rref(n): return {"$ref": f"#/components/responses/{n}"}

ERROS = {"401": rref("NaoAutenticado"), "403": rref("SemPermissao"), "429": rref("LimitePedidos")}

def op(resumo, perm, descricao="", params=None, corpo=None, respostas=None, tag=None, publico=False):
    d = {"summary": resumo, "tags": [tag] if tag else []}
    texto = descricao
    if perm:
        texto = (texto + "\n\n" if texto else "") + f"**Permissão da chave:** `{perm}`"
        d["x-permissao"] = perm
    if texto: d["description"] = texto
    if params: d["parameters"] = params
    if corpo: d["requestBody"] = {"required": True, "content": {"application/json": {"schema": corpo}}}
    r = dict(respostas or {})
    if not publico:
        for k, v in ERROS.items(): r.setdefault(k, v)
        d["security"] = [{"bearer": []}, {"chave": []}]
    else:
        d["security"] = []
    d["responses"] = r
    return d

def ok(schema, desc="OK", codigo="200"):
    return {codigo: {"description": desc, "content": {"application/json": {"schema": schema}}}}

def lista(item): return {"allOf": [ref("Pagina"), {"type": "object", "properties": {"dados": {"type": "array", "items": item}}}]}

def qp(nome, tipo, desc, obrig=False, exemplo=None, fmt=None):
    s = {"type": tipo}
    if fmt: s["format"] = fmt
    p = {"name": nome, "in": "query", "required": obrig, "description": desc, "schema": s}
    if exemplo is not None: p["example"] = exemplo
    return p

id_path = {"name": "id", "in": "path", "required": True, "schema": {"type": "integer"}}
paginacao = [pref("desde_id"), pref("limite")]

kz = {"type": "integer", "description": "Valor em cêntimos de Kz (1 Kz = 100)."}

schemas = {
    "Erro": {"type": "object", "properties": {"erro": {"type": "object", "properties": {
        "codigo": {"type": "string", "example": "sem_permissao", "description": "nao_autenticado, chave_invalida, chave_revogada, chave_inactiva, chave_expirada, ip_nao_autorizado, utilizador_inactivo, plano_sem_api, cloudflare_access, sem_permissao, nao_encontrado, metodo_nao_permitido, pedido_invalido, campo_desconhecido, validacao, limite_pedidos, demasiadas_falhas, pedido_grande, erro_interno, ocupado"},
        "mensagem": {"type": "string", "example": "Sem permissão: «Emitir Factura (FT) e Guia de Transporte (GT)» (vendas.fatura)."},
        "pedido": {"type": "integer", "example": 1234, "description": "Número do pedido no registo da API (o mesmo do cabeçalho X-KanGest-Pedido)."}}}}},
    "Pagina": {"type": "object", "properties": {
        "dados": {"type": "array", "items": {"type": "object"}},
        "mais": {"type": "boolean", "description": "Há mais registos depois desta página."},
        "seguinte": {"type": "integer", "nullable": True, "description": "Valor a usar em desde_id para pedir a página seguinte (null na última)."}}},
    "Artigo": {"type": "object", "properties": {
        "id": {"type": "integer"}, "referencia": {"type": "string", "example": "ARZ5"}, "codigo_barras": {"type": "string", "nullable": True},
        "codigo_balanca": {"type": "string", "nullable": True, "description": "Código do artigo na balança (PLU) das etiquetas com peso ou preço."},
        "descricao": {"type": "string"}, "descricao_curta": {"type": "string", "nullable": True},
        "tipo": {"type": "string", "enum": ["P", "S"], "description": "P produto, S serviço"},
        "familia_id": {"type": "integer", "nullable": True}, "familia": {"type": "string", "nullable": True},
        "subfamilia_id": {"type": "integer", "nullable": True}, "subfamilia": {"type": "string", "nullable": True},
        "unidade_id": {"type": "integer", "nullable": True}, "unidade": {"type": "string", "nullable": True},
        "taxa_iva_id": {"type": "integer"}, "taxa_iva": {"type": "number", "example": 14}, "motivo_isencao": {"type": "string", "nullable": True},
        "preco1": kz, "preco2": kz, "preco3": kz, "preco4": kz,
        "preco1_iva": kz, "preco2_iva": kz, "preco3_iva": kz, "preco4_iva": kz,
        "promo_desconto": {"type": "number"}, "promo_inicio": {"type": "string", "nullable": True}, "promo_fim": {"type": "string", "nullable": True},
        "promo_atual": {"type": "number", "description": "Desconto (%) da promoção em vigor hoje (0 = nenhuma)."},
        "qtd_minima": {"type": "number"}, "qtd_maxima": {"type": "number"},
        "controla_stock": {"type": "boolean"}, "permite_negativo": {"type": "boolean"}, "controla_lote": {"type": "boolean"},
        "composto": {"type": "boolean"}, "ativo": {"type": "boolean"},
        "custo": dict(kz, description="Só com a permissão api.custos."), "custo_medio": dict(kz, description="Só com a permissão api.custos."),
        "armazem_id": {"type": "integer"}, "stock": {"type": "number", "description": "Existência no armazém indicado (por omissão, o do posto da chave)."},
        "criado_em": {"type": "string"}, "alterado_em": {"type": "string", "description": "Última alteração (AAAA-MM-DDTHH:MM:SS)."},
        "leitura": {"type": "object", "description": "Só em /artigos/codigo/{codigo}: quanto vale a leitura quando não é 1.", "properties": {
            "origem": {"type": "string", "enum": ["embalagem", "balanca"]}, "quantidade": {"type": "number"}, "descricao": {"type": "string", "nullable": True},
            "peso": {"type": "number"}, "preco_etiqueta": kz}}}},
    "ArtigoEntrada": {"type": "object", "description": "Na criação: descricao, taxa_iva_id e preco1 (ou preco1_iva) são obrigatórios. Na alteração (PUT/PATCH) só se enviam os campos a mudar.",
        "properties": {
            "referencia": {"type": "string", "description": "Gerada automaticamente se omitida."}, "codigo_barras": {"type": "string"},
            "codigo_balanca": {"type": "string", "description": "Código na balança (PLU), só algarismos."},
            "descricao": {"type": "string"}, "descricao_curta": {"type": "string"}, "tipo": {"type": "string", "enum": ["P", "S"]},
            "familia_id": {"type": "integer"}, "subfamilia_id": {"type": "integer"}, "unidade_id": {"type": "integer"},
            "taxa_iva_id": {"type": "integer", "description": "Ver GET /tabelas (taxas_iva)."}, "motivo_isencao": {"type": "string", "description": "Obrigatório nos artigos isentos (ex.: M10)."},
            "custo": kz, "preco1": dict(kz, description="Preço sem IVA."), "preco2": kz, "preco3": kz, "preco4": kz,
            "preco1_iva": dict(kz, description="Preço com IVA: o KanGest calcula o preço sem IVA à taxa do artigo."), "preco2_iva": kz, "preco3_iva": kz, "preco4_iva": kz,
            "qtd_minima": {"type": "number"}, "qtd_maxima": {"type": "number"}, "controla_stock": {"type": "boolean"}, "permite_negativo": {"type": "boolean"},
            "ativo": {"type": "boolean"}, "controla_lote": {"type": "boolean"}, "aviso_validade_dias": {"type": "integer"}, "composto": {"type": "boolean"},
            "componentes": {"type": "array", "items": {"type": "object", "properties": {"componente_id": {"type": "integer"}, "quantidade": {"type": "number"}}}},
            "promo_desconto": {"type": "number"}, "promo_inicio": {"type": "string", "format": "date"}, "promo_fim": {"type": "string", "format": "date"}},
        "additionalProperties": False},
    "Stock": {"type": "object", "properties": {
        "id": {"type": "integer", "description": "Id do artigo."}, "referencia": {"type": "string"}, "codigo_barras": {"type": "string", "nullable": True},
        "descricao": {"type": "string"}, "unidade": {"type": "string", "nullable": True}, "qtd_minima": {"type": "number"}, "qtd_maxima": {"type": "number"},
        "quantidade": {"type": "number"}, "reservado": {"type": "number", "description": "Em notas de encomenda de clientes por facturar."}, "disponivel": {"type": "number"},
        "armazens": {"type": "array", "items": {"type": "object", "properties": {
            "armazem_id": {"type": "integer"}, "armazem": {"type": "string"}, "quantidade": {"type": "number"}, "reservado": {"type": "number"},
            "disponivel": {"type": "number"}, "ultima_entrada": {"type": "string", "nullable": True}, "ultima_saida": {"type": "string", "nullable": True}}}}}},
    "Movimento": {"type": "object", "properties": {
        "id": {"type": "integer"}, "data_hora": {"type": "string"}, "artigo_id": {"type": "integer"}, "referencia": {"type": "string"}, "descricao": {"type": "string"},
        "armazem_id": {"type": "integer"}, "quantidade": {"type": "number", "description": "+ entrada, − saída"}, "saldo": {"type": "number", "description": "Existência no armazém depois do movimento."},
        "documento_id": {"type": "integer", "nullable": True}, "numero_doc": {"type": "string", "nullable": True}, "movimento": {"type": "string"}}},
    "Entidade": {"type": "object", "properties": {
        "id": {"type": "integer"}, "tipo": {"type": "string", "enum": ["C", "F", "A"], "description": "C cliente, F fornecedor, A ambos"},
        "nome": {"type": "string"}, "nif": {"type": "string"}, "endereco": {"type": "string", "nullable": True}, "localidade": {"type": "string", "nullable": True},
        "provincia": {"type": "string", "nullable": True}, "pais": {"type": "string", "example": "AO"}, "telefone": {"type": "string", "nullable": True},
        "email": {"type": "string", "nullable": True}, "limite_credito": kz, "desconto": {"type": "number"}, "nivel_preco": {"type": "integer", "minimum": 1, "maximum": 4},
        "estado": {"type": "boolean", "description": "Entidade pública (SAF-T do Estado)."}, "ativo": {"type": "boolean"}, "observacoes": {"type": "string", "nullable": True},
        "saldo": dict(kz, description="Em dívida menos crédito a favor."), "credito": dict(kz, description="Crédito a favor (notas de crédito, adiantamentos)."),
        "criado_em": {"type": "string"}, "alterado_em": {"type": "string"}}},
    "EntidadeEntrada": {"type": "object", "description": "Na criação, o nome é obrigatório; sem NIF fica 999999999. Na alteração só se enviam os campos a mudar.",
        "properties": {
            "tipo": {"type": "string", "enum": ["C", "F", "A"]}, "nome": {"type": "string"}, "nif": {"type": "string"}, "endereco": {"type": "string"},
            "localidade": {"type": "string"}, "provincia": {"type": "string"}, "pais": {"type": "string", "description": "Código de 2 letras (AO, PT…)."},
            "estado": {"type": "boolean"}, "observacoes": {"type": "string"}, "telefone": {"type": "string"}, "email": {"type": "string"},
            "limite_credito": kz, "desconto": {"type": "number"}, "nivel_preco": {"type": "integer"}},
        "additionalProperties": False},
    "Linha": {"type": "object", "properties": {
        "linha": {"type": "integer"}, "artigo_id": {"type": "integer", "nullable": True}, "referencia": {"type": "string"}, "descricao": {"type": "string"},
        "quantidade": {"type": "number"}, "unidade": {"type": "string", "nullable": True}, "preco_unitario": dict(kz, description="Sem IVA."),
        "desconto_pct": {"type": "number"}, "valor_desconto": kz, "taxa_iva_pct": {"type": "number"}, "motivo_isencao": {"type": "string", "nullable": True},
        "valor_liquido": kz, "valor_iva": kz, "total": kz, "lote": {"type": "string", "nullable": True}, "validade": {"type": "string", "nullable": True}}},
    "Documento": {"type": "object", "properties": {
        "id": {"type": "integer"}, "numero_doc": {"type": "string", "example": "FR W2026/15"}, "tipo_codigo": {"type": "string"}, "serie": {"type": "string"}, "numero": {"type": "integer"},
        "data_emissao": {"type": "string", "format": "date"}, "data_hora": {"type": "string"}, "data_vencimento": {"type": "string", "nullable": True},
        "entidade_id": {"type": "integer", "nullable": True}, "entidade_nome": {"type": "string"}, "entidade_nif": {"type": "string"},
        "total_bruto": kz, "total_desconto": kz, "total_liquido": kz, "total_iva": kz, "total_retencao": kz, "total": kz, "valor_pendente": kz,
        "estado": {"type": "string", "enum": ["N", "A", "F"], "description": "N normal, A anulado, F facturado (proformas e encomendas)"},
        "hash_controlo": {"type": "string", "description": "4 caracteres da assinatura AGT, impressos no documento."},
        "referencia": {"type": "string", "nullable": True}, "api_chave_id": {"type": "integer", "nullable": True, "description": "Chave que emitiu o documento (null = emitido no programa)."},
        "linhas": {"type": "array", "items": ref("Linha")},
        "pagamentos": {"type": "array", "items": {"type": "object", "properties": {"modo_codigo": {"type": "string"}, "descricao": {"type": "string"}, "valor": kz}}},
        "liquidacoes": {"type": "array", "items": {"type": "object"}}, "retencoes": {"type": "array", "items": {"type": "object"}},
        "resumo_iva": {"type": "array", "items": {"type": "object", "properties": {"taxa": {"type": "number"}, "motivo_isencao": {"type": "string", "nullable": True}, "base": kz, "iva": kz}}},
        "fe": {"type": "object", "nullable": True, "description": "Facturação Electrónica da AGT (quando ligada).", "properties": {
            "estado": {"type": "string", "enum": ["PENDENTE", "ENVIADO", "VALIDO", "INVALIDO"]}, "url_consulta": {"type": "string", "description": "Conteúdo do QR code da AGT."}}}}},
    "DocumentoEntrada": {"type": "object", "required": ["tipo"], "properties": {
        "tipo": {"type": "string", "example": "FR", "description": "FR, FT, GT, PP, OR, NE, NC, ND, RC, FC, VFR, VNC, VND, VNE, VOR, VGT, AF, ES, SS, CI, TS."},
        "entidade_id": {"type": "integer", "description": "Cliente ou fornecedor. Nas vendas, por omissão Consumidor Final (1)."},
        "cliente": {"type": "object", "description": "Em vez de entidade_id: o cliente pelo NIF; se não existir é criado (exige a permissão clientes). Sem NIF: Consumidor Final.",
            "properties": {"nif": {"type": "string"}, "nome": {"type": "string"}, "endereco": {"type": "string"}, "localidade": {"type": "string"}, "provincia": {"type": "string"}, "pais": {"type": "string"}, "telefone": {"type": "string"}, "email": {"type": "string"}}},
        "linhas": {"type": "array", "items": {"type": "object", "properties": {
            "artigo_id": {"type": "integer"}, "referencia": {"type": "string", "description": "Em vez de artigo_id."}, "codigo_barras": {"type": "string", "description": "Em vez de artigo_id. O código de uma embalagem multiplica a quantidade; uma etiqueta da balança dá o peso."},
            "quantidade": {"type": "number"}, "preco_unitario": dict(kz, description="Sem IVA. Omitido = preço da ficha (preço diferente exige pos.alterar_preco)."),
            "preco_com_iva": dict(kz, description="Em vez de preco_unitario: preço com IVA."), "desconto_pct": {"type": "number", "description": "Exige pos.desconto (acima da promoção em vigor)."},
            "descricao": {"type": "string"}, "lote": {"type": "string"}, "validade": {"type": "string", "format": "date"}}, "required": ["quantidade"]}},
        "pagamentos": {"type": "array", "description": "Obrigatórios nos documentos pagos no acto (FR, RC…). O troco só em numerário (NU).",
            "items": {"type": "object", "properties": {"modo": {"type": "string", "example": "TB", "description": "Ver GET /tabelas (modos_pagamento)."}, "valor": kz}}},
        "liquidar": {"type": "array", "description": "Recibos (RC): documentos em dívida a liquidar.", "items": {"type": "object", "properties": {"documento_id": {"type": "integer"}, "valor": kz}}},
        "retencoes": {"type": "array", "items": {"type": "object", "properties": {"codigo": {"type": "string", "example": "II"}, "percentagem": {"type": "number"}}}},
        "desconto_global_pct": {"type": "number"}, "desconto_global_valor": kz,
        "doc_origem_id": {"type": "integer", "description": "Documento de origem (nota de crédito, conversão de proforma/encomenda). Envie as linhas sem preço: o preço e o desconto vêm da origem (sem pos.alterar_preco/pos.desconto, cada artigo tem de estar na origem)."},
        "motivo": {"type": "string", "description": "Obrigatório nas notas de crédito e de débito."},
        "referencia": {"type": "string", "description": "V/ Referência: n.º da encomenda no outro sistema (sai impresso)."},
        "observacoes": {"type": "string"}, "data_vencimento": {"type": "string", "format": "date"}, "prazo": {"type": "string"},
        "armazem_id": {"type": "integer"}, "armazem_destino_id": {"type": "integer"}, "serie": {"type": "string"},
        "vendedor_id": {"type": "integer"}, "endereco_entrega": {"type": "string"}, "transporte": {"type": "object"},
        "moeda": {"type": "string", "example": "AOA"}, "cambio": {"type": "number"}, "valor": dict(kz, description="Recibo de adiantamento."),
        "doc_externo": {"type": "string", "description": "Compras: n.º do documento do fornecedor."}, "doc_externo_data": {"type": "string", "format": "date"},
        "doc_externo_hash": {"type": "string"}, "deducao_tipo": {"type": "string"},
        "custos": {"type": "array", "items": {"type": "object", "properties": {"tipo": {"type": "string"}, "descricao": {"type": "string"}, "valor": kz}}}}},
}

info_desc = """API de integração do KanGest: lojas online, contabilidade, BI e outras aplicações ligam-se ao KanGest com **chaves de acesso**.

- **Chave:** `Authorization: Bearer kg_…` (ou `X-API-Key: kg_…`). Cria-se em *Sistema → API e Integrações*; o segredo só aparece uma vez.
- **Permissões:** cada chave tem as suas e age em nome de um utilizador: nunca faz mais do que ele.
- **Valores monetários** em cêntimos de Kz (inteiros): 1 140,00 Kz = `114000`. Datas `AAAA-MM-DD`; data e hora `AAAA-MM-DDTHH:MM:SS` (hora local).
- **Listas** ordenadas por `id`, em páginas: `?desde_id=<seguinte>&limite=…` até `mais` ser `false`. `alterado_desde` traz só o que mudou (artigos e entidades).
- **Emissão:** os documentos seguem as regras do programa (série do posto da chave, assinatura AGT, stock, Facturação Electrónica). Envie `Idempotency-Key` para poder repetir o pedido sem emitir outro documento.
- **Erros:** `{"erro": {"codigo": "...", "mensagem": "..."}}` com 400, 401, 403, 404, 405, 413, 422, 429 ou 500.
- **Limite:** pedidos por minuto definidos em cada chave (cabeçalhos `X-RateLimit-*`; `Retry-After` quando passa).
- **Cloudflare Access** (opcional): atrás de um túnel da Cloudflare, os pedidos têm também de trazer a autorização do Access (`CF-Access-Client-Id` e `CF-Access-Client-Secret` de um token de serviço)."""

spec = {
    "openapi": "3.0.3",
    "info": {"title": "API do KanGest", "version": "1.0", "description": info_desc},
    "servers": [{"url": "http://{servidor}:{porta}/api/v1", "variables": {"servidor": {"default": "localhost"}, "porta": {"default": "7800"}}}],
    "tags": [{"name": t} for t in ["Geral", "Artigos", "Stock", "Clientes e fornecedores", "Documentos", "Relatórios e fiscal"]],
    "security": [{"bearer": []}, {"chave": []}],
    "components": {
        "securitySchemes": {"bearer": {"type": "http", "scheme": "bearer", "description": "Authorization: Bearer kg_…"},
                            "chave": {"type": "apiKey", "in": "header", "name": "X-API-Key"}},
        "schemas": schemas,
        "parameters": {
            "desde_id": {"name": "desde_id", "in": "query", "description": "Só registos com id maior (paginação: use o valor «seguinte» da página anterior).", "schema": {"type": "integer", "default": 0}},
            "limite": {"name": "limite", "in": "query", "description": "Registos por página (1–1000).", "schema": {"type": "integer", "default": 100, "minimum": 1, "maximum": 1000}},
            "alterado_desde": {"name": "alterado_desde", "in": "query", "description": "Só os alterados desde esta data/hora (inclui os desactivados, salvo inativos=0).", "schema": {"type": "string"}, "example": "2026-11-01T00:00:00"},
            "inativos": {"name": "inativos", "in": "query", "description": "Incluir os desactivados (1/0).", "schema": {"type": "boolean"}},
            "q": {"name": "q", "in": "query", "description": "Pesquisa no texto.", "schema": {"type": "string"}},
            "armazem_id": {"name": "armazem_id", "in": "query", "description": "Armazém (por omissão, o do posto da chave).", "schema": {"type": "integer"}},
        },
        "responses": {
            "NaoAutenticado": {"description": "Chave em falta, inválida, revogada, desactivada ou expirada.", "content": {"application/json": {"schema": ref("Erro")}}},
            "SemPermissao": {"description": "Sem permissão, IP não autorizado ou utilizador inactivo.", "content": {"application/json": {"schema": ref("Erro")}}},
            "NaoEncontrado": {"description": "Não encontrado.", "content": {"application/json": {"schema": ref("Erro")}}},
            "Validacao": {"description": "Pedido recusado pelas regras do programa (mensagem em português).", "content": {"application/json": {"schema": ref("Erro")}}},
            "LimitePedidos": {"description": "Limite de pedidos por minuto (ou demasiadas chaves erradas deste IP).", "content": {"application/json": {"schema": ref("Erro")}}},
        },
    },
    "paths": {},
}
P = spec["paths"]
P["/saude"] = {"get": op("Verificar se a API responde", None, publico=True, tag="Geral", respostas=ok({"type": "object", "properties": {"ok": {"type": "boolean"}, "api": {"type": "string"}}}))}
P["/openapi.json"] = {"get": op("Esta especificação (OpenAPI 3)", None, publico=True, tag="Geral", respostas={"200": {"description": "Especificação"}})}
P["/"] = {"get": op("Empresa, chave e posto", None, "Confirma a chave: devolve a empresa, as permissões efectivas da chave, o utilizador e o posto (série e armazém).", tag="Geral",
    respostas=ok({"type": "object", "properties": {"app": {"type": "string"}, "versao": {"type": "string"}, "api": {"type": "string"}, "hora": {"type": "string"},
        "empresa": {"type": "object", "properties": {"nome": {"type": "string"}, "nif": {"type": "string"}}},
        "chave": {"type": "object", "properties": {"id": {"type": "integer"}, "nome": {"type": "string"}, "prefixo": {"type": "string"}, "permissoes": {"type": "array", "items": {"type": "string"}}, "limite_minuto": {"type": "integer"}, "utilizador": {"type": "string"}}},
        "posto": {"type": "object", "properties": {"id": {"type": "integer"}, "descricao": {"type": "string"}, "armazem_id": {"type": "integer"}, "serie_prefixo": {"type": "string"}}}}}))}
P["/tabelas"] = {"get": op("Tabelas auxiliares", None, "Famílias, subfamílias, unidades, taxas de IVA, motivos de isenção, armazéns, modos de pagamento, tipos de documento, prazos, retenções e moedas.", tag="Geral", respostas=ok({"type": "object"}))}
P["/artigos"] = {
    "get": op("Listar artigos", "api.artigos", "Sem a permissão api.custos, os custos não são enviados.", tag="Artigos",
              params=paginacao + [pref("q"), qp("familia_id", "integer", "Família."), pref("armazem_id"), pref("inativos"), pref("alterado_desde")], respostas=ok(lista(ref("Artigo")))),
    "post": op("Criar artigo", "artigos.criar", tag="Artigos", corpo=ref("ArtigoEntrada"),
               respostas={**ok(ref("Artigo"), "Criado", "201"), "400": rref("Validacao"), "422": rref("Validacao")}),
}
P["/artigos/{id}"] = {
    "parameters": [id_path],
    "get": op("Obter artigo", "api.artigos", tag="Artigos", params=[pref("armazem_id")], respostas={**ok(ref("Artigo")), "404": rref("NaoEncontrado")}),
    "put": op("Alterar artigo", "artigos.editar", "Só os campos enviados mudam.", tag="Artigos", corpo=ref("ArtigoEntrada"), respostas={**ok(ref("Artigo")), "404": rref("NaoEncontrado"), "422": rref("Validacao")}),
    "patch": op("Alterar artigo (igual ao PUT)", "artigos.editar", tag="Artigos", corpo=ref("ArtigoEntrada"), respostas={**ok(ref("Artigo")), "404": rref("NaoEncontrado"), "422": rref("Validacao")}),
}
P["/artigos/codigo/{codigo}"] = {"get": op("Procurar pelo código de barras ou pela referência", "api.artigos", "Leitores de código de barras, verificadores de preço.", tag="Artigos",
    params=[{"name": "codigo", "in": "path", "required": True, "schema": {"type": "string"}}, pref("armazem_id")], respostas={**ok(ref("Artigo")), "404": rref("NaoEncontrado")})}
P["/stock"] = {"get": op("Existências por artigo e armazém", "api.stock", "Artigos com controlo de stock (sem kits nem serviços), com o detalhe por armazém e o reservado por notas de encomenda.", tag="Stock",
    params=paginacao + [qp("artigo_id", "integer", "Só este artigo."), qp("armazem_id", "integer", "Só este armazém (omitido = todos)."), pref("q"), qp("abaixo_minimo", "boolean", "Só os que estão no mínimo ou abaixo.")],
    respostas=ok(lista(ref("Stock"))))}
P["/stock/movimentos"] = {"get": op("Movimentos de stock", "api.stock", "Para sincronizar o stock: peça os movimentos desde o último id visto; `saldo` é a existência do armazém depois de cada movimento.", tag="Stock",
    params=paginacao + [qp("artigo_id", "integer", "Só este artigo."), qp("armazem_id", "integer", "Só este armazém."), qp("de", "string", "A partir desta data.")], respostas=ok(lista(ref("Movimento"))))}
for nome, perm_escrita, tag in [("clientes", "clientes", "Clientes e fornecedores"), ("fornecedores", "fornecedores", "Clientes e fornecedores")]:
    singular = "cliente" if nome == "clientes" else "fornecedor"
    P[f"/{nome}"] = {
        "get": op(f"Listar {nome}", "api.entidades", tag=tag, params=paginacao + [pref("q"), qp("nif", "string", "NIF exacto."), pref("inativos"), pref("alterado_desde")], respostas=ok(lista(ref("Entidade")))),
        "post": op(f"Criar {singular}", perm_escrita, tag=tag, corpo=ref("EntidadeEntrada"), respostas={**ok(ref("Entidade"), "Criado", "201"), "400": rref("Validacao"), "422": rref("Validacao")}),
    }
    P[f"/{nome}/{{id}}"] = {
        "parameters": [id_path],
        "get": op(f"Obter {singular}", "api.entidades", tag=tag, respostas={**ok(ref("Entidade")), "404": rref("NaoEncontrado")}),
        "put": op(f"Alterar {singular}", perm_escrita, "Só os campos enviados mudam.", tag=tag, corpo=ref("EntidadeEntrada"), respostas={**ok(ref("Entidade")), "404": rref("NaoEncontrado"), "422": rref("Validacao")}),
        "patch": op(f"Alterar {singular} (igual ao PUT)", perm_escrita, tag=tag, corpo=ref("EntidadeEntrada"), respostas={**ok(ref("Entidade")), "404": rref("NaoEncontrado"), "422": rref("Validacao")}),
    }
    P[f"/{nome}/{{id}}/pendentes"] = {"parameters": [id_path], "get": op("Documentos em dívida", "api.entidades", tag=tag,
        respostas={**ok({"type": "object", "properties": {"dados": {"type": "array", "items": {"type": "object"}}}}), "404": rref("NaoEncontrado")})}
P["/documentos"] = {
    "get": op("Listar documentos", "api.documentos", tag="Documentos",
              params=paginacao + [qp("tipo", "string", "Um ou mais tipos separados por vírgulas.", exemplo="FT,FR"), qp("grupo", "string", "VENDAS, COMPRAS ou STOCK."),
                                  qp("de", "string", "Data de emissão desde."), qp("ate", "string", "Data de emissão até."), qp("entidade_id", "integer", "Cliente/fornecedor."),
                                  qp("estado", "string", "N, A ou F."), qp("desta_chave", "boolean", "Só os emitidos por esta chave."), qp("linhas", "boolean", "Incluir linhas e pagamentos.")],
              respostas=ok(lista(ref("Documento")))),
    "post": op("Emitir documento", "a do tipo: vendas.fatura_recibo (FR), vendas.fatura (FT, GT), vendas.proforma (PP, OR, NE), vendas.nota_credito (NC), vendas.nota_debito (ND), tesouraria.recibo (RC), compras.fatura (compras), stock.entrada (ES), stock.saida (SS, CI), stock.transferencia (TS)",
               "Emite com as regras do programa: série do posto da chave, assinatura AGT, stock, conta corrente e Facturação Electrónica. Os documentos da API não entram no caixa de nenhum posto.\n\nCabeçalho opcional **Idempotency-Key** (até 100 caracteres): o mesmo pedido repetido com a mesma chave devolve o documento já emitido (200, `Idempotent-Replayed: true`).",
               tag="Documentos", corpo=ref("DocumentoEntrada"),
               params=[{"name": "Idempotency-Key", "in": "header", "required": False, "schema": {"type": "string", "maxLength": 100}, "example": "encomenda-1001"}],
               respostas={**ok(ref("Documento"), "Emitido", "201"), **ok(ref("Documento"), "Já emitido com esta Idempotency-Key"), "400": rref("Validacao"), "422": rref("Validacao")}),
}
P["/documentos/{id}"] = {"parameters": [id_path], "get": op("Obter documento", "api.documentos", tag="Documentos", respostas={**ok(ref("Documento")), "404": rref("NaoEncontrado")})}
P["/documentos/{id}/anular"] = {"parameters": [id_path], "post": op("Anular documento", "documentos.anular", "Com a Facturação Electrónica ligada, os documentos comunicados à AGT não se anulam: emite-se uma nota de crédito.", tag="Documentos",
    corpo={"type": "object", "required": ["motivo"], "properties": {"motivo": {"type": "string", "minLength": 5}}}, respostas={**ok(ref("Documento")), "404": rref("NaoEncontrado"), "422": rref("Validacao")})}
P["/relatorios/vendas"] = {"get": op("Resumo de vendas", "relatorios.ver", "Vendas do período agrupadas (notas de crédito a abater). O lucro só aparece com api.custos.", tag="Relatórios e fiscal",
    params=[qp("de", "string", "Desde (AAAA-MM-DD).", True), qp("ate", "string", "Até (AAAA-MM-DD).", True),
            {"name": "agrupar", "in": "query", "schema": {"type": "string", "default": "dia", "enum": ["dia", "mes", "artigo", "familia", "operador", "cliente", "pagamento", "vendedor", "posto", "documento", "iva", "iva_dia", "iva_familia", "artigo_operador", "artigo_entidade"]}},
            qp("entidade_id", "integer", "Só este cliente.")],
    respostas=ok({"type": "object", "properties": {"de": {"type": "string"}, "ate": {"type": "string"}, "agrupar": {"type": "string"}, "dados": {"type": "array", "items": {"type": "object"}}}}))}
P["/saft"] = {"get": op("Exportar SAF-T (AO)", "sistema.saft", "Devolve o XML (não grava ficheiros no computador do KanGest).", tag="Relatórios e fiscal",
    params=[qp("de", "string", "Desde (AAAA-MM-DD).", True), qp("ate", "string", "Até (AAAA-MM-DD).", True),
            {"name": "tipo", "in": "query", "schema": {"type": "string", "default": "facturacao", "enum": ["facturacao", "aquisicao", "estado"]}}],
    respostas={"200": {"description": "Ficheiro SAF-T", "content": {"application/xml": {"schema": {"type": "string"}}}}, "400": rref("Validacao"), "422": rref("Validacao")})}

destino = sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "docs" / "openapi.json"
with open(destino, "w", encoding="utf-8") as f:
    json.dump(spec, f, ensure_ascii=False, indent=2)
    f.write("\n")
print("ok", len(P), "caminhos")

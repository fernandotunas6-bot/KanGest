# KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
# © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.

"""Base com o volume de ~3 anos de uma loja, para medir as consultas (docs/BASE-DE-DADOS.md).

    python3 scripts/base-volume.py <base de origem> <base nova> [documentos=150000]

Copia a base de origem (por exemplo a do servidor de demonstração, /tmp/kigest-auditoria/kigest.db) e acrescenta
clientes, documentos com linhas, pagamentos e movimentos de stock, movimentos de tesouraria, uma sessão de caixa por
dia e só ~5 % das facturas por pagar. Depois:

    KANGEST_BASE=<base nova> cargo test --release --lib volume_tempos -- --ignored --nocapture
"""
import datetime, random, sqlite3, sys, time
orig, dest, n_docs = sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 150_000
random.seed(7)
s = sqlite3.connect(orig, timeout=10); d = sqlite3.connect(dest); s.backup(d); s.close()
d.execute("PRAGMA foreign_keys=OFF"); d.execute("PRAGMA journal_mode=OFF"); d.execute("PRAGMA synchronous=OFF")
cols = lambda t: [r[1] for r in d.execute(f"PRAGMA table_info({t})")]
def copiar(t, linha, novos):
    c = cols(t); v = dict(zip(c, linha)); v.update(novos)
    d.execute(f"INSERT INTO {t} ({','.join(c)}) VALUES ({','.join('?' * len(c))})", [v[k] for k in c])
t0 = time.time()
# clientes
ent_cols = cols("entidades"); modelo_ent = d.execute("SELECT * FROM entidades WHERE id <> 1 AND tipo = 'C' LIMIT 1").fetchone() or d.execute("SELECT * FROM entidades WHERE id <> 1 LIMIT 1").fetchone()
id_ent = d.execute("SELECT MAX(id) FROM entidades").fetchone()[0]
clientes = []
for i in range(3000):
    id_ent += 1
    nif = f"5{400000000 + i:09d}"
    copiar("entidades", modelo_ent, {"id": id_ent, "nome": f"Cliente Volume {i}", "nif": nif, **({"codigo": f"CV{i}"} if "codigo" in ent_cols else {})})
    clientes.append((id_ent, f"Cliente Volume {i}", nif))
modelos = d.execute("SELECT * FROM documentos WHERE tipo_codigo IN ('FR','FT') AND estado <> 'A' AND importado = 0").fetchall()
dc = cols("documentos")
linhas_por_doc = {}
for l in d.execute("SELECT * FROM documento_linhas").fetchall():
    linhas_por_doc.setdefault(l[cols("documento_linhas").index("documento_id")], []).append(l)
pag_por_doc = {}
for p in d.execute("SELECT * FROM documento_pagamentos").fetchall():
    pag_por_doc.setdefault(p[cols("documento_pagamentos").index("documento_id")], []).append(p)
mov_modelo = d.execute("SELECT * FROM movimentos_stock LIMIT 1").fetchone()
funcs = [r[0] for r in d.execute("SELECT id FROM funcionarios")] or [None]
id_doc = d.execute("SELECT MAX(id) FROM documentos").fetchone()[0]
id_lin = d.execute("SELECT MAX(id) FROM documento_linhas").fetchone()[0]
id_pag = d.execute("SELECT COALESCE(MAX(id),0) FROM documento_pagamentos").fetchone()[0]
id_mov = d.execute("SELECT COALESCE(MAX(id),0) FROM movimentos_stock").fetchone()[0]
inicio = datetime.date(2023, 11, 15)
for i in range(n_docs):
    m = random.choice(modelos); mv = dict(zip(dc, m))
    dia = inicio + datetime.timedelta(days=int(i * 1095 / n_docs))
    hora = f"{dia.isoformat()}T{8 + (i % 12):02d}:{i % 60:02d}:{(i * 7) % 60:02d}"
    id_doc += 1
    cli = random.choice(clientes) if mv["tipo_codigo"] == "FT" or random.random() < 0.3 else (1, mv["entidade_nome"], mv["entidade_nif"])
    novos = {"id": id_doc, "serie": f"B{dia.year}", "numero": i + 1, "numero_doc": f"{mv['tipo_codigo']} B{dia.year}/{i + 1}",
             "data_emissao": dia.isoformat(), "data_hora": hora, "entidade_id": cli[0], "entidade_nome": cli[1], "entidade_nif": cli[2],
             "vendedor_id": random.choice(funcs), "posto_id": 1}
    copiar("documentos", m, {k: v for k, v in novos.items() if k in dc})
    for l in linhas_por_doc.get(mv["id"], []):
        id_lin += 1
        copiar("documento_linhas", l, {"id": id_lin, "documento_id": id_doc})
        if mov_modelo:
            id_mov += 1
            lc = dict(zip(cols("documento_linhas"), l))
            copiar("movimentos_stock", mov_modelo, {k: v for k, v in {"id": id_mov, "documento_id": id_doc, "artigo_id": lc["artigo_id"], "data_hora": hora, "quantidade": -abs(lc.get("quantidade") or 1)}.items() if k in cols("movimentos_stock")})
    for p in pag_por_doc.get(mv["id"], []):
        id_pag += 1
        copiar("documento_pagamentos", p, {"id": id_pag, "documento_id": id_doc})
    if i % 20000 == 0:
        d.commit()
# tesouraria e caixa
contas = [r[0] for r in d.execute("SELECT id FROM contas")] or [1]
mt = d.execute("SELECT * FROM movimentos_tesouraria LIMIT 1").fetchone()
id_mt = d.execute("SELECT COALESCE(MAX(id),0) FROM movimentos_tesouraria").fetchone()[0]
for i in range(40000):
    dia = inicio + datetime.timedelta(days=int(i * 1095 / 40000))
    id_mt += 1
    if mt:
        copiar("movimentos_tesouraria", mt, {"id": id_mt, "conta_id": random.choice(contas), "data": dia.isoformat(), "data_hora": f"{dia}T10:00:00", "entidade_id": random.choice(clientes)[0], "id_origem": None, "anulado": 0})
d.commit()
for t in ["entidades", "documentos", "documento_linhas", "documento_pagamentos", "movimentos_stock", "movimentos_tesouraria"]:
    print(t, d.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0])
d.commit(); d.close()
print(f"pronto em {time.time() - t0:.0f} s")

# ---- dados realistas: uma sessão de caixa por dia e só ~5 % das facturas por pagar (as dos últimos meses) ----
c = sqlite3.connect(dest); c.execute("PRAGMA foreign_keys=OFF")
modelo = c.execute("SELECT * FROM caixa_sessoes ORDER BY id LIMIT 1").fetchone()
cols = [r[1] for r in c.execute("PRAGMA table_info(caixa_sessoes)")]
aberta = c.execute("SELECT id FROM caixa_sessoes WHERE estado <> 'FECHADA' ORDER BY id DESC LIMIT 1").fetchone()
hoje = c.execute("SELECT MAX(data_emissao) FROM documentos").fetchone()[0]
proximo = c.execute("SELECT MAX(id) FROM caixa_sessoes").fetchone()[0]
dias = [r[0] for r in c.execute("SELECT DISTINCT data_emissao FROM documentos WHERE serie LIKE 'B%' AND data_emissao < ? ORDER BY 1", (hoje,))]
for d in dias:
    proximo += 1
    v = dict(zip(cols, modelo)); v.update({"id": proximo, "aberta_em": f"{d}T08:00:00", "fechada_em": f"{d}T20:00:00", "estado": "FECHADA"})
    c.execute(f"INSERT INTO caixa_sessoes ({','.join(cols)}) VALUES ({','.join('?'*len(cols))})", [v[k] for k in cols])
    c.execute("UPDATE documentos SET caixa_sessao_id = ? WHERE serie LIKE 'B%' AND data_emissao = ? AND caixa_sessao_id IS NOT NULL", (proximo, d))
# só ~5 % das facturas ficam por pagar (as dos últimos meses)
c.execute("UPDATE documentos SET valor_pendente = 0 WHERE serie LIKE 'B%' AND valor_pendente > 0 AND (data_emissao < date(?, '-90 day') OR id % 20 <> 0)", (hoje,))
c.commit()
print("sessões:", c.execute("SELECT COUNT(*) FROM caixa_sessoes").fetchone()[0],
      "· docs na sessão aberta:", c.execute("SELECT COUNT(*) FROM documentos WHERE caixa_sessao_id = ?", (aberta[0] if aberta else -1,)).fetchone()[0],
      "· pendentes:", c.execute("SELECT COUNT(*) FROM documentos WHERE estado = 'N' AND valor_pendente > 0").fetchone()[0])

# como nas instalações: sem estatísticas do ANALYZE
c.execute("DROP TABLE IF EXISTS sqlite_stat1"); c.execute("DROP TABLE IF EXISTS sqlite_stat4"); c.commit()

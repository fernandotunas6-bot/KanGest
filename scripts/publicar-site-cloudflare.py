#!/usr/bin/env python3
"""Publica a pasta site/ no Cloudflare Pages (envio directo, sem o wrangler).

    CLOUDFLARE_API_TOKEN=… python3 scripts/publicar-site-cloudflare.py [--projecto kangest] [--ensaio]

O token precisa da permissão «Account · Cloudflare Pages · Edit» (e «Account Settings · Read» para encontrar a
conta; ou indique CLOUDFLARE_ACCOUNT_ID). Cria o projecto se não existir e publica no ramo de produção («main»):
o endereço fica https://<projecto>.pages.dev. Com --ensaio só lista os ficheiros e os resumos, sem enviar nada.

O Cloudflare identifica cada ficheiro pelo BLAKE3 de (conteúdo em base64 + extensão), como o wrangler; a
implementação abaixo é a de referência, em Python puro, para não depender de pacotes externos.
"""
import base64, json, mimetypes, os, ssl, struct, sys, urllib.error, urllib.request, uuid
from pathlib import Path

API = "https://api.cloudflare.com/client/v4"
RAIZ = Path(__file__).resolve().parent.parent / "site"
EXCLUIR = {"LEIA-ME.md"}

# ------------------------------------------------------------------ BLAKE3 (hash simples, 32 bytes)
_IV = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19]
_PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8]
_M = 0xFFFFFFFF
_INICIO, _FIM, _PAI, _RAIZ = 1, 2, 4, 8


def _g(s, a, b, c, d, x, y):
    s[a] = (s[a] + s[b] + x) & _M
    s[d] = ((s[d] ^ s[a]) >> 16 | (s[d] ^ s[a]) << 16) & _M
    s[c] = (s[c] + s[d]) & _M
    s[b] = ((s[b] ^ s[c]) >> 12 | (s[b] ^ s[c]) << 20) & _M
    s[a] = (s[a] + s[b] + y) & _M
    s[d] = ((s[d] ^ s[a]) >> 8 | (s[d] ^ s[a]) << 24) & _M
    s[c] = (s[c] + s[d]) & _M
    s[b] = ((s[b] ^ s[c]) >> 7 | (s[b] ^ s[c]) << 25) & _M


def _comprimir(cv, m, contador, tamanho, flags):
    s = list(cv) + _IV[:4] + [contador & _M, (contador >> 32) & _M, tamanho, flags]
    m = list(m)
    for r in range(7):
        _g(s, 0, 4, 8, 12, m[0], m[1]); _g(s, 1, 5, 9, 13, m[2], m[3])
        _g(s, 2, 6, 10, 14, m[4], m[5]); _g(s, 3, 7, 11, 15, m[6], m[7])
        _g(s, 0, 5, 10, 15, m[8], m[9]); _g(s, 1, 6, 11, 12, m[10], m[11])
        _g(s, 2, 7, 8, 13, m[12], m[13]); _g(s, 3, 4, 9, 14, m[14], m[15])
        if r < 6:
            m = [m[i] for i in _PERM]
    for i in range(8):
        s[i] ^= s[i + 8]
        s[i + 8] ^= cv[i]
    return s


def _palavras(bloco):
    return list(struct.unpack("<16I", bloco.ljust(64, b"\0")))


def _saida_bloco(bloco_de_1024, contador):
    """Saída por calcular do último bloco de um pedaço: (cv, palavras, contador, tamanho, flags)."""
    cv = _IV[:]
    blocos = [bloco_de_1024[i:i + 64] for i in range(0, len(bloco_de_1024), 64)] or [b""]
    for j, b in enumerate(blocos):
        flags = (_INICIO if j == 0 else 0) | (_FIM if j == len(blocos) - 1 else 0)
        if j == len(blocos) - 1:
            return (cv, _palavras(b), contador, len(b), flags)
        cv = _comprimir(cv, _palavras(b), contador, 64, flags)[:8]


def _cv(saida):
    return _comprimir(*saida)[:8]


def _subarvore(pedacos, inicio):
    if len(pedacos) == 1:
        return _saida_bloco(pedacos[0], inicio)
    esquerda = 1 << ((len(pedacos) - 1).bit_length() - 1)  # maior potência de 2 que deixa pelo menos 1 à direita
    e = _subarvore(pedacos[:esquerda], inicio)
    d = _subarvore(pedacos[esquerda:], inicio + esquerda)
    return (_IV[:], _cv(e) + _cv(d), 0, 64, _PAI)


def blake3(dados: bytes) -> str:
    pedacos = [dados[i:i + 1024] for i in range(0, len(dados), 1024)] or [b""]
    cv, m, _, tamanho, flags = _subarvore(pedacos, 0)
    return struct.pack("<8I", *_comprimir(cv, m, 0, tamanho, flags | _RAIZ)[:8]).hex()


def _verificar_blake3():
    # vectores oficiais (entrada = bytes 0..250 a repetir)
    teste = lambda n: bytes(i % 251 for i in range(n))
    esperado = {
        0: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        1024: "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7",
        1025: "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444",
    }
    for n, h in esperado.items():
        assert blake3(teste(n)) == h, f"BLAKE3 errado para {n} bytes"
    assert blake3(b"abc") == "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"


# ------------------------------------------------------------------ API
def _contexto_tls():
    """O Python do python.org no macOS não usa os certificados do sistema: usa os do certifi ou /etc/ssl/cert.pem."""
    try:
        import certifi
        return ssl.create_default_context(cafile=certifi.where())
    except ImportError:
        pass
    for f in ("/etc/ssl/cert.pem", "/usr/local/etc/openssl/cert.pem", "/opt/homebrew/etc/openssl@3/cert.pem"):
        if os.path.exists(f):
            return ssl.create_default_context(cafile=f)
    return ssl.create_default_context()


_TLS = _contexto_tls()


def pedido(metodo, caminho, token, dados=None, multipart=None, base=API):
    url = caminho if caminho.startswith("http") else base + caminho
    cab = {"Authorization": f"Bearer {token}"}
    corpo = None
    if multipart is not None:
        fronteira = uuid.uuid4().hex
        partes = []
        for nome, valor in multipart.items():
            partes.append(f'--{fronteira}\r\nContent-Disposition: form-data; name="{nome}"\r\n\r\n{valor}\r\n')
        corpo = ("".join(partes) + f"--{fronteira}--\r\n").encode()
        cab["Content-Type"] = f"multipart/form-data; boundary={fronteira}"
    elif dados is not None:
        corpo = json.dumps(dados).encode()
        cab["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=corpo, headers=cab, method=metodo)
    try:
        with urllib.request.urlopen(req, timeout=180, context=_TLS) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read() or b"null")
        except ValueError:
            return e.code, None


def erro(resposta):
    if isinstance(resposta, dict) and resposta.get("errors"):
        return "; ".join(f"{x.get('code')}: {x.get('message')}" for x in resposta["errors"])
    return str(resposta)[:300]


def main():
    args = sys.argv[1:]
    projecto = args[args.index("--projecto") + 1] if "--projecto" in args else "kangest"
    ensaio = "--ensaio" in args
    _verificar_blake3()

    ficheiros = {}
    for f in sorted(RAIZ.rglob("*")):
        rel = f.relative_to(RAIZ).as_posix()
        if f.is_file() and f.name not in EXCLUIR and not any(p.startswith(".") for p in rel.split("/")):
            dados = f.read_bytes()
            b64 = base64.b64encode(dados).decode()
            chave = blake3((b64 + f.suffix[1:]).encode())[:32]
            tipo = mimetypes.guess_type(f.name)[0] or "application/octet-stream"
            ficheiros["/" + rel] = (chave, b64, tipo)
    for caminho, (chave, b64, tipo) in ficheiros.items():
        print(f"  {caminho:<36} {tipo:<24} {len(b64) * 3 // 4:>8} bytes  {chave}")
    if ensaio:
        return print(f"Ensaio: {len(ficheiros)} ficheiros prontos a publicar no projecto «{projecto}».")

    token = os.environ.get("CLOUDFLARE_API_TOKEN") or sys.exit("Falta CLOUDFLARE_API_TOKEN.")
    conta = os.environ.get("CLOUDFLARE_ACCOUNT_ID")
    if not conta:
        st, r = pedido("GET", "/accounts?per_page=50", token)
        if st != 200 or not r.get("result"):
            sys.exit(f"Não consegui ver a conta ({st}): {erro(r)}. Indique CLOUDFLARE_ACCOUNT_ID ou dê ao token «Account Settings · Read».")
        conta = r["result"][0]["id"]
        print(f"Conta: {r['result'][0]['name']}")

    st, r = pedido("GET", f"/accounts/{conta}/pages/projects/{projecto}", token)
    if st == 404:
        st, r = pedido("POST", f"/accounts/{conta}/pages/projects", token, {"name": projecto, "production_branch": "main"})
        if st not in (200, 201):
            sys.exit(f"Não foi possível criar o projecto ({st}): {erro(r)}")
        print(f"Projecto «{projecto}» criado.")
    elif st != 200:
        sys.exit(f"Não foi possível ler o projecto ({st}): {erro(r)}")
    subdominio = r["result"].get("subdomain") or f"{projecto}.pages.dev"

    st, r = pedido("GET", f"/accounts/{conta}/pages/projects/{projecto}/upload-token", token)
    if st != 200:
        sys.exit(f"Sem autorização para enviar ficheiros ({st}): {erro(r)}")
    jwt = r["result"]["jwt"]

    chaves = [c for c, _, _ in ficheiros.values()]
    st, r = pedido("POST", "/pages/assets/check-missing", jwt, {"hashes": chaves})
    if st != 200:
        sys.exit(f"Falhou a verificação dos ficheiros ({st}): {erro(r)}")
    em_falta = set(r["result"])
    lote = [{"key": c, "value": b64, "metadata": {"contentType": tipo}, "base64": True} for c, b64, tipo in ficheiros.values() if c in em_falta]
    if lote:
        st, r = pedido("POST", "/pages/assets/upload", jwt, lote)
        if st != 200:
            sys.exit(f"Falhou o envio dos ficheiros ({st}): {erro(r)}")
    pedido("POST", "/pages/assets/upsert-hashes", jwt, {"hashes": chaves})
    print(f"Enviados {len(lote)} ficheiros novos ({len(ficheiros) - len(lote)} já estavam no Cloudflare).")

    manifesto = {caminho: chave for caminho, (chave, _, _) in ficheiros.items()}
    st, r = pedido("POST", f"/accounts/{conta}/pages/projects/{projecto}/deployments", token,
                   multipart={"manifest": json.dumps(manifesto), "branch": "main"})
    if st not in (200, 201):
        sys.exit(f"Falhou a publicação ({st}): {erro(r)}")
    print(f"Publicado: {r['result'].get('url')}")
    print(f"Endereço do site: https://{subdominio}")


if __name__ == "__main__":
    main()

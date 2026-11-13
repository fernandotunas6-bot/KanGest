#!/usr/bin/env python3
"""Publica o servidor de licenças (servidor-licencas/) no Cloudflare Workers + D1, sem o wrangler.

    CLOUDFLARE_API_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… python3 scripts/publicar-licencas.py [--segredos] [--email-api CHAVE --remetente "KanGest <licencas@dominio>"]

Na primeira vez: cria a base D1 «kangest-licencas», aplica o schema.sql, envia o Worker com os segredos e liga o
endereço *.workers.dev. Nas seguintes: aplica o schema (só cria o que falta) e envia o código, mantendo os segredos.
--segredos volta a enviar a chave de assinatura e a chave do painel.

Segredos (nunca no repositório):
  ~/.kangest-licencas/servidor/privada.pem      chave de assinatura do servidor (a pública está no licenca.rs)
  ~/.kangest-licencas/servidor/admin-token.txt  chave do painel /admin (criada aqui se não existir)
O token precisa de «Workers Scripts · Edit» e «D1 · Edit».
"""
import json, os, secrets, ssl, subprocess, sys, urllib.error, urllib.request, uuid
from pathlib import Path

API = "https://api.cloudflare.com/client/v4"
RAIZ = Path(__file__).resolve().parent.parent / "servidor-licencas"
PASTA = Path(os.environ.get("KANGEST_LICENCAS", Path.home() / ".kangest-licencas")) / "servidor"
NOME = "kangest-licencas"
DATA_COMPAT = "2026-10-01"


def _contexto_tls():
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


def api(metodo, caminho, token, corpo=None, tipo="application/json"):
    dados = corpo if isinstance(corpo, (bytes, type(None))) else json.dumps(corpo).encode()
    req = urllib.request.Request(API + caminho, data=dados, method=metodo, headers={"Authorization": f"Bearer {token}"})
    if dados is not None:
        req.add_header("Content-Type", tipo)
    try:
        with urllib.request.urlopen(req, context=_TLS, timeout=300) as r:
            res = json.load(r)
    except urllib.error.HTTPError as e:
        try:
            res = json.load(e)
        except Exception:
            raise SystemExit(f"{metodo} {caminho}: HTTP {e.code}")
    if not res.get("success"):
        raise SystemExit(f"{metodo} {caminho}: {res.get('errors')}")
    return res["result"]


def multipart(partes):
    fronteira = "----kangest" + uuid.uuid4().hex
    corpo = b""
    for nome, ficheiro, tipo, conteudo in partes:
        disp = f'form-data; name="{nome}"' + (f'; filename="{ficheiro}"' if ficheiro else "")
        corpo += f"--{fronteira}\r\nContent-Disposition: {disp}\r\nContent-Type: {tipo}\r\n\r\n".encode() + conteudo + b"\r\n"
    corpo += f"--{fronteira}--\r\n".encode()
    return corpo, f"multipart/form-data; boundary={fronteira}"


def main():
    a = sys.argv[1:]
    token = os.environ.get("CLOUDFLARE_API_TOKEN") or sys.exit("Falta CLOUDFLARE_API_TOKEN.")
    conta = os.environ.get("CLOUDFLARE_ACCOUNT_ID") or api("GET", "/accounts", token)[0]["id"]
    opc = lambda n: a[a.index(n) + 1] if n in a else None

    # 1. painel gerado a partir do HTML
    subprocess.run(["node", str(RAIZ / "construir.mjs")], check=True)

    # 2. base de dados
    bases = api("GET", f"/accounts/{conta}/d1/database?name={NOME}", token)
    base = next((b for b in bases if b["name"] == NOME), None)
    if not base:
        base = api("POST", f"/accounts/{conta}/d1/database", token, {"name": NOME})
        print(f"Base D1 criada: {base['uuid']}")
    bid = base["uuid"]
    api("POST", f"/accounts/{conta}/d1/database/{bid}/query", token, {"sql": (RAIZ / "schema.sql").read_text()})
    print("Esquema aplicado.")

    # 3. código (os segredos já definidos mantêm-se)
    existe = any(s["id"] == NOME for s in api("GET", f"/accounts/{conta}/workers/scripts", token))
    metadados = {"main_module": "worker.js", "compatibility_date": DATA_COMPAT,
                 "bindings": [{"type": "d1", "name": "DB", "id": bid}], "keep_bindings": ["secret_text"]}
    corpo, tipo = multipart([
        ("metadata", None, "application/json", json.dumps(metadados).encode()),
        ("worker.js", "worker.js", "application/javascript+module", (RAIZ / "src/worker.js").read_bytes()),
        ("painel.js", "painel.js", "application/javascript+module", (RAIZ / "src/painel.js").read_bytes()),
    ])
    api("PUT", f"/accounts/{conta}/workers/scripts/{NOME}", token, corpo, tipo)
    print("Worker enviado.")

    # 4. segredos
    segredos = {}
    if "--segredos" in a or not existe:
        tf = PASTA / "admin-token.txt"
        if not tf.exists():
            tf.write_text(secrets.token_urlsafe(32) + "\n")
            tf.chmod(0o600)
            print(f"Chave do painel criada em {tf}")
        segredos["CHAVE_PRIVADA"] = (PASTA / "privada.pem").read_text()
        segredos["ADMIN_TOKEN"] = tf.read_text().strip()
    if opc("--email-api"):
        segredos["RESEND_API_KEY"] = opc("--email-api")
    if opc("--remetente"):
        segredos["EMAIL_REMETENTE"] = opc("--remetente")
    for nome, valor in segredos.items():
        api("PUT", f"/accounts/{conta}/workers/scripts/{NOME}/secrets", token, {"name": nome, "text": valor, "type": "secret_text"})
    if segredos:
        print("Segredos definidos: " + ", ".join(segredos))

    # 5. endereço público
    api("POST", f"/accounts/{conta}/workers/scripts/{NOME}/subdomain", token, {"enabled": True, "previews_enabled": False})
    sub = api("GET", f"/accounts/{conta}/workers/subdomain", token)["subdomain"]
    print(f"Servidor de licenças: https://{NOME}.{sub}.workers.dev  ·  painel: https://{NOME}.{sub}.workers.dev/admin")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
# © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.
"""Gera o site estático (site/) a partir de site-fonte/.

    python3 scripts/gerar-site.py

site-fonte/
  config.json     versão, links dos instaladores, contactos, endereços do SIGA e da livraria
  base.html       cabeçalho, menu e rodapé comuns ({{conteudo}} recebe cada página)
  estilo.css      estilos
  site.js         comportamento (menu, separadores, formulários)
  paginas/*.html  conteúdo de cada página; a 1.ª linha é um comentário com JSON:
                  <!--{"titulo": "…", "descricao": "…", "menu": "kangest", "imagem": "assets/…"}-->
  assets/         imagens e ícones

Marcadores: {{config.a.b}} (valor do config.json), {{icone:nome}}, {{url_pagina:nome}}.
A pasta site/ é apagada e gerada de novo (fica o .nojekyll); é essa que se publica (Cloudflare e GitHub Pages).
"""
import datetime, hashlib, json, re, shutil, sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
FONTE, SAIDA = RAIZ / "site-fonte", RAIZ / "site"

MENU = [
    ("kangest", "kangest.html", "KanGest"),
    ("migracao", "migracao-kigest.html", "Migração"),
    ("equipamento", "equipamento.html", "Equipamento POS"),
    ("siga", "siga-plus.html", "SIGA Plus"),
    ("projectos", "projectos.html", "Projectos"),
    ("aderir", "aderir.html", "Aderir"),
    ("contacto", "contacto.html", "Contacto"),
]

_SVG = 'viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"'
ICONES = {
    "baixar": f'<svg {_SVG}><path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14"/></svg>',
    "email": f'<svg {_SVG}><path d="M4 6h16v12H4z"/><path d="m4 7 8 6 8-6"/></svg>',
    "seta": f'<svg {_SVG}><path d="M5 12h14m-5-5 5 5-5 5"/></svg>',
    "externo": f'<svg {_SVG}><path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/></svg>',
    "ok": f'<svg {_SVG} stroke-width="2.4"><path d="m5 12.5 4.5 4.5L19 7.5"/></svg>',
    "documento": f'<svg {_SVG}><path d="M7 3h7l5 5v13H7z"/><path d="M14 3v5h5M10 13h6M10 17h6"/></svg>',
    "pos": f'<svg {_SVG}><rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/></svg>',
    "caixa": f'<svg {_SVG}><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/></svg>',
    "carteira": f'<svg {_SVG}><rect x="3" y="6" width="18" height="13" rx="2"/><path d="M3 10h18M7 15h3"/></svg>',
    "escudo": f'<svg {_SVG}><path d="M12 3 4 6v6c0 4.5 3.4 8.2 8 9 4.6-.8 8-4.5 8-9V6z"/><path d="m9 12 2 2 4-4"/></svg>',
    "pessoas": f'<svg {_SVG}><circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c.8-3.5 3.4-5.5 6.5-5.5s5.7 2 6.5 5.5M16 4.5a3.5 3.5 0 0 1 0 7M18 14.6c1.9.7 3.1 2.6 3.5 5.4"/></svg>',
    "rede": f'<svg {_SVG}><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="8.5" y="14" width="7" height="7" rx="1.5"/><path d="M6.5 10v2h11v-2M12 12v2"/></svg>',
    "cadeado": f'<svg {_SVG}><rect x="5" y="10" width="14" height="10" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/></svg>',
    "trocar": f'<svg {_SVG}><path d="M4 7h11l-3-3M20 17H9l3 3"/></svg>',
    "camiao": f'<svg {_SVG}><path d="M3 6h11v10H3zM14 10h4l3 3v3h-7"/><circle cx="7" cy="18" r="2"/><circle cx="17" cy="18" r="2"/></svg>',
    "chave": f'<svg {_SVG}><circle cx="8" cy="15" r="4"/><path d="m11 12 9-9M17 6l3 3M15 8l2 2"/></svg>',
    "grafico": f'<svg {_SVG}><path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/></svg>',
    "impressora": f'<svg {_SVG}><path d="M7 8V3h10v5M7 17H5a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><path d="M7 14h10v7H7z"/></svg>',
    "nuvem": f'<svg {_SVG}><path d="M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19.5 10 4 4 0 0 1 18 18z"/></svg>',
    "ferramenta": f'<svg {_SVG}><path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18v3h3l6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.4-.6-.6-2.4z"/></svg>',
    "loja": f'<svg {_SVG}><path d="M4 9h16l-1-5H5zM5 9v11h14V9M9 20v-6h6v6"/></svg>',
    "chapeu": f'<svg {_SVG}><path d="m2 9 10-5 10 5-10 5z"/><path d="M6 11v5c3 2 9 2 12 0v-5M22 9v6"/></svg>',
    "telefone": f'<svg {_SVG}><path d="M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2"/></svg>',
    "windows": '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M3 5.5 10.4 4.5v7H3zM11.3 4.4 21 3v8.5h-9.7zM3 12.4h7.4v7.1L3 18.5zM11.3 12.4H21V21l-9.7-1.4z"/></svg>',
    "apple": '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M16.4 12.6c0-2.4 2-3.6 2.1-3.7-1.1-1.7-2.9-1.9-3.5-1.9-1.5-.2-2.9.9-3.7.9-.8 0-1.9-.9-3.2-.8-1.6 0-3.2 1-4 2.5-1.7 3-.4 7.4 1.2 9.8.8 1.2 1.8 2.5 3 2.4 1.2 0 1.7-.8 3.1-.8 1.5 0 1.9.8 3.2.8 1.3 0 2.1-1.2 2.9-2.4.9-1.4 1.3-2.7 1.3-2.8-.1 0-2.4-.9-2.4-3.9zM14 5.5c.7-.8 1.1-1.9 1-3-1 0-2.1.7-2.8 1.5-.6.7-1.2 1.8-1 2.9 1.1.1 2.1-.6 2.8-1.4z"/></svg>',
    "whatsapp": '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2a10 10 0 0 0-8.6 15.1L2 22l5-1.3A10 10 0 1 0 12 2zm0 18.2a8.2 8.2 0 0 1-4.2-1.2l-.3-.2-3 .8.8-2.9-.2-.3A8.2 8.2 0 1 1 12 20.2zm4.5-6.1c-.2-.1-1.5-.7-1.7-.8-.2-.1-.4-.1-.6.1l-.8 1c-.1.2-.3.2-.5.1a6.7 6.7 0 0 1-3.3-2.9c-.2-.4.2-.4.7-1.3.1-.2 0-.3 0-.4l-.8-1.8c-.2-.5-.4-.4-.6-.4h-.5a1 1 0 0 0-.7.3 3 3 0 0 0-.9 2.2 5.2 5.2 0 0 0 1.1 2.8 11.9 11.9 0 0 0 4.6 4c1.7.7 2.4.8 3.2.7a2.8 2.8 0 0 0 1.8-1.3 2.3 2.3 0 0 0 .2-1.3c-.1-.1-.3-.2-.5-.3z"/></svg>',
}


def valor(cfg, chave):
    v = cfg
    for parte in chave.split("."):
        v = v[parte]
    return str(v)


def substituir(texto, cfg, extra):
    # primeiro os valores da página (que podem ter marcadores, ex. a versão na descrição), depois o resto
    for k, v in extra.items():
        texto = texto.replace("{{" + k + "}}", v)
    texto = re.sub(r"\{\{icone:([a-z]+)\}\}", lambda m: ICONES[m.group(1)], texto)
    texto = re.sub(r"\{\{config\.([a-zA-Z.]+)\}\}", lambda m: valor(cfg, m.group(1)), texto)
    resto = re.findall(r"\{\{[^}]+\}\}", texto)
    if resto:
        raise SystemExit(f"Marcadores por substituir: {sorted(set(resto))}")
    return texto


def url_de(cfg, ficheiro):
    return cfg["site"] + "/" + ("" if ficheiro == "index.html" else ficheiro.removesuffix(".html"))


def cabecalhos(scripts_em_linha, site, licencas):
    """Cabeçalhos de segurança do Cloudflare Pages (_headers). Os scripts em linha entram na CSP pelo hash SHA-256;
    os formulários vão para o formsubmit.co, a página de entrada consulta o servidor de licenças; a Web Analytics da Cloudflare pode ser ligada no painel do Pages."""
    import base64
    hashes = " ".join(sorted(f"'sha256-{base64.b64encode(hashlib.sha256(s.encode()).digest()).decode()}'" for s in scripts_em_linha))
    # o endereço público do site também conta como «próprio»: a página 404 carrega por ele (<base href>)
    proprio = f"'self' {site.rstrip('/')}"
    csp = "; ".join([
        "default-src 'self'",
        f"script-src {proprio} {hashes} https://static.cloudflareinsights.com",
        f"style-src {proprio} 'unsafe-inline'",
        f"img-src {proprio} data:",
        f"font-src {proprio}",
        f"connect-src 'self' https://formsubmit.co https://cloudflareinsights.com {licencas}",
        "form-action 'self' https://formsubmit.co",
        "frame-ancestors 'none'",
        # a página 404 usa <base href> com o endereço público do site
        f"base-uri 'self' {site.rstrip('/')}/",
        "object-src 'none'",
        "upgrade-insecure-requests",
    ])
    return f"""# Gerado por scripts/gerar-site.py: cabeçalhos de segurança do Cloudflare Pages.
/*
  Content-Security-Policy: {csp}
  Strict-Transport-Security: max-age=31536000; includeSubDomains
  X-Content-Type-Options: nosniff
  X-Frame-Options: DENY
  Referrer-Policy: strict-origin-when-cross-origin
  Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()
  Cross-Origin-Opener-Policy: same-origin

/assets/*
  Cache-Control: public, max-age=604800
"""


def main():
    cfg = json.loads((FONTE / "config.json").read_text())
    # os instaladores ficam no próprio site (a GitHub Actions junta-os em site/actualizacoes ao publicar):
    # «/actualizacoes/KanGest_{versao}_x64-setup.exe» leva a versão do site
    cfg["downloads"] = {k: v.replace("{versao}", cfg["versao"]) if isinstance(v, str) else v for k, v in cfg["downloads"].items()}
    base = (FONTE / "base.html").read_text()
    css, js = (FONTE / "estilo.css").read_text(), (FONTE / "site.js").read_text()
    versao_ficheiros = hashlib.sha256((css + js).encode()).hexdigest()[:8]

    if SAIDA.exists():
        shutil.rmtree(SAIDA)
    SAIDA.mkdir()
    (SAIDA / ".nojekyll").write_text("")  # GitHub Pages: servir os ficheiros tal como estão
    shutil.copytree(FONTE / "assets", SAIDA / "assets")
    (SAIDA / "estilo.css").write_text(css)
    (SAIDA / "site.js").write_text(js)

    paginas = []
    scripts_em_linha = set()
    for f in sorted((FONTE / "paginas").glob("*.html")):
        texto = f.read_text()
        m = re.match(r"\s*<!--\s*(\{.*?\})\s*-->\s*", texto, re.S)
        if not m:
            raise SystemExit(f"{f.name}: falta o comentário inicial com JSON")
        meta = json.loads(m.group(1))
        conteudo = texto[m.end():]
        activo = meta.get("menu", "")
        menu = "\n".join(
            f'      <a href="{href}"' + (' aria-current="page"' if mid == activo else "") + f">{rotulo}</a>" for mid, href, rotulo in MENU
        )
        modelo = base.replace("{{conteudo}}", conteudo)
        if f.name == "404.html":
            # o Cloudflare mostra a 404 em qualquer caminho (ex. /a/b): os endereços relativos têm de partir da raiz
            modelo = modelo.replace("<head>\n", f'<head>\n<base href="{cfg["site"]}/">\n', 1)
        pagina = substituir(
            modelo,
            cfg,
            {
                "titulo": meta["titulo"],
                "descricao": meta["descricao"],
                "url": url_de(cfg, f.name),
                "imagem": meta.get("imagem", "assets/ecras/painel.jpg"),
                "menu": menu,
                "ano": str(datetime.date.today().year),
                "versao_ficheiros": versao_ficheiros,
                "config_json": json.dumps(cfg, ensure_ascii=False),
            },
        )
        (SAIDA / f.name).write_text(pagina)
        scripts_em_linha.update(re.findall(r"<script>(.*?)</script>", pagina, flags=re.S))
        if f.name != "404.html":
            paginas.append(f.name)

    hoje = datetime.date.today().isoformat()
    urls = "\n".join(f"  <url><loc>{url_de(cfg, p)}</loc><lastmod>{hoje}</lastmod></url>" for p in paginas)
    (SAIDA / "sitemap.xml").write_text(f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{urls}\n</urlset>\n')
    # Área do produtor (admin.html): o painel do servidor de licenças (servidor-licencas/src/painel.html), a falar com ele
    # pela internet. A chave de administração escreve-se na página e fica só na sessão do browser; o servidor só aceita
    # o site nas origens de ADMIN_ORIGENS. Fora do sitemap e dos motores de busca.
    painel = (RAIZ / "servidor-licencas" / "src" / "painel.html").read_text()
    marca = 'const API = "" /* KG_API */;'
    if marca not in painel:
        sys.exit("servidor-licencas/src/painel.html: falta a marca do endereço da API (const API = \"\" /* KG_API */;)")
    admin = painel.replace(marca, f"const API = {json.dumps(cfg['web']['licencas'].rstrip('/'))};")
    (SAIDA / "admin.html").write_text(admin)
    scripts_em_linha.update(re.findall(r"<script>(.*?)</script>", admin, flags=re.S))
    # A conta de cada empresa (conta.html, como as definições da conta do Claude): licença, pagamentos, computadores e
    # segurança, a falar com o servidor de licenças (/v1/conta/*). Também fora dos motores de busca.
    conta = (FONTE / "conta.html").read_text()
    if marca not in conta:
        sys.exit("site-fonte/conta.html: falta a marca do endereço da API (const API = \"\" /* KG_API */;)")
    conta = conta.replace(marca, f"const API = {json.dumps(cfg['web']['licencas'].rstrip('/'))};")
    (SAIDA / "conta.html").write_text(conta)
    scripts_em_linha.update(re.findall(r"<script>(.*?)</script>", conta, flags=re.S))
    (SAIDA / "robots.txt").write_text(f"User-agent: *\nAllow: /\nDisallow: /admin.html\nDisallow: /conta.html\nSitemap: {cfg['site']}/sitemap.xml\n")
    (SAIDA / "_headers").write_text(cabecalhos(scripts_em_linha, cfg["site"], cfg["web"]["licencas"]))
    print(f"Site gerado em {SAIDA.relative_to(RAIZ)}/: {len(paginas)} páginas + 404, sitemap e robots ({versao_ficheiros}).")


if __name__ == "__main__":
    main()

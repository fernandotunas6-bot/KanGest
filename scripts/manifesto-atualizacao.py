#!/usr/bin/env python3
# KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
# © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.
"""Manifesto das actualizações automáticas e somas SHA-256 de uma versão.

Usado pela GitHub Actions ao publicar uma etiqueta (build.yml, passo «Manifesto das actualizações»), depois de os
três instaladores estarem prontos; também serve para publicar à mão numa pasta partilhada.

    python3 scripts/manifesto-atualizacao.py <pasta com os instaladores> <versão> [notas.md]

Grava na mesma pasta:
- kangest-atualizacao.json — o manifesto que o KanGest lê em Sistema → Sobre → Actualizações
  (ver src-tauri/src/services/atualizacao.rs e docs/ATUALIZACOES.md);
- SHA256SUMS.txt — as somas de todos os instaladores, para conferir um descarregamento à mão.

A assinatura de cada pacote vem do ficheiro «<instalador>.sig» ao lado (npx tauri signer sign), quando existe.
"""
import datetime
import hashlib
import json
import pathlib
import re
import sys

# nome do instalador (o do Tauri) → plataforma do KanGest («<sistema>-<arquitectura>», atualizacao::plataforma)
PLATAFORMAS = [
    (re.compile(r"_x64-setup\.exe$"), "windows-x86_64"),
    (re.compile(r"_aarch64\.dmg$"), "macos-aarch64"),
    (re.compile(r"_x64\.dmg$"), "macos-x86_64"),
]


def sha256(f: pathlib.Path) -> str:
    h = hashlib.sha256()
    with f.open("rb") as e:
        for bloco in iter(lambda: e.read(1 << 20), b""):
            h.update(bloco)
    return h.hexdigest()


def resumo_notas(texto: str, limite: int = 600) -> str:
    """O primeiro parágrafo das notas da versão (sem títulos nem marcação), para mostrar no programa."""
    for paragrafo in re.split(r"\n\s*\n", texto.strip()):
        p = paragrafo.strip()
        if p and not p.startswith("#"):
            p = re.sub(r"\*\*|__|`", "", " ".join(l.strip() for l in p.splitlines()))
            return p if len(p) <= limite else p[: limite - 1].rstrip() + "…"
    return ""


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    pasta, versao = pathlib.Path(sys.argv[1]), sys.argv[2].lstrip("v")
    notas = pathlib.Path(sys.argv[3]).read_text(encoding="utf-8") if len(sys.argv) > 3 and pathlib.Path(sys.argv[3]).is_file() else ""
    instaladores = sorted(f for f in pasta.iterdir() if f.is_file() and f.suffix in (".exe", ".dmg"))
    pacotes, somas = {}, []
    for f in instaladores:
        if f"_{versao}_" not in f.name:
            print(f"Manifesto: {f.name} não é da versão {versao}", file=sys.stderr)
            return 1
        soma = sha256(f)
        somas.append(f"{soma}  {f.name}")
        plataforma = next((p for padrao, p in PLATAFORMAS if padrao.search(f.name)), None)
        if not plataforma:
            continue
        pacote = {"ficheiro": f.name, "sha256": soma}
        sig = f.with_name(f.name + ".sig")
        if sig.is_file():
            pacote["assinatura"] = sig.read_text(encoding="utf-8").strip()
        pacotes[plataforma] = pacote
    if not pacotes:
        print(f"Manifesto: nenhum instalador em {pasta}", file=sys.stderr)
        return 1
    manifesto = {
        "versao": versao,
        "data": datetime.date.today().isoformat(),
        "notas": resumo_notas(notas),
        "pacotes": dict(sorted(pacotes.items())),
    }
    (pasta / "kangest-atualizacao.json").write_text(json.dumps(manifesto, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (pasta / "SHA256SUMS.txt").write_text("\n".join(somas) + "\n", encoding="utf-8")
    for p, v in manifesto["pacotes"].items():
        print(f"{p:16} {v['ficheiro']}  {v['sha256'][:16]}…  {'assinado' if 'assinatura' in v else 'sem assinatura'}")
    em_falta = {p for _, p in PLATAFORMAS} - set(pacotes)
    if em_falta:
        print(f"Aviso: sem instalador para {', '.join(sorted(em_falta))}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())

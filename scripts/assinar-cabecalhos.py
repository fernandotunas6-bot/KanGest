#!/usr/bin/env python3
# KanGest — Desenvolvido por Valentino Canguele · 926 445 277 · valentinocanguele@gmail.com
# © 2026 Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.
"""Põe o cabeçalho de autoria no topo de cada ficheiro-fonte do repositório (só os ficheiros no git).

    python3 scripts/assinar-cabecalhos.py --produto KanGest              # acrescenta onde falta
    python3 scripts/assinar-cabecalhos.py --produto KanGest --verificar  # só confere (sai com 1 se faltar)

- Idempotente: um ficheiro que já tem o cabeçalho nas primeiras linhas não muda.
- Mantém a linha `#!` (shebang) em primeiro lugar.
- NÃO toca nas migrações da base de dados: o sqlx guarda um checksum de cada migração aplicada e uma migração
  alterada, mesmo só num comentário, impede as bases dos clientes de abrir na actualização seguinte. Pelo mesmo
  motivo não toca nas migrações do Supabase (`supabase/migrations`), nem nos ficheiros gerados.
"""
import argparse
import subprocess
import sys
from pathlib import Path

MARCA = "Desenvolvido por Valentino Canguele"
ANO = 2026

# comentário de linha por extensão
COMENTARIO = {
    ".rs": "//",
    ".ts": "//",
    ".tsx": "//",
    ".js": "//",
    ".jsx": "//",
    ".mjs": "//",
    ".cjs": "//",
    ".sql": "--",
    ".py": "#",
}

# nunca: migrações (checksum), ficheiros gerados, dependências
EXCLUIR = (
    "src-tauri/migrations/",
    "supabase/migrations/",
    "site/",
    "dist/",
    "node_modules/",
    "src-tauri/gen/",
    "src-tauri/target/",
)


def cabecalho(produto: str, c: str) -> list[str]:
    return [
        f"{c} {produto} — {MARCA} · 926 445 277 · valentinocanguele@gmail.com",
        f"{c} © {ANO} Valentino Canguele. Todos os direitos reservados. Proibido copiar, alterar ou redistribuir sem autorização escrita do autor.",
    ]


def ficheiros(raiz: Path) -> list[Path]:
    saida = subprocess.run(["git", "ls-files"], cwd=raiz, capture_output=True, text=True, check=True).stdout
    lista = []
    for linha in saida.splitlines():
        if linha.startswith(EXCLUIR) or any(f"/{e}" in f"/{linha}" for e in EXCLUIR):
            continue
        p = raiz / linha
        if p.suffix in COMENTARIO and p.is_file():
            lista.append(p)
    return lista


def assinado(texto: str) -> bool:
    return MARCA in "\n".join(texto.splitlines()[:6])


def assinar(p: Path, produto: str) -> bool:
    texto = p.read_text(encoding="utf-8")
    if assinado(texto):
        return False
    linhas = texto.splitlines(keepends=True)
    novo = [linha + "\n" for linha in cabecalho(produto, COMENTARIO[p.suffix])]
    # o shebang tem de continuar a ser a primeira linha
    if linhas and linhas[0].startswith("#!"):
        resto = linhas[1:]
        p.write_text(linhas[0] + "".join(novo) + "".join(resto), encoding="utf-8")
    else:
        separa = "\n" if linhas and linhas[0].strip() else ""
        p.write_text("".join(novo) + separa + "".join(linhas), encoding="utf-8")
    return True


def main() -> int:
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument("--produto", required=True, help="Nome do produto no cabeçalho (ex.: KanGest)")
    a.add_argument("--raiz", default=".", help="Raiz do repositório (por omissão, a pasta actual)")
    a.add_argument("--verificar", action="store_true", help="Só confere; sai com 1 se algum ficheiro não tiver o cabeçalho")
    args = a.parse_args()
    raiz = Path(args.raiz).resolve()
    lista = ficheiros(raiz)
    if args.verificar:
        faltam = [p for p in lista if not assinado(p.read_text(encoding="utf-8"))]
        for p in faltam:
            print(f"sem cabeçalho: {p.relative_to(raiz)}")
        print(f"{len(lista) - len(faltam)} de {len(lista)} ficheiros com o cabeçalho de autoria.")
        return 1 if faltam else 0
    mudados = sum(assinar(p, args.produto) for p in lista)
    print(f"Cabeçalho acrescentado a {mudados} ficheiros ({len(lista) - mudados} já o tinham).")
    return 0


if __name__ == "__main__":
    sys.exit(main())

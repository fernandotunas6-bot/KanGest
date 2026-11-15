"""Versões -720 (720×450) das capturas do site, a partir das de 1440×900 em site-fonte/assets/ecras.

    python3 scripts/auditoria/miniaturas-site.py
"""
from pathlib import Path

from PIL import Image

PASTA = Path(__file__).resolve().parent.parent.parent / "site-fonte" / "assets" / "ecras"

for grande in sorted(PASTA.glob("*.jpg")):
    if grande.stem.endswith("-720") or Image.open(grande).size != (1440, 900):
        continue
    destino = PASTA / f"{grande.stem}-720.jpg"
    Image.open(grande).convert("RGB").resize((720, 450), Image.LANCZOS).save(destino, quality=86, optimize=True)
    print(destino.name)

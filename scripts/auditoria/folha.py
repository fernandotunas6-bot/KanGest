"""Junta até 4 capturas numa folha reduzida (para rever muitas de uma vez).
   python3 folha.py <saida.png> <captura1.png> [...]   — tamanho original via W e H."""
import os, sys
from PIL import Image, ImageDraw

saida, *ficheiros = sys.argv[1:]
w, h = int(os.environ.get("W", 1366)), int(os.environ.get("H", 820))
cw, ch = w // 2, h // 2
folha = Image.new("RGB", (cw * 2 + 10, (ch + 24) * 2 + 10), "white")
d = ImageDraw.Draw(folha)
for k, f in enumerate(ficheiros[:4]):
    x, y = (k % 2) * (cw + 10), (k // 2) * (ch + 34)
    d.text((x + 4, y + 4), os.path.basename(f), fill="black")
    folha.paste(Image.open(f).convert("RGB").resize((cw, ch)), (x, y + 20))
folha.save(saida)

"""Compõe a imagem do ecrã de entrada do KanGest a partir da fotografia 9363112 do Pexels (licença Pexels: uso
gratuito, também comercial, com alterações): fato azul → cinzento claro, gravata bordô → laranja, a parede lisa →
colunas brancas, e a pessoa a subir de baixo, à direita.
Uso: python3 -I scripts/compor-imagem-entrada.py <pexels-9363112 com 1600 px de largura> src/assets/acesso/entrada.jpg"""
import sys
import numpy as np
from PIL import Image, ImageFilter

origem, destino = sys.argv[1], sys.argv[2]
foto = Image.open(origem).convert("RGB")
W, H = foto.size  # 1600 x 2400
rgb = np.asarray(foto).astype(np.float32)
hsv = np.asarray(foto.convert("HSV")).astype(np.float32)
h, s, v = hsv[..., 0], hsv[..., 1], hsv[..., 2]  # 0..255

# --- fato: o azul passa a cinzento claro, com as mesmas sombras
# só do pescoço para baixo: no cabelo afro há fios escuros azulados que pareceriam fato
corpo = np.mgrid[0:H, 0:W][0] > 0.42 * H
fato = (h > 130) & (h < 180) & (s > 55) & (v > 15) & corpo
# as sombras fundas do fato (dobras, junto ao braço e às calças): escuras e azuladas, perto do fato; o cinto e o
# cordão do cartão são pretos neutros e ficam
perto = np.asarray(Image.fromarray((fato * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(15))) > 0
r_, g_, b_ = rgb[..., 0], rgb[..., 1], rgb[..., 2]
sombra = perto & (v < 120) & (b_ > r_ + 4) & ~((h > 2) & (h < 30) & (s > 60)) & corpo
fato = fato | sombra
# o que sobra escuro junto ao fato (pontinhos nas bordas do braço e nas calças), menos a zona do cinto e a pele
yy0, xx0 = np.mgrid[0:H, 0:W]
cinto = (yy0 > 0.765 * H) & (yy0 < 0.84 * H) & (xx0 > 0.38 * W) & (xx0 < 0.64 * W)
perto2 = np.asarray(Image.fromarray((fato * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(9))) > 0
resto = perto2 & (v < 75) & ~cinto & ~((h > 2) & (h < 30) & (s > 60)) & (yy0 > 0.45 * H)
fato = fato | resto
fato_m = np.asarray(Image.fromarray((fato * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.2))).astype(np.float32) / 255
lum = 0.30 * rgb[..., 0] + 0.59 * rgb[..., 1] + 0.11 * rgb[..., 2]
cinza = np.clip(lum * 1.55 + 48, 0, 238)
novo_fato = np.stack([cinza * 0.985, cinza * 0.99, cinza * 1.0], axis=-1)

# --- gravata: o bordô (só na zona da gravata) passa a laranja
yy, xx = np.mgrid[0:H, 0:W]
zona = (xx > 0.47 * W) & (xx < 0.62 * W) & (yy > 0.50 * H) & (yy < 0.92 * H)
gravata = zona & ((h > 215) | (h < 9)) & (s > 45) & (v < 210)
grav_m = np.asarray(Image.fromarray((gravata * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.0))).astype(np.float32) / 255
hsv_g = np.stack([np.full_like(h, 16.0), np.full_like(s, 225.0), np.clip(v * 1.9 + 45, 0, 254)], axis=-1)
laranja = np.asarray(Image.fromarray(hsv_g.astype(np.uint8), "HSV").convert("RGB")).astype(np.float32)

pessoa = rgb * (1 - fato_m[..., None]) + novo_fato * fato_m[..., None]
pessoa = pessoa * (1 - grav_m[..., None]) + laranja * grav_m[..., None]

# --- fundo: a parede lisa ligada às margens (enchimento a partir das bordas, numa imagem mais pequena)
f = 4
pequena = np.asarray(foto.resize((W // f, H // f), Image.BILINEAR)).astype(np.float32)
mx, mn = pequena.max(-1), pequena.min(-1)
parede = (mn > 168) & ((mx - mn) < 24)
cheio = np.zeros_like(parede)
cheio[0, :] = parede[0, :]
cheio[:, 0] = parede[:, 0]
cheio[:, -1] = parede[:, -1]
while True:
    viz = cheio.copy()
    viz[1:, :] |= cheio[:-1, :]
    viz[:-1, :] |= cheio[1:, :]
    viz[:, 1:] |= cheio[:, :-1]
    viz[:, :-1] |= cheio[:, 1:]
    viz &= parede
    if (viz == cheio).all():
        break
    cheio = viz
# junto à pessoa fica a parede original (num halo suave): o cabelo afro tem parede entre os caracóis, e assim não se
# recorta fio a fio; as colunas só aparecem mais longe
pessoa_m = Image.fromarray(((~cheio) * 255).astype(np.uint8))
halo = pessoa_m.filter(ImageFilter.MaxFilter(17)).resize((W, H), Image.BILINEAR).filter(ImageFilter.GaussianBlur(28))
alfa_fundo = 1 - np.asarray(halo).astype(np.float32) / 255

# --- tela: colunas brancas (luz fria, como uma fachada) com a pessoa a subir de baixo, à direita
CW, CH = 1800, 1500
xs = np.arange(CW, dtype=np.float32)
largura = 170.0
fase = (xs % largura) / largura
base = 228 + 16 * np.sin(np.pi * fase) ** 2          # cada coluna mais clara ao meio
aresta = np.where(fase < 0.07, 250, 0)               # faixa de luz na aresta
coluna = np.maximum(base, aresta)
grad_y = np.linspace(1.0, 0.965, CH, dtype=np.float32)[:, None]
fundo = np.stack([coluna * 0.975, coluna * 0.985, coluna * 1.0], axis=-1)[None, :, :] * grad_y[..., None]
fundo = np.clip(fundo, 0, 255)

escala = 0.6
pw, ph = int(W * escala), int(H * escala)
p_img = Image.fromarray(np.clip(pessoa, 0, 255).astype(np.uint8)).resize((pw, ph), Image.LANCZOS)
a_img = Image.fromarray((alfa_fundo * 255).astype(np.uint8)).resize((pw, ph), Image.LANCZOS)
x0, y0 = int(0.62 * CW - pw / 2), CH - ph
tela = Image.fromarray(fundo.astype(np.uint8))
corte_p = np.asarray(p_img).astype(np.float32)
corte_a = 1 - np.asarray(a_img).astype(np.float32)[..., None] / 255  # 1 = pessoa
reg = np.asarray(tela).astype(np.float32)
ys0, ys1 = max(y0, 0), min(y0 + ph, CH)
xs0, xs1 = max(x0, 0), min(x0 + pw, CW)
sub_p = corte_p[ys0 - y0:ys1 - y0, xs0 - x0:xs1 - x0]
sub_a = corte_a[ys0 - y0:ys1 - y0, xs0 - x0:xs1 - x0]
reg[ys0:ys1, xs0:xs1] = reg[ys0:ys1, xs0:xs1] * (1 - sub_a) + sub_p * sub_a
Image.fromarray(np.clip(reg, 0, 255).astype(np.uint8)).save(destino, quality=82, optimize=True, progressive=True)
print("feito", destino, CW, "x", CH, "| fato", int(fato.sum()), "px | gravata", int(gravata.sum()), "px | parede", round(float(alfa_fundo.mean()) * 100), "%")

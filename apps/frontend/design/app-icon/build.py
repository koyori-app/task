"""アプリの icon の元の絵から、PWA の icon と favicon を焼く（TASK-299）。

使い方: uv run --with pillow python build.py <元の絵> <出力の dir>
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageFilter

SRC, OUT = Path(sys.argv[1]), Path(sys.argv[2])
OUT.mkdir(parents=True, exist_ok=True)

BG = (255, 255, 255, 255)  # PWA_BACKGROUND_COLOR と同じ白
ICON_FRAC = 0.80  # 通常の icon で、絵の長い辺が占める割合
SAFE_RADIUS = 0.40  # maskable の安全域（直径 80% の円）の半径
SAFE_MARGIN = 0.96  # 安全域の縁にも触れぬよう、さらに 4% 内側に収める

src = Image.open(SRC).convert("RGBA")
alpha = src.getchannel("A")
# 絵の外に散る薄い点（alpha の低い画素）を除いた、本当の絵の範囲
solid = alpha.point(lambda v: 255 if v >= 128 else 0).getbbox()
art = src.crop((solid[0] - 4, solid[1] - 4, solid[2] + 4, solid[3] + 4))


def max_radius(im):
    """絵の中心から、目に見える画素（alpha >= 128）までの最大の距離（画素）。"""
    cx, cy = im.width / 2, im.height / 2
    a = im.getchannel("A").load()
    r = 0.0
    for y in range(im.height):
        for x in range(im.width):
            if a[x, y] >= 128:
                r = max(r, math.hypot(x + 0.5 - cx, y + 0.5 - cy))
    return r


def bake(size, scale, bg, shadow):
    """絵を正方の中心へ置いて焼く。scale は元の絵の 1 画素あたりの出力の画素。"""
    w, h = round(art.width * scale), round(art.height * scale)
    x, y = (size - w) // 2, (size - h) // 2
    im = Image.new("RGBA", (size, size), bg)
    a2 = art.resize((w, h), Image.LANCZOS)
    if shadow:
        # 札の淡い地が白に溶けて縁が消えるので、淡い影で札の輪郭を見せる
        sh = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        sh.paste((40, 40, 90, 255), (x, y + round(size * 0.015)), a2.getchannel("A").point(lambda v: int(v * 0.22)))
        im.alpha_composite(sh.filter(ImageFilter.GaussianBlur(size * 0.02)))
    im.alpha_composite(a2, (x, y))
    return im


r_art = max_radius(art)
for name, size in [("apple-touch-icon.png", 180), ("icon-192.png", 192), ("icon-512.png", 512)]:
    bake(size, ICON_FRAC * size / max(art.size), BG, True).convert("RGB").save(OUT / name, optimize=True)

maskable_scale = min(ICON_FRAC * 512 / max(art.size), SAFE_RADIUS * 512 * SAFE_MARGIN / r_art)
bake(512, maskable_scale, BG, True).convert("RGB").save(OUT / "icon-maskable-512.png", optimize=True)

for size in (16, 32):
    bake(size, size / max(art.size), (0, 0, 0, 0), False).save(OUT / f"favicon-{size}.png", optimize=True)

print("solid box", solid, "art", art.size, "art center", ((solid[0] + solid[2]) / 2, (solid[1] + solid[3]) / 2))
print("art max radius", round(r_art, 1), "maskable scale", round(maskable_scale, 4),
      "maskable max radius px", round(r_art * maskable_scale, 1), "/ safe", round(SAFE_RADIUS * 512, 1))

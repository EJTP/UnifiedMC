"""An isometric block whose faces carry a real 16x16 texture, projected texel by texel.

Minecraft draws block icons at rotation [30, 225, 0], orthographic. Worked through, that
gives a 2:1 top face and a vertical axis foreshortened to 0.866 - so the side height is
1.2247 half-widths. Guessing that ratio by eye produces a squat block; this is the number.

The texture is ours. Mojang's are theirs, and a repo that redistributes them is a repo
that fails the review it was written for.
"""
import sys
from PIL import Image, ImageDraw

sys.path.insert(0, str(__import__("pathlib").Path(__file__).parent))
from textures import STYLES, N

SS = 6
OUT = 1024
SIDE_RATIO = 1.2247

BASE = (124, 58, 237)
ACCENT = (244, 63, 94)
TOP, RIGHT, LEFT = 1.0, 0.78, 0.58


def shade(rgb, k):
    return tuple(max(0, min(255, round(c * k))) for c in rgb)


def cells(style, seed, accent=()):
    shades = STYLES[style](seed)
    return [[ACCENT if (x, y) in accent else shade(BASE, shades[y][x])
             for x in range(N)] for y in range(N)]


def inlay():
    """One part that is not like the others, big enough to still be there at 32px."""
    return {(x, y) for x in range(8, 14) for y in range(2, 8)}


def face(draw, origin, e1, e2, grid, k):
    ox, oy = origin
    for v in range(N):
        for u in range(N):
            p0 = (ox + u * e1[0] + v * e2[0], oy + u * e1[1] + v * e2[1])
            p1 = (p0[0] + e1[0], p0[1] + e1[1])
            p2 = (p1[0] + e2[0], p1[1] + e2[1])
            p3 = (p0[0] + e2[0], p0[1] + e2[1])
            draw.polygon([p0, p1, p2, p3], fill=shade(grid[v][u], k))


def render(style, plate=True, accent=True):
    size = OUT * SS
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    if plate:
        mask = Image.new("L", (size, size), 0)
        ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1],
                                               radius=int(size * 0.22), fill=255)
        ImageDraw.Draw(img).rectangle([0, 0, size - 1, size - 1], fill=(19, 19, 39))
        img.putalpha(mask)

    draw = ImageDraw.Draw(img)
    w = size * (0.34 if plate else 0.38)
    hh = w / 2
    s = w * SIDE_RATIO
    cx, cy = size / 2, size / 2 - s / 2

    top = cells(style, 1, inlay() if accent else ())
    left_face = cells(style, 2)
    right_face = cells(style, 3)

    t, l, r, b = (cx, cy - hh), (cx - w, cy), (cx + w, cy), (cx, cy + hh)
    face(draw, l, ((t[0] - l[0]) / N, (t[1] - l[1]) / N),
         ((b[0] - l[0]) / N, (b[1] - l[1]) / N), top, TOP)
    face(draw, l, ((b[0] - l[0]) / N, (b[1] - l[1]) / N), (0, s / N), left_face, LEFT)
    face(draw, b, ((r[0] - b[0]) / N, (r[1] - b[1]) / N), (0, s / N), right_face, RIGHT)

    return img.resize((OUT, OUT), Image.LANCZOS)


STYLE = "panel"

if __name__ == "__main__":
    render(STYLE, plate=True, accent=False).save("brand/icon.png")
    mark = render(STYLE, plate=False, accent=False)
    mark.save("brand/mark.png")
    for size in (16, 24, 32, 48, 64, 128, 256):
        mark.resize((size, size), Image.LANCZOS).save(f"brand/mark-{size}.png")
    print("panel ohne Akzent gerendert")

"""Four 16x16 block textures, drawn as structure rather than noise.

Noise alone reads as dirt at any size. What makes a block texture legible small is
a repeating structure the eye can latch onto: a seam, a bevel, a cluster.
"""
N = 16


def _hash(x, y, seed):
    h = (x * 374761393 + y * 668265263 + seed * 2654435761) & 0xFFFFFFFF
    h = (h ^ (h >> 13)) * 1274126177 & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFF) / 255.0


def grain(seed=1):
    """Scattered, no structure. The baseline to compare the others against."""
    return [[0.66 if (n := _hash(x, y, seed)) < 0.16 else 0.82 if n < 0.36
             else 1.0 if n < 0.78 else 1.18 for x in range(N)] for y in range(N)]


def cobble(seed=7):
    """Clustered stones with dark seams between them. Structure survives downscaling."""
    out = [[1.0] * N for _ in range(N)]
    # a few stone centres; every texel takes the brightness of its nearest centre
    centres = [(3, 3), (11, 2), (6, 8), (13, 9), (2, 11), (9, 13)]
    for y in range(N):
        for x in range(N):
            best, second = 99, 99
            for i, (cx, cy) in enumerate(centres):
                d = (x - cx) ** 2 + (y - cy) ** 2
                if d < best:
                    second, best, which = best, d, i
                elif d < second:
                    second = d
            edge = (second ** 0.5 - best ** 0.5) < 1.1      # near a border between stones
            tone = 0.86 + 0.24 * _hash(which, which, seed)  # each stone its own shade
            out[y][x] = 0.55 if edge else tone
    return out


def panel(seed=3):
    """A bevelled plate: lit top-left, shaded bottom-right, studs in the corners."""
    out = [[1.0] * N for _ in range(N)]
    for y in range(N):
        for x in range(N):
            if x == 0 or y == 0:
                k = 1.22                       # highlight edge
            elif x == N - 1 or y == N - 1:
                k = 0.62                       # shadow edge
            elif x in (2, N - 3) and 2 <= y <= N - 3:
                k = 0.80                       # inner frame
            elif y in (2, N - 3) and 2 <= x <= N - 3:
                k = 0.80
            else:
                k = 1.0 + 0.06 * (_hash(x, y, seed) - 0.5)
            out[y][x] = k
    for sx, sy in ((4, 4), (11, 4), (4, 11), (11, 11)):
        out[sy][sx] = 1.25
        out[sy + 1][sx] = 0.7
    return out


def bricks(seed=5):
    """Offset courses with mortar. The most obviously man-made of the four."""
    out = [[1.0] * N for _ in range(N)]
    for y in range(N):
        course = y // 4
        offset = 0 if course % 2 == 0 else 4
        for x in range(N):
            mortar = (y % 4 == 3) or ((x + offset) % 8 == 7)
            out[y][x] = 0.58 if mortar else 0.9 + 0.22 * _hash((x + offset) // 8, course, seed)
    return out


STYLES = {"grain": grain, "cobble": cobble, "panel": panel, "bricks": bricks}

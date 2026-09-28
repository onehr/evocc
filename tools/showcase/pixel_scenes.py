"""Small repository-owned demo inputs, not output attributed to a compiler.

These procedural RGB fixtures make compression and file operations visible.
No personal photos, downloaded artwork, or generated application screenshots.
"""


def sprite_sheet():
    palette = {".": (13, 22, 44), "w": (227, 246, 255), "b": (82, 209, 233),
               "p": (174, 113, 245), "y": (255, 206, 102), "r": (255, 113, 136)}
    sprites = [
        ["....bb....", "...bwwb...", "...bwwb...", "..bbwwbb..", ".bbbwwbbb.",
         "bbbwwwwbbb", "...byyb...", "...yrry...", "....rr...."],
        ["....yy....", "..bbbbbb..", ".bwwbbwwb.", ".bwwbbwwb.", ".bbbbbbbb.",
         "..byyyyb..", "bbbbbbbbbb", "b.bbbbbb.b", "..bb..bb.."],
        ["...yyyy...", "..ywwwwy..", ".ywwyywwy.", "ywwyyyywwy", ".ywwyywwy.",
         "..ywwwwy..", "...ywwy...", "....yy....", ".........."],
        [".ppp..ppp.", "pwwwpppppp", "pwwppppppp", "pppppppppp", ".pppppppp.",
         "..pppppp..", "...pppp...", "....pp....", ".........."],
        ["...bbbb...", "..bwwwbb..", ".bwwbbbbb.", "bbbbbbyybb", "yyyyyyyyyy",
         "bbyybbbbbb", ".bbbbbbbb.", "..bbbbbb..", "...bbbb..."],
        ["....yy....", "....yy....", "...yyyy...", "yyyywwyyyy", ".yywwwwyy.",
         "..yyyyyy..", "..yyyyyy..", ".yyy..yyy.", ".yy....yy."],
    ]
    width, height = 96, 64
    pixels = [palette["."]] * (width * height)
    for n, sprite in enumerate(sprites):
        for y, row in enumerate(sprite):
            for x, value in enumerate(row):
                for dy in range(2):
                    for dx in range(2):
                        xx, yy = 6 + (n % 3) * 32 + x * 2 + dx, 7 + (n // 3) * 32 + y * 2 + dy
                        pixels[yy * width + xx] = palette[value]
    return {"width": width, "height": height,
            "rgb": bytes(v for pixel in pixels for v in pixel).hex()}


def postcards():
    """Three deliberately distinct, tiny landscape fixtures for deduplication."""
    result = {}
    width, height = 96, 54
    for kind in ("alpine", "coast", "dunes"):
        pixels = []
        for y in range(height):
            for x in range(width):
                if kind == "alpine":
                    color = (96 + y, 162 + y, 213 + y // 2)
                    ridge = min(12 + abs(x - 29), 18 + abs(x - 72))
                    if y > ridge:
                        color = (228, 239, 234) if y < ridge + 7 else (64, 100, 119)
                    if y > 39:
                        color = (34, 123 + (y % 3) * 12, 153)
                elif kind == "coast":
                    color = (244, 159 + y, 141)
                    if (x - 70) ** 2 + (y - 17) ** 2 < 64:
                        color = (255, 224, 160)
                    if y > 27:
                        color = (35, 153 + (y % 5) * 6, 174)
                    if y > 45 - x // 12:
                        color = (247, 215, 160)
                else:
                    color = (48 + y, 39 + y, 86 + y)
                    if (x - 22) ** 2 + (y - 14) ** 2 < 45:
                        color = (255, 224, 165)
                    if y > 26 + (x - 24) ** 2 // 280:
                        color = (224, 142, 100)
                    if y > 35 + (x - 77) ** 2 // 290:
                        color = (169, 83, 93)
                pixels.extend(color)
        result[kind] = {"width": width, "height": height, "rgb": bytes(pixels).hex()}
    return result


def ppm(picture):
    return f"P6\n{picture['width']} {picture['height']}\n255\n".encode() + bytes.fromhex(picture["rgb"])

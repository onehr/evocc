"""Small repository-owned demo inputs, not output attributed to a compiler.

The procedural RGB fixture makes compression visible.
No downloaded artwork or generated application screenshots.
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

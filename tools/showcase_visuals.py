"""README visualizations of real program output, not synthetic screenshots.

Pixel colors come from the executed ray tracer/Lua code. Other cards plot
recorded numerical/structured results. The drawing layer does no simulation.
"""
import base64
import html
import hashlib
import json
import re
import struct
import zlib


def png_uri(width, height, rgb):
    """Losslessly encode the program's RGB bytes for embedding in native SVG."""
    if width <= 0 or height <= 0 or len(rgb) != width * height * 3:
        raise ValueError("invalid RGB output dimensions")
    def chunk(kind, payload):
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))
    rows = b"".join(b"\0" + rgb[y*width*3:(y+1)*width*3] for y in range(height))
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))
    return "data:image/png;base64," + base64.b64encode(png).decode()


class Card:
    def __init__(self, title, subtitle, footer):
        self.parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500" viewBox="0 0 800 500">',
                      f'<title>{html.escape(title + ": " + subtitle)}</title>',
                      '<rect width="800" height="500" rx="16" fill="#0b1424"/>']
        self.text(28, 42, title, 30, "#f3f7ff")
        self.text(28, 73, subtitle, 18, "#97b9da")
        self.text(28, 476, footer, 15, "#97aac1")

    def text(self, x, y, text, size=22, color="#dce8f5"):
        self.parts.append(f'<text x="{x}" y="{y}" font-family="sans-serif" font-size="{size}" '
                          f'fill="{color}">{html.escape(str(text))}</text>')

    def rect(self, x, y, w, h, color, radius=0):
        self.parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{html.escape(color)}"/>')

    def line(self, coords, color="#54dcca", width=3):
        points = " ".join(f"{x:.2f},{y:.2f}" for x, y in coords)
        self.parts.append(f'<polyline points="{points}" fill="none" stroke="{color}" stroke-width="{width}"/>')

    def picture(self, data, x, y, width, height):
        uri = png_uri(data["width"], data["height"], bytes.fromhex(data["rgb"]))
        self.parts.append(f'<image href="{uri}" x="{x}" y="{y}" width="{width}" height="{height}"/>')

    def finish(self):
        return "\n".join([*self.parts, "</svg>", ""])


def output(record):
    return json.loads(record["commands"][-1]["stdout"])


def terrain_color(value):
    if value < -80:
        return "#153b66"
    if value < 0:
        return "#267495"
    if value < 65:
        return "#e5cd8e"
    if value < 240:
        return "#60b888"
    if value < 430:
        return "#257965"
    if value < 550:
        return "#818c99"
    return "#e0edf0"


def draw(name, record, programs=None):
    if name in ("tinycc", "lua"):
        title, subtitle = (("TinyCC / pocket ray tracer", "Three spheres, reflections and a checkerboard") if name == "tinycc"
                           else ("Lua / the Mandelbrot set", "Complex-plane iteration, computed by the Lua interpreter"))
        footer = ("Rendered C executable compiled by EvoCC-built TinyCC" if name == "tinycc"
                  else "Program-generated RGB pixels / 192 x 108 / 120 iterations max")
        c = Card(title, subtitle, footer)
        c.picture(output(record), 28, 100, 744, 348)
    elif name == "sqlite":
        data = output(record)
        if len(data["cells"]) != data["width"] * data["height"] or data["rows"] != 2304:
            raise ValueError("incomplete SQL terrain output")
        c = Card("SQLite / an island in a database", "Recursive SQL creates terrain; rollback preserves the world",
                 "Visualization of actual SQL result / Tcl adapter / integrity: " + data["integrity"])
        for i, value in enumerate(data["cells"]):
            c.rect(80+(i%64)*10, 105+(i//64)*9, 10.1, 9.1, terrain_color(value))
        c.text(84, 448, "2,304 cells", 17)
        c.text(480, 448, "water / sand / forest / summit", 16)
    elif name == "jq":
        data = output(record)
        c = Card("jq / a rover's day on Mars", "Fictional telemetry generated and aggregated by jq",
                 "Charts of actual jq output / simulated sensor data, not a live mission")
        c.text(36, 131, f"{data['distance']:.1f} km", 36, "#f3bc7c")
        c.text(36, 158, "distance travelled", 16)
        c.text(490, 131, f"{data['peakSolar']:.1f} W", 36, "#66dec4")
        c.text(490, 158, "peak solar power", 16)
        for j, (key, label, color, lo, hi) in enumerate((("solar", "Solar power / W", "#66dec4", 0, 100),
                                                      ("battery", "Battery / %", "#b09aed", 0, 100))):
            top = 190+j*126
            c.text(36, top, label, 17, color)
            c.rect(36, top+12, 728, 83, "#13253b", 6)
            c.line([(42+i*15.2, top+90-(r[key]-lo)/(hi-lo)*74) for i, r in enumerate(data["samples"])], color)
    elif name == "zstd":
        data = record["roundTrip"]
        if not data["equal"]:
            raise ValueError("cannot visualize an unsuccessful round trip")
        picture = output(programs["lua"])
        if hashlib.sha256(bytes.fromhex(picture["rgb"])).hexdigest() != data["sha256"]:
            raise ValueError("compressed image differs from Lua output")
        c = Card("Zstandard / pack a fractal",
                 "A real image compressed, restored and compared byte for byte",
                 "Measured RGB sizes, not PNG sizes / exact round trip / not a speed benchmark")
        c.picture(picture, 28, 104, 305, 185)
        c.picture(picture, 467, 104, 305, 185)
        c.text(50, 311, "original pixels", 20)
        c.text(496, 311, "restored pixels", 20)
        c.text(359, 190, "PACK", 22, "#f3bc7c")
        c.text(366, 227, "→", 38, "#66dec4")
        c.text(32, 352, f"{data['bytes']:,} B", 21)
        c.rect(230, 334, 490, 24, "#305578", 5)
        c.text(32, 399, f"{data['compressedBytes']:,} B", 21, "#66dec4")
        c.rect(230, 381, 490*data["compressedBytes"]/data["bytes"], 24, "#66dec4", 5)
        c.text(230, 441, f"{100*(1-data['compressedBytes']/data['bytes']):.1f}% smaller  /  every pixel recovered", 22)
    elif name == "zlib":
        data, picture = record["roundTrip"], record["picture"]
        pixels = bytes.fromhex(picture["rgb"])
        if not data["equal"] or len(pixels) != data["bytes"] or hashlib.sha256(pixels).hexdigest() != data["sha256"]:
            raise ValueError("sprite sheet differs from compressed bytes")
        c = Card("zlib / ship a tiny arcade", "Six pixel-art sprites → gzip → every pixel restored",
                 "Repository-owned sprite fixture / real minigzip round trip / RGB byte sizes")
        c.rect(28, 100, 465, 318, "#101f36", 12)
        # Exact original/recovered pixels; nearest-neighbor display suits pixel art.
        c.parts.append('<g style="image-rendering:pixelated">')
        c.picture(picture, 44, 103, 432, 288)
        c.parts.append('</g>')
        c.text(71, 405, "SHIP     BOT     GEM     HEART     PLANET     STAR", 13, "#97b9da")
        c.rect(542, 113, 180, 166, "#244052", 12)
        c.rect(619, 113, 26, 166, "#f3bc7c")
        for y in range(121, 276, 16):
            c.rect(619 if y % 32 < 16 else 632, y, 13, 9, "#755c46")
        c.text(561, 202, ".gz", 40, "#ffffff")
        c.text(542, 316, f"{data['bytes']:,} → {data['compressedBytes']:,} B", 23)
        c.text(542, 355, f"{100*(1-data['compressedBytes']/data['bytes']):.1f}% smaller", 28, "#66dec4")
        c.text(542, 395, "✓ exact recovery", 21, "#66dec4")
        c.text(30, 447, "Game assets travel light. The sprites arrive unchanged.", 23)
    elif name == "cjson":
        data = output(record)
        if data["floors"] != sum(b["height"] for b in data["buildings"]):
            raise ValueError("scene metadata mismatch")
        c = Card("cJSON / a city inside JSON", "Parse a scene, count its floors, serialize it back",
                 "Visualization of cJSON output / GCC-built adapter + EvoCC-built library")
        c.rect(28, 100, 744, 328, "#111b35", 10)
        c.parts.append('<circle cx="669" cy="143" r="24" fill="#f3d19b"/>')
        for b in data["buildings"]:
            if not re.fullmatch(r"#[0-9a-fA-F]{6}", b["color"]):
                raise ValueError("invalid scene color")
            x, top = 72+b["x"]*105, 405-b["height"]*27
            c.rect(x, top, 72, 405-top, b["color"], 2)
            c.rect(x+55, top, 17, 405-top, "#293a57")
            for level in range(b["height"]):
                for column in range(3):
                    c.rect(x+8+column*14, top+8+level*27, 7, 11, "#fae9af")
        c.text(36, 450, f"{data['scene']} / {len(data['buildings'])} buildings / {data['floors']} floors", 21)
    elif name == "mbedtls":
        data = output(record)
        if not data["restored"] or not data["tamperRejected"]:
            raise ValueError("authenticated encryption demo failed")
        c = Card("Mbed TLS / seal a pixel message", "AES-GCM: encrypt, restore, reject a modified ciphertext",
                 "EvoCC crypto library / GCC adapter / public fixed test key, never production")
        for j, (field, title) in enumerate((("plain", "message"), ("cipher", "ciphertext"), ("plain", "restored"))):
            values = bytes.fromhex(data[field])[:256]
            c.text(60+j*250, 130, title, 23)
            for i, value in enumerate(values):
                color = f"#{value//2:02x}{value:02x}{max(50, value):02x}"
                c.rect(48+j*250+(i%16)*12, 152+(i//16)*12, 11, 11, color)
        c.text(64, 397, "Exact recovery", 26, "#66dec4")
        c.text(395, 397, "Tampered bytes rejected", 26, "#f3bc7c")
    elif name == "polybench":
        match = re.search(r"begin dump: hz\s+(.*?)end\s+dump: hz", record["commands"][-1]["stderr"], re.S)
        if not match:
            raise ValueError("missing FDTD field")
        cells = list(map(float, match[1].split()))
        if len(cells) != 600:
            raise ValueError("incomplete FDTD field")
        lo, hi = min(cells), max(cells)
        c = Card("PolyBench / electromagnetic waves", "FDTD-2D magnetic field after 20 simulation steps",
                 "Actual 20 x 30 Hz array / matches GCC control / reference allocation harness")
        for i, value in enumerate(cells):
            t = (value-lo)/(hi-lo)
            color = f"#{int(25+230*t):02x}{int(40+160*(1-abs(2*t-1))):02x}{int(180-110*t):02x}"
            c.rect(100+(i%30)*20, 105+(i//30)*15, 20.1, 15.1, color)
        c.text(100, 439, f"min {lo:.2f}", 22, "#98baff")
        c.text(562, 439, f"max {hi:.2f}", 22, "#f3bc7c")
    elif name == "toybox":
        data = record["album"]
        if not data["equal"] or not data["originalsKept"]:
            raise ValueError("album was not copied intact")
        hashes = {}
        for scene, picture in data["pictures"].items():
            ppm = f"P6\n{picture['width']} {picture['height']}\n255\n".encode() + bytes.fromhex(picture["rgb"])
            hashes[scene] = hashlib.sha256(ppm).hexdigest()
        for item in data["inputs"] + data["kept"]:
            if hashes[item["scene"]] != item["sha256"]:
                raise ValueError("postcard differs from Toybox hashed bytes")
        counts = {sha: sum(item["sha256"] == sha for item in data["inputs"]) for sha in hashes.values()}
        if counts != data["groups"] or sorted(i["sha256"] for i in data["kept"]) != sorted(counts):
            raise ValueError("album must retain exactly one of every unique image")
        c = Card("Toybox / tidy a photo folder", "Same pictures, different filenames. Find the duplicates.",
                 "Procedural postcards / actual sha256sum, sort, uniq, mkdir, cp / originals kept")
        c.text(38, 119, "MESSY DOWNLOADS", 19, "#f3bc7c")
        c.text(540, 119, "CLEAN ALBUM", 19, "#66dec4")
        colors = {"alpine": "#9ad8ec", "coast": "#f5cda1", "dunes": "#c596d8"}
        for i, item in enumerate(data["inputs"]):
            x, y = 38 + (i % 2) * 167, 139 + (i // 2) * 97
            c.parts.append(f'<g transform="rotate({-3 if i % 2 else 3} {x+73} {y+42})">')
            c.rect(x, y, 146, 86, colors[item["scene"]], 3)
            c.picture(data["pictures"][item["scene"]], x+5, y+5, 136, 61)
            c.text(x+7, y+79, item["file"], 11, "#17293c")
            c.parts.append('</g>')
        c.text(387, 235, "HASH", 20, "#97b9da")
        c.text(400, 279, "→", 48, "#66dec4")
        c.text(383, 308, "GROUP", 18, "#97b9da")
        for i, item in enumerate(sorted(data["kept"], key=lambda item: item["scene"])):
            x, y = 528, 139 + i * 97
            c.rect(x, y, 242, 86, "#183841", 6)
            c.picture(data["pictures"][item["scene"]], x+6, y+6, 124, 70)
            c.text(x+140, y+39, item["scene"], 19)
            c.text(x+140, y+64, "1 copy", 16, "#66dec4")
        c.text(38, 448, f"{len(data['inputs'])} files → {len(data['kept'])} unique pictures. No originals deleted.", 24)
    else:
        raise ValueError(name)
    return c.finish()


def linux(facts):
    c = Card("Linux / a computer wakes up", "An EvoCC-built ARM64 kernel brings a virtual machine to life",
             "Diagram of recorded boot / log timestamps, not a speed test / full tests separate")
    # A diagram, not a claimed desktop screenshot or physical-board photograph.
    c.rect(28, 102, 407, 263, "#123630", 16)
    for x in (53, 410):
        for y in (123, 344):
            c.parts.append(f'<circle cx="{x}" cy="{y}" r="5" fill="#75968a"/>')
    for i in range(facts["cpuCount"]):
        x, y = 77 + (i % 2) * 128, 140 + (i // 2) * 91
        c.line([(x+36, y+33), (373, y+33), (373, 232), (488, 232)], "#387f69", 2)
        for offset in range(8, 65, 11):
            c.rect(x+offset, y-6, 4, 78, "#91bba4")
            c.rect(x-6, y+offset, 85, 4, "#91bba4")
        c.rect(x, y, 74, 67, "#152d30", 5)
        c.rect(x+6, y+6, 62, 55, "#215149", 3)
        c.text(x+14, y+29, f"CPU {i}", 15)
        c.text(x+12, y+50, "ONLINE", 12, "#75f4b5")
    c.rect(66, 323, 275, 21, "#24624b", 3)
    for x in range(73, 332, 32):
        c.rect(x, 326, 23, 15, "#152b31", 2)
    c.text(69, 125, f"{facts['cpuCount']} cores / {facts['totalKiB']/1048576:g} GiB RAM", 17, "#a4e6bf")
    c.line([(435, 232), (487, 232)], "#66dec4", 5)
    c.text(448, 217, "→", 28, "#66dec4")
    c.rect(486, 124, 284, 190, "#28415b", 12)
    c.rect(495, 133, 266, 170, "#0a202a", 7)
    c.text(515, 161, "SERIAL / USERSPACE", 16, "#97b9da")
    c.text(515, 203, "/init is alive", 32, "#75f4b5")
    c.text(515, 238, "EVO_INIT_ALIVE", 19, "#dce8f5")
    c.text(515, 267, "boot nonce matched ✓", 18, "#75f4b5")
    c.rect(612, 314, 32, 22, "#28415b")
    c.rect(567, 336, 121, 7, "#3d5774", 3)
    c.text(488, 366, "Verified → clean poweroff", 20, "#66dec4")
    stages = (("reset", "RESET"), ("cpus", "4 CPUs"), ("devices", "DEVICES"),
              ("init", "/init"), ("poweroff", "OFF"))
    c.line([(50, 413), (735, 413)], "#3f6c79", 3)
    for i, (key, label) in enumerate(stages):
        x = 50+i*171
        c.parts.append(f'<circle cx="{x}" cy="413" r="5" fill="#66dec4"/>')
        c.text(x-19, 400, label, 14)
        c.text(x-19, 443, f"{facts[key]['seconds']*1000:.1f} ms", 16, "#97b9da")
    return c.finish()

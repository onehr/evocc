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
    elif name in ("zstd", "zlib"):
        data = record["roundTrip"]
        if not data["equal"]:
            raise ValueError("cannot visualize an unsuccessful round trip")
        picture = output(programs["lua"])
        if hashlib.sha256(bytes.fromhex(picture["rgb"])).hexdigest() != data["sha256"]:
            raise ValueError("compressed image differs from Lua output")
        c = Card(("Zstandard / pack a fractal" if name == "zstd" else "zlib / pixels survive the journey"),
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
        data = record["archive"]
        if not data["equal"]:
            raise ValueError("archive round trip failed")
        c = Card("Toybox / build a world capsule", "Bundle the SQL island and Lua renderer; unpack intact",
                 "Actual tar create/list/extract / byte equality / not full Toybox test closure")
        c.rect(48, 135, 240, 235, "#193958", 16)
        c.rect(48, 114, 107, 45, "#275679", 8)
        c.text(75, 205, "CAPSULE", 29, "#66dec4")
        c.text(75, 250, f"{data['bytes']:,} bytes", 23)
        c.text(75, 286, "2 files", 23)
        c.text(315, 250, "→", 48, "#f3bc7c")
        for j, title in enumerate(("maps / island.json", "scripts / fractal.lua")):
            c.rect(391, 145+j*126, 354, 92, "#182b44", 12)
            c.text(414, 182+j*126, title, 23)
            c.text(414, 213+j*126, "extracted bytes match", 18, "#66dec4")
        c.text(50, 431, "Small Unix tools, one self-contained executable", 23)
    else:
        raise ValueError(name)
    return c.finish()


def linux(lines):
    c = Card("Linux / from reset to userspace", "Real ARM64 boot / 7,008 EvoCC target-C objects",
             "Visualization + serial excerpt from recorded boot / not full kernel-test completion")
    for j, label in enumerate(("KERNEL", "/init", "NONCE", "POWEROFF")):
        x = 38+j*192
        c.rect(x, 125, 156, 70, "#163549", 12)
        c.text(x+13, 168, label, 22, "#66dec4")
        if j<3:
            c.text(x+163, 166, "→", 26)
    for i, line in enumerate(lines[2:]):
        c.text(36, 249+i*34, line, 18)
    return c.finish()

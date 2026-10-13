"""README visualizations of real program output, not synthetic screenshots.

Pixel colors come from the executed ray tracer/Lua code. Other cards plot
recorded numerical/structured results, or quote recorded test logs verbatim.
The drawing layer does no simulation.
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

    def text(self, x, y, text, size=22, color="#dce8f5", mono=False):
        family = "monospace" if mono else "sans-serif"
        self.parts.append(f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" '
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


def draw(name, record, programs=None):
    if name in ("tinycc", "lua"):
        title, subtitle = (("TinyCC / pocket ray tracer", "Three spheres, reflections and a checkerboard") if name == "tinycc"
                           else ("Lua / the Mandelbrot set", "Complex-plane iteration, computed by the Lua interpreter"))
        footer = ("Rendered C executable compiled by EvoCC-built TinyCC" if name == "tinycc"
                  else "Program-generated RGB pixels / 192 x 108 / 120 iterations max")
        c = Card(title, subtitle, footer)
        c.picture(output(record), 28, 100, 744, 348)
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
    else:
        raise ValueError(name)
    return c.finish()


# Log cards quote recorded logs. The text is verbatim: a long line is clipped at
# the window edge, and an ellipsis line marks lines left out between segments.
TERMINAL = {"text": "#e6edf3", "dim": "#7d8590", "green": "#7ee787", "yellow": "#d29922", "red": "#ff7b72"}
LOG_STYLES = {
    "sqlite": [(r"^0 errors out of \d+ tests", "green", True), (r"^\d\d:\d\d tcl\(.*", "dim", False)],
    "jq": [(r"^PASS", "green", True), (r"^=+$", "dim", False)],
    "cjson": [(r"^Version: .*", "dim", False)],
    "linux": [(r"^\[ *\d+\.\d+\]", "dim", False), (r"^EVO_.*", "dim", False),
              (r"evocc \d+\.\d+\.\d+ \([0-9a-f]+ \d{4}-\d{2}-\d{2}\)", "green", True)],
    "toybox": [(r"^PASS", "green", True), (r"^SKIP", "yellow", True), (r"^FAIL", "red", True)],
}
LOG_ROWS = 11
# A card listed here wraps long lines at this column, as a terminal does,
# instead of clipping them. The Linux banner must stay readable in full.
LOG_WRAP = {"linux": 45}


def log_spans(line, rules):
    """Color the parts of a line that the rules match. The text does not change."""
    matches = sorted((match.start(), index, match.end(), color, bold)
                     for index, (pattern, color, bold) in enumerate(rules)
                     for match in re.finditer(pattern, line) if match.end() > match.start())
    parts, position = [], 0
    for start, _, end, color, bold in matches:
        if start < position:
            continue
        parts += [(line[position:start], "text", False), (line[start:end], color, bold)]
        position = end
    parts.append((line[position:], "text", False))
    return [part for part in parts if part[0]]


def wrap_spans(spans, columns):
    """Split styled spans into rows of at most ``columns`` characters.

    A row ends after its last space, so a word moves to the next row whole.
    Only a word longer than a row is split. The rows join to the original text.
    """
    text = "".join(part for part, _, _ in spans)
    ends, start = [], 0
    while len(text) - start > columns:
        space = text.rfind(" ", start + 1, start + columns)
        start = space + 1 if space != -1 else start + columns
        ends.append(start)
    ends.append(len(text))
    rows, row, position = [], [], 0
    for part, color, bold in spans:
        while part:
            take = ends[len(rows)] - position
            row.append((part[:take], color, bold))
            position += len(part[:take])
            part = part[take:]
            if position == ends[len(rows)]:
                rows.append(row)
                row = []
    if row or not rows:
        rows.append(row)
    return rows


def log_card(name, record):
    """Draw a terminal window that shows verbatim excerpts of a recorded log."""
    rows = []
    for segment in record["segments"]:
        if rows:
            rows.append([("\u2026", "dim", False)])
        for line in segment["text"]:
            spans = log_spans(line.expandtabs(), LOG_STYLES[name])
            rows += wrap_spans(spans, LOG_WRAP[name]) if name in LOG_WRAP else [spans]
    if not rows or len(rows) > LOG_ROWS:
        raise ValueError(f"log excerpt does not fit the card: {name}")
    mono = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    sans = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
    title = html.escape(record["title"])
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500" viewBox="0 0 800 500" '
           f'role="img" aria-label="{title}">',
           f"<title>{title}</title>",
           '<defs><clipPath id="body"><rect x="0" y="56" width="776" height="444"/></clipPath>'
           '<linearGradient id="fade"><stop offset="0" stop-color="#0d1117" stop-opacity="0"/>'
           '<stop offset="1" stop-color="#0d1117"/></linearGradient></defs>',
           '<rect x="1" y="1" width="798" height="498" rx="16" fill="#0d1117" stroke="#30363d" '
           'stroke-width="2"/>',
           '<circle cx="32" cy="30" r="9" fill="#ff5f56"/><circle cx="60" cy="30" r="9" fill="#ffbd2e"/>'
           '<circle cx="88" cy="30" r="9" fill="#27c93f"/>',
           f'<text x="400" y="38" fill="{TERMINAL["text"]}" font-family="{sans}" font-size="24" '
           f'font-weight="600" text-anchor="middle">{title}</text>',
           '<g clip-path="url(#body)">']
    bold_attr = ' font-weight="700"'
    for row, parts in enumerate(rows):
        spans = "".join(f'<tspan fill="{TERMINAL[color]}"{bold_attr if bold else ""}>{html.escape(text)}</tspan>'
                        for text, color, bold in parts)
        out.append(f'<text x="34" y="{100 + 35 * row}" font-family="{mono}" font-size="26" '
                   f'xml:space="preserve">{spans}</text>')
    out += ["</g>", '<rect x="736" y="56" width="40" height="420" fill="url(#fade)"/>', "</svg>", ""]
    return "\n".join(out)

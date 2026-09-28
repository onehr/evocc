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


def bundle_commands(record):
    """Validate both the positive verification and the intentional negative control."""
    data = record["bundle"]
    stages = {}
    for command in record["commands"]:
        if "stage" in command:
            if command["stage"] in stages:
                raise ValueError("repeated artifact verification stage")
            stages[command["stage"]] = command
        expected = 1 if command.get("stage") == "detect-change" else 0
        if command["exitCode"] != expected or command.get("expectedExitCode", 0) != expected:
            raise ValueError("unexpected artifact verification exit code")
    if set(stages) != {"manifest", "contents", "verify", "detect-change"}:
        raise ValueError("missing artifact verification stage")
    files = data["files"]
    names = [f["name"] for f in files]
    if names != ["raytrace.c", "raytrace", "frame.rgb"]:
        raise ValueError("unexpected build artifact set")
    expected_manifest = "".join(f"{f['sha256']}  {f['name']}\n" for f in files)
    if stages["manifest"]["stdout"] != expected_manifest:
        raise ValueError("artifact manifest differs from recorded file hashes")
    if stages["contents"]["stdout"].splitlines() != names + ["SHA256SUMS"]:
        raise ValueError("archive is missing build artifacts")
    # Ignore output spacing, not missing files or changed verdicts.
    good = [line.split() for line in stages["verify"]["stdout"].splitlines()]
    bad = [line.split() for line in stages["detect-change"]["stdout"].splitlines()]
    if good != [[name + ":", "OK"] for name in names]:
        raise ValueError("restored artifacts did not all verify")
    if data["changedFile"] != "raytrace.c" or data["changedBytes"] != 1 or bad != [
            [name + ":", "FAILED" if name == "raytrace.c" else "OK"] for name in names]:
        raise ValueError("one-byte change was not detected in exactly one artifact")
    return stages


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
        data, stages = record["bundle"], bundle_commands(record)
        c = Card("Toybox / verify a build artifact", "Package the TinyCC demo's C source, ELF and rendered RGB",
                 "Real Toybox commands / disposable one-byte negative control / not a full-suite run")
        for x, label in ((28, "BUILD OUTPUT"), (288, "tar → extract"), (548, "sha256sum -c")):
            c.rect(x, 99, 224, 40, "#183044", 5)
            c.text(x+14, 125, label, 20, "#a8d4ed", mono=True)
        c.text(260, 126, "→", 23, "#66dec4")
        c.text(520, 126, "→", 23, "#66dec4")
        for i, item in enumerate(data["files"]):
            y = 175+i*37
            c.text(30, y, item["name"], 21, mono=True)
            c.text(240, y, item["sha256"][:16] + "…", 19, "#97b9da", mono=True)
            c.text(565, y, "OK", 22, "#66dec4", mono=True)
        c.text(30, 289, f"{data['archiveBytes']:,}-byte tar / manifest + byte-for-byte cmp verified", 21)
        c.rect(28, 309, 744, 136, "#172334", 8)
        c.text(44, 337, "CONTROL: flip one byte in extracted raytrace.c", 19, "#f3bc7c", mono=True)
        for i, line in enumerate(stages["detect-change"]["stdout"].splitlines()):
            c.text(44, 368+i*27, line, 20, "#f3bc7c" if "FAILED" in line else "#66dec4", mono=True)
        c.text(588, 399, "exit 1", 27, "#f3bc7c", mono=True)
    else:
        raise ValueError(name)
    return c.finish()


def linux(facts, objects, image_sha):
    c = Card("Linux / boot → module → FPU stress", "Linux 6.18.44 / AArch64 / recorded kernel + module execution",
             "Recorded HVF run / reference-built userspace tests / full kselftest + KUnit still open")
    c.text(28, 129, f"{objects:,}/{objects:,}", 37, "#66dec4")
    c.text(28, 157, "target-C objects from EvoCC", 19)
    c.text(441, 129, f"{facts['passed']:,}/{facts['expectedWorkers']:,}", 37, "#66dec4")
    c.text(441, 157, f"worker OKs / {facts['onlineCpus']} online CPUs", 19)
    c.rect(28, 180, 744, 231, "#111f31", 7)
    c.text(44, 208, "SERIAL EXCERPT / run_test_fpu.sh", 17, "#97b9da", mono=True)
    lines = [facts["moduleLoaded"], "# Running 1000 iterations on all CPUs...",
             f"… {facts['passed']:,} × [OK] test_fpu (counted; not shown individually)",
             facts["tap"], "EVO_INIT_DONE", facts["poweroff"]]
    for i, line in enumerate(lines):
        c.text(44, 240+i*29, line, 18, "#66dec4" if i == 2 else "#dce8f5", mono=True)
    c.text(28, 444, "Image SHA-256  " + image_sha[:32] + "…", 19, "#97b9da", mono=True)
    return c.finish()

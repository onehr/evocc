# Built with EvoCC: fifteen small, real demos

The README uses **five rows of three**. These are program-generated pixels or
visualizations of real output, not invented application windows. Rendering a
chart does not imply the CLI program has a GUI. No benchmark verdict is changed
by these small examples, and no kernel rebuild or large disk image is needed.

| Demo | What actually executes | Scope / results |
|---|---|---|
| Doom / Freedoom | Recorded demo1, repetition 0, frame 350 | [81 C units; paired headless demos](../../../tests/contracts/ladder-doom-receipt.json); no live input or audio |
| TinyCC ray tracer | EvoCC-built TinyCC compiles a new C program; that program computes reflections and emits RGB pixels | [11-unit compiler](../../../tests/contracts/ladder-tinycc-receipt.json); ray tracer is compiled by TinyCC, not directly by EvoCC; platform libm |
| Lua fractal | EvoCC-built Lua performs Mandelbrot iteration and computes every displayed RGB pixel | [34 C units](../../../tests/contracts/ladder-lua-receipt.json); 192×108 image, up to 120 iterations per pixel |
| SQLite island | Recursive SQL generates 2,304 terrain cells; delete/rollback restores the data and integrity check passes | [86-unit testfixture](../../../tests/contracts/ladder-sqlite-receipt.json); reference-built Tcl adapter; renderer maps SQL elevations to colors |
| jq rover | jq generates and aggregates 48 fictional sensor samples | [69 C units](../../../tests/contracts/ladder-jq-receipt.json); charts depict simulated data, not a real Mars mission |
| cJSON harbor | A small GCC-built driver uses the audited EvoCC cJSON object to parse a scene, count its floors and serialize it | [Library build](../../../tests/contracts/ladder-cjson-receipt.json); renderer draws the returned buildings, not a game engine |
| Linux kernel/module stress | Recorded kernel boots, loads `test_fpu.ko` and runs the unchanged upstream `run_test_fpu.sh`: 4,000/4,000 worker OKs, then orderly shutdown | [7,008 target-C objects](../../../tests/contracts/kernel-bpf-full-receipt.json); [runtime evidence](../../../tests/fixtures/fpu-worker-verdict.out); tests kernel/user FPU control-state isolation; reference-built userspace driver; recorded HVF lane, not a fresh boot or full kselftest/KUnit completion |
| PolyBench field | Existing EvoCC FDTD-2D executable runs 20 steps and dumps a 20×30 magnetic field | [Original kernel](../../../tests/contracts/ladder-polybench-receipt.json); exact output matches retained GCC dump; reference-built allocation harness; field colors normalized to observed min/max |
| Mbed TLS pixels | EvoCC-built crypto archive encrypts/decrypts a pixel message with AES-GCM and rejects a changed ciphertext | [Library build](../../../tests/contracts/ladder-mbedtls-receipt.json); GCC-built adapter, public fixed test key/nonce, never production; first 256 ciphertext bytes displayed, tag not pictured |
| Zstandard fractal | The real Lua RGB image is compressed and restored with byte equality | [44 C units](../../../tests/contracts/ladder-zstd-receipt.json); measured raw-RGB sizes, not PNG sizes or a speed benchmark |
| zlib arcade sprites | `minigzip` compresses/restores a repository-owned six-sprite RGB sheet, independent of the fractal | [19 C units](../../../tests/contracts/ladder-zlib-receipt.json); Python-generated input fixture, actual compression and byte-equal recovery; not a game engine or full-suite rerun |
| Toybox build artifacts | `tar` packages the TinyCC demo's source, ELF and RGB output; `sha256sum -c` and `cmp` verify the extracted files; a deliberate one-byte change is detected | [Measured executable](../../../tests/contracts/ladder-toybox-receipt.json); real artifacts, not mock filenames; expected negative exits 1, all other commands exit 0; full Toybox lane remains incomplete with its shared test failure |
| OpenTyrian | Built-in recorded shooter demo, real SDL2 framebuffer | 52/52 C units; zero C fallback; GCC and EvoCC exit 0; frames 180/360/600 match exactly |
| Neverball | `map-easy/coins.sol`, Enter at presentation 60, normal physics/render loop | 82/82 C units including map compiler; 81 in game; zero C fallback; one disclosed application macro fix; both builds exit 0 |
| Quakespasm / LibreQuake | LibreQuake v0.09-beta `demo1` through the real OpenGL engine | 77/77 C units; zero C fallback; both builds exit 0; no commercial Quake assets |

## Three native C games

The new row was captured on September 29, 2026. All game C objects were emitted
by EvoCC, assembled/linked with the reference GNU toolchain, and run as AArch64
Linux executables. SDL2, Mesa **llvmpipe**, libc/libm, image/font/audio libraries,
Xvfb and the framebuffer observation adapter are reference-built dependencies.
This is not a claim that EvoCC compiled the graphics stack or the running host
kernel. Game music/audio, multiplayer and complete gameplay were **not tested**.

[`games.json`](games.json) retains original and EvoCC compile argv, compiler,
source/header, assembly, object and executable hashes, both run commands,
exit codes and frame hashes. [`games-packages.tsv`](games-packages.tsv) identifies
the runtime packages. Upstream source commits and archive digests are pinned in
[`games.py`](../../../tools/ladder/games.py); Tyrian 2.1 and LibreQuake v0.09-beta
lite have separate data-archive hashes. Neverball's `.sol` geometry is generated
by the GCC-built map compiler and separately hashed; game execution is EvoCC.

The reference and subject use **the same explicit demo build configuration**,
not every project's default warning/tuning flags. OpenTyrian disables networking;
Neverball retains its working curl build backend but uses `online 0` at runtime;
Quakespasm disables MP3/Vorbis. Unsupported driver options are not silently erased.

The [small SDL adapter](../../../tools/showcase/sdl_capture.c) reads the actual
framebuffer before presentation. It retains only presentations 180, 360 and 600,
then requests normal shutdown with `SDL_QUIT` at 610. Neverball also receives an
Enter key at 60. OpenTyrian uses its built-in attract replay; Quakespasm uses
`+timedemo demo1`. PNGs show presentation 600, losslessly encoded without cropping,
repainting, generated overlays or upstream screenshots. The checker reconstructs
the captured PPM from each PNG and verifies its original hash.

**Comparison boundary:** OpenTyrian's three saved frames match GCC byte-for-byte.
Neverball and Quakespasm both render and exit successfully, but their captured
frame hashes differ from GCC. This lane does **not** assert visual/semantic
equivalence for those two games. Presentation count is not a synchronized
simulation-state identifier; timing and animation remain potential causes, not
a completed diagnosis. Their images establish native execution only, not a full
benchmark PASS. No existing roadmap verdict is promoted by this row.

### Neverball source correction

The sole game-source change is the disclosed
[`neverball-mtrl.patch`](../../../tools/showcase/neverball-mtrl.patch), applied to
**both** GCC and EvoCC inputs. Upstream `tobyte(f)` omitted parentheses around
its macro argument. In `toushort(40.0f)`, this produced `40 - 40 * 255 = -10160`,
then converted it to an unsigned byte. GCC's `-fsanitize=float-cast-overflow`
reports that conversion, and EvoCC correctly stops with E3063. The fix computes
`(40 - 40) * 255` instead; it does not suppress the compiler's check. The
[minimal regression](../../../tests/fixtures/neverball-material-conversion.c)
tests original rejection and corrected execution in both interpreters.

The game builds also exposed and fixed three compiler gaps: the ISO C99 option
alias, missing `__TIME__`, and an include at EOF without a trailing newline.
Each has a focused driver regression; the dependent Rust tests and Clippy were
rerun. `__DATE__` and `__TIME__` now share one translation timestamp, honoring
`SOURCE_DATE_EPOCH`.

### Game reproduction

This is a separate, bounded graphics container; do not reuse a kernel disk image.
Stage the five pinned archives under `.build/ladder/games/download/`:

- `opentyrian.tar.gz`: [pinned C source](https://codeload.github.com/opentyrian/opentyrian/tar.gz/5a9d8daa2811347bec3a0c42f8da87682378f3cb).
- `neverball.tar.gz`: [pinned C source](https://codeload.github.com/Neverball/neverball/tar.gz/d9df382786d7e5635a1457b6b465ab943cf4fee6).
- `quakespasm.tar.gz`: [pinned C source](https://codeload.github.com/sezero/quakespasm/tar.gz/f5fe17864918239d443fe4c0d6bfb980e44d19e6).
- `tyrian21.zip`: [freeware game data](https://camanis.net/tyrian/tyrian21.zip).
- `librequake-lite.zip`: [v0.09-beta lite data](https://github.com/lavenderdotpet/LibreQuake/releases/download/v0.09-beta/lite.zip).

`python3 tools/ladder/games.py prepare` checks the pins, extracts locally and
applies the disclosed Neverball patch. Use Python 3.12+ for safe tar extraction.
Mount the repository read-only at `/repo` and `.build/ladder` writable at `/work`
in `evo-flight-lab:sealed`. Install these additional reference dependencies:
`libsdl2-dev libsdl2-ttf-dev libgl-dev libgl1-mesa-dri libpng-dev libjpeg-dev
libvorbis-dev libcurl4-openssl-dev xvfb xauth pkg-config`.
The frozen base uses historical Debian snapshot metadata; package versions from
the measured container are retained in the package manifest, not inferred from
whatever a future `apt-get` happens to install.

Inside the container, on fresh trees:

```sh
python3 /repo/tools/ladder/games.py reference opentyrian --work /work/games
python3 /repo/tools/ladder/games.py reference neverball --work /work/games
python3 /repo/tools/ladder/games.py reference quakespasm --work /work/games
mkdir -p /work/games/sysroot
tar -chf /tmp/games-headers.tar /usr/include /usr/lib/gcc/aarch64-linux-gnu/14/include
tar -xf /tmp/games-headers.tar -C /work/games/sysroot
dpkg-query -W > /work/games/packages.tsv
gcc-14 -shared -fPIC -O2 -Wall -Wextra -I/usr/include/SDL2 \
  /repo/tools/showcase/sdl_capture.c -o /work/games/sdl_capture.so -lSDL2 -lGL -ldl
```

On the host, emit each game's assembly using the current EvoCC binary:

```sh
python3 tools/ladder/games.py emit opentyrian
python3 tools/ladder/games.py emit neverball
python3 tools/ladder/games.py emit quakespasm
```

Then inside the container, repeat these commands for each game name:

```sh
python3 /repo/tools/ladder/games.py link opentyrian --work /work/games
python3 /repo/tools/ladder/game_capture.py opentyrian gcc
python3 /repo/tools/ladder/game_capture.py opentyrian evocc
```

`python3 tools/showcase_games.py --publish` publishes validated captures on the
host (also expects the retained LibreQuake notices in the work directory).
`python3 tools/showcase.py --check` validates the entire committed gallery offline.
The three new PNGs total about **1.6 MiB**; source/build/data scratch work was about
**688 MiB**, not another kernel-sized image. No videos are retained.

## Linux kernel/module demo

The engineering question is whether compiled kernel code actually runs across a
module and userspace boundary. The upstream FPU test changes userspace rounding
and exception controls, invokes the kernel helper through debugfs, and checks that
userspace controls survive. The card shows the retained September 28 result on
Image `18b36fee4bb02657…`, the same Image as the 7,008-object build receipt.

[`linux-fpu.stdout.txt`](linux-fpu.stdout.txt) retains the complete raw serial log.
Rendering reuses the existing kselftest parser to verify the nonce, 1,000-iteration
announcement, four online CPUs and all 4,000 worker messages; it also requires
module loading and orderly shutdown. The condensed line on the card is labeled
as a count, not fabricated serial output or a per-CPU execution trace. These are
worker messages, not 4,000 independent TAP test cases. The original boot excerpt
is retained separately in [`linux.stdout.txt`](linux.stdout.txt).

This was the **HVF host lane with a recorded QEMU contract mismatch**, not the
locked-QEMU lane. The installed module was signed, but `CONFIG_MODULE_SIG_FORCE`
was disabled: loading does not demonstrate signature enforcement. The userspace
test driver is reference-built; the target-C provenance claim is for the kernel
and module. Full kselftest and expanded KUnit remain separate open milestones.

## Toybox build-artifact demo

This links two real demos: the EvoCC-built TinyCC produces `raytrace`, and the
EvoCC-built Toybox handles that executable, its `raytrace.c` input and the actual
`frame.rgb` output. Python only stages files and routes command output.

```text
source + compiled ELF + rendered RGB
  → sha256sum manifest → tar → extract → sha256sum -c + cmp
  → change one byte in the temporary extracted source → sha256sum -c exits 1
```

Every restored payload must first verify. The negative control must subsequently
report **only `raytrace.c` as FAILED**, while ELF and RGB remain OK. This is a
file-integrity check, not a reproducible-build, signed-release or whole-suite
claim. Original repository/build files are never modified. Exact argv, exit
codes, hashes and stdout/stderr are in [`captures.json`](captures.json) and
[`toybox.stdout.txt`](toybox.stdout.txt).

## Source and identity

Ten cases are fresh executions of existing measured builds; Linux and Doom
reuse named recorded outputs. TinyCC and Toybox executable hashes match their
original receipts. Other installed EvoCC object hashes are checked against the
original records. For the crypto adapter, **every archive member** must match an
audited EvoCC object before linking. cJSON links the audited `cJSON.o` directly.
All invoked application executable hashes are checked before/after execution;
new adapter hashes and compiler commands are recorded too. Those new hashes
identify these captures, not retroactive binary hashes for older receipts.

Runtime/CRT, libm, Tcl and reference adapters remain explicit dependencies. The
application container is not the EvoCC-built kernel shown in the boot tile.
[`captures.json`](captures.json) preserves commands, complete stdout/stderr,
fixture and executable hashes, measurements and asset hashes. Separate text
files retain raw output. The [demo sources](../../../tools/showcase) are small;
the [renderer](../../../tools/showcase_visuals.py) only plots their results.
PNG embedding preserves emitted RGB bytes exactly; palette/diagram choices for
structured data are presentation, not extra measurements.

## Reproduce

From the repository root, with existing `.build/ladder` artifacts and the sealed
`evo-flight-lab:sealed` image available (no downloads or full rebuilds):

```sh
container run --rm --platform linux/arm64 --network none --cpus 2 --memory 1G \
  -v "$PWD:/repo:ro" \
  -v "$PWD/docs/images/showcase:/repo/docs/images/showcase" \
  evo-flight-lab:sealed python3 /repo/tools/showcase.py --capture
python3 tools/showcase.py --render --check
```

The container creates temporary demo executables/data and is removed on exit.
Results are published only after all ten cases succeed. The numerical image
data is retained; no video or heavyweight build tree is added. Renderer uses
ImageMagick only to compare the existing Doom PNG against its PPM; the PPM is
also reconstructed to BGRA and matched against the accepted exact frame hash.
`--check` alone needs only Python and committed files.

## Artwork

Doom's screenshot uses **Freedoom 0.13.0 Phase 1**, not commercial Doom game
data. Its notices remain in [FREEDOOM-COPYING.txt](FREEDOOM-COPYING.txt) and
[FREEDOOM-CREDITS.txt](FREEDOOM-CREDITS.txt). LibreQuake notices are in
[LIBREQUAKE-COPYING.txt](LIBREQUAKE-COPYING.txt) and
[LIBREQUAKE-CREDITS.txt](LIBREQUAKE-CREDITS.txt). Neverball retains its
[license/exceptions](NEVERBALL-LICENSE.md) and [GPL text](NEVERBALL-COPYING.txt).
OpenTyrian's screenshot depicts the original **Tyrian 2.1 freeware** artwork;
freeware game data is not being relicensed as EvoCC-owned art. Game/project names
and artwork remain their respective owners' work; these captures imply no
endorsement. Other scenes and demo inputs are repository-owned procedural
examples. No AI-generated imagery is used.

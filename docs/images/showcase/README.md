# Built with EvoCC: fifteen small, real demos

The README uses **five rows of three**. These are program-generated pixels,
visualizations of real output, or verbatim excerpts of recorded test logs. They
are not invented application windows. Rendering a chart does not imply the CLI
program has a GUI. No benchmark verdict is changed by these small examples, and
no kernel rebuild or large disk image is needed.

| Demo | What actually executes | Scope / results |
|---|---|---|
| Doom / Freedoom | Recorded demo1, repetition 0, frame 350 | [81 C units; paired headless demos](../../../tests/contracts/ladder-doom-receipt.json); no live input or audio |
| TinyCC ray tracer | EvoCC-built TinyCC compiles a new C program; that program computes reflections and emits RGB pixels | [11-unit compiler](../../../tests/contracts/ladder-tinycc-receipt.json); ray tracer is compiled by TinyCC, not directly by EvoCC; platform libm |
| Lua fractal | EvoCC-built Lua performs Mandelbrot iteration and computes every displayed RGB pixel | [34 C units](../../../tests/contracts/ladder-lua-receipt.json); 192×108 image, up to 120 iterations per pixel |
| SQLite tests | The accepted EvoCC build of `testfixture` ran SQLite's own `veryquick` suite: 0 errors out of 329,803 tests | [86 C units](../../../tests/contracts/ladder-sqlite-receipt.json); Tcl is reference-built; see [test-log cards](#test-log-cards) |
| jq tests | `make check` ran jq's 9 test programs and Oniguruma's 21, and all passed | [70 C units](../../../tests/contracts/ladder-jq-receipt.json); the build creates a missing version stamp; see [test-log cards](#test-log-cards) |
| cJSON tests | `./cJSON_test` parses and prints the example JSON; 26 Unity programs then ran 216 tests with 0 failures | [23 C units](../../../tests/contracts/ladder-cjson-receipt.json); the Unity framework is GCC-built |
| Linux boot | Kbuild builds the kernel with `make CC=evocc`. It boots on 4 CPUs, runs `/init` and powers down | [2,799 target C units](../../../tests/contracts/kernel-direct-virt-receipt.json); GNU tools assemble, link and build host programs; see [test-log cards](#test-log-cards) |
| PolyBench field | Existing EvoCC FDTD-2D executable runs 20 steps and dumps a 20×30 magnetic field | [Original kernel](../../../tests/contracts/ladder-polybench-receipt.json); exact output matches retained GCC dump; reference-built allocation harness; field colors normalized to observed min/max |
| Mbed TLS pixels | EvoCC-built crypto archive encrypts/decrypts a pixel message with AES-GCM and rejects a changed ciphertext | [Library build](../../../tests/contracts/ladder-mbedtls-receipt.json); GCC-built adapter, public fixed test key/nonce, never production; first 256 ciphertext bytes displayed, tag not pictured |
| Zstandard fractal | The real Lua RGB image is compressed and restored with byte equality | [44 C units](../../../tests/contracts/ladder-zstd-receipt.json); measured raw-RGB sizes, not PNG sizes or a speed benchmark |
| zlib arcade sprites | `minigzip` compresses/restores a repository-owned six-sprite RGB sheet, independent of the fractal | [19 C units](../../../tests/contracts/ladder-zlib-receipt.json); Python-generated input fixture, actual compression and byte-equal recovery; not a game engine or full-suite rerun |
| Toybox tests | The original `make tests` recipe: 1,648 PASS, 1 FAIL and 20 SKIP, line for line the same as the GCC build | [203 C units](../../../tests/contracts/ladder-toybox-receipt.json); the one failure, `tar ownership`, also fails with GCC |
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
in `evo-nexlab:sealed`. Install these additional reference dependencies:
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

## Test-log cards

The SQLite, jq, cJSON, Linux and Toybox cards quote the recorded logs of accepted
runs. They run nothing new. `python3 tools/showcase.py --render-logs` reads each
log from the local `.evidence` tree, checks the verdict in the receipt and copies
whole lines. Where a receipt pins the log hash (Linux and Toybox), the log must
match it. [`captures.json`](captures.json) records the source path, the log hash,
the first line number of each excerpt and the lines themselves.

The card text is verbatim. Only trailing spaces are removed. A line that is too
long is clipped at the window edge. A line with an ellipsis marks lines that are
left out between excerpts. The colors are ours: they mark result lines and dim
progress lines and timestamps. `--check` draws each card again from
`captures.json` and compares it with the committed SVG. Where the local log
exists, it also compares the excerpt with the log.

- **SQLite.** The receipt's test command runs
  `testfixture test/testrunner.tcl --jobs 2 veryquick` as an unprivileged user.
  All 86 C units of `testfixture` come from EvoCC. The Tcl library is
  reference-built.
- **jq.** Run `20261010T094547Z`: all 70 C units come from EvoCC, and the
  original `make check V=1` passes 30 test programs, 9 from jq and 21 from
  Oniguruma. The relink and test logs contain no compile command. The earlier
  run counted only 69 units. Automake prints the compile rule for
  `src/main.c` across continued lines, and the build-log parser skipped them,
  so GCC compiled `main.c`. Also, the release ships `src/version.h` without its
  `.remake-version-h` stamp, so every later `make` regenerated the header and
  recompiled `main.c` with GCC. The parser now joins continued lines, and the
  jq build creates the stamp with an old time. See B07a in
  [TODO.md](../../../TODO.md).
- **cJSON.** `./cJSON_test` prints the examples from the cJSON README. Then 26
  Unity test programs run 216 tests: none fails, and upstream marks one as
  ignored. The Unity framework itself is GCC-built.
- **Linux.** The serial log comes from a kernel that Kbuild built directly
  with `make ARCH=arm64 HOSTCC=gcc-14 CC=evocc`, Image `16a87d16bc1e6212…`, on
  the host boot lane. Its version banner names `evocc 0.1.0`. All 2,799 target
  C objects come from EvoCC. The receipt ties each object to its compile
  command in the build log, and no GCC target C object remains. GNU tools
  assemble the 67 assembly files, link the kernel, build the host programs and
  preprocess one device-tree source. The configuration is the frozen GCC
  kernel configuration after `olddefconfig` with EvoCC. Only compiler
  capability probes differ, and the hardening features behind them are off in
  the GCC configuration too. Both configurations ask for kernel return-address
  signing. Kbuild passes `-mbranch-protection=pac-ret` to every target C
  object, and EvoCC signs the return address of every function: the Image has
  77,584 `paciasp` instructions. The host lane enables pointer
  authentication, so this boot runs the signed code. Nine compiler options
  have no effect, and the receipt lists them. The kernel also boots on the TCG
  lane, whose CPU model has no pointer authentication. Nine paired upstream
  kselftest programs, with 125 subtests, pass on both the GCC and the EvoCC
  kernel. Both lanes ran QEMU 11.1.2, which differs from the pinned
  QEMU contract; the receipt allows and records this drift. The console prints
  the kernel's earlier messages late, so the `EVO_*` lines of the test init
  program appear before `Run /init` in the log. The card leaves them out. The
  older 7,008-unit build used object substitution after a GCC Kbuild run; it
  keeps its own receipt, but its raw configuration is no longer available.
  Full kselftest and KUnit runs remain open.
- **Toybox.** The original `make tests` recipe ran 1,669 tests: 1,648 pass, 1
  fails and 20 skip. The PASS, FAIL and SKIP lines are identical to those of the
  GCC build. The one failure, `tar ownership`, fails in both builds.

## Source and identity

Six cases are fresh executions of existing measured builds. Doom reuses a named
recorded output, and five cards quote recorded test logs. The TinyCC executable
hash matches its original receipt. Other installed EvoCC object hashes are
checked against the original records. For the crypto adapter, **every archive
member** must match an audited EvoCC object before linking.
All invoked application executable hashes are checked before/after execution;
new adapter hashes and compiler commands are recorded too. Those new hashes
identify these captures, not retroactive binary hashes for older receipts.

Runtime/CRT, libm and reference adapters remain explicit dependencies. The
application container is not the EvoCC-built kernel shown in the boot tile.
[`captures.json`](captures.json) preserves commands, complete stdout/stderr,
fixture and executable hashes, measurements and asset hashes. Separate text
files retain raw output. The [demo sources](../../../tools/showcase) are small;
the [renderer](../../../tools/showcase_visuals.py) only plots their results.
PNG embedding preserves emitted RGB bytes exactly; palette/diagram choices for
structured data are presentation, not extra measurements.

## Reproduce

From the repository root, with existing `.build/ladder` artifacts and the sealed
`evo-nexlab:sealed` image available (no downloads or full rebuilds):

```sh
container run --rm --platform linux/arm64 --network none --cpus 2 --memory 1G \
  -v "$PWD:/repo:ro" \
  -v "$PWD/docs/images/showcase:/repo/docs/images/showcase" \
  evo-nexlab:sealed python3 /repo/tools/showcase.py --capture
python3 tools/showcase.py --render --check
```

`--render` also draws the test-log cards. To draw only those cards, run
`python3 tools/showcase.py --render-logs --check`. Both need the local
`.evidence` tree that the receipts name.

The container creates temporary demo executables/data and is removed on exit.
Results are published only after all six cases succeed. The numerical image
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

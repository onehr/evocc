# Showcase capture notes

The README gallery has fifteen images in five rows of three. Each image is program output, a plot
of program output or a verbatim excerpt of a recorded test log. A chart of the output of a
command-line program does not mean that the program has a graphical interface. The captures change
no benchmark verdict, and they need no kernel rebuild and no large disk image.

The six demo captures, the Doom frame and the five log cards come from the builds and the logs of
the release campaign and the kernel replay of 2026-10-27, with the release compiler of commit
`8177cff2`. The three game captures date from 2026-09-29
([Three native C games](#three-native-c-games)).

| Demo | What actually executes | Scope / results |
|---|---|---|
| Doom / Freedoom | Recorded demo1, repetition 0, frame 350 | [81 C units; paired headless demos](../../../tests/contracts/ladder-doom-receipt.json); no live input or audio |
| TinyCC ray tracer | EvoCC-built TinyCC compiles a new C program; that program computes reflections and emits RGB pixels | [11-unit compiler](../../../tests/contracts/ladder-tinycc-receipt.json); TinyCC, not EvoCC, compiles the ray tracer; platform libm |
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

The EvoCC build that emitted the game objects has the SHA-256 `5785fa7b1ba0…`
([`games.json`](games.json)). No record names its source commit; the capture commit is `85a892cd`.
The games were not built again for release 0.1.0, because their pinned sources and data are not
kept offline. EvoCC emitted all game C objects. The reference GNU toolchain assembled and linked
them into AArch64 Linux executables. SDL2, Mesa llvmpipe, libc and libm, the image, font and audio
libraries, Xvfb and the framebuffer adapter are reference-built. Game music and audio, multiplayer
and complete gameplay were not tested.

[`games.json`](games.json) keeps the original and the EvoCC compile argv, the compiler, source,
header, assembly, object and executable hashes, both run commands, the exit codes and the frame
hashes. [`games-packages.tsv`](games-packages.tsv) identifies the runtime packages.
[`games.py`](../../../tools/ladder/games.py) pins the upstream source commits and archive digests;
Tyrian 2.1 and LibreQuake v0.09-beta lite have separate data-archive hashes. The GCC-built map
compiler generates Neverball's `.sol` geometry, which has its own hash; EvoCC compiles the game.

The reference and the subject use the same explicit demo build configuration, not the default
warning and tuning flags of each project. OpenTyrian disables networking. Neverball keeps its curl
build backend and uses `online 0` at runtime. Quakespasm disables MP3 and Vorbis. Unsupported
driver options stay in the command line; the build does not remove them.

The [SDL adapter](../../../tools/showcase/sdl_capture.c) reads the framebuffer before
presentation. It keeps only presentations 180, 360 and 600, then requests normal shutdown with
`SDL_QUIT` at 610. Neverball also receives an Enter key at 60. OpenTyrian uses its built-in attract
replay; Quakespasm uses `+timedemo demo1`. Each PNG shows presentation 600, losslessly encoded,
without cropping, repainting, overlays or upstream screenshots. The checker reconstructs the
captured PPM from each PNG and verifies its original hash.

OpenTyrian's three saved frames match GCC byte for byte. Neverball and Quakespasm render and exit
with status 0, but their frame hashes differ from GCC. Their images show native execution, not
equivalence with GCC and not a benchmark PASS. A presentation number does not identify a
simulation state: timing and animation can cause the difference, and no diagnosis is complete.

### Neverball source correction

The one game-source change is
[`neverball-mtrl.patch`](../../../tools/showcase/neverball-mtrl.patch), applied to the GCC and the
EvoCC inputs. Upstream `tobyte(f)` has no parentheses around its macro argument. In
`toushort(40.0f)`, this computes `40 - 40 * 255 = -10160` and converts it to an unsigned byte.
GCC's `-fsanitize=float-cast-overflow` reports that conversion, and EvoCC stops with E3063. The
patch computes `(40 - 40) * 255`; it does not suppress the check. The
[minimal regression](../../../tests/fixtures/neverball-material-conversion.c) tests the refusal of
the original and the execution of the corrected form in both interpreters.

The game builds found three compiler gaps, each fixed with a driver regression test: the ISO C99
option alias, the missing `__TIME__`, and an include at end of file without a final newline.
`__DATE__` and `__TIME__` share one translation timestamp, which `SOURCE_DATE_EPOCH` sets.

### Game reproduction

The games run in a separate graphics container. Do not reuse a kernel disk image. Stage the five
pinned archives under `.build/ladder/games/download/`:

- `opentyrian.tar.gz`: [pinned C source](https://codeload.github.com/opentyrian/opentyrian/tar.gz/5a9d8daa2811347bec3a0c42f8da87682378f3cb).
- `neverball.tar.gz`: [pinned C source](https://codeload.github.com/Neverball/neverball/tar.gz/d9df382786d7e5635a1457b6b465ab943cf4fee6).
- `quakespasm.tar.gz`: [pinned C source](https://codeload.github.com/sezero/quakespasm/tar.gz/f5fe17864918239d443fe4c0d6bfb980e44d19e6).
- `tyrian21.zip`: [freeware game data](https://camanis.net/tyrian/tyrian21.zip).
- `librequake-lite.zip`: [v0.09-beta lite data](https://github.com/lavenderdotpet/LibreQuake/releases/download/v0.09-beta/lite.zip).

`python3 tools/ladder/games.py prepare` checks the pins, extracts the archives and applies the
Neverball patch. Use Python 3.12 or later for safe tar extraction. Mount the repository read-only at
`/repo` and `.build/ladder` writable at `/work` in `evo-nexlab:sealed`. Install these additional
reference dependencies:
`libsdl2-dev libsdl2-ttf-dev libgl-dev libgl1-mesa-dri libpng-dev libjpeg-dev
libvorbis-dev libcurl4-openssl-dev xvfb xauth pkg-config`.
The package versions of the measured container are in `games-packages.tsv`; a later `apt-get` can
install other versions.

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

On the host, emit the assembly of each game with the EvoCC binary that you test:

```sh
python3 tools/ladder/games.py emit opentyrian --evocc target/release/evocc
python3 tools/ladder/games.py emit neverball --evocc target/release/evocc
python3 tools/ladder/games.py emit quakespasm --evocc target/release/evocc
```

Then, inside the container, run these commands for each game name:

```sh
python3 /repo/tools/ladder/games.py link opentyrian --work /work/games
python3 /repo/tools/ladder/game_capture.py opentyrian gcc
python3 /repo/tools/ladder/game_capture.py opentyrian evocc
```

On the host, `python3 tools/showcase_games.py --publish` publishes the validated captures. It also
needs the LibreQuake notices in the work directory. The three PNGs total 1.6 MiB. The capture used
about 688 MiB of scratch space for sources, builds and data. No video is kept.

## Test-log cards

The SQLite, jq, cJSON, Linux and Toybox cards quote the recorded logs of accepted runs. They run
nothing new. `python3 tools/showcase.py --render-logs` reads each log from the local `.evidence`
tree, checks the verdict in the receipt and copies whole lines. Where a receipt pins the log hash
(Linux and Toybox), the log must match it. [`captures.json`](captures.json) records the source path,
the log hash, the first line number of each excerpt and the lines themselves.

The card text is verbatim; only trailing spaces are removed. A line that is too long is clipped at
the window edge. A line with an ellipsis marks lines that are left out between excerpts. The colors
are ours: they mark result lines and dim progress lines and timestamps. `--check` draws each card
again from `captures.json` and compares it with the committed SVG. Where the local log exists, it
also compares the excerpt with the log.

- The SQLite card comes from the receipt's test command,
  `testfixture test/testrunner.tcl --jobs 2 veryquick`, run as an unprivileged user. All 86 C units
  of `testfixture` come from EvoCC. The Tcl library is reference-built.
- The jq card comes from run `20261027T121424Z`. All 70 C units come from EvoCC, and the original
  `make check V=1` passes 30 test programs, 9 from jq and 21 from Oniguruma. The relink and test
  logs contain no compile command. The release ships `src/version.h` without its
  `.remake-version-h` stamp, so each `make` would regenerate the header and compile `main.c` again
  with GCC. The jq build creates the stamp with an old time (B07a in [TODO.md](../../../TODO.md)).
- The cJSON card comes from `./cJSON_test`, which prints the examples of the cJSON README. Then 26
  Unity test programs run 216 tests: none fails, and upstream marks one as ignored. The Unity
  framework is GCC-built.
- The Linux card comes from the serial log of the host boot lane on 2026-10-27, for the kernel that
  Kbuild built with `make ARCH=arm64 HOSTCC=gcc-14 CC=evocc`, Image `6df1602bbb74eef5…`. Its
  version banner names `evocc 0.1.0 (08bf09bd7631 2026-10-27)`. The console prints the earlier
  kernel messages late, so the `EVO_*` lines of the test init program appear before `Run /init` in
  the log. The card leaves them out. [Linux boot, kselftest and KUnit](../../evidence.md#linux-boot-kselftest-and-kunit)
  gives the build, boot and test results of this kernel.
- The Toybox card comes from the original `make tests` recipe, which ran 1,669 tests: 1,648 pass,
  1 fails and 20 skip. The PASS, FAIL and SKIP lines are identical to those of the GCC build. The
  one failure, `tar ownership`, fails in both builds.

## Source and identity

Six cases are fresh executions of existing measured builds. Doom reuses a named recorded output,
and five cards quote recorded test logs. The TinyCC executable hash matches its original receipt.
The other installed EvoCC object hashes are checked against the original records. For the crypto
adapter, every archive member must match an audited EvoCC object before linking. Each invoked
application executable hash is checked before and after execution; the new adapter hashes and
compiler commands are recorded too. These new hashes identify these captures. They are not binary
hashes of the older receipts.

The runtime and CRT, libm and the reference adapters stay explicit dependencies. The application
container is not the EvoCC-built kernel of the boot tile. [`captures.json`](captures.json) keeps
the commands, the complete stdout and stderr, the fixture and executable hashes, the measurements
and the asset hashes. Separate text files keep the raw output. The
[demo sources](../../../tools/showcase) are small; the
[renderer](../../../tools/showcase_visuals.py) only plots their results. PNG embedding keeps the
emitted RGB bytes exactly; the palette and diagram choices for structured data are presentation,
not measurements.

## Reproduce

From the repository root, with the existing `.build/ladder` artifacts and the sealed
`evo-nexlab:sealed` image (no downloads or full rebuilds):

```sh
container run --rm --platform linux/arm64 --network none --cpus 2 --memory 1G \
  -v "$PWD:/repo:ro" \
  -v "$PWD/docs/images/showcase:/repo/docs/images/showcase" \
  evo-nexlab:sealed python3 /repo/tools/showcase.py --capture
python3 tools/showcase.py --render --check
```

`--render` also draws the test-log cards. To draw only those cards, run
`python3 tools/showcase.py --render-logs --check`. Both need the local `.evidence` tree that the
receipts name. `python3 tools/showcase.py --check` alone needs only Python and the committed files:

```console
$ python3 tools/showcase.py --check
Showcase: 15 images, 6 output demos, 5 log excerpts and 3 paired game captures verified
```

The container makes temporary demo executables and data and is removed on exit. The capture
publishes results only after all six cases succeed. The numerical image data is kept; no video or
large build tree is added. The renderer uses ImageMagick only to compare the existing Doom PNG with
its PPM; it also reconstructs the PPM as BGRA and matches it against the accepted frame hash.

## Artwork

Doom's screenshot uses Freedoom 0.13.0 Phase 1, not commercial Doom game data. Its notices are in
[FREEDOOM-COPYING.txt](FREEDOOM-COPYING.txt) and [FREEDOOM-CREDITS.txt](FREEDOOM-CREDITS.txt).
LibreQuake notices are in [LIBREQUAKE-COPYING.txt](LIBREQUAKE-COPYING.txt) and
[LIBREQUAKE-CREDITS.txt](LIBREQUAKE-CREDITS.txt). Neverball keeps its
[license and exceptions](NEVERBALL-LICENSE.md) and [GPL text](NEVERBALL-COPYING.txt).
OpenTyrian's screenshot shows the original Tyrian 2.1 freeware artwork; EvoCC does not relicense
freeware game data as its own art. Game and project names and artwork stay the work of their
owners; these captures imply no endorsement. The other scenes and demo inputs are
repository-owned procedural examples. No AI-generated imagery is used.

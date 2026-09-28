# Built with EvoCC: capture notes

These are actual output samples, not a new benchmark campaign. `doom.png` is
an unchanged recorded framebuffer. The SVGs are **verbatim captured stdout or
serial-log excerpts rendered for readability**, not screenshots of interactive
application windows. Zstandard's panel additionally labels measurements made
by the capture runner. Click a thumbnail in the main README to see it full size.

| Tile | What it shows | Build/test record and boundary |
|---|---|---|
| Doom / Freedoom | demo1, repetition 0, frame 350 | [81 C units; paired headless demos](../../../tests/contracts/ladder-doom-receipt.json); no live input or audio |
| Linux 6.18.44 | Existing verified serial log ending in normal poweroff | [7,008 target-C objects](../../../tests/contracts/kernel-bpf-full-receipt.json); not full kselftest/KUnit completion |
| TinyCC | The EvoCC-built TinyCC compiles and executes `hello.c` | [11 C units](../../../tests/contracts/ladder-tinycc-receipt.json); generated example is compiled by TinyCC, not directly by EvoCC |
| SQLite | Queries, rollback and integrity check in an in-memory database | [86-unit testfixture](../../../tests/contracts/ladder-sqlite-receipt.json); Tcl harness dependency is reference-built |
| Lua | A small floating-point Mandelbrot script | [34 C units](../../../tests/contracts/ladder-lua-receipt.json); this capture is not another full official-suite run |
| jq | JSON grouping and revenue aggregation | [69 C units](../../../tests/contracts/ladder-jq-receipt.json); this capture is a small CLI example |
| Zstandard | Compression/decompression with exact byte comparison | [44 C units](../../../tests/contracts/ladder-zstd-receipt.json); not a performance measurement |
| cJSON | First 11 lines from the upstream JSON example | [EvoCC-built library and tests](../../../tests/contracts/ladder-cjson-receipt.json); this particular example driver is reference-built |

The six CLI examples are fresh executions of the existing measured builds.
Their installed EvoCC object hashes are checked against the original build
records before execution, except TinyCC, whose final executable hash is checked
directly against its original receipt (its intermediate objects were reused
since that run). All executable hashes are captured before and after running.
Other executables' new hashes identify these captures, not retroactive binary
hashes for older receipts. Runtime libraries/CRT remain the sealed lab's
existing dependencies. The application container is not the EvoCC-built Linux
kernel shown in the separate boot tile.

[`captures.json`](captures.json) preserves commands, stdout/stderr, input and
binary hashes, source receipt identities, and asset hashes. Separate text files
retain complete CLI output; excerpting is only for display. Examples live in
[`tools/showcase`](../../../tools/showcase).

## Reproduce

Run from the repository root, with the existing `.build/ladder` artifacts and
sealed `evo-flight-lab:sealed` image available. No downloads or full rebuilds:

```sh
container run --rm --platform linux/arm64 --network none --cpus 2 --memory 1G \
  -v "$PWD:/repo:ro" \
  -v "$PWD/docs/images/showcase:/repo/docs/images/showcase" \
  evo-flight-lab:sealed python3 /repo/tools/showcase.py --capture
python3 tools/showcase.py --render --check
```

The container creates only temporary example files and is removed on exit.
Rendering reuses the recorded Linux log and Doom frame; it does not reboot a
kernel or rerun Doom. Historical build profiles and adjustments remain in the
linked receipts. This gallery does not change any benchmark verdict.

Rendering uses ImageMagick only to compare the existing PNG's pixels with the
original PPM. The PPM is reconstructed to BGRA and checked against the accepted
Doom run's exact frame hash before copying. SVG panels normalize tabs to two
spaces and wrap long lines; they do not change values. `--check` alone is an
offline integrity check requiring only Python and these committed files.

## Artwork

The Doom screenshot shows **Freedoom 0.13.0 Phase 1** data rendered by the pinned
doomgeneric engine. Freedoom's copyright/permission notice is retained in
[`FREEDOOM-COPYING.txt`](FREEDOOM-COPYING.txt), with attribution in
[`FREEDOOM-CREDITS.txt`](FREEDOOM-CREDITS.txt). This is not a screenshot of
commercial Doom game data.

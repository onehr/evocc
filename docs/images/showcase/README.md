# Built with EvoCC: twelve small, real demos

The README uses **four rows of three**. These are program-generated pixels or
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
[FREEDOOM-CREDITS.txt](FREEDOOM-CREDITS.txt). Other scenes and demo inputs are
repository-owned procedural examples. No AI-generated imagery is used.

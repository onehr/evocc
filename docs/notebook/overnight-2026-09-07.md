# Overnight log, 2026-09-07

Each entry is one check by the architect session while the user sleeps.

## 00:xx tick 1
- main at 0bd4bfe (front end). Probed the release driver: `visibility=internal` is emitted for static functions, `&&label` lowers to `block_addr`, `__builtin_return_address(0)` lowers to `return_address`. The three items that gate the kernel lane are in.
- Started: `tools/k1/measure.sh` (K1_ALLOW_QEMU_DRIFT=1, jobs 6, max-bisections 6) and the zlib, toybox and musl rungs, in the background.
- No subagent running; the finished Csmith agent's three polling shells were killed earlier. rust-analyzer respawned once and was killed again.
- Machine: Docker VM still holds about 23 GB compressed; free 3.7 GB.

## tick 2 (03:23)
- Codex landed f727f39 (bit-field initialization and masked updates).
- zlib re-run: 17 of 19 units in a passing build (was 2). toybox re-run: 164 units compiled, none set aside for symbol collisions any more; passing-build count in the receipt. musl re-run and the kernel measurement still running (K1 at 1 h, second evidence directory measure-20260906T181955Z).
- B11 (zero data to .bss) has two commits on backend-b11, still running.
- Load average around 170: the two ladder lanes, the kernel relink and the backend gate overlap. Memory compressed 15.8 GB, free 2.1 GB.

## tick 3 (03:29, kernel measurement finished)
- Kernel mixed link at the current front end: 26 units substituted, boot PASS on the first attempt, 24 units by EvoCC proven inside a booting kernel (was 12). 16 units still excluded for duplicate symbols, but the groups are the fdt/ctype sources the kernel compiles three times for three link targets (pi, EFI stub, lib) which never share an image; the lane's collision check should scope by link target.
- The survey's face after return_address is E3021 (inline asm), 2,734 units; the constraint or feature named is being classified for the next rule.
- Codex re-ran E0 (torture 1,055 of 1,692, was 740) and Csmith (396 agree); status refreshed (590f1d3). toybox 164/201.
- Ruled: EvoIR 1.9.74, `always_inline` substitution in C-HIR (the `alternative_has_cap_likely` face, 2,727 units). For Codex in the morning: this is the single item between the front end and nearly every kernel unit; after it, the survey's face will be whatever remains under it (expect the 16-unit fdt/ctype duplicates to be a lane scoping issue, not a compiler one).

## tick 4 (03:38, B11 finished)
- Landed backend-b11: zero data to .bss, zero runs as .zero; sieve's assembly 1,000,289 lines to 287, static image 1.71 MB to 705 KB (GCC: 705 KB). Spec 0.20 records it and a Darwin variadic gap the window found (anonymous arguments go on the stack on Darwin; the Darwin lane judges exit status only).

## tick 5 (04:41, musl re-run)
- The musl re-run timed out in a container step while the machine sat at load 170 (Codex's survey and the kernel relink at the same time); its receipt still shows the earlier 178. Relaunching it alone now that the load has dropped.
- toybox 164/201 and zlib 17/19 receipts committed; tracker refreshed.

# D06 guarded division

This directory keeps the source of TODO item D06 without change.
`evocc-demo-core-integer.c` is a byte copy of
`.evidence/diagnostics/20261007/agent-demo/evocc-demo-core-integer.c`.
Its SHA-256 is
`d1858fe34e7e16bb4e2f3b24f5d1f4dfc130c9191d70d65ac02e36cdc66949a4`.

The function `divide` returns early for a negative or zero divisor. Then it
divides the constant 12 by the divisor. The emitted code checks the signed
overflow condition as the 32-bit OR of two `cset` results and tests bit 0.

## Command

Run the command from the repository root. `NEW_DIR` must not exist; the
driver creates it. `CHECKER` is a compiled `evoir-sem`. `SHA256` is the
digest of `CHECKER`.

```sh
evocc --emit=asm --target=aarch64-unknown-linux-gnu -O0 -fno-builtin \
  --color=never --proof-required=NEW_DIR \
  --proof-checker=CHECKER --proof-checker-sha256=SHA256 \
  --core-entry=divide --core-entry=main \
  tests/fixtures/d06-guarded-division/evocc-demo-core-integer.c
```

`-o` is not a proof-required option. The driver writes the assembly to
standard output and to `NEW_DIR/output.s`.

## Expected outcome

With a checker that has the D06 change, the command exits 0. Standard
error names `CHECKED_TO_FINAL_ASSEMBLY`. Both entries have these check
lines:

```text
STAGE_ACCEPTED stage=source-to-evoir entry=ENTRY route=call scope=source-to-evoir
STAGE_ACCEPTED stage=lowering entry=ENTRY scope=evoir-to-post-ssa-mir
STAGE_ACCEPTED stage=allocation entry=ENTRY functions=2 scope=post-ssa-mir-to-allocated-mir
STAGE_ACCEPTED stage=frame-emission entry=ENTRY functions=2 bytes=BYTES scope=allocated-mir-to-assembly-text
PIPELINE_ACCEPTED entry=ENTRY rewrites=0 functions=2 scope=source-to-assembly-text
```

`ENTRY` is `divide` or `main`. `BYTES` is the size of the assembly text:
1144 with release 0.1.0. The rewrite row says that no rewrite ran.
`output.s` is equal to the assembly of the same command without the proof
options.

With the checker of 2026-10-07 (SHA-256
`7cd984fd9c304c4efc4989adb17eedcd632bc15a4487edb474ade70924e74240`), the
command exits 3 and writes no output:

```text
STAGE_REJECTED stage=lowering entry=divide no checked selection or SSA transport
```

The same checker also refuses the allocation stage of these inputs:

```text
STAGE_REJECTED stage=allocation entry=divide allocation not confirmed in function divide
```

The D06 change closes both gaps.

## Refused changes

Each change below alters one proof input after export. The command must
then exit 3, write nothing to standard output and leave no `output.s` or
`record.json`.

| Changed roles | Change | Refused stage |
|---|---|---|
| `ssa`, `postssa` | `tbz` becomes `tbnz` | lowering |
| `ssa`, `postssa` | the zero-divisor branch tests `ne` | lowering |
| `ssa`, `postssa` | `orr` becomes `and`, and the `-1` operand becomes 1 | lowering |
| `ssa`, `postssa` | the `INT_MIN` operand becomes 12, and the `-1` operand becomes 1 | lowering |
| `allocation` | the entry copy reads `w3`, which holds no argument | allocation |
| `allocation` | the first compare reads `w0`, which holds the dead parameter | allocation |
| `allocation` | the entry hint places the live parameter in `x1` | allocation |

A divisor that reaches the division is at least 1. Each lowering change
traps for such a divisor, where the source returns `12 / divisor`. The
first two allocation changes read an unspecified value or the wrong value.
The last one states a false entry hint.

The replay of 2026-10-13 and its controls are in `.evidence/d06-20261013/`.

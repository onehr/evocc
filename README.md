<p align="center"><img src="docs/images/mascot.png" alt="EvoCC mascot, a chameleon holding a wrench" width="200"></p>
<h1 align="center">EvoCC</h1>
<p align="center">a C compiler whose optimizer keeps every equivalent form</p>

EvoCC is a C compiler written from scratch in Rust. It has its own preprocessor, parser, IRs, optimizer, register allocator and emitter; the platform assembler and linker are the only outside tools. The optimizer is an [e-graph](https://egraphs-good.github.io/): a rewrite adds an equivalent form instead of replacing code, a cost model picks one form at the end, and every union carries a certificate that a separate program checks. The target is AArch64 (Linux and macOS), and the program it is being brought up on is the Linux 6.18.44 `arm64 virtconfig` kernel.

![EvoCC](docs/images/evocc-concept.png)

The compiler is written by AI agents. What keeps that honest:

- **The specification decides.** Every language rule lives in [docs/evoir-spec.md](docs/evoir-spec.md) and [docs/evomir-spec.md](docs/evomir-spec.md). Before a rule is written, the behaviour is checked against GCC 14.2 and Clang 19.1 in a sealed container. Where the two disagree, the rule refuses the construct. Anything unspecified is refused with a numbered diagnostic.
- **Tests first, receipts after.** Each change starts with a commit whose failing tests are listed in advance, and ends with a receipt in `tests/contracts/` holding the numbers. An agent that finds a gap in the specification stops and reports it.
- **Numbers are produced, not typed.** [docs/status.md](docs/status.md) is rendered from the receipts by `tools/render-status.py`; `--check` fails if it drifts.

## Where it is

![Milestones](docs/images/evocc-milestones.png)

As of 2026-09-06: all 2,798 kernel translation units preprocess and parse; 30 lower to EvoIR with no diagnostic, and the rest stop at constructs that now have rules and are being implemented. 384 C programs compile to native code and agree with clang on macOS and with GCC 14 on Linux. Five EvoCC-compiled units run inside a Linux kernel that boots, with GCC compiling the rest. Of the outside test suites, 187 of 220 c-testsuite programs and 725 of 1,692 GCC torture programs pass; every failure is classified by the construct that caused it and filed as a compiler defect. The plan, the evaluation ladder and the pinned versions are in [docs/roadmap.md](docs/roadmap.md).

## Try it

```sh
cargo build --release
```

```c
// demo.c
struct point { int x, y; };
static inline int dot(struct point a, struct point b) { return a.x * b.x + a.y * b.y; }
int main(void) {
    struct point p = { .x = 3, .y = 4 };
    int n = ({ int t = dot(p, p); t + 1; });          /* statement expression: 26 */
    return __builtin_choose_expr(__builtin_constant_p(1), n / 2, 0);
}
```

The two interpreters are separate implementations and must agree; the native binary must agree with both:

```sh
$ target/release/evocc --interpret-c-hir demo.c      # checked high-level IR
13
$ target/release/evocc --interpret-evoir demo.c      # after lowering to SSA
13
$ target/release/evocc --target=aarch64-unknown-linux-gnu --emit=asm demo.c > demo.s
```

A rewrite removes the multiply but keeps the overflow obligation the program carried, because an optimization may remove work, not a fact the program must satisfy:

```sh
$ echo 'int f(int x) { return x * 1; }' > opt.c
$ target/release/evocc --emit=evoir -Oeq-scalar opt.c | tail -4
    %v2 = signed_mul_defined %v0, %v1
    ub.assume.signed_overflow %v2
    ret %v0
}
```

`evo-proofcheck before.evoir after.evoir proof.evp2` re-checks the certificate for such a rewrite. Other flags: `-E`, `--parse-only`, `--emit=c-hir`, `--emit=evoir`, and `--survey`, which reports every diagnostic in a file instead of the first.

## How it works

- C lowers to EvoIR, a block-argument SSA in which memory and observable effects are explicit `mem` and `world` tokens, undefined behaviour is an `ub.assume` obligation, and pointers carry provenance. The e-graph only sees programs whose dangerous parts are already explicit.
- Rewrites merge e-classes instead of replacing nodes. Pass order stops mattering; the cost model chooses at extraction from everything the rules proved equal.
- Each union records why it was allowed. `evo-proofcheck` replays the record against the before and after programs and shares no code with the optimizer.
- EvoMIR's register allocation is checked by an independent verifier. A randomized gate runs 21,000 generated programs as source IR, optimized IR, machine IR and native binary on both targets.
- The kernel is measured the way a compiler bring-up is done: GCC builds it, EvoCC replaces translation units one at a time, kbuild relinks, QEMU boots, and a failure bisects to one unit.

## Layout

```
crates/evocc            driver                      crates/evocc-egraph      e-graph core
crates/evocc-cpp        preprocessor                crates/evocc-eqopt       rewrites, certificates
crates/evocc-lex        lexer                       crates/evocc-proof       certificate format
crates/evocc-parser     C11 + GNU parser            crates/evocc-proofcheck  evo-proofcheck
crates/evocc-types      types and layout            crates/evocc-policy      cost and schedule policies
crates/evocc-chir       checked IR, interpreter     crates/evocc-randgate    randomized semantic gate
crates/evocc-lower      C-HIR -> EvoIR              crates/evocc-mir         EvoMIR, allocation, emission
crates/evocc-evoir      EvoIR, validator, interp    tools/k1/                kernel mixed-link lane
docs/                   specifications, roadmap, status, figures
tests/contracts/        frozen corpora and receipts
```

Every landing passes the workspace tests, `clippy -D warnings`, `rustfmt`, Ruff, both layout corpora against the sealed oracles, and an isolated offline replay; `tools/gate-changed.sh` runs the same gate for only the crates a change touched. Figures come from `tools/render-figures.py`.

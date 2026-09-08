<p align="center"><img src="docs/images/mascot.png" alt="EvoCC mascot, a chameleon holding a wrench" width="200"></p>
<h1 align="center">EvoCC</h1>
<p align="center">a C compiler whose optimizer keeps every equivalent form</p>

EvoCC is a C compiler written from scratch in Rust. It has its own preprocessor, parser, IRs, optimizer, register allocator and emitter; the platform assembler and linker are the only outside tools. The optimizer is an [e-graph](https://egraphs-good.github.io/): a rewrite adds an equivalent form instead of replacing code, a cost model picks one form at the end, and every union carries a certificate that a separate program checks. The target is AArch64 (Linux and macOS), and the program it is being brought up on is the Linux 6.18.44 `arm64 virtconfig` kernel.

![EvoCC](docs/images/evocc-concept.png)

The compiler is written by AI agents. What keeps that honest:

- **The specification decides.** Every language rule lives in [docs/evoir-spec.md](docs/evoir-spec.md) and [docs/evomir-spec.md](docs/evomir-spec.md). Before a rule is written, the behaviour is checked against GCC 14.2 and Clang 19.1 in a sealed container. Where the two disagree, the rule refuses the construct. Anything unspecified is refused with a numbered diagnostic.
- **Tests first, receipts after.** Each change starts with a commit whose failing tests are listed in advance, and ends with a receipt in `tests/contracts/` holding the numbers. An agent that finds a gap in the specification stops and reports it.
- **Numbers are produced, not typed.** Run milestones through `python3 tools/bench.py csmith cjson` (or `--list`, or `all`). One release compiler is measured across the selected lanes; failures, unavailable oracles and missing adapters remain visible. [docs/status.md](docs/status.md) is rendered from the receipts by `tools/render-status.py`; `--check` fails if it drifts.

## Where it is

![Milestones](docs/images/evocc-milestones.png)

The frozen front-end gate covers 2,798 kernel translation units. End-to-end
native coverage is still partial: a passing interpreter corpus or a mixed-link
boot does not establish a fully EvoCC-built kernel. Current, command-produced
figures are in [docs/status.md](docs/status.md), with configurations, limitations
and blocking constructs in the [benchmark review](docs/notebook/benchmark-evaluation.md).
The illustration above is not a live counter. The evaluation ladder and pinned
subjects are in [docs/roadmap.md](docs/roadmap.md).

## Try it

### Evaluate the roadmap

```sh
python3 tools/bench.py --list
python3 tools/bench.py csmith cjson
python3 tools/bench.py torture zlib toybox --jobs 4
python3 tools/bench.py all --programs 500 --jobs 4 --budget 20 --allow-qemu-drift
```

`all` runs every available lane sequentially and lists missing adapters explicitly.
It is a long native evaluation, not a unit-test shortcut. Csmith's default is
500 programs **per campaign** (1,000 total); `--budget` limits toybox/musl
bisection probes, not corpus size. `--allow-qemu-drift` explicitly acknowledges
the installed QEMU digest for the kernel lane. `--dry-run` prints commands;
`--evocc PATH` measures an existing compiler without rebuilding it.

The script streams lane output, keeps only a small summary under `.evidence/bench/`,
updates `tests/contracts/bench-receipt.json`, and renders `docs/status.md`.
Single-lane runs retain older readings for other milestones, explicitly marked
as previous measurements with their original compiler identity.
Existing lane evidence is reused, not copied into another archive. Exit 0 means
all selected gates pass; 1 means measured gaps or failures; 2 means a runner or
evidence error. Do not run the legacy lane scripts concurrently against the same
build directories. Partial mixed-link coverage is not a fully compiled milestone.
A PASS is bounded by the displayed configuration and population; a smaller
`--programs` experiment does not replace the full campaign's acceptance gate.
Native lanes require the existing sealed flight-lab image and their pinned
source/build inputs. Missing prerequisites are reported as errors, never skips.

### Compile a program

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
- The rewrite rules themselves are machine-checked. Every rule of the scalar registry and every row of the known-bits analysis it relies on is proved in Lean 4 against one model of the EvoIR scalar semantics, for every width the profile admits; an audit lists the axioms each proof uses and writes a receipt, and the Rust implementations are compared with the Lean model on 1.3 million vectors. This checks the rules, not the whole compiler: the front end, the backend and the memory model are not proved.
- EvoMIR's register allocation is checked by an independent verifier. A randomized gate runs 21,000 generated programs as source IR, optimized IR, machine IR and native binary on both targets.
- The kernel is measured the way a compiler bring-up is done: GCC builds it, EvoCC replaces translation units one at a time, kbuild relinks, QEMU boots, and a failure bisects to one unit.

## Layout

```
crates/evocc            driver                      crates/evocc-egraph      e-graph core
crates/evocc-cpp        preprocessor                crates/evocc-eqopt       rewrites, certificates
crates/evocc-lex        lexer                       crates/evocc-proof       certificate format
crates/evocc-parser     C11 + GNU parser            crates/evocc-proofcheck  evo-proofcheck
lean/                   Lean 4 proofs of the rules  tools/lean               proof audit, vectors
crates/evocc-types      types and layout            crates/evocc-policy      cost and schedule policies
crates/evocc-chir       checked IR, interpreter     crates/evocc-randgate    randomized semantic gate
crates/evocc-lower      C-HIR -> EvoIR              crates/evocc-mir         EvoMIR, allocation, emission
crates/evocc-evoir      EvoIR, validator, interp    tools/k1/                kernel mixed-link lane
docs/                   specifications, roadmap, status, figures
tests/contracts/        frozen corpora and receipts
```

Every landing passes the workspace tests, `clippy -D warnings`, `rustfmt`, Ruff, both layout corpora against the sealed oracles, and an isolated offline replay; `tools/gate-changed.sh` runs the same gate for only the crates a change touched. Figures come from `tools/render-figures.py`.

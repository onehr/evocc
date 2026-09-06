# EvoCC

**A C compiler that never throws away an equivalent form of your program, proves every optimization it makes, and is aimed at booting Linux, with no LLVM or GCC anywhere inside.**

Most compilers optimize by rewriting: each pass replaces code with something it believes is better, and the original is gone. EvoCC keeps all of them. Its optimizer is an [e-graph](https://egraphs-good.github.io/): every rewrite *adds* an equivalent form instead of replacing one, a cost model picks the best at the end, and every union carries a certificate that a separate checker replays without trusting the optimizer. The target is a real one: a pinned Linux 6.18.44 `arm64 virtconfig` kernel.

![Every equivalent form, kept. Every rewrite, proved.](docs/images/evocc-concept.png)

- **From scratch.** Preprocessor, parser, IRs, optimizer, register allocator and emitter are all EvoCC's own, in Rust. The platform assembler and linker are the only outside tools.
- **Built by AI, governed by a specification.** The compiler is written by AI agents that may not guess. Every language rule is frozen in a versioned specification, measured against sealed GCC-14 and Clang-19 before it is written, and a gap in the specification stops the work instead of being papered over.
- **Every number is measured.** Status is rendered from sealed receipts by a script that fails on drift. Nothing here was typed by hand into a table.

## Four questions

1. **Can a specification be the sole authority for an AI that writes a compiler?** The AI proposes; the oracles and the checker dispose.
2. **Can AI build, from nothing, a compiler that is correct on industrial code?** Correct on the Linux kernel, checked layer by layer against compilers that are.
3. **Can an e-graph replace the optimization pipeline?** Within a fixed budget, with a proof for every rewrite.
4. **Can the optimizer improve itself?** Rules, cost model and schedule are policies the checker never has to trust, so a search can propose without being able to make the compiler wrong.

Where each question stands, with the evidence, is in [docs/status.md](docs/status.md); the reasoning and the evaluation ladder are in [docs/roadmap.md](docs/roadmap.md).

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

Two independent interpreters must agree, and the machine must agree with both:

```sh
$ target/release/evocc --interpret-c-hir demo.c      # checked high-level IR
13
$ target/release/evocc --interpret-evoir demo.c      # after lowering to SSA
13
$ target/release/evocc --target=aarch64-unknown-linux-gnu --emit=asm demo.c > demo.s
```

Watch a rewrite land and keep its obligation. The multiply disappears; the overflow fact the program carried stays, because an optimization may remove work, never a fact the program must still satisfy:

```sh
$ echo 'int f(int x) { return x * 1; }' > opt.c
$ target/release/evocc --emit=evoir -Oeq-scalar opt.c | tail -4
    %v2 = signed_mul_defined %v0, %v1
    ub.assume.signed_overflow %v2
    ret %v0
}
```

Every such rewrite emits a certificate, and `evo-proofcheck before.evoir after.evoir proof.evp2` replays it with no code shared with the optimizer. Other flags: `-E`, `--parse-only`, `--emit=c-hir`, `--emit=evoir`, and `--survey`, which reports every diagnostic in a file instead of the first.

## How it works

- **Semantics first.** C lowers to EvoIR, a block-argument SSA where memory and observable effects are explicit `mem` and `world` tokens, undefined behaviour is an `ub.assume` obligation, and pointers carry provenance. The e-graph only ever sees a program whose dangerous parts are already spelled out.
- **Equivalence is state.** Rewrites merge e-classes rather than replacing nodes; phase order stops mattering, and a cost model chooses at extraction from everything the rules could prove equal.
- **Proofs are the product.** Each union records why it was allowed; a standalone checker replays the record against the before and after programs.
- **The machine is checked too.** EvoMIR's register allocation is verified by an independent allocator checker, and a randomized gate runs 21,000 generated programs through the source IR, the optimized IR, the machine IR and the native binary on both targets.

## How the work is done

The specifications ([EvoIR](docs/evoir-spec.md), [EvoMIR](docs/evomir-spec.md)) are the only authority. A rule about observable behaviour is measured in a sealed GCC-14.2 / Clang-19.1 container before it is written down; where the two disagree, the spec takes the answer that refuses. Every change opens with a tests-only commit whose failures are declared in advance and closes with a receipt under `tests/contracts/`. The kernel is measured the way a compiler bring-up is done: GCC builds it, EvoCC replaces translation units one at a time, kbuild relinks, QEMU boots, and a failure bisects to one unit.

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
docs/                   specifications, roadmap, status
tests/contracts/        frozen corpora and receipts
```

Every landing passes the workspace tests, `clippy -D warnings`, `rustfmt`, Ruff, both layout corpora against the sealed oracles, and an isolated offline replay; `tools/gate-changed.sh` runs the same gate for only the crates a change touched.

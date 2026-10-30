# Contributing to EvoCC

Thank you for your interest in EvoCC. This page tells you how to build the compiler, how a change
moves from a failing test to a commit, and where to ask questions. [The EvoCC Book](book/src/SUMMARY.md)
explains how to use the compiler.

## Where to ask

- To ask a question or to propose an idea, open an
  [issue](https://github.com/onehr/evocc/issues).
- To report a bug, a wrong-code result or a refused proof, use the matching issue template.
- To report wrong code or a checker defect, open a pull request that adds a failing test first.
  Put the C input, the command and the expected and actual results in the test.

## Build

EvoCC builds offline. The three Rust dependencies are in `vendor/`, and
[`.cargo/config.toml`](.cargo/config.toml) points Cargo at them.

```sh
cargo build --release --offline -p evocc -p evocc-proofcheck
```

- Use the one build tree, `target/`. `.cargo/config.toml` sets two jobs. Do not make a second tree.
- Check the free disk space with `df -h .` first. Do not start a build below 25 GiB free.
- [`rust-toolchain.toml`](rust-toolchain.toml) pins Rust. [`lean/lean-toolchain`](lean/lean-toolchain)
  pins Lean.
- Build the Lean checkers from inside `lean/`: `(cd lean && lake build)`. Do not run `lake` from the
  repository root.

## The change workflow

Each change follows the same steps, in this order:

1. **Find the task.** [TODO.md](TODO.md) gives each task a stable ID. Add a task if none fits.
   `python3 tools/render-status.py --todo` checks and prints the queues.
2. **RED.** Commit a test that fails, in its own commit. It must fail for the reason that the change
   removes.
3. **Implementation.** Fix the cause in shared code. Do not special-case the test.
4. **Targeted tests.** Run the tests of the crates that you changed, for example
   `cargo test --offline -p evocc-lower`.
5. **Original-program replay.** Run the unchanged program that showed the problem again, for
   example its `tools/bench.py` lane. A reduced test that passes does not show that the original
   program passes.
6. **Gate.** Run `tools/gate-changed.sh <base-commit>`. It tests the changed crates and the crates
   that depend on them, then runs Clippy and rustfmt. Use
   `tools/gate-changed.sh --full-workspace --offline` for a full run.
7. **Receipt.** Record the result where the claim lives: a receipt in `tests/contracts/`, a row in
   [docs/evidence.md](docs/evidence.md), or the lane receipt that `tools/bench.py` writes. A receipt
   names the compiler, the inputs and the limits of the result.
8. **Tracker.** Update the line of the task in TODO.md in the same commit. If a receipt changed, run
   `python3 tools/render-status.py` and commit `docs/status.md`. `--check` must pass.

A gate that cannot find its evidence fails. A skip, a timeout or a missing result is not a pass.

## Size limits

Rust source under `crates/*/src/` and Python under `tools/` have limits of 1,500 lines per file
and 150 lines per function. Run `python3 tools/check-size.py`; `--report` lists the largest items.
`tools/size-baseline.json` records older exceptions. `--update` only lowers or removes them;
it never adds an exception or raises its size. Both the gate and CI run this check.

## Specifications

The documents in [`spec/`](spec/README.md) are pinned by SHA-256 in `spec/pins.json`. When you
change a document:

1. Edit the document and increase its revision line.
2. Run `python3 tools/check-spec-pins.py --update`.
3. Commit the document and `spec/pins.json` together.

`tools/gate-changed.sh` refuses a changed document with an old pin. When code and a specification
disagree, one of them has a defect. Fix the code, or change the document with a new revision.

## Lean proofs

- Do not commit `sorry`.
- Do not add an axiom. The theorems depend only on `propext`, `Classical.choice` and `Quot.sound`,
  and `#print axioms` checks in the Lean tests make `lake build` fail on a new one.
- Run `tools/gate-lean.sh` for a change in `lean/` or in a stage export. It rebuilds the proofs,
  checks the rule registry and the vectors, and audits every axiom. It also rewrites a tracked
  receipt, so commit that receipt with your change.

## Repository layout

| Path | Content |
|---|---|
| `crates/` | The compiler: 18 Rust crates, from the preprocessor to the AArch64 emitter |
| `lean/EvoIR/` | Lean 4 semantics, rule proofs, stage checkers and the pipeline theorem |
| `spec/`, `targets/aarch64/` | The four pinned specifications; the machine tables |
| `tools/bench.py`, `tools/ladder/`, `tools/k1/` | The evaluation ladder: program, game and Linux lanes |
| `tools/gate-changed.sh`, `tools/gate-lean.sh` | The Rust gate and the Lean gate |
| `tools/experiment/`, `tools/demo/`, `tools/proof-native/` | Agent experiments, demo scripts, native runs of checked output |
| `tests/contracts/`, `tests/fixtures/` | Receipts and pinned corpora; test inputs |
| `book/`, `docs/`, `examples/` | The book; the paper, evidence and design; the README examples |

## Project documents

- [Architecture](ARCHITECTURE.md): the pipeline, crate interfaces and trust boundaries.
- [The paper](docs/paper.md): the story of the project, the design and the evaluation.
- [Evidence](docs/evidence.md) and the [status dashboard](docs/status.md): measured results with
  dates.
- [TODO.md](TODO.md): the task tracker.
- [C checker workflow](docs/checker.md) and [verified optimization design](docs/verified-optimization-design.md).
- [`docs/archive/`](docs/archive/README.md): superseded plans and dated lab notes.

To update the README figures, run `python3 tools/render-readme.py` after a release build.

## Commit messages

- Write the subject in the imperative mood: "Reject a pointer argument in core-v1", not
  "Rejected ..." or "Rejects ...".
- Keep the subject short. Put the reason for the change in the body.
- Make one logical change per commit. The RED test is its own commit.
- Do not add attribution trailers or tool signatures.

## License

EvoCC is licensed under the [Apache License 2.0](LICENSE). Copyright 2026 Haoran Wang, NexRidge.
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be licensed under the Apache License 2.0,
without any additional terms or conditions.

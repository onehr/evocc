# Work with EvoCC

EvoCC is a C compiler for AArch64 macOS and Linux. EvoIR records execution
obligations. The proof-required path checks its supported typed Core subset
through final assembly text. This is not a proof of every accepted C program.

## Commands

Use `evo help` or `evo VERB --help`. Help exits 0. No arguments exits 64.
Use `--json` before `--` for one JSON object per line on standard output.

| Command | Exit status and result |
|---|---|
| `evo run FILE.c [-- ARGS]` | Main's low 8-bit return value; interpreter errors are nonzero |
| `evo check FILE.c` | 0 for a completed checked path, even if main returns nonzero; 1 for a finding or refusal |
| `evo explain CODE` | 0 for the diagnostic page; 1 for an unknown code |
| `evo prove FILE.c -o OUT.s [--entry NAME]` | 0 for a checked output; 1 for input/command errors; 3 rejected, 4 bad proof input, 5 checker failure, 6 outside the profile |
| `evo build DIR [-- MAKE_ARGS]` | The configure or make status; writes a receipt and a build log |
| `evo fix FILE.c --plan` | A repair plan; does not edit the source |
| `evo fix FILE.c --apply [--review]` | Applies selected repairs and checks the edited source; errors are nonzero |
| `evo version` | 0 and the compiler version |
| `evo mcp` | Newline JSON-RPC on stdio; closes on input EOF |
| `evo lsp` | Framed LSP on stdio; shutdown then exit returns 0; early exit returns 1 |

The command JSON fields are in `book/src/agents.md`. Diagnostic and repair
records are in `spec/diagnostics.md`. Read the exit code as well as the record.
An execution check covers the path that ran, not all possible inputs.
The interpreter does not support arbitrary external libc calls.

Run evo check before evocc -c. A refusal is not a proof that the source has UB.
A review repair needs a human. Do not pass `--review` without approval.
The MCP tool `fix_plan` does not apply edits. MCP has no build or fix-apply tool.

## Reproduce a defect

Keep the source bytes, compiler version, full command, target, sysroot and
input arguments. Add a small test in `tests/` before changing implementation.
Use the existing pinned source and target for an application replay. Do not
replace a failing original input with an easier program and call it a pass.
Do not change emitted assembly, IR or certificates to repair presentation.

## Repository checks

Use `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2` for Cargo commands.
Run Cargo commands serially and offline. Do not create another target tree.
Check free disk space first: do not start below 25 GiB; stop below 15 GiB.
Keep captures below 500 MiB. Do not commit raw captures or generated binaries.
Preserve `tests/contracts/`, sealed fixtures, required evidence and `.lake`.

Run `tools/gate-changed.sh --full-workspace --offline` for package acceptance.
Run `tools/gate-lean.sh` when the work requires the Lean gate.
Keep specification pins current. Update TODO.md and render-status output.
Keep code, comments and documentation in English. State the tested scope.
Commit only intended files. Do not push without the owner's instruction.

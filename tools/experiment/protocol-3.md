# Third repair experiment: method

Status: pilot. No registration tag yet. The full run waits for the owner's quota decision.

## Agent and isolation

A real coding agent repairs the program. The agent is the Kimi Code CLI
(`~/.kimi-code/bin/kimi`, version 2.0.2), model `kimi-code/k3`, thinking effort
`low` (`KIMI_MODEL_THINKING_EFFORT`). The CLI runs non-interactively:
`kimi -p <prompt> --output-format stream-json -m kimi-code/k3 --agent-file <agent> --skills-dir <empty>`.
The agent file removes the web tools (`WebSearch`, `FetchURL`) and sub-agents
(`Agent`, `AgentSwarm`). Built-in product skills are off.

Each session runs under a Seatbelt sandbox (`sandbox-exec`), chosen over the
Linux container of the owner's riscv-eval project because the host CLI is
already signed in, so one profile isolates the agent with the least code and no
change to that project. The sandbox:

1. allows the agent to execute only an allowlist: the shell, the basic
   utilities, `rg`, the `check` client and `kimi` itself. No compiler,
   interpreter or network tool can run, by name or by absolute path.
2. denies reading the repository, the real user home (except the Kimi files
   the CLI needs), `/etc`, and the run's own output tree; it confines writes to
   the workspace, the session temp directory and the Kimi home.
3. runs the agent shell with a clean `HOME` and a login-shell stand-in, so no
   user startup file loads. `KIMI_CODE_HOME` points at the real `~/.kimi-code`,
   read in place; no credential is copied.
4. gives each session its own `TMPDIR`.
5. leaves the Kimi API reachable; the web tools stay off through the agent file.

The workspace holds only `program.c` and `check`. The hidden test, the fixed
source, the fix spans and the harness live outside the sandbox.

## The check command

`check` is an opaque client. It holds no harness, repository or run-directory
path. It relays the named file to the harness through a `.check` directory in
the workspace; the harness runs the condition's tools outside the sandbox and
returns the text.

`check FILE.c` builds and runs any C file of the workspace in the sealed,
network-disabled container and prints the build output, the program stdout and
stderr, and the exit codes. The agent can therefore write and run its own test
drivers in every condition. The prompt is identical for every condition.

Only the diagnostic layer differs:

- **C0 plain:** sealed `gcc-14 -O0`. No warnings, no sanitizer.
- **C1 sanitizer:** sealed `gcc-14 -Wall -Wextra -fsanitize=undefined -O0`.
- **C3 EvoCC:** `evocc` builds the file for `aarch64-unknown-linux-gnu` with the
  checked undefined-behavior profile; the container assembles, links and runs
  it. The harness then adds `evo check --json`, `evo explain CODE --json` for
  each reported code, and `evo fix --plan --json`.

### C0 is a plain compiler, not a no-feedback control

In experiments 1 and 2, C0 returned "No feedback is available in this
environment." The second pilot showed why that control does not hold for a
coding agent: the agent has a shell. Its C0 sessions fetched the upstream
source and ran a host compiler, so C0 measured host-tool access, not the
absence of feedback. A compiler is the floor a coding agent always has.
Experiment 3 therefore makes C0 a plain compiler with no diagnostics, and the
sandbox removes the host tools and the network, so every condition offers the
same build-and-run capability and differs only in its diagnostics. This
experiment no longer has a no-feedback control; prediction 5 of experiment 2
does not apply.

## Scoring, limits and audit

The final `program.c` gets the unchanged hidden test of `repair.py` (sealed
`gcc-14 -O0`, `main` renamed to `program_main`, the hidden reference driver,
exact `expected.txt`). The diff size and the fix-span hits reuse the P18
functions. Each session has a 15-minute wall limit and a 100-step limit
(`KIMI_LOOP_MAX_STEPS_PER_TURN`). A rate-limit, quota or authentication refusal
stops the run at once; there is no retry.

The audit reads the session `wire.jsonl` and sorts every tool action into three
classes: scratch use of the workspace or session temp (not a deviation),
blocked attempts (the sandbox denied them), and successful escapes (a compiler,
interpreter, network tool or outside-the-workspace access that ran). A clean run
has zero escapes. A filesystem backstop lists any new file in the common escape
targets.

## Quota pacing

Before each session the harness reads the plan quota from the local Kimi server
(`GET /api/v1/oauth/usage`; the bearer token stays in memory). When the 5-hour
`usedRatio` is at or above 0.9, it waits until the window resets. When the
7-day `usedRatio` reaches `--max-week-ratio`, it stops and records the reason.

## Pilot

Items `ldm-gear-buckets`, `frame-block-walk` and `json-merge-patch` from
`tests/fixtures/agent-repair-2-spare/`, one per class, the hardest items.
Conditions C0, C1, C3, one repetition, nine sessions, sequential. The output
goes outside the repository; the receipt and the compressed `wire.jsonl`
archive are committed after a credential scan.

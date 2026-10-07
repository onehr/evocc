---
name: evocc
description: Check C programs, explain diagnostics, apply safe repairs, and obtain checked assembly with EvoCC when the task calls for its agent-facing commands or MCP tools.
---

# EvoCC

Use EvoCC for executed-path checks, diagnostic-guided repairs, and checked
compilation of its supported typed Core (`core-v1`) subset. Follow the repository's
[AGENTS.md](../../AGENTS.md) for targets, build discipline and evidence retention.
A completed execution check is not a proof of safety for every input.
A compilation proof checks source-to-assembly correspondence, not repair intent.

## Repair loop

1. Run `evo check FILE.c --json` before `evocc -c` and after source edits.
   Preserve the actual command, source bytes, arguments and exit status.
2. For an unfamiliar diagnostic code, run `evo explain CODE --json`.
   If the code is unknown, report that result; do not invent an explanation.
3. Obtain `evo fix FILE.c --plan --json`. The plan does not edit the source.
   Apply safe repairs with `evo fix FILE.c --apply --json` when edits are in scope.
   A review repair needs a human; never add `--review` without approval.
4. If undefined behavior remains without a safe repair, establish the intended
   behavior with the owner before changing it. Demo-specific saturation or
   return-zero guards are not general repair policies.
5. Re-check the edited source. Limit the loop to five check/repair rounds.
   Stop sooner for missing behavior policy, unsupported features or missing
   authority. Report unresolved findings instead of adding a planner or retries.
6. After the check completes, run `evo prove FILE.c -o OUT.s --json` for the
   admitted subset. Prove the same edited source that was checked; do not swap
   in an easier input. For a recorded demo, repair a temporary copy and preserve
   the original fixture.
7. Read the retained `record.json`, its source identity and stage results.
   Keep `NOT_CHECKED` rows visible. Success requires exit 0 and checked output;
   a record alone does not prove that the requested output was published.
   Match recorded source and output hashes to the files being handed off.
   Do not edit assembly, IR, exported nodes or certificates to bypass refusal.
8. Report the actual result, checked scope, record path and remaining findings.
   Preserve raw command output when recording a transcript.

## Exit statuses

Read the process status as well as the JSON record.

- `run`: main's low eight bits; interpreter errors are nonzero.
  A nonzero main result alone is not an interpreter failure.
- `check`: 0 for a completed checked path, even when main returns nonzero;
  1 for a finding or refusal. Refusal does not establish source UB.
- `explain`: 0 for a known code; 1 for an unknown code.
- `prove`: 0 checked output; 1 input/command error; 3 rejection;
  4 bad proof input; 5 checker failure; 6 outside the supported profile.
- `fix --plan`: read-only plan. `fix --apply`: applies selected repairs
  and checks the edited source; errors are nonzero.
- `build`: configure or make status, with a receipt and log.
  `help` and `version` exit 0; invoking `evo` without a command exits 64.

## JSON and MCP

Place `--json` before `--` for one command-record JSON object per line.
Read [book/src/agents.md](../../book/src/agents.md) for command schemas and
MCP arguments, and [spec/diagnostics.md](../../spec/diagnostics.md) for
diagnostic spans, repair records and safety classifications.
MCP `fix_plan` does not apply edits; MCP has no build or fix-apply tool.
Use the [recorded loop](../../book/src/agent-loop.md) as a workflow example,
not as authority to copy its behavior-changing repair into another program.

# N05: measurement automation

**Acceptance pending.** Plan frozen in `917f8955`:
[contract](../../tests/contracts/codegen-n05-20261002.json). Implementation
commit: the containing commit. [Compact receipt](../../tests/contracts/codegen-n05-20261002-receipt.json).

| Item | Result |
| --- | --- |
| Focused Python adapters | 127 PASS / 0 FAIL; mock benchmark output is not native evidence |
| Injected AArch64 object sizes | 8 → 24 → 24 bytes; each built twice, byte-identical |
| First bad Git commit | `ff5b17a0665d4013ff6e18e88947789a0b0d3372`; no receipt rebuild required |
| Six current-source TU profiles / ladder / steps | NOT RUN; [live plan](../../tests/contracts/codegen-n05-live-20261002.json) frozen in the containing commit |
| Full workspace | 377 suites; 2,915 PASS / 0 FAIL / 4 ignored; Clippy and formatting PASS |
| Live pre-push | NOT ACTIVATED |

The injection used real `commit-tree` objects and `/usr/bin/clang` as an
AArch64 assembler; it did not check out or alter the main index. Its unit is
`test:n05_regression_probe`, not a substituted S5 unit. GCC ratios, compile
times and interpreter/ladder coverage remain `NOT_RUN` in that capture.
Raw receipt: `.evidence/codegen-trend/injection-yu9yd2yl/receipt.json`.

The fresh collector reuses S1, `Capture`, `bench.lock` and S5 recipes. It
compiles the original C with the current release compiler, not old EvoIR;
three compile samples are mechanical host-cost trends, never native latency.
It checks twin object identity, replays certificates once, runs the bounded
mixed-object ladder and consumes N02 step reports. `integer-mix` source-bridge
`MISMATCH_NOT_VALIDATED` stays visible beside same-IR step observations, never
relabeled translation validation; its numeric diagnostic is retained. Proof failures reuse
the existing disposition buckets: completed rejection, bundle failure and
unknown are distinct; a producer recovery proof never admits its rollback.

`tools/hooks/pre-push` is an opt-in local hook, not active merely because it
exists. After the live plan and shared release binaries pass, configure
`evocc.codegenPlan` and install/compose the hook without replacing another
hook. It measures missing current `HEAD`, and requires retained observations
for pushed implementation commits and each tip since the frozen N05 plan.
Missing historical commits **block with `NOT_RUN` and their exact hashes**;
they need the same plan in an exclusively scheduled clean checkout. Neither
the hook nor receipt-first bisect creates a worktree or silently switches the
shared checkout. Installing a hook is not evidence it has run on a push.

```sh
python3 tools/render-status.py --codegen-trend .evidence/codegen-trend/history.jsonl
python3 tools/bisect.py --history .evidence/codegen-trend/injection-yu9yd2yl/trend.jsonl --good 34a9b56de201663e7caf529d2849bcd018235448 --bad 41509ab8e36feca096ac508c06cb64290e178879 --unit test:n05_regression_probe --variant injected-assembly-test
```

**Claim ceiling:** local detector/localizer and adapter tests only until fresh
six-TU, full-workspace and real hook gates complete; no latency, optimizer
quality or verified-size gain. **Disk:** injection retained 80 KiB allocated;
temporary index/objects removed. Observed host free-space delta was −18,911,232
bytes during concurrent work, not a slice attribution; final net delta awaits
shared build cleanup. No new target, worktree or VM disk was created.

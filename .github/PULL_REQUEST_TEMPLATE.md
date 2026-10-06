### What this changes

<!-- One or two sentences. Name the task: the TODO.md line, for example "K04 — ublk stress/shutdown stall". -->

### TODO line

- [ ] I updated the line of the task in `TODO.md`, and `python3 tools/render-status.py --todo` passes.

### RED test

<!-- The commit that adds the failing test, and how it failed before the change. -->

### Gate

<!-- The command and the last lines of its output, for example tools/gate-changed.sh <base-commit>. -->

```text

```

### Original program

<!-- The unchanged program that showed the problem, and its result after the change. Write "none" if there is none. -->

### Receipt

<!-- The receipt or evidence row that records the result. Write "none" if the change makes no claim. -->

### Checklist

- [ ] A changed `spec/` document has a new revision, and `spec/pins.json` is updated in the same commit.
- [ ] A change in `lean/` has no `sorry` and no new axiom, and `tools/gate-lean.sh` passes.
- [ ] The commit messages use the imperative mood and have no attribution trailers.

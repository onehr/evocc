---
title: Before the First Token
chapter: 1
day: Day 1
slug: before-the-first-token
description: Build the reproducible world, oracle hierarchy, and evidence chain that must exist before EvoCC reads its first source token.
---

# 1. Before the First Token

> A compiler that cannot identify its inputs cannot explain its output.

Most compiler tutorials begin with a character. Perhaps it is a parenthesis,
the first digit of a number, or the opening quote of a string. We are going to
begin earlier, before a source file reaches the lexer and before an abstract
syntax tree exists.

We begin with the machine on which the compiler is built, the machines on which
its output is tested, and the rules that decide whether an observation counts
as truth.

That sounds like infrastructure. For EvoCC, it is part of correctness.

This chapter is a guided reconstruction of Day 1. It describes the foundation
as it exists in repository commit `159f2e5`, including repairs discovered by
later audits. The goal is not to memorize hashes. It is to understand why a
proof-carrying compiler needs a reproducible world before it needs a parser.

## 1.1 The compiler we are building

EvoCC is a GNU C11 compiler whose optimizer uses **equality saturation**. Its
primary 30-day launch target is an ARM64 Linux kernel built with proof-producing
EqSat enabled.

That sentence combines three difficult jobs:

- understand the source semantics of real GNU C;
- explore many equivalent implementations without silently changing behavior;
- produce native code that survives an operating-system boot.

A conventional optimizer repeatedly replaces one program with another:

```text
program₀  --rewrite-->  program₁  --rewrite-->  program₂
```

Each arrow discards the previous representation. A poor local choice can hide
an alternative that would have become better after later transformations.

An e-graph keeps equivalent forms together instead:

```evocc-sketch
egraph-equivalence
```

The optimizer first accumulates representations that are known to be equal,
then extracts one according to a target cost model. The word *known* carries
most of the weight. In C, overflow, pointer provenance, volatile access,
evaluation order, and undefined behavior can turn an attractive algebraic
identity into a miscompilation.

EvoCC therefore couples the equality database to reasons. A rewrite is not
accepted merely because two expressions looked similar in a test. It must
eventually be replayable against the rule, substitution, premises, types, and
definedness facts that justified it.

### Design note: optimization is not the specification

Equality saturation changes how alternatives are represented; it does not
change what C means. ISO C, the selected GNU dialect, and the target ABI remain
the semantic authorities. An e-graph may contain only equal alternatives under
those contracts.

This distinction will recur throughout the notebook: a clever representation
can make a correct proof easier, but it cannot make an invalid transformation
correct.

## 1.2 A map of EvoCC

Before following Day 1, we need a rough map of the whole system. The compiler
has two high-level representations because the source language and optimizer
need different facts.

**C-HIR** is language-facing. It preserves C types, qualifiers, lvalues,
storage, source spans, initializers, attributes, and other facts needed to say
what the source program means.

**EvoIR** is optimization-facing. It uses block-argument SSA, explicit memory
and world tokens, typed bit-vector operations, and explicit definedness
obligations. The authoritative starting contract lives in
[`docs/evoir-spec.md`](../evoir-spec.md).

```evocc-sketch
compiler-territory
```

The diagram also marks an important boundary. EvoCC may use an external
assembler, linker, archiver, and object inspectors. Those tools package the
machine code selected by EvoCC. They are not allowed to parse C, optimize it,
or secretly replace the compiler under test.

## 1.3 There are really two systems

The source-to-kernel path is only half of the project. Beside it is an evidence
path that answers a different question: *why should we believe this run?*

```evocc-sketch
semantic-and-evidence
```

These systems must meet, but they must not be confused.

- A proof checker can establish that a recorded rewrite follows from its
  semantic premises. It cannot establish which compiler binary ran.
- A binary hash can identify the compiler that ran. It cannot establish that
  the compiler implemented C correctly.
- A kernel boot can show that a large artifact remained live through loading,
  initialization, and shutdown. It cannot resolve a disagreement between the
  C specification and an optimizer.

Day 1 is primarily about constructing the second system strongly enough that
later semantic claims have somewhere trustworthy to land.

## 1.4 The empty-workspace problem

At the beginning there was a roadmap, but no Rust workspace, parser, kernel
tree, guest image, or reference evidence. That is not unusual for an ambitious
systems project. It is also a dangerous moment: almost every choice is still
implicit.

Suppose we simply installed a compiler and ran a kernel build. Several hidden
variables would immediately enter the result:

- the floating kernel release and source revision;
- the generated kernel configuration;
- the real compiler behind a command named `gcc`;
- the container image behind a mutable tag;
- package repository contents at installation time;
- timestamps, build paths, locale, timezone, and file modes;
- the QEMU machine alias and CPU model;
- the predicate used to call a serial log a successful boot.

If one of those changes, a later failure may be a compiler regression—or merely
environment drift. Worse, a later success may have been produced by a different
system from the one we intended to test.

So the first working artifact was not a lexer. It was a set of **locked
contracts**.

## 1.5 Three ARM64 worlds on one Mac

The development host is Apple silicon, but the launch target is ARM64 Linux.
Day 1 separated building from booting while keeping both observable from the
host.

```evocc-sketch
three-arm64-worlds
```

The Apple Container runtime is itself pinned by
[`contracts/apple-container-bootstrap.json`](../../contracts/apple-container-bootstrap.json):
release, commit, installer digest and signature identity, CLI digest, default
kernel, runtime roots, probe image, and cleanup rules.

The build substrate is pinned by
[`contracts/build-environment.json`](../../contracts/build-environment.json):
the Linux ARM64 image, Debian snapshot, package manifest, compiler binaries,
Rust toolchain, and reproducibility environment.

QEMU runs on the host, not inside the build guest. Each produced kernel
`Image` is booted through two deliberately different execution lanes:

- **HVF + host CPU** gives a fast local flight on Apple silicon.
- **TCG + cortex-a72** gives a portable software-emulated witness with a fixed
  CPU model.

Agreement between those lanes is useful because their virtualization paths are
different. We do not claim their runtime state or timing is byte-identical.

## 1.6 What counts as golden truth?

Calling GCC a “golden compiler” is convenient and wrong. Compilers have bugs,
extensions, version-dependent behavior, and shared implementation lineage.

EvoCC uses a hierarchy recorded in
[`contracts/oracle-policy.json`](../../contracts/oracle-policy.json):

| Rank | Authority | What it can establish |
|---:|---|---|
| 1 | Frozen language and ABI contracts | Normative meaning of defined programs |
| 2 | Hand-audited fixtures | Expected behavior for a reviewed concrete case |
| 3 | Independent GCC and Clang runs | Strong differential evidence, not proof |
| 4 | ABI observers and metamorphic relations | Properties derived from the target contract |
| 5 | Kernel boot | End-to-end integration and liveness only |

The hierarchy is fail-closed. If GCC and Clang disagree on a defined input, the
result is `ORACLE_CONFLICT`. EvoCC does not win a two-to-one vote by joining one
side, and a successful kernel boot cannot overrule the language contract.

### The misleading `gcc` on macOS

On this host, `/usr/bin/gcc` is an Apple Clang compatibility command. Treating
that command and `clang` as two independent oracles would give one
implementation two votes.

The reference compilers therefore live in the sealed Linux build image:

```text
GCC   /usr/bin/gcc-14    Debian 14.2.0-19
Clang /usr/bin/clang-19  Debian 19.1.7
```

Their real paths, vendors, versions, and binary digests must differ and must
match the locked contract at execution time.

## 1.7 Pinning a kernel means more than naming a version

The reference source is Linux `6.18.44`, resolved to commit
`1efe5d048a391de3ead2804b2e7f86376c356cc5`. Its tarball digest and kernel.org
signature evidence are recorded in
[`contracts/kernel-target.json`](../../contracts/kernel-target.json).

That is only the source identity. A Linux configuration depends partly on
compiler capability probes, so GCC and Clang do not necessarily produce the
same canonical `.config`. Day 1 freezes a separate configuration for each
compiler family and forbids shrinking either one merely to rescue a failing
build.

The QEMU side is pinned too:

```text
binary:   qemu-system-aarch64 11.0.3
machine:  virt-11.0              # never floating "virt"
fast CPU: host under HVF
fixed CPU: cortex-a72 under TCG
```

The general rule is simple:

> If changing a value could change the artifact or the verdict, that value is
> either locked or explicitly classified as observation-only.

## 1.8 Reproducibility is an experiment, not a flag

Day 1 builds each compiler lane twice from a clean state:

```text
                 clean build 1        clean build 2
GCC family       gcc-lane-1           gcc-lane-2
Clang family     clang-lane-1         clang-lane-2
```

Every declared artifact in a same-family pair must be byte-identical. GCC and
Clang outputs are not required to equal each other; they are separate reference
families.

This experiment exposed a subtle timestamp defect. The kernel build expects
`KBUILD_BUILD_TIMESTAMP` in a form understood by GNU `date`. Passing a bare
epoch caused an initramfs helper to drop its timestamp option, which embedded
wall-clock mtimes and produced a small but real `vmlinux` divergence.

The repaired contract fixes both the human-readable build timestamp and
`SOURCE_DATE_EPOCH` to the kernel release timestamp. It also freezes locale,
timezone, build user, build host, and umask. The lesson is broader than this
one bug: setting something called “reproducible” is not evidence of
reproducibility. Two clean builds are.

## 1.9 A boot predicate that rejects yesterday's success

A naive boot test searches a serial log for a familiar line. Such a test can
accept a stale log copied from an earlier run.

The Day 1 predicate gives every flight a fresh nonce. The nonce is injected on
the kernel command line and echoed by the init process. The acceptance harness
then requires:

1. the expected kernel `Image` digest;
2. the fresh nonce exactly once;
3. ordered `ALIVE`, nonce, and `DONE` markers;
4. controlled poweroff;
5. no panic and no timeout.

```evocc-sketch
nonce-bound-boot
```

A log with the wrong nonce, a duplicate nonce, reordered markers, a different
image, a panic, or a timeout is a failure. Boot remains a liveness witness, but
at least it is a witness from *this* run.

## 1.10 Red can be the correct color

On Day 1, EvoCC did not yet compile C. A dishonest skeleton could accept a
compiler-shaped command, return zero, and produce nothing useful. That would
make downstream automation look green while no capability existed.

Instead, unimplemented compiler probes had to fail with an explicit
`NOT_IMPLEMENTED` classification and a defined nonzero exit code. This makes
the absence of a capability observable and prevents the scaffold from being
mistaken for the compiler.

This is the first example of a pattern used throughout the project:

```text
known supported behavior       -> PASS or a precise semantic diagnostic
known unsupported capability   -> controlled, named RED
missing evidence               -> FAIL
unknown crash or timeout       -> FAIL
silent success                 -> FAIL
```

RED is useful only when the reason is the intended missing behavior. A compile
error in the test harness, a missing command, and an actual semantic rejection
are different observations and must never be collapsed into the same result.

## 1.11 Design note: green can lie

The first green-looking system was not the last word. Later review found three
classes of false confidence:

- a differential corpus became smaller when a difficult include case was
  removed;
- selected Miri tests missed an alignment defect in the generic Arena API;
- an artifact check validated a synthetic mutation exercise without first
  validating the live manifest it claimed to protect.

The repair was not “more green.” It was stronger causality:

- restore the exact corpus denominator and forbid non-PASS outcomes;
- exercise the public safe API with adversarial alignment, Drop, and panic
  behavior under Miri;
- join the acceptance result to the live artifact set;
- seal the final evidence namespace and require an independent verifier to
  reject modified, missing, and additional files.

This history matters. An acceptance harness is executable code and can contain
bugs just like a compiler. Evidence integrity needs negative tests of its own.

## 1.12 The artifacts left by Day 1

The foundation is small enough to survey:

```text
contracts/
  apple-container-bootstrap.json   runtime and micro-VM identity
  build-environment.json           image, packages, tools, reproducibility
  kernel-target.json               source, configs, QEMU, boot predicate
  oracle-policy.json               hierarchy and conflict rules

.toolchain/
  bootstrap-workspace.sh           reconstruct the sealed build substrate
  build-reference-kernels.sh       four clean reference builds
  build-flight-initramfs.sh        nonce-reporting initramfs
  boot-reference-kernel.sh         one bounded QEMU flight
  day1-flight-gate.sh              end-to-end reconstruction and acceptance
  records/                         frozen configs and package/tool records

tests/acceptance/flight-lab/
  run.sh                            fail-closed Day 1 flight acceptance
```

The Rust workspace is also part of the skeleton. It names the narrow-waist
crates—common infrastructure, types, preprocessing, both IRs, e-graph, proof,
parser, lowering, and driver—even though Day 1 initially required the compiler
capabilities themselves to remain explicitly RED.

## 1.13 Running the checkpoint

There are two useful entry points.

The expensive Day 1 reconstruction rebuilds the environment, builds two clean
kernel copies per compiler family, boots four flights, writes a manifest, and
runs the flight acceptance harness:

```sh
bash .toolchain/day1-flight-gate.sh
```

The current consolidated Day 1–3 checkpoint also runs workspace tests, Clippy,
formatting, Miri, flight acceptance, isolated preprocessing differential tests,
residue checks, and evidence mutation tests:

```sh
bash tests/acceptance/foundation-conformance.sh
```

On the repository snapshot used to write this chapter, the latter produced an
exact `11/11 PASS` bundle at:

```text
.evidence/foundation-conformance/20260812T042312Z-96784
```

The bundle can be checked independently:

```sh
bash tests/acceptance/verify-foundation-evidence.sh \
  .evidence/foundation-conformance/20260812T042312Z-96784
```

Evidence directories are observations, not source inputs. A fresh run should
create a new run directory instead of rewriting an old one.

## 1.14 What we now know

We still have not tokenized a single line in this chapter, but we have learned
the vocabulary needed to understand the project:

- **semantic contract** — what a supported program means;
- **implementation** — the compiler code attempting to realize that contract;
- **oracle** — an independent source of comparative evidence;
- **execution substrate** — the exact tools and machines that produce an
  observation;
- **acceptance predicate** — the rule converting raw observations into a
  verdict;
- **provenance** — the causal chain joining inputs, execution, artifacts, and
  evidence;
- **proof** — a semantic witness for an equality, not a synonym for a green
  process exit;
- **flight** — a bounded end-to-end liveness experiment.

This is the ground under the rest of the compiler. In the next chapter we can
finally approach the source text: characters become preprocessing tokens, and
every token must retain enough identity and location information to survive
macro expansion, included files, diagnostics, and later proof generation.

## Challenges

1. Copy `contracts/kernel-target.json` to a temporary directory and change one
   hex digit in a digest. Which acceptance rule should detect it, and why would
   merely checking that the field is 64 hex characters be insufficient?
2. Construct a defined C expression on which two real compilers could disagree
   because one contains a bug. Explain why majority voting cannot tell you
   which compiler is correct.
3. The HVF and TCG flights boot the same kernel bytes through different CPU
   execution paths. List one class of defect this diversity may expose and one
   semantic claim it cannot establish.
4. Design a stale-log attack against a boot check that searches only for
   `EVO_INIT_DONE`. Then explain how the nonce-bound ordered predicate rejects
   it.
5. A test suite reports `999/999 PASS` after one difficult fixture is deleted.
   What additional invariant is needed before that number means anything?

### Further reading

The chapter structure—territory map, incremental construction, design notes,
and challenges—is inspired by the approach used in
[*Crafting Interpreters*](https://craftinginterpreters.com/contents.html), in
particular its [map of a language implementation](https://craftinginterpreters.com/a-map-of-the-territory.html)
and its discussion of [testing a language](https://craftinginterpreters.com/chunks-of-bytecode.html#design-note-test-your-language).

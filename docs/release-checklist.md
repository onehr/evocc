# Release checklist

This page audits the first release, EvoCC 0.1.0, against the repository. It covers the two
archives, checksums, notices, install and build instructions, reproducibility, version strings,
the changelog and the proof-claim boundary. CI is out of scope. The audit ran on 2026-10-13 on
macOS (AArch64), at commit `366fd796` with the release-hygiene changes of the same day.

**Done** means that this audit checked it. **Partial** means that a part works, and the row names
the rest. **Open** means that it is missing. **Not verified** means that the check needs a host or
an input that this audit did not have.

## Archives and checksums

| Deliverable | State | Evidence |
|---|---|---|
| macOS archive `evocc-0.1.0-aarch64-apple-darwin.tar.gz` | Done as a packaging check. `tools/release.sh --skip-build` packaged binaries from a shared working tree, not from a clean tag. | [`tools/release.sh`](../tools/release.sh) |
| Smoke test of the macOS archive | Done. All 4,000 entries of the inner `SHA256SUMS` match. `evocc --version` prints `evocc 0.1.0 (366fd796613e 2026-10-13)`. The interpreter hello prints 12. `evoir-sem --selftest` exits 0. | The smoke commands, run on the extracted archive |
| Linux object from the macOS archive | Done. The bundled sysroot gives an ELF object. The object links and prints `hello from EvoCC on Linux` in the sealed Linux image. | `evocc -v --target=aarch64-unknown-linux-gnu` prints the bundled sysroot |
| Linux archive `evocc-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | Not verified. The build needs an AArch64 Linux host, the Rust target `aarch64-unknown-linux-musl` and Lake. | [`tools/release.sh`](../tools/release.sh) |
| Linux archive on a host with glibc older than 2.38 | Open risk, not tested. On Linux the bundled glibc 2.41 headers come before the host headers. With `_GNU_SOURCE` they redirect `strtol` to `__isoc23_strtol`, which older glibc does not have. | [`crates/evocc/src/toolchain.rs`](../crates/evocc/src/toolchain.rs) (`linux_sysroot`), `usr/include/features.h` of the sysroot |
| Deterministic archive | Done. Two packaging runs with the same inputs gave the same SHA-256. | [`tools/release.sh`](../tools/release.sh) |
| Path-independent binaries | Open. `evo`, `evocc` and `evocc-checker` hold 23, 75 and 11 strings with the home directory of the build machine. The build does not remap source paths. | `strings -a bin/evocc` |
| Inner `SHA256SUMS` | Done. It lists every file of the archive, the license texts too. The archive check of the sysroot test passes. | [`tools/tests/test_sysroot.py`](../tools/tests/test_sysroot.py) with `EVOCC_RELEASE_ARCHIVE` |
| One `SHA256SUMS` for both archives | Open for a release without CI. `tools/release.sh` merges digests only in its own output directory, and the two archives build on two hosts. | [`tools/release.sh`](../tools/release.sh) |

## Notices

| Deliverable | State | Evidence |
|---|---|---|
| `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md` in the archive | Done. | [`tools/release.sh`](../tools/release.sh) |
| Notices for the four binaries | Done. The notices name `evo`, `evocc`, `evocc-checker` and `evoir-sem` and the third-party code in each. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1 |
| License texts of the Linux headers | Done. The archive holds the Debian copyright files of the 13 packages and the 12 license texts that they cite. The contract pins all 25 files, and `tools/release.sh` refuses a mismatch. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`contracts/sysroot-aarch64-unknown-linux-gnu.json`](../contracts/sysroot-aarch64-unknown-linux-gnu.json) |
| License texts of the code inside the binaries | Open. `evoir-sem` contains GMP (LGPL-3.0) and libuv (MIT), and the Linux binaries contain musl (MIT). The archive has none of these texts. The notices link to repository files that the archive does not hold. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1 |

## Install, build and reproducibility

| Deliverable | State | Evidence |
|---|---|---|
| Install in the README | Partial. The commands download the v0.1.0 assets, which exist only after the release. The README has no checksum step; the book has one. | [`README.md`](../README.md#install) |
| Install in the book | Partial. The download, check and unpack steps are complete, and the archive layout names `sysroot/`. The source build text names three programs, but Cargo also writes `evo`. It omits `--locked`, which `tools/release.sh` uses. It does not say how a source build gets the Linux sysroot. | [`book/src/install.md`](../book/src/install.md) |
| macOS quarantine | Open. The binaries are not notarized. A browser download gets the quarantine attribute; `curl` does not. The book does not say this. | [`book/src/install.md`](../book/src/install.md) |
| Build from source | Done. Rust 1.97.1, Lean v4.32.2 and the vendored crates give an offline build. | [`rust-toolchain.toml`](../rust-toolchain.toml), [`lean/lean-toolchain`](../lean/lean-toolchain), [`vendor/`](../vendor/) |
| Reproduce a release | Open. No page gives the commands to rebuild an archive from a tag and compare its SHA-256. | None |
| Sysroot provenance | Partial. The contract pins 3,966 headers from `evo-nexlab:sealed-20260810` (`sha256:2c23684c`). The default image of the exporter, `evo-nexlab:sealed` (`sha256:e8d52bdc`), adds 34 headers of `libclang-rt-19-dev`. An export without `EVOCC_SYSROOT_IMAGE=evo-nexlab:sealed-20260810` changes the pins. | [`tools/export-sysroot.sh`](../tools/export-sysroot.sh), [`contracts/build-environment.json`](../contracts/build-environment.json) |
| Sysroot size | Open decision. 2,319 of the 3,966 headers (40.8 of 52.7 MiB) are outside the C search path: the `asm/` trees of 21 other triples (2.3 MiB), the C++ headers (12.5 MiB), two copies of the Clang 19 resource headers (25.9 MiB) and the Objective-C headers. | [`crates/evocc/src/toolchain.rs`](../crates/evocc/src/toolchain.rs) (`linux_directories`) |
| No self-hosting claim | Done. The README, the book and the paper do not claim self-hosting. A Rust compiler builds EvoCC. | [`README.md`](../README.md) |

## Versions and changelog

| Deliverable | State | Evidence |
|---|---|---|
| Version of `evocc` and `evo` | Done. Both print `evocc 0.1.0 (<commit> <date>)`. | `evocc --version`, `evo version` |
| Version of `evocc-checker` and `evoir-sem` | Open. `evocc-checker --version` exits 2 with no output. `evoir-sem --version` prints the usage text. | The two commands |
| Version policy | Done. `tools/release.sh` accepts only a `0.y.z` version with a matching heading in `CHANGELOG.md`. | [`tools/release.sh`](../tools/release.sh) |
| Package description | Done. It describes the compiler without internal lab names. | [`crates/evocc/Cargo.toml`](../crates/evocc/Cargo.toml) |
| Changelog and tag | Partial. The heading `## [0.1.0] - Unreleased` has no date. No `v0.1.0` tag exists. | [`CHANGELOG.md`](../CHANGELOG.md) |

## Proof-claim boundary

| Deliverable | State | Evidence |
|---|---|---|
| The claim | Done. Section 1 of the verification contract states the proved path: the subset `core-v1`, the target `aarch64-unknown-linux-gnu`, the trusted base and what is not claimed. | [`spec/verification.md`](../spec/verification.md) |
| Release-blocking proof tasks | Done. V00 to V06 are complete in their frozen scope. | [`TODO.md`](../TODO.md) |
| README wording | Partial. The Status section limits proofs to integer functions without pointers, arrays or recursion. The opening paragraph and the Highlights state proofs without that limit. No README line names the Linux target of the proved path. | [`README.md`](../README.md#status) |

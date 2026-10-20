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
| Smoke test of the macOS archive | Done. On 2026-10-14, after the header trim and with the runtime license texts, all 1,696 entries of the inner `SHA256SUMS` match. `evocc --version` prints `evocc 0.1.0 (366fd796613e 2026-10-13)`. The interpreter hello prints 12. `evoir-sem --selftest` exits 0. | The smoke commands, run on the extracted archive |
| Linux object from the macOS archive | Done. The bundled sysroot gives an ELF object. The object links and prints `hello from EvoCC on Linux` in the sealed Linux image. On 2026-10-14 this passed again with the trimmed sysroot. | `evocc -v --target=aarch64-unknown-linux-gnu` prints the bundled sysroot |
| Linux archive `evocc-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | Not verified. The build needs an AArch64 Linux host, the Rust target `aarch64-unknown-linux-musl` and Lake. | [`tools/release.sh`](../tools/release.sh) |
| Linux archive on a host with glibc older than 2.38 | Done: the book states the floor. On Linux the bundled glibc 2.41 headers come before the host headers. With `_GNU_SOURCE` they redirect `strtol`, `scanf` and related functions to `__isoc23_*` names of glibc 2.38. In `debian:bookworm-slim` (glibc 2.36-9+deb12u14, GCC 12.2.0), a `_GNU_SOURCE` `strtol` object of the frozen compiler does not link: `undefined reference to '__isoc23_strtol'`. The same program, linked against glibc 2.41, stops with ``version `GLIBC_2.38' not found``. Without `_GNU_SOURCE`, and with the Debian 12 headers, it links and runs. | [`book/src/install.md`](../book/src/install.md#glibc-on-linux), `usr/include/features.h` of the sysroot |
| Deterministic archive | Done. On 2026-10-14, three worktrees of commit `34d81cbf` at three paths, each with its own header export, packaged the same programs into archives with the same SHA-256. | [`tools/release.sh`](../tools/release.sh), [`book/src/release.md`](../book/src/release.md) |
| Path-independent binaries | Open. `evo`, `evocc` and `evocc-checker` hold 23, 75 and 11 strings with the home directory of the build machine. The build does not remap source paths. | `strings -a bin/evocc` |
| Inner `SHA256SUMS` | Done. It lists every file of the archive, the license texts too. The archive check of the sysroot test passes. | [`tools/tests/test_sysroot.py`](../tools/tests/test_sysroot.py) with `EVOCC_RELEASE_ARCHIVE` |
| One `SHA256SUMS` for both archives | Open for a release without CI. `tools/release.sh` merges digests only in its own output directory, and the two archives build on two hosts. | [`tools/release.sh`](../tools/release.sh) |

## Notices

| Deliverable | State | Evidence |
|---|---|---|
| `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md` in the archive | Done. | [`tools/release.sh`](../tools/release.sh) |
| Notices for the four binaries | Done. The notices name `evo`, `evocc`, `evocc-checker` and `evoir-sem` and the third-party code in each. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1 |
| License texts of the Linux headers | Done. The archive holds the Debian copyright files of the 11 packages and the 12 license texts that they cite. The contract pins all 23 files, and `tools/release.sh` refuses a mismatch. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`contracts/sysroot-aarch64-unknown-linux-gnu.json`](../contracts/sysroot-aarch64-unknown-linux-gnu.json) |
| License texts of the code inside the binaries | Done. `licenses/runtime/` of each archive holds the COPYRIGHT file of musl 1.2.5, the `LICENSE` and `LICENSE-extra` files of libuv 1.52.1 with the notices of its `tree.h` and `inet.c`, and the GNU LGPL 3 and GPL 3 texts for GMP 6.3.0. `tools/release.sh` pins their SHA-256. A changed text or a missing directory stops it with exit status 1 and no archive. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1; [`licenses/runtime/`](../licenses/runtime/) |

## Install, build and reproducibility

| Deliverable | State | Evidence |
|---|---|---|
| Install in the README | Done. The commands download, check and unpack the v0.1.0 assets, which exist only after the release. The text names the four programs, the source build with `--locked` and the header export. | [`README.md`](../README.md#install) |
| Install in the book | Done. The page names the four programs and `licenses/runtime/`. The source build uses `--locked`, as `tools/release.sh` does. It says how a source build gets the Linux headers: `tools/export-sysroot.sh`, or the `sysroot/` of a release archive. | [`book/src/install.md`](../book/src/install.md) |
| macOS quarantine | Done as documentation. The binaries are not notarized: `codesign -dv` shows an ad-hoc signature and no team identifier. The book documents the quarantine attribute of a browser download and the `xattr` commands that clear it. A synthetic quarantined archive showed that `tar` copies the attribute to each file and that both commands clear it. | [`book/src/install.md`](../book/src/install.md#a-browser-download-on-macos) |
| Build from source | Done. Rust 1.97.1, Lean v4.32.2 and the vendored crates give an offline build. | [`rust-toolchain.toml`](../rust-toolchain.toml), [`lean/lean-toolchain`](../lean/lean-toolchain), [`vendor/`](../vendor/) |
| Reproduce a release | Done for the packaging step. The book gives the commands that package the programs of a release again from a clean checkout and compare the SHA-256. They gave the same archive from three checkouts. A new build of the programs does not reproduce the digest until R05e is done. | [`book/src/release.md`](../book/src/release.md) |
| Sysroot provenance | Done. The default image of the exporter is the image of the contract, `evo-nexlab:sealed-20260810`. Apple `container` cannot run a local image by digest, so the exporter stops when the image has another digest than `sha256:2c23684c`. On 2026-10-14 two exports without `EVOCC_SYSROOT_IMAGE`, at two paths, wrote the contract byte for byte. An export of `evo-nexlab:sealed` (`sha256:e8d52bdc`) now gives the same headers, license texts and packages; only the image name and digest differ. | [`tools/export-sysroot.sh`](../tools/export-sysroot.sh), [`contracts/build-environment.json`](../contracts/build-environment.json) |
| Sysroot size | Done. The exporter leaves out 24 directories: the `asm/` trees of 21 other triples (1,002 files), the C++ headers (825) and two copies of the Clang 19 resource headers (482), 40.7 MiB in all. 1,657 headers (12.1 MiB) stay. The 10 Objective-C headers stay, because GCC's header directory is on the search path. With the trimmed headers, zlib 1.3.2's `example` and the cross hello link and pass in the sealed image. | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`crates/evocc/src/toolchain.rs`](../crates/evocc/src/toolchain.rs) (`linux_directories`) |
| No self-hosting claim | Done. The README, the book and the paper do not claim self-hosting. A Rust compiler builds EvoCC. | [`README.md`](../README.md) |

## Versions and changelog

| Deliverable | State | Evidence |
|---|---|---|
| Version of `evocc` and `evo` | Done. Both print `evocc 0.1.0 (<commit> <date>)`. | `evocc --version`, `evo version` |
| Version of `evocc-checker` and `evoir-sem` | Done on 2026-10-19 in the working tree. Both print their binary name and `0.1.0`, exit zero and leave stderr empty. Tests run both from an empty directory and compare with Cargo's version. This does not certify a release archive. | [`crates/evocc/tests/evo_cli.rs`](../crates/evocc/tests/evo_cli.rs); full workspace: 3,475 passed, zero failed, eight ignored; all six Lean steps passed |
| Version policy | Done. `tools/release.sh` accepts only a `0.y.z` version with a matching heading in `CHANGELOG.md`. | [`tools/release.sh`](../tools/release.sh) |
| Package description | Done. It describes the compiler without internal lab names. | [`crates/evocc/Cargo.toml`](../crates/evocc/Cargo.toml) |
| Changelog and tag | Partial. The heading `## [0.1.0] - Unreleased` has no date. No `v0.1.0` tag exists. | [`CHANGELOG.md`](../CHANGELOG.md) |

## Proof-claim boundary

| Deliverable | State | Evidence |
|---|---|---|
| The claim | Done. Section 1 of the verification contract states the proved path: the subset `core-v1`, the target `aarch64-unknown-linux-gnu`, the trusted base and what is not claimed. | [`spec/verification.md`](../spec/verification.md) |
| Release-blocking proof tasks | Done. V00 to V06 are complete in their frozen scope. | [`TODO.md`](../TODO.md) |
| README wording | Partial. The Status section limits proofs to integer functions without pointers, arrays or recursion. The opening paragraph and the Highlights state proofs without that limit. No README line names the Linux target of the proved path. | [`README.md`](../README.md#status) |

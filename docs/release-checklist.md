# Release checklist

This page audits the first release, EvoCC 0.1.0, against the repository. It covers the two
archives, checksums, notices, install and build instructions, reproducibility, version strings,
the changelog, the hosted workflows and the proof-claim boundary. The audit ran on 2026-10-13 on
macOS (AArch64), at commit `366fd796` with the release-hygiene changes of the same day. It was
updated on 2026-10-20 at commit `537e745a`, and on 2026-10-27 at commit `08bf09bd`, whose compiler
the release acceptance uses. Each row names the date of its last check. Hosted CI is paused by the
owner since 2026-10-20 (TODO R05m). The Linux archive and the merged checksum file come from a
local release build (R05a) or from the release workflow.

**Done** means that this audit checked it. **Partial** means that a part works, and the row names
the rest. **Open** means that it is missing. **Not verified** means that the check needs a host or
an input that this audit did not have.

The column **Waits for** names the next check of a row: the release-archive build (R05a), the
final gate (T05), or the frozen acceptance (the kernel replay and the application campaign B10).
**Owner** marks a step that only the owner can do: the date and the tag, the publication (R05k)
and the hosted runs (R05m). **Nothing** means that no step of this release checks the row again.

## Archives and checksums

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| macOS archive `evocc-0.1.0-aarch64-apple-darwin.tar.gz` | Done as a packaging check on 2026-10-14: `tools/release.sh --skip-build` packaged binaries from a shared working tree, not from a clean commit. The release archive is not built yet. It comes from one clean commit, with the Linux archive. | Release-archive build (R05a) | [`tools/release.sh`](../tools/release.sh) |
| Smoke test of the macOS archive | Done on 2026-10-14 for that packaging check. After the header trim and with the runtime license texts, all 1,696 entries of the inner `SHA256SUMS` match. `evocc --version` prints `evocc 0.1.0 (366fd796613e 2026-10-13)`. The interpreter hello prints 12. `evoir-sem --selftest` exits 0. The smoke steps run again on the release archive. | Release-archive build (R05a) | The smoke commands, run on the extracted archive |
| Linux object from the macOS archive | Done on 2026-10-14. The bundled sysroot gives an ELF object. The object links and prints `hello from EvoCC on Linux` in the sealed Linux image, also with the trimmed sysroot. | Release-archive build (R05a) | `evocc -v --target=aarch64-unknown-linux-gnu` prints the bundled sysroot |
| Linux archive `evocc-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | Open on 2026-10-27: no Linux archive has been built. The build needs an AArch64 Linux host, the Rust target `aarch64-unknown-linux-musl` and Lake. The release-archive build makes it in an AArch64 Linux container. The release workflow builds it on a hosted runner. | Release-archive build (R05a); hosted build: Owner (R05m) | [`tools/release.sh`](../tools/release.sh) |
| Linux archive on a host with glibc older than 2.38 | Done on 2026-10-14 (R05h): the book states the floor. On Linux the bundled glibc 2.41 headers come before the host headers. With `_GNU_SOURCE` they redirect `strtol`, `scanf` and related functions to `__isoc23_*` names of glibc 2.38. In `debian:bookworm-slim` (glibc 2.36-9+deb12u14, GCC 12.2.0), a `_GNU_SOURCE` `strtol` object of the compiler of that day does not link: `undefined reference to '__isoc23_strtol'`. The same program, linked against glibc 2.41, stops with ``version `GLIBC_2.38' not found``. Without `_GNU_SOURCE`, and with the Debian 12 headers, it links and runs. | Nothing | [`book/src/install.md`](../book/src/install.md#glibc-on-linux), `usr/include/features.h` of the sysroot |
| Deterministic archive | Done for macOS. On 2026-10-14, three worktrees of commit `34d81cbf` at three paths, each with its own header export, packaged the same programs into archives with the same SHA-256. On 2026-10-23 (`b396893c`) two checkouts at two paths gave byte-identical programs and the same archive SHA-256. The release-archive build repeats this at another path. | Release-archive build (R05a) | [`tools/release.sh`](../tools/release.sh), [`book/src/release.md`](../book/src/release.md) |
| Path-independent binaries | Done on 2026-10-23 at `b396893c` (R05e). `tools/release.sh` builds `evo`, `evocc` and `evocc-checker` with a `dist` profile without a debug map, and remaps the checkout, the Rust sysroot and the target directory. A byte search finds no path of the build machine in the three programs; before, `evocc` held 434. The script refuses a program that holds the checkout or target path, and a Cargo program that holds `$HOME`. `evoir-sem` keeps five paths of the Lean toolchain's own build, the same for every builder. | Release-archive build (R05a) | [`tools/release.sh`](../tools/release.sh), [`tools/tests/test_release_paths.py`](../tools/tests/test_release_paths.py) |
| Inner `SHA256SUMS` | Done on 2026-10-14 for the packaging check. It lists every file of the archive, the license texts too. The archive check of the sysroot test passes. It runs again on both release archives. | Release-archive build (R05a) | [`tools/tests/test_sysroot.py`](../tools/tests/test_sysroot.py) with `EVOCC_RELEASE_ARCHIVE` |
| One `SHA256SUMS` for both archives | Open on 2026-10-27. `tools/release.sh` merges digests only in its own output directory, and the two archives build on two hosts. The release-archive build writes one `SHA256SUMS` for both archives, as the publish job of `release.yml` does. The owner publishes it. | Release-archive build (R05a); publication: Owner (R05k) | [`tools/release.sh`](../tools/release.sh) |

## Notices

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md` in the archive | Done at `08bf09bd` (2026-10-27): `tools/release.sh` copies the three files into the archive. | Release-archive build (R05a) | [`tools/release.sh`](../tools/release.sh) |
| Notices for the four binaries | Done at `08bf09bd` (2026-10-27). The notices name `evo`, `evocc`, `evocc-checker` and `evoir-sem` and the third-party code in each. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1 |
| License texts of the Linux headers | Done at `08bf09bd` (2026-10-27). The archive holds the Debian copyright files of the 11 packages and the 12 license texts that they cite. The contract pins all 23 files, and `tools/release.sh` refuses a mismatch. | Release-archive build (R05a) | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`contracts/sysroot-aarch64-unknown-linux-gnu.json`](../contracts/sysroot-aarch64-unknown-linux-gnu.json) |
| License texts of the code inside the binaries | Done at `08bf09bd` (2026-10-27). `licenses/runtime/` of each archive holds the COPYRIGHT file of musl 1.2.5, the `LICENSE` and `LICENSE-extra` files of libuv 1.52.1 with the notices of its `tree.h` and `inet.c`, and the GNU LGPL 3 and GPL 3 texts for GMP 6.3.0. `tools/release.sh` pins their SHA-256. A changed text or a missing directory stops it with exit status 1 and no archive. | Release-archive build (R05a) | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1; [`licenses/runtime/`](../licenses/runtime/) |

## Install, build and reproducibility

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Install in the README | Done in the text at `08bf09bd` (2026-10-27). The commands download, check and unpack the v0.1.0 assets, which exist only after the publication. The text names the four programs, the source build with `--locked` and the header export. | Owner: publication (R05k) | [`README.md`](../README.md#install) |
| Install in the book | Done in the text at `08bf09bd` (2026-10-27). The page names the four programs and `licenses/runtime/`. The source build uses `--locked`, as `tools/release.sh` does. It says how a source build gets the Linux headers: `tools/export-sysroot.sh`, or the `sysroot/` of a release archive. | Owner: publication (R05k) | [`book/src/install.md`](../book/src/install.md) |
| macOS quarantine | Done as documentation on 2026-10-14 (R05i). The binaries are not notarized: `codesign -dv` shows an ad-hoc signature and no team identifier. The book documents the quarantine attribute of a browser download and the `xattr` commands that clear it. A synthetic quarantined archive showed that `tar` copies the attribute to each file and that both commands clear it. | Nothing | [`book/src/install.md`](../book/src/install.md#a-browser-download-on-macos) |
| Build from source | Done. Rust 1.97.1, Lean v4.32.2 and the vendored crates give an offline build. On 2026-10-27 the release build of `08bf09bd` gave the `evocc` of the release acceptance for macOS and for Linux (musl). | Final gate (T05); release-archive build from a clean checkout (R05a) | [`rust-toolchain.toml`](../rust-toolchain.toml), [`lean/lean-toolchain`](../lean/lean-toolchain), [`vendor/`](../vendor/) |
| Reproduce a release | Done for macOS on 2026-10-23 (R05e, R05f). The book gives the commands that build the programs of a release again from a clean checkout and compare the SHA-256. Two checkouts at two paths gave the same archive. The Linux archive is not reproduced yet. | Release-archive build (R05a) | [`book/src/release.md`](../book/src/release.md) |
| Sysroot provenance | Done on 2026-10-14 (R05c). The default image of the exporter is the image of the contract, `evo-nexlab:sealed-20260810`. Apple `container` cannot run a local image by digest, so the exporter stops when the image has another digest than `sha256:2c23684c`. Two exports without `EVOCC_SYSROOT_IMAGE`, at two paths, wrote the contract byte for byte. An export of `evo-nexlab:sealed` (`sha256:e8d52bdc`) gives the same headers, license texts and packages; only the image name and digest differ. | Release-archive build (R05a) | [`tools/export-sysroot.sh`](../tools/export-sysroot.sh), [`contracts/build-environment.json`](../contracts/build-environment.json) |
| Sysroot size | Done on 2026-10-14 (R05d). The exporter leaves out 24 directories: the `asm/` trees of 21 other triples (1,002 files), the C++ headers (825) and two copies of the Clang 19 resource headers (482), 40.7 MiB in all. 1,657 headers (12.1 MiB) stay. The 10 Objective-C headers stay, because GCC's header directory is on the search path. With the trimmed headers, zlib 1.3.2's `example` and the cross hello link and pass in the sealed image. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`crates/evocc/src/toolchain.rs`](../crates/evocc/src/toolchain.rs) (`linux_directories`) |
| No self-hosting claim | Done at `08bf09bd` (2026-10-27). The README, the book and the paper do not claim self-hosting. A Rust compiler builds EvoCC. | Nothing | [`README.md`](../README.md) |

## Versions and changelog

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Version of `evocc` and `evo` | Done. Both print `evocc 0.1.0 (<commit> <date>)`. On 2026-10-27 the release `evocc` of `08bf09bd` prints `evocc 0.1.0 (08bf09bd7631 2026-10-27)`. | Release-archive build (R05a) | `evocc --version`, `evo version` |
| Version of `evocc-checker` and `evoir-sem` | Done on 2026-10-19 and committed in `f81a13d4`. Both print their binary name and `0.1.0`, exit zero and leave stderr empty. Tests run both from an empty directory and compare with Cargo's version. This does not certify a release archive. | Final gate (T05) | [`crates/evocc/tests/evo_cli.rs`](../crates/evocc/tests/evo_cli.rs); full workspace: 3,475 passed, zero failed, eight ignored; all six Lean steps passed |
| Version policy | Done at `08bf09bd` (2026-10-27). `tools/release.sh` accepts only a `0.y.z` version with a matching heading in `CHANGELOG.md`. | Nothing | [`tools/release.sh`](../tools/release.sh) |
| Package description | Done at `08bf09bd` (2026-10-27). It describes the compiler without internal lab names. | Nothing | [`crates/evocc/Cargo.toml`](../crates/evocc/Cargo.toml) |
| Changelog and tag | Partial on 2026-10-27. The heading `## [0.1.0] - Unreleased` has no date. No `v0.1.0` tag exists. The section lists the changes of the release and its known limits. Its results wait for the frozen acceptance. The owner dates the heading and tags the commit as the last step of the release (R05k). | Frozen acceptance (kernel replay, B10); date and tag: Owner (R05k) | [`CHANGELOG.md`](../CHANGELOG.md) |

## Hosted workflows

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Hosted runs of `ci.yml`, `book.yml`, `nightly.yml` and `release.yml` | Open on 2026-10-27. No hosted run has passed. Since 2026-10-07 GitHub refuses every job because of the billing state of the account. `book.yml` publishes to GitHub Pages, which the owner sets up. On a clean hosted runner, seven tests need inputs that Git does not track or the macOS 26 SDK; the known limits of `CHANGELOG.md` name them. | Owner (R05m) | [`.github/workflows/`](../.github/workflows/) |

## Proof-claim boundary

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| The claim | Done at `08bf09bd` (2026-10-27). Section 1 of the verification contract states the proved path: the subset `core-v1`, the target `aarch64-unknown-linux-gnu`, the trusted base and what is not claimed. | Nothing | [`spec/verification.md`](../spec/verification.md) |
| Release-blocking proof tasks | Done. On 2026-10-27 the tracker records V00 to V08 as complete in their frozen scope. | Final gate (T05) | [`TODO.md`](../TODO.md) |
| README wording | Done on 2026-10-15 (R05l). The opening paragraph, the Highlights, the proof example, the architecture and the Status section name `core-v1`. The proof example and the architecture name the Linux target. | Owner: confirm before the tag (R05k) | [`README.md`](../README.md#status) |

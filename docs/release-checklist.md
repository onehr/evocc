# Release checklist

This page audits the first release, EvoCC 0.1.0, against the repository. It covers the two
archives, checksums, notices, install and build instructions, reproducibility, version strings,
the changelog, the hosted workflows and the proof-claim boundary. The audit ran on 2026-10-13 on
macOS (AArch64), at commit `546fe46a` with the release-hygiene changes of the same day. It was
updated on 2026-10-20 at commit `814188e5`, and on 2026-10-27 at commit `8177cff2`, whose compiler
passed the frozen release acceptance on the same day. Each row names the date of its last check.
Hosted CI is paused by the owner since 2026-10-20 (TODO R05m). On 2026-10-27 a local release
build made both archives and one `SHA256SUMS` from `5125cc44` (R05a).

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
| macOS archive `evocc-0.1.0-aarch64-apple-darwin.tar.gz` | Done on 2026-10-27 at `5125cc44`: `tools/release.sh --target aarch64-apple-darwin --with-lean` from a fresh checkout (`8a4ed58a…f907`, 39,691,226 bytes). The dating commit changes the commit that `evocc --version` prints, so the archive is built again from it with the same checks. | Owner: dating commit (R05k) | [`tools/release.sh`](../tools/release.sh) |
| Smoke test of the macOS archive | Done on 2026-10-27 on the release archive: the two steps of `release.yml` pass for `v0.1.0`; 1,696 entries match; `evocc 0.1.0 (9e83b76a7f1f 2026-10-27)`; interpreter 12; `evoir-sem --selftest` exits 0; the book programs build and run. | Nothing | The smoke commands, run on the extracted archive |
| Linux object from the macOS archive | Done on 2026-10-27: the release archive made `cross-hello.o`; the publish job's step links it with `gcc` in the Linux build container, and it prints `hello from EvoCC on Linux`. | Nothing | `evocc -v --target=aarch64-unknown-linux-gnu` prints the bundled sysroot |
| Linux archive `evocc-0.1.0-aarch64-unknown-linux-gnu.tar.gz` | Done on 2026-10-27 at `5125cc44` (R05a): fresh checkout, `debian:13-slim` AArch64 container, no network (`5a86e3fb…356e`, 45,089,091 bytes). Smoke steps and static-link check pass; `test_sysroot.py` passes; the book programs run with the bundled sysroot and with `--sysroot=/`. | Owner: dating commit (R05k); hosted build (R05m) | [`tools/release.sh`](../tools/release.sh) |
| Linux archive on a host with glibc older than 2.38 | Done on 2026-10-14 (R05h): the book states the floor. On Linux the bundled glibc 2.41 headers come before the host headers. With `_GNU_SOURCE` they redirect `strtol`, `scanf` and related functions to `__isoc23_*` names of glibc 2.38. In `debian:bookworm-slim` (glibc 2.36-9+deb12u14, GCC 12.2.0), a `_GNU_SOURCE` `strtol` object of the compiler of that day does not link: `undefined reference to '__isoc23_strtol'`. The same program, linked against glibc 2.41, stops with ``version `GLIBC_2.38' not found``. Without `_GNU_SOURCE`, and with the Debian 12 headers, it links and runs. | Nothing | [`book/src/install.md`](../book/src/install.md#glibc-on-linux), `usr/include/features.h` of the sysroot |
| Deterministic archive | Done. On 2026-10-27 two fresh checkouts of `5125cc44` at two paths gave the same macOS archive and the same four programs (earlier: 2026-10-14, 2026-10-23). The Linux archive was built at one path. | Nothing | [`tools/release.sh`](../tools/release.sh), [`book/src/release.md`](../book/src/release.md) |
| Path-independent binaries | Done on 2026-10-23 at `5e33decb` (R05e). `tools/release.sh` builds `evo`, `evocc` and `evocc-checker` with a `dist` profile without a debug map, and remaps the checkout, the Rust sysroot and the target directory. A byte search finds no path of the build machine in the three programs; before, `evocc` held 434. The script refuses a program that holds the checkout or target path, and a Cargo program that holds `$HOME`. `evoir-sem` keeps five paths of the Lean toolchain's own build, the same for every builder. | Nothing | [`tools/release.sh`](../tools/release.sh), [`tools/tests/test_release_paths.py`](../tools/tests/test_release_paths.py) |
| Inner `SHA256SUMS` | Done on 2026-10-27 on both release archives: the smoke step checks every entry, and the archive test of `test_sysroot.py` passes on each. | Nothing | [`tools/tests/test_sysroot.py`](../tools/tests/test_sysroot.py) with `EVOCC_RELEASE_ARCHIVE` |
| One `SHA256SUMS` for both archives | Done on 2026-10-27 for the archives of `5125cc44`: `sha256sum evocc-*.tar.gz`, as the publish job does. The owner publishes the file of the dated commit's archives. | Owner: publication (R05k) | [`tools/release.sh`](../tools/release.sh) |

## Notices

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md` in the archive | Done at `8177cff2` (2026-10-27): `tools/release.sh` copies the three files into the archive. | Nothing | [`tools/release.sh`](../tools/release.sh) |
| Notices for the four binaries | Done at `8177cff2` (2026-10-27). The notices name `evo`, `evocc`, `evocc-checker` and `evoir-sem` and the third-party code in each. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1 |
| License texts of the Linux headers | Done at `8177cff2` (2026-10-27). The archive holds the Debian copyright files of the 11 packages and the 12 license texts that they cite. The contract pins all 23 files, and `tools/release.sh` refuses a mismatch. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`contracts/sysroot-aarch64-unknown-linux-gnu.json`](../contracts/sysroot-aarch64-unknown-linux-gnu.json) |
| License texts of the code inside the binaries | Done at `8177cff2` (2026-10-27). `licenses/runtime/` of each archive holds the COPYRIGHT file of musl 1.2.5, the `LICENSE` and `LICENSE-extra` files of libuv 1.52.1 with the notices of its `tree.h` and `inet.c`, and the GNU LGPL 3 and GPL 3 texts for GMP 6.3.0. `tools/release.sh` pins their SHA-256. A changed text or a missing directory stops it with exit status 1 and no archive. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 1; [`licenses/runtime/`](../licenses/runtime/) |

## Install, build and reproducibility

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Install in the README | Done in the text at `8177cff2` (2026-10-27). The commands download, check and unpack the v0.1.0 assets, which exist only after the publication. The text names the four programs, the source build with `--locked` and the header export. | Owner: publication (R05k) | [`README.md`](../README.md#install) |
| Install in the book | Done in the text at `8177cff2` (2026-10-27). The page names the four programs and `licenses/runtime/`. The source build uses `--locked`, as `tools/release.sh` does. It says how a source build gets the Linux headers: `tools/export-sysroot.sh`, or the `sysroot/` of a release archive. | Owner: publication (R05k) | [`book/src/install.md`](../book/src/install.md) |
| macOS quarantine | Done as documentation on 2026-10-14 (R05i). The binaries are not notarized: `codesign -dv` shows an ad-hoc signature and no team identifier. The book documents the quarantine attribute of a browser download and the `xattr` commands that clear it. A synthetic quarantined archive showed that `tar` copies the attribute to each file and that both commands clear it. | Nothing | [`book/src/install.md`](../book/src/install.md#a-browser-download-on-macos) |
| Build from source | Done. Rust 1.97.1, Lean v4.32.2 and the vendored crates give an offline build. On 2026-10-27 the release build of `8177cff2` gave the `evocc` of the release acceptance for macOS and for Linux (musl). | Nothing | [`rust-toolchain.toml`](../rust-toolchain.toml), [`lean/lean-toolchain`](../lean/lean-toolchain), [`vendor/`](../vendor/) |
| Reproduce a release | Done for macOS on 2026-10-27 (`5125cc44`, two paths) and on 2026-10-23 (R05e, R05f). The Linux archive was built at one path. | Nothing | [`book/src/release.md`](../book/src/release.md) |
| Sysroot provenance | Done on 2026-10-14 (R05c). The default image of the exporter is the image of the contract, `evo-nexlab:sealed-20260810`. Apple `container` cannot run a local image by digest, so the exporter stops when the image has another digest than `sha256:2c23684c`. Two exports without `EVOCC_SYSROOT_IMAGE`, at two paths, wrote the contract byte for byte. An export of `evo-nexlab:sealed` (`sha256:e8d52bdc`) gives the same headers, license texts and packages; only the image name and digest differ. | Nothing | [`tools/export-sysroot.sh`](../tools/export-sysroot.sh), [`contracts/build-environment.json`](../contracts/build-environment.json) |
| Sysroot size | Done on 2026-10-14 (R05d). The exporter leaves out 24 directories: the `asm/` trees of 21 other triples (1,002 files), the C++ headers (825) and two copies of the Clang 19 resource headers (482), 40.7 MiB in all. 1,657 headers (12.1 MiB) stay. The 10 Objective-C headers stay, because GCC's header directory is on the search path. With the trimmed headers, zlib 1.3.2's `example` and the cross hello link and pass in the sealed image. | Nothing | [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), section 2; [`crates/evocc/src/toolchain.rs`](../crates/evocc/src/toolchain.rs) (`linux_directories`) |
| No self-hosting claim | Done at `8177cff2` (2026-10-27). The README, the book and the paper do not claim self-hosting. A Rust compiler builds EvoCC. | Nothing | [`README.md`](../README.md) |

## Versions and changelog

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Version of `evocc` and `evo` | Done. The archives of 2026-10-27 print `evocc 0.1.0 (9e83b76a7f1f 2026-10-27)`. | Owner: dating commit (R05k) | `evocc --version`, `evo version` |
| Version of `evocc-checker` and `evoir-sem` | Done on 2026-10-19 and committed in `f53f67b6`. Both print their binary name and `0.1.0`, exit zero and leave stderr empty. Tests run both from an empty directory and compare with Cargo's version. This does not certify a release archive. | Nothing | [`crates/evocc/tests/evo_cli.rs`](../crates/evocc/tests/evo_cli.rs); full workspace: 3,475 passed, zero failed, eight ignored; all six Lean steps passed |
| Version policy | Done at `8177cff2` (2026-10-27). `tools/release.sh` accepts only a `0.y.z` version with a matching heading in `CHANGELOG.md`. | Nothing | [`tools/release.sh`](../tools/release.sh) |
| Package description | Done at `8177cff2` (2026-10-27). It describes the compiler without internal lab names. | Nothing | [`crates/evocc/Cargo.toml`](../crates/evocc/Cargo.toml) |
| Changelog and tag | Partial on 2026-10-28. The dating commit gives the heading its date: `## [0.1.0] - 2026-10-28`. No `v0.1.0` tag exists yet; it marks the dating commit, and both archives are built again from that commit. The section lists the changes of the release, its known limits and the results of the frozen acceptance of 2026-10-27; the results of the final gate and of the archives follow. The owner tags the commit as the last step of the release (R05k). | Tag: Owner (R05k) | [`CHANGELOG.md`](../CHANGELOG.md) |

## Hosted workflows

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| Hosted runs of `ci.yml`, `book.yml`, `nightly.yml` and `release.yml` | Open on 2026-10-28. No hosted run has passed. Since 2026-10-07 GitHub refuses every job because of the billing state of the account. `book.yml` publishes to GitHub Pages, which the owner sets up. On a clean hosted runner, seven tests need inputs that Git does not track or the macOS 26 SDK; the known limits of `CHANGELOG.md` name them. `ci.yml` skips the four that need untracked inputs, and its macOS job, like that of `release.yml`, now runs on `macos-26` for the three that need the macOS 26 SDK. The nightly workflow stays disabled until a hosted run passes, and the archives of 0.1.0 are the local release build. | Owner (R05m) | [`.github/workflows/`](../.github/workflows/) |

## Proof-claim boundary

| Deliverable | State | Waits for | Evidence |
|---|---|---|---|
| The claim | Done at `8177cff2` (2026-10-27). Section 1 of the verification contract states the proved path: the subset `core-v1`, the target `aarch64-unknown-linux-gnu`, the trusted base and what is not claimed. | Nothing | [`spec/verification.md`](../spec/verification.md) |
| Release-blocking proof tasks | Done. On 2026-10-27 the tracker records V00 to V08 as complete in their frozen scope. | Nothing | [`TODO.md`](../TODO.md) |
| README wording | Done on 2026-10-15 (R05l). The opening paragraph, the Highlights, the proof example, the architecture and the Status section name `core-v1`. The proof example and the architecture name the Linux target. | Owner: confirm before the tag (R05k) | [`README.md`](../README.md#status) |

# Installation

You can install EvoCC in two ways. You can download a release, or you can build EvoCC from source.
A release is the faster way.

EvoCC runs on two hosts:

| Host | Target name |
|---|---|
| macOS on Apple silicon | `aarch64-apple-darwin` |
| Linux on 64-bit Arm | `aarch64-unknown-linux-gnu` |

EvoCC writes its own assembly, but it uses the assembler, the linker and the C library of your
system to make a program. On macOS, install the Xcode Command Line Tools with
`xcode-select --install`. On Linux, install a C toolchain, for example the `gcc` package of your
distribution.

## Download a release

Each release on the [GitHub Releases page](https://github.com/onehr/evocc/releases) has one
archive for each host. The name of the archive is `evocc-<version>-<target>.tar.gz`. A file
`SHA256SUMS` beside the archives lists their SHA-256 digests.

These commands download version 0.1.0 for macOS, check the archive and unpack it:

```console
$ curl -LO https://github.com/onehr/evocc/releases/download/v0.1.0/evocc-0.1.0-aarch64-apple-darwin.tar.gz
$ curl -LO https://github.com/onehr/evocc/releases/download/v0.1.0/SHA256SUMS
$ grep evocc-0.1.0-aarch64-apple-darwin.tar.gz SHA256SUMS | shasum -a 256 -c -
$ tar xzf evocc-0.1.0-aarch64-apple-darwin.tar.gz
```

On Linux, use the name with `aarch64-unknown-linux-gnu`, and use `sha256sum -c -` in place of
`shasum -a 256 -c -`.

The archive holds one directory. Its `bin/` directory holds the four programs of EvoCC. The root holds the README, the
files `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md`, and its own `SHA256SUMS`:

| Program | What it does |
|---|---|
| `evo` | The commands to run, check, explain, prove and build. |
| `evocc` | The compiler. You use it like `cc`. |
| `evocc-checker` | The certificate checker. It replays each change of the optimizer ([chapter 6](optimization.md)). |
| `evoir-sem` | The Lean checker. It checks a compilation with proof ([chapter 5](proof.md)). |

Add its `bin/` directory to your `PATH`, and check that the shell finds `evocc`:

```console
$ cd evocc-0.1.0-aarch64-apple-darwin
$ shasum -a 256 -c SHA256SUMS
$ export PATH="$PWD/bin:$PATH"
$ evocc --version
```

`evocc --version` prints two lines. The first line gives the version, the commit and the date of
the build. The second line gives the target that `evocc` makes code for by default.

The `export` line lasts until you close the shell. To keep it, add the line to your shell profile,
for example `~/.zshrc`, with the full path of the directory.

## Nightly builds

The [rolling nightly prerelease](https://github.com/onehr/evocc/releases/tag/nightly)
is the download location for daily and manual builds of `main`. The workflow uses
the same packaging, native smoke checks and cross-host object check as stable
releases. A successful run replaces two archives named
`evocc-nightly-<target>.tar.gz` and their `SHA256SUMS`; it does not create a new
release each day. The directory inside each archive still names the compiler's
version. Verify its checksum before unpacking, as for a stable release.

Nightly is not a stable release. Its release notes identify the source commit,
and `evocc --version` retains the version and commit built into the compiler.
Stable releases keep `vX.Y.Z` tags. The current package version policy is
`0.y.z`; packaging requires an exact matching `## [0.y.z]` heading in
`CHANGELOG.md` before it writes an archive.

## Build from source

To build EvoCC from source, you need:

- Git;
- Rust, installed with [rustup](https://rustup.rs). The file `rust-toolchain.toml` selects the
  right version for you;
- Lean, installed with [elan](https://github.com/leanprover/elan), if you want `evoir-sem`. The
  file `lean/lean-toolchain` selects the right version;
- 25 GiB of free disk space. Check it with `df -h .` before you build.

Clone the repository and build the compiler and the certificate checker:

```console
$ git clone https://github.com/onehr/evocc.git
$ cd evocc
$ CARGO_TARGET_DIR=target/frontend cargo build --release --offline -j 2 -p evocc -p evocc-proofcheck
```

The build is offline: the three Rust libraries that EvoCC uses are in the repository, in `vendor/`.
Cargo writes `evocc` and `evocc-checker` to `target/frontend/release/`.

Then build the Lean checker. Run Lake from inside the `lean` directory. The first build takes a
long time:

```console
$ (cd lean && lake build 2>&1 | tail -n 1)
Build completed successfully (728 jobs).
```

Lake writes `evoir-sem` to `lean/.lake/build/bin/`. Add both directories to your `PATH`:

```console
$ export PATH="$PWD/target/frontend/release:$PWD/lean/.lake/build/bin:$PATH"
```

Now `evocc --version` works as in the release. A build of commit `7b64cc1c0b04` on macOS prints:

```console
$ evocc --version
evocc 0.1.0 (7b64cc1c0b04 2026-10-07)
Target: aarch64-apple-darwin
```

Your build prints the commit and the date of your checkout.

The rest of this book calls the three programs by their names, `evocc`, `evocc-checker` and
`evoir-sem`, and expects them on your `PATH`.

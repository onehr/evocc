# Reproducing a Release

A release archive is the output of `tools/release.sh`. To check a published archive, make it
again from a clean checkout and compare the two.

## What the archive holds

The archive of a release depends on these inputs:

- the four programs: `evo`, `evocc`, `evocc-checker` and `evoir-sem`;
- the files `README.md`, `LICENSE`, `NOTICE` and `THIRD-PARTY-NOTICES.md` of the checkout;
- the license texts in `licenses/runtime/` of the checkout. `tools/release.sh` pins the SHA-256 of
  each text;
- the Linux headers and their license texts. Each file must have the SHA-256 that
  [the contract](https://github.com/onehr/evocc/blob/main/contracts/sysroot-aarch64-unknown-linux-gnu.json)
  of the checkout records;
- the version, the target, and the commit time of the checkout. `SOURCE_DATE_EPOCH` replaces the
  commit time;
- the zlib library of the Python that runs the script. It compresses the archive. The check below
  used Python 3.9.6 with zlib 1.2.12.

The script sorts the entries. It writes 0 as the owner and the group of each entry, and the commit
time as the time of each entry. The gzip header has no name and no time. Thus the path of the
checkout, the user and the time of the run do not change the archive.

The script writes no archive unless the version has the form `0.y.z` and `CHANGELOG.md` has the
heading `## [0.y.z]` of that version.

## Package the programs of a release again

These steps make the archive again from the programs of a published archive. If the SHA-256 is
the same, the published archive holds the files of the release commit and the pinned headers,
and nothing more.

First, download the archive and `SHA256SUMS`, check the archive and unpack it, as
[Installation](install.md) shows. Then check out the release commit in a new directory. In a clone
of the repository, `git worktree add` does this. Give the release tag in place of `HEAD`:

```console
$ git worktree add --detach ../evocc-check HEAD
$ cd ../evocc-check
```

Export the Linux headers from the pinned image, as [Installation](install.md#linux-headers-for-a-source-build)
describes:

```console
$ tools/export-sysroot.sh
```

Without the image, move the `sysroot/` directory of the unpacked archive into the checkout. Then
the check compares the headers of the archive with the contract of the release commit, but it does
not export them again.

Put the programs where `tools/release.sh --skip-build` looks for them. `evo`, `evocc` and
`evocc-checker` go in `dist/` of the Cargo target directory: `target/`, or the directory that
`CARGO_TARGET_DIR` names. `evoir-sem` goes in `lean/.lake/build/bin/` of the checkout. These
commands take them from the archive that you unpacked beside the checkout:

```console
$ mkdir -p ../programs/dist lean/.lake/build/bin
$ cp ../evocc-0.1.0-aarch64-apple-darwin/bin/evo ../evocc-0.1.0-aarch64-apple-darwin/bin/evocc ../evocc-0.1.0-aarch64-apple-darwin/bin/evocc-checker ../programs/dist/
$ cp ../evocc-0.1.0-aarch64-apple-darwin/bin/evoir-sem lean/.lake/build/bin/
$ CARGO_TARGET_DIR="$PWD/../programs" tools/release.sh --skip-build --out ../repack
$ shasum -a 256 ../repack/evocc-0.1.0-aarch64-apple-darwin.tar.gz
```

Compare the digest with the line of the archive in the published `SHA256SUMS`. The script also
writes the digest to `../repack/SHA256SUMS`.

These steps were checked with the macOS archive. Three checkouts of the same commit, at three
paths, gave archives with the same SHA-256, with the exported headers and with the headers of the
archive. For the Linux archive, put `evo`, `evocc` and `evocc-checker` in
`aarch64-unknown-linux-musl/dist/` of the Cargo target directory, and give
`--target aarch64-unknown-linux-gnu`.

## Build the programs again

Without `--skip-build`, `tools/release.sh` builds the programs first. It runs
`cargo build --profile dist --offline --locked -p evocc -p evocc-proofcheck`. It builds
`evoir-sem` with Lake when `evoir-sem` is absent, or when you give `--with-lean`. A build must run
on its target host. The checkout path must not contain a space, because `RUSTFLAGS` splits at
spaces. [Installation](install.md#build-from-source) lists what the build needs.

The dist profile removes the debug map and remaps the checkout, the Rust sysroot and the Cargo
target directory. Before it writes an archive, the script refuses a program that holds the path
of the checkout or of the Cargo target directory, and a Cargo program that holds `$HOME`.
`evoir-sem` holds paths of the Lean toolchain's own build. They are the same for every builder of
the release, so the script does not look for `$HOME` in it.

On an Apple silicon Mac with macOS 26.6 and Rust 1.97.1, two checkouts at two paths, each with its
own target directory, gave byte-identical programs and archives.

Neither check used a Linux archive: none was available to repack, and a Linux build needs an
AArch64 Linux host.

#!/bin/bash
# Unchanged --owner/--group overrides must hide an on-disk chown.
# Run in the disposable Linux container; arguments are audited toybox binaries.
set -eu
runtime=$(mktemp -d /tmp/tb-owner.XXXXXX)
trap 'rm -rf -- "$runtime"' EXIT
cd "$runtime"
umask 0002
mkdir dir
touch dir/file
ln dir/file dir/hardlink
for binary in "$@"; do
    chown root:root dir/file
    "$binary" tar c --owner root --group sys --mtime @1234567890 dir/file > before.tar
    chown nobody:nogroup dir/file
    "$binary" tar c --owner root --group sys --mtime @1234567890 dir/file > after.tar
    cmp before.tar after.tar
    printf '%s identical override archive: ' "$binary"
    head -c 1536 after.tar | sha1sum
done

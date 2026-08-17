#!/bin/sh

set -u

if [ "$#" -ne 5 ]; then
  printf 'usage: %s LANE TOOL SOURCE_DIR OUTPUT_DIR SOURCE_DATE_EPOCH\n' "$0" >&2
  exit 2
fi

lane="$1"
tool="$2"
source_dir="$3"
output_dir="$4"
source_date_epoch="$5"

cd "$source_dir" || exit 2

for interface in /sys/class/net/*; do
  [ "${interface##*/}" = lo ] || {
    printf 'unexpected network interface: %s\n' "${interface##*/}" >&2
    exit 70
  }
done
network_rc=0
/usr/bin/timeout 3 /bin/bash -c 'exec 3<>/dev/tcp/1.1.1.1/80' \
  >/dev/null 2>&1 || network_rc=$?
if [ "$network_rc" -eq 0 ]; then
  printf 'network-none lane reached an external endpoint\n' >&2
  exit 70
fi
printf 'PASS network_none active_probe_rc=%s\n' "$network_rc"

for source in [0-9][0-9][0-9]-*.c; do
  base="$(basename "$source")"
  case "$lane" in
    evocc)
      SOURCE_DATE_EPOCH="$source_date_epoch" /usr/bin/timeout 10 "$tool" \
        --target=aarch64-unknown-linux-gnu \
        -std=gnu11 -E -P -undef -nostdinc -I. -Iinclude "$base" \
        >"$output_dir/$base.out" 2>"$output_dir/$base.stderr"
      ;;
    gcc|clang)
      SOURCE_DATE_EPOCH="$source_date_epoch" /usr/bin/timeout 10 "$tool" \
        -std=gnu11 -E -P -undef -nostdinc -I. -Iinclude "$base" \
        >"$output_dir/$base.out" 2>"$output_dir/$base.stderr"
      ;;
    *)
      printf 'unknown CPP lane: %s\n' "$lane" >&2
      exit 2
      ;;
  esac
  printf '%s\n' "$?" >"$output_dir/$base.rc"
done

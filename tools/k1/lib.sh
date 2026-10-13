#!/bin/bash

# Shared helpers for the mixed-link measurement scripts.
#
# The QEMU recipe and the boot predicate are the ones the day-one nexlab
# harness pinned (.toolchain/boot-reference-kernel.sh and
# tests/acceptance/nexlab/run.sh). They are reproduced here rather than
# reused because the original appends a receipt into .build/reference, which
# is protected evidence this lane must never write into.

export LC_ALL=C TZ=UTC

K1_JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
K1_SHASUM="${C30_SHASUM:-/usr/bin/shasum}"
K1_TIMEOUT="${C30_TIMEOUT:-/opt/homebrew/bin/timeout}"

k1_die() { printf 'k1: %s\n' "$1" >&2; exit 1; }

k1_sha256() { "$K1_SHASUM" -a 256 "$1" | awk '{print $1}'; }

k1_require_tools() {
  for tool in "$K1_JQ" "$K1_SHASUM" "$K1_TIMEOUT"; do
    [ -x "$tool" ] || k1_die "required tool is unavailable: $tool"
  done
}

# A fresh nonce per flight. An old serial log can never satisfy a new one.
k1_nonce() {
  printf 'k1-%s-%s' "$(date -u '+%Y%m%dT%H%M%SZ')" "$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
}

# The emulator the contract pins. Drift is refused by default: a boot verdict
# is only comparable across runs on one emulator. Setting K1_ALLOW_QEMU_DRIFT=1
# accepts a drifted binary, and every receipt then carries both digests so the
# deviation is in the evidence rather than in a person's memory.
k1_qemu_binary() {
  local contract="$1"
  local qemu expected actual
  qemu="$("$K1_JQ" -r '.qemu.binaryPath' "$contract")"
  expected="$("$K1_JQ" -r '.qemu.binarySha256' "$contract")"
  actual="$(k1_sha256 "$qemu")"
  if [ "$actual" != "$expected" ] && [ "${K1_ALLOW_QEMU_DRIFT:-0}" != "1" ]; then
    k1_die "QEMU binary drifted from contracts/kernel-target.json: $actual != $expected (set K1_ALLOW_QEMU_DRIFT=1 to proceed and record the drift)"
  fi
  printf '%s' "$qemu"
}

# The digest of the emulator actually used, for the receipt.
k1_qemu_facts() {
  local contract="$1" qemu expected actual
  qemu="$("$K1_JQ" -r '.qemu.binaryPath' "$contract")"
  expected="$("$K1_JQ" -r '.qemu.binarySha256' "$contract")"
  actual="$(k1_sha256 "$qemu")"
  "$K1_JQ" -n --arg path "$qemu" --arg actual "$actual" --arg expected "$expected" \
    '{path:$path,sha256:$actual,contractSha256:$expected,matchesContract:($actual==$expected)}'
}

# Keep diagnostics on one of the target contract's declared CPU models.
k1_boot_lane_args() {
  local contract="$1" lane="$2"
  "$K1_JQ" -e -r --arg lane "$lane" \
    '[.qemu.lanes[] | select(.name == $lane)] | if length == 1 then .[0].argv[] else error("unknown boot lane") end' \
    "$contract"
}

# Use Linux's normal init environment and the upstream runner's per-program
# argument variable. No altered payload or separate diagnostic runner is needed.
k1_selftest_cmdline() {
  local program="$1" args="$2" timeout="${3:-}" hugepages="${4:-}" name
  if [ -z "$program" ]; then
    [ -z "$args" ] || k1_die "selftest arguments require one program"
    [ -z "$timeout" ] || k1_die "selftest timeout requires one program"
    [ -z "$hugepages" ] || k1_die "huge-page setup requires one program"
    return
  fi
  case "$program" in
    *[!a-zA-Z0-9_/:.+-]*|*..*) k1_die "invalid selftest collection or program" ;;
  esac
  printf ' evo_selftest=%s' "$program"
  if [ -n "$hugepages" ]; then
    case "$program" in *:*) ;; *) k1_die "huge-page setup requires one program" ;; esac
    case "$hugepages" in *[!0-9]*) k1_die "invalid huge-page count" ;; esac
    [ "${#hugepages}" -le 4 ] && [ "$hugepages" -ge 1 ] && [ "$hugepages" -le 1024 ] || k1_die "huge-page count outside 1..1024"
    printf ' kselftest_hugepages=%s' "$hugepages"
  fi
  if [ -n "$timeout" ]; then
    case "$program" in *:*) ;; *) k1_die "selftest timeout requires one program" ;; esac
    case "$timeout" in *[!0-9]*) k1_die "invalid selftest timeout" ;; esac
    [ "${#timeout}" -le 5 ] && [ "$timeout" -ge 1 ] && [ "$timeout" -le 86400 ] || k1_die "selftest timeout outside 1..86400 seconds"
    printf ' kselftest_override_timeout=%s' "$timeout"
  fi
  if [ -n "$args" ]; then
    case "$program" in *:*) ;; *) k1_die "selftest arguments require one program" ;; esac
    case "$args" in
      *[!a-zA-Z0-9_/:.,+\ =-]*) k1_die "invalid selftest arguments" ;;
    esac
    [ "${#args}" -le 512 ] || k1_die "selftest arguments exceed the command-line bound"
    name="${program##*/}"
    name="${name#*:}"
    name="$(printf '%s' "$name" | tr -c '[:alnum:]_' '_' | tr '[:lower:]' '[:upper:]')"
    printf ' KSELFTEST_%s_ARGS="%s"' "$name" "$args"
  fi
}

k1_kunit_cmdline() {
  local filter="$1" autorun="${2:-}"
  case "$autorun" in
    '') ;;
    0|1) printf ' kunit.autorun=%s' "$autorun" ;;
    *) k1_die "KUnit autorun must be 0 or 1" ;;
  esac
  [ -n "$filter" ] || return 0
  case "$filter" in *[!a-zA-Z0-9_.*?-]*) k1_die "invalid KUnit filter" ;; esac
  [ "${#filter}" -le 256 ] || k1_die "KUnit filter exceeds the command-line bound"
  printf ' kunit.filter_glob=%s' "$filter"
}

# Nested KVM is opt-in even on a CPU implementing FEAT_NV2. Keep this
# supplemental selftest mode separate from the unchanged K6 command line.
k1_kvm_cmdline() {
  case "$1" in
    '') ;;
    nested) printf ' kvm-arm.mode=nested' ;;
    *) k1_die "unsupported KVM selftest mode" ;;
  esac
}

# k1_boot <contract> <image> <initramfs> <nonce> <serial-log> <timeout-seconds>
#
# Boots one Image on the hvf-host lane and evaluates the ordered predicate:
# EVO_INIT_ALIVE, then the exact nonce exactly once, then EVO_INIT_DONE, then
# a controlled power down. Prints one verdict word on stdout: PASS, TIMEOUT,
# QEMU_EXIT_<n>, MARKERS_MISSING, MARKERS_OUT_OF_ORDER, NONCE_COUNT_<n> or
# NO_POWERDOWN. Always returns 0; the verdict is the result.
k1_boot() {
  local contract="$1" image="$2" initramfs="$3" nonce="$4" log="$5" limit="$6"
  local qemu machine smp mem rc
  qemu="$(k1_qemu_binary "$contract")" || return 1
  machine="$("$K1_JQ" -r '.qemu.machine' "$contract")"
  smp="$("$K1_JQ" -r '.qemu.fixedArgv.smp' "$contract")"
  mem="$("$K1_JQ" -r '.qemu.fixedArgv.memoryMiB' "$contract")"
  local command_line="console=ttyAMA0 panic=-1 evo_nonce=$nonce"
  local selftest_cmdline
  selftest_cmdline="$(k1_selftest_cmdline "${K1_SELFTEST_COLLECTION:-}" "${K1_SELFTEST_ARGS:-}" "${K1_SELFTEST_TIMEOUT:-}" "${K1_SELFTEST_HUGEPAGES:-}")" || return 1
  command_line="$command_line$selftest_cmdline"
  local kunit_cmdline
  kunit_cmdline="$(k1_kunit_cmdline "${K1_KUNIT_FILTER:-}" "${K1_KUNIT_AUTORUN:-}")" || return 1
  command_line="$command_line$kunit_cmdline"
  local kvm_cmdline
  kvm_cmdline="$(k1_kvm_cmdline "${K1_KVM_MODE:-}")" || return 1
  command_line="$command_line$kvm_cmdline"
  if [ -n "${K1_CONSOLE_LOGLEVEL:-}" ]; then
    case "$K1_CONSOLE_LOGLEVEL" in
      [1-8]) command_line="$command_line loglevel=$K1_CONSOLE_LOGLEVEL" ;;
      *) k1_die "invalid console loglevel" ;;
    esac
  fi
  local -a test_disk=()
  if [ -n "${K1_TEST_DISK:-}" ]; then
    test_disk=(-drive "file=$K1_TEST_DISK,format=raw,if=none,id=selftest,snapshot=on"
               -device virtio-blk-device,drive=selftest)
  fi

  local lane_args_text arg
  local -a lane_args=()
  lane_args_text="$(k1_boot_lane_args "$contract" "${K1_BOOT_LANE:-hvf-host}")" || return 1
  while IFS= read -r arg; do lane_args+=("$arg"); done <<< "$lane_args_text"

  set +e
  "$K1_TIMEOUT" --foreground "$limit" "$qemu" \
    -M "$machine",dtb-randomness=off \
    "${lane_args[@]}" \
    -smp "$smp" \
    -m "$mem" \
    -kernel "$image" \
    -initrd "$initramfs" \
    ${test_disk[@]+"${test_disk[@]}"} \
    -append "$command_line" \
    -nographic \
    -no-reboot \
    > "$log" 2>&1
  rc=$?
  set -e

  if [ "$rc" -eq 124 ]; then printf 'TIMEOUT'; return 0; fi
  if [ "$rc" -ne 0 ]; then printf 'QEMU_EXIT_%d' "$rc"; return 0; fi

  local alive nonce_off done_off count
  alive="$(grep -a -b -o -F 'EVO_INIT_ALIVE' "$log" | head -1 | cut -d: -f1 || true)"
  nonce_off="$(grep -a -b -o -F "EVO_BOOT_NONCE $nonce" "$log" | head -1 | cut -d: -f1 || true)"
  done_off="$(grep -a -b -o -F 'EVO_INIT_DONE' "$log" | head -1 | cut -d: -f1 || true)"
  if [ -z "$alive" ] || [ -z "$nonce_off" ] || [ -z "$done_off" ]; then
    printf 'MARKERS_MISSING'; return 0
  fi
  if [ "$alive" -ge "$nonce_off" ] || [ "$nonce_off" -ge "$done_off" ]; then
    printf 'MARKERS_OUT_OF_ORDER'; return 0
  fi
  count="$(grep -a -c -F "EVO_BOOT_NONCE $nonce" "$log")"
  if [ "$count" -ne 1 ]; then printf 'NONCE_COUNT_%s' "$count"; return 0; fi
  grep -a -q -F 'reboot: Power down' "$log" || { printf 'NO_POWERDOWN'; return 0; }
  printf 'PASS'
}

#!/bin/bash

# Fail-closed acceptance for the Apple container bootstrap runtime.
#
# This harness intentionally aggregates PASS, FAIL, and BLOCKED outcomes instead
# of stopping at the first missing dependency. A pre-install run must therefore
# preserve a complete red result set, while a release run succeeds only when all
# required IDs in both this file and the locked contract are present and PASS.

set -u
set -o pipefail

export LC_ALL=C
umask 077

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
CONTRACT="${C30_CONTAINER_CONTRACT:-$PROJECT_ROOT/contracts/apple-container-bootstrap.json}"
EVIDENCE_ROOT="${C30_CONTAINER_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/apple-container-bootstrap}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RAW_DIR="$RUN_DIR/raw"
RESULTS="$RUN_DIR/results.jsonl"
SUMMARY="$RUN_DIR/summary.json"

JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/bin/timeout}"

mkdir -p "$RAW_DIR"
: >"$RESULTS"

if [ ! -x "$JQ" ]; then
  printf 'required jq is unavailable: %s\n' "$JQ" >&2
  exit 2
fi

if [ ! -r "$CONTRACT" ] || ! "$JQ" -e . "$CONTRACT" >/dev/null 2>&1; then
  printf 'contract is absent or invalid JSON: %s\n' "$CONTRACT" >&2
  exit 2
fi

cp "$CONTRACT" "$RUN_DIR/contract.json"
"$SHASUM" -a 256 "$RUN_DIR/contract.json" >"$RUN_DIR/contract.sha256"

VERSION="$($JQ -r '.release.version // ""' "$CONTRACT")"
COMMIT_SHA="$($JQ -r '.release.commitSha // ""' "$CONTRACT")"
RELEASE_API_URL="$($JQ -r '.release.releaseApiUrl // ""' "$CONTRACT")"
ASSET_ID="$($JQ -r '.installer.assetId // 0' "$CONTRACT")"
ASSET_NAME="$($JQ -r '.installer.fileName // ""' "$CONTRACT")"
ASSET_URL="$($JQ -r '.installer.url // ""' "$CONTRACT")"
PKG_PATH="$($JQ -r '.installer.localPath // ""' "$CONTRACT")"
PKG_SIZE="$($JQ -r '.installer.sizeBytes // 0' "$CONTRACT")"
PKG_SHA="$($JQ -r '.installer.sha256 // ""' "$CONTRACT")"
PKG_ID="$($JQ -r '.installer.packageIdentifier // ""' "$CONTRACT")"
SIGNER_CN="$($JQ -r '.installer.signer.commonName // ""' "$CONTRACT")"
SIGNER_FP="$($JQ -r '.installer.signer.sha256Fingerprint // ""' "$CONTRACT")"
SIGNER_INTERMEDIATE="$($JQ -r '.installer.signer.intermediateCommonName // ""' "$CONTRACT")"
SIGNER_ROOT="$($JQ -r '.installer.signer.rootCommonName // ""' "$CONTRACT")"
CONTAINER_BIN="$($JQ -r '.payload.cliPath // ""' "$CONTRACT")"
CONTAINER_SHA="$($JQ -r '.payload.cliSha256 // ""' "$CONTRACT")"
UNINSTALLER="$($JQ -r '.payload.uninstallerPath // ""' "$CONTRACT")"
UNINSTALLER_SHA="$($JQ -r '.payload.uninstallerSha256 // ""' "$CONTRACT")"
APP_ROOT="$($JQ -r '.runtime.appRoot // ""' "$CONTRACT")"
INSTALL_ROOT="$($JQ -r '.payload.installRoot // ""' "$CONTRACT")"
KERNEL_URL="$($JQ -r '.runtime.kernel.url // ""' "$CONTRACT")"
KERNEL_DIGEST="$($JQ -r '.runtime.kernel.digest // ""' "$CONTRACT")"
KERNEL_BINARY="$($JQ -r '.runtime.kernel.binaryPath // ""' "$CONTRACT")"
VMINIT_IMAGE="$($JQ -r '.runtime.vminitImage // ""' "$CONTRACT")"
BUILDER_IMAGE="$($JQ -r '.runtime.builderImage // ""' "$CONTRACT")"
PLATFORM="$($JQ -r '.probe.platform // ""' "$CONTRACT")"
IMAGE="$($JQ -r '.probe.image // ""' "$CONTRACT")"
IMAGE_DIGEST="$($JQ -r '.probe.manifestDigest // ""' "$CONTRACT")"
ALPINE_VERSION="$($JQ -r '.probe.alpineVersion // ""' "$CONTRACT")"
NETWORK_URL="$($JQ -r '.probe.networkUrl // ""' "$CONTRACT")"
NETWORK_TEXT="$($JQ -r '.probe.networkExpectedText // ""' "$CONTRACT")"
NETWORK_TIMEOUT="$($JQ -r '.probe.networkTimeoutSeconds // 0' "$CONTRACT")"
RESOURCE_PREFIX="$($JQ -r '.resources.namePrefix // ""' "$CONTRACT")"

RUN_TOKEN="$(printf '%s' "$RUN_ID" | tr '[:upper:]_' '[:lower:]-' | tr -cd 'a-z0-9-')"
VM_A="${RESOURCE_PREFIX}vm-a-${RUN_TOKEN}"
VM_B="${RESOURCE_PREFIX}vm-b-${RUN_TOKEN}"
VM_A_NONCE="nonce-a-${RUN_TOKEN}"
VM_B_NONCE="nonce-b-${RUN_TOKEN}"
VM_A_RESTART_NONCE="nonce-a-restart-${RUN_TOKEN}"
VM_B_CHECK_NONCE="nonce-b-check-${RUN_TOKEN}"
PROBE_NAME="${RESOURCE_PREFIX}probe-${RUN_TOKEN}"
EXPOSURE_NAME="${RESOURCE_PREFIX}exposure-${RUN_TOKEN}"
DEFAULT_NET_NAME="${RESOURCE_PREFIX}default-net-${RUN_TOKEN}"
INTERNAL_NAME="${RESOURCE_PREFIX}internal-${RUN_TOKEN}"
INTERNAL_RUN_NAME="${RESOURCE_PREFIX}internal-run-${RUN_TOKEN}"
DEFAULT_AFTER_NAME="${RESOURCE_PREFIX}default-after-${RUN_TOKEN}"
HOST_CANARY="/private/tmp/${RESOURCE_PREFIX}host-canary-${RUN_TOKEN}"
WORKSPACE_CANARY="$PROJECT_ROOT/.${RESOURCE_PREFIX}host-canary-${RUN_TOKEN}"

BASELINE_CAPTURED=0
FOREIGN_RESOURCES_PASS=0
DEFAULT_NETWORK_PASS=0
PROBE_IMAGE_PREEXISTED=0

# This immutable list prevents a contract edit from silently deleting a gate.
SCRIPT_REQUIRED_IDS='host_is_supported
contract_is_locked
installer_asset_is_exact
installer_signature_is_trusted
receipt_and_payload_are_exact
recovery_path_is_available
runtime_version_is_exact
kernel_configuration_is_exact
foreign_mutable_resources_are_absent
system_lifecycle_is_idempotent
probe_image_is_exact_arm64
two_vms_are_independent
default_host_data_is_not_exposed
default_network_has_outbound_connectivity
internal_network_blocks_outbound
cleanup_has_no_test_residue'

record_result() {
  result_id="$1"
  result_status="$2"
  result_message="$3"
  result_evidence="${4:-}"

  "$JQ" -cn \
    --arg id "$result_id" \
    --arg status "$result_status" \
    --arg message "$result_message" \
    --arg evidence "$result_evidence" \
    --arg recordedAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    '{id:$id,status:$status,message:$message,evidence:$evidence,recordedAt:$recordedAt}' \
    >>"$RESULTS"
}

capture() {
  capture_name="$1"
  shift
  capture_stdout="$RAW_DIR/${capture_name}.stdout"
  capture_stderr="$RAW_DIR/${capture_name}.stderr"
  capture_rc="$RAW_DIR/${capture_name}.rc"

  "$@" >"$capture_stdout" 2>"$capture_stderr"
  rc=$?
  printf '%s\n' "$rc" >"$capture_rc"
  return "$rc"
}

captured_rc() {
  cat "$RAW_DIR/$1.rc"
}

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

runtime_ready() {
  [ -x "$CONTAINER_BIN" ] || return 1
  capture runtime-ready-status "$CONTAINER_BIN" system status --format json || return 1
  "$JQ" -e --arg app "$APP_ROOT" --arg install "$INSTALL_ROOT" '
    .status == "running" and .appRoot == $app and .installRoot == $install
  ' "$RAW_DIR/runtime-ready-status.stdout" >/dev/null 2>&1
}

exact_cleanup() {
  [ -x "$CONTAINER_BIN" ] || return 0

  # Every destructive command names only resources owned by this run. Broad
  # --all and prune operations are deliberately forbidden.
  for owned_container in \
    "$VM_A" "$VM_B" "$PROBE_NAME" "$EXPOSURE_NAME" \
    "$DEFAULT_NET_NAME" "$DEFAULT_AFTER_NAME" "$INTERNAL_RUN_NAME"; do
    "$CONTAINER_BIN" delete --force "$owned_container" >/dev/null 2>&1 || true
  done
  "$CONTAINER_BIN" network delete "$INTERNAL_NAME" >/dev/null 2>&1 || true
  if [ -e "$HOST_CANARY" ]; then
    /bin/rm -f -- "$HOST_CANARY"
  fi
  if [ -e "$WORKSPACE_CANARY" ]; then
    /bin/rm -f -- "$WORKSPACE_CANARY"
  fi
}

preserve_exit_cleanup() {
  main_rc=$?
  trap - EXIT INT TERM
  exact_cleanup
  exit "$main_rc"
}

trap preserve_exit_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

manifest_tree() {
  manifest_root="$1"
  manifest_output="$2"
  manifest_ndjson="$manifest_output.ndjson"
  : >"$manifest_ndjson"

  if [ ! -d "$manifest_root" ]; then
    printf '[]\n' >"$manifest_output"
    return 0
  fi

  find "$manifest_root" -print0 | while IFS= read -r -d '' manifest_path; do
    relative_path="${manifest_path#"$manifest_root"}"
    relative_path="${relative_path#/}"
    [ -n "$relative_path" ] || continue

    file_type="$(stat -f '%HT' "$manifest_path" 2>/dev/null || printf 'UNREADABLE')"
    file_mode="$(stat -f '%Mp%Lp' "$manifest_path" 2>/dev/null || printf '')"
    file_size="$(stat -f '%z' "$manifest_path" 2>/dev/null || printf '0')"
    file_sha=""
    link_target=""
    if [ -f "$manifest_path" ] && [ ! -L "$manifest_path" ]; then
      file_sha="$(sha256_file "$manifest_path" 2>/dev/null || printf 'UNREADABLE')"
    elif [ -L "$manifest_path" ]; then
      link_target="$(readlink "$manifest_path" 2>/dev/null || printf 'UNREADABLE')"
    fi

    "$JQ" -cn \
      --arg path "$relative_path" \
      --arg type "$file_type" \
      --arg mode "$file_mode" \
      --argjson size "${file_size:-0}" \
      --arg sha256 "$file_sha" \
      --arg linkTarget "$link_target" \
      '{path:$path,type:$type,mode:$mode,size:$size,sha256:$sha256,linkTarget:$linkTarget}' \
      >>"$manifest_ndjson"
  done

  "$JQ" -s 'sort_by(.path)' "$manifest_ndjson" >"$manifest_output"
}

test_host_is_supported() {
  host_arch="$(uname -m)"
  host_version="$(sw_vers -productVersion)"
  host_build="$(sw_vers -buildVersion)"
  host_major="${host_version%%.*}"
  expected_arch="$($JQ -r '.host.architecture' "$CONTRACT")"
  minimum_major="$($JQ -r '.host.minimumMacOSMajor' "$CONTRACT")"

  "$JQ" -n \
    --arg architecture "$host_arch" \
    --arg productVersion "$host_version" \
    --arg buildVersion "$host_build" \
    '{architecture:$architecture,productVersion:$productVersion,buildVersion:$buildVersion}' \
    >"$RUN_DIR/host.json"

  if [ "$host_arch" = "$expected_arch" ] && \
     [ "$host_major" -ge "$minimum_major" ] 2>/dev/null && \
     [ -n "$host_build" ]; then
    record_result host_is_supported PASS "host is Apple Silicon on a supported macOS major" "host.json"
  else
    record_result host_is_supported FAIL "host architecture or macOS version is outside the locked contract" "host.json"
  fi
}

test_contract_is_locked() {
  printf '%s\n' "$SCRIPT_REQUIRED_IDS" | sort >"$RAW_DIR/script-required-ids.txt"
  "$JQ" -r '.acceptance.requiredTestIds[]' "$CONTRACT" | sort >"$RAW_DIR/contract-required-ids.txt"

  contract_shape_ok=0
  contract_ids_ok=0
  if "$JQ" -e '
      .schemaVersion == 1 and
      .contractState == "locked" and
      (.release.version | test("^[0-9]+\\.[0-9]+\\.[0-9]+$")) and
      (.release.commitSha | test("^[0-9a-f]{40}$")) and
      (.installer.assetId | type == "number") and
      (.installer.sizeBytes > 0) and
      (.installer.sha256 | test("^[0-9a-f]{64}$")) and
      (.installer.signer.sha256Fingerprint | test("^[0-9A-F]{64}$")) and
      (.payload.cliSha256 | test("^[0-9a-f]{64}$")) and
      (.payload.uninstallerSha256 | test("^[0-9a-f]{64}$")) and
      (.probe.manifestDigest | test("^sha256:[0-9a-f]{64}$")) and
      ([.. | strings | select(test("(^|[^A-Za-z])(TBD|UNRESOLVED|latest)([^A-Za-z]|$)"; "i"))] | length == 0) and
      ((.acceptance.requiredTestIds | length) == (.acceptance.requiredTestIds | unique | length))
    ' "$CONTRACT" >/dev/null 2>&1; then
    contract_shape_ok=1
  fi
  if cmp -s "$RAW_DIR/script-required-ids.txt" "$RAW_DIR/contract-required-ids.txt"; then
    contract_ids_ok=1
  fi

  if [ "$contract_shape_ok" -eq 1 ] && [ "$contract_ids_ok" -eq 1 ]; then
    record_result contract_is_locked PASS "contract is immutable in shape and contains every hard-coded gate" "contract.json"
  else
    record_result contract_is_locked FAIL "contract is candidate, unresolved, malformed, duplicated, or missing a hard-coded gate" "raw/contract-required-ids.txt"
  fi
}

test_installer_asset_is_exact() {
  release_metadata_ok=1
  capture release-api /usr/bin/curl --fail --silent --show-error --location \
    --proto '=https' --tlsv1.2 "$RELEASE_API_URL" || release_metadata_ok=0
  if [ "$release_metadata_ok" -eq 1 ]; then
    "$JQ" -e \
      --arg version "$VERSION" \
      --arg name "$ASSET_NAME" \
      --arg url "$ASSET_URL" \
      --arg digest "sha256:$PKG_SHA" \
      --argjson assetId "$ASSET_ID" \
      --argjson size "$PKG_SIZE" '
        .tag_name == $version and
        ([.assets[] | select(.name == $name)] | length == 1) and
        ([.assets[] | select(.name == $name)][0] |
          .id == $assetId and .size == $size and .digest == $digest and
          .browser_download_url == $url)
      ' "$RAW_DIR/release-api.stdout" >/dev/null 2>&1 || release_metadata_ok=0
  fi

  if [ ! -f "$PKG_PATH" ]; then
    record_result installer_asset_is_exact FAIL "the exact signed installer asset is absent; release metadata was retained separately" "$PKG_PATH"
    return
  fi

  local_size="$(stat -f '%z' "$PKG_PATH" 2>/dev/null || printf '0')"
  local_sha="$(sha256_file "$PKG_PATH" 2>/dev/null || printf 'UNREADABLE')"
  "$JQ" -n \
    --arg path "$PKG_PATH" \
    --argjson expectedSize "$PKG_SIZE" \
    --argjson actualSize "${local_size:-0}" \
    --arg expectedSha256 "$PKG_SHA" \
    --arg actualSha256 "$local_sha" \
    '{path:$path,expectedSize:$expectedSize,actualSize:$actualSize,expectedSha256:$expectedSha256,actualSha256:$actualSha256}' \
    >"$RUN_DIR/installer-identity.json"

  if [ "$release_metadata_ok" -eq 1 ] && [ "$local_size" = "$PKG_SIZE" ] && [ "$local_sha" = "$PKG_SHA" ]; then
    record_result installer_asset_is_exact PASS "unique GitHub asset metadata, local size, and independently computed SHA-256 all agree" "installer-identity.json"
  else
    record_result installer_asset_is_exact FAIL "release authority, installer size, or local SHA-256 differs from the locked asset" "installer-identity.json"
  fi
}

test_installer_signature_is_trusted() {
  if [ ! -f "$PKG_PATH" ]; then
    record_result installer_signature_is_trusted BLOCKED "signature cannot be assessed because the installer asset is absent" "$PKG_PATH"
    return
  fi

  capture pkg-signature /usr/sbin/pkgutil --check-signature "$PKG_PATH"
  pkg_sig_rc="$(captured_rc pkg-signature)"
  cat "$RAW_DIR/pkg-signature.stdout" "$RAW_DIR/pkg-signature.stderr" >"$RAW_DIR/pkg-signature.combined"
  compact_signature="$(tr -d '[:space:]:' <"$RAW_DIR/pkg-signature.combined" | tr '[:lower:]' '[:upper:]')"

  capture gatekeeper-raw /usr/sbin/spctl \
    --assess --type install --ignore-cache --no-cache --raw "$PKG_PATH"
  gatekeeper_rc="$(captured_rc gatekeeper-raw)"
  gatekeeper_verdict=""
  gatekeeper_authority=""
  if [ "$gatekeeper_rc" -eq 0 ]; then
    gatekeeper_verdict="$(/usr/bin/plutil -extract 'assessment:verdict' raw -o - "$RAW_DIR/gatekeeper-raw.stdout" 2>/dev/null || printf '')"
    gatekeeper_authority="$(/usr/bin/plutil -extract 'assessment:authority.assessment:authority:source' raw -o - "$RAW_DIR/gatekeeper-raw.stdout" 2>/dev/null || printf '')"
  fi

  signature_ok=1
  [ "$pkg_sig_rc" -eq 0 ] || signature_ok=0
  grep -Fq 'Status: signed by a developer certificate issued by Apple for distribution' "$RAW_DIR/pkg-signature.combined" || signature_ok=0
  grep -Fq 'Signed with a trusted timestamp' "$RAW_DIR/pkg-signature.combined" || signature_ok=0
  grep -Fq "$SIGNER_CN" "$RAW_DIR/pkg-signature.combined" || signature_ok=0
  grep -Fq "$SIGNER_INTERMEDIATE" "$RAW_DIR/pkg-signature.combined" || signature_ok=0
  grep -Fq "$SIGNER_ROOT" "$RAW_DIR/pkg-signature.combined" || signature_ok=0
  case "$compact_signature" in
    *"$SIGNER_FP"*) ;;
    *) signature_ok=0 ;;
  esac
  [ "$gatekeeper_rc" -eq 0 ] || signature_ok=0
  [ "$gatekeeper_verdict" = "true" ] || signature_ok=0
  [ "$gatekeeper_authority" = "Notarized Developer ID" ] || signature_ok=0

  "$JQ" -n \
    --argjson pkgutilExit "$pkg_sig_rc" \
    --argjson gatekeeperExit "$gatekeeper_rc" \
    --arg gatekeeperVerdict "$gatekeeper_verdict" \
    --arg gatekeeperAuthority "$gatekeeper_authority" \
    '{pkgutilExit:$pkgutilExit,gatekeeperExit:$gatekeeperExit,gatekeeperVerdict:$gatekeeperVerdict,gatekeeperAuthority:$gatekeeperAuthority}' \
    >"$RUN_DIR/installer-trust.json"

  if [ "$signature_ok" -eq 1 ]; then
    record_result installer_signature_is_trusted PASS "package trust chain, timestamp, leaf fingerprint, and Gatekeeper verdict all match" "installer-trust.json"
  else
    record_result installer_signature_is_trusted FAIL "one or more independent package trust assertions failed" "installer-trust.json"
  fi
}

test_receipt_and_payload_are_exact() {
  receipt_ok=1
  capture package-receipt /usr/sbin/pkgutil --pkg-info-plist "$PKG_ID" || receipt_ok=0
  if [ "$receipt_ok" -eq 1 ]; then
    /usr/bin/plutil -convert json -o "$RAW_DIR/package-receipt.json" "$RAW_DIR/package-receipt.stdout" >/dev/null 2>&1 || receipt_ok=0
  fi
  [ -x "$CONTAINER_BIN" ] || receipt_ok=0
  if [ -f "$CONTAINER_BIN" ]; then
    actual_container_sha="$(sha256_file "$CONTAINER_BIN" 2>/dev/null || printf 'UNREADABLE')"
  else
    actual_container_sha="ABSENT"
  fi
  if [ "$receipt_ok" -eq 1 ]; then
    "$JQ" -e --arg id "$PKG_ID" --arg version "$VERSION" '
      .pkgid == $id and .["pkg-version"] == $version and .volume == "/"
    ' "$RAW_DIR/package-receipt.json" >/dev/null 2>&1 || receipt_ok=0
  fi
  [ "$actual_container_sha" = "$CONTAINER_SHA" ] || receipt_ok=0
  capture package-files /usr/sbin/pkgutil --files "$PKG_ID" || receipt_ok=0
  grep -Fxq 'bin/container' "$RAW_DIR/package-files.stdout" || receipt_ok=0
  grep -Fxq 'bin/uninstall-container.sh' "$RAW_DIR/package-files.stdout" || receipt_ok=0

  "$JQ" -n \
    --arg cliPath "$CONTAINER_BIN" \
    --arg expectedCliSha256 "$CONTAINER_SHA" \
    --arg actualCliSha256 "$actual_container_sha" \
    '{cliPath:$cliPath,expectedCliSha256:$expectedCliSha256,actualCliSha256:$actualCliSha256}' \
    >"$RUN_DIR/payload-identity.json"

  if [ "$receipt_ok" -eq 1 ]; then
    record_result receipt_and_payload_are_exact PASS "receipt identity and installed CLI hash match the signed package contract" "payload-identity.json"
  else
    record_result receipt_and_payload_are_exact FAIL "receipt is absent/wrong or installed CLI payload differs from the contract" "payload-identity.json"
  fi
}

test_recovery_path_is_available() {
  recovery_ok=1
  [ -x "$UNINSTALLER" ] || recovery_ok=0
  if [ -f "$UNINSTALLER" ]; then
    actual_uninstaller_sha="$(sha256_file "$UNINSTALLER" 2>/dev/null || printf 'UNREADABLE')"
  else
    actual_uninstaller_sha="ABSENT"
  fi
  [ "$actual_uninstaller_sha" = "$UNINSTALLER_SHA" ] || recovery_ok=0

  "$JQ" -n \
    --arg path "$UNINSTALLER" \
    --arg expectedSha256 "$UNINSTALLER_SHA" \
    --arg actualSha256 "$actual_uninstaller_sha" \
    --arg keepDataCommand "$UNINSTALLER -k" \
    --arg destructiveCommand "$UNINSTALLER -d" \
    '{path:$path,expectedSha256:$expectedSha256,actualSha256:$actualSha256,keepDataCommand:$keepDataCommand,destructiveCommand:$destructiveCommand,wholeScriptMustNotRunUnderSudo:true}' \
    >"$RUN_DIR/recovery-path.json"

  if [ "$recovery_ok" -eq 1 ]; then
    record_result recovery_path_is_available PASS "the signed-package recovery script is installed with its exact hash" "recovery-path.json"
  else
    record_result recovery_path_is_available FAIL "the recovery script is absent, non-executable, or differs from the signed payload" "recovery-path.json"
  fi
}

test_runtime_version_is_exact() {
  if ! runtime_ready; then
    record_result runtime_version_is_exact BLOCKED "runtime version cannot be verified until the exact CLI and service are available" "raw/runtime-ready-status.stderr"
    return
  fi

  capture system-version "$CONTAINER_BIN" system version --format json
  version_rc="$(captured_rc system-version)"
  if [ "$version_rc" -eq 0 ] && "$JQ" -e \
    --arg version "$VERSION" --arg commit "$COMMIT_SHA" '
      type == "array" and length == 2 and
      ([.[].appName] | sort) == ["container", "container-apiserver"] and
      (map(select(.appName == "container")) | length == 1) and
      (map(select(.appName == "container-apiserver")) | length == 1) and
      (map(select(.appName == "container"))[0] |
        .version == $version and .buildType == "release" and .commit == $commit) and
      (map(select(.appName == "container-apiserver"))[0] |
        .buildType == "release" and .commit == $commit and
        .version == ("container-apiserver version " + $version + " (build: release, commit: " + $commit[0:7] + ")"))
    ' "$RAW_DIR/system-version.stdout" >/dev/null 2>&1; then
    record_result runtime_version_is_exact PASS "CLI and API server report the exact release commit and release build" "raw/system-version.stdout"
  else
    record_result runtime_version_is_exact FAIL "CLI/API version JSON does not exactly match the signed tag target" "raw/system-version.stdout"
  fi
}

test_kernel_configuration_is_exact() {
  if ! runtime_ready; then
    record_result kernel_configuration_is_exact BLOCKED "kernel properties cannot be verified until the service is running" "raw/runtime-ready-status.stderr"
    return
  fi

  capture system-properties "$CONTAINER_BIN" system property list --format json
  property_rc="$(captured_rc system-properties)"
  if [ "$property_rc" -eq 0 ] && "$JQ" -e \
    --arg url "$KERNEL_URL" --arg digest "$KERNEL_DIGEST" --arg binary "$KERNEL_BINARY" \
    --arg vminit "$VMINIT_IMAGE" --arg builder "$BUILDER_IMAGE" '
      .kernel.url == $url and .kernel.digest == $digest and .kernel.binaryPath == $binary and
      .vminit.image == $vminit and .build.image == $builder
    ' "$RAW_DIR/system-properties.stdout" >/dev/null 2>&1; then
    record_result kernel_configuration_is_exact PASS "runtime kernel URL, digest, and selected binary are exact" "raw/system-properties.stdout"
  else
    record_result kernel_configuration_is_exact FAIL "runtime kernel selection differs from the locked bootstrap kernel" "raw/system-properties.stdout"
  fi
}

capture_resource_state() {
  state_prefix="$1"
  capture "$state_prefix-containers" "$CONTAINER_BIN" list --all --format json || return 1
  capture "$state_prefix-networks" "$CONTAINER_BIN" network list --format json || return 1
  capture "$state_prefix-volumes" "$CONTAINER_BIN" volume list --format json || return 1
  capture "$state_prefix-images" "$CONTAINER_BIN" image list --format json --verbose || return 1
  capture "$state_prefix-df" "$CONTAINER_BIN" system df --format json || return 1

  "$JQ" -e 'type == "array"' "$RAW_DIR/$state_prefix-containers.stdout" >/dev/null 2>&1 || return 1
  "$JQ" -e 'type == "array"' "$RAW_DIR/$state_prefix-networks.stdout" >/dev/null 2>&1 || return 1
  "$JQ" -e 'type == "array"' "$RAW_DIR/$state_prefix-volumes.stdout" >/dev/null 2>&1 || return 1
  "$JQ" -e 'type == "array"' "$RAW_DIR/$state_prefix-images.stdout" >/dev/null 2>&1 || return 1
  "$JQ" -e '
    type == "object" and
    (["images","containers","volumes"] - keys | length == 0) and
    all(.images,.containers,.volumes;
      (.total|type)=="number" and (.active|type)=="number" and
      (.sizeInBytes|type)=="number" and (.reclaimable|type)=="number")
  ' "$RAW_DIR/$state_prefix-df.stdout" >/dev/null 2>&1 || return 1
}

test_foreign_mutable_resources_are_absent() {
  if ! runtime_ready; then
    record_result foreign_mutable_resources_are_absent BLOCKED "resource precondition cannot be inspected until the service is running" "raw/runtime-ready-status.stderr"
    return
  fi
  if ! capture_resource_state baseline; then
    record_result foreign_mutable_resources_are_absent FAIL "one or more resource inventories failed or returned invalid JSON" "raw/baseline-containers.stderr"
    return
  fi

  manifest_tree "$APP_ROOT" "$RUN_DIR/app-root-before.json"
  BASELINE_CAPTURED=1

  containers_count="$($JQ 'length' "$RAW_DIR/baseline-containers.stdout")"
  networks_count="$($JQ 'length' "$RAW_DIR/baseline-networks.stdout")"
  volumes_count="$($JQ 'length' "$RAW_DIR/baseline-volumes.stdout")"
  foreign_images="$($JQ --arg digest "$IMAGE_DIGEST" '
    [ .[] | select(
      ((.configuration.descriptor.digest // "") != $digest) and
      (all(.variants[]?; (.digest // "") != $digest))
    ) ] | length
  ' "$RAW_DIR/baseline-images.stdout")"
  if "$JQ" -e --arg digest "$IMAGE_DIGEST" '
      any(.[]; (.configuration.descriptor.digest // "") == $digest or any(.variants[]?; (.digest // "") == $digest))
    ' "$RAW_DIR/baseline-images.stdout" >/dev/null 2>&1; then
    PROBE_IMAGE_PREEXISTED=1
  fi

  builtin_network_ok=0
  if [ "$networks_count" -eq 1 ] && "$JQ" -e '
      .[0].id == "default" and .[0].configuration.name == "default" and
      .[0].configuration.mode == "nat" and
      .[0].configuration.plugin == "container-network-vmnet" and
      .[0].configuration.labels["com.apple.container.resource.role"] == "builtin"
    ' "$RAW_DIR/baseline-networks.stdout" >/dev/null 2>&1; then
    builtin_network_ok=1
  fi

  if [ "$containers_count" -eq 0 ] && [ "$builtin_network_ok" -eq 1 ] && \
     [ "$volumes_count" -eq 0 ] && [ "$foreign_images" -eq 0 ]; then
    FOREIGN_RESOURCES_PASS=1
    record_result foreign_mutable_resources_are_absent PASS "no foreign containers, user networks, volumes, or workload images can be disrupted" "raw/baseline-containers.stdout"
  else
    record_result foreign_mutable_resources_are_absent FAIL "foreign mutable resources exist; lifecycle mutation is refused" "raw/baseline-containers.stdout"
  fi
}

status_matches_locked_roots() {
  status_file="$1"
  "$JQ" -e --arg app "$APP_ROOT" --arg install "$INSTALL_ROOT" --arg commit "$COMMIT_SHA" '
    .status == "running" and .appRoot == $app and .installRoot == $install and
    .apiServerBuild == "release" and .apiServerCommit == $commit and
    .apiServerAppName == "container-apiserver"
  ' "$status_file" >/dev/null 2>&1
}

stopped_status_is_valid() {
  status_file="$1"
  "$JQ" -e '.status == "unregistered" or .status == "not running"' "$status_file" >/dev/null 2>&1
}

test_system_lifecycle_is_idempotent() {
  if [ "$FOREIGN_RESOURCES_PASS" -ne 1 ]; then
    record_result system_lifecycle_is_idempotent BLOCKED "system lifecycle mutation requires a clean foreign-resource precondition" "raw/baseline-containers.stdout"
    return
  fi

  lifecycle_ok=1
  capture system-stop-first "$CONTAINER_BIN" system stop || lifecycle_ok=0
  capture system-status-stopped-first "$CONTAINER_BIN" system status --format json || true
  stopped_status_is_valid "$RAW_DIR/system-status-stopped-first.stdout" || lifecycle_ok=0

  capture system-stop-second "$CONTAINER_BIN" system stop || lifecycle_ok=0
  capture system-status-stopped-second "$CONTAINER_BIN" system status --format json || true
  stopped_status_is_valid "$RAW_DIR/system-status-stopped-second.stdout" || lifecycle_ok=0

  capture system-start-first "$CONTAINER_BIN" system start --disable-kernel-install --timeout 60 || lifecycle_ok=0
  capture system-status-started-first "$CONTAINER_BIN" system status --format json || lifecycle_ok=0
  status_matches_locked_roots "$RAW_DIR/system-status-started-first.stdout" || lifecycle_ok=0

  capture system-start-second "$CONTAINER_BIN" system start --disable-kernel-install --timeout 60 || lifecycle_ok=0
  capture system-status-started-second "$CONTAINER_BIN" system status --format json || lifecycle_ok=0
  status_matches_locked_roots "$RAW_DIR/system-status-started-second.stdout" || lifecycle_ok=0

  if [ "$lifecycle_ok" -eq 1 ]; then
    record_result system_lifecycle_is_idempotent PASS "two stops converge to stopped and two starts converge to the exact running service" "raw/system-status-started-second.stdout"
  else
    record_result system_lifecycle_is_idempotent FAIL "repeated system stop/start did not preserve exit and state invariants" "raw/system-start-second.stderr"
  fi
}

test_probe_image_is_exact_arm64() {
  if ! runtime_ready; then
    record_result probe_image_is_exact_arm64 BLOCKED "exact image cannot run until the service is healthy" "raw/runtime-ready-status.stderr"
    return
  fi

  probe_ok=1
  capture image-pull "$CONTAINER_BIN" image pull --platform "$PLATFORM" --progress plain "$IMAGE" || probe_ok=0
  capture image-inspect "$CONTAINER_BIN" image inspect "$IMAGE" || probe_ok=0
  capture image-list-after-pull "$CONTAINER_BIN" image list --format json --verbose || probe_ok=0
  if [ -s "$RAW_DIR/image-inspect.stdout" ]; then
    grep -Fq "$IMAGE_DIGEST" "$RAW_DIR/image-inspect.stdout" || probe_ok=0
  else
    probe_ok=0
  fi
  "$JQ" -e --arg digest "$IMAGE_DIGEST" '
      type == "array" and length > 0 and
      any(.[]; any(.variants[]?;
        .digest == $digest and .platform.os == "linux" and
        .platform.architecture == "arm64"))
    ' "$RAW_DIR/image-list-after-pull.stdout" >/dev/null 2>&1 || probe_ok=0
  capture probe-run "$CONTAINER_BIN" run --rm --name "$PROBE_NAME" --platform "$PLATFORM" "$IMAGE" \
    sh -eu -c 'uname -s; uname -m; cat /etc/alpine-release'
  [ "$(captured_rc probe-run)" -eq 0 ] || probe_ok=0
  [ "$(sed -n '1p' "$RAW_DIR/probe-run.stdout")" = "Linux" ] || probe_ok=0
  [ "$(sed -n '2p' "$RAW_DIR/probe-run.stdout")" = "aarch64" ] || probe_ok=0
  [ "$(sed -n '3p' "$RAW_DIR/probe-run.stdout")" = "$ALPINE_VERSION" ] || probe_ok=0

  if [ "$probe_ok" -eq 1 ]; then
    record_result probe_image_is_exact_arm64 PASS "digest-pinned image executes as Linux/aarch64 with the locked userspace version" "raw/probe-run.stdout"
  else
    record_result probe_image_is_exact_arm64 FAIL "image identity, platform, or guest userspace differs from the contract" "raw/probe-run.stdout"
  fi
}

container_identity() {
  identity_name="$1"
  identity_label="$2"
  identity_nonce="$3"
  capture "$identity_label" "$CONTAINER_BIN" exec "$identity_name" \
    sh -eu -c 'printf "%s\n" "$1"; uname -s; uname -m; cat /proc/sys/kernel/random/boot_id' sh "$identity_nonce"
}

test_two_vms_are_independent() {
  if ! runtime_ready; then
    record_result two_vms_are_independent BLOCKED "VM independence cannot be tested until the runtime is healthy" "raw/runtime-ready-status.stderr"
    return
  fi

  vm_ok=1
  capture vm-a-run "$CONTAINER_BIN" run --detach --name "$VM_A" --platform "$PLATFORM" "$IMAGE" sleep 300 || vm_ok=0
  capture vm-b-run "$CONTAINER_BIN" run --detach --name "$VM_B" --platform "$PLATFORM" "$IMAGE" sleep 300 || vm_ok=0

  identity_attempt=0
  while [ "$identity_attempt" -lt 20 ]; do
    if container_identity "$VM_A" vm-a-identity "$VM_A_NONCE" && \
       container_identity "$VM_B" vm-b-identity "$VM_B_NONCE"; then
      break
    fi
    identity_attempt=$((identity_attempt + 1))
    sleep 1
  done
  [ "$identity_attempt" -lt 20 ] || vm_ok=0
  container_identity "$VM_A" vm-a-identity-repeat "$VM_A_NONCE" || vm_ok=0
  container_identity "$VM_B" vm-b-identity-repeat "$VM_B_NONCE" || vm_ok=0

  capture vm-running-inventory "$CONTAINER_BIN" list --all --format json || vm_ok=0
  capture vm-a-boot-log "$CONTAINER_BIN" logs --boot "$VM_A" || vm_ok=0
  capture vm-b-boot-log "$CONTAINER_BIN" logs --boot "$VM_B" || vm_ok=0

  a_boot="$(sed -n '4p' "$RAW_DIR/vm-a-identity.stdout" 2>/dev/null || printf '')"
  b_boot="$(sed -n '4p' "$RAW_DIR/vm-b-identity.stdout" 2>/dev/null || printf '')"
  [ "$(sed -n '1p' "$RAW_DIR/vm-a-identity.stdout" 2>/dev/null || printf '')" = "$VM_A_NONCE" ] || vm_ok=0
  [ "$(sed -n '2p' "$RAW_DIR/vm-a-identity.stdout" 2>/dev/null || printf '')" = "Linux" ] || vm_ok=0
  [ "$(sed -n '3p' "$RAW_DIR/vm-a-identity.stdout" 2>/dev/null || printf '')" = "aarch64" ] || vm_ok=0
  [ "$(sed -n '1p' "$RAW_DIR/vm-b-identity.stdout" 2>/dev/null || printf '')" = "$VM_B_NONCE" ] || vm_ok=0
  [ "$(sed -n '2p' "$RAW_DIR/vm-b-identity.stdout" 2>/dev/null || printf '')" = "Linux" ] || vm_ok=0
  [ "$(sed -n '3p' "$RAW_DIR/vm-b-identity.stdout" 2>/dev/null || printf '')" = "aarch64" ] || vm_ok=0
  [ "$(sed -n '4p' "$RAW_DIR/vm-a-identity-repeat.stdout" 2>/dev/null || printf '')" = "$a_boot" ] || vm_ok=0
  [ "$(sed -n '4p' "$RAW_DIR/vm-b-identity-repeat.stdout" 2>/dev/null || printf '')" = "$b_boot" ] || vm_ok=0
  printf '%s\n' "$a_boot" | grep -Eq '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' || vm_ok=0
  printf '%s\n' "$b_boot" | grep -Eq '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' || vm_ok=0
  [ "$a_boot" != "$b_boot" ] || vm_ok=0

  "$JQ" -e --arg a "$VM_A" --arg b "$VM_B" --arg image "$IMAGE" '
    (map(select(.id == $a and .status.state == "running")) | length == 1) and
    (map(select(.id == $b and .status.state == "running")) | length == 1) and
    all(.[] | select(.id == $a or .id == $b);
      .configuration.platform.os == "linux" and
      .configuration.platform.architecture == "arm64" and
      .configuration.image.reference == $image and
      .configuration.mounts == [] and
      .configuration.ssh == false and
      .configuration.rosetta == false)
  ' "$RAW_DIR/vm-running-inventory.stdout" >/dev/null 2>&1 || vm_ok=0

  # Reboot only A, twice exercising idempotent stop/start. B must remain running
  # with the same boot ID throughout, proving cross-VM non-interference.
  capture vm-a-stop-first "$CONTAINER_BIN" stop "$VM_A" || vm_ok=0
  capture vm-a-stop-second "$CONTAINER_BIN" stop "$VM_A" || vm_ok=0
  container_identity "$VM_B" vm-b-after-a-stop "$VM_B_CHECK_NONCE" || vm_ok=0
  [ "$(sed -n '4p' "$RAW_DIR/vm-b-after-a-stop.stdout" 2>/dev/null || printf '')" = "$b_boot" ] || vm_ok=0

  capture vm-a-start-first "$CONTAINER_BIN" start "$VM_A" || vm_ok=0
  capture vm-a-start-second "$CONTAINER_BIN" start "$VM_A" || vm_ok=0
  identity_attempt=0
  while [ "$identity_attempt" -lt 20 ]; do
    if container_identity "$VM_A" vm-a-restarted-identity "$VM_A_RESTART_NONCE" && \
       container_identity "$VM_B" vm-b-after-a-restart-identity "$VM_B_CHECK_NONCE"; then
      break
    fi
    identity_attempt=$((identity_attempt + 1))
    sleep 1
  done
  [ "$identity_attempt" -lt 20 ] || vm_ok=0

  a_restarted_boot="$(sed -n '4p' "$RAW_DIR/vm-a-restarted-identity.stdout" 2>/dev/null || printf '')"
  b_after_restart="$(sed -n '4p' "$RAW_DIR/vm-b-after-a-restart-identity.stdout" 2>/dev/null || printf '')"
  [ "$a_restarted_boot" != "$a_boot" ] || vm_ok=0
  [ "$a_restarted_boot" != "$b_boot" ] || vm_ok=0
  [ "$b_after_restart" = "$b_boot" ] || vm_ok=0

  capture vm-delete "$CONTAINER_BIN" delete --force "$VM_A" "$VM_B" || vm_ok=0

  "$JQ" -n \
    --arg vmA "$VM_A" --arg vmB "$VM_B" \
    --arg vmANonce "$VM_A_NONCE" --arg vmBNonce "$VM_B_NONCE" \
    --arg vmABootBefore "$a_boot" --arg vmABootAfter "$a_restarted_boot" \
    --arg vmBBoot "$b_boot" --arg vmBBootAfter "$b_after_restart" \
    '{vmA:$vmA,vmB:$vmB,vmANonce:$vmANonce,vmBNonce:$vmBNonce,vmABootBefore:$vmABootBefore,vmABootAfter:$vmABootAfter,vmBBoot:$vmBBoot,vmBBootAfter:$vmBBootAfter}' \
    >"$RUN_DIR/vm-independence.json"

  if [ "$vm_ok" -eq 1 ]; then
    record_result two_vms_are_independent PASS "concurrent VMs have distinct kernels; rebooting A leaves B unchanged; stop/start is idempotent" "vm-independence.json"
  else
    record_result two_vms_are_independent FAIL "VM identity, concurrent state, lifecycle, or cross-VM isolation invariant failed" "vm-independence.json"
  fi
}

test_default_host_data_is_not_exposed() {
  if ! runtime_ready; then
    record_result default_host_data_is_not_exposed BLOCKED "default mount behavior cannot be tested until the runtime is healthy" "raw/runtime-ready-status.stderr"
    return
  fi

  canary_value="c30-public-tmp-sentinel-${RUN_TOKEN}"
  workspace_canary_value="c30-public-workspace-sentinel-${RUN_TOKEN}"
  printf '%s\n' "$canary_value" >"$HOST_CANARY"
  printf '%s\n' "$workspace_canary_value" >"$WORKSPACE_CANARY"
  host_canary_sha="$(sha256_file "$HOST_CANARY")"
  workspace_canary_sha="$(sha256_file "$WORKSPACE_CANARY")"
  exposure_ok=1
  capture default-exposure-run env C30_HOST_ONLY_SENTINEL="$canary_value" \
    "$CONTAINER_BIN" run --detach --name "$EXPOSURE_NAME" --platform "$PLATFORM" "$IMAGE" sleep 180 || exposure_ok=0
  capture default-exposure-inspect "$CONTAINER_BIN" inspect "$EXPOSURE_NAME" || exposure_ok=0
  "$JQ" -e --arg name "$EXPOSURE_NAME" '
      (if type == "array" then .[0] else . end) |
      .id == $name and .configuration.mounts == [] and .configuration.ssh == false and
      .configuration.publishedSockets == [] and .configuration.platform.os == "linux" and
      .configuration.platform.architecture == "arm64"
    ' "$RAW_DIR/default-exposure-inspect.stdout" >/dev/null 2>&1 || exposure_ok=0

  capture default-exposure "$TIMEOUT_BIN" 45 env C30_HOST_ONLY_SENTINEL="$canary_value" \
    "$CONTAINER_BIN" exec "$EXPOSURE_NAME" \
    sh -eu -c '
      tmp_canary="$1"
      workspace_canary="$2"
      host_home="$3"
      workspace="$4"
      tmp_sha="$5"
      workspace_sha="$6"
      [ ! -e "$tmp_canary" ]
      [ ! -e "$workspace_canary" ]
      [ ! -e "$host_home" ]
      [ ! -e "$workspace" ]
      [ ! -e /var/host-services/ssh-auth.sock ]
      ! env | grep -q "^C30_HOST_ONLY_SENTINEL="
      ! env | grep -q "^SSH_AUTH_SOCK="
      ! grep -Fq "$tmp_canary" /proc/self/mountinfo
      ! grep -Fq "$workspace_canary" /proc/self/mountinfo
      find / -xdev \
        \( -path /proc -o -path /sys -o -path /dev \) -prune -o \
        -type f -size -2097152c -exec sha256sum {} + > /tmp/c30-guest-hashes
      ! grep -Eq "^(${tmp_sha}|${workspace_sha})  " /tmp/c30-guest-hashes
      printf "HOST_DATA_ABSENT\n"
      cat /proc/self/mountinfo
    ' sh "$HOST_CANARY" "$WORKSPACE_CANARY" "$HOME" "$PROJECT_ROOT" \
      "$host_canary_sha" "$workspace_canary_sha" || exposure_ok=0
  grep -Fxq 'HOST_DATA_ABSENT' "$RAW_DIR/default-exposure.stdout" || exposure_ok=0
  capture default-exposure-delete "$CONTAINER_BIN" delete --force "$EXPOSURE_NAME" || exposure_ok=0
  /bin/rm -f -- "$HOST_CANARY"
  /bin/rm -f -- "$WORKSPACE_CANARY"

  if [ "$exposure_ok" -eq 1 ]; then
    record_result default_host_data_is_not_exposed PASS "no default host canary, home/workspace path, host-only env, or SSH agent exposure was observed" "raw/default-exposure.stdout"
  else
    record_result default_host_data_is_not_exposed FAIL "one or more default user-controlled host-data surfaces were visible" "raw/default-exposure.stdout"
  fi
}

test_default_network_has_outbound_connectivity() {
  if ! runtime_ready; then
    record_result default_network_has_outbound_connectivity BLOCKED "default network cannot be measured until the runtime is healthy" "raw/runtime-ready-status.stderr"
    return
  fi

  capture default-network "$CONTAINER_BIN" run --rm --name "$DEFAULT_NET_NAME" --platform "$PLATFORM" "$IMAGE" \
    sh -eu -c '
      url="$1"
      timeout_seconds="$2"
      expected="$3"
      printf "%s\n" "--- route ---"
      ip route
      printf "%s\n" "--- resolv.conf ---"
      cat /etc/resolv.conf
      wget -q -T "$timeout_seconds" -O /tmp/c30-network-body "$url"
      test -s /tmp/c30-network-body
      grep -Fq "$expected" /tmp/c30-network-body
      wc -c /tmp/c30-network-body
      sha256sum /tmp/c30-network-body
    ' sh "$NETWORK_URL" "$NETWORK_TIMEOUT" "$NETWORK_TEXT"
  default_network_rc="$(captured_rc default-network)"

  if [ "$default_network_rc" -eq 0 ]; then
    DEFAULT_NETWORK_PASS=1
    record_result default_network_has_outbound_connectivity PASS "default network resolved and fetched the locked external probe" "raw/default-network.stdout"
  else
    record_result default_network_has_outbound_connectivity FAIL "default network could not complete the bounded external fetch" "raw/default-network.stderr"
  fi
}

test_internal_network_blocks_outbound() {
  if [ "$DEFAULT_NETWORK_PASS" -ne 1 ]; then
    record_result internal_network_blocks_outbound BLOCKED "an internal-network denial is not attributable until the same external probe succeeds on default networking" "raw/default-network.stderr"
    return
  fi

  internal_ok=1
  capture internal-network-create "$CONTAINER_BIN" network create --internal "$INTERNAL_NAME" || internal_ok=0
  capture internal-network-inspect "$CONTAINER_BIN" network inspect "$INTERNAL_NAME" || internal_ok=0
  if [ -s "$RAW_DIR/internal-network-inspect.stdout" ]; then
    "$JQ" -e --arg name "$INTERNAL_NAME" '
      (if type == "array" then .[0] else . end) |
      .id == $name and .configuration.name == $name and
      .configuration.mode == "hostOnly" and .configuration.plugin == "container-network-vmnet"
    ' "$RAW_DIR/internal-network-inspect.stdout" >/dev/null 2>&1 || internal_ok=0
  else
    internal_ok=0
  fi

  internal_gateway="$($JQ -r '
      (if type == "array" then .[0] else . end) |
      .status.ipv4Gateway // ""
    ' "$RAW_DIR/internal-network-inspect.stdout" 2>/dev/null || printf '')"
  [ -n "$internal_gateway" ] || internal_ok=0

  capture internal-network-run "$CONTAINER_BIN" run --rm --name "$INTERNAL_RUN_NAME" \
    --platform "$PLATFORM" --network "$INTERNAL_NAME" "$IMAGE" \
    sh -eu -c '
      url="$1"
      timeout_seconds="$2"
      gateway="$3"
      printf "%s\n" "--- route ---"
      ip route
      printf "%s\n" "--- resolv.conf ---"
      cat /etc/resolv.conf
      nc -z -w 3 "$gateway" 53
      printf "HOST_GATEWAY_REACHABLE\n"
      if wget -q -T "$timeout_seconds" -O /tmp/c30-internal-body "$url"; then
        printf "UNEXPECTED_OUTBOUND_SUCCESS\n"
        exit 91
      fi
      printf "OUTBOUND_BLOCKED\n"
    ' sh "$NETWORK_URL" "$NETWORK_TIMEOUT" "$internal_gateway" || internal_ok=0
  grep -Fxq 'HOST_GATEWAY_REACHABLE' "$RAW_DIR/internal-network-run.stdout" || internal_ok=0
  grep -Fxq 'OUTBOUND_BLOCKED' "$RAW_DIR/internal-network-run.stdout" || internal_ok=0
  grep -Fq 'UNEXPECTED_OUTBOUND_SUCCESS' "$RAW_DIR/internal-network-run.stdout" && internal_ok=0
  capture internal-network-delete "$CONTAINER_BIN" network delete "$INTERNAL_NAME" || internal_ok=0

  # Repeat the identical default-network probe after the denial. This sandwich
  # prevents a host/Internet outage from being misclassified as isolation.
  capture default-network-after "$CONTAINER_BIN" run --rm --name "$DEFAULT_AFTER_NAME" \
    --platform "$PLATFORM" "$IMAGE" \
    sh -eu -c '
      url="$1"
      timeout_seconds="$2"
      expected="$3"
      wget -q -T "$timeout_seconds" -O /tmp/c30-network-body "$url"
      test -s /tmp/c30-network-body
      grep -Fq "$expected" /tmp/c30-network-body
      printf "DEFAULT_OUTBOUND_RECONFIRMED\n"
    ' sh "$NETWORK_URL" "$NETWORK_TIMEOUT" "$NETWORK_TEXT" || internal_ok=0
  grep -Fxq 'DEFAULT_OUTBOUND_RECONFIRMED' "$RAW_DIR/default-network-after.stdout" || internal_ok=0

  if [ "$internal_ok" -eq 1 ]; then
    record_result internal_network_blocks_outbound PASS "default-before/host-gateway/internal-deny/default-after differential is complete" "raw/internal-network-run.stdout"
  else
    record_result internal_network_blocks_outbound FAIL "internal network mode, outbound denial, or exact cleanup failed" "raw/internal-network-run.stdout"
  fi
}

resource_id_set_equal() {
  before_file="$1"
  after_file="$2"
  "$JQ" -S '[.[].id] | sort' "$before_file" >"$RAW_DIR/resource-before-ids.tmp"
  "$JQ" -S '[.[].id] | sort' "$after_file" >"$RAW_DIR/resource-after-ids.tmp"
  cmp -s "$RAW_DIR/resource-before-ids.tmp" "$RAW_DIR/resource-after-ids.tmp"
}

test_cleanup_has_no_test_residue() {
  if [ "$BASELINE_CAPTURED" -ne 1 ] || ! runtime_ready; then
    record_result cleanup_has_no_test_residue BLOCKED "cleanup requires valid before/after resource inventories around an operational runtime" "raw/runtime-ready-status.stderr"
    return
  fi

  exact_cleanup
  cleanup_ok=1
  if [ "$PROBE_IMAGE_PREEXISTED" -eq 0 ]; then
    capture probe-image-delete "$CONTAINER_BIN" image delete "$IMAGE" || cleanup_ok=0
  fi

  capture_resource_state final || cleanup_ok=0
  manifest_tree "$APP_ROOT" "$RUN_DIR/app-root-after.json"

  resource_id_set_equal "$RAW_DIR/baseline-containers.stdout" "$RAW_DIR/final-containers.stdout" || cleanup_ok=0
  resource_id_set_equal "$RAW_DIR/baseline-networks.stdout" "$RAW_DIR/final-networks.stdout" || cleanup_ok=0
  resource_id_set_equal "$RAW_DIR/baseline-volumes.stdout" "$RAW_DIR/final-volumes.stdout" || cleanup_ok=0
  resource_id_set_equal "$RAW_DIR/baseline-images.stdout" "$RAW_DIR/final-images.stdout" || cleanup_ok=0

  "$JQ" -S '{images:{total:.images.total,active:.images.active,sizeInBytes:.images.sizeInBytes},containers:{total:.containers.total,active:.containers.active,sizeInBytes:.containers.sizeInBytes},volumes:{total:.volumes.total,active:.volumes.active,sizeInBytes:.volumes.sizeInBytes}}' \
    "$RAW_DIR/baseline-df.stdout" >"$RAW_DIR/baseline-df-stable.json"
  "$JQ" -S '{images:{total:.images.total,active:.images.active,sizeInBytes:.images.sizeInBytes},containers:{total:.containers.total,active:.containers.active,sizeInBytes:.containers.sizeInBytes},volumes:{total:.volumes.total,active:.volumes.active,sizeInBytes:.volumes.sizeInBytes}}' \
    "$RAW_DIR/final-df.stdout" >"$RAW_DIR/final-df-stable.json"
  cmp -s "$RAW_DIR/baseline-df-stable.json" "$RAW_DIR/final-df-stable.json" || cleanup_ok=0

  for final_inventory in final-containers final-networks final-volumes; do
    if grep -Fq "$RESOURCE_PREFIX" "$RAW_DIR/$final_inventory.stdout"; then
      cleanup_ok=0
    fi
  done

  capture host-processes ps -axo pid=,ppid=,user=,etime=,command= || cleanup_ok=0
  if grep -Fq "$RESOURCE_PREFIX" "$RAW_DIR/host-processes.stdout"; then
    cleanup_ok=0
  fi

  "$JQ" -n \
    --slurpfile before "$RUN_DIR/app-root-before.json" \
    --slurpfile after "$RUN_DIR/app-root-after.json" \
    --arg prefix "$RESOURCE_PREFIX" '
      def by_path: map({key:.path,value:.}) | from_entries;
      ($before[0] | by_path) as $b |
      ($after[0] | by_path) as $a |
      (($b|keys) + ($a|keys) | unique) |
      [ .[] as $p |
        select(($b[$p] // null) != ($a[$p] // null)) |
        {path:$p,before:($b[$p] // null),after:($a[$p] // null),
         classification:(if ($p|contains($prefix)) then "TEST_RESOURCE" else "RUNTIME_STATE" end)}
      ]
    ' >"$RUN_DIR/app-root-delta.json"
  if "$JQ" -e 'any(.[]; .classification == "TEST_RESOURCE")' "$RUN_DIR/app-root-delta.json" >/dev/null 2>&1; then
    cleanup_ok=0
  fi

  if [ "$cleanup_ok" -eq 1 ]; then
    record_result cleanup_has_no_test_residue PASS "resource ID sets and stable disk accounting returned to baseline with no owned process or filesystem residue" "app-root-delta.json"
  else
    record_result cleanup_has_no_test_residue FAIL "resource IDs, disk accounting, processes, or app-root inventory did not return to the classified baseline" "app-root-delta.json"
  fi
}

test_host_is_supported
test_contract_is_locked
test_installer_asset_is_exact
test_installer_signature_is_trusted
test_receipt_and_payload_are_exact
test_recovery_path_is_available
test_runtime_version_is_exact
test_kernel_configuration_is_exact
test_foreign_mutable_resources_are_absent
test_system_lifecycle_is_idempotent
test_probe_image_is_exact_arm64
test_two_vms_are_independent
test_default_host_data_is_not_exposed
test_default_network_has_outbound_connectivity
test_internal_network_blocks_outbound
test_cleanup_has_no_test_residue

"$JQ" -s \
  --arg runId "$RUN_ID" \
  --arg contractSha256 "$(awk '{print $1}' "$RUN_DIR/contract.sha256")" \
  --argjson expected "$($JQ -c '.acceptance.requiredTestIds' "$CONTRACT")" '
    . as $results |
    ($results | map(.id)) as $actual |
    ($results | group_by(.id) | map(select(length != 1) | .[0].id)) as $duplicates |
    ($expected - $actual) as $missing |
    ($actual - $expected) as $unexpected |
    {
      runId:$runId,
      contractSha256:$contractSha256,
      results:$results,
      counts:($results | group_by(.status) | map({key:.[0].status,value:length}) | from_entries),
      missingTestIds:$missing,
      unexpectedTestIds:$unexpected,
      duplicateTestIds:$duplicates,
      overall:(if (($missing|length)==0 and ($unexpected|length)==0 and ($duplicates|length)==0 and all($results[]; .status=="PASS")) then "PASS" else "FAIL" end)
    }
  ' "$RESULTS" >"$SUMMARY"

printf 'evidence: %s\n' "$RUN_DIR"
"$JQ" '{overall,counts,missingTestIds,unexpectedTestIds,duplicateTestIds}' "$SUMMARY"

if [ "$("$JQ" -r '.overall' "$SUMMARY")" = "PASS" ]; then
  exit 0
fi
exit 1

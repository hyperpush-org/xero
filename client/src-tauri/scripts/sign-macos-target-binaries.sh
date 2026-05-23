#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  exit 0
fi

script_dir="$(cd "$(dirname "$0")" && pwd)"
tauri_dir="$(cd "$script_dir/.." && pwd)"

cert_path=""
keychain_dir=""
keychain_path=""
original_keychains=()

cleanup() {
  if [ -n "$keychain_path" ]; then
    if [ "${#original_keychains[@]}" -gt 0 ]; then
      security list-keychains -d user -s "${original_keychains[@]}" >/dev/null 2>&1 || true
    fi
    security delete-keychain "$keychain_path" >/dev/null 2>&1 || true
  fi
  if [ -n "$cert_path" ]; then
    rm -f "$cert_path"
  fi
  if [ -n "$keychain_dir" ]; then
    rm -rf "$keychain_dir"
  fi
}
trap cleanup EXIT

run_with_timeout() {
  local seconds="$1"
  shift

  "$@" &
  local command_pid=$!

  (
    sleep "$seconds"
    if kill -0 "$command_pid" 2>/dev/null; then
      echo "Command timed out after ${seconds}s: $*" >&2
      kill "$command_pid" 2>/dev/null || true
    fi
  ) &
  local watchdog_pid=$!

  local status=0
  wait "$command_pid" || status=$?
  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true
  return "$status"
}

while IFS= read -r keychain; do
  keychain="${keychain#\"}"
  keychain="${keychain%\"}"
  original_keychains+=("$keychain")
done < <(security list-keychains -d user | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

configured_identity="${APPLE_SIGNING_IDENTITY:-}"
identity="$configured_identity"
if [ -n "$identity" ] && ! security find-identity -v -p codesigning | grep -Fq "$identity"; then
  echo "Configured macOS signing identity is not available in the keychain; using imported Developer ID identity."
  identity=""
fi

if [ -z "$identity" ]; then
  if [ -n "${APPLE_CERTIFICATE:-}" ]; then
    cert_path="$(mktemp "${TMPDIR:-/tmp}/xero-apple-cert.XXXXXX.p12")"
    keychain_dir="$(mktemp -d "${TMPDIR:-/tmp}/xero-signing-keychain.XXXXXX")"
    keychain_path="$keychain_dir/build.keychain-db"
    keychain_password="$(openssl rand -base64 32)"

    echo "Importing Apple Developer ID certificate into temporary keychain for target helper signing."
    python3 - "$cert_path" <<'PY'
import base64
import os
import sys

with open(sys.argv[1], "wb") as certificate:
    certificate.write(base64.b64decode(os.environ["APPLE_CERTIFICATE"]))
PY

    security create-keychain -p "$keychain_password" "$keychain_path"
    security set-keychain-settings -lut 21600 "$keychain_path"
    security unlock-keychain -p "$keychain_password" "$keychain_path"
    security import "$cert_path" \
      -P "${APPLE_CERTIFICATE_PASSWORD:-}" \
      -A \
      -t cert \
      -f pkcs12 \
      -k "$keychain_path" \
      -T /usr/bin/codesign
    security list-keychains -d user -s "$keychain_path" "${original_keychains[@]}"
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain_path"

    if [ -n "$configured_identity" ]; then
      identity="$(security find-identity -v -p codesigning "$keychain_path" | grep -F "$configured_identity" | sed -n 's/.*"\(Developer ID Application:.*\)".*/\1/p' | head -1 || true)"
    fi
    if [ -z "$identity" ]; then
      identity="$(security find-identity -v -p codesigning "$keychain_path" | sed -n 's/.*"\(Developer ID Application:.*\)".*/\1/p' | head -1)"
    fi
  fi

  if [ -z "$identity" ]; then
    identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)".*/\1/p' | head -1)"
  fi
fi

if [ -z "$identity" ]; then
  echo "No Developer ID Application identity available; skipping target helper signing."
  exit 0
fi

helper_names=(
  xero-harness-evals
  tool-harness
  xero
)
codesign_timeout_seconds="${XERO_CODESIGN_TIMEOUT_SECONDS:-300}"

signed_any=0
while IFS= read -r release_dir; do
  for helper_name in "${helper_names[@]}"; do
    helper_path="$release_dir/$helper_name"
    if [ ! -f "$helper_path" ]; then
      continue
    fi

    echo "Signing target helper binary $helper_path"
    run_with_timeout "$codesign_timeout_seconds" codesign --force --options runtime --timestamp --sign "$identity" "$helper_path"
    signed_any=1
  done
done < <(find "$tauri_dir/target" -type d -path "*/release" 2>/dev/null | sort)

if [ "$signed_any" -eq 0 ]; then
  echo "No target helper binaries found to sign."
fi

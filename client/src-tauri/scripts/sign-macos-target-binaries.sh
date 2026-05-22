#!/usr/bin/env bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  exit 0
fi

script_dir="$(cd "$(dirname "$0")" && pwd)"
tauri_dir="$(cd "$script_dir/.." && pwd)"

identity="${APPLE_SIGNING_IDENTITY:-}"
if [ -n "$identity" ] && ! security find-identity -v -p codesigning | grep -Fq "$identity"; then
  echo "Configured macOS signing identity is not available in the keychain; using imported Developer ID identity."
  identity=""
fi

if [ -z "$identity" ]; then
  if [ -n "${APPLE_CERTIFICATE:-}" ]; then
    cert_path="$(mktemp "${TMPDIR:-/tmp}/xero-apple-cert.XXXXXX.p12")"
    cleanup() {
      rm -f "$cert_path"
    }
    trap cleanup EXIT

    echo "Importing Apple Developer ID certificate for target helper signing."
    python3 - "$cert_path" <<'PY'
import base64
import os
import sys

with open(sys.argv[1], "wb") as certificate:
    certificate.write(base64.b64decode(os.environ["APPLE_CERTIFICATE"]))
PY

    security import "$cert_path" \
      -P "${APPLE_CERTIFICATE_PASSWORD:-}" \
      -A \
      -T /usr/bin/codesign
  fi

  identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)".*/\1/p' | head -1)"
fi

if [ -z "$identity" ]; then
  echo "No Developer ID Application identity available; skipping target helper signing."
  exit 0
fi

helper_names=(
  xero-harness-evals
  tool-harness
  xero-tui
)

signed_any=0
while IFS= read -r release_dir; do
  for helper_name in "${helper_names[@]}"; do
    helper_path="$release_dir/$helper_name"
    if [ ! -f "$helper_path" ]; then
      continue
    fi

    echo "Signing target helper binary $helper_path"
    codesign --force --options runtime --timestamp --sign "$identity" "$helper_path"
    signed_any=1
  done
done < <(find "$tauri_dir/target" -type d -path "*/release" 2>/dev/null | sort)

if [ "$signed_any" -eq 0 ]; then
  echo "No target helper binaries found to sign."
fi

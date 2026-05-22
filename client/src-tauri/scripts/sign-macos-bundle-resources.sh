#!/usr/bin/env bash
set -euo pipefail

if [ "${TAURI_ENV_PLATFORM:-}" != "darwin" ] && [ "$(uname -s)" != "Darwin" ]; then
  exit 0
fi

find_developer_id_identity() {
  security find-identity -v -p codesigning 2>/dev/null |
    awk -F '"' '/Developer ID Application/ { print $2; exit }'
}

import_ci_certificate() {
  if [ -z "${APPLE_CERTIFICATE:-}" ] || [ -z "${APPLE_CERTIFICATE_PASSWORD:-}" ]; then
    return
  fi

  local temp_dir="${RUNNER_TEMP:-/tmp}"
  local keychain_password="${XERO_RESOURCE_SIGNING_KEYCHAIN_PASSWORD:-xero-resource-signing}"
  local keychain_path="$temp_dir/xero-resource-signing.keychain-db"
  local certificate_path="$temp_dir/xero-resource-signing.p12"

  rm -f "$keychain_path" "$certificate_path"
  python3 - "$certificate_path" <<'PY'
import base64
import os
import sys

certificate = os.environ.get("APPLE_CERTIFICATE", "")
with open(sys.argv[1], "wb") as output:
    output.write(base64.b64decode(certificate))
PY

  security create-keychain -p "$keychain_password" "$keychain_path"
  security set-keychain-settings -lut 21600 "$keychain_path"
  security unlock-keychain -p "$keychain_password" "$keychain_path"
  security import "$certificate_path" \
    -k "$keychain_path" \
    -P "$APPLE_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign \
    -T /usr/bin/security
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$keychain_password" \
    "$keychain_path" >/dev/null
  security list-keychains -d user -s "$keychain_path" $(security list-keychains -d user | tr -d '"')
  rm -f "$certificate_path"
}

identity="${XERO_MACOS_CODESIGN_IDENTITY:-${APPLE_SIGNING_IDENTITY:-}}"
if [ -n "$identity" ] &&
  ! security find-identity -v -p codesigning 2>/dev/null | grep -Fq "$identity"; then
  echo "Configured macOS signing identity is not available in the keychain; using imported Developer ID identity."
  identity=""
fi

if [ -z "$(find_developer_id_identity)" ]; then
  echo "Importing Apple Developer ID certificate for bundled resource signing."
  import_ci_certificate
fi

if [ -z "$identity" ]; then
  identity="$(find_developer_id_identity)"
fi

if [ -z "$identity" ]; then
  echo "No Developer ID Application identity available; skipping bundled resource signing."
  exit 0
fi

resource_root="${XERO_IDB_COMPANION_ROOT:-}"
if [ -z "$resource_root" ]; then
  if [ -d "src-tauri/resources/idb-companion.universal" ]; then
    resource_root="src-tauri/resources/idb-companion.universal"
  elif [ -d "resources/idb-companion.universal" ]; then
    resource_root="resources/idb-companion.universal"
  else
    exit 0
  fi
fi

sign_path() {
  local path="$1"
  codesign --force --options runtime --timestamp --sign "$identity" "$path"
}

echo "Signing bundled idb_companion resources."

find "$resource_root" -type f -print0 |
  while IFS= read -r -d '' file_path; do
    if file "$file_path" | grep -q "Mach-O"; then
      sign_path "$file_path"
    fi
  done

find "$resource_root" -type d -name "*.framework" -print0 |
  while IFS= read -r -d '' framework_path; do
    sign_path "$framework_path"
  done

idb_binary="$resource_root/bin/idb_companion"
if [ -x "$idb_binary" ]; then
  codesign --verify --strict --verbose=2 "$idb_binary"
fi

#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Xero dev runner expected the path to the compiled Tauri binary." >&2
  exit 64
fi

readonly executable="$1"
shift

sign_code() {
  local path="$1"
  local identifier="$2"
  local output

  if ! output=$(
    codesign \
      --force \
      --sign - \
      --identifier "$identifier" \
      "$path" \
      2>&1
  ); then
    echo "$output" >&2
    echo "Xero dev runner could not sign $path for macOS privacy prompts." >&2
    exit 65
  fi
}

plist_set_string() {
  local plist="$1"
  local key="$2"
  local value="$3"

  if ! /usr/libexec/PlistBuddy -c "Set :$key $value" "$plist" >/dev/null 2>&1; then
    /usr/libexec/PlistBuddy -c "Add :$key string $value" "$plist" >/dev/null
  fi
}

copy_sidecar_if_present() {
  local source="$1"
  local destination_dir="$2"
  local identifier_prefix="$3"

  [[ -f "$source" && -x "$source" ]] || return 0

  local name
  name="$(basename "$source")"
  local destination="$destination_dir/$name"
  cp -p "$source" "$destination"
  chmod 755 "$destination"
  sign_code "$destination" "$identifier_prefix.$name"
}

sync_resources_if_present() {
  local source="$1"
  local destination="$2"

  [[ -d "$source" ]] || return 0

  if [[ -L "$destination" || ! -d "$destination" ]]; then
    rm -rf "$destination"
    mkdir -p "$destination"
  fi

  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete "$source/" "$destination/"
    return 0
  fi

  rm -rf "$destination"
  cp -R "$source" "$destination"
}

prepare_macos_dev_bundle() {
  local source_executable="$1"
  local script_dir
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  local src_tauri_dir
  src_tauri_dir="$(cd "$script_dir/.." && pwd)"
  local info_template="$src_tauri_dir/Info.plist"
  local target_dir
  target_dir="$(cd "$(dirname "$source_executable")" && pwd)"
  local executable_name
  executable_name="$(basename "$source_executable")"
  local identifier="${XERO_DEV_CODESIGN_IDENTIFIER:-dev.sn0w.xero}"
  local app_bundle="$target_dir/Xero Dev.app"
  local contents_dir="$app_bundle/Contents"
  local macos_dir="$contents_dir/MacOS"
  local resources_dir="$contents_dir/Resources"
  local bundled_executable="$macos_dir/$executable_name"
  local info_plist="$contents_dir/Info.plist"

  if [[ ! -f "$info_template" ]]; then
    echo "Xero dev runner could not find $info_template for macOS privacy prompts." >&2
    exit 66
  fi

  rm -rf "$macos_dir" "$contents_dir/_CodeSignature"
  mkdir -p "$macos_dir" "$resources_dir"

  cp -p "$source_executable" "$bundled_executable"
  chmod 755 "$bundled_executable"
  cp "$info_template" "$info_plist"

  plist_set_string "$info_plist" CFBundleDevelopmentRegion en
  plist_set_string "$info_plist" CFBundleExecutable "$executable_name"
  plist_set_string "$info_plist" CFBundleIdentifier "$identifier"
  plist_set_string "$info_plist" CFBundleInfoDictionaryVersion 6.0
  plist_set_string "$info_plist" CFBundleName Xero
  plist_set_string "$info_plist" CFBundleDisplayName Xero
  plist_set_string "$info_plist" CFBundlePackageType APPL
  plist_set_string "$info_plist" CFBundleShortVersionString "${XERO_DEV_BUNDLE_VERSION:-0.1.0}"
  plist_set_string "$info_plist" CFBundleVersion "${XERO_DEV_BUNDLE_VERSION:-0.1.0}"
  plist_set_string "$info_plist" LSMinimumSystemVersion 10.15

  sync_resources_if_present "$target_dir/resources" "$resources_dir/resources"

  copy_sidecar_if_present "$target_dir/xero-cookie-importer" "$macos_dir" "$identifier"
  copy_sidecar_if_present "$target_dir/xero-ios-helper" "$macos_dir" "$identifier"
  for sidecar in "$target_dir"/Xero-runtime-supervisor*; do
    copy_sidecar_if_present "$sidecar" "$macos_dir" "$identifier"
  done

  sign_code "$bundled_executable" "$identifier"
  sign_code "$app_bundle" "$identifier"

  printf '%s\n' "$bundled_executable"
}

if [[ "$(uname -s)" == "Darwin" ]]; then
  if ! command -v codesign >/dev/null 2>&1; then
    echo "Xero dev runner requires codesign so macOS privacy prompts can read Info.plist." >&2
    exit 69
  fi

  readonly bundled_executable="$(prepare_macos_dev_bundle "$executable")"
  exec "$bundled_executable" "$@"
fi

exec "$executable" "$@"

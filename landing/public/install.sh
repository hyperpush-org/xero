#!/bin/sh
set -eu

fail() {
  printf 'xero-tui install: %s\n' "$1" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

checksum_cmd() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf 'sha256sum'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    printf 'shasum'
    return
  fi

  fail 'missing required command: sha256sum or shasum'
}

verify_checksum() {
  cmd="$1"
  checksum_file="$2"

  if [ "$cmd" = "sha256sum" ]; then
    sha256sum -c "$checksum_file" >/dev/null
    return
  fi

  shasum -a 256 -c "$checksum_file" >/dev/null
}

target_triple() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os:$arch" in
    Darwin:arm64|Darwin:aarch64)
      printf 'aarch64-apple-darwin'
      ;;
    Darwin:x86_64|Darwin:amd64)
      printf 'x86_64-apple-darwin'
      ;;
    Linux:x86_64|Linux:amd64)
      printf 'x86_64-unknown-linux-gnu'
      ;;
    *)
      fail "unsupported platform: $os $arch"
      ;;
  esac
}

main() {
  need_cmd curl
  need_cmd mktemp
  need_cmd tar

  target="$(target_triple)"
  checksum="$(checksum_cmd)"
  base_url="${XERO_INSTALL_BASE_URL:-https://xeroshell.com}"
  base_url="${base_url%/}"
  install_dir="${XERO_INSTALL_DIR:-$HOME/.local/bin}"
  archive="xero-tui-$target.tar.gz"
  archive_url="$base_url/downloads/tui/latest/$archive"
  checksum_url="$archive_url.sha256"
  tmp_dir="$(mktemp -d)"

  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  printf 'Downloading %s\n' "$archive_url"
  curl -fsSL "$archive_url" -o "$tmp_dir/$archive"
  curl -fsSL "$checksum_url" -o "$tmp_dir/$archive.sha256"

  (
    cd "$tmp_dir"
    verify_checksum "$checksum" "$archive.sha256"
    tar -xzf "$archive"
  )

  [ -f "$tmp_dir/xero-tui" ] || fail "archive did not contain xero-tui"

  mkdir -p "$install_dir"
  if command -v install >/dev/null 2>&1; then
    install -m 0755 "$tmp_dir/xero-tui" "$install_dir/xero-tui"
  else
    cp "$tmp_dir/xero-tui" "$install_dir/xero-tui"
    chmod 0755 "$install_dir/xero-tui"
  fi

  printf 'Installed xero-tui to %s/xero-tui\n' "$install_dir"
  case ":$PATH:" in
    *":$install_dir:"*)
      printf 'Run it with: xero-tui\n'
      ;;
    *)
      printf 'Add this to your shell profile, then run xero-tui:\n'
      printf '  export PATH="%s:$PATH"\n' "$install_dir"
      ;;
  esac
}

main "$@"

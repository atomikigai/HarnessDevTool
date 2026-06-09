#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$(rustc -vV | awk '/host:/{print $2}')"
SIDECAR="$ROOT/src-tauri/binaries/harness-server-${TARGET}"
BUNDLE_DIR="$ROOT/src-tauri/target/release/bundle"

fail() {
  echo "tauri bundle check: $*" >&2
  exit 1
}

note() {
  echo "tauri bundle check: $*" >&2
}

[[ -x "$SIDECAR" ]] || fail "missing executable sidecar: $SIDECAR"
[[ -d "$BUNDLE_DIR" ]] || fail "missing bundle directory: $BUNDLE_DIR"

found_bundle=0
found_sidecar=0

check_listing() {
  local label="$1"
  local list="$2"
  found_bundle=1
  if grep -Eq '(^|/)harness-server([^/]*$)' <<<"$list"; then
    found_sidecar=1
    note "ok: $label contains harness-server sidecar"
  else
    note "missing sidecar in $label"
  fi
}

while IFS= read -r appdir; do
  listing="$(find "$appdir" -type f -printf '%P\n')"
  check_listing "$appdir" "$listing"
done < <(find "$BUNDLE_DIR" -type d -name '*.AppDir' 2>/dev/null | sort)

while IFS= read -r deb_dir; do
  data_tar="$(find "$deb_dir" -maxdepth 1 -type f -name 'data.tar.*' | head -1)"
  [[ -n "${data_tar:-}" ]] || continue
  listing="$(tar -tf "$data_tar")"
  check_listing "$deb_dir" "$listing"
done < <(find "$BUNDLE_DIR/deb" -maxdepth 1 -type d -name 'HarnessDevTool_*' 2>/dev/null | sort)

while IFS= read -r deb; do
  if command -v dpkg-deb >/dev/null 2>&1; then
    listing="$(dpkg-deb -c "$deb")"
  elif command -v ar >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
    tmp="$(mktemp -d)"
    (cd "$tmp" && ar x "$deb")
    data_tar="$(find "$tmp" -maxdepth 1 -type f -name 'data.tar.*' | head -1)"
    [[ -n "${data_tar:-}" ]] || fail "could not locate data.tar.* inside $deb"
    listing="$(tar -tf "$data_tar")"
    rm -rf "$tmp"
  else
    note "skip: no dpkg-deb/ar+tar available for $deb"
    continue
  fi
  check_listing "$deb" "$listing"
done < <(find "$BUNDLE_DIR/deb" -maxdepth 1 -type f -name '*.deb' 2>/dev/null | sort)

while IFS= read -r archive; do
  if command -v bsdtar >/dev/null 2>&1; then
    listing="$(bsdtar -tf "$archive" || true)"
  elif command -v rpm >/dev/null 2>&1; then
    listing="$(rpm -qlp "$archive" || true)"
  else
    note "skip: no bsdtar/rpm available for $archive"
    continue
  fi
  check_listing "$archive" "$listing"
done < <(find "$BUNDLE_DIR" -type f \( -name '*.rpm' -o -name '*.msi' -o -name '*.dmg' -o -name '*.AppImage' \) 2>/dev/null | sort)

[[ "$found_bundle" -eq 1 ]] || fail "no inspectable Tauri bundles found under $BUNDLE_DIR"
[[ "$found_sidecar" -eq 1 ]] || fail "no inspected bundle contains harness-server sidecar"

note "passed"

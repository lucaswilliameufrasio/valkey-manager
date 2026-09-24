#!/usr/bin/env bash
set -euo pipefail

target="${TARGET:?TARGET must be set to an Apple Rust target}"
version="${VERSION:-0.0.0}"
version="${version#v}"
dist_dir="${DIST_DIR:-dist}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${BINARY:-$repo_root/target/$target/release/valkey-manager}"
app_only="${APP_ONLY:-false}"
tmp_dir="$(mktemp -d)"
app_bundle="$tmp_dir/Valkey Manager.app"
contents="$app_bundle/Contents"
asset_base="valkey-manager-$target"

cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

checksum() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$@"
    else
        sha256sum "$@"
    fi
}

if [[ "$target" != "x86_64-apple-darwin" && "$target" != "aarch64-apple-darwin" ]]; then
    echo "unsupported macOS target: $target" >&2
    exit 1
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "invalid macOS bundle version: $version" >&2
    exit 1
fi
if [[ ! -x "$binary" ]]; then
    echo "release binary not found or not executable: $binary" >&2
    exit 1
fi

mkdir -p "$contents/MacOS" "$contents/Resources" "$tmp_dir/dmg-root" "$dist_dir"
cp "$binary" "$contents/MacOS/valkey-manager"
cp "$repo_root/assets/icons/valkey-manager.icns" "$contents/Resources/icon.icns"

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Valkey Manager</string>
  <key>CFBundleExecutable</key><string>valkey-manager</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundleIdentifier</key><string>com.lucaseufrasio.valkey-manager</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Valkey Manager</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

python3 - "$app_bundle" "$contents/Info.plist" "$dist_dir/$asset_base.app.zip" <<'PY'
import plistlib
import stat
import sys
import zipfile
from pathlib import Path

bundle = Path(sys.argv[1])
plist_path = Path(sys.argv[2])
archive_path = Path(sys.argv[3])
with plist_path.open("rb") as plist_file:
    plistlib.load(plist_file)

with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(bundle.rglob("*")):
        if not path.is_file():
            continue
        relative = Path(bundle.name) / path.relative_to(bundle)
        info = zipfile.ZipInfo.from_file(path, str(relative))
        info.create_system = 3
        info.external_attr = (stat.S_IMODE(path.stat().st_mode) & 0xFFFF) << 16
        archive.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
PY
(cd "$dist_dir" && checksum "$asset_base.app.zip" > "$asset_base.app.zip.sha256")

if [[ "$app_only" == "true" ]]; then
    exit 0
fi
if ! command -v hdiutil >/dev/null 2>&1; then
    echo "hdiutil is required to create a macOS DMG (set APP_ONLY=true to create only the .app.zip)" >&2
    exit 1
fi

ditto "$app_bundle" "$tmp_dir/dmg-root/Valkey Manager.app"
ln -s /Applications "$tmp_dir/dmg-root/Applications"
hdiutil create \
    -volname "Valkey Manager" \
    -srcfolder "$tmp_dir/dmg-root" \
    -ov \
    -format UDZO \
    "$dist_dir/$asset_base.dmg"

(cd "$dist_dir" && checksum "$asset_base.dmg" > "$asset_base.dmg.sha256")

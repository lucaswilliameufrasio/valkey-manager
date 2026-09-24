#!/usr/bin/env bash
set -euo pipefail

target="${TARGET:?TARGET must be set to an Apple Rust target}"
version="${VERSION:-0.0.0}"
version="${version#v}"
dist_dir="${DIST_DIR:-dist}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/$target/release/valkey-manager"
tmp_dir="$(mktemp -d)"
app_bundle="$tmp_dir/Valkey Manager.app"
contents="$app_bundle/Contents"
asset_base="valkey-manager-$target"

cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

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
cp "$repo_root/src-tauri/icons/icon.icns" "$contents/Resources/icon.icns"

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
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$contents/Info.plist" >/dev/null

ditto -c -k --sequesterRsrc --keepParent \
    "$app_bundle" "$dist_dir/$asset_base.app.zip"
ditto "$app_bundle" "$tmp_dir/dmg-root/Valkey Manager.app"
ln -s /Applications "$tmp_dir/dmg-root/Applications"
hdiutil create \
    -volname "Valkey Manager" \
    -srcfolder "$tmp_dir/dmg-root" \
    -ov \
    -format UDZO \
    "$dist_dir/$asset_base.dmg"

(cd "$dist_dir" && shasum -a 256 "$asset_base.app.zip" > "$asset_base.app.zip.sha256")
(cd "$dist_dir" && shasum -a 256 "$asset_base.dmg" > "$asset_base.dmg.sha256")

#!/usr/bin/env bash
set -euo pipefail

target="${TARGET:-x86_64-unknown-linux-gnu}"
dist_dir="${DIST_DIR:-dist}"
linuxdeploy_version="1-alpha-20251107-1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_root/target/$target/release/valkey-manager"
tmp_dir="$(mktemp -d)"
appdir="$tmp_dir/AppDir"
tool_dir="$tmp_dir/tools"
linuxdeploy="$tool_dir/linuxdeploy-x86_64.AppImage"
asset="valkey-manager-x86_64.AppImage"

cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

if [[ "$target" != "x86_64-unknown-linux-gnu" ]]; then
    echo "AppImage packaging is configured for x86_64-unknown-linux-gnu, got: $target" >&2
    exit 1
fi
if [[ ! -x "$binary" ]]; then
    echo "release binary not found or not executable: $binary" >&2
    exit 1
fi

mkdir -p \
    "$appdir/usr/bin" \
    "$appdir/usr/share/applications" \
    "$appdir/usr/share/icons/hicolor/128x128/apps" \
    "$tool_dir" \
    "$dist_dir"
cp "$binary" "$appdir/usr/bin/valkey-manager"
cp "$repo_root/assets/icons/valkey-manager.png" \
    "$appdir/usr/share/icons/hicolor/128x128/apps/valkey-manager.png"

cat > "$appdir/usr/share/applications/valkey-manager.desktop" <<'DESKTOP'
[Desktop Entry]
Name=Valkey Manager
Comment=Desktop client for managing Valkey instances
Exec=valkey-manager
Icon=valkey-manager
Terminal=false
Type=Application
Categories=Development;Database;
DESKTOP

curl --fail --location --silent --show-error \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/$linuxdeploy_version/linuxdeploy-x86_64.AppImage" \
    --output "$linuxdeploy"
chmod +x "$linuxdeploy"

(
    cd "$tmp_dir"
    APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" \
        --appdir "$appdir" \
        --executable "$appdir/usr/bin/valkey-manager" \
        --desktop-file "$appdir/usr/share/applications/valkey-manager.desktop" \
        --icon-file "$repo_root/assets/icons/valkey-manager.png" \
        --output appimage
)

shopt -s nullglob
images=("$tmp_dir"/*.AppImage)
if [[ "${#images[@]}" -ne 1 ]]; then
    printf 'expected one generated AppImage, found %s\n' "${#images[@]}" >&2
    exit 1
fi
install -m 0755 "${images[0]}" "$dist_dir/$asset"
(cd "$dist_dir" && sha256sum "$asset" > "$asset.sha256")

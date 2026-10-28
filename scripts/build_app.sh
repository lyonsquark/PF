#!/bin/bash
set -euo pipefail

# ─── Configuration ───
APP_NAME="PFRust"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${ROOT}/target/release"
APP_BUNDLE="${ROOT}/${APP_NAME}.app"
ICON_SOURCE="${ROOT}/apps/pf/macos/AppIcon.png"
PLIST_TEMPLATE="${ROOT}/apps/pf/macos/Info.plist"

usage() {
    echo "Usage: scripts/build_app.sh [--install]" >&2
    exit 2
}

case "${1:-}" in
    "" | --install) ;;
    *) usage ;;
esac

# The version is the first `version = "..."` under [workspace.package]. The build number is the
# commit count, or 1 when git is unavailable.
VERSION="$(awk '/^\[workspace.package\]/ { f = 1; next } /^\[/ { f = 0 } f && /^version = / { gsub(/"/, "", $3); print $3; exit }' "${ROOT}/Cargo.toml")"
BUILD="$(git -C "${ROOT}" rev-list --count HEAD 2>/dev/null || echo 1)"

echo "🔨 Building PFRust in release mode..."
cargo build --release -p pf --manifest-path "${ROOT}/Cargo.toml"

echo "📦 Assembling ${APP_NAME}.app bundle..."

# Clean previous bundle
rm -rf "${APP_BUNDLE}"

# Create bundle structure
mkdir -p "${APP_BUNDLE}/Contents/MacOS"
mkdir -p "${APP_BUNDLE}/Contents/Resources"

# Copy the executable (the binary is named PF; see apps/pf/Cargo.toml)
cp "${TARGET_DIR}/PF" "${APP_BUNDLE}/Contents/MacOS/PF"

# Write Info.plist from the template, then check it
sed -e "s/@VERSION@/${VERSION}/" -e "s/@BUILD@/${BUILD}/" "${PLIST_TEMPLATE}" > "${APP_BUNDLE}/Contents/Info.plist"
plutil -lint "${APP_BUNDLE}/Contents/Info.plist" > /dev/null

# ─── Create .icns from the PNG ───
if [ -f "${ICON_SOURCE}" ]; then
    echo "🎨 Creating app icon..."
    ICONSET_DIR="${ROOT}/target/AppIcon.iconset"
    rm -rf "${ICONSET_DIR}"
    mkdir -p "${ICONSET_DIR}"

    # Generate all required sizes
    sips -z 16 16     "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_16x16.png"      > /dev/null 2>&1
    sips -z 32 32     "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_16x16@2x.png"   > /dev/null 2>&1
    sips -z 32 32     "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_32x32.png"      > /dev/null 2>&1
    sips -z 64 64     "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_32x32@2x.png"   > /dev/null 2>&1
    sips -z 128 128   "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_128x128.png"    > /dev/null 2>&1
    sips -z 256 256   "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_128x128@2x.png" > /dev/null 2>&1
    sips -z 256 256   "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_256x256.png"    > /dev/null 2>&1
    sips -z 512 512   "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_256x256@2x.png" > /dev/null 2>&1
    sips -z 512 512   "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_512x512.png"    > /dev/null 2>&1
    sips -z 1024 1024 "${ICON_SOURCE}" --out "${ICONSET_DIR}/icon_512x512@2x.png" > /dev/null 2>&1

    # Convert iconset to icns
    iconutil --convert icns --output "${APP_BUNDLE}/Contents/Resources/AppIcon.icns" "${ICONSET_DIR}"
    rm -rf "${ICONSET_DIR}"
    echo "  ✅ Icon created"
else
    echo "  ⚠️  No icon source found at ${ICON_SOURCE}, skipping icon"
fi

# ─── Write PkgInfo ───
echo -n "APPL????" > "${APP_BUNDLE}/Contents/PkgInfo"

# Ad-hoc sign so Gatekeeper accepts the locally built binary on Apple Silicon
codesign --force --deep -s - "${APP_BUNDLE}"

echo ""
echo "✅ ${APP_NAME}.app built successfully!"
echo "   Version: ${VERSION} (build ${BUILD})"
echo "   Location: ${APP_BUNDLE}"
echo ""

# ─── Optionally install to /Applications ───
if [ "${1:-}" = "--install" ]; then
    echo "📲 Installing to /Applications..."
    rm -rf "/Applications/${APP_NAME}.app"
    cp -R "${APP_BUNDLE}" "/Applications/${APP_NAME}.app"
    echo "✅ Installed to /Applications/${APP_NAME}.app"
    echo ""
    echo "You can now launch PFRust from Launchpad, Spotlight, or /Applications!"
else
    echo "To install to /Applications, run:  scripts/build_app.sh --install"
fi

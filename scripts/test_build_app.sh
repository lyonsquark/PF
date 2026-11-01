#!/bin/bash
set -uo pipefail

# Builds PF.app (without --install) and checks the bundle. Prints OK, or the failing check.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP="${ROOT}/PF.app"

fail() {
    echo "FAIL: $1" >&2
    exit 1
}

check() {
    local name="$1"
    shift
    "$@" > /dev/null 2>&1 || fail "${name}"
}

VERSION="$(awk '/^\[workspace.package\]/ { f = 1; next } /^\[/ { f = 0 } f && /^version = / { gsub(/"/, "", $3); print $3; exit }' "${ROOT}/Cargo.toml")"

"${SCRIPT_DIR}/build_app.sh" > /dev/null 2>&1 || fail "build_app.sh failed"

check "binary exists and is executable" test -x "${APP}/Contents/MacOS/PF"
check "bundle identifier" test "$(plutil -extract CFBundleIdentifier raw "${APP}/Contents/Info.plist")" = "local.pf.app"
check "version matches the workspace" test "$(plutil -extract CFBundleShortVersionString raw "${APP}/Contents/Info.plist")" = "${VERSION}"
if [ -f "${ROOT}/apps/pf/macos/AppIcon.png" ]; then
    check "icon exists" test -f "${APP}/Contents/Resources/AppIcon.icns"
fi
check "PkgInfo" grep -q "APPL????" "${APP}/Contents/PkgInfo"
check "codesign verifies" codesign --verify "${APP}"

"${SCRIPT_DIR}/build_app.sh" --bogus > /dev/null 2>&1
[ "$?" -eq 2 ] || fail "--bogus must exit with code 2"

echo "OK"

# T25: `scripts/build_app.sh`, the PFRust.app bundle, icon and install

**Phase:** 4 · **Deps:** T13 · **Crate(s):** none (`scripts/`, `apps/pf/macos/`, `xtask`) · **Size:** small (≈150 lines)

## Goal

One command builds a double-clickable `PFRust.app` with the app icon and, with `--install`, copies it to `/Applications`.
It mirrors the Swift project's `../PFApp/build_app.sh`, without the WebCharts resource bundle (PFRust has no web content).

## Read first

- `../PFApp/build_app.sh` (whole file; copy its structure and messages)
- `../PFApp/Sources/PFApp/Resources/Info.plist` (template to adapt)
- `apps/pf/Cargo.toml` (the binary is named **`PF`**: `[[bin]] name = "PF"`, so the release binary is `target/release/PF`)

## Deliverables

```
apps/pf/macos/Info.plist        # template with @VERSION@ placeholders
scripts/build_app.sh            # executable (chmod +x)
scripts/test_build_app.sh       # verification script (below)
xtask: `cargo xtask bundle [--install]`  → runs scripts/build_app.sh with the same args
```

### `apps/pf/macos/Info.plist`

Start from the Swift plist and change exactly these values:

| Key | Value |
|---|---|
| `CFBundleExecutable` | `PF` |
| `CFBundleIdentifier` | `local.pfrust.app` |
| `CFBundleName` / `CFBundleDisplayName` | `PFRust` |
| `CFBundleShortVersionString` | `@VERSION@` (replaced by the workspace version, e.g. `0.1.0`) |
| `CFBundleVersion` | `@BUILD@` (replaced by `git rev-list --count HEAD`, or `1` when git is unavailable) |
| `LSMinimumSystemVersion` | `13.0` |

Keep `CFBundleIconFile=AppIcon`, `CFBundlePackageType=APPL`, `NSHighResolutionCapable=true`,
`LSApplicationCategoryType=public.app-category.finance`, and `NSAppTransportSecurity/NSAllowsLocalNetworking=true`.

### `scripts/build_app.sh` (behaviour)

```bash
#!/bin/bash
set -euo pipefail
APP_NAME="PFRust"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"            # PFRust/
TARGET_DIR="${ROOT}/target/release"
APP_BUNDLE="${ROOT}/${APP_NAME}.app"
ICON_SOURCE="${ROOT}/../PFApp/Sources/PFApp/Resources/AppIcon.png"
PLIST_TEMPLATE="${ROOT}/apps/pf/macos/Info.plist"
```
1. `echo "🔨 Building PFRust in release mode..."` then `cargo build --release -p pf` (from `$ROOT`).
2. `rm -rf` the old bundle; create `Contents/MacOS` and `Contents/Resources`.
3. Copy `target/release/PF` to `Contents/MacOS/PF`.
4. Version = the `version = "…"` line under `[workspace.package]` in `$ROOT/Cargo.toml` (use `sed -n`/`awk`, no extra tools).
   Write the plist with `@VERSION@`/`@BUILD@` replaced (`sed`), then `plutil -lint` it (fail the script on error).
5. Icon: if `AppIcon.png` exists, create a temporary `AppIcon.iconset` under `target/` with the **same 10 `sips -z` sizes**
   as the Swift script (16, 16@2x, 32, 32@2x, 128, 128@2x, 256, 256@2x, 512, 512@2x), run
   `iconutil --convert icns --output Contents/Resources/AppIcon.icns`, then delete the iconset. If it is missing, print the
   same ⚠️ warning as Swift and continue.
6. Write `Contents/PkgInfo` = `APPL????`.
7. Ad-hoc sign so Gatekeeper on Apple Silicon accepts the locally built binary: `codesign --force --deep -s - "${APP_BUNDLE}"`.
8. Print the success block. With `--install`: `rm -rf /Applications/PFRust.app`, `cp -R`, and print the "You can now launch
   PFRust from Launchpad, Spotlight, or /Applications!" message. Without it, print the hint for `--install`, as Swift does.
9. Any other argument ⇒ print the usage `Usage: scripts/build_app.sh [--install]` and exit 2.

**Never** touch `/Applications/PF.app` (the Swift app).

### `scripts/test_build_app.sh`

Runs `scripts/build_app.sh` (without `--install`) and asserts:
- `PFRust.app/Contents/MacOS/PF` exists and is executable
- `plutil -extract CFBundleIdentifier raw PFRust.app/Contents/Info.plist` == `local.pfrust.app`
- `plutil -extract CFBundleShortVersionString raw …` equals the workspace version
- `PFRust.app/Contents/Resources/AppIcon.icns` exists (when the PNG exists)
- `PFRust.app/Contents/PkgInfo` contains `APPL????`
- `codesign --verify PFRust.app` succeeds
- `scripts/build_app.sh --bogus` exits with code 2
Prints `OK` and exits 0, or prints the failing check and exits 1.

### xtask

Add a `bundle` subcommand to `xtask/src/main.rs` that runs `scripts/build_app.sh` with the remaining args and forwards the
exit code. Keep the existing `rules` and `ci` subcommands unchanged.

### `.gitignore`

Add `/PFRust.app`.

## Steps

1. Create the plist template.
2. Write `build_app.sh`; `chmod +x`.
3. Write `test_build_app.sh`; `chmod +x`.
4. Add the xtask subcommand and the `.gitignore` line.
5. Run Done-when. Then double-click `PFRust.app` in Finder: the window opens, the Dock shows the icon, the title is "PF".

## Tests (required)

| Test name | Covers | What it checks |
|---|---|---|
| `scripts/test_build_app.sh` | — | All the assertions listed above |
| `shellcheck scripts/*.sh` (if installed: `brew install shellcheck`) | — | No warnings |

## Pitfalls

- macOS ships BSD `sed`: use `sed -i ''` or write to a temp file; prefer `sed 's/@VERSION@/…/' template > out`.
- `set -u` fails on an unset `$1`; use `"${1:-}"`.
- `cargo build` must run from `$ROOT`, or pass `--manifest-path "$ROOT/Cargo.toml"`.
- Both apps bind port 8989; the user must quit one before starting the other (PLAN.md "Migration procedure"). Do not change the port here.

## Done when

```bash
cd PFRust
scripts/test_build_app.sh          # prints OK
cargo xtask bundle                 # same as scripts/build_app.sh
cargo clippy -p xtask -- -D warnings
```
Manual: `open PFRust.app` launches the app with the icon.

## Out of scope

- Notarisation and Developer ID signing, DMG packaging, auto-update.

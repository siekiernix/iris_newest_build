#!/usr/bin/env bash
# Build iris-appkit and wrap it in "IRIS Preview.app", the native macOS shell
# (bundle id io.techomancer.iris.appkit). Optionally sign with a Developer ID,
# notarize, and make a DMG. The release packager, scripts/dist/package-macos.sh,
# calls this with --binary to wrap the universal binary CI built.
#
#   iris-gui/frontends/appkit/scripts/bundle.sh [options]
#
# Options:
#   --features LIST    iris-gui-appkit features
#                      (default: bundled,lightning,jitv2,idle-pause,hostgl)
#   --version V        CFBundleShortVersionString (default: dev.<commit>)
#   --arch A           universal (default), arm64, x86_64
#   --binary PATH      wrap this iris-appkit instead of building one
#   --out DIR          where the .app (and .dmg) go (default: dist/)
#   --bundle-id ID     CFBundleIdentifier (default: io.techomancer.iris.appkit)
#   --sign IDENTITY    codesign identity; "-" (the default) signs ad hoc.
#                      Also read from CODESIGN_IDENTITY.
#   --dmg              also make "IRIS-Preview-<version>-macos.dmg"
#   --notarize         submit the DMG with notarytool and staple it. Uses
#                      NOTARY_KEY_ID/NOTARY_ISSUER_ID/NOTARY_KEY_PATH, or
#                      NOTARY_APPLE_ID/NOTARY_PASSWORD/NOTARY_TEAM_ID, as
#                      scripts/dist/package-macos.sh does.
#
# Then: open "dist/IRIS Preview.app"
source "$(dirname "$0")/../../../../scripts/dist/lib.sh"

FEATURES="bundled,lightning,jitv2,idle-pause,hostgl"
ARCH_SEL=universal
BUNDLE_ID="io.techomancer.iris.appkit"
IDENTITY="${CODESIGN_IDENTITY:--}"
DMG=0
NOTARIZE=0
NAME="IRIS Preview"
PREBUILT=""

while [ $# -gt 0 ]; do
    case "$1" in
        --features)  FEATURES="$2"; shift 2 ;;
        --version)   VERSION="$2"; shift 2 ;;
        --arch)      ARCH_SEL="$2"; shift 2 ;;
        --binary)    PREBUILT="$2"; shift 2 ;;
        --out)       OUT_DIR="$2"; shift 2 ;;
        --bundle-id) BUNDLE_ID="$2"; shift 2 ;;
        --sign)      IDENTITY="$2"; shift 2 ;;
        --dmg)       DMG=1; shift ;;
        --notarize)  NOTARIZE=1; DMG=1; shift ;;
        -h|--help)   sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done

[ "$(uname -s)" = Darwin ] || die "bundle.sh runs on macOS only"
cd "$DIST_ROOT"
[ -n "$VERSION" ] || VERSION=$("$DIST_ROOT/scripts/dist/version.sh" dev)
case "$OUT_DIR" in /*) ;; *) OUT_DIR="$DIST_ROOT/$OUT_DIR" ;; esac
mkdir -p "$OUT_DIR"
export RELEASE_VERSION="$VERSION"

case "$ARCH_SEL" in
    universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
    arm64)     TARGETS=(aarch64-apple-darwin) ;;
    x86_64|x64) TARGETS=(x86_64-apple-darwin) ;;
    *) die "--arch must be universal, arm64 or x86_64" ;;
esac

# ── Build ────────────────────────────────────────────────────────────────────
WORK=$(stage_dir)
trap 'rm -rf "$WORK"' EXIT
BINARY="$WORK/iris-appkit"
if [ -n "$PREBUILT" ]; then
    [ -x "$PREBUILT" ] || die "--binary $PREBUILT is not an executable"
    cp "$PREBUILT" "$BINARY"
else
    slices=()
    for t in "${TARGETS[@]}"; do
        ensure_target "$t"
        group_start "cargo build iris-gui-appkit ($t) features=$FEATURES"
        cargo build --release --target "$t" -p iris-gui-appkit --features "$FEATURES"
        group_end
        slices+=("$(release_dir "$t")/iris-appkit")
    done
    if [ "${#slices[@]}" -gt 1 ]; then
        lipo -create -output "$BINARY" "${slices[@]}"
    else
        cp "${slices[0]}" "$BINARY"
    fi
fi
lipo -info "$BINARY" >&2

# ── Assemble ─────────────────────────────────────────────────────────────────
APP="$OUT_DIR/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BINARY" "$APP/Contents/MacOS/iris-appkit"
chmod 755 "$APP/Contents/MacOS/iris-appkit"
cp iris-gui/assets/icons/icon.icns "$APP/Contents/Resources/AppIcon.icns"
cp LICENSE LICENSE-libchdman-rs.txt LICENSE-rusty-backup-crates.txt "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>${NAME}</string>
    <key>CFBundleDisplayName</key><string>${NAME}</string>
    <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleExecutable</key><string>iris-appkit</string>
    <key>CFBundleIconFile</key><string>AppIcon.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>LSMinimumSystemVersion</key><string>10.13</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSCameraUsageDescription</key><string>Provides the IndyCam video input for SGI Indy emulation (VINO device).</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# ── Sign ─────────────────────────────────────────────────────────────────────
if [ "$IDENTITY" = "-" ]; then
    log "signing ad hoc"
    codesign --force --deep --sign - "$APP"
else
    ENT="installer/iris-gui-notarized.entitlements"
    plutil -lint "$ENT" >/dev/null
    log "signing as '$IDENTITY' with $ENT"
    codesign --force --options runtime --timestamp --entitlements "$ENT" --sign "$IDENTITY" "$APP"
    codesign --verify --deep --strict "$APP"
fi

# ── DMG and notarization ─────────────────────────────────────────────────────
if [ "$DMG" = 1 ]; then
    DMG_PATH="$OUT_DIR/IRIS-Preview-${VERSION}-macos.dmg"
    rm -f "$DMG_PATH"
    STAGE="$WORK/dmg"
    mkdir -p "$STAGE"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"
    hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG_PATH" >/dev/null
    [ "$IDENTITY" = "-" ] || codesign --force --timestamp --sign "$IDENTITY" "$DMG_PATH"
    if [ "$NOTARIZE" = 1 ]; then
        if [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_KEY_PATH:-}" ]; then
            auth=(--key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID")
        elif [ -n "${NOTARY_APPLE_ID:-}" ]; then
            auth=(--apple-id "$NOTARY_APPLE_ID" --password "$NOTARY_PASSWORD" --team-id "$NOTARY_TEAM_ID")
        else
            die "--notarize needs NOTARY_KEY_ID/NOTARY_KEY_PATH/NOTARY_ISSUER_ID or NOTARY_APPLE_ID/NOTARY_PASSWORD/NOTARY_TEAM_ID"
        fi
        xcrun notarytool submit "$DMG_PATH" "${auth[@]}" --wait
        xcrun stapler staple "$DMG_PATH"
    fi
    log "built $DMG_PATH"
fi

log "built $APP ($VERSION, $BUNDLE_ID)"

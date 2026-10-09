#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FLUTTER="${FLUTTER_BIN:-/Users/mvmanh/Development/flutter-sdk/bin/flutter}"
cd "$ROOT"
export CARGO_TARGET_DIR="$ROOT/target"
SIGN_MODE="${CLASSROOM_MAC_SIGN_MODE:-adhoc}"
case "$SIGN_MODE" in
  adhoc) SIGN_OPTIONS=(--development-ad-hoc --manifest "$ROOT/target/teacher-development-signing.json") ;;
  apple) python3 scripts/sign-macos-app.py --check-identity; SIGN_OPTIONS=(--manifest "$ROOT/target/teacher-signing.json") ;;
  *) echo "CLASSROOM_MAC_SIGN_MODE must be adhoc or apple" >&2; exit 1 ;;
esac
# Always build paired roles from the same working tree; never ship a stale Student.
export CLASSROOM_BUILD_LABEL="${CLASSROOM_BUILD_LABEL:-paired-$(date +%Y%m%d-%H%M%S)}"
rustup run stable cargo build --release --bin Classroom --bin ClassroomStudent
CROSS_DIR="$ROOT/target/rustup-windows"
env CARGO_TARGET_DIR="$CROSS_DIR" rustup run stable cargo build --release \
  --bin ClassroomStudent --bin ClassroomGuardService --target x86_64-pc-windows-gnu
mkdir -p "$ROOT/target/updates"
STUDENT_PACKAGE="$ROOT/target/updates/student-windows-x64.zip"
python3 scripts/classroom-dev-update.py package \
  "$CROSS_DIR/x86_64-pc-windows-gnu/release/ClassroomStudent.exe" \
  --version "$CLASSROOM_BUILD_LABEL" --output "$STUDENT_PACKAGE"
# ClassroomGuardService.exe rides alongside as an extra, unverified entry —
# the manifest/hash the auto-update path trusts still covers only
# ClassroomStudent.exe, so this cannot weaken that mechanism. The Teacher's
# "Share Student app" HTTP download (student_bootstrap.rs) carries this
# entry through into what a Student actually downloads, so running the
# downloaded ClassroomStudent.exe elevated once self-installs the Guard
# service — one download, one elevated run, fully protected.
WINDOWS_RELEASE_DIR="$CROSS_DIR/x86_64-pc-windows-gnu/release"
(cd "$WINDOWS_RELEASE_DIR" && zip -q "$STUDENT_PACKAGE" ClassroomGuardService.exe)
# Separate plain-folder bundle too, for an admin who wants to copy both
# executables onto a machine directly (e.g. over SMB) without going through
# the HTTP share at all.
BUNDLE_DIR="$ROOT/target/updates/classroom-student-guard-windows-x64"
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR"
cp "$WINDOWS_RELEASE_DIR/ClassroomStudent.exe" "$BUNDLE_DIR/ClassroomStudent.exe"
cp "$WINDOWS_RELEASE_DIR/ClassroomGuardService.exe" "$BUNDLE_DIR/ClassroomGuardService.exe"
GUARD_BUNDLE="$ROOT/target/updates/classroom-student-guard-windows-x64.zip"
rm -f "$GUARD_BUNDLE"
(cd "$ROOT/target/updates" && zip -q -r "$(basename "$GUARD_BUNDLE")" "$(basename "$BUNDLE_DIR")")
CLASSROOM_MAC_OUTPUT_DIR="${CLASSROOM_MAC_OUTPUT_DIR:-$ROOT/target}" \
  bash scripts/build-student-macos.sh
MAC_STUDENT="${CLASSROOM_MAC_OUTPUT_DIR:-$ROOT/target}/Classroom Student.app"
python3 scripts/classroom-dev-update.py package "$MAC_STUDENT" \
  --version "$CLASSROOM_BUILD_LABEL" --output "$ROOT/target/updates/student-macos-arm64.zip"
cd apps/classroom_console
"$FLUTTER" build macos --release
BUILD_APP="$ROOT/apps/classroom_console/build/macos/Build/Products/Release/classroom_console.app"
OUTPUT_DIR="${CLASSROOM_MAC_OUTPUT_DIR:-$ROOT/target}"
mkdir -p "$OUTPUT_DIR"
APP="$OUTPUT_DIR/Classroom Console.app"
AGENT_APP="$BUILD_APP/Contents/Helpers/Classroom Agent.app"
mkdir -p "$AGENT_APP/Contents/MacOS"
# A raw helper inside the console bundle is registered by LaunchServices as
# that console. Finder/Dock reopen then activates Rust (no window) instead of
# Flutter. Give the background process its own nested bundle identity.
rm -f "$BUILD_APP/Contents/Helpers/Classroom"
cp "$ROOT/target/release/Classroom" "$AGENT_APP/Contents/MacOS/Classroom"
cat > "$AGENT_APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.classroom.classroomConsole.agent</string>
<key>CFBundleExecutable</key><string>Classroom</string>
<key>CFBundleName</key><string>Classroom Teacher Agent</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSUIElement</key><true/>
<key>NSLocalNetworkUsageDescription</key><string>Connect to classroom Students and share or observe classroom screens.</string>
</dict></plist>
PLIST
APP_VERSION="$(python3 -c 'import pathlib,tomllib; print(tomllib.loads(pathlib.Path("../../Cargo.toml").read_text())["workspace"]["package"]["version"])')"
BUILD_NUMBER="$(sed -n 's/^version: .*+//p' pubspec.yaml)"
/usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $APP_VERSION" "$AGENT_APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUILD_NUMBER" "$AGENT_APP/Contents/Info.plist"
cat > "$ROOT/target/flutter-helper.entitlements" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict/></plist>
PLIST
# These packages were rebuilt above; missing packages are fatal, never optional.
mkdir -p "$BUILD_APP/Contents/Resources/student-packages"
cp "$STUDENT_PACKAGE" "$BUILD_APP/Contents/Resources/student-packages/student-windows-x64.zip"
cp "$ROOT/target/updates/student-macos-arm64.zip" "$BUILD_APP/Contents/Resources/student-packages/student-macos-arm64.zip"
python3 "$ROOT/scripts/sign-macos-app.py" "$BUILD_APP" \
  --identifier com.classroom.classroomConsole \
  --helper "$AGENT_APP" \
  --helper-entitlements "$ROOT/target/flutter-helper.entitlements" \
  --entitlements macos/Runner/Release.entitlements \
  "${SIGN_OPTIONS[@]}"
rm -rf "$APP"
cp -R "$BUILD_APP" "$APP"
codesign --verify --deep --strict "$APP"
echo "$APP"
echo "$GUARD_BUNDLE"

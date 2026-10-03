#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FLUTTER="${FLUTTER_BIN:-/Users/mvmanh/Development/flutter-sdk/bin/flutter}"
cd "$ROOT"
SIGN_MODE="${CLASSROOM_MAC_SIGN_MODE:-adhoc}"
case "$SIGN_MODE" in
  adhoc) SIGN_OPTIONS=(--development-ad-hoc); MANIFEST="$ROOT/target/teacher-development-signing.json" ;;
  apple) python3 scripts/sign-macos-app.py --check-identity; SIGN_OPTIONS=(); MANIFEST="$ROOT/target/teacher-signing.json" ;;
  *) echo "CLASSROOM_MAC_SIGN_MODE must be adhoc or apple" >&2; exit 1 ;;
esac
cargo build --release --bin Classroom
cd apps/classroom_console
"$FLUTTER" build macos --release
BUILD_APP="$ROOT/apps/classroom_console/build/macos/Build/Products/Release/classroom_console.app"
APP="$ROOT/target/Classroom Console.app"
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
cat > "$ROOT/target/flutter-helper.entitlements" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.app-sandbox</key><true/><key>com.apple.security.inherit</key><true/></dict></plist>
PLIST
python3 "$ROOT/scripts/sign-macos-app.py" "$BUILD_APP" \
  --identifier com.classroom.classroomConsole \
  --helper "$AGENT_APP" \
  --helper-entitlements "$ROOT/target/flutter-helper.entitlements" \
  --entitlements macos/Runner/Release.entitlements \
  --manifest "$MANIFEST" "${SIGN_OPTIONS[@]}"
rm -rf "$APP"
cp -R "$BUILD_APP" "$APP"
codesign --verify --deep --strict "$APP"
echo "$APP"

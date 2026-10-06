#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
SIGN_MODE="${CLASSROOM_MAC_SIGN_MODE:-adhoc}"
case "$SIGN_MODE" in
  adhoc) SIGN_OPTIONS=(--development-ad-hoc); MANIFEST="$ROOT/target/student-development-signing.json" ;;
  apple) python3 scripts/sign-macos-app.py --check-identity; SIGN_OPTIONS=(); MANIFEST="$ROOT/target/student-signing.json" ;;
  *) echo "CLASSROOM_MAC_SIGN_MODE must be adhoc or apple" >&2; exit 1 ;;
esac
env CARGO_TARGET_DIR="$ROOT/target" rustup run stable cargo build --release --bin ClassroomStudent
STAGING="$ROOT/target/student-macos-staging/Classroom Student.app"
OUTPUT_DIR="${CLASSROOM_MAC_OUTPUT_DIR:-$ROOT/target}"
mkdir -p "$OUTPUT_DIR"
APP="$OUTPUT_DIR/Classroom Student.app"
mkdir -p "$STAGING/Contents/MacOS"
cp target/release/ClassroomStudent "$STAGING/Contents/MacOS/ClassroomStudent"
cat > "$STAGING/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.classroom.student</string>
<key>CFBundleExecutable</key><string>ClassroomStudent</string>
<key>CFBundleName</key><string>Classroom Student</string>
<key>CFBundleDisplayName</key><string>Classroom Student</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>12.3</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>NSLocalNetworkUsageDescription</key><string>Connect to the classroom Teacher and receive or publish classroom screens.</string>
</dict></plist>
PLIST
python3 scripts/sign-macos-app.py "$STAGING" --identifier com.classroom.student \
  --manifest "$MANIFEST" "${SIGN_OPTIONS[@]}"
rm -rf "$APP"
cp -R "$STAGING" "$APP"
codesign --verify --deep --strict "$APP"
echo "$APP"

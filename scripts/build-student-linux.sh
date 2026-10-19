#!/usr/bin/env bash
# Native Linux Student development package. Does not build or publish Teacher.
set -euo pipefail
if [[ "${1:-}" == "--help" ]]; then
  printf '%s\n' 'Usage: bash scripts/build-student-linux.sh' \
    'Run on Ubuntu x86_64/aarch64 with Rust stable >= 1.89 and native build dependencies.' \
    'CLASSROOM_LINUX_OUTPUT_DIR overrides target/ubuntu-student.'
  exit 0
fi
if [[ $# -ne 0 ]]; then
  echo 'Unexpected arguments; use --help.' >&2
  exit 2
fi
if [[ "$(uname -s)" != Linux ]]; then
  echo 'Build this Student on Linux. This script does not cross-build from macOS or package Teacher.' >&2
  exit 1
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
for tool in cargo rustc cc c++ ar make pkg-config python3 tar sha256sum ldd; do
  command -v "$tool" >/dev/null || { echo "Missing build tool: $tool" >&2; exit 1; }
done
HOST="$(rustc +stable -vV | sed -n 's/^host: //p')"
case "$HOST" in
  x86_64-unknown-linux-gnu) ARCH=x86_64 ;;
  aarch64-unknown-linux-gnu) ARCH=aarch64 ;;
  *) echo "Unsupported native Linux target: $HOST" >&2; exit 1 ;;
esac
OUTPUT_DIR="${CLASSROOM_LINUX_OUTPUT_DIR:-$ROOT/target/ubuntu-student}"
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
STAGING="$(mktemp -d "$OUTPUT_DIR/.student-staging.XXXXXX")"
trap 'rm -rf -- "$STAGING"' EXIT
NAME="Classroom-Student-Ubuntu-$ARCH"
mkdir "$STAGING/$NAME"
python3 scripts/student-linux-build-info.py --target "$HOST" \
  --output "$STAGING/$NAME/build-info.json"
# An explicit target avoids accidentally honoring a Windows/macOS build.target.
BUILD_DIR="$ROOT/target/ubuntu-student-build"
CARGO_TARGET_DIR="$BUILD_DIR" cargo +stable build --locked --release \
  --target "$HOST" --package classroom-app --bin ClassroomStudent
BINARY="$BUILD_DIR/$HOST/release/ClassroomStudent"
# Cargo success alone does not establish that this host can load its libraries.
"$BINARY" --version
DEPENDENCIES="$(ldd "$BINARY")"
if [[ "$DEPENDENCIES" == *'not found'* ]]; then
  printf '%s\n' "$DEPENDENCIES" >&2
  exit 1
fi
cp "$BINARY" "$STAGING/$NAME/ClassroomStudent"
chmod 755 "$STAGING/$NAME/ClassroomStudent"
cp docs/28_UBUNTU_STUDENT.md "$STAGING/$NAME/README.md"
cp scripts/install-student-linux.sh "$STAGING/$NAME/install.sh"
chmod 755 "$STAGING/$NAME/install.sh"
printf '%s\n' "$DEPENDENCIES" > "$STAGING/$NAME/linked-libraries.txt"
python3 scripts/student-linux-build-info.py --target "$HOST" \
  --output "$STAGING/$NAME/build-info.json" --verify "$STAGING/$NAME/build-info.json"
(cd "$STAGING/$NAME" && sha256sum ClassroomStudent > SHA256SUMS)
tar -czf "$STAGING/$NAME.tar.gz" -C "$STAGING" "$NAME"
mv -f "$STAGING/$NAME.tar.gz" "$OUTPUT_DIR/$NAME.tar.gz"
printf 'Student development package: %s/%s.tar.gz\n' "$OUTPUT_DIR" "$NAME"
printf '%s\n' 'Teacher was not built or modified. LAN/runtime/performance acceptance remains separate.'

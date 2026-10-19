#!/usr/bin/env bash
# Per-user install of Classroom Student on Ubuntu (no root). Run from the extracted package.
#   ./install.sh [--teacher-ip IPV4] [--interface NAME] [--no-autostart] [--no-start]
#   ./install.sh --uninstall      (keeps the profile/settings in ~/.config/Classroom)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${CLASSROOM_INSTALL_DIR:-$HOME/.local/share/Classroom}"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/classroom-student.desktop"
LAUNCHER="${XDG_DATA_HOME:-$HOME/.local/share}/applications/classroom-student.desktop"
TEACHER_IP="" IFACE="" AUTO=1 START=1 UNINSTALL=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --teacher-ip) TEACHER_IP="${2:?}"; shift 2 ;;
    --interface) IFACE="${2:?}"; shift 2 ;;
    --no-autostart) AUTO=0; shift ;;
    --no-start) START=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) sed -n 2,5p "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [[ -n "$TEACHER_IP" && ! "$TEACHER_IP" =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
  echo "Teacher address must be an IPv4 address" >&2; exit 2
fi
if [[ -n "$IFACE" && ! "$IFACE" =~ ^[A-Za-z0-9_.:-]{1,15}$ ]]; then
  echo "Invalid interface name" >&2; exit 2
fi
stop_running() {
  # Only this installation's binary, matched by exact path (never other Students).
  pkill -f -- "^$DEST/ClassroomStudent( |\$)" 2>/dev/null || true
  for _ in $(seq 1 30); do pgrep -f -- "^$DEST/ClassroomStudent( |\$)" >/dev/null 2>&1 || return 0; sleep 0.2; done
  pkill -9 -f -- "^$DEST/ClassroomStudent( |\$)" 2>/dev/null || true
}
if [[ $UNINSTALL -eq 1 ]]; then
  stop_running
  rm -f -- "$AUTOSTART" "$LAUNCHER"
  rm -rf -- "$DEST"
  echo "Removed $DEST, autostart and launcher entries. Settings in ~/.config/Classroom were kept."
  exit 0
fi
[[ "$(uname -s)" == Linux ]] || { echo "Linux only" >&2; exit 1; }
[[ -f "$HERE/ClassroomStudent" ]] || { echo "Run this from the extracted package" >&2; exit 1; }
(cd "$HERE" && sha256sum -c SHA256SUMS >/dev/null) || { echo "Checksum mismatch; refusing to install" >&2; exit 1; }
ARGS=()
[[ -n "$TEACHER_IP" ]] && ARGS+=(--teacher-ip "$TEACHER_IP" --remember)
[[ -n "$IFACE" ]] && ARGS+=(--interface "$IFACE")
# Desktop Exec lines need quoting for spaces; install paths here never contain them
# unless the user overrides CLASSROOM_INSTALL_DIR, which is rejected.
[[ "$DEST" != *[[:space:]\"\'\\]* ]] || { echo "Install path must not contain spaces or quotes" >&2; exit 1; }
stop_running
mkdir -p "$DEST"
install -m 755 "$HERE/ClassroomStudent" "$DEST/.ClassroomStudent.new"
mv -f "$DEST/.ClassroomStudent.new" "$DEST/ClassroomStudent"
install -m 644 "$HERE/build-info.json" "$DEST/build-info.json" 2>/dev/null || true
EXEC="$DEST/ClassroomStudent ${ARGS[*]}"
EXEC="${EXEC% }"
mkdir -p "$(dirname "$LAUNCHER")"
cat > "$LAUNCHER" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Classroom Student
Comment=Receive the teacher's screen and classroom tools
Exec=$EXEC
Terminal=false
Categories=Education;
StartupNotify=false
DESKTOP
if [[ $AUTO -eq 1 ]]; then
  mkdir -p "$(dirname "$AUTOSTART")"
  cat > "$AUTOSTART" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Classroom Student
Exec=$EXEC
Terminal=false
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=5
NoDisplay=true
DESKTOP
fi
echo "Installed to $DEST"
echo "Autostart: $([[ $AUTO -eq 1 ]] && echo "$AUTOSTART" || echo disabled)"
if [[ $START -eq 1 ]]; then
  if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
    echo "No graphical session in this shell; it will start at the next login."
  else
    nohup "$DEST/ClassroomStudent" "${ARGS[@]}" >/dev/null 2>&1 < /dev/null &
    disown
    echo "Started."
  fi
fi

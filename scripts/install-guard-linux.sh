#!/usr/bin/env bash
# Installs the root Guard helper (Internet control) for Ubuntu Students. Needs root:
#   sudo ./install-guard.sh --uid <desktop user's numeric id>     (default: $SUDO_UID)
#   sudo ./install-guard.sh --uninstall
# The helper only obeys the Student running as that one user and removes every rule
# when that Student disconnects, so a crashed Student never leaves the network cut off.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB=/usr/local/lib/classroom
UNIT=/etc/systemd/system/classroom-guard.service
UID_ALLOWED="${SUDO_UID:-}"
UNINSTALL=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --uid) UID_ALLOWED="${2:?}"; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ $EUID -eq 0 ]] || { echo "Run with sudo" >&2; exit 1; }
if [[ $UNINSTALL -eq 1 ]]; then
  systemctl disable --now classroom-guard.service 2>/dev/null || true
  rm -f "$UNIT"; rm -rf "$LIB"; systemctl daemon-reload
  nft delete table inet classroom_guard 2>/dev/null || true
  nft delete table ip classroom_guard_nat 2>/dev/null || true
  echo "Guard helper removed."; exit 0
fi
[[ "$UID_ALLOWED" =~ ^[0-9]+$ && "$UID_ALLOWED" -ge 1000 ]] || { echo "Pass --uid <numeric desktop user id (>=1000)>" >&2; exit 2; }
command -v nft >/dev/null || { echo "nftables is required: sudo apt install nftables" >&2; exit 1; }
[[ -f "$HERE/ClassroomStudent" ]] || { echo "Run from the extracted package" >&2; exit 1; }
(cd "$HERE" && sha256sum -c SHA256SUMS >/dev/null) || { echo "Checksum mismatch" >&2; exit 1; }
install -d -m 755 -o root -g root "$LIB"
install -m 755 -o root -g root "$HERE/ClassroomStudent" "$LIB/ClassroomStudent"
cat > "$UNIT" <<UNITFILE
[Unit]
Description=Classroom Guard helper (Internet control)
After=network-online.target

[Service]
ExecStart=$LIB/ClassroomStudent --guard-helper --allow-uid $UID_ALLOWED
ExecStopPost=-/usr/sbin/nft delete table inet classroom_guard
ExecStopPost=-/usr/sbin/nft delete table ip classroom_guard_nat
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
UNITFILE
systemctl daemon-reload
systemctl enable --now classroom-guard.service
sleep 1
systemctl is-active classroom-guard.service
echo "Guard helper installed for uid $UID_ALLOWED. Restart the Student to use it."

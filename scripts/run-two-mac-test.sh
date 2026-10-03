#!/usr/bin/env bash
set -euo pipefail

# Build, deploy and collect a two-Mac Classroom run. The remote password is not
# accepted here: configure SSH key authentication once, then this script is
# non-interactive and safe to keep in the repository.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REMOTE_SSH="${CLASSROOM_REMOTE_SSH:-lolem@192.168.100.254}"
SSH_KEY="${CLASSROOM_SSH_KEY:-$HOME/.ssh/classroom_two_mac}"
DURATION="${CLASSROOM_DURATION:-60}"
SCENARIOS="${CLASSROOM_SCENARIOS:-baseline,loss}"
DROP_EVERY="${CLASSROOM_DROP_EVERY:-500}"
HEADLESS="${CLASSROOM_HEADLESS:-0}"
RUN_ROOT="${CLASSROOM_RUN_ROOT:-$ROOT_DIR/target/two-mac-runs}"

if [[ ! -r "$SSH_KEY" ]]; then
  printf 'SSH key not found: %s\n' "$SSH_KEY" >&2
  printf 'Set CLASSROOM_SSH_KEY or install the key used for the remote Mac.\n' >&2
  exit 2
fi
if ! [[ "$DURATION" =~ ^[1-9][0-9]*$ ]]; then
  printf 'CLASSROOM_DURATION must be a positive integer.\n' >&2
  exit 2
fi
if ! [[ "$DROP_EVERY" =~ ^[1-9][0-9]*$ ]] || ! [[ "$HEADLESS" =~ ^[01]$ ]]; then
  printf 'CLASSROOM_DROP_EVERY must be positive; CLASSROOM_HEADLESS must be 0 or 1.\n' >&2
  exit 2
fi
active_teacher_pid=""
active_ssh_pid=""
cleanup() {
  if [[ -n "$active_ssh_pid" ]]; then
    kill "$active_ssh_pid" 2>/dev/null || true
    wait "$active_ssh_pid" 2>/dev/null || true
  fi
  if [[ -n "$active_teacher_pid" ]]; then
    kill "$active_teacher_pid" 2>/dev/null || true
    wait "$active_teacher_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

SSH=(ssh -i "$SSH_KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=8)
RSYNC=(rsync -a -e "ssh -i $SSH_KEY -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=8")

REMOTE_HOST="${REMOTE_SSH#*@}"
REMOTE_HOME="$("${SSH[@]}" "$REMOTE_SSH" 'printf %s "$HOME"')"
REMOTE_DIR="${CLASSROOM_REMOTE_DIR:-$REMOTE_HOME/Classroom-two-mac}"
LOCAL_IP="${CLASSROOM_LOCAL_IP:-}"
if [[ -z "$LOCAL_IP" ]]; then
  route_output="$(route -n get "$REMOTE_HOST" 2>/dev/null || true)"
  local_interface="$(awk '$1 == "interface:" { print $2; exit }' <<<"$route_output")"
  if [[ -n "$local_interface" ]]; then
    LOCAL_IP="$(ipconfig getifaddr "$local_interface" 2>/dev/null || true)"
  fi
fi
if [[ -z "$LOCAL_IP" ]]; then
  printf 'Could not determine local IP. Set CLASSROOM_LOCAL_IP.\n' >&2
  exit 2
fi

"${SSH[@]}" "$REMOTE_SSH" "printf 'remote=%s arch=%s\\n' \"\$(hostname)\" \"\$(uname -m)\""
printf 'Building native macOS Classroom release...\n'
cargo build --release --bin Classroom --manifest-path "$ROOT_DIR/Cargo.toml"
LOCAL_BINARY="$ROOT_DIR/target/release/Classroom"
[[ -x "$LOCAL_BINARY" ]] || { printf 'Missing %s\n' "$LOCAL_BINARY" >&2; exit 1; }
xattr -d com.apple.quarantine "$LOCAL_BINARY" 2>/dev/null || true

"${SSH[@]}" "$REMOTE_SSH" "mkdir -p \"$REMOTE_DIR/bin\" \"$REMOTE_DIR/runs\""
"${RSYNC[@]}" "$LOCAL_BINARY" "$REMOTE_SSH:$REMOTE_DIR/bin/Classroom"
"${SSH[@]}" "$REMOTE_SSH" "xattr -d com.apple.quarantine \"$REMOTE_DIR/bin/Classroom\" 2>/dev/null || true; chmod 755 \"$REMOTE_DIR/bin/Classroom\"; test -x \"$REMOTE_DIR/bin/Classroom\""

RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$RUN_ROOT/$RUN_ID"
mkdir -p "$RUN_DIR"
printf 'local_ip=%s\nremote=%s\nrun=%s\n' "$LOCAL_IP" "$REMOTE_SSH" "$RUN_ID" | tee "$RUN_DIR/manifest.txt"

run_scenario() {
  local scenario="$1" drop_every=0
  if [[ "$scenario" == "loss" ]]; then drop_every="$DROP_EVERY"; fi
  if [[ ! "$scenario" =~ ^(baseline|loss|latejoin|disconnect|reconnect|fanout3)$ ]]; then
    printf 'Unsupported scenario: %s\n' "$scenario" >&2; return 2
  fi
  local remote_run_dir="$REMOTE_DIR/runs/$RUN_ID-$scenario"
  local local_log="$RUN_DIR/$scenario-teacher.log"
  local local_stdout="$RUN_DIR/$scenario-teacher.stdout.log"
  local remote_log="$remote_run_dir/student.log"
  local remote_stdout="$remote_run_dir/student.stdout.log"
  local teacher_pid
  local teacher_seconds="$((DURATION + 12))" effective_headless="$HEADLESS"
  if [[ "$scenario" == disconnect || "$scenario" == reconnect ]]; then
    if ((DURATION < 20)); then printf 'Lifecycle scenarios require duration >= 20.\n' >&2; return 2; fi
    teacher_seconds=8
  fi
  if [[ "$scenario" == fanout3 ]]; then effective_headless=1; fi
  "${SSH[@]}" "$REMOTE_SSH" "mkdir -p \"$remote_run_dir\""
  printf '\nStarting %s (%ss, drop_every=%s)...\n' "$scenario" "$DURATION" "$drop_every"
  local headless_flag=""
  if [[ "$effective_headless" == "1" ]]; then headless_flag="--headless"; fi
  local connect_delay=2000
  if [[ "$scenario" == latejoin ]]; then connect_delay=8000; fi
  # Keep SSH alive until Student exits. Detaching via nohup and closing SSH
  # caused macOS to deny Classroom local-network access on the remote machine.
  local remote_command="exec caffeinate -di \"$REMOTE_DIR/bin/Classroom\" --role student $headless_flag --student-connect-delay-ms '$connect_delay' --teacher-ip '$LOCAL_IP' --share-seconds '$DURATION' --log-file \"$remote_log\" >\"$remote_stdout\" 2>&1"
  if [[ "$scenario" == fanout3 ]]; then
    remote_command=""
    for ((index=0; index<3; index++)); do
      local port="$((47810 + index * 2))" name=student
      if ((index > 0)); then name="student-$index"; fi
      remote_command+="caffeinate -di \"$REMOTE_DIR/bin/Classroom\" --role student --headless --student-connect-delay-ms '$connect_delay' --teacher-ip '$LOCAL_IP' --media-port '$port' --feedback-port '$((port+1))' --share-seconds '$DURATION' --log-file \"$remote_run_dir/$name.log\" >\"$remote_run_dir/$name.stdout.log\" 2>&1 & classroom_pid$index=\$!; "
    done
    remote_command+="classroom_exit=0; wait \$classroom_pid0 || classroom_exit=\$?; wait \$classroom_pid1 || classroom_exit=\$?; wait \$classroom_pid2 || classroom_exit=\$?; exit \$classroom_exit"
  fi
  "${SSH[@]}" "$REMOTE_SSH" "$remote_command" &
  active_ssh_pid=$!
  # Student initializes first. Its duration starts after adapter discovery and
  # the optional join delay, so a slow OS inventory cannot exhaust Teacher's run.
  if ! "${SSH[@]}" "$REMOTE_SSH" "for classroom_attempt in {1..360}; do grep -q 'student UDP receive buffer ready' \"$remote_log\" 2>/dev/null && exit 0; sleep 0.25; done; exit 1"; then
    printf 'Student initialization timeout; see %s\n' "$remote_run_dir" >&2
    return 1
  fi
  "$LOCAL_BINARY" --role teacher --bind-ip "$LOCAL_IP" --share-seconds "$teacher_seconds" \
    --drop-every-nth-rtp-packet "$drop_every" --log-file "$local_log" \
    >"$local_stdout" 2>&1 &
  teacher_pid=$!
  active_teacher_pid="$teacher_pid"
  # Wait for the actual media listener rather than a blind startup delay.
  local ready=0 student_status=0 teacher_status=0
  for ((attempt=0; attempt<100; attempt++)); do
    if [[ -f "$local_log" ]] && rg -q 'teacher M3 media loop ready' "$local_log"; then
      ready=1; break
    fi
    if ! kill -0 "$teacher_pid" 2>/dev/null; then break; fi
    sleep 0.1
  done
  if [[ "$ready" != 1 ]]; then
    kill "$teacher_pid" 2>/dev/null || true
    wait "$teacher_pid" 2>/dev/null || true
    active_teacher_pid=""
    printf 'Teacher failed to become ready; see %s\n' "$local_stdout" >&2
    return 1
  fi
  if [[ "$scenario" == reconnect ]]; then
    wait "$teacher_pid" || teacher_status=$?
    active_teacher_pid=""
    sleep 2
    "$LOCAL_BINARY" --role teacher --bind-ip "$LOCAL_IP" --share-seconds "$((DURATION + 12))" \
      --log-file "$RUN_DIR/$scenario-restart.log" >"$RUN_DIR/$scenario-restart.stdout.log" 2>&1 &
    teacher_pid=$!
    active_teacher_pid="$teacher_pid"
  fi
  wait "$active_ssh_pid" || student_status=$?
  active_ssh_pid=""
  wait "$teacher_pid" || teacher_status=$?
  active_teacher_pid=""
  printf 'headless=%s\nstudent_exit=%s\nteacher_exit=%s\n' "$effective_headless" "$student_status" "$teacher_status" >"$RUN_DIR/$scenario.status"
  mkdir -p "$RUN_DIR/$scenario-remote"
  "${RSYNC[@]}" "$REMOTE_SSH:$remote_run_dir/" "$RUN_DIR/$scenario-remote/"
  : >"$RUN_DIR/$scenario.complete"
}

IFS=',' read -r -a scenario_list <<<"$SCENARIOS"
for scenario in "${scenario_list[@]}"; do run_scenario "$scenario"; done
python3 "$ROOT_DIR/scripts/analyze-two-mac-logs.py" "$RUN_DIR" | tee "$RUN_DIR/summary.txt"
printf '\nCollected run: %s\n' "$RUN_DIR"

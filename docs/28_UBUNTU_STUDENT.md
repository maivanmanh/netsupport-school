# Ubuntu Student — receive-only milestone

Owner authorized Ubuntu Student implementation on 2026-10-13. This extends the
Student platform scope; it does not replace the Windows/Ethernet performance
acceptance criteria in [06_PERFORMANCE_ACCEPTANCE.md](06_PERFORMANCE_ACCEPTANCE.md)
or authorize Ubuntu Teacher implementation.

## Tasks and scope

1. **Receive and display H.264:** reuse TCP registration/reconnect, RTP/H.264 UDP
   ingress, jitter/depacketization, NACK/keyframe recovery and the bounded decoded
   frame slot. Add a Linux software decoder and native presentation renderer.
   Initial display transport is X11, including XWayland when `DISPLAY` is present.
2. **Make the development build reproducible:** compile `ClassroomStudent` on
   Linux, produce a tar.gz with checksum/build metadata and document explicit
   Teacher address/profile configuration. No Flutter runtime is required.
3. **Validate on Ubuntu:** first registration and real Teacher video, then stop/
   restart/reconnect, late join, cursor/aspect ratio/resolution changes and loss
   recovery. Compare decode/render FPS and CPU with the same Teacher workload.
4. **Future tasks after the receiving path works:** native Wayland presentation,
   hardware decode, desktop launcher/autostart, monitoring/capture and explicitly
   authorized platform controls. Prioritize measured bottlenecks.

The first implementation uses bundled OpenH264 software decoding and an X11
minifb renderer. It does not require system FFmpeg or an installed H.264 codec.
Queues remain bounded; the renderer consumes the latest decoded frame. OpenH264
support is H.264 only; do not infer H.265 support from Windows/macOS capabilities.
XWayland is an X11 compatibility path, not native Wayland integration. Fullscreen,
focus and stacking behavior must be checked on the target desktop/compositor.

Capture/reverse publishing, Observe/control of the Ubuntu desktop, input locks,
app/Internet restrictions, remote terminal, hardware commands and LAN self-update
are outside this milestone. Ubuntu should not advertise unsupported capabilities.
The existing tar.gz is a manual development package, not the Teacher's bundled
**Share Student app** package. ELF detection in `classroom-dev-update.py` prepares
package identification only; it does not establish a working Linux updater.

## Native Ubuntu build

Build on an Ubuntu desktop or a Linux build host of the same architecture. Ubuntu
22.04/24.04 x86_64 are intended starting environments; ARM64 packages are built
separately and also require runtime validation. They are not yet certified.
Use a recent Rust stable toolchain (1.89 or newer for the current dependency set).
Install Rust using the normal Rust installation process, then:

```sh
sudo apt-get update
sudo apt-get install build-essential pkg-config python3 libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxdo-dev libxkbcommon-dev
rustup update stable
bash scripts/build-student-linux.sh
```

The script refuses non-Linux hosts and unsupported targets. It builds from the
current working tree using the lockfile, runs `ClassroomStudent --version`, checks
linked libraries and emits:

```text
target/ubuntu-student/Classroom-Student-Ubuntu-x86_64.tar.gz
```

The ARM64 name ends in `aarch64.tar.gz`. Set `CLASSROOM_LINUX_OUTPUT_DIR` to change
the output directory. The archive contains `ClassroomStudent`, `SHA256SUMS`,
`build-info.json`, `linked-libraries.txt` and this guide. Build metadata records
source revision and dirty state (both unknown/null for a non-Git copy), plus a
SHA-256 manifest of source files and actual Rust compiler version. Source is
fingerprinted before compilation and checked again before packaging; a concurrent
source change stops packaging. A dirty build is not represented as a clean
revision. The source manifest identifies a snapshot; retain that source alongside
the package to reproduce it. Native libraries/glibc are not bundled. Build on the oldest Ubuntu
release you intend to run, or rebuild on the destination release.

This script is a standalone Student development build. It does not build,
replace or publish a Teacher package. The owner's paired Teacher/Student build
rule remains mandatory for requested Teacher/release packages; this tar.gz does
not satisfy that deliverable. Existing Teacher packages/deployments are untouched.

## Launch on the Ubuntu desktop

Runtime requires a graphical user session and X11 libraries. Ubuntu Desktop
normally provides them; minimal installations may need `libx11-6 libxcursor1
libxrandr2 libxi6 libxdo3 libxkbcommon0`. For Wayland sessions, XWayland must be installed/running and
`DISPLAY` set. Otherwise use the login screen's **Ubuntu on Xorg** session.

Extract to a writable per-user application directory, check its checksum, then
launch from a terminal inside that graphical session:

```sh
tar -xzf Classroom-Student-Ubuntu-x86_64.tar.gz
cd Classroom-Student-Ubuntu-x86_64
sha256sum -c SHA256SUMS
./ClassroomStudent --teacher-ip 192.168.100.219 --remember
```

Replace the address with the actual Teacher's IPv4. An explicit address avoids
relying on room inference or a virtual adapter. To select a physical LAN adapter,
use `--interface enp3s0` (substitute the actual name from `ip -brief address`).
Teacher and Student must have LAN connectivity; do not expose classroom ports to
the public Internet. Default control port is TCP 47800 and media receive is UDP
47810, with feedback UDP 47811; inspect the actual config when ports are changed.

For an isolated test profile and writable log:

```sh
mkdir -p "$HOME/.config/Classroom-ubuntu-test"
./ClassroomStudent --teacher-ip 192.168.100.219 --remember \
  --launch-profile "$HOME/.config/Classroom-ubuntu-test/profile.toml" \
  --log-file "$HOME/.config/Classroom-ubuntu-test/log.txt"
```

The `profile.toml` argument identifies the profile directory; current settings
and stable device identity are stored in that directory's Classroom store.
Default Linux profile directory is `$XDG_CONFIG_HOME/Classroom`, or
`$HOME/.config/Classroom` when the XDG variable is unset. Keep the directory across
binary upgrades. Do not run two Students with the same profile concurrently.
The process waits in the background until Teacher starts a presentation; stop
this terminal-launched development instance with Ctrl+C. No system service or
automatic startup is installed.

`--diagnose` checks configuration/adapter selection and exits. `--headless`
receives/decodes without a presentation window; it cannot verify rendering.

## Verification status and next gate

Verified on 2026-10-13 against the owner's Ubuntu **22.04 ARM64** Parallels VM
(`10.211.55.5`), from an isolated `/tmp/classroom-ubuntu-validation` workspace:

- Five real H.264 decoder tests and three surface/pointer tests passed natively.
- A native X11 query confirmed the background host stays unmapped while pumping.
- The real receiver pipeline passed active late join, UDP/depacketize/decode/
  native render, Stop/Start on one control connection, and disconnect teardown.
  Both streams had actual received, decoded and rendered counters. This ran both
  under Xvfb and as user `parallels` on the existing GNOME Wayland desktop through
  XWayland. The source was a synthetic loopback Teacher with 64×48 H.264 fixtures.
- The actual ARM64 Student executable was built and passed `--version`,
  `--diagnose --loopback` and linked-library checks.
- Linux x86_64 compile checks, Windows/macOS role compile checks, strict new-crate
  Clippy, 409 workspace tests (including macOS codec tests run with VideoToolbox
  access), and four Python packaging tests passed. Existing full-file formatting
  issues outside the added code and existing app/vendor lint warnings remain.

The VM runs from a slow disk. The owner explicitly requested **functional checks
only**, with no performance assessment. Cold display startup is completed before
TCP registration; a hidden native host retains the display across sessions.
The smoke sender waits for an actual render report before ending each stream,
with bounded deadlines. Initial fixed-duration sender and cold-display failures
are retained in the checkpoint; they are not reported as runtime passes.

No Teacher release package or Ubuntu release tar.gz was shipped by this source
implementation task. The native packaging script has syntax/host-guard/source
fingerprint/ELF tests; its release-build/archive path still needs a native run.
No existing Teacher/Student deployment, profile, privacy grant or release asset
was replaced. The VM compiler/test directories are temporary validation artifacts.

Follow-up functional LAN validation on 2026-10-13 **passed** the complete Student
agent against the existing macOS Teacher: real 1512×982 screen capture, idle
registration/Start, Stop, active late join, Teacher crash/restart reconnect,
silent-control timeout/recovery and a three-minute session. It used an isolated
room and a five-FPS test profile on the slow VM, with no performance assertions.
See [functional evidence and limits](../checkpoints/diagnostics/2026-10-13-ubuntu-student-lan.md).

Validation on **Ubuntu 26.04 ARM64** (2026-10-19, VM `Ubuntu 26.04 ARM64`, 2 vCPU,
Shared NIC `eth1` 10.211.55.12; the bridged Wi-Fi NIC could not reach the Mac Teacher
reliably): native `build-student-linux.sh` succeeded (Rust 1.99.1), codec/renderer
tests passed (including the Xvfb renderer test) and `--diagnose` passed. The real
macOS Teacher (1512×982 capture) drove the full functional harness on an owned Xvfb
(`--xvfb`): first screen, Teacher report, Stop, active late join, Teacher restart,
silent-Teacher release and soak passed at 5 FPS and at the product 30 FPS profile.
At 30 FPS: decode/render ≈ 30 FPS (brief 24–26 FPS dips), 0 missing packets, decode
p95 ≈ 10–13 ms, Student CPU ≈ 34 % of one core, RSS ≈ 120–140 MB. Content was mostly
static (0.1–3.6 Mbps), so this is not a 1080p/heavy-motion or wired-LAN result.
Not covered: real GNOME session (26.04 is Wayland-only; fullscreen/ABOVE through
XWayland), visual QA, display changes. Harness flags: `--xvfb`, `--fps`,
`--student-source`, `--student-interface`.

Next gate: desktop launcher/per-user installation and graphical-session autostart,
then Ubuntu bootstrap/updater integration. Visual cursor/text/aspect QA and display
mode changes remain pending. Linux x86_64 runtime,
Ubuntu 24.04 and native Wayland remain unverified. Capture/Observe and OS controls
remain separately scoped Linux backends. These functional checks do not
prove 1080p performance or any 40-PC acceptance result.

Backend references: [OpenH264 decoder API](https://docs.rs/openh264/0.9.8/openh264/decoder/struct.Decoder.html),
[EWMH fullscreen/stacking](https://specifications.freedesktop.org/wm/latest/ar01s05.html).

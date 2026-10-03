# Master prompt for Codex / coding agent

You are taking ownership of the initial implementation of a native classroom screen-sharing system. The repository contains the authoritative design package. Read **all** of the following before writing production code:

- `AGENTS.md`
- `project_spec.yaml`
- every document under `docs/`
- `config/default.toml`

Do not ask the human owner to repeat information already written in those files.

## Product context

The product is a native classroom-management application inspired by the core capabilities of NetSupport School, but Phase 1 has one dominant objective: a teacher PC must be able to broadcast its screen to approximately 40 student PCs in the same wired classroom LAN at up to 1920×1080, with smooth presentation, low latency, readable code/text, and adaptive quality. There is no dedicated classroom server. The teacher machine is the coordinator and source/distributor.

The user develops primarily on macOS but the real deployment target is Windows. The current best laboratory uses Windows 10 Education 22H2, 16 GB RAM, Intel Core i7-14700 CPUs, SSDs, integrated Intel graphics rather than discrete GPUs, one 1920×1080 monitor per machine, and wired 1 Gbps Ethernet. Other rooms may have older/weaker i5/i7 machines, so the program cannot rely on the best-room hardware.

The network has a room addressing convention such as teacher `10.0.55.200` and students `10.0.55.11`, `.12`, etc. A different room may use another third octet, e.g. teacher `10.0.63.200`. The observed subnet mask is `/20`. Virtual adapters from Hyper-V and VMware are installed on some PCs, therefore network-interface selection must be robust and should prefer the physical interface on the default route rather than the first matching IPv4 address.

No audio is required. No Internet dependency is allowed. Security hardening is not a Phase 1 priority. The end product is a native desktop application; do not use a web application architecture.

## Required architectural direction

Implement a modular Rust workspace. Keep platform-independent interfaces separate from Windows-specific implementations so macOS can be added later, but do not implement macOS in Phase 1.

The intended media path is:

Teacher:

`DXGI Desktop Duplication -> frame analysis/conversion -> H.264 encode once -> RTP/H.264 packetization once -> UDP unicast fan-out to all registered students`

Student:

`UDP receive -> small jitter buffer -> RTP/H.264 depacketization -> H.264 decode -> native render`

The initial encoder/decoder backend should use Windows Media Foundation. Probe for a usable hardware H.264 encoder MFT and use it when available; otherwise use software Media Foundation encoding. This capability detection is an optimization mechanism, not a functional dependency. The codebase must expose a backend interface so x264/OpenH264/other codecs can be evaluated later without redesigning the application.

The control path should initially use a straightforward reliable protocol: length-prefixed Protobuf messages over TCP. Do not introduce WebRTC, ICE, STUN, TURN, cloud signalling, or a separate media server. Those solve problems this LAN-only Phase 1 does not have.

Multicast is not required in Phase 1. Design `MediaDistributor` so a multicast backend can be added later, but implement unicast fan-out first. The teacher must encode once regardless of the number of students.

## Application shape

Prefer one binary for the prototype:

`Classroom.exe`

On launch, show a small native role selector (Teacher / Student) and permit equivalent CLI role selection for testing. Keep UI minimal. The media core must not depend on the chosen UI toolkit.

The program should be usable as a portable executable during development. Configuration must have embedded defaults but allow override from an adjacent or user configuration file and/or command-line arguments. Do not bake room A505 addresses into the code.

## Phase 1 functional scope

Implement only enough UI/control to exercise the media system:

Teacher:
- choose Teacher role,
- show selected active network adapter/IP,
- show/override teacher bind address and ports,
- show connected student roster,
- Start Sharing / Stop Sharing,
- choose quality preset or Auto,
- show basic runtime metrics/logs.

Student:
- choose Student role,
- resolve or manually enter the teacher address,
- register with teacher over TCP,
- receive presentation start/stop messages,
- receive/decode/render the teacher stream,
- report receiver health/metrics,
- reconnect gracefully if teacher/control connection restarts.

Do not implement file transfer, screen-grid monitoring, remote control, app blocking, Internet blocking, keyboard/mouse locking, Windows service, or macOS yet. Preserve interfaces and clean boundaries for them.

## Performance model and invariants

The program is not a video-conferencing clone. It is a one-to-many LAN classroom broadcast system. Optimize for that topology.

The teacher must perform:

- one capture,
- one encode,
- one packetization,
- N sends of the same encoded RTP packet to N student endpoints.

Do not create one encoder per student.

Use a latest-frame-wins policy. The capture pipeline must not build a queue of stale frames. A bounded single-slot handoff between capture and encode is preferred. If the encoder is temporarily slower than capture, replace/drop the waiting old frame rather than increasing latency.

H.264 should run in a low-latency configuration. Avoid B-frames. Use a sensible GOP/keyframe interval, cache codec parameter sets, support an immediate keyframe request, and make late joining reliable.

Classroom content is often static slides, code, IDEs, browsers, and terminals. Use DXGI dirty-region/move information where practical and implement/adapt a frame scheduler so static scenes can be transmitted at lower effective FPS while fast scrolling/motion can rise toward 30 fps. Keep thresholds configurable and evidence-driven.

Quality adaptation priority is:

1. protect freshness/latency,
2. preserve 1080p text readability where possible,
3. reduce bitrate and frame rate under pressure,
4. reduce resolution only after those steps are insufficient.

Initial profile: 1920×1080, max 30 fps, approximately 6 Mbps target, configurable about 1.5–8 Mbps. These are starting values to validate, not sacred constants.

Do not allow one pathological client to degrade the entire class automatically. Aggregate/global quality decisions should use teacher load and robust receiver statistics (for example median/percentiles), while an individual unhealthy client can be reported separately.

## Network details

Default configuration example:

- teacher address: `10.0.55.200`
- control port: choose a configurable high port and document it,
- student media port: one configurable high UDP port shared across hosts,
- optional feedback port or multiplexed feedback design: document your decision.

Student teacher-address inference can use `10.0.<local-third-octet>.200` as a convenience rule. Always support explicit override.

Do not rely on broadcast discovery because the observed mask is `/20` and institutional topology may span multiple rooms. Do not scan the whole `/20` network.

Select the appropriate local interface using the active default route/gateway and operational Ethernet adapter. The machine may contain Hyper-V and VMware virtual Ethernet adapters. Surface the chosen adapter in the UI/log and allow the user to override it.

## Loss/recovery behavior

Use RTP-style sequence/timestamp semantics. A small jitter buffer (roughly 30–50 ms initially) should absorb normal scheduling variation without turning into a large latency buffer.

Implement or design for:

- missing sequence detection,
- late packet discard,
- short-term retransmission cache on the teacher,
- receiver NACK for packets still useful to recover,
- keyframe/IDR request when the receiver cannot recover a valid reference chain,
- late join during an already-running presentation.

Do not retransmit indefinitely and do not allow recovery traffic to make the receiver show stale content.

## Observability and benchmarking

Metrics are part of the implementation, not an optional later task. Emit structured logs and, if convenient, a simple metrics panel.

At minimum collect teacher-side capture/encode/network metrics and student-side receive/decode/render/loss metrics listed in `AGENTS.md` and `docs/06_PERFORMANCE_ACCEPTANCE.md`.

The human owner's most important question is not merely whether a picture appears, but whether the system remains smooth and low-latency at 40 clients. Therefore build a repeatable benchmark mode or test procedure early.

## Required milestone order

Implement in this order and do not skip directly to feature expansion:

### M0 — repository/platform skeleton
- Rust workspace and modules/interfaces.
- config and logging.
- role selection/CLI.
- Windows-specific build path.
- robust active-adapter selection.
- no fake implementation of later features.

### M1 — capture benchmark
- DXGI Desktop Duplication at 1920×1080.
- include cursor.
- handle timeout/no-change frames correctly.
- recover from duplication access loss/display changes.
- measure capture time/FPS/change ratio.

### M2 — local encode/decode/render benchmark
- Media Foundation H.264 backend.
- hardware MFT probe + software fallback.
- low-latency settings.
- frame conversion.
- local loop: capture -> encode -> decode -> render.
- prove sustained 1080p30 capability on the target PC or explain the measured bottleneck.

### M3 — one teacher to one student
- TCP control registration.
- RTP/H.264 over UDP.
- jitter buffer.
- render teacher screen remotely.
- measure end-to-end latency using a reproducible method.

### M4 — resilience
- late join.
- student reconnect.
- NACK/retransmission for useful recent loss.
- keyframe request/recovery.
- bounded queues and verified freshness under temporary overload.

### M5 — scale fan-out
- 1 -> 5 -> 10 -> 20 -> 40 students.
- still exactly one encoder instance for the teacher stream.
- report CPU, memory, NIC Mbps, encode p95, loss, receiver decode/render health.

### M6 — adaptive quality
- runtime bitrate changes when supported.
- adaptive FPS based on motion/dirty area and load.
- pressure response based on encode latency/network/receiver aggregate.
- resolution reduction only as a later fallback.

Only after M6 passes the acceptance gate may you start optional Phase 2 features.

## Acceptance targets

Use `docs/06_PERFORMANCE_ACCEPTANCE.md` as authoritative. High-level desired behavior:

- up to 40 concurrent students,
- 1920×1080 maintained where the environment can support it,
- up to 30 fps, adaptive rather than rigid,
- p95 presentation latency goal <= 200 ms on healthy wired LAN, with approximately 100–150 ms preferred,
- no multi-second queue buildup,
- no encoder count growth with viewer count,
- no Internet/cloud requirement,
- graceful degradation rather than freezing.

Do not falsify or approximate benchmark results. If a target is not reached, record the measured result and bottleneck.

## Architecture for later features — preserve, do not implement

The future system will need:

- student screen thumbnail grid refreshing roughly every 1–3 seconds,
- click a student to request a higher-quality live preview,
- promote a selected student to presentation source and broadcast that student's screen using the same reusable publisher/distributor pipeline,
- file transfer,
- Windows privileged service for app control, Internet control, keyboard/mouse locking, process protection,
- macOS capture/codec backends.

Therefore model presentation source generically (teacher or selected student) and keep capture/encoder/publisher interfaces reusable. Do not hard-code “teacher is the only possible source” deep inside the media engine.

## Working style and checkpoints

Before implementing a milestone, briefly state the implementation plan and files expected to change. Then implement, build/test, and update `checkpoints/PROGRESS.md` with:

- milestone status,
- exact build/test commands,
- what was verified,
- measured metrics when relevant,
- deviations from architecture and why,
- known defects/risks,
- exact next step.

Prefer small verifiable increments. If a design assumption is uncertain, create a focused spike/benchmark rather than burying uncertainty inside a large implementation.

Do not ask the human to make low-level implementation decisions that can be resolved by measurement or Windows documentation. Escalate only choices that materially change product behavior or the locked architecture.

Your first task after reading the package is to propose the concrete Rust workspace/module structure and M0 implementation plan, then begin M0.

# 08 — Future roadmap (architecture awareness only)

These features must **not** distract from Phase 1, but the core design should not make them unnecessarily difficult.

## Student monitoring grid

Desired future behavior:

- teacher sees all ~40 machines in a grid,
- default refresh configurable around 1–3 seconds,
- lightweight thumbnails (for example ~480×270 compressed image) rather than 40 continuous high-quality streams,
- click a machine to request higher-quality/live preview,
- close preview to return that student to thumbnail mode.

## Present a student to the class

A selected student later becomes a `PresentationSource`.

Conceptually:

`Student capture -> H.264 publisher -> teacher/coordinator routing -> class subscribers`

Reuse the same capture/codec/RTP/distributor contracts. Do not build an entirely separate “student sharing” stack.

## File transfer

Future reliable transfer can use a dedicated control/data channel. It must not share the real-time video queue or block media/control heartbeats.

## Privileged classroom control

Future features:

- lock keyboard/mouse,
- lock/blank student display,
- block/allow apps,
- block Internet while keeping classroom LAN control alive,
- protect the student agent from ordinary termination,
- auto-start.

These should eventually be implemented using a privileged Windows Service plus IPC to the user-session application. Internet blocking should use an appropriate Windows filtering/policy mechanism rather than simply disabling the NIC, because classroom LAN communication must continue.

## macOS

Add platform backends later, e.g. ScreenCaptureKit/VideoToolbox, behind the same capture/codec interfaces. Do not allow macOS portability work to slow Windows Phase 1.

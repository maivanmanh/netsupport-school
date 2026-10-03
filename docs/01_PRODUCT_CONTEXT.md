# 01 — Product and deployment context

## Product direction

The application is intended to grow into a native classroom-management product with capabilities conceptually similar to the core lab-management functions of NetSupport School. The immediate implementation must remain deliberately narrower.

The Phase 1 success condition is teacher-to-class screen broadcast. If this is not smooth and measurable at 40 clients, other features do not compensate for that failure.

## Deployment environment

The primary deployment is a university computer laboratory:

- one teacher PC,
- approximately 40 student PCs,
- all wired Ethernet,
- 1 Gbps NICs,
- Cisco switching infrastructure,
- no dedicated classroom server,
- Internet normally available but the application must continue to work when Internet is disabled centrally,
- Windows 10 Education 22H2 currently used,
- administrative rights are available on both teacher and student machines.

The strongest current lab has 16 GB RAM and Intel Core i7-14700 processors. There are weaker rooms, so the implementation must not assume this CPU everywhere.

The ideal reference room uses one 1920×1080 display per machine. Other Teacher and Student resolutions are supported; Windows capture fits the source into a maximum 1920×1080 bounding box while preserving aspect ratio within even-pixel rounding and without upscaling. Phase 1 optimizes for a single display. Preserve an API shape that can later expose multiple displays/windows without implementing them now.

## Network addressing

Reference room:

- teacher: `10.0.55.200`
- students: `10.0.55.11`, `.12`, `.13`, ...
- mask: `255.255.240.0` (/20)
- gateway shown: `10.0.48.1`

Another room can use the same convention with a different third octet, e.g. `10.0.63.200` for the teacher.

Because `/20` is broad and institutional network boundaries are not fully known, do not use broad broadcast discovery or assume that IP subnet equals one physical classroom.

## Installed adapters

Some PCs contain:

- physical Intel Ethernet,
- Hyper-V virtual Ethernet,
- VMware VMnet1/VMnet8,
- Windows WAN miniports.

Interface selection must identify the active routed physical Ethernet path instead of taking the first address returned by the OS.

## Development/deployment preference

Development is often done on macOS. Windows binaries may be cross-built from macOS, but native Windows testing is mandatory for DXGI, Media Foundation, drivers, NIC behavior, and performance.

For development convenience, one portable executable that can run as Teacher or Student is preferred. The production installer split can be added later.

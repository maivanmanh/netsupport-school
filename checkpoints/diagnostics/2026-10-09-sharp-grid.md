# Compact grid and sharper previews — 2026-10-09

Owner requested an overlay footer and readable larger previews before assignment
work. Grid cards now put title/IP/actions on an 84%-opaque white bottom overlay;
removed the separate 64px layout reservation. List layout remains horizontal.
Compact icon constraints preserve room at small widths. Preview uses aspect-fit
and high-quality display filtering.

Investigation found a 480×270 capture ceiling, 240/360/480 wire tiers, JPEG65 and
a direct quality30 fallback above 48KiB. At Retina display scale, the largest
preview lacked source pixels; enlarging the widget alone cannot restore text.

Optional ThumbnailHello.max_width advertises 960px. Teacher maps existing logical
tiers to 480/720/960 for capable Students, or keeps legacy 240/360/480 requests.
New Student capture is stable/bounded 960×540, aspect-preserving and never
upscaled. Teacher asks quality85; initial JPEG uses 4:4:4, then quality fallback
75/65/50/30 only as needed. Requests above legacy bounds permit 192KiB instead
of 48KiB. Monitoring TCP framing alone grows to 193KiB; reliable control framing
is unchanged. Decoder verifies JPEG dimensions and limits decoded storage.
Each local IPC publication includes at most 4MiB of raw JPEG (before base64),
leaving room under its existing 8MiB frame ceiling. Omitted sequences stay unsent
and are eligible on the next publication, retaining only each peer's latest image.

Refresh cadence and one-at-a-time pull requests are unchanged. Maximum 40-peer
JPEG payload at two-second intervals is about 31.5Mbps before overhead; this is
arithmetic, not measured throughput. CPU, readability and wired 35–40-PC effect
remain to be measured. Presentation encode-once/fan-out path is untouched.

Flutter format/analyze and native Teacher/Student cargo check passed. No tests
added or run. Existing hello test fixture supplies default for new optional field.
Paired `paired-sharp-grid-20261009` build completed after the final IPC bound
change. Teacher replaced/reopened at its fixed path; embedded Mac/Windows ZIPs
byte-match fresh outputs. Both Students updated through Teacher and confirmed
the new build; updater preserves settings and removes the obsolete build.
Both endpoints are online. Windows snapshots received at 860×540 (source aspect
ratio), about 136KiB, with decode around 22ms in the sampled log; this proves
expanded preview delivery, not classroom performance or text readability.

Mac capture is blocked by TCC after signing; log explicitly says user declined
display capture. Paused Mac verification for owner to grant Screen Recording to
`/Users/lolem/Classroom/Classroom Student.app` on `192.168.100.254`, followed by
Student restart. No privacy settings automated. Visual grid review and readability
at all three tiers remain manual; wired 35–40-PC load remains deferred.

# AI and chatbot websites (2026-10-16)

## What the Teacher gets

- **Ribbon → Kiểm soát lớp → Website AI** opens the list (56 services to start with, see
  `apps/classroom/src/ai_sites.rs`): search, add, edit, delete, and
  **Khôi phục danh sách gốc**, which restores exactly the shipped list after anything
  was deleted. The list belongs to the Teacher and is kept across days.
- A **switch** blocks every site in the list on all Student computers. Like every other
  class rule it lasts until the end of the local day (Students drop it at midnight).
- With **Theo dõi hoạt động** on, a Student who has any listed site open in any browser
  tab - front or not, switch on or off - is highlighted on the tile in grid, list and
  compact views (orange border, tinted row and an "AI · site" chip). This is a warning
  only.

## How blocking combines with the Internet rule

A Student still receives one Internet rule. The Teacher sends the combination of its own
Internet rule and the AI list, so a site is blocked when **either** asks for it, and a
conflict is always settled by blocking:

| Teacher's Internet rule | What the Student gets with AI blocking on |
| --- | --- |
| Open | block only the AI list |
| Block listed sites | the listed sites plus the AI list |
| Allow only listed sites | the allow-list with every AI site (and any pattern wide enough to let one through) removed; if nothing is left, everything is blocked |
| Block everything | unchanged |

Switching it off releases the Students that were only being blocked for this reason.
Late-joining and restarted Students receive the same combination.

## Limits to know

- Only host names can be blocked at network level (HTTPS hides paths). Entries written
  with a path - `github.com/features/copilot`, `adobe.com/products/firefly.html`,
  `notion.so/product/ai` - therefore block **the whole site**; the list marks them
  "cả trang" and the Teacher can delete them.
- Blocking needs a Student that can enforce rules (Windows, running with the privileges
  the Internet feature already needs). Others are left alone, as with the Internet rule.
- Detection uses the page addresses the Student's browser exposes. The front tab's
  address is exact; other tabs are matched through browser history by title and may be
  missed.

## Roster tile content

Per Student, most important first: the AI warning, the page a browser shows (a browser
that is open but not in front still shows its page, marked as running in the
background), then up to the configured number of application icons (Settings → Theo dõi
hoạt động). Grid, list and compact rows each arrange these for the room they have.

## Evidence screenshots (2026-10-16)

While **Theo dõi hoạt động** is on and AI websites are **not blocked**, a Student who
uses a listed site is photographed a few times, as proof for the Teacher.

**When the picture is taken** (`apps/classroom/src/activity/evidence.rs`, all on the
Student, so it reacts within seconds):

- only while the site's tab is the one in front of the front browser window, and the
  Student has touched keyboard or mouse in the last 20 s - otherwise the picture would
  not show it, or nobody is there;
- the first picture waits for signs of real use: the page changed after loading (new
  title or address: a conversation started) and the Student has been there 15 s; or, if
  the page never changes, 60 s of use. A page that has only just loaded (login box, empty
  chat) is therefore not photographed;
- later pictures of the same site need another 3 minutes of use and a changed page (or
  twice that time), so they show different content instead of replacing each other;
- at most 3 pictures per site per Student per day (the count survives restarting the
  Student program); time spent in other windows is not counted.

The numbers are settings (Cài đặt → Theo dõi hoạt động; the timings under *Nâng cao*).
A site listed with a path (`github.com/features/copilot`) is only watched under that
path, so ordinary GitHub use is neither flagged nor photographed.

**What happens to it:** the Student takes the screenshot, shrinks it to a JPEG of a few
hundred KB and sends it to the Teacher over its own short connection; the Teacher checks
it (listed site, allowance, size, checksum, JPEG, free disk) and stores it under its
profile folder `evidence/<yyyymmdd>/`. The folders of earlier days are deleted at
start-up and at midnight, so evidence never accumulates; the viewer has *Lưu vào
Desktop* for anything to keep. A day holds at most 800 pictures / 1 GB.

**What the Teacher sees:** a toast ("PC-07 đang dùng chatgpt.com — đã lưu ảnh minh
chứng") with *Xem*; an orange camera count on the computer's tile (stays even after the
Student closes the tab); and *Website AI → Minh chứng hôm nay* for all of them. Pictures
open full size with the page title, time and reason, and can be saved or deleted.

Not covered: Students that cannot report the front window's page (non-Chromium
browsers), and Students running an older build (they simply take no pictures).

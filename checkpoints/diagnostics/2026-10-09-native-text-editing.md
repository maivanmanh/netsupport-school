# Editable Rust text fields and right-aligned roster controls — 2026-10-09

Owner requested normal in-place editing everywhere Rust text inputs are used.
Audit found three InputCallback consumers: launcher/legacy role settings,
Student operational settings, and Student message reply. Existing settings
selected the entire value on click, then cleared it on typing; deletion only
removed the end. Reply also only appended/popped.

Added shared bounded in-memory Editor: UTF-8 boundary cursor/anchor, measured
click placement, drag/Shift selection, Left/Right and word movement, Home/End,
Backspace/Delete and word deletion, Ctrl/Cmd+A/C/X/V, 32-step undo/redo. Reply
supports Up/Down over wrapped lines and keeps caret in the visible three-line
viewport. Fields retain existing limits and validation (numeric IP/port fields
filter pasted text; reply stays bounded to 500 Unicode scalar values). Tab can
select the current value, while pointer focus no longer selects everything.
Enter/Save/Send semantics are retained. Positions are Unicode scalar boundaries;
this is not a full platform text-services/IME or grapheme-cluster implementation.

Native clipboard read complements existing copy_text in vendored minifb:
Windows CF_UNICODETEXT handle is locked/read/copied/unlocked while clipboard is
open; macOS NSPasteboard copies bounded UTF-8 into owned Rust storage. Clipboard
reads are limited to 64KiB (Windows 65536 UTF-16 units), paste is further bounded
by the target field; failure preserves text. No shell clipboard processes and no
clipboard contents written to logs/disk. Undo history remains only in memory.

Teacher toolbar separates business tools from filter/Rescan/selection/view group;
the latter is anchored right in wide and two-row layouts. Business tools can
scroll horizontally without pulling roster controls away from the right edge.

Native Mac cargo check and Flutter analyze/format passed. No tests added/run.
Paired `paired-native-editor-20261009` build completed (Mac and Windows x64),
Teacher replaced at fixed path; both embedded ZIPs match fresh outputs. Both
Students updated via Teacher, confirmed new build and are online. Actual input, clipboard and drag-selection
interaction on Windows/Mac remain manual verification; performance acceptance
on 35–40 physical PCs is unrelated and remains deferred.

Mac Student remains online but capture log explicitly reports TCC denial after
signing. Paused Mac capture verification for owner manual Screen Recording grant
to `/Users/lolem/Classroom/Classroom Student.app` on `.254`, followed by Student
restart. No privacy grants changed automatically; text editing itself does not
require Screen Recording.

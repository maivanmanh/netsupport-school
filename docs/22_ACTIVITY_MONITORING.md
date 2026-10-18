# Activity monitoring

(The code refers to this file for the full design; only the live roster strip is
documented here so far.)

## Live strip on the roster (2026-10-16)

While *Theo dõi hoạt động* is on, each tile (grid, list, compact) shows the front
three applications as icons and, when a browser is in front, the active tab's address
with its site icon. The Student sends a compact summary (`activity_live`, a few
hundred bytes, every 3 s when it changes and at least every 12 s) over the existing
tools connection; program icons go once per connection and are cached by the
Teacher. Only the address is reported: the Teacher fetches the site icon itself.
Summaries older than 30 s, or any while monitoring is off, are not shown. Requires an
updated Windows Student.

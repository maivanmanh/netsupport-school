# Open website toolbar and suggestions — 2026-10-09

Owner confirmed the website feature works and requested UI refinements.

- Toolbar Open website now uses the same compact 18px IconButton styling as
  adjacent tools. Removed visible label and outlined button; tooltip retains
  the action and audience.
- URL field uses Flutter RawAutocomplete over the persisted recent websites.
  Matches case-insensitively anywhere in the address, ignoring HTTP/HTTPS prefix;
  shows at most six suggestions. Clicking only fills the textbox and preserves
  focus. Open button remains the explicit dispatch action. Keyboard suggestion
  navigation and Enter submission are supported. Existing recent-list quick
  opening/removal and durable history are preserved.
- Flutter format/analyze passed; no tests added or run. This is Teacher UI only;
  paired packages are rebuilt per policy, with running Students kept on their
  compatible version to avoid unnecessary permission churn.

Paired build `paired-url-suggestions-20261009` completed. Teacher replaced and
reopened at the existing `target/present-recovery-build/Classroom Console.app`
path; both embedded Student ZIPs byte-match fresh build outputs. Compatible
Students were not redeployed; Mac `.254` and Windows `.155` are online. Saved
URL history (one entry) matches its pre-restart hash. No privacy grants changed.

Manual autocomplete interaction and physical classroom scale remain unverified.

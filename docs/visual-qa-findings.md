# Visual QA findings — 2026-10-30

Status: recorded, not fixed. Local findings, not published tracker issues.

## Reproduction environment

- valkey-manager 0.3.0 release build; eframe 0.36.2, glow, native Wayland.
- Private KWin 6.8.1 `--virtual` session, real NVIDIA GPU (EGL probe in the same
  session). Fresh private HOME/XDG; no saved profiles; no Secret Service.
- Server: disposable `valkey/valkey:9-alpine` on `127.0.0.1:16479` with 25 synthetic
  `qa:user:N` strings and one `qa:profile` hash.
- Frames captured and inspected; screenshots stay in the operator's private scratch area.

## VQA-VALKEY-001 — Main panel is wider than the default window

**Priority:** medium. **State:** open; default window (1x), connected state.

**Reproduce:** connect to the local server; open Keys and Console.

**Observed:** the right edge is clipped: the Scan button reads `Sca`, the console Run
button reads `Ru`, and the key-count badge touches the edge. The disconnected empty
state fits, so it appears once content with a wide input row is shown.

**Acceptance:** at the default and minimum window sizes the action buttons are fully
visible without horizontal scrolling.

## VQA-VALKEY-002 — Enter in the key pattern field does not rescan

**Priority:** low. **State:** open.

**Observed:** typing `qa:user:1*` and pressing Enter leaves the unfiltered list
(including `qa:user:9`). Clicking the (clipped) Scan button filters correctly: 11 keys
(`qa:user:1`, `qa:user:10`..`19`).

**Expected:** Enter submits the scan, matching the visible Scan action.

## VQA-VALKEY-003 — Sidebar footer overlaps the privacy card

**Priority:** low. **State:** open; most visible at 150% scale and when connected.

**Observed:** `Connected · 26 keys` is drawn over the bottom of the "Credentials stay
private" card at 1x. At 150% on a 1360x860 logical display the window opens at about
1020x683 and the footer is clipped and overlapping. At 200% it matches 1x.

## VQA-VALKEY-004 — Icon-like glyphs render as empty squares

**Priority:** medium. Same class as sql-manager `VQA-001`: the empty-state icon and the
`OFFLINE`/`CONNECTED` badge symbol are empty squares on native Wayland.

## Passed

- Connect to `redis://127.0.0.1:16479`: badge CONNECTED, "Connected · 26 keys".
- Console `HGET qa:profile name` returned `String("João")` (Unicode intact).
- Pattern scan via the Scan button.
- Profile name accepts typed ASCII and pasted accents; password field masks input.

## Not covered

- Saving a profile with a password (keyring), key create/edit/delete, Monitor tab.
- Light theme (the app forces dark), macOS, other compositors, a real keyring prompt.

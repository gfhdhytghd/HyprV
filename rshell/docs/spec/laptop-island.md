# UI spec delta — Dynamic Island, laptop profile (Dell XPS)

2026-10-24, parity QA lead. Phase A of `docs/tasks/laptop.md`.

**Decision D1 resolved (user, 2026-10-24): the laptop profile uses main's read-only island.**
- §2–§3 (pending card, blocking hooks) and §5.2 are kept as the record of the legacy
  laptop behaviour. rshell does **not** implement them.
- The laptop references for those states are captured as **expected divergences**, not
  gated (`laptop-qa-plan.md` §3.6).
- Island behaviour on laptop is `ui-island.md` exactly (incl. `AgentTaskModel` in-place
  list sync and the 650/1200 ms sampling), except for §4 (hidden while the bar is compact)
  and the budget rules in §4 and §5.1.

This file lists only what the laptop branch has or renders differently from the main-tree
island in `docs/spec/ui-island.md`. Everything not listed here is identical, so the desktop
spec applies unchanged.

**Source of truth:** `origin/laptop` at **`24972f7`** (2026-10-16 19:32 −0400), read from a
scratch worktree. Citations use these prefixes:
- `LDI:` = laptop `quickshell/DynamicIsland.qml`
- `LSH:` = laptop `quickshell/shell.qml`
- `LBS:` = laptop `quickshell/BarServices.qml`
- `LAI:` = laptop `quickshell/scripts/agent_island.py`

The comparison base is the main **working tree**, which is what rshell was specced from.

**History matters for one decision (D1).**
- Laptop `DynamicIsland.qml` and the agent hook/action scripts were last changed on
  2026-08-25 (`34f6732`).
- Main made the island read-only on 2026-10-16 23:49 (`a51ae43`: `AgentTaskModel`,
  `test_agent_readonly.py`, hook timeouts 5 s). That was 4 h after the laptop tip.
- So most of the island delta is the laptop branch **lagging** main, not a laptop-specific
  design.

## 0. Summary

| # | Delta | Visible? | Proposed rshell handling |
|---|---|---|---|
| I1 | Pending approval/question card plus Allow/Deny/Reply/answer buttons in the expanded agent view | yes | **not implemented** (D1 = read-only); expected divergence, §2 |
| I2 | Agent hooks block until the island answers (timeout 86400 s) | agent behaviour | **not implemented**: main hook (5 s, non-blocking) on both profiles, §3 |
| I3 | The island is hidden whenever the bar is compact (laptop adaptive bar) | yes | required, §4 |
| I4 | Agent transport is `agent-island-watch.py` (file monitor + 5 s fallback), not the 650/1200 ms ticker | ring cadence | **not preserved**: main cadence while visible, no tick while hidden, §5.1 |
| I5 | The agent list is a plain JS array, so the ListView resets (scroll to top, delegates rebuilt) on every emission | yes, when scrolled | **not preserved** (main's `AgentTaskModel`), §5.2 |
| I6 | Codex app-server: laptop *spawns* `codex app-server --listen stdio://` | no (yields nothing) | keep rshell attach-only, §5.3 |
| I7 | Media metadata from `BarServices` MPRIS events; position on a 1 s timer only while `mediaUiVisible` | no (same contract) | §6 |
| I8 | Clock `now`: 1 s timer while `mediaUiVisible`, else minute-aligned timer | ≤1 s minute-flip lag | do not preserve, §6 |
| I9 | `AudioSpectrum` repaints even while invisible | no (perf only) | do not preserve |
| I10 | Bar surface height is always 820, not 58/820 | no (transparent, input masked) | keep the desktop 58/820, §4 |
| I11 | `assets/lais/*` (7 extra files) | no | no runtime delta, §7 |

`agent-island-watch.py` is **byte-identical** on both branches. Only the laptop shell runs
it; main runs `desktop-state.py`.

## 1. Inputs added to `DynamicIsland` (laptop only)

| property / signal | `LDI:` | notes |
|---|---|---|
| `agentPending: var` (default `null`) | 20 | `snapshot.pending`: the **oldest** pending request by `created_at` (`LAI:925-937`) |
| `agentPendingCount: int` | 21 | `snapshot.pending_count`. **Unused by any visual** (no badge) |
| `agentApproveRequested(requestId)` | 32 | |
| `agentDenyRequested(requestId)` | 33 | |
| `agentReplyRequested(requestId)` | 34 | |
| `agentAnswerRequested(requestId, answer)` | 35 | |

Helpers (`LDI:298-326`):
- `pendingKind()`: `String(pending.kind)`, or `""`.
- `pendingRequestId()`: `String(pending.id)`, or `"current"`.
- `pendingQuestionText()`: `pending.question || pending.detail || ""`.
- `pendingOptions()`: `pending.options.slice(0, 3)` mapped to strings, `[]` if not an array.

A request is a **question** iff `pendingKind().indexOf("question") >= 0`. That covers
`question` (from a Notification) and `question_from_permission` (AskUserQuestion). Anything
else, in practice `approval`, is an **approval**.

### 1.1 Pending request shape (`LAI:774-822`)

| kind | fields |
|---|---|
| `approval` | `id`, `kind`, `session_id`, `source`, `tool` (tool name or `"Permission"`), `detail` (tool detail), `created_at` |
| `question_from_permission` | `id`, `kind`, `session_id`, `source`, `question`, `options: string[]`, `answer_key` (question `header`, else `"answer_1"`), `created_at` |
| `question` | as above; `answer_key: "answer"` |

`id` = `"{source}-{session_id}-{tool_use_id | ms}"`. Options come from `options[]` strings or
`options[].label`.

## 2. I1 — Pending card (expanded agent view only)

### 2.1 Geometry changes (`LDI:96-112`)

```
agentExpandedTopReserve = 26 + 58 + (agentPending ? 11 + 74 + 13 : 20)   // 182 | 104
agentSessionListHeight  = max(42, min(contentH, 490 − agentExpandedTopReserve))
agentExpandedHeight     = max(agentPending ? 216 : 184, agentExpandedTopReserve + agentSessionListHeight)
```

- With a pending request: n = 1 → **224**; n = 3 → 182 + 142 = **324**; budget cap reached
  at 490 (list ≤ 308 px, so 6 full rows and a partial seventh visible).
- Without one: the desktop values apply (n = 1 → 184).
- The width is unchanged (`agentExpandedWidth` 448).
- When `agentPending` flips, the height target changes. It animates with the existing
  island height Behavior; no new Behavior is added.

### 2.2 `pendingAgentCard` (`LDI:1392-1478`)

**Container.** It is a child of `expandedAgent`, which fills `islandFrame` with margins 13,
so the content width is 448 − 26 = **422**.

**Card.**
- Anchors: left/right of `expandedAgent`; top = `expandedAgentHeader.bottom + 11`, giving
  y = 69 in content coordinates.
- Height: `74`, or `0` without a pending request. `visible: agentPending !== null`.
- Radius **16**. Colour `softFill`. Border 1 px `strokeColor`, or 0 px without a pending
  request.
- The session list stays bottom-anchored (`LDI:1480-1488`). With a pending request, the
  gap between card bottom and list top is 13.

**Left column.**
- Anchors: left + 12; right = `pendingActions.left − 10`; vertically centred; spacing 4.
- Title text:
  - `"Question"` if question, else `"Approval"`.
  - 11 px, `Font.Black`.
  - Colour `agentAccent(primaryAgent)`.
- Body text: `pendingQuestionText()`, 12 px `Font.DemiBold`, `textColor`, `ElideRight`,
  width = column width.
- Font for both: `shellRoot.baseFont`, `NativeRendering`.

**`QUIRK_LAPTOP_PENDING_ACCENT_PRIMARY`.**
- The title colour and the Allow/option button colour use the **primary** session's accent
  (`sessions[0]`, sorted by status priority).
- They do not use the accent of the session that owns the request.
- The request shown is the oldest, which can belong to a different session. A Claude
  request can therefore render with the Codex accent.

**`pendingActions` Row.**
- Anchors: right − 10, vertically centred. Spacing **7**.
- Children, in order:
  1. **Question:** a Repeater over `pendingOptions()` (≤ 3). Each is an `AgentActionButton`
     with label = option, `buttonColor` = accent, `labelColor` `#ffffff`; it emits
     `agentAnswerRequested(id, option)`.
  2. **Question:** `"Reply"` (softFill / textColor) → `agentReplyRequested(id)`.
  3. **Approval:** `"Allow"` (accent / `#ffffff`) → `agentApproveRequested(id)`.
  4. **Approval:** `"Deny"` (softFill / textColor) → `agentDenyRequested(id)`.
- Hidden buttons take no Row space (`visible: false`).

**`QUIRK_LAPTOP_PENDING_ROW_OVERFLOW`.**
- No width clamp exists.
- With 3 long options, the Row's left edge can pass x = 12, and the left column then gets
  zero or negative width: the title and body are clipped or elided to nothing.
- Keep the layout maths; do not add a clamp.

### 2.3 `AgentActionButton` (`LDI:751-794`)

**Item.**
- Width `max(56, label.implicitWidth + 22)`, height **30**.
- `scale: pressed ? 0.95 : 1`. This is instant: there is no Behavior.
- The transform origin is Qt's default for Item, `Center`.

**Background Rectangle.**
- Fills the item. Radius **15**, colour = `buttonColor`, border 1 `strokeColor`.
- `opacity: containsMouse ? 1 : 0.86`. Opacity applies to the fill and border only, because
  the label is a sibling.

**Label.**
- Centred. 12 px `Font.Bold`, `NativeRendering`, colour = `labelColor`.
- Defaults: `buttonColor` softFill, `labelColor` textColor.

**MouseArea.** Fills the item, `hoverEnabled`, `PointingHandCursor`, emits `clicked()` on
click.

### 2.4 Shell actions (`LSH:2991-3004`, `agent-island-action.py`)

`agentIslandAction(action, id, value)` runs this detached:

```
python3 scripts/agent-island-action.py <approve|deny|reply|answer> <id|current> [value]
```

220 ms later (`agentIslandFollowupRefresh`, `LSH:3185-3191`), it writes `refresh\n` to the
watcher's stdin.

What the action script does under the state lock:
- It writes `responses/<safe id>.json`. `safe` keeps `[A-Za-z0-9._-]` and maps everything
  else to `_`.
- **approve:** `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`.
- **deny:** the same with `"behavior":"deny","message":"Denied from HyprV island"`.
- **answer:**
  - `question_from_permission` → `allow` with `updatedInput.answers{answer_key: value}`.
  - `question` → `{"hookSpecificOutput":{"hookEventName":"Notification","answer":value}}`.
- **reply:** prompts with `rofi-wayland -dmenu -p <question[:80]>`. It falls back to `rofi`,
  lists the options and has a 3600 s timeout. An empty answer means exit 1 and nothing is
  written.
- Afterwards it sets the session to `status` `running`, or `idle` on deny, clears
  `pending_request_id`, sets `last_activity = now` and pops the request.
- **dismiss** exists in the script, but no UI calls it.

The Rust actor must produce byte-identical response JSON: Python
`json.dumps(ensure_ascii=False, separators=(",",":"))` plus `\n`. The `reply` path is a
user-initiated action with no native API, so spawning rofi is allowed.

## 3. I2 — Blocking hooks (laptop `agent-island-hook.py`, `agent-island-install-hooks.py`)

**Hook installer timeouts.** On laptop, `PermissionRequest` and `Notification` are
**86400 s**; main uses 5 s. All other events and matchers are identical.

**Hook behaviour.** After recording a `PermissionRequest`, or a `Notification` carrying a
question:
- It polls `responses/<id>.json` every **0.18 s** until
  `HYPRV_AGENT_ISLAND_PERMISSION_TIMEOUT` (default 86400 s).
- It reads and deletes the file, then prints it as the hook's stdout JSON.
- If no response arrives, it emits a deny with message `"No HyprV island response"`.
- It then clears the pending request and sets the session status to `running`, or `idle`
  when denied.

Main instead pops the request immediately and never holds the hook.

For rshell, `hyprv-agent-hook` (`crates/hyprv-svc-desktop/src/bin/hyprv-agent-hook.rs:97`)
and `agent_island/install.rs` are desktop/read-only today. Laptop parity under D1 needs:
- the profile-dependent installer timeouts;
- a hook mode that waits for the response file. This should be event-driven: inotify on
  `responses/`, not a 0.18 s poll. The externally observable timing is "answer applied
  within one poll", so inotify is strictly better and allowed.

**Risk (why D1 exists).**
- `~/.claude/settings.json` and `~/.codex` hooks are per-user and rewritten by whichever
  shell runs.
- An interactive laptop profile makes every permission prompt wait on the island. If the
  shell is not running, the hook blocks until the timeout. Claude Code's own prompt still
  appears, but the hook process lingers for up to 24 h.
- Main removed exactly this deliberately ("the agent's own UI handles responses",
  `agent-island-hook.py` on main).

### D1 — resolved: (b), read-only (user, 2026-10-24)

- ~~**(a) Exact laptop parity.** Implement I1 and I2 behind `QUIRK_LAPTOP_AGENT_INTERACTIVE`
  (laptop profile only). The parity fixtures in `laptop-qa-plan.md` §3.6 then gate it.~~ Rejected.
- **(b) Supersede with main's read-only island on both profiles. Chosen.**
  - The laptop code is simply older than the read-only change, and it is risky (24 h
    blocked hooks).
  - The laptop references for I1 are then recorded as *expected divergences*. The fixtures
    are still captured, so the decision stays auditable.
  - The user should sync the read-only change to `origin/laptop` or accept the divergence.

## 4. I3 — Island hidden while the laptop bar is compact

The laptop bar has a per-screen persistent compact/full state (`LSH:41-50`, toggled by IPC
`bar toggle <monitor>` with a 500 ms debounce). The gesture comes from the Hyprland edge
plugin, `hyprland-plugins.lua:196-205`. The bar owner specs the bar itself; the
island-relevant facts are below.

**Compact state.**
- `contentRoot.visible: barWindow.reveal > 0` (`LSH:3884`), and `contentRoot.y` slides by
  `−standardTravel·(1−reveal)` (`LSH:3883`). The island lives in `centerSection` inside
  `contentRoot`.
- So at `reveal == 0` the island is **not rendered and receives no input**. There is no
  compact island, pill or indicator of any kind.
- During the slide (0 < reveal < 1) the island renders normally, translated with the bar.
- The `exclusiveZone` is `round(25 + 33·reveal)`.

**Data while compact.**
- `mediaUiVisible = anyBarExpanded || controlPanel open` (`LSH:37`). With every bar compact
  and the CP closed, it is false:
  - cava does not run (`LSH:3324`);
  - the MPRIS position timer stops (`LBS:144-149`);
  - the clock falls back to the minute-aligned timer.
- The agent watcher **keeps running** (5 s fallback, §5.1). The island state stays current,
  so expanding the bar shows fresh sessions on the first frame.

**rshell requirement.**
- While every screen is compact and nothing else needs it, the island must do zero work: no
  progress-ring tick, no spectrum, no position reads.
- It may drop its GPU resources; on reveal it must render the current state on the first
  revealed frame.
- The agent model must stay current, but event-driven (inotify), with no 650 ms tick while
  hidden.

**I10.**
- The laptop window `implicitHeight` is always `min(screen.height, 820)` (`LSH:3794`). Main
  uses 58/820 (`ui-island.md` §1.1).
- The extra area is transparent, and input is masked to the same region, so nothing is
  observable.
- Keep the desktop 58/820 rule: a smaller surface, which is better for the budget.

## 5. Agent data path

### 5.1 I4 — Transport and cadence (`LSH:3399-3413`, `agent-island-watch.py`)

The watcher is the process `python3 scripts/agent-island-watch.py` with stdin enabled:
- stdout is split on `\n`, and each line goes to `updateAgentIslandState(line)`
  (`LSH:477-492`).
- On exit it restarts after 3000 ms (`agentWatcherRestart`). It does **not** reset the state
  on exit; main resets via `onStopped`.

Watcher semantics:
- It uses a `Gio` directory monitor (`WATCH_MOVES`) on the state dir and reacts to changes
  to `state.json`.
- It ignores its own writes by comparing the bytes captured under the lock.
- It coalesces bursts with a **150 ms** one-shot.
- Any stdin data triggers an immediate refresh.
- A **5 s** `timeout_add_seconds` fallback refreshes unconditionally.
- Each refresh:
  - runs `public_snapshot`, including the codex process scan, rate-limited to 1.5 s by
    `last_process_scan`;
  - writes the state;
  - prints the JSON, **every time, even when unchanged**.

**Visible consequence in legacy (not preserved, D1 = read-only).**
- The running/processing progress sawtooth (`ui-island.md` §6.4.3) is sampled at each
  emission, not every 650 ms.
- With no hook activity, the ring steps once per **5 s**. Hook events make it step sooner.
- Process-death and expiry removal (20 s / 900 s) are likewise observed with up to 5 s
  latency.
- rshell laptop profile uses main's cadence (`ui-island.md` §6.4.3: 650 ms while a session
  is active, 1200 ms otherwise) **only while the island is visible**. While hidden there is
  no tick; the model updates on hook events (inotify) and expiry is a deadline timer.

Snapshot additions vs main: `pending` (object|null) and `pending_count` (int);
`updated_at` is present too. Parsing:
- `pending_count = Number(..) || 0`.
- `sessions = Array.isArray(..) ? .. : []`.
- `pending = data.pending || null`.
- A parse failure means a reset.

### 5.2 I5 — list reset in legacy (`LDI:1480-1494`, not preserved)

- `model: island.agentSessions` (a JS array; `required property var modelData`). Main uses
  `AgentTaskModel`, a ListModel synced in place.
- `updateAgentIslandState` assigns a new array on **every** line (≤ 5 s). In Qt that is a
  model reset:
  - all delegates are destroyed and recreated;
  - `contentY` returns to 0;
  - the scroll indicator resets;
  - hover inside a row is lost until the next pointer motion.
- On laptop, a user scrolled down in a 9-session list therefore snaps back to the top
  within 5 s.
- **Not preserved** (D1 = read-only): main's `AgentTaskModel` is the fix. The fixture
  `laptop-island/agent-expanded-9-scrolled` is captured as an expected divergence.

### 5.3 I6 — Codex app-server (`LSH:3362-3367`, `agent-island-codex-appserver.py`)

- Laptop always spawns the script. If `codex` is on PATH, the script runs
  `codex app-server --listen stdio://` and restarts it every 4 s after errors.
- That private app-server only reports threads it hosts itself, which is none, so it never
  adds sessions in practice. It does cost a resident `codex` process.
- rshell's attach-only adapter (`agent_island/appserver.rs`) is the correct laptop
  behaviour too.
- **Do not spawn.** This is a budget item, not a parity item.

## 6. Media and clock inputs (I7, I8, I9)

The island UI contract (`ui-island.md` §6.2, §9) is unchanged. Only the producer differs
(`LBS:87-160`):

**Player selection.** Event-driven `Mpris.players` with the same priority rule as
`media-status.sh`: playing → paused-with-content → any-with-content → paused → first.

**Metadata.**
- Fields: `trackTitle`, `trackArtist`, `trackArtUrl`, `trackAlbum`,
  `length` (s, if `lengthSupported`), `isPlaying`, `identity`, and the `dbusName` suffix.
- They are applied on change. The Cider metadata repair is kept: `media-status.sh` runs on a
  metadata change, with a 30 s retry only for a confirmed Cider.

**Position.**
- `onPositionChanged` plus a 1 s timer running while
  `mediaVisible && player && player.isPlaying`.
- The displayed value still steps at ≤ 1 s, so the desktop rule holds.

**Multi-artist joining.** Verify in Phase B against a 2-artist fixture: Quickshell's
`trackArtist` vs `media-status.sh` `, `.

**Clock** (`LSH:3268-3274`).
- The interval is `1000` while `mediaUiVisible`. Otherwise it is the ms to the next minute,
  with `triggeredOnStart`.
- `hh:mm` pixels are identical. Only the minute flip may lag ≤ 1 s while the bar is
  expanded.
- **Do not preserve.** Use a minute-aligned tick.

**`AudioSpectrum`.** The laptop repaints on every value change even when invisible. That
has no pixel effect; do not preserve.

## 7. I11 — `assets/lais/`

| file | used by laptop runtime? |
|---|---|
| `lais.svg` | yes: tray icon override (`LSH:2534`, `2607-2609`, `?v=deepblue`). **Byte-identical** to main's |
| `bubble.svg`, `chip.svg`, `layers.svg`, `nodes.svg`, `orbit.svg`, `spark.svg`, `lais.png` (128×128 RGBA), `preview.html` | **no**: icon candidates for the LAIS tray app (`preview.html` 「LAIS · Fluent 图标」); no QML, script or Lua reference |

There is no island use and no runtime delta. Do not ship the unused files.

## 8. Quirk constants (laptop profile)

| const | default | gated by |
|---|---|---|
| `QUIRK_LAPTOP_ISLAND_HIDDEN_WHEN_COMPACT` | on (not optional: it is the bar structure) | §4 |

Withdrawn by D1 = read-only: `QUIRK_LAPTOP_AGENT_INTERACTIVE`,
`QUIRK_LAPTOP_PENDING_ACCENT_PRIMARY`, `QUIRK_LAPTOP_PENDING_ROW_OVERFLOW`,
`QUIRK_LAPTOP_AGENT_TICK`, `QUIRK_LAPTOP_AGENT_LIST_RESET`.

## 9. Visual states for references (laptop additions to `ui-island.md` §10)

Capture in both themes at scale 2 (XPS `eDP-1`: 2880×1800 px, 1440×900 logical,
`hyprland-monitors.lua:5`) and at scale 1. Items 1–8 are **expected divergences** (D1 =
read-only): captured for the record, excluded from the gate. Item 9 is gated.

1. Agent expanded with **approval** pending, n = 1 (height 224). Also with a long
   `detail` (elide).
2. Agent expanded with a **question** pending and 0, 1 and 3 options; 4 options (only 3
   shown); 3 long CJK options (row overflow quirk).
3. Pending with n = 3 and n = 9 (budget cap 490).
4. Hover on Allow (opacity 1 vs 0.86); Allow pressed (scale 0.95); Deny hovered.
5. Pending owned by a non-primary session (accent quirk): Claude primary + Codex pending,
   and vice versa.
6. Pending appears and disappears while expanded: height morph frames at t = 0, 105 and
   210 ms.
7. 9 sessions scrolled to the middle, then one emission: the list is back at the top
   (`LIST_RESET`).
8. Running ring at two emissions 5 s apart (step cadence; the ring is identical between
   them).
9. Bar compact, with agent and music active: the island region is fully transparent.
   Slide frames at reveal 0.25, 0.5 and 0.75 show the island translated.

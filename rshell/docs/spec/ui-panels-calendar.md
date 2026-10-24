# UI spec — Week calendar, CalendarBridge/Socket, StartupConfirm

Read `ui-panels.md` first. Sources: `WeekCalendarPanel.qml`, `CalendarBridge.qml`,
`CalendarSocket.qml`, `StartupConfirm.qml`, `shell.qml` (bar window hosting `:3551-3997`,
calendar functions `:2730-2762`, child-window timers `:4371-4403`).

---

## 1. Hosting inside the bar window
The calendar is **not** a separate surface. It is drawn inside each bar window
(`PanelWindow barWindow`, namespace `hyprv-quickshell`, Top/Overlay layer — the bar spec owns
it) in the `centerSection` that normally holds the Dynamic Island.

### 1.1 Shell state (`shell.qml:134-139`, `:2742-2762`)
- `calendarOpen: bool`, `calendarScreen: Screen?`, `calendarChildOpening`, `calendarChildActive`.
- `toggleCalendar(screen)`: if open on that screen → `requestCalendarClose()`; else
  `calendarScreen = screen; calendarOpen = true`. Callers: IPC `calendar toggle` (primary bar),
  the island's calendar click (`onCalendarClicked`, `:3936`).
- `requestCalendarClose(force)`: refused (returns false) unless `force` while a child window is
  opening/active; else `calendarOpen = false; calendarScreen = null`.
- `beginCalendarChild()`: `calendarChildOpening = true`, start 2600 ms launch timeout,
  `requestCalendarClose(true)` — opening an event editor closes the panel.
- Child window tracking (`:4371-4403`): while open/opening/active, every 180 ms check whether a
  toplevel with app-id `com.danklinux.dankcalendar` exists: yes → opening = false, active = true;
  no and was active → active = false, emit `calendarChildReleased`. Launch timeout (2600 ms):
  if no window → opening = false, emit released. **Port:** use foreign-toplevel events
  instead of polling.

### 1.2 Per-bar-window state (`shell.qml:3565-3622`, `:3673-3712`)
- `calendarVisible = calendarOpen && calendarScreen == this screen`.
- `calendarPanelPresented` (bool), `calendarProgress` (0..1, `Behavior`: 210 ms **OutCubic**),
  `calendarDismissArmed`.
- On `calendarVisible` → true: stop leave/close timers; armed = false; start arm timer
  (260 ms → armed = calendarVisible); `calendarPanelPresented = true`; next tick
  `calendarProgress = 1`.
- On → false: stop leave/arm timers; armed = false; `calendarProgress = 0`; if presented start
  `calendarPanelCloseTimer` (210 ms): if visible again → progress = 1; else progress = 0,
  `calendarPanelPresented = false`.
- Bar window `implicitHeight = min(screenH, presented || islandExpanded ? 820 : 58)` — the
  surface grows to 820 px while the calendar is presented (`:3637`).
- Input mask adds `calendarDismissLayer` (full content height while visible) (`:3644-3653`).
- `calendarLeaveTimer` (220 ms): closes if still visible, pointer not inside the panel, and no
  child opening/active (`:3673-3685`).
- `onCalendarChildReleased`: if visible and pointer outside → restart leave timer (`:3708-3714`).

### 1.3 Dismiss layer (`shell.qml:3738-3755`)
MouseArea z 45, full width, height = full bar-window height while visible else 0, hover:
- pointer motion (while visible, armed, no child) → restart the 220 ms leave timer (moving the
  pointer anywhere outside the panel closes it after 220 ms);
- left click → `requestCalendarClose()`.

### 1.4 `centerSection` morph (`shell.qml:3878-3911`)
Rectangle z 50, horizontally centred (+ island offset when not presented), top margin 10:
- `width/height` = presented ? calendarPanel size : island size;
- `radius` = calendarVisible ? **30** : 24;
- colour = `calendarProgress > 0.001 ? moduleBackground : transparent`; border 1 px
  primaryText α0.11 D / 0.09 L when progress > 0.001 else 0; clip.
- `Behavior on width / height / radius`: 210 ms **OutCubic**, **enabled only while presented**.
- Children: `DynamicIsland` with `opacity = 1 − calendarProgress` (hidden < 0.01) and
  `calendarOpen: presented`; `calendarPanel` Loader (active while presented) top-centred,
  `width = min(520, contentRoot.width − 48)`, `height = min(500, barWindow.height − 78)`
  (500 on any screen ≥ 578 px tall), `opacity = calendarProgress`, holding
  `WeekCalendarPanel { anchors.fill; embeddedSurface: true; bridge: CalendarBridge{…} }`.
Open sequence: presented → container animates from island size to 520×500 and radius → 30
over 210 ms while island fades out and calendar fades in (210 ms OutCubic).
Close sequence: progress → 0 (210 ms) and radius → 24 while the size stays 520×500; at 210 ms
`presented = false` → size binding flips back to the island size in the same instant the size
Behaviors are disabled (expect a snap; confirm with a reference capture).

Panel signal wiring (`:3972-3996`): `pointerInsideChanged` → (visible) inside ? stop leave
timer : (no child) restart leave timer; `closeRequested` → `requestCalendarClose()`;
`expandRequested` → `requestCalendarClose(true)` then `bridge.showFullApp()`;
`bridge.childWindowRequested` → `beginCalendarChild()`.
The `CalendarBridge` is created with the panel each time it is presented, with
`weekStart: new Date()` (now, *not* Monday) until the first `loadWeek` normalises it.

---

## 2. `WeekCalendarPanel` (`WeekCalendarPanel.qml`)

### 2.1 Root
Rectangle; embedded → radius 0, transparent, no border (standalone: radius 28,
moduleBackground, 1 px lineColor) (`:37-42`); clip. Size 520×500 in practice.
Tokens (`:28-32`): `textColor = primaryText`; `mutedColor = primaryText α0.62 D / 0.58 L`;
`accentColor = launchColor`; `lineColor = primaryText α0.11 D / 0.09 L`; `elevatedColor =
#fff α0.055 D / α0.30 L`. Fonts: baseFont, **default QtRendering** (no `renderType`).
Constants (`:16-25`): `hourHeight 44`, `bufferDayCount 21`, `bufferLeadingDays 7`,
`toolbarHeight 58`, `dayHeaderHeight 48`, `allDayHeight = clamp(12 + lanes·24, 34, 106)`,
`timeColumnWidth 58`, `dayWidth = (W − 58 − 12)/7` (W = 520 → 450/7 ≈ 64.2857).
`pointerInside = visible && HoverHandler.hovered` (handler enabled iff visible && opacity > 0.01).
`currentTime = shellRoot.now` (shell clock).

### 2.2 `HeaderButton` component (`:44-79`)
`implicitWidth = max(36, labelW + 22)`, height 34, radius 17. Colour: emphasized → accent;
hovered → elevatedColor; else transparent. Border 1 px lineColor unless emphasized (0).
`scale` = pressed ? 0.96 : 1. `Behavior on color`: ColorAnimation 120 ms (linear);
`Behavior on scale`: NumberAnimation 90 ms (linear). Label centred, 13 px **DemiBold**,
colour emphasized ? `#ffffff` : textColor. MouseArea fill, hover, pointer → `clicked`.

### 2.3 Layout (panel coordinates, W = 520, H = 500)
1. **Toolbar** Row at x 14, width 492, height 58 (`:348-399`), no spacing:
   - Item 102 wide: HeaderButton `"＋ 创建日程"` (fullwidth plus U+FF0B), emphasized, left,
     vcentred → `bridge.createEvent()`.
   - Row width 492 − 178 = 314, spacing 7: spacer `max(0, (314 − navW)/2)` then `navContent`
     Row (spacing 7, vcentred): `"‹"` → `scrollByWeek(−1)`; centre label button
     `rangeLabel() + "  ·  " + HH:mm(currentTime)` → `closeRequested()`; `"›"` →
     `scrollByWeek(1)`.
   - Item 76 wide: HeaderButton `"↗"` right-aligned → `expandRequested()`.
2. Divider 1 px lineColor, full width, y = 57 (`:401-407`).
3. **Day header viewport** x 58, y 58, 450×48, clip (`:409-467`): Row at x = −contentX of 21
   cells (dayWidth wide). Cell: Column centred, spacing 1: weekday
   `["一","二","三","四","五","六","日"][index % 7]` 11 px DemiBold mutedColor; date pill 28×24
   radius 12, fill accent if `isToday(day)` else transparent, number `day.getDate()` 14 px Bold,
   white if today else textColor. (Weekday labels assume the buffer starts on a Monday.)
4. **All-day area** x 58, y 106, 450 × allDayHeight, clip (`:469-525`): content at x = −contentX.
   Chip per layout entry: `x = startDay·dayWidth + 3`, `y = 6 + lane·24`, `w = span·dayWidth −
   6`, h 20, radius 6, fill eventColor α0.80, 1 px `#fff` α0.22; text (left 7, right 5, vcentred)
   `summary || "(无标题)"` 10 px DemiBold white ElideRight. Hover → tooltip; click →
   `bridge.openEvent(event)`.
   Label `"全天"` at x 8, y = 106 + 8, width 44, right-aligned, 10 px mutedColor (`:527-536`).
   Divider 1 px lineColor at y = 106 + allDayHeight − 1 (`:538-544`).
5. **Timeline** Flickable x 58, y = 106 + allDayHeight, width 450, height = 500 − y
   (`:546-680`); `contentWidth = 21·dayWidth` (1350), `contentHeight = 24·44 = 1056`; clip;
   StopAtBounds; flickDeceleration 2600; drag both directions; pressDelay 80 ms.
   Content (`timelineContent`):
   - 25 hour lines: y = i·44, full content width, 1 px lineColor.
   - 22 day lines: x = i·dayWidth, full height, 1 px lineColor.
   - Timed events: `laneW = (dayWidth − 6)/lanes`; `x = day·dayWidth + 3 + lane·laneW`;
     `y = startMinute/60·44 + 1`; `w = max(18, laneW − 2)`; `h = max(18, (end−start)/60·44 −
     2)`; radius 7; fill eventColor α0.84; 1 px `#fff` α0.24; clip. Inner Column margins 5,
     spacing 1: summary (or "(无标题)") 10 px Bold white ElideRight; start `HH:mm` 9 px
     `#f7f7fb` opacity 0.84, visible only if card height ≥ 34. Hover → tooltip; click → open.
   - Now line (z 20, visible iff today is inside the 21-day buffer): x = dayIndex·dayWidth,
     y = (h·60 + m + s/60)/60·44, size dayWidth × 2, accent; dot 7×7 radius 4 accent at its
     left, vertically centred.
   - "Scroll indicator" (`:670-679`): declared inside the Flickable, so it is reparented into
     the *content* item: 3×max(28, heightRatio·h) rect, radius 2, mutedColor, opacity moving ?
     0.65 : 0.25, anchored to the content's right edge (x = 1350 − 4 − 3) at content-y =
     `visibleArea.yPosition·h`. It scrolls with the content and is only on screen when scrolled
     to the far right. Reproduce literally (or fix deliberately).
6. **Time column** x 0, y = timeline.y, 58 × timeline.h, clip (`:682-709`): labels at
   y = i·44 − 7 − contentY, x 7, width 44, right-aligned, `"HH:00"` (i < 24; i = 24 empty),
   9 px mutedColor.
7. **Status pill** (visible iff bridge && (!connected || lastError)), centred in the panel:
   width text + 28, h 34, radius 17, elevatedColor; text `lastError || "正在连接日历…"` 12 px
   textColor (`:711-726`).
8. **Tooltip** (z 100, visible iff hoveredEvent) (`:728-788`): width 270, height content + 22,
   radius 12, fill moduleBackground, 1 px lineColor. `x = clamp(anchorX − 135, 8, W − 278)`
   (anchorX = hovered item centre); below the item (+7) unless `belowY + h > H − 8`, then above
   (`max(8, topY − 7 − h)`). Column margins 11, spacing 4: summary 13 px Bold textColor elide;
   time 11 px muted (`"全天"` or `"M月d日 HH:mm – HH:mm"`); location or calendar name 11 px muted
   elide (hidden if empty); description 11 px muted wrap, max 3 lines, elide (hidden if empty).
   Hover-leave of the same event clears it.

### 2.4 Scrolling (`:260-322`)
- Initial: `contentX = 7·dayWidth` (week of `bridge.weekStart` at the left edge) once
  dayWidth > 0; on becoming visible also `positionNearNow()`: `contentY = clamp((hour − 2)·44,
  0, contentH − h)` where hour = current hour if today is in the buffer else 8. Also re-run
  after `weekStartChanged`.
- Wheel (WheelHandlers on day header, all-day area and timeline; mouse + touchpad): if
  |dx| > |dy| → horizontal: `contentX −= pixelDelta.x || angleDelta.x/120·dayWidth`; else
  vertical: `contentY −= pixelDelta.y || angleDelta.y/120·132`; both clamped; accepted.
- Infinite horizontal paging (`rebaseHorizontalScroll` on every contentX change): if
  `contentX < 3·dayWidth` → `contentX += 7·dayWidth` and `bridge.previousWeek()`; if
  `> 11·dayWidth` → `−= 7·dayWidth` and `bridge.nextWeek()` (the content shift and the week
  change are applied together so the view does not jump).
- `scrollByWeek(dir)`: `contentX = clamp(contentX + dir·7·dayWidth, 0, contentW − viewW)`
  (instant, no animation; the rebase then pages the data).
- `rangeLabel()`: first visible day index `floor(contentX/dayWidth + 1e−4)`, last
  `ceil((contentX + 450)/dayWidth − 1e−4) − 1` (clamped to 0..20); format: different years →
  `"yyyy/M/d – yyyy/M/d"`; different months → `"yyyy/M/d – M/d"`; else `"yyyy/M/d – d"`
  (en dash U+2013 with spaces).

### 2.5 Event layout (`rebuildLayouts`, `:142-224`)
Rebuilt on bridge/events/calendars/weekStart change. Buffer start = `weekStart − 7 days`.
Events whose calendar is `hidden` or `holdsTasks` are dropped; colour = calendar.color or accent.
- All-day: start/end are read as **UTC dates** (`allDayDate`), `startDay = max(0, Δdays(start,
  buffer))`, `endIdx = min(21, max(start+1, Δdays(end, buffer)))`, kept if in range; `span =
  max(1, endIdx − startDay)`. Sorted by startDay asc, span desc; greedy lane = first lane
  whose end ≤ startDay. `allDayLaneCount = lanes`.
- Timed: split per local day across the 21 days; `startMinute` from day start; `endMinute =
  max(start + 15, end)`. Per day, sort by start then end; greedy lanes; every event in that day
  gets `lanes = number of lanes used that day`.

### 2.6 Data contract
```rust
struct Calendar { id: String, color: Option<Color>, hidden: bool, holds_tasks: bool,
                  summary: String, name: String }
struct CalEvent { uid: String, calendar_id: String, summary: String, start: DateTime,
                  end: DateTime, all_day: bool, location: String, description: String }
struct CalendarView { connected: bool, last_error: String, week_start: NaiveDate /*Monday*/,
                      calendars: Vec<Calendar>, events: Vec<CalEvent>, now: DateTime }
// actions: previous_week(), next_week(), open_event(&CalEvent), create_event(), show_full_app()
```

---

## 3. `CalendarBridge` + `CalendarSocket` (service side, no visuals)
Talks to DankCalendar's daemon (`~/.local/bin/dcal`).
- Discovery (`CalendarBridge.qml:166-195`): run `dcal --json socket` → JSON `{path}`; on parse
  error set `lastError = "无法连接日历服务"` and retry after 1800 ms; non-zero exit → retry.
- Two Unix-socket connections to `path` (`:204-241`), newline-delimited JSON:
  - request socket: on link up → `connected = true`, open the subscribe socket,
    `calendars.list`, `loadWeek(weekStart)`; on link down → `subscribed = false`, drop pending.
  - subscribe socket: on up → send `{"id", "method":"subscribe", "params":{"topics":
    ["accounts","calendars","events","sync"]}}`.
- Request: `{"id": Date.now()+counter, "method", "params"}`; response `{"id", "result"|"error"}`
  matched to callbacks; a message with `"event"` is a subscription push: `accounts`/`calendars`
  → `calendars.list` + debounced reload (350 ms); `events`/`sync` → debounced reload.
- `sendRequest` with the request link down → `lastError = "日历服务未连接"`.
- `loadWeek(d)`: `weekStart = Monday 00:00 local of d`; serial++; if connected: `events.list
  {from: (weekStart−7d).toISOString(), to: (weekStart+14d).toISOString(), limit: 2000}`; drop
  stale responses by serial; `events = result.events`, `lastError = ""` (or error).
  `previousWeek/nextWeek` = ±7 days; `today()` = now.
- UI methods: `ui.openEvent {uid, start, editorOnly: true}` and `ui.newEvent {editorOnly:
  true}` (both emit `childWindowRequested` first); `ui.show {view: "week"}`.
- `CalendarSocket` (`CalendarSocket.qml`): wrapper with auto-reconnect while `connected` is
  desired: on disconnect, retry after `base = min(400·2^min(attempt,7), 12000)` ms plus random
  jitter in `[0, base/4)`; attempt counter resets on connect. `send()` appends `\n` and flushes.

---

## 4. `StartupConfirm` (`StartupConfirm.qml`) — separate one-shot process
Launched by `hypr/scripts/restore-session-layout.sh:163-171` as `qs --path
…/StartupConfirm.qml` with env `HYPRV_STARTUP_APPROVAL=<tmpfile>`; the script launches the
session apps only if the process exits 0 **and** the file contains `launch`. Port as e.g.
`hyprv-shell startup-confirm` keeping the env/file contract.

### 4.1 Behaviour (`:10-43`)
- `finish(launch)` (once): launch && path set → write `"launch\n"` to the file, quit on saved or
  save failure; otherwise quit immediately (nothing written = cancelled).
- 150 ms after start: focus "本次跳过" (Tab focus reason → focus ring visible) and set
  `keyboardReady`. Before that every key is swallowed (prevents a held key from deciding).
- `Escape` / `Alt+F4` (when ready, no autorepeat) → `finish(false)`.
- Buttons: Return/Enter/Space (when ready, not autorepeat) click the focused button; releases
  of those keys are swallowed. `KeyNavigation` Left/Right between the two buttons; Tab works.
  Mouse click on a button activates it.
- `dark = SystemPalette.window.hslLightness < 0.5` (Qt platform palette, not the shell's
  darkMode). `foreground = dark ? #cdd6f4 : #2b2b2c`; `accent = dark ? #89b4fa : #1e66f5`.

### 4.2 Surface (`:45-53`)
PanelWindow, **no anchors** (compositor centres it on the output), `implicitWidth 360`,
`implicitHeight = card height`, transparent, `ExclusionMode.Ignore`, namespace
`hyprv-startup-confirm`, Overlay, keyboard **Exclusive**.

### 4.3 Visuals (`:62-157`)
- Card fills the window: radius 12, colour `#1e1e2e` D / `#fafafb` L, border 1 px
  `#20ffffff` D (white α 0x20/255 ≈ 0.125) / `#14000000` L (black α ≈ 0.078).
- Content Column, margins 24 (left/right/top), width 312:
  1. Title `"启动用户应用"`: HarmonyOS Sans SC 16 px Medium, foreground, fixed line height 24.
  2. 6 px spacer.
  3. Body `"启动常用应用并恢复工作区布局。"`: HarmonyOS Sans SC 14 px, `#a6adc8` D / `#5e5e63` L,
     fixed line height 22, wrap.
  4. 24 px spacer.
  5. Row right-aligned, spacing 8: two `ActionButton`s 88×32: `"本次跳过"` (skip) then
     `"启动"` (launch).
  Card height = 24 + 24 + 6 + 22 + 24 + 32 + 24 = **156** → window 360×156.
- `ActionButton` (QtQuick.Controls.Basic `Button`): background radius 8, colour D: pressed
  `#585b70`, hovered `#45475a`, else `#313244`; L: pressed `#d9d9dd`, hovered `#e3e3e6`, else
  `#ededed`. Focus ring (visible iff `visualFocus`, i.e. keyboard focus): rect at −3 px margins,
  radius 11, 2 px border `accent`, transparent fill. Label centred HarmonyOS Sans SC 13 px
  Medium foreground. No animations.
- Text here uses default QtRendering.

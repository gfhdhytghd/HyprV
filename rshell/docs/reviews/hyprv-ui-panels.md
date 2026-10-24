# Adversarial review: hyprv-ui-panels

Reviewed 2026-10-23 (round 6). **Scope:** input and gestures, timers, action routing, password handling. **Review only:** the production crate was not changed.

Evidence is in [scratch/round6](scratch/round6/):
- [results.json](scratch/round6/results.json)
- [ui-panels.log](scratch/round6/ui-panels.log)
- [source-hashes.json](scratch/round6/source-hashes.json)
- `ui-panels/`: a frozen copy of the crate. Probes are appended after `// ROUND6 REVIEW PROBES` in `wifi.rs` and `control_panel.rs`, plus a read-only cache-key accessor in `qtext.rs`.

**Reproduce:** `python3 docs/reviews/scratch/round6/verify.py`. The Cargo part runs `cargo test --lib -- round6_` with the workspace `target/`.

**Severity.** P1: secret-policy violation. P2: input or behavior defect with real side effects. P3: parity or defense in depth.

## Verified good

- **Action routing.**
  - `hyprv-shell` maps an empty Wi-Fi password to `None` (`services.rs:359`), and so does the crate's own `services::route`.
  - `Action`'s hand-written `Debug` redacts `WifiConnect.password` (`action_debug_tests`).
  - `Return` submits only when the field is focused and non-empty, and clears the panel copy (`control_return_submits_typed_password_and_clears_it`).
  - Escape, close and collapse clear the password (`control_escape_clears_password`).
  - Busy guards (`action_busy`, `vm_busy`, `viewflow_busy`) stop repeat actions.
  - Enterprise networks route to the manager.
- **Normal press, drag and release.** Release ends the slider grab and emits `BrightnessFinish` (`control_release_ends_slider_grab`). Outside-press closes popups. Disabled zones fall through.
- **Timers.**
  - Every timed state has a deadline in `next_deadline`: popup open timer, Wi-Fi focus timer and cursor blink (only while focused and mapped), QuickAdjust hide/finalize.
  - Animations run only while `animating()` is true.
  - The calendar has only its flick, which is frame-driven.
  - The QuickAdjust 3 s auto-hide is not paused while dragging. That matches `QuickAdjustPopup.qml` (`hideTimer.restart()` only).

## PANEL-1 [P1] The typed Wi-Fi password stays in long-lived caches and transient strings as plaintext

**Location:**
- `src/qtext.rs:154` (`TextBox::key()` uses `self.text`, not the masked `shown_text()`)
- `src/qtext.rs:190-191,412,856`: the thread-local `LAYOUTS` (up to 4096 entries) and `IMAGES` (up to 2048) caches
- `src/wifi.rs:542` (`pw` plus a second clone into the `TextBox`)
- `src/wifi.rs:572-582`: `key_string` embeds `self.password` and is stored in `Ids.key`
- `src/wifi.rs:67`: `password: String`; `push_str`, `pop` and `clear` do not zeroize

The password field is drawn masked (`input.password = true`). But the layout and coverage cache keys are built from the **plaintext**. Every rebuild while typing inserts one key per prefix into thread-local caches that live for the whole process and are only emptied when they overflow.

After the user connects or presses Escape, the panel clears its own field, yet the caches still hold the full password. The probe found 8 entries, the longest being `"hunter2-secret"`. The scene-diff key string held it too while typing.

This defeats the zeroization work in hyprv-svc-net (NET-1). The value can be read from a core dump, swap, or by anything that can read the process's memory.

**Probe:** `round6_probe_panel1_password_plaintext_left_in_text_layout_cache`.

**Fix:**
- For `password` TextBoxes, build the key and the layout from `shown_text()`. Only the glyph count matters for the masked rendering.
- Drop the password from `key_string`. Use its char count plus a generation counter instead.
- Store `password` as `Zeroizing<String>`, with a fixed capacity reserved up front so `push_str` does not reallocate and leave copies behind.
- Zeroize before `clear()`, and in `activate`. `Action::WifiConnect` should carry a zeroizing type, or the host should zeroize after routing. Today `services.rs` and `route()` clone it with `a.clone()`.

## PANEL-2 [P2] A pointer Leave during a slider drag keeps the grab: hovering without a button keeps changing brightness, volume or fan limits

**Location:**
- `src/ui.rs:246` (`Pointer::Leave` clears `pointer` and `hovered` but not `pressed`)
- `src/ui.rs:236` (with `pressed` set, every `Motion` becomes `Dragged`)
- `src/ui.rs:256` (later presses are swallowed while `pressed` is set)
- consumers: `src/control_panel.rs:709` and `src/quick_adjust.rs:331`

If the compositor breaks the implicit grab, the panel gets `wl_pointer.leave` and no `release`. Triggers include a Hyprland mouse or keyboard binding mid-drag, a workspace or monitor change, or the lock screen. `hyprv-shell/src/mapping.rs` forwards Leave and Cancel unchanged as `Pointer::Leave`.

The slider stays `adjusting`. When the pointer comes back, every motion with **no button held** emits `BrightnessSet`, `SetVolume` or `FanLimit`. The probe got `BrightnessSet(10)` from plain hovering. Every press inside the panel is ignored until it closes.

QtWayland resets its button state on `pointer_leave`, and `MouseArea` gets `canceled`, so the legacy shell does not do this.

**Probe:** `round6_probe_panel2_slider_grab_survives_pointer_leave`.

**Fix:**
- On `Leave` (and on a host `Cancel`), clear `pressed` and return a `Canceled` hit.
- Panels end `adjusting` on cancel. For brightness, emit `BrightnessFinish` with the last preview value. Clear `bt_drag`.
- Separately, hyprv-shell should map `PointerEvent::Cancel` to a distinct cancel event.

## PANEL-3 [P3] The password field accepts control characters

**Location:** `src/wifi.rs:735` (`Key::Text(t) if self.focused => push_str(t)`). Routing is in `hyprv-shell/src/mapping.rs:130` (`_ => K::Text(text?)`).

Any xkb UTF-8 text is appended unfiltered:
- keypad Enter gives `"\r"` instead of submitting;
- Ctrl+U gives `U+0015`, Ctrl+Backspace and similar give other controls;
- Escape text gives `U+001B` if it reaches the field.

The resulting PSK is silently wrong, and NetworkManager rejects it as an invalid key. QML `TextInput` drops non-printable input (`QInputControl::isAcceptableInput`) and treats Enter and Return alike.

**Probe:** `round6_probe_panel3_password_accepts_control_characters`. The password ends up as `"\r\u{15}\u{1b}"`.

**Fix:**
- Filter `char::is_control` in `Key::Text`.
- Map `KP_Enter` to `Key::Enter` in hyprv-shell.
- Optionally support Ctrl+U and Ctrl+Backspace (clear), which legacy `TextInput` also lacked. That is a parity choice.

## Notes

- **Dead duplicate mapping.** `services::route` is not used by hyprv-shell, which has its own mapping in `services.rs`. The two mappings can drift. Either delete `route` or make hyprv-shell call it.
- **Mihomo busy flag.** `services::controls` hard-codes `mihomo.busy = false`, so Mihomo start/stop chips never show busy.

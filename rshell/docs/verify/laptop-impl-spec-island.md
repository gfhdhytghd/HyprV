# Laptop Phase B — spec-island report (island implementation and laptop QA)

2026-10-24. Spec: `docs/spec/laptop-island.md` (D1 = read-only). QA plan:
`docs/verify/laptop-qa-plan.md`.

## 1. Island (`crates/hyprv-ui-island`)

D1 leaves only one laptop delta, `QUIRK_LAPTOP_ISLAND_HIDDEN_WHEN_COMPACT` (§4). The island
crate is pure UI, so it gains a host-fed input, not a profile check. On desktop the bar's
`reveal` is always 1, so desktop needs no profile branch.

**`Island::set_bar_reveal(reveal, now)`** and **`Island::bar_hidden()`**
(`src/island.rs`). At `reveal <= 0` (`contentRoot.visible: reveal > 0`, `LSH:3884`):
- **No input.** The press, drag or seek grab is cancelled the way Qt hides a grabbing
  MouseArea; hover clears; `pointer()` and `wheel()` are no-ops.
- **No ticks.** The 66 ms AudioSpectrum idle waves stop (I9 is not preserved), so
  `next_deadline()` holds only pending one-shots.
- **No frames.** `animating()` is false. Behaviors keep their wall-clock targets, so the
  revealed frame shows where they would be, as in Qt.
- **No draw.** `visual_rev()` freezes at the last revealed tree, so the host's island tree
  cache does not rebuild or reconcile.
- **Model stays current.** `set_inputs` keeps applying, including agent rows and the clock.
  The first `reveal > 0` bumps the revision, so the first revealed frame renders the
  current state (§4 requirement). During the slide (0 < reveal < 1) the island renders
  normally.

**Non-blocking hook.** No change was needed. `hyprv-agent-hook` never waits for a response
(`hyprv-agent-hook.rs:97`). The installer timeouts are 5 s on both profiles
(`agent_island/install.rs`). The laptop's 86400 s blocking and response files (I2) are not
implemented.

**Not implemented (D1):** the pending card and its buttons (I1), the 5 s tick (I4) and the
list reset (I5).

**Wiring (hyprv-shell owner).** This was requested in `docs/requests/hyprv-shell.md`, and it
is already present in `assembly.rs`:
- `set_bar_reveal` is called from both island update paths.
- `Consumer::Island` is gated on `bar_hidden()`.

**Tests.** `src/island/laptop_tests.rs` (3 tests):
- hidden: no deadline, no animation and a frozen revision while inputs change; no input;
- the revealed state equals a fresh island; the wave re-arms after reveal;
- hiding cancels a press;
- reveal 1 or more is a no-op.

**Results.** `cargo test -p hyprv-ui-island` passes everything: lib 21, behaviour 7,
flickable 3, calendar morph 1 and island parity 2. Desktop parity code paths are untouched
when `set_bar_reveal` is never called. Scoped clippy has no island warnings.

## 2. Laptop QA

**E2E result (2026-10-24, pinned release `81068a07…`, built 11:26).**
- **38/38 PASS** and the fakes 7/7 (`docs/verify/laptop-e2e.md` lists every case).
- The 32 spec cases, plus the suite's extra `lap.strip.control/stats/clock/workspace` and `ipc.shell.kill`.
- `lap.desktop.regression`: desktop suite **204/204** under auto-detect.
- Coverage gaps were filed to hyprv-shell.

**E2E runner (`parity/laptop/e2e/run-laptop-e2e.py`).**
- It runs the nested laptop suite with:
  - `HYPRV_PROFILE=laptop` and `HYPRV_FUNCTIONAL_FILTER=lap.`, so the desktop `e2e.md` is never touched;
  - the fake bus as session and system bus, plus the fake sysfs at `HYPRV_SYSFS_ROOT=<run>/fs`, a filesystem root;
  - the controls stub, at 1440×900 @2.
- It merges the cases by ID. `--desktop` covers `lap.desktop.regression`.
- `lap.brightness.slider` is corrected to the laptop spec (Set only, never Finish).

**Scenario driver.**
- `org.hyprv.Test.Step`/`State` on the fake bus. The presets are plug, unplug, full, `tick:N`, `lid-close`/`lid-open`,
  suspend/resume and `profile:X`. Each is applied synchronously and the bus is flushed before the reply.
- The fakes self-test passes 7/7.

**Parity diff (`parity/laptop/verify-laptop.py`).**
- It uses the desktop gate against the laptop references and writes `docs/verify/parity-laptop.md`.
- `--regions` gives a compare-inside-only mode; that is the `laptop-panels` contract.
- Runs merge across owners.

**Theme-toggle icon at @2.**
- The root cause is a harness artifact in the laptop reference: an eager CP Loader plus a pre-slot load at qApp DPR 1.
- The fix and recapture are pending (`laptop-qa-plan.md` §8).

**Capture host fix and recapture (2026-10-24).**
- parity-host builds laptop scenes inside the scale-N window (`PARITY_SCENE_IN_WINDOW=1`).
- The laptop references were recaptured: `laptop-bar` 182 and `laptop-panels` 106 images changed.
- The laptop CP replay is **92/92**.
- **B1 PASS:** 992/992 desktop images unchanged.

**Final consolidated run (2026-10-24 12:52, `parity/laptop/run-laptop-qa.py`): PASS.**
- Shell: `target/release/hyprv-shell` built 12:35, sha256 `3cd717d8…` (a pinned copy).
- The full table is in `docs/verify/laptop-e2e.md` and `docs/verify/parity-laptop.md`.

| area | check | result |
| --- | --- | --- |
| e2e | fakes self-test | 7/7 PASS |
| e2e | laptop nested suite (`lap.*`, `ipc.shell.kill`) | 37/37 PASS |
| laptop parity | `laptop-bar` (full frames) | 248/248 PASS |
| laptop parity | `laptop-panels` CP / QuickAdjust (card region) | 92/92, 32/32 PASS |
| laptop parity | `laptop-panels` battery popup | 28/28 PASS |
| laptop parity | island (= main, D1), GL | 380/380 PASS |
| desktop regression | desktop nested e2e | 204/204 PASS |
| desktop regression | desktop parity GL island / bar / panels | 380 / 300 (+12 excluded) / 300, equal to the baseline |

The earlier 38/38 counted `lap.desktop.regression` as a case. It is now its own row (204/204).

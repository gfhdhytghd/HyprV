# Requests to hyprv-core

## From svc-media (`crates/hyprv-svc-media`)

1. **`ServiceRuntime` must not self-join on its own thread.** If the last
   `Arc<ServiceRuntime>` is dropped on the `hyprv-services` thread (e.g. a task captured
   a `Context` holding it), `Drop` calls `thread.join()` on itself and panics with
   `failed to join thread: Resource deadlock avoided (os error 35)` (observed in
   svc-media live tests before working around it). Suggest: in `Drop`, skip the join
   when `thread::current().id()` equals the runtime thread's id (just signal shutdown).
   svc-media now only moves bus handles into tasks, but every service crate can hit this.

2. **`SamplingLease` / combined demand.** The core README mentions `SamplingLease`, but
   core only provides `Topic::demand()` + `run_sampled`. Spectrum needs "subscribers > 0
   AND an external gate" (media playing); svc-media builds a private
   `watch<usize>` combiner. A generic `demand_all([watch<usize>/watch<bool>…]) ->
   watch::Receiver<usize>` helper in `service` would avoid each crate re-implementing it.

3. **Shared resume signal.** Several actors need logind `PrepareForSleep(false)` to
   re-snapshot (MPRIS position anchors here). `clock::logind_sleep` only feeds a calloop
   channel. A store topic (e.g. `SleepTopic { sleeping: bool, resume_generation: u64 }`)
   published once by core would avoid one system-bus subscription per service crate.

## Status (2026-10-23, hyprv-core now owned by svc-media agent)
1. Done — `ServiceRuntime` drop skips the join on its own thread; `shutdown_and_join(self)` added.
2. Done — `service::gated_demand(demand, gates, shutdown)`.
3. Done — `clock::SleepTopic` + `clock::run_sleep_topic`. **Shell host must spawn
   `run_sleep_topic` once** (svc-media's mpris now relies on it for resume re-anchoring).

## From svc-desktop (`crates/hyprv-svc-desktop`)

1. **Name-filtered / file-level watch.** `config::FileWatch` wakes on any event in the
   parent directory. In `~/.local/state/hyprv` the fan controller rewrites
   `fan-percent.lock` at 1 Hz, so any watcher there wakes every second. svc-desktop ships
   `util::Watch` (specs `(dir, &[names])`; existing named files are watched directly with
   `IN_CLOSE_WRITE|IN_ATTRIB|IN_DELETE_SELF|IN_MOVE_SELF`, missing ones through a
   name-filtered directory/ancestor watch). Suggest promoting it (or equivalent) into core
   and using it for `watch_config`.
2. **Shared service context.** Each svc crate defines its own context
   (`store`, runtime handle, shutdown receiver, resolved XDG/env paths). A core
   `ServiceCtx { store: Arc<StateStore>, rt: Handle, shutdown: watch::Receiver<bool>, env }`
   with an `Env` that tests can root in a temp dir would let the shell host construct one
   value for every crate. svc-desktop's `Ctx`/`Env` (`src/lib.rs`) can serve as the template.

Engine profile module landed under explicit Phase B scope (profile.rs + lib export),
with process-latch/root/override/DMI/battery/escape tests. Please document it in your
core README when updating other modules: core::profile::{Profile, profile, sys_root,
sys_path}, root filesystem default /, fake sysfs at ROOT/sys/class/...; no independent
profile detection. ARCHITECTURE.md records implementation and superseding root rule.

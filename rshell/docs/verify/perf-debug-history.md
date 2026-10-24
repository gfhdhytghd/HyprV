# Same-window service-probe / Quickshell process measurement

Baseline and active modes measure the probe and the live main-shell Quickshell process plus descendants concurrently for 60 seconds; separately labelled isolation diagnostics use 10-second windows. Quickshell was only observed; no panel visibility, service or compositor state was changed. The separate DankCal Quickshell config is listed in session.json and excluded from the main-shell family.

`idle`: all topics are initially subscribed; after 5 s only demand-driven subscriptions are released. Measurement begins after 8 s. Event-driven subscriptions stay live. `active`: all safe topics retain demand (stats/battery/Mihomo/weather). Spectrum capture stays off; backlight performs only sysfs/DDC Get VCP reads, and absent helper is never activated.

Latest 60-second idle result: **target not met** (12.080% of one core, 378.53 scheduling events/s). Event counts and matched group diagnostics below constrain interpretation; they do not certify every readiness branch.

## Results

| Mode | Process family | CPU seconds | CPU % of one core | RSS start/end KiB | Scheduling proxy / s | Context switches / s | Exited threads |
|---|---|---|---|---|---|---|
| idle | probe | 7.260 | 12.080 | 48172 / 48600 | 378.53 | 378.53 | 0 |
| idle | quickshell | 1.540 | 2.562 | 356548 / 357924 | 71.53 | 71.53 | 0 |
| after-sys-review-idle | probe | 10.520 | 17.504 | 47164 / 46188 | 676.62 | 676.62 | 0 |
| after-sys-review-idle | quickshell | 1.480 | 2.462 | 357068 / 356936 | 55.44 | 55.44 | 0 |
| initial-idle | probe | 24.020 | 39.948 | 43104 / 42160 | 661.14 | 661.14 | 0 |
| initial-idle | quickshell | 1.570 | 2.611 | 410568 / 410016 | 72.10 | 72.10 | 0 |
| no-agent | probe | 41.830 | 69.577 | 36468 / 36908 | 1225.97 | 1226.00 | 2 |
| no-agent | quickshell | 1.520 | 2.528 | 409296 / 409316 | 58.05 | 58.05 | 7 |

idle state publications during the interval: `{'agent_island': 116, 'tray': 2, 'hyprland': 2, 'systemd': 3}`.
after-sys-review-idle state publications during the interval: `{'agent_island': 74, 'hyprland': 2, 'tray': 2}`.
initial-idle state publications during the interval: `{'agent_island': 125, 'notifications': 1, 'tray': 4, 'hyprland': 4}`.
no-agent state publications during the interval: `{'systemd': 1, 'tray': 6, 'hyprland': 7}`.

`after-sys-review-idle`, `initial-idle` and `no-agent` are earlier build snapshots, retained as diagnostic history. They are not matched interventions against the current idle build. Each raw payload retains its source hashes where available.

## Earlier matched-binary isolation

These four sequential 10-second windows used the same earlier binary, with media omitted during an in-flight source transition. They identified paths for owner investigation; they do not represent the final build.

| Exclusion | Probe CPU % | Scheduling proxy / s |
|---|---|---|
| all | 105.429 | 1075.53 |
| no-agent | 75.271 | 1146.53 |
| no-viewflow | 47.610 | 945.98 |
| neither | 17.887 | 869.75 |

## Latest matched-binary group diagnostics

Each group-only window lasts 10 seconds after 8 seconds warmup; demand is released at 5 seconds. These are separate sequential observations, not additive CPU attribution. Their embedded source hashes identify the pre-follow-up build.

| Included group | CPU % | Scheduling proxy / s | State events |
|---|---|---|---|
| desktop | 3.253 | 22.57 | {'agent_island': 18} |
| sys | 0.099 | 3.95 | {'hyprland': 1} |
| power | 14.035 | 707.89 | {'backlight': 2} |
| net-media | 0.000 | 0.00 | {} |

## Limits

The third `/proc/PID/task/TID/schedstat` field counts scheduling timeslices, **not exact wakeups**. Voluntary/nonvoluntary context-switch totals are also scheduling proxies. Exited threads and short-lived legacy child processes have incomplete scheduling counters because only window endpoints are read; reported scheduling rates are lower bounds. CPU uses utime+stime+cutime+cstime over surviving family members so reaped child CPU is included. RSS is a family sum, not PSS/USS, and shared pages may be counted multiple times. Raw endpoint counters and process/thread identities are retained in `raw/perf-*.json`. No perf privileges, tracing changes or sysctls were used.

This is an active multi-agent desktop: hook files, tray icons, compositor events and external network/provider signals may be genuine activity. JSON serialization/publication is part of probe CPU, while rendering is part of the live Quickshell family. Do not interpret the difference as a causal speedup of a fully integrated replacement UI. Missing providers can create retry deadlines; active sampling deliberately creates timer wakeups. A zero-idle target is not certified by this measurement.

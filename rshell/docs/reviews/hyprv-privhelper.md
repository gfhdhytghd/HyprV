# Adversarial review: hyprv-privhelper

Reviewed 2026-10-23. Production crates and packaging were read-only. Findings were verified with source inspection and isolated probes under [scratch/round2](scratch/round2/README.md); [source fingerprints](scratch/round2/source-hashes.json) and [results](scratch/round2/results.json) identify the audited snapshot. Shared core/media lifecycle code changed during the review; affected copies were refreshed and probes rerun against the final fingerprinted source.

P1 means a high-priority security, cooling-safety, starvation, deadlock or resource-lifetime defect; P2 means correctness or deployment risk. Expected regression assertions deliberately fail; this is not a claim that the original unit suite failed. No root execution, live hardware writes, real-bus mutations, installs or service/compositor restarts occurred. Physical fan/thermal behavior, real polkit prompts and systemd sandbox execution remain unverified. Run `python3 docs/reviews/scratch/round2/verify.py` to reproduce.

The D-Bus caller is resolved through GetConnectionUnixUser, channel/percent inputs are allowlisted, polkit uses the actual unique sender, and no caller path/command is accepted. Final sysfs writes use O_NOFOLLOW; sysfs ancestors are kernel-controlled. No arbitrary sysfs path traversal or polkit wire-subject forgery was established. The persistence ownership issue below is distinct from symlink rejection.

<a id="priv-1"></a>
## PRIV-1 [P1] User-controlled persistence bypasses polkit and defeats singleton ownership

**Location:** `crates/hyprv-privhelper/src/cooling.rs:82` (directory traversal), `crates/hyprv-privhelper/src/cooling.rs:190` (locks), `crates/hyprv-privhelper/src/cooling.rs:202` (import); fixed path at `crates/hyprv-privhelper/src/main.rs:196`.

**Failure scenario:** The helper validates symlinks and file type but never checks owner/mode of the directory, ancestors, state file, or lock files. Its production state directory is inside wilf's writable home. That user can replace even a root-owned mode0600 `fan-percent` by renaming a new file into the directory. On the next helper start, `chassis=0/pump=0` is accepted without calling polkit; the desktop session then receives zero caps. The same directory owner can unlink/recreate `fan-control.lock` while the first controller retains its old inode, allowing a second controller to acquire a different lock and race PWM writes. This bypasses the administrator authorization required by SetFanLimit; it is not a claim of arbitrary root file writes or an unauthenticated remote D-Bus exploit.

**Verification:** `review_helper_untrusted_state_cannot_bypass_authorization` imports user-owned zero limits and observes both fans at PWM0 at 100°C. `review_helper_user_cannot_replace_singleton_lock` unlinks the held lock and successfully opens a second Controller. Both fail their protection assertions under the ordinary non-root test account in scratch. The source inspection confirms that the production path and ownership checks are identical. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log).

**Suggested fix:** Put authoritative limits and singleton/state locks in root-owned, non-user-replaceable directories (for example /var/lib/hyprv and /run/hyprv); validate ownership/type/mode through retained FDs. Import legacy user data once through an explicit authorized migration, then cease trusting it. Keeping a directory FD prevents path redirection but does not prevent directory-owner replacement of entries. This directly violates the unsafe-ownership rejection requirement in docs/spec/services.md:138.

<a id="priv-2"></a>
## PRIV-2 [P1] Malformed saved state aborts before boot cooling is established

**Location:** `crates/hyprv-privhelper/src/cooling.rs:204` and `crates/hyprv-privhelper/src/main.rs:194`.

**Failure scenario:** A saved state file containing invalid UTF-8 causes read_to_string to return an error through Controller::open. main propagates it before controller.tick(false), before the safety actor, and before D-Bus is available. A truncated multibyte sequence at the 4096-byte read boundary has the same effect. systemd Restart=always repeats the same startup error; it cannot repair the file. If hardware was already left in manual mode by the previous process, there is no new boot-cooling write. This availability defect remains relevant after moving persistence to trusted storage: corrupted configuration must not prevent establishing safe hardware policy.

**Verification:** `review_helper_bad_state_does_not_prevent_boot_cooling` writes one byte 0xff into fake persisted state and verifies open fails. All parent paths are canonicalized first, so this is the decoding error rather than the controller's parent-component rejection. The main startup order was traced directly. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log).

**Suggested fix:** Establish a conservative autonomous/boot policy independently of reading saved desktop limits. Treat malformed, oversized, or unreadable optional state as a diagnostic plus conservative defaults, without terminating the cooling controller. Parse bounded bytes atomically, and do not partially import malformed configuration.

<a id="priv-3"></a>
## PRIV-3 [P1] Manual zero PWM survives helper teardown without an independent fail-safe

**Location:** `crates/hyprv-privhelper/src/cooling.rs:305` (manual writes), `crates/hyprv-privhelper/src/main.rs:267` (shutdown), `crates/hyprv-privhelper/packaging/hyprv-privhelper.service:16` (restart only).

**Failure scenario:** A cool package makes tick set both pwmN_enable=1 and PWM0. Shutdown aborts the safety task and drops the Controller; neither has cleanup restoring a hardware-controlled mode or a safe duty. Those sysfs settings persist after the process exits. A restart normally shortens the gap, but a killed/stalled process or repeated startup failure (PRIV-2) leaves no software temperature response. Restart=always/1s is recovery scheduling, not autonomous thermal protection. There is no watchdog or crash-independent handoff in the supplied unit/controller.

**Verification:** `review_helper_shutdown_restores_autonomous_cooling` performs a real Controller tick against fake sysfs, drops it, and observes pwm1_enable still 1 and PWM still zero. Inspection verifies main aborts rather than handing control back, and the unit has no independent fail-safe. No SIGKILL of a real service or physical thermal test was performed. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log).

**Suggested fix:** Use a hardware autonomous curve/watchdog where supported so loss of the process is safe. Add a validated graceful handoff to that mode, best-effort error cleanup, and supervisor/watchdog recovery with bounded startup. A Drop or ExecStop hook alone cannot handle SIGKILL, power loss, or a hung process; explicitly document and validate the hardware fallback before deployment. docs/spec/services.md:72 requires crash policy to retain cooling.

<a id="priv-4"></a>
## PRIV-4 [P1] Failure on the first channel prevents safety writes to the second

**Location:** `crates/hyprv-privhelper/src/cooling.rs:312` and `crates/hyprv-privhelper/src/cooling.rs:324`.

**Failure scenario:** tick processes chassis first and returns on its first enable/PWM write error. If pwm1 is missing or denied while pwm7 is healthy and still zero, a hot CPU never causes a write to pwm7. The next one-second retry repeats the same early return. It also switches a channel into manual mode before verifying that the subsequent duty write can succeed, without restoring autonomous mode on error. Publishing FanState.last_error does not recover the healthy channel.

**Verification:** `review_helper_one_channel_error_does_not_skip_other_channel` sets a fake CPU to 100°C, removes only pwm1, and calls tick. It returns an error after changing pwm1_enable; healthy pwm7 remains zero. The assertion that healthy-channel cooling proceeds fails. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log).

**Suggested fix:** Attempt every channel independently and aggregate errors after best-effort safe writes. Validate/open the required attributes before changing modes, choose an appropriate duty/mode transition order for this chip, and restore autonomous/safe operation when a partial transition fails. Do not let one failed register suppress the other chassis fan's safety policy.

<a id="priv-5"></a>
## PRIV-5 [P1] Over-range hot telemetry is discarded in favor of a cool package; no emergency override exists

**Location:** `crates/hyprv-privhelper/src/cooling.rs:72` and `crates/hyprv-privhelper/src/cooling.rs:13`.

**Failure scenario:** With package readings 60000 and 130000 m°C, Hardware::temperature filters out the hotter one before taking max. It reports 60°C, and the controller writes zero PWM to both channels with no error. Thus a dangerous reading or sensor-range fault reduces cooling instead of entering a conservative fault policy. Separately, a saved zero cap produces PWM0 even at 120°C or with missing telemetry. The zero-cap behavior follows the explicitly specified legacy cap curve; it is a safety-design gap, not a claim that the implementation violates that formula.

**Verification:** `review_helper_overrange_hot_sensor_does_not_choose_cold_sensor` reproduces temperature_c=60 and PWM0 for both fans with those two sensors. `review_helper_critical_temperature_overrides_zero_cap` demonstrates target_pwm(0,120000)=0. These are arithmetic/fake-sysfs results, not proof of physical overheating or of a particular chip's sensor range. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log).

**Suggested fix:** Distinguish missing/invalid/over-range telemetry from trustworthy cool readings. Latch/report sensor faults and choose conservative cooling rather than silently discarding a hot outlier. Define a reviewed critical-temperature/fault policy independent of user caps and RPM/stall detection where appropriate. An emergency override requires an explicit update to the legacy-curve contract in docs/spec/services.md:70; do not silently change normal slider semantics.

<a id="priv-6"></a>
## PRIV-6 [P2] Repository-root packaging still deploys the obsolete, nonfunctional helper contract

**Location:** `packaging/hyprv-privhelper.service:14`, `packaging/hyprv-privhelper.service:19`, `packaging/org.hyprv.Shell.PrivHelper.conf:7`; compare `crates/hyprv-privhelper/packaging/hyprv-privhelper.service:23`.

**Failure scenario:** A packager taking the conventional repository-root packaging files installs ProtectHome=yes although main requires /home/wilf/.local/state/hyprv, forbids AF_NETLINK although startup creates a hwmon netlink socket, grants no writable sysfs/state exception, omits persistent restart, and uses a policy lacking GetFans/ProtocolVersion property reads needed by the service client. Startup/control consequently cannot fulfill the implemented protocol. The crate README explicitly tells deployers to use the crate-local packaging instead, so this is a conflicting release-artifact hazard, not evidence that the intended crate-local unit or a live host is already misconfigured.

**Verification:** `packaging_probe.py` parses the actual top-level D-Bus XML and checks the unit directives against the executable startup requirements; its one regression assertion lists all five mismatches in packaging.log. Both packaging sets were inspected. No unit was installed, launched, or sandbox-tested. See [hyprv-privhelper probe log](scratch/round2/hyprv-privhelper.log) and [packaging log](scratch/round2/packaging.log).

**Suggested fix:** Maintain one authoritative packaging set and point build/install manifests at it. Remove the obsolete skeleton artifacts or replace them with an unmistakable non-installable notice; validate policy members and sandbox paths against the implemented API. Keep the separate deployment coordination for legacy fan permissions/controller described in the README.


# hyprv-svc-net

Native NetworkManager and BlueZ actors for the Rust shell. Implementation lives entirely in this crate; it does not modify Quickshell, Hyprland, the core crate, workspace metadata or system configuration.

## Integrating the actors

```rust,no_run
use std::sync::Arc;
use hyprv_core::{runtime::ServiceRuntime, store::StateStore};
use hyprv_svc_net::{Context, network, bluetooth};

let runtime = Arc::new(ServiceRuntime::new()?);
let store = Arc::new(StateStore::default());
let ctx = Context::new(store, runtime.clone());
let network = network::spawn(ctx.clone());
let bluetooth = bluetooth::spawn(ctx);
let wifi_subscription = network.state.subscribe();
let bluetooth_subscription = bluetooth.state.subscribe();
// Forward changed_diff() to the UI with ServiceRuntime::bridge.
// Render network.prompts / bluetooth.prompts in the shell's prompt UI.
// On explicit user interaction, dispatch(Action) then await PendingAction::result().
// On shutdown request runtime.request_shutdown(), and allow actors to finish
// cleanup before dropping the runtime. A pending widget action survives unmount.
# Ok::<(), Box<dyn std::error::Error>>(())
```

Both modules export serde `State`, `Action`, `StateTopic`, `Handle`, `spawn(Context) -> Handle` and an injectable async `Transport`. Topics come from core `StateStore`, suppress equal values, and retain their last observed state. Actors use core `Service` start/stop and core runtime/shutdown; there is no alternate store or sampling scheduler. Each module has a bounded action queue. Prompt replies bypass the action queue so a pending pairing/password method never blocks its own UI response or incoming bus events.

`Handle::dispatch(action).await?` returns an independently owned completion. `Handle::respond(prompt_id, Reply)` answers an agent request. `Reply` and Wi-Fi `Action` deliberately have no Debug/Serialize implementations: credentials must not enter state, logs or diagnostic IPC. Both wipe any password they still hold when dropped; internally every secret lives in `zeroize::Zeroizing` buffers, and settings maps carry secrets as `secret::SValue::Secret`, which serializes the zeroizing buffer by reference into the D-Bus message (residual: zbus's own message buffers are outside this crate's control). Network and Bluetooth prompt IDs are local to their respective handles. `Context::read_only = true` disables agent registration and rejects every dispatched action before the transport is called; this is the mode used by live inspection tests.

States include `available`, `error`, action busy/message fields and all QML-visible Wi-Fi/Bluetooth snapshot roles. Wire/display field names serialize in camelCase. Network AP identity retains raw SSID bytes, while the visible label uses lossy UTF-8. APs are deduplicated by raw SSID, prefer active AP then strongest, and sort active/strength/name. Bluetooth uses a deterministic powered adapter preference, filters its device list to that adapter, suppresses address-shaped names and orders connected/paired/trusted/name, with stable address ties. A failed snapshot does not masquerade as available hardware.

Sources: `quickshell/BarServices.qml:14`, `quickshell/shell.qml:2003`, `quickshell/shell.qml:2119`, `quickshell/shell.qml:2177`. `docs/spec/services.md` has the larger audited source/contract inventory.

## Protocol and action coverage

| Domain | Implementation |
|---|---|
| Network state | System bus NetworkManager root/device/wireless/AP properties, settings connection enumeration, primary route interface and wired/other connectivity. Signal match rules precede initial enumeration. Properties/device/AP/profile changes refresh the retained snapshot. |
| Wi-Fi actions | WirelessEnabled Set, RequestScan, device Disconnect, ActivateConnection for existing profiles, AddAndActivateConnection2 for new profiles. An empty password is "no password" and never touches a saved secret.<br>A password typed for a saved profile never changes its secret flags:<br>• AGENT_OWNED: stored in the keyring and handed to the agent for this activation;<br>• NOT_SAVED: handed to the agent for this activation only;<br>• NOT_REQUIRED: ignored;<br>• system-owned: replaced in place via `Update`, flags untouched.<br>New profiles store their secrets **system-owned** (flags NONE): no `*-flags` key, exactly what the legacy `nmcli dev wifi connect … password` path created (`wifi-action.sh:64`). The password lives in the NetworkManager profile, so it is available before login and auto-connects at boot.

This is a deliberate choice of legacy functional parity over plasma-nm, whose default is AGENT_OWNED in the keyring. The plasma-nm semantics still apply everywhere else:
- existing profiles' flags are never changed;
- empty input never overwrites a secret;
- existing AGENT_OWNED secrets use NetworkManager's Secret Service schema;
- NOT_SAVED/NOT_REQUIRED are respected. Unknown or malformed SSIDs fail before activation. |
| Security | Open, WPA-PSK, SAE/WPA3, WEP, OWE, and enterprise profiles. EAP identity/method, anonymous identity, phase2 auth/autheap, CA certificate or system CA domain validation, client certificate/private-key file references and password prompts. Enterprise setup emits a dedicated prompt rather than spawning a connection editor. |
| SecretAgent | Exports the standard NetworkManager SecretAgent object and GetSecrets, CancelGetSecrets, SaveSecrets, DeleteSecrets, authenticating every call against the daemon's current unique bus owner (`PermissionDenied` otherwise). Secret flags follow plasma-nm/NetworkManagerQt:<br>• Without REQUEST_NEW the agent uses, in order: secrets NetworkManager passed, a one-shot secret typed for this activation, then the keyring, the keyring only for AGENT_OWNED keys (never NOT_SAVED or NOT_REQUIRED).<br>• It prompts only for REQUEST_NEW or when a needed secret is missing and interaction is allowed. Empty input never counts.<br>• Typed AGENT_OWNED secrets are stored.<br>• `CancelGetSecrets` cancels the request at any stage (keyring lookup, keyring prompt, user prompt) and answers `AgentCanceled`. An explicit Reject answers `UserCanceled`; a prompt timeout answers `NoSecrets`.<br>• `SaveSecrets` stores AGENT_OWNED secrets and removes keyring copies of keys that are no longer agent-owned. |
| Secret persistence | freedesktop Secret Service in NetworkManager's standard schema `org.freedesktop.NetworkManager.Connection`, the schema libnma, nm-applet and plasma-nm's Secret Service backend use.<br>• Each key is its own item with `connection-uuid`, `setting-name` and `setting-key` attributes, plus `xdg:schema` when stored. The label is "Network secret for ID/SETTING/KEY".<br>• Lookups do not match the schema name, so items written by other NetworkManager agents are found.<br>• The transport session is `dh-ietf1024-sha256-aes128-cbc-pkcs7`: RFC 2409 group 2, constant-time exponentiation, HKDF-SHA256, AES-128-CBC/PKCS#7, random IV per secret. `plain` is used only if the service refuses DH.<br>• Keyring prompts are dismissed when the waiting request is cancelled.<br>• Only AGENT_OWNED secrets are stored. There is no plaintext disk fallback. A keyring failure never fails an activation the user already typed secrets for. |
| Bluetooth state | BlueZ ObjectManager enumeration, InterfacesAdded/Removed and PropertiesChanged; adapter Powered/Discovering/Pairable and device identity/icon/Paired/Bonded/Trusted/Connected/Blocked/RSSI. |
| Bluetooth actions | Power, four-second discovery and explicit stop, Pair → Trusted → Connect when not paired, connect/disconnect/remove, trust/block setters and cancel-pairing. Uses observed pairing state rather than trusting a stale caller-supplied `paired` boolean. Overall UI action timeout is twelve seconds; pairing timeout cancels the provider request and UI prompt. |
| Pairing Agent1 | KeyboardDisplay registration and default-agent request; PIN, passkey, display PIN/passkey/entered count, numeric confirmation, pairing/service authorization, Cancel and Release. Incoming caller must match BlueZ's current owner. PIN/passkey validation occurs before a reply is sent. |
| rfkill | Read-only nonblocking `/dev/rfkill` event records, soft/hardware block per radio type. Inotify on `/dev` handles later device creation/permission changes without a retry timer. An inaccessible device is explicitly reported; NetworkManager WirelessHardwareEnabled remains available independently. |
| Provider restart | NameOwnerChanged distinguishes loss from appearance.<br>• On loss: pending jobs and prompts are cancelled (`AgentCanceled`), registration is forgotten, and state becomes unavailable ("NetworkManager is not running" / "Bluetooth service (org.bluez) is not running"). **Nothing is sent** to the ownerless name, so a stopped NetworkManager or bluetoothd is never D-Bus-activated.<br>• On appearance: the agent re-registers and state refreshes.<br>Daemon presence comes from `NameHasOwner`, which never activates, both at startup and on these events. Provider calls also carry `NO_AUTO_START`. Registration failures stay visible in State.error. |
| Shutdown | Drops pending prompt responders, aborts action jobs, releases owned discovery, unregisters/removes exported agents and drops signal watchers. Core supervisor bounds shutdown waiting to three seconds. |

Wi-Fi source actions: `quickshell/shell.qml:2582`, `quickshell/scripts/wifi-action.sh:25`. Bluetooth source actions and timing: `quickshell/shell.qml:2599`, `quickshell/shell.qml:2621`, `quickshell/shell.qml:3080`, `quickshell/shell.qml:3088`; fallback script's 35-second pairing timeout is a different path from the active native UI's twelve-second action deadline. Main UI state roles follow `quickshell/shell.qml:2119`; fallback shell-script RSSI is preserved as an additional role rather than fabricated as zero.

Enterprise prompt `Reply::Fields` keys are `identity`, `eap` (comma-separated method list), optional `anonymousIdentity`, `phase2Auth`, `phase2Autheap`, `caCert`, `domainSuffixMatch`, `clientCert`, `privateKey`, and `password`. EAP-TLS requires client certificate/private key. Subsequent `NetworkSecrets` prompts use NetworkManager setting secret names such as `psk`, `password`, `wep-key0`, `private-key-password`. New profile secrets default to NOT_SAVED; existing profile secret ownership is honored. Use a preconfigured profile for options outside the exposed enterprise configuration fields.

Official protocol references: [NetworkManager root](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html), [SecretAgent](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.SecretAgent.html), [AgentManager](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AgentManager.html), [BlueZ Agent1](https://github.com/bluez/bluez/blob/master/doc/org.bluez.Agent.rst).

## Wakeup and lifecycle contract

Desktop behavior has no polling loops, CLI subprocesses or sample timers. Laptop scanning has the explicit page lease described below. Neither domain samples a metric, so neither acquires a SamplingLease: D-Bus, rfkill and inotify readiness are event sources. Timers exist only for an outstanding prompt/action, requested discovery, and bounded shutdown. Virtual-clock tests advance an idle actor by an hour and verify no extra discovery calls. No timer is used to discover missing hardware or restart a missing daemon. Signals arriving during an initial snapshot remain queued and cause a fresh snapshot afterward.

Retain one handle per domain for the whole shell. UI subscriptions may come and go without owning the agent or connection lifetime. Release exclusive popup keyboard focus before allowing an external keyring unlock dialog. Agent prompt widgets should call respond with Reject on explicit cancellation; prompt disappearance must remove the widget. Dropping the action completion future does not implicitly cancel system work.

## Verification

Use the shared workspace target, with incremental compilation disabled:

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p hyprv-svc-net
HYPRV_PROFILE=laptop CARGO_INCREMENTAL=0 cargo test --locked -p hyprv-svc-net
CARGO_INCREMENTAL=0 cargo clippy --locked -p hyprv-svc-net --all-targets -- -D warnings
```

Do not create private Cargo targets or use the historical isolated `check.sh` workflow for laptop validation.

Regression tests for review round 5 (`docs/reviews/hyprv-svc-net.md`; `net1`…`net7`, `net8_net9…`, `bt1…`, `control_*`) run the real agents against fake NetworkManager, BlueZ and Secret Service peers on a private `dbus-daemon`. No test touches the real system or session bus: in test builds the keyring refuses the user bus. A scanning test allocator verifies that secret buffers are zeroized before they are freed. The deterministic suite also covers raw SSID deduplication, profile settings, QML field serialization, Bluetooth ObjectManager mapping/removal, rfkill records, prompt reply/cancel/drop behavior, secret cache/new-secret flags and ownership, mock action success/failure, identical-state suppression, idle snapshot suppression, provider-owner changes, observation while prompts are pending, read-only rejection, shutdown discovery cleanup, four-second scan expiry and twelve-second pairing cancellation.

Read-only live inspection is opt-in:

```sh
./crates/hyprv-svc-net/check.sh --test read_only_bus -- --ignored --nocapture
```

This opens the system bus, installs read-only signal matches, and reads NetworkManager/BlueZ properties. It explicitly does not export/register agents, activate connections, scan, pair, set properties, invoke keyring operations or control services. Current host inspection passed with one AP and three Bluetooth devices; these counts describe the verification run, not a persistent machine property.

## Remaining integration and manual validation

- The shell binary/UI is owned by other agents. It must instantiate these actors, bridge subscriptions, render/answer prompts, and release exclusive input for any native keyring dialog. This crate does not edit that integration.
- Real Wi-Fi activation, enterprise authentication, keyring unlock/save/delete, Bluetooth pairing/authorization/trust, rfkill hotplug/permission recovery and real daemon restart behavior have **not** been exercised: the authorized live test is read-only. On a separately approved test network/device, verify success/cancel/auth-denied paths and resulting observed properties, not only method return values.
- Daemon owner disappearance/reappearance is handled on an intact bus. Loss/replacement of the entire system-bus connection produces an explicit unavailable state and ends that actor; the shell supervisor must respawn it. No background reconnect polling is hidden here.
- This is the shell's Wi-Fi/802.1X secret agent, not a general VPN/WWAN secrets UI. Existing enterprise profiles retain options not expressible through the supplied setup fields; vendor EAP/token configuration requires a preconfigured NetworkManager profile.
- Native D-Bus behavior, registration/polkit permissions and UI prompts need deployment-level validation. Passing build/unit/read-only checks does not prove live mutation, visual parity or measured zero wakeups of the complete shell.


## Laptop Phase B (2026-10-24)

Uses `hyprv_core::profile::{profile, Profile}`; no independent detector. Desktop state fields and explicit actions retain their existing behavior. Laptop selects the first connected Wi-Fi radio, else the first radio, matching pinned `origin/laptop@24972f7` `BarServices.qml:18–20`. Bluetooth QML has no service delta: passive BlueZ signals and the existing user-requested four-second discovery remain unchanged.

`network::Handle::scan_lease.acquire()` returns a core `Subscription<()>` guard. Hold a guard only for a **requested Wi-Fi page**, including the existing control-panel action that opens the standalone Wi-Fi popup. Generic control-panel/bar presence and network-state subscriptions are not demand. Multiple pages share one scanner; drop the final guard immediately on requested close/suspend, before the exit animation. On laptop, `State.scanning` projects the page's scannerEnabled setting when a Wi-Fi device exists. A hidden explicit Rescan is rejected before transport access; desktop Rescan requires no page lease.

Visible scanning uses Quickshell's 10001-ms cooldown (`src/network/nm/wireless.hpp`, `NMWirelessDevice::onScanTimeout` in `wireless.cpp`), considering both cached NM LastScan (CLOCK_BOOTTIME milliseconds) and last request. Reopening does not defeat the request cooldown. There is no timer while hidden, read-only, radio-disabled, hardware-blocked, device/provider-absent, or after shutdown. Provider appearance re-enables a visible scanner through owner-change signals. Hidden transitions cancel queued/in-flight client requests and clear deadlines; a RequestScan already delivered to NetworkManager cannot undo the hardware scan it started. RequestScan uses NO_AUTO_START. No timer polls a missing provider. Errors remain observable.

Network rates are owned by `hyprv-svc-power::sysstats`, not this crate. Only the actually painted rate module/stats consumer may hold a `Metric::Network` lease; compact CPU/RAM and generic panel observers must not request it. Shell/power owners receive the integration contract in their requests files. Reuse their shared scheduler and baseline reset; never introduce a second counter sampler here.

NM/BlueZ constructors use zbus's DBUS_SYSTEM_BUS_ADDRESS support, with no error fallback to the host. Test contexts inject a private bus; a fresh-child invalid-address test covers both native constructors. No sysfs paths are read by this crate. rfkill remains readiness-driven, with no retry timer. Private bus tests disable the real rfkill transport.

Laptop regressions use private dbus-daemon NM/BlueZ fakes and a shared virtual actor clock. They check no hidden discovery/snapshot reads over an hour, lease coalescing/reopen/close, provider loss/reappearance, read-only refusal, backend scan-age arithmetic, pending-request cancellation, and one-shot Bluetooth expiry. The desktop case checks model JSON byte identity while holding a page lease and explicit Rescan without a lease. Complete UI pixel identity, nested e2e, measured laptop CPU/RSS, physical radio behavior and suspend integration remain QA/deployment validation, not inferred from these service tests.

# Adversarial review: hyprv-svc-net

Reviewed 2026-10-23. **Review only:** no production crate and no real NetworkManager, BlueZ, keyring, bus or configuration was changed. Evidence is in [scratch/round5](scratch/round5/README.md): [results](scratch/round5/results.json), [log](scratch/round5/hyprv-svc-net.log), [source hashes](scratch/round5/source-hashes.json).

**Reproducing.** `python3 docs/reviews/scratch/round5/verify.py` exits 0 only when all of these hold:
- nine deliberately failing probes reproduce, each with its specific assertion;
- four controls pass;
- the crate's 13 existing tests pass;
- the audited source is unchanged.

Probes run the crate's real agent objects, `Native` transports and actors against fake peers on a private `dbus-daemon`.

**Severity.** P1 means secret-policy violation or credential loss. P2 means correctness, interop, spec or availability defect. P3 means defense in depth.

## Scope and verified-good areas

These held under test or inspection:

- **Caller verification (control).** Both agents compare the message sender with `GetNameOwner(org.freedesktop.NetworkManager | org.bluez)` on every call. A foreign caller on the same bus gets `SecretAgent.Failed` / `org.bluez.Error.Rejected`. That rejection happens before any prompt, keyring lookup or store access.
- **Bluetooth consent (control).** `RequestConfirmation`, `RequestAuthorization` and `AuthorizeService` are always routed to a UI prompt; nothing auto-accepts.
  - Silence never authorizes: the prompt times out after 120 s, which is an error, and the 12 s action timeout cancels pairing.
  - `Reject` returns `Rejected`; only `Accept` authorizes.
  - PIN/passkey replies are range-checked before replying.
- **NetworkManager restart (control).** `NameOwnerChanged` triggers `RegisterWithCapabilities` against the new daemon. The agent object is exported before registration. In `read_only` mode no agent is exported and no action reaches the transport.
- **Secrets stay out of state and logs.** `Reply` and `network::Action` deliberately implement neither Debug nor Serialize. `State` and `Prompt` carry no secret values:
  - `NetworkSecrets` prompts carry only connection/setting/hints;
  - `DisplayPin`/`DisplayPasskey`/`Confirmation` values are meant to be shown;
  - the crate has no logging statements;
  - error strings never interpolate secret values.

  One exception sits outside this crate: see the cross-crate note at the end.

## NET-1 [P2] Secret buffers are never zeroized (spec deviation)

**Location:**
- `src/prompts.rs:57`: `Reply::Fields(BTreeMap<String,String>)`, `Reply::Pin(String)`
- `src/network.rs:121`: `Action::Connect.password: Option<String>`
- `src/network.rs:899` (`resolve_secrets`): `Fields`, and `OwnedValue` strings in the returned `Settings`
- `src/keyring.rs:186`: the JSON serialization of secrets

The crate does not depend on `zeroize`.

**Failure.** `services-api.md:164` says: *"Store secret buffers in `zeroize::Zeroizing`."* Every password, PSK, enterprise password and PIN instead travels through plain `String`/`Vec<u8>`/`OwnedValue` allocations. These are released without wiping, plus intermediate copies: the Connect password is cloned into the settings map, the keyring JSON, and the `OwnedValue`s returned to NetworkManager. A later allocation, a core dump or swapped-out heap page can expose them.

**Verification:** `review_net_secret_buffers_are_zeroized`. A scanning global allocator counts freed blocks that still contain a marker secret. It covers one GetSecrets round-trip (prompt reply → `resolve_secrets` → returned settings) and dropping a `Connect` action: **5 blocks**, expected 0.

**Fix:**
- Wrap secrets in `Zeroizing<String>`/`Zeroizing<Vec<u8>>` (prompt `Reply`, `Action::Connect.password`, keyring `Fields`, JSON buffers).
- Build the D-Bus reply values immediately before sending.
- Avoid intermediate clones; drop and zeroize the reply maps after the call.
- Keep `Reply` free of Debug/Serialize.

## NET-2 [P2] `CancelGetSecrets` is lost while the keyring lookup runs; a stale password prompt appears

**Location:** `src/network.rs:982`–`991` (`cancel_get_secrets` only removes already-published prompts) and `src/network.rs:912` (the `store.load` that precedes the prompt).

**Failure.** NetworkManager calls GetSecrets; the agent first awaits the keyring, which may show an unlock prompt for up to 120 s. If NetworkManager cancels during that window (timeout, connection deactivated, new request), there is no prompt yet to remove, so the cancel is a no-op. When the keyring returns empty:
- the password prompt is published for a request NetworkManager already abandoned;
- the user's answer goes nowhere useful;
- the D-Bus method stays pending until the prompt times out.

Cancellation also never interrupts an in-flight keyring unlock prompt.

**Verification:** `review_net_cancel_before_prompt_cancels_request`. The fake NetworkManager calls GetSecrets, the store blocks, NetworkManager calls `CancelGetSecrets`, then the store returns. Result: `prompt shown=true, GetSecrets still pending=true`.

**Fix:**
- Register each request (connection path + setting) in a cancellation map before any await.
- Run lookup, prompt and save under one cancellable task; `CancelGetSecrets` aborts the task, dismisses any keyring Prompt, and answers the pending call immediately.
- Never publish a prompt for a cancelled key.

## NET-3 [P2] Cancelled requests answer `UserCanceled` instead of `AgentCanceled`

**Location:** `src/network.rs:838` (`AgentError` has no `AgentCanceled`) and the `map_err(AgentError::UserCanceled)` in `resolve_secrets`.

**Failure.** The SecretAgent contract (libnm `NM_SECRET_AGENT_ERROR_AGENT_CANCELED`) distinguishes the two cases:
- an agent-side cancellation in response to `CancelGetSecrets` → `AgentCanceled`;
- the user dismissing the dialog → `UserCanceled`.

NetworkManager treats `UserCanceled` as an explicit user refusal, which fails the activation instead of quietly retiring the request. Prompt timeouts are also reported as user cancellation.

**Verification:** `review_net_cancelled_request_reports_agent_canceled`. GetSecrets fails with `org.freedesktop.NetworkManager.SecretAgent.UserCanceled`; expected `…AgentCanceled`.

**Fix:**
- Add `AgentCanceled` (and `InvalidConnection` for malformed input).
- Map broker cancellation caused by `CancelGetSecrets` or a provider owner change to `AgentCanceled`.
- Reserve `UserCanceled` for an explicit Reject.
- Map timeouts to `NoSecrets`.

## NET-4 [P1] A secret marked NOT_SAVED ("ask every time") is answered silently from the keyring

**Location:** `src/network.rs:910`–`915`. The keyring is consulted whenever `REQUEST_NEW` is absent, regardless of the setting's `*-flags`.

**Failure.** The user (or the profile) chose `psk-flags = NOT_SAVED (2)`, meaning the secret must be asked for on every activation. If the keyring still holds a value for this UUID/setting, the agent returns it without prompting. That happens, for example, after the profile used to be agent-owned: `SaveSecrets` never deletes entries whose ownership changed. This bypasses the user's choice and hands NetworkManager a possibly stale credential. It also reintroduces a secret the user expected not to be stored.

**Verification:** `review_net_not_saved_secret_is_never_served_from_keyring`. The profile has `psk-flags=2` and the store holds a stale `psk`. Result: `prompted=false, answered from keyring=true`.

**Fix:**
- Only load keyring values for keys whose flags are `AGENT_OWNED` and not `NOT_SAVED`/`NOT_REQUIRED`.
- Prompt for NOT_SAVED keys.
- In `SaveSecrets`, delete stored keys that are no longer agent-owned, as plasma-nm does when a secret leaves the wallet's ownership.

## NET-5 [P1] Connecting with a password rewrites an existing profile's secret ownership to NOT_SAVED

**Location:** `src/network.rs:552`–`577` (`fields.insert(flags, 2u32.into())` before `Settings.Connection.Update`).

**Failure.** For any saved profile, supplying a password forces `psk-flags`/`password-flags`/`wep-key-flags` to `NOT_SAVED` and calls `Update`.
- **System-owned profile (flags 0, what the legacy `nmcli` path created):** the profile stops storing the PSK. Autoconnect at boot or before login fails, and every activation needs the agent.
- **Agent-owned profile (flags 1):** the secret leaves the keyring policy.

The README states *"existing profile secret ownership is honored"*; the code does the opposite. Legacy `wifi-action.sh` used `nmcli dev wifi connect … password`, which keeps NetworkManager's ownership semantics.

**Verification:** `review_net_existing_profile_keeps_secret_ownership`. The fake NetworkManager records the `Update` payload; agent-owned and system profiles become `(2, 2)`, expected `(1, 0)`.

**Fix:**
- Preserve the existing `*-flags` value.
- If the profile is system-owned, update the secret in place.
- If agent-owned, store it in the keyring.
- Prefer passing the new secret through the agent at activation (GetSecrets) instead of rewriting the profile.

## NET-6 [P1] An empty password on a saved network overwrites its stored secret with ""

**Location:** `src/network.rs:552` (`if let Some(password) = password` accepts `Some("")`), together with `hyprv-shell/src/services.rs:359`, which always sends `password: Some(password.clone())` from the UI's `String` field.

**Failure.** Clicking a known network without typing a password has these effects:
- the saved profile is updated with `psk = ""` and `psk-flags = 2`, destroying the stored PSK;
- the activation that follows fails or prompts;
- after that the network can only be joined by re-entering the password.

The legacy path treated an empty password as absent: `wifi-action.sh:63` checks `[ -n "$password" ]` and otherwise activates the saved profile unchanged.

**Verification:** `review_net_empty_password_keeps_saved_secret`. A system-owned profile receiving `Some("")` produces `Update sent for an empty password: psk=Some("") psk-flags=2`.

**Fix:**
- In this crate, treat an empty or whitespace password as `None`.
- Never send `Update` without a non-empty new secret.
- Shell side: pass `None` when the field is empty.

## NET-7 [P2] New profiles store the password as NOT_SAVED (parity and availability regression)

**Location:**
- `src/network.rs:826` (`psk-flags = 2`)
- `src/network.rs:759` (`password-flags = 2`)
- the WEP equivalent

**Failure.** A password entered for a new network is used once and then discarded by NetworkManager. Consequences:
- every reconnect, roam or resume prompts again;
- with no agent running (boot, before login, shell restarted) the connection fails with "no secrets".

Both reference behaviors persist the secret:
- legacy `nmcli dev wifi connect … password` stores it in the profile (system-owned);
- plasma-nm, which the spec names as the model (`services.md:13`), defaults to agent-owned with keyring storage.

The README documents NOT_SAVED as intended, but it contradicts both references.

**Verification:** `review_net_new_profile_secret_is_persisted` asserts the generated `psk-flags` is not NOT_SAVED; it is `2`.

**Fix:** default to agent-owned (`1`) and store the secret in the keyring through `SaveSecrets`, matching plasma-nm. Alternatively, offer the legacy system-owned choice. Do not drop the secret silently.

## NET-8 [P2] The keyring uses a private schema, so it cannot see NetworkManager secrets already in the Secret Service

**Location:** `src/keyring.rs:88` and `:173`: attributes `application=hyprv-shell`, `connection-uuid`, `setting`. One JSON blob per setting.

**Failure.** This host's Secret Service is gnome-keyring (`org.freedesktop.secrets`). Agent-owned Wi-Fi and 802.1X secrets already stored there by libnma, nm-applet or nm-connection-editor (all on this machine's install path) use NetworkManager's `org.freedesktop.NetworkManager.Connection` schema: one item per key, attributes `connection-uuid`, `setting-name`, `setting-key`. Because of the mismatch:
- the agent never finds those secrets, so the user is re-prompted;
- the agent stores duplicates under its own schema;
- `DeleteSecrets` leaves the libnma items behind after a profile is deleted;
- other agents cannot read what this agent writes.

The spec asks for plasma-nm semantics. plasma-nm keys its wallet entries by connection UUID and setting name; on Secret Service backends the interoperable form is the NetworkManager schema.

**Verification:** `review_keyring_uses_networkmanager_schema_and_encrypted_session`. A fake Secret Service records `SearchItems {"setting", "connection-uuid", "application": "hyprv-shell"}`.

**Fix:**
- Read and write the NetworkManager schema: `xdg:schema = org.freedesktop.NetworkManager.Connection`, `connection-uuid`, `setting-name`, `setting-key`, one secret per key, label like nm-applet's.
- Optionally import legacy hyprv items once.
- Make `DeleteSecrets` remove all of a connection's NetworkManager-schema items.

## NET-9 [P3] Secret Service session uses the `plain` algorithm

**Location:** `src/keyring.rs:33`.

**Failure.** Secrets cross the session bus unencrypted. libsecret negotiates `dh-ietf1024-sha256-aes128-cbc-pkcs7` when the service supports it, which gnome-keyring and kwallet's secretservicecompat both do. This is defense in depth on a same-user bus: a bus monitor or eavesdropping-capable peer can read them.

**Verification:** same probe; `OpenSession algorithm "plain"`.

**Fix:** implement the DH/AES session (fall back to `plain` only when refused), and zeroize session keys.

## BT-1 [P2] Stopping bluetoothd makes the shell D-Bus-activate it again

**Location:**
- `src/bluetooth.rs:217`: the `Event::OwnerChanged` handler calls `owner_changed()` → `register()` (`RegisterAgent`/`RequestDefaultAgent` on `org.bluez`) on *any* owner change, including disappearance;
- `refresh = true` then sends `GetManagedObjects` to the ownerless name.

The NetworkManager actor has the same pattern (`src/network.rs:255`).

**Failure.** `org.bluez` is activatable on this host (`/usr/share/dbus-1/system-services/org.bluez.service`, `SystemdService=dbus-org.bluez.service`). When the user or a tool stops bluetoothd (`systemctl stop bluetooth`, a power-profile tool, troubleshooting), the very next agent call to `org.bluez` asks the bus to start it again, silently undoing the stop. The same applies wherever NetworkManager is packaged as activatable.

**Verification:** `review_bt_daemon_stop_does_not_reactivate_bluez`. A private bus makes `org.bluez` activatable by `touch marker`; closing the fake BlueZ makes the actor trigger activation: `stopping bluetoothd caused a D-Bus activation request for org.bluez`.

**Fix:**
- On `NameOwnerChanged` with an empty new owner, clear registration and state and send nothing.
- Re-register and refresh only when a new owner appears.
- Send provider calls with `NO_AUTO_START`, or check `NameHasOwner` first, so the shell never activates a daemon.

## Cross-crate note (not counted)

`hyprv-ui-panels/src/model.rs:327` derives `Debug` on the UI `Action` enum that carries `WifiConnect { password: String }`. This undoes this crate's deliberate no-Debug guarantee at the UI boundary: any `{:?}` of an action prints the password. I'm noting it for the ui-panels owner; it is out of this review's write scope.

## Spec/contract deviations summary

| Item | Spec / reference | Observed |
|---|---|---|
| Zeroization | `services-api.md:164` | none (NET-1) |
| Cancellation | SecretAgent `CancelGetSecrets` → `AgentCanceled` | lost before prompt, wrong error (NET-2, NET-3) |
| Secret ownership / storage | plasma-nm semantics (`services.md:13`), legacy nmcli | NOT_SAVED forced and served from keyring (NET-4, 5, 7) |
| Keyring backend | keyring-backed storage, interoperable with NetworkManager secrets | private schema, plain session (NET-8, 9) |
| Provider restart | re-register on restart without side effects | re-activates stopped daemon (BT-1) |

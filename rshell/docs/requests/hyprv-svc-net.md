# Requests to hyprv-svc-net

## From the round-5 adversarial review (2026-10-23)

Full report: [docs/reviews/hyprv-svc-net.md](../reviews/hyprv-svc-net.md). Reproducer: `python3 docs/reviews/scratch/round5/verify.py`. It exits 0 while every listed failure still reproduces, so after fixing, the matching probe should start passing.

| ID | Sev | Summary | Probe |
|---|---|---|---|
| [NET-4](../reviews/hyprv-svc-net.md#net-4-p1-a-secret-marked-not_saved-ask-every-time-is-answered-silently-from-the-keyring) | P1 | A NOT_SAVED ("ask every time") secret is answered from the keyring without prompting; `SaveSecrets` never removes entries that are no longer agent-owned | `review_net_not_saved_secret_is_never_served_from_keyring` |
| [NET-5](../reviews/hyprv-svc-net.md#net-5-p1-connecting-with-a-password-rewrites-an-existing-profiles-secret-ownership-to-not_saved) | P1 | `Connect` with a password rewrites an existing profile's `*-flags` to NOT_SAVED (contradicts the README) | `review_net_existing_profile_keeps_secret_ownership` |
| [NET-6](../reviews/hyprv-svc-net.md#net-6-p1-an-empty-password-on-a-saved-network-overwrites-its-stored-secret-with-) | P1 | `Some("")` (always sent by hyprv-shell `services.rs:359`) overwrites a saved PSK with "" | `review_net_empty_password_keeps_saved_secret` |
| [NET-1](../reviews/hyprv-svc-net.md#net-1-p2-secret-buffers-are-never-zeroized-spec-deviation) | P2 | No `zeroize`; secrets are freed unwiped (`services-api.md:164`) | `review_net_secret_buffers_are_zeroized` |
| [NET-2](../reviews/hyprv-svc-net.md#net-2-p2-cancelgetsecrets-is-lost-while-the-keyring-lookup-runs-a-stale-password-prompt-appears) | P2 | `CancelGetSecrets` during the keyring lookup is lost; a stale prompt appears and the call stays pending | `review_net_cancel_before_prompt_cancels_request` |
| [NET-3](../reviews/hyprv-svc-net.md#net-3-p2-cancelled-requests-answer-usercanceled-instead-of-agentcanceled) | P2 | Cancellation answers `UserCanceled`, not `AgentCanceled` | `review_net_cancelled_request_reports_agent_canceled` |
| [NET-7](../reviews/hyprv-svc-net.md#net-7-p2-new-profiles-store-the-password-as-not_saved-parity-and-availability-regression) | P2 | New profiles are NOT_SAVED (legacy nmcli and plasma-nm persist the secret) | `review_net_new_profile_secret_is_persisted` |
| [NET-8](../reviews/hyprv-svc-net.md#net-8-p2-the-keyring-uses-a-private-schema-so-it-cannot-see-networkmanager-secrets-already-in-the-secret-service) | P2 | Private keyring schema, not NetworkManager's `org.freedesktop.NetworkManager.Connection` schema | `review_keyring_uses_networkmanager_schema_and_encrypted_session` |
| [BT-1](../reviews/hyprv-svc-net.md#bt-1-p2-stopping-bluetoothd-makes-the-shell-d-bus-activate-it-again) | P2 | Owner-loss handling calls the ownerless `org.bluez` and re-activates a stopped bluetoothd (same pattern in the NetworkManager actor) | `review_bt_daemon_stop_does_not_reactivate_bluez` |
| [NET-9](../reviews/hyprv-svc-net.md#net-9-p3-secret-service-session-uses-the-plain-algorithm) | P3 | Secret Service session uses the `plain` algorithm | same probe as NET-8 |

These were verified good (controls pass):
- caller verification on both agents;
- explicit consent for Bluetooth authorization and confirmation (no auto-accept);
- NetworkManager agent re-registration after a restart.

**Cross-crate pointers, out of this crate:**
- `hyprv-shell/src/services.rs:359` should pass `None` for an empty password (NET-6).
- `hyprv-ui-panels/src/model.rs:327` derives `Debug` on an `Action` that carries a plaintext Wi-Fi password.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved import `crate::secret`

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:5:5`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/bluetooth.rs:5:5
  |
5 |     secret::SecretString,
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved imports `crate::dh`, `crate::secret`

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/keyring.rs:11:5`, `crates/hyprv-svc-net/src/keyring.rs:12:5`.

`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.
`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.

```text
error[E0432]: unresolved imports `crate::dh`, `crate::secret`
  --> crates/hyprv-svc-net/src/keyring.rs:11:5
   |
11 |     dh,
   |     ^^ no `dh` in the root
12 |     secret::{Fields, Zeroizing},
   |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved import `crate::secret`

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:3:5`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/prompts.rs:3:5
  |
3 |     secret::{Fields, Secret, Zeroize, Zeroizing},
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:854:73`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:854:73
    |
854 | fn secret_fields(settings: &Settings, setting: &str) -> crate::keyring::Fields {
    |                                                                         ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
854 - fn secret_fields(settings: &Settings, setting: &str) -> crate::keyring::Fields {
854 + fn secret_fields(settings: &Settings, setting: &str) -> secret::Fields {
    |


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:880:30`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:880:30
    |
880 |     fields: &crate::keyring::Fields,
    |                              ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
880 -     fields: &crate::keyring::Fields,
880 +     fields: &secret::Fields,
    |


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:881:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:881:22
    |
881 | ) -> crate::keyring::Fields {
    |                      ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
881 - ) -> crate::keyring::Fields {
881 + ) -> secret::Fields {
    |


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: borrow of moved value: `c`

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:274:72`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0382]: borrow of moved value: `c`
   --> crates/hyprv-svc-net/src/bluetooth.rs:274:72
    |
250 |         let c = match &ctx.system_bus {
    |             - move occurs because `c` has type `zbus::Connection`, which does not implement the `Copy` trait
...
268 |             connection: c,
    |                         - value moved here
...
274 |             present: std::sync::atomic::AtomicBool::new(bus::has_owner(&c, BLUEZ).await),
    |                                                                        ^^ value borrowed here after move
    |
help: consider cloning the value if the performance cost is acceptable
    |
268 |             connection: c.clone(),
    |                          ++++++++


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: non-exhaustive patterns: `bus::Event::OwnerLost` not covered

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:253:49`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0004]: non-exhaustive patterns: `bus::Event::OwnerLost` not covered
   --> crates/hyprv-svc-net/src/network.rs:253:49
    |
253 |             event=transport.next_event()=>match event {
    |                                                 ^^^^^ pattern `bus::Event::OwnerLost` not covered
    |
note: `bus::Event` defined here
   --> crates/hyprv-svc-net/src/bus.rs:19:10
    |
 19 | pub enum Event {
    |          ^^^^^
...
 25 |     OwnerLost,
    |     --------- not covered
    = note: the matched value is of type `bus::Event`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
    |
256 ~                 Event::Disconnected(e)=>{let mut s=state.snapshot().value;s.available=false;s.error=Some(e);state.publish(s);break;},
257 ~                 bus::Event::OwnerLost => todo!(),
    |


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: type mismatch in function arguments

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:933:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0631]: type mismatch in function arguments
   --> crates/hyprv-svc-net/src/network.rs:933:22
    |
839 |     UserCanceled(String),
    |     ------------ found signature defined here
...
933 |             .map_err(AgentError::UserCanceled)?
    |              ------- ^^^^^^^^^^^^^^^^^^^^^^^^ expected due to this
    |              |
    |              required by a bound introduced by this call
    |
    = note: expected function signature `fn(PromptEnd) -> _`
               found function signature `fn(String) -> _`
note: required by a bound in `std::result::Result::<T, E>::map_err`
   --> /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/result.rs:964:12
    |
962 |     pub const fn map_err<F, O>(self, op: O) -> Result<T, F>
    |                  ------- required by a bound in this associated function
963 |     where
964 |         O: [const] FnOnce(E) -> F + [const] Destruct,
    |            ^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Result::<T, E>::map_err`
help: consider wrapping the function in a closure
    |
933 |             .map_err(|arg0: PromptEnd| AgentError::UserCanceled(/* String */))?
    |                      +++++++++++++++++                         ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: this method takes 4 arguments but 3 arguments were supplied

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:945:14`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
   --> crates/hyprv-svc-net/src/network.rs:945:14
    |
945 |             .save(&id, &setting_name, &owned)
    |              ^^^^---------------------------- argument #4 is missing
    |
note: method defined here
   --> crates/hyprv-svc-net/src/keyring.rs:33:14
    |
 33 |     async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
    |              ^^^^                                             ------
help: provide the argument
    |
945 |             .save(&id, &setting_name, &owned, /* fields */)
    |                                             ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:951:19`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> crates/hyprv-svc-net/src/network.rs:951:19
    |
951 |         .map(|(k, v)| (k, value_string(&v)))
    |                   ^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `str`
    = note: all local variables must have a statically known size


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:951:15`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> crates/hyprv-svc-net/src/network.rs:951:15
    |
951 |         .map(|(k, v)| (k, value_string(&v)))
    |               ^^^^^^ doesn't have a size known at compile-time
    |
    = help: within `(_, str)`, the trait `Sized` is not implemented for `str`
    = note: required because it appears within the type `(_, str)`
    = note: all function arguments must have a statically known size


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: this method takes 4 arguments but 3 arguments were supplied

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1011:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
    --> crates/hyprv-svc-net/src/network.rs:1011:22
     |
1011 |                     .save(&id, setting, &fields)
     |                      ^^^^----------------------- argument #4 is missing
     |
note: method defined here
    --> crates/hyprv-svc-net/src/keyring.rs:33:14
     |
  33 |     async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
     |              ^^^^                                             ------
help: provide the argument
     |
1011 |                     .save(&id, setting, &fields, /* fields */)
     |                                                ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: `?` couldn't convert the error to `String`

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:602:35`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T06:33:43+00:00; SHA-256 `40af84c302b9958ff05295ad27290f8f83c3b45ab39be61c65625233e47cb881`; changed during command: False.

```text
error[E0277]: `?` couldn't convert the error to `String`
   --> crates/hyprv-svc-net/src/network.rs:602:35
    |
599 |                           let Reply::Fields(fields) = self
    |  _____________________________________________________-
600 | |                             .broker
601 | |                             .request(Kind::Enterprise { ssid: ssid.clone() })
602 | |                             .await?
    | |                                  -^ the trait `From<PromptEnd>` is not implemented for `String`
    | |__________________________________|
    |                                    this can't be annotated with `?` because it has type `Result<_, PromptEnd>`
    |
note: `PromptEnd` needs to implement `Into<String>`
   --> crates/hyprv-svc-net/src/prompts.rs:103:1
    |
103 | pub enum PromptEnd {
    | ^^^^^^^^^^^^^^^^^^
    = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
    = help: the following other types implement trait `From<T>`:
              `String` implements `From<&String>`
              `String` implements `From<&mut str>`
              `String` implements `From<&str>`
              `String` implements `From<Box<str>>`
              `String` implements `From<Cow<'_, str>>`
              `String` implements `From<Guid<'_>>`
              `String` implements `From<char>`
              `String` implements `From<uuid::Uuid>`
              `String` implements `From<zbus::zvariant::Str<'_>>`


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:37+00:00 — debug: no method named `zeroize` found for mutable reference `&mut String` in the current scope

Reproduce: `cargo build --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:95:36`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut String` in the current scope
  --> crates/hyprv-svc-net/src/prompts.rs:95:36
   |
95 |             Reply::Pin(pin) => pin.zeroize(),
   |                                    ^^^^^^^ method not found in `&mut String`
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
   |
 2 + use zeroize::Zeroize;
   |


```

Full log: `docs/verify/raw/ci/round-01/debug.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved import `crate::secret`

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:5:5`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/bluetooth.rs:5:5
  |
5 |     secret::SecretString,
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved imports `crate::dh`, `crate::secret`

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/keyring.rs:11:5`, `crates/hyprv-svc-net/src/keyring.rs:12:5`.

`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.
`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.

```text
error[E0432]: unresolved imports `crate::dh`, `crate::secret`
  --> crates/hyprv-svc-net/src/keyring.rs:11:5
   |
11 |     dh,
   |     ^^ no `dh` in the root
12 |     secret::{Fields, Zeroizing},
   |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved import `crate::secret`

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:3:5`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/prompts.rs:3:5
  |
3 |     secret::{Fields, Secret, Zeroize, Zeroizing},
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:854:73`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:854:73
    |
854 | fn secret_fields(settings: &Settings, setting: &str) -> crate::keyring::Fields {
    |                                                                         ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
854 - fn secret_fields(settings: &Settings, setting: &str) -> crate::keyring::Fields {
854 + fn secret_fields(settings: &Settings, setting: &str) -> secret::Fields {
    |


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:880:30`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:880:30
    |
880 |     fields: &crate::keyring::Fields,
    |                              ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
880 -     fields: &crate::keyring::Fields,
880 +     fields: &secret::Fields,
    |


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: unresolved item import `Fields` is private

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:881:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
   --> crates/hyprv-svc-net/src/network.rs:881:22
    |
881 | ) -> crate::keyring::Fields {
    |                      ^^^^^^ private unresolved item import
    |
note: the unresolved item import `Fields` is defined here
   --> crates/hyprv-svc-net/src/keyring.rs:12:14
    |
 12 |     secret::{Fields, Zeroizing},
    |              ^^^^^^
help: import `Fields` through the re-export
    |
881 - ) -> crate::keyring::Fields {
881 + ) -> secret::Fields {
    |


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: borrow of moved value: `c`

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:274:72`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0382]: borrow of moved value: `c`
   --> crates/hyprv-svc-net/src/bluetooth.rs:274:72
    |
250 |         let c = match &ctx.system_bus {
    |             - move occurs because `c` has type `zbus::Connection`, which does not implement the `Copy` trait
...
268 |             connection: c,
    |                         - value moved here
...
274 |             present: std::sync::atomic::AtomicBool::new(bus::has_owner(&c, BLUEZ).await),
    |                                                                        ^^ value borrowed here after move
    |
help: consider cloning the value if the performance cost is acceptable
    |
268 |             connection: c.clone(),
    |                          ++++++++


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: non-exhaustive patterns: `bus::Event::OwnerLost` not covered

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:253:49`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0004]: non-exhaustive patterns: `bus::Event::OwnerLost` not covered
   --> crates/hyprv-svc-net/src/network.rs:253:49
    |
253 |             event=transport.next_event()=>match event {
    |                                                 ^^^^^ pattern `bus::Event::OwnerLost` not covered
    |
note: `bus::Event` defined here
   --> crates/hyprv-svc-net/src/bus.rs:19:10
    |
 19 | pub enum Event {
    |          ^^^^^
...
 25 |     OwnerLost,
    |     --------- not covered
    = note: the matched value is of type `bus::Event`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
    |
256 ~                 Event::Disconnected(e)=>{let mut s=state.snapshot().value;s.available=false;s.error=Some(e);state.publish(s);break;},
257 ~                 bus::Event::OwnerLost => todo!(),
    |


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: type mismatch in function arguments

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:933:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0631]: type mismatch in function arguments
   --> crates/hyprv-svc-net/src/network.rs:933:22
    |
839 |     UserCanceled(String),
    |     ------------ found signature defined here
...
933 |             .map_err(AgentError::UserCanceled)?
    |              ------- ^^^^^^^^^^^^^^^^^^^^^^^^ expected due to this
    |              |
    |              required by a bound introduced by this call
    |
    = note: expected function signature `fn(PromptEnd) -> _`
               found function signature `fn(String) -> _`
note: required by a bound in `std::result::Result::<T, E>::map_err`
   --> /home/wilf/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/result.rs:964:12
    |
962 |     pub const fn map_err<F, O>(self, op: O) -> Result<T, F>
    |                  ------- required by a bound in this associated function
963 |     where
964 |         O: [const] FnOnce(E) -> F + [const] Destruct,
    |            ^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Result::<T, E>::map_err`
help: consider wrapping the function in a closure
    |
933 |             .map_err(|arg0: PromptEnd| AgentError::UserCanceled(/* String */))?
    |                      +++++++++++++++++                         ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: this method takes 4 arguments but 3 arguments were supplied

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:945:14`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
   --> crates/hyprv-svc-net/src/network.rs:945:14
    |
945 |             .save(&id, &setting_name, &owned)
    |              ^^^^---------------------------- argument #4 is missing
    |
note: method defined here
   --> crates/hyprv-svc-net/src/keyring.rs:33:14
    |
 33 |     async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
    |              ^^^^                                             ------
help: provide the argument
    |
945 |             .save(&id, &setting_name, &owned, /* fields */)
    |                                             ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:951:19`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> crates/hyprv-svc-net/src/network.rs:951:19
    |
951 |         .map(|(k, v)| (k, value_string(&v)))
    |                   ^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `str`
    = note: all local variables must have a statically known size


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:951:15`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> crates/hyprv-svc-net/src/network.rs:951:15
    |
951 |         .map(|(k, v)| (k, value_string(&v)))
    |               ^^^^^^ doesn't have a size known at compile-time
    |
    = help: within `(_, str)`, the trait `Sized` is not implemented for `str`
    = note: required because it appears within the type `(_, str)`
    = note: all function arguments must have a statically known size


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: this method takes 4 arguments but 3 arguments were supplied

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1011:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
    --> crates/hyprv-svc-net/src/network.rs:1011:22
     |
1011 |                     .save(&id, setting, &fields)
     |                      ^^^^----------------------- argument #4 is missing
     |
note: method defined here
    --> crates/hyprv-svc-net/src/keyring.rs:33:14
     |
  33 |     async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
     |              ^^^^                                             ------
help: provide the argument
     |
1011 |                     .save(&id, setting, &fields, /* fields */)
     |                                                ++++++++++++++


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: `?` couldn't convert the error to `String`

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:602:35`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: `?` couldn't convert the error to `String`
   --> crates/hyprv-svc-net/src/network.rs:602:35
    |
599 |                           let Reply::Fields(fields) = self
    |  _____________________________________________________-
600 | |                             .broker
601 | |                             .request(Kind::Enterprise { ssid: ssid.clone() })
602 | |                             .await?
    | |                                  -^ the trait `From<PromptEnd>` is not implemented for `String`
    | |__________________________________|
    |                                    this can't be annotated with `?` because it has type `Result<_, PromptEnd>`
    |
note: `PromptEnd` needs to implement `Into<String>`
   --> crates/hyprv-svc-net/src/prompts.rs:103:1
    |
103 | pub enum PromptEnd {
    | ^^^^^^^^^^^^^^^^^^
    = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
    = help: the following other types implement trait `From<T>`:
              `String` implements `From<&String>`
              `String` implements `From<&mut str>`
              `String` implements `From<&str>`
              `String` implements `From<Box<str>>`
              `String` implements `From<Cow<'_, str>>`
              `String` implements `From<Guid<'_>>`
              `String` implements `From<char>`
              `String` implements `From<uuid::Uuid>`
              `String` implements `From<zbus::zvariant::Str<'_>>`


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:45+00:00 — release: no method named `zeroize` found for mutable reference `&mut String` in the current scope

Reproduce: `cargo build --workspace --release --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:95:36`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut String` in the current scope
  --> crates/hyprv-svc-net/src/prompts.rs:95:36
   |
95 |             Reply::Pin(pin) => pin.zeroize(),
   |                                    ^^^^^^^ method not found in `&mut String`
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
   |
 2 + use zeroize::Zeroize;
   |


```

Full log: `docs/verify/raw/ci/round-01/release.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved import `crate::secret`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:5:5`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/bluetooth.rs:5:5
  |
5 |     secret::SecretString,
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved imports `crate::dh`, `crate::secret`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/keyring.rs:11:5`, `crates/hyprv-svc-net/src/keyring.rs:12:5`.

`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.
`crates/hyprv-svc-net/src/keyring.rs`: mtime 2026-10-23T08:11:40+00:00; SHA-256 `9a719ad712db0dc8e7dee01dc0f664862c592ff126e760f166b698ddddccac99`; changed during command: False.

```text
error[E0432]: unresolved imports `crate::dh`, `crate::secret`
  --> crates/hyprv-svc-net/src/keyring.rs:11:5
   |
11 |     dh,
   |     ^^ no `dh` in the root
12 |     secret::{Fields, Zeroizing},
   |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved imports `crate::secret`, `crate::secret`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:7:14`, `crates/hyprv-svc-net/src/network.rs:7:5`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.
`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0432]: unresolved imports `crate::secret`, `crate::secret`
 --> crates/hyprv-svc-net/src/network.rs:7:5
  |
7 |     secret::{self, Fields, SProps, SSettings, SValue, Secret, Zeroize, Zeroizing},
  |     ^^^^^^   ^^^^ no `secret` in the root
  |     |
  |     could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved import `crate::secret`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:3:5`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0432]: unresolved import `crate::secret`
 --> crates/hyprv-svc-net/src/prompts.rs:3:5
  |
3 |     secret::{Fields, Secret, Zeroize, Zeroizing},
  |     ^^^^^^ could not find `secret` in the crate root


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: cannot find type `Reply` in this scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:167:43`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0425]: cannot find type `Reply` in this scope
   --> crates/hyprv-svc-net/src/network.rs:167:43
    |
167 |     pub fn respond(&self, id: u64, reply: Reply) -> Result<()> {
    |                                           ^^^^^ not found in this scope
    |
help: consider importing this enum
    |
  2 + use crate::prompts::Reply;
    |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: cannot find type `Reply` in this scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:629:29`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0433]: cannot find type `Reply` in this scope
   --> crates/hyprv-svc-net/src/network.rs:629:29
    |
629 |                         let Reply::Fields(fields) = self
    |                             ^^^^^ use of undeclared type `Reply`
    |
help: consider importing this enum
    |
  2 + use crate::prompts::Reply;
    |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: borrow of moved value: `c`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:274:72`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0382]: borrow of moved value: `c`
   --> crates/hyprv-svc-net/src/bluetooth.rs:274:72
    |
250 |         let c = match &ctx.system_bus {
    |             - move occurs because `c` has type `zbus::Connection`, which does not implement the `Copy` trait
...
268 |             connection: c,
    |                         - value moved here
...
274 |             present: std::sync::atomic::AtomicBool::new(bus::has_owner(&c, BLUEZ).await),
    |                                                                        ^^ value borrowed here after move
    |
help: consider cloning the value if the performance cost is acceptable
    |
268 |             connection: c.clone(),
    |                          ++++++++


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1057:22`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
    --> crates/hyprv-svc-net/src/network.rs:1057:22
     |
1057 |                 for (k, v) in stored {
     |                      ^ doesn't have a size known at compile-time
     |
     = help: the trait `Sized` is not implemented for `str`
     = note: all local variables must have a statically known size


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: the size for values of type `str` cannot be known at compilation time

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1057:31`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: the size for values of type `str` cannot be known at compilation time
    --> crates/hyprv-svc-net/src/network.rs:1057:31
     |
1057 |                 for (k, v) in stored {
     |                               ^^^^^^ doesn't have a size known at compile-time
     |
     = help: the trait `Sized` is not implemented for `str`
     = note: only the last element of a tuple may have a dynamically sized type


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: cannot find type `Reply` in this scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1457:17`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0433]: cannot find type `Reply` in this scope
    --> crates/hyprv-svc-net/src/network.rs:1457:17
     |
1457 |                 Reply::Fields(BTreeMap::from([("psk".into(), "new-value".into())])),
     |                 ^^^^^ use of undeclared type `Reply`
     |
help: consider importing this enum
     |
1359 +     use crate::prompts::Reply;
     |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved item import `Fields` is private

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1363:46`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
    --> crates/hyprv-svc-net/src/network.rs:1363:46
     |
1363 |         values: Mutex<Option<crate::keyring::Fields>>,
     |                                              ^^^^^^ private unresolved item import
     |
note: the unresolved item import `Fields` is defined here
    --> crates/hyprv-svc-net/src/keyring.rs:12:14
     |
  12 |     secret::{Fields, Zeroizing},
     |              ^^^^^^
help: import `Fields` through the re-export
     |
1363 -         values: Mutex<Option<crate::keyring::Fields>>,
1363 +         values: Mutex<Option<secret::Fields>>,
     |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved item import `Fields` is private

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1368:90`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
    --> crates/hyprv-svc-net/src/network.rs:1368:90
     |
1368 |         async fn load(&self, _: &str, _: &str, _: bool) -> Result<Option<crate::keyring::Fields>> {
     |                                                                                          ^^^^^^ private unresolved item import
     |
note: the unresolved item import `Fields` is defined here
    --> crates/hyprv-svc-net/src/keyring.rs:12:14
     |
  12 |     secret::{Fields, Zeroizing},
     |              ^^^^^^
help: import `Fields` through the re-export
     |
1368 -         async fn load(&self, _: &str, _: &str, _: bool) -> Result<Option<crate::keyring::Fields>> {
1368 +         async fn load(&self, _: &str, _: &str, _: bool) -> Result<Option<secret::Fields>> {
     |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: unresolved item import `Fields` is private

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1371:73`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0603]: unresolved item import `Fields` is private
    --> crates/hyprv-svc-net/src/network.rs:1371:73
     |
1371 |         async fn save(&self, _: &str, _: &str, fields: &crate::keyring::Fields) -> Result<()> {
     |                                                                         ^^^^^^ private unresolved item import
     |
note: the unresolved item import `Fields` is defined here
    --> crates/hyprv-svc-net/src/keyring.rs:12:14
     |
  12 |     secret::{Fields, Zeroizing},
     |              ^^^^^^
help: import `Fields` through the re-export
     |
1371 -         async fn save(&self, _: &str, _: &str, fields: &crate::keyring::Fields) -> Result<()> {
1371 +         async fn save(&self, _: &str, _: &str, fields: &secret::Fields) -> Result<()> {
     |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: no method named `zeroize` found for mutable reference `&mut String` in the current scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:135:15`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut String` in the current scope
   --> crates/hyprv-svc-net/src/network.rs:135:15
    |
135 |             p.zeroize();
    |               ^^^^^^^ method not found in `&mut String`
    |
    = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
    |
  2 + use zeroize::Zeroize;
    |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: method `save` has 4 parameters but the declaration in trait `keyring::SecretStore::save` has 5

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1371:23`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0050]: method `save` has 4 parameters but the declaration in trait `keyring::SecretStore::save` has 5
    --> crates/hyprv-svc-net/src/network.rs:1371:23
     |
1371 |         async fn save(&self, _: &str, _: &str, fields: &crate::keyring::Fields) -> Result<()> {
     |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected 5 parameters, found 4
     |
    ::: crates/hyprv-svc-net/src/keyring.rs:33:19
     |
  33 |     async fn save(&self, uuid: &str, id: &str, setting: &str, fields: &Fields) -> Result<()>;
     |                   ----------------------------------------------------------- trait requires 5 parameters


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: not all trait items implemented, missing: `remove`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1367:5`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0046]: not all trait items implemented, missing: `remove`
    --> crates/hyprv-svc-net/src/network.rs:1367:5
     |
1367 |     impl SecretStore for Store {
     |     ^^^^^^^^^^^^^^^^^^^^^^^^^^ missing `remove` in implementation
     |
    ::: crates/hyprv-svc-net/src/keyring.rs:35:5
     |
  35 |     async fn remove(&self, uuid: &str, setting: &str, keys: &[String]) -> Result<()>;
     |     --------------------------------------------------------------------------------- `remove` from trait


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: `?` couldn't convert the error to `String`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:632:35`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: `?` couldn't convert the error to `String`
   --> crates/hyprv-svc-net/src/network.rs:632:35
    |
629 |                           let Reply::Fields(fields) = self
    |  _____________________________________________________-
630 | |                             .broker
631 | |                             .request(Kind::Enterprise { ssid: ssid.clone() })
632 | |                             .await?
    | |                                  -^ the trait `From<PromptEnd>` is not implemented for `String`
    | |__________________________________|
    |                                    this can't be annotated with `?` because it has type `Result<_, PromptEnd>`
    |
note: `PromptEnd` needs to implement `Into<String>`
   --> crates/hyprv-svc-net/src/prompts.rs:103:1
    |
103 | pub enum PromptEnd {
    | ^^^^^^^^^^^^^^^^^^
    = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
    = help: the following other types implement trait `From<T>`:
              `String` implements `From<&String>`
              `String` implements `From<&mut str>`
              `String` implements `From<&str>`
              `String` implements `From<Box<str>>`
              `String` implements `From<Cow<'_, str>>`
              `String` implements `From<Guid<'_>>`
              `String` implements `From<char>`
              `String` implements `From<uuid::Uuid>`
              `String` implements `From<zbus::zvariant::Str<'_>>`


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: no method named `zeroize` found for mutable reference `&mut String` in the current scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:95:36`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut String` in the current scope
  --> crates/hyprv-svc-net/src/prompts.rs:95:36
   |
95 |             Reply::Pin(pin) => pin.zeroize(),
   |                                    ^^^^^^^ method not found in `&mut String`
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
   |
 2 + use zeroize::Zeroize;
   |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: `?` couldn't convert the error to `std::string::String`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/bluetooth.rs:845:31`.

`crates/hyprv-svc-net/src/bluetooth.rs`: mtime 2026-10-23T08:12:45+00:00; SHA-256 `aa3ecd9bb24d5ca2df96191cdba400e18d8cb8092804184fce35fa6a4df2e842`; changed during command: False.

```text
error[E0277]: `?` couldn't convert the error to `std::string::String`
   --> crates/hyprv-svc-net/src/bluetooth.rs:845:31
    |
840 | /                     self.broker
841 | |                         .request(Kind::Confirmation {
842 | |                             device: "device".into(),
843 | |                             passkey: 123,
844 | |                         })
845 | |                         .await?;
    | |                              -^ the trait `From<PromptEnd>` is not implemented for `std::string::String`
    | |______________________________|
    |                                this can't be annotated with `?` because it has type `Result<_, PromptEnd>`
    |
note: `PromptEnd` needs to implement `Into<std::string::String>`
   --> crates/hyprv-svc-net/src/prompts.rs:103:1
    |
103 | pub enum PromptEnd {
    | ^^^^^^^^^^^^^^^^^^
    = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
    = help: the following other types implement trait `From<T>`:
              `std::string::String` implements `From<&mut str>`
              `std::string::String` implements `From<&std::string::String>`
              `std::string::String` implements `From<&str>`
              `std::string::String` implements `From<Box<str>>`
              `std::string::String` implements `From<Cow<'_, str>>`
              `std::string::String` implements `From<Guid<'_>>`
              `std::string::String` implements `From<char>`
              `std::string::String` implements `From<uuid::Uuid>`
              `std::string::String` implements `From<zbus::zvariant::Str<'_>>`


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: no method named `zeroize` found for mutable reference `&mut std::string::String` in the current scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:135:15`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut std::string::String` in the current scope
   --> crates/hyprv-svc-net/src/network.rs:135:15
    |
135 |             p.zeroize();
    |               ^^^^^^^ method not found in `&mut std::string::String`
    |
    = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
    |
  2 + use zeroize::Zeroize;
    |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: `?` couldn't convert the error to `std::string::String`

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:632:35`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0277]: `?` couldn't convert the error to `std::string::String`
   --> crates/hyprv-svc-net/src/network.rs:632:35
    |
629 |                           let Reply::Fields(fields) = self
    |  _____________________________________________________-
630 | |                             .broker
631 | |                             .request(Kind::Enterprise { ssid: ssid.clone() })
632 | |                             .await?
    | |                                  -^ the trait `From<PromptEnd>` is not implemented for `std::string::String`
    | |__________________________________|
    |                                    this can't be annotated with `?` because it has type `Result<_, PromptEnd>`
    |
note: `PromptEnd` needs to implement `Into<std::string::String>`
   --> crates/hyprv-svc-net/src/prompts.rs:103:1
    |
103 | pub enum PromptEnd {
    | ^^^^^^^^^^^^^^^^^^
    = note: the question mark operation (`?`) implicitly performs a conversion on the error value using the `From` trait
    = help: the following other types implement trait `From<T>`:
              `std::string::String` implements `From<&mut str>`
              `std::string::String` implements `From<&std::string::String>`
              `std::string::String` implements `From<&str>`
              `std::string::String` implements `From<Box<str>>`
              `std::string::String` implements `From<Cow<'_, str>>`
              `std::string::String` implements `From<Guid<'_>>`
              `std::string::String` implements `From<char>`
              `std::string::String` implements `From<uuid::Uuid>`
              `std::string::String` implements `From<zbus::zvariant::Str<'_>>`


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: mismatched types

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1401:17`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0308]: mismatched types
    --> crates/hyprv-svc-net/src/network.rs:1401:17
     |
1399 |             resolve_secrets(
     |             --------------- arguments to this function are incorrect
1400 |                 &broker,
1401 |                 store.as_ref(),
     |                 ^^^^^^^^^^^^^^ expected `&AgentSecrets`, found `&Store`
     |
     = note: expected reference `&AgentSecrets`
                found reference `&Store`
note: function defined here
    --> crates/hyprv-svc-net/src/network.rs:1020:10
     |
1020 | async fn resolve_secrets(
     |          ^^^^^^^^^^^^^^^
1021 |     broker: &Broker,
1022 |     secrets: &AgentSecrets,
     |     ----------------------


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: mismatched types

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1415:13`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0308]: mismatched types
    --> crates/hyprv-svc-net/src/network.rs:1415:13
     |
1413 |         let cached = resolve_secrets(
     |                      --------------- arguments to this function are incorrect
1414 |             &broker,
1415 |             store.as_ref(),
     |             ^^^^^^^^^^^^^^ expected `&AgentSecrets`, found `&Store`
     |
     = note: expected reference `&AgentSecrets`
                found reference `&Store`
note: function defined here
    --> crates/hyprv-svc-net/src/network.rs:1020:10
     |
1020 | async fn resolve_secrets(
     |          ^^^^^^^^^^^^^^^
1021 |     broker: &Broker,
1022 |     secrets: &AgentSecrets,
     |     ----------------------


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: mismatched types

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1434:17`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0308]: mismatched types
    --> crates/hyprv-svc-net/src/network.rs:1434:17
     |
1432 |             resolve_secrets(
     |             --------------- arguments to this function are incorrect
1433 |                 &b,
1434 |                 st.as_ref(),
     |                 ^^^^^^^^^^^ expected `&AgentSecrets`, found `&Store`
     |
     = note: expected reference `&AgentSecrets`
                found reference `&Store`
note: function defined here
    --> crates/hyprv-svc-net/src/network.rs:1020:10
     |
1020 | async fn resolve_secrets(
     |          ^^^^^^^^^^^^^^^
1021 |     broker: &Broker,
1022 |     secrets: &AgentSecrets,
     |     ----------------------


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: mismatched types

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/network.rs:1478:17`.

`crates/hyprv-svc-net/src/network.rs`: mtime 2026-10-23T08:14:44+00:00; SHA-256 `10af5c6a63c7e3761d908500f905fef240881712c0c2329720d57e6e21bff66e`; changed during command: True.

```text
error[E0308]: mismatched types
    --> crates/hyprv-svc-net/src/network.rs:1478:17
     |
1476 |             resolve_secrets(
     |             --------------- arguments to this function are incorrect
1477 |                 &cloned,
1478 |                 &Store::default(),
     |                 ^^^^^^^^^^^^^^^^^ expected `&AgentSecrets`, found `&Store`
     |
     = note: expected reference `&AgentSecrets`
                found reference `&Store`
note: function defined here
    --> crates/hyprv-svc-net/src/network.rs:1020:10
     |
1020 | async fn resolve_secrets(
     |          ^^^^^^^^^^^^^^^
1021 |     broker: &Broker,
1022 |     secrets: &AgentSecrets,
     |     ----------------------


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.


### Integration CI — 2026-10-23T08:14:52+00:00 — tests: no method named `zeroize` found for mutable reference `&mut std::string::String` in the current scope

Reproduce: `cargo test --workspace --locked --message-format=json`. Exit 101.

Primary location(s): `crates/hyprv-svc-net/src/prompts.rs:95:36`.

`crates/hyprv-svc-net/src/prompts.rs`: mtime 2026-10-23T08:11:53+00:00; SHA-256 `1e64dbc804a37b2662432994d20d584aa5c4ed9e7d21839d110412d6e35bf392`; changed during command: False.

```text
error[E0599]: no method named `zeroize` found for mutable reference `&mut std::string::String` in the current scope
  --> crates/hyprv-svc-net/src/prompts.rs:95:36
   |
95 |             Reply::Pin(pin) => pin.zeroize(),
   |                                    ^^^^^^^ method not found in `&mut std::string::String`
   |
   = help: items from traits can only be used if the trait is in scope
help: trait `Zeroize` which provides `zeroize` is implemented but not in scope; perhaps you want to import it
   |
 2 + use zeroize::Zeroize;
   |


```

Full log: `docs/verify/raw/ci/round-01/tests.log`. Attribution is to the diagnostic owner and current uncommitted source, not a proven causal commit.

## Assembly build checkpoint (engine, 2026-10-23 04:12)

Latest shared build caught an in-progress net edit: lib.rs does not export
`secret`/`dh`, network.rs cannot resolve Reply, and bluetooth.rs moves connection
`c` before has_owner(&c). This blocks the final shell/workspace build; earlier
full assembly and nested six-scene regression passed. Please finish these edits
in your owned files; the engine is continuing independent UI checks.

## Status: round-5 findings fixed (2026-10-23, svc-desktop agent at the user's request)

All of NET-1 to NET-9 and BT-1 are fixed in `crates/hyprv-svc-net`. Each finding has a regression test ported from the round5 probe; the full mapping is in the crate README.

**Verification:**
- 34 unit tests plus the integration tests pass, stable across 5 reruns.
- `clippy --all-targets -D warnings` is clean.
- The workspace builds, including `hyprv-shell` and `hyprv-ui-panels`; the public `Action`/`Reply` field types are unchanged.

**No real configuration touched:** tests use only fakes on a private dbus-daemon. In test builds the keyring refuses the user session bus. No real keyring item or NetworkManager profile was read or written.

The "Integration CI — 08:14" entries above caught the crate mid-edit (unresolved `crate::secret`/`crate::dh` imports, a moved value). They are resolved; the workspace builds again.

Still open outside this crate: the cross-crate pointers above (`hyprv-shell/src/services.rs:359`, `hyprv-ui-panels/src/model.rs:327`). The NET-6 data-loss path itself is closed in this crate, since empty input is now ignored.

`docs/reviews/scratch/round5/verify.py` now exits 1 with `source_drift`. That is expected: it gates on the pre-fix source hashes. The frozen probes there document the defects; the living regression tests are in the crate.

### Follow-up (2026-10-23): legacy parity for new profiles

At the user's direction, functional parity with the legacy shell wins over plasma-nm for **new** networks. `connection_settings` sets no `*-flags`, so new profiles get NONE (0): the secret is stored in the system profile, available before login and for auto-connect at boot, exactly like `nmcli dev wifi connect … password`.

All other fixes stand:
- existing profiles' flags are never changed;
- empty input never overwrites a secret;
- secrets are zeroized;
- existing AGENT_OWNED secrets use NetworkManager's Secret Service schema;
- NOT_SAVED/NOT_REQUIRED are respected.

The NET-7 regression test is now `net7_new_profile_secret_is_system_owned_like_nmcli`. The README documents the choice.

The two cross-crate pointers are resolved:
- `hyprv-shell/src/services.rs:359` passes `None` for an empty password;
- `hyprv-ui-panels/src/model.rs` uses a redacting `Debug`.

Notes are in those crates' request files, including a workspace-wide leak sweep.


### Integration CI — 2026-10-23T13:15:39+00:00 — tests failed without compiler diagnostic

Command `cargo test --workspace --locked --message-format=json` exited 101. See `docs/verify/raw/ci/round-17/tests.log`. Tail:

```text
temp_home_writes_once_then_never ... ok
test agent_island::install::tests::concurrent_settings_edit_is_preserved ... ok
test util::watch_tests::one_watch_survives_repeated_atomic_replaces ... ok
test util::watch_tests::filters_by_name_and_sees_missing_dirs ... ok

test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s

     Running unittests src/bin/hyprv-agent-hook.rs (target/debug/deps/hyprv_agent_hook-67aa044391a5d292)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/live_readonly.rs (target/debug/deps/live_readonly-851e9f93edccf288)

running 5 tests
test systemd_user_manager_readonly ... ok
test hyprland_socket_readonly_queries ... ok
test agent_island_follows_hook_writes ... ok
test hook_binary_records_observation_only ... ok
test all_actors_start_and_publish ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s

     Running unittests src/lib.rs (target/debug/deps/hyprv_svc_media-5d6dc0109419e23b)

running 36 tests
test audio::model::tests::over_100_percent_is_reported ... ok
test audio::model::tests::balance_is_preserved ... ok
test audio::model::tests::mute_toggle_and_route_target ... ok
test audio::model::tests::partial_props_merge ... ok
test audio::model::tests::set_increase_decrease_limits ... ok
test audio::model::tests::state_is_cubic_and_rounded ... ok
test audio::model::tests::unavailable_without_default_or_props ... ok
test cider::tests::artwork_formatting ... ok
test cider::tests::api_payload_shapes ... ok
test audio::model::tests::write_objects_serialize ... ok
test cider::tests::scoring_rules ... ok
test cider::tests::mapping_prefers_attributes_and_python_or ... ok
test common::tests::backoff_grows_and_caps ... ok
test common::tests::single_line_collapses ... ok
test mpris::model::tests::anchor_extrapolates_only_while_playing_and_clamps ... ok
test mpris::model::tests::cider_merge_rules_and_key_validity ... ok
test mpris::model::tests::flatten_basic_and_cached_paused_content ... ok
test mpris::model::tests::metadata_parsing ... ok
test mpris::model::tests::selection_priority ... ok
test mpris::tests::player_name_filter ... ok
test notifications::tests::waybar_format_matches_swaync_client ... ok
test mpris::tests::property_application ... ok
test spectrum::cava::tests::monstercat_matches_c_semantics ... ok
test cider::tests::varints_and_cache_scan ... ok
test tray::model::tests::item_props_and_icon_string ... ok
test tray::model::tests::keys ... ok
test tray::model::tests::layout_depth_is_bounded ... ok
test tray::model::tests::mnemonics ... ok
test tray::model::tests::layout_parsing ... ok
test cider::tests::http_lookup_against_fake_server ... ok
test tray::model::tests::pixmaps_are_validated ... ok
test spectrum::tests::closing_demand_stops_an_active_capture_through_run_sampled ... ok
test spectrum::tests::dropping_capture_quits_its_loop_and_timer ... ok
test spectrum::cava::tests::overflow_discards_like_cava ... ok
test spectrum::cava::tests::plan_and_mode ... ok
test spectrum::cava::tests::silence_and_tone ... ok

test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s

     Running tests/fake_bus.rs (target/debug/deps/fake_bus-1eb00357bd29a707)

running 5 tests
test mpris_discovery_signals_actions_and_owner_loss ... ok
test tray_serves_watcher_tracks_items_and_menus ... ok
test mpris_read_only_refuses_actions ... ok
test swaync_snapshot_signal_action_and_loss ... ok
test tray_read_only_never_owns_watcher ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s

     Running tests/live_readonly.rs (target/debug/deps/live_readonly-613674410c2e3874)

running 3 tests
test live_audio_observes_default_sink ... ignored, needs the live PipeWire session
test live_mpris_notifications_tray ... ignored, needs the live session bus
test live_spectrum_demand_lifecycle ... ignored, needs live PipeWire; creates a passive capture stream only while running

test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/review_regressions.rs (target/debug/deps/review_regressions-87bba47e30a06dd2)

running 4 tests
test media4_signal_from_non_owner_is_ignored ... ok
test media5_snapshot_from_departed_owner_is_discarded ... ok
test media2_late_position_read_cannot_undo_seeked ... ok
test media3_layout_revision_never_regresses_and_settles_on_latest ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.59s

     Running unittests src/lib.rs (target/debug/deps/hyprv_svc_net-1aa1920f084da112)

running 34 tests
test dh::tests::aes_cbc_matches_nist_sp800_38a ... ok
test bluetooth::tests::qml_display_fields_serialize ... ok
test bluetooth::tests::object_manager_selection_fields_sort_and_removal ... ok
test bluetooth::deadline_tests::pairing_timeout_cancels_provider_and_prompt ... ok
test network::tests::net7_new_profile_secret_is_system_owned_like_nmcli ... ok
test bluetooth::deadline_tests::scan_stops_at_four_seconds_without_idle_timer ... ok
test network::tests::only_agent_owned_secrets_are_stored ... ok
test network::tests::open_psk_sae_wep_and_enterprise_profiles ... ok
test network::tests::qml_fields_are_camel_case_and_secrets_are_not_state ... ok
test network::tests::net4_not_saved_secret_is_never_served_from_keyring ... ok
test network::tests::net1_secret_buffers_are_zeroized ... ok
test network::tests::strongest_ap_and_active_priority_preserve_raw_ssid_identity ... ok
test network::tests::staged_secret_answers_next_request_once ... ok
test prompts::tests::cancelled_future_removes_prompt_and_display_updates_coalesce ... ok
test rfkill::tests::change_delete_and_change_all ... ok
test prompts::tests::response_cancellation_and_redaction ... ok
test network::tests::not_required_secret_is_not_requested ... ok
test dh::tests::both_sides_derive_the_same_key_and_roundtrip ... FAILED
test network::tests::explicit_reject_is_user_canceled_and_timeout_is_no_secrets ... ok
test network::tests::secrets_noninteractive_keyring_and_new_secret_prompt ... ok
test network::tests::save_secrets_syncs_ownership ... ok
test network::tests::net3_cancelled_request_reports_agent_canceled ... ok
test bluetooth::agent_tests::pin_code_reply_roundtrip ... ok
test network::tests::net5_existing_profile_keeps_secret_ownership ... ok
test network::tests::net6_empty_password_keeps_saved_secret ... ok
test keyring::tests::net8_net9_networkmanager_schema_over_encrypted_session ... ok
test bluetooth::agent_tests::control_foreign_caller_is_rejected ... ok
test network::tests::net2_cancel_before_prompt_cancels_request ... ok
test network::tests::control_foreign_caller_is_rejected ... ok
test network::tests::control_reregisters_after_nm_restart ... ok
test bluetooth::agent_tests::control_authorization_requires_explicit_consent ... ok
test bluetooth::agent_tests::startup_without_daemon_does_not_activate ... ok
dbus-daemon[1546177]: [session uid=1000 pid=1546177 pidfd=4] Unable to reload configuration: Failed to open "/tmp/hyprv-net-nm-act-1545882/services-1545882/bus.conf": No such file or directory
test network::tests::nm_stop_does_not_reactivate_networkmanager ... ok
test bluetooth::agent_tests::bt1_daemon_stop_does_not_reactivate_bluez ... ok

failures:

---- dh::tests::both_sides_derive_the_same_key_and_roundtrip stdout ----

thread 'dh::tests::both_sides_derive_the_same_key_and_roundtrip' (1545893) panicked at crates/hyprv-svc-net/src/dh.rs:158:17:
assertion `left != right` failed: ciphertext is not plaintext
  left: [120]
 right: [120]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    dh::tests::both_sides_derive_the_same_key_and_roundtrip

test result: FAILED. 33 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s

error: test failed, to rerun pass `-p hyprv-svc-net --lib`

```

Source fingerprints at failure:
`crates/hyprv-svc-net/src/dh.rs`: {'sha256': '593b7da8b2252bf99bc4a8617ed6641433333f2782852099284bf9830ce42a29', 'mtime': '2026-10-23T08:22:40+00:00'}
`crates/hyprv-svc-net/src/lib.rs`: {'sha256': '07facc6b73822653e55a0035dfa2ae5cf792ed3fe71a0a791f381ad88384314f', 'mtime': '2026-10-23T08:22:30+00:00'}
`crates/hyprv-svc-net/tests/actors.rs`: {'sha256': '138db4031d54b4f1a5bff98d28c4f3d8d38a7a8f4eebd35e06cd681191eb457c', 'mtime': '2026-10-23T08:18:40+00:00'}
`crates/hyprv-svc-net/tests/read_only_bus.rs`: {'sha256': 'c9ffe1d2759d1e7499f873d27b00a4d903301f7320de2a0e5e42859212365463', 'mtime': '2026-10-23T06:29:56+00:00'}


Integration CI root-cause clarification, 13:17 UTC: round 17 failed `dh::tests::both_sides_derive_the_same_key_and_roundtrip` at dh.rs:158 because random AES-CBC ciphertext first byte equalled the one-byte plaintext `x` (`left: [120]`, `right: [120]`). The test truncates ciphertext to plaintext length before `assert_ne!`; a one-byte equality is a valid random outcome (~1/256), not encryption failure. Keep roundtrip/NIST coverage, remove this probabilistic prefix-inequality assertion or compare the complete padded ciphertext (different length for the existing messages). Reproduce/evidence: `docs/verify/raw/ci/round-17/tests.log`. No cryptographic implementation change is justified by this failure.


### Integration CI — 2026-10-23T13:52 UTC — workspace test hangs in foreign-caller control

Round 20 `cargo test --workspace --locked --message-format=json` reached `network::tests::control_foreign_caller_is_rejected`, reported running over 60 seconds, and the net test process remains active after >4 minutes. Other net tests completed. `network_tests.rs:709` calls `get_secrets(&attacker, &agent, profile(1), ALLOW_INTERACTION).await` without an outer test timeout before its rejection assertion; determine whether rejection/DBus response is stranded and bound the test waits. Log: `docs/verify/raw/ci/round-20/tests.log`; test executable PID1834641 belongs to this CI run. Current source mtime2026-10-23T08:24:36UTC, SHA256 `249c48ff4ec408ec085c7ec7910a48978c0df6f855cd0edc69ea2ee63561f5f1`. Observed hang, not proof of the precise blocked await.

Service-idle attribution in progress: temporary IDLE_TRACE_START/END future wrappers installed for a diagnostic release build; core exports a scratch-only poll/CPU/waker logger. These markers will be removed before final performance measurements. Please preserve unrelated changes; no private Cargo target or workspace copy. Normal release ELF saved separately; diagnostics will not count toward the CPU budget.

Service-idle experiment cleanup: all 90 temporary future/blocking wrappers, legacy-mode branches, decoder event tags and core export removed; crate search is empty. Scoped media/sys tests:80passed,5intentionally ignored; strict all-target Clippy passes. Normal release 777b2a29 verified to include tray/wire and no tracing symbols. See docs/verify/service-runtime-idle.md for attribution and budget results; no permanent code changes to core/net/power/desktop in this task.

## Laptop fake-source contract (deploy/user decision, 2026-10-24)

Please ensure all NM/BlueZ/system-bus construction and reconnect paths respect
DBUS_SYSTEM_BUS_ADDRESS, with no host-bus fallback on fixture errors. Any sysfs
reader/probe/watch path must use test-only HYPRV_SYSFS_ROOT, default /sys. User
selected no eGPU on laptop: no eGPU root service/policy/API in laptop install set.
Specs: docs/spec/laptop-deploy.md (test-isolation and user-decisions sections).
Contract only here; no service source changed by deploy owner.

### Laptop Phase B implementation in progress (svc-net,2026-10-24)
Using engine's landed hyprv_core::profile API. Adding refcounted page-only scan_lease; private-bus tests, no host mutations. Bluetooth laptop QML is unchanged; preserve explicit four-second scan and passive signals. Rate sampling is svc-power::sysstats ownership; visibility handoff filed to power/shell. Only this crate and coordination/report docs edited.

### Laptop Phase B svc-net complete —2026-10-24
network.scan_lease.acquire() ready (core Subscription<()> guard);47tests pass in default and forced laptop modes, strictClippy PASS. Private NM/BlueZ virtual-hour tests show no hidden discovery/snapshot reads; pending scan cancellation and desktop JSON identity covered. Only lib.rs/network.rs/new wifi_scan.rs + network/bluetooth tests + README edited. Shared profile used, no sysfs reader, no new Cargo dependency/target, no live actions. Source hashes and logs: docs/verify/laptop-impl-svc-net.md. Shell owns requested-visibility hook (closing animation issue filed), power owns Metric::Network rate leases (20scoped owner tests pass). No further source edits planned.

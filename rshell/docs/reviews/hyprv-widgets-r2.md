# Adversarial re-review: icon theme lookup

**Implementation follow-up:** all findings in this report are fixed locally; see [fixes and regression evidence](../verify/discovery-icons-r2-fixes.md). Original review evidence below describes the pre-fix checkpoint.

Reviewed 2026-10-23, read-only outside review/request documentation. [Probe source and method](scratch/discovery-icons-reconcile/README.md), [results](scratch/discovery-icons-reconcile/icons-results.json), [source fingerprints](scratch/discovery-icons-reconcile/source-hashes.json). The probes compile the actual icon_theme.rs directly, without copying a crate or creating a Cargo target directory.

<a id="widgets-r2-1"></a>
## WIDGETS-R2-1 [P1] A cyclic GTK cache collision chain hangs the UI thread forever

**Location:** `crates/hyprv-widgets/src/icon_theme.rs:156` and `icon_theme.rs:173`; shell caller `crates/hyprv-shell/src/assets.rs:118`.

**Failure scenario:** a version-1 icon-theme.cache passes gtk_cache's timestamp/directory checks but contains a hash node whose next offset points back to itself (or a longer cycle). If its stored icon name differs from the requested name, gtk_cache_lookup repeatedly visits that node without a termination bound. A normal tray icon lookup reaches this code synchronously on the UI thread, so one corrupt user/system theme cache freezes rendering, input and IPC processing.

**Verification:** `probe_icons.py`, `cycle`, builds a complete private theme and a cache accepted by gtk_cache, then calls public lookup_entries. The driver kills that child after a one-second timeout. No real icon cache or compositor is modified.

**Suggested fix:** validate all linked offsets, detect cycles or bound traversal by the maximum number of records fitting in the input, and return None on any malformed chain so lookup scans theme directories. Bound cache file size/read work too. Include self-cycle and two-node-cycle regressions and verify valid chains still resolve.

<a id="widgets-r2-2"></a>
## WIDGETS-R2-2 [P2] Out-of-range nonzero bucket offsets suppress the directory fallback

**Location:** `crates/hyprv-widgets/src/icon_theme.rs:156` and `icon_theme.rs:175`; fallback dispatch at `icon_theme.rs:365`.

**Failure scenario:** a cache's selected bucket, or a node's next pointer, is nonzero but beyond the buffer. The while condition simply fails and returns `Some([])`, indistinguishable from a valid cache saying the icon is absent. find_in_theme then uses an empty directory list rather than scanning index.theme directories. A real installed icon can disappear until the corrupt cache is rebuilt or bypassed. This contradicts the function's documented malformed-cache → None → full-scan behavior.

**Verification:** `probe_icons.py`, `bad-offset`: an 80-byte buffer has a selected bucket offset of 4096. The parser returns `Some([])`. A complete private theme contains `32x32/apps/victim.png` and accepts the cache, yet public lookup_entries returns zero entries; the expected full-scan fallback assertion fails.

**Suggested fix:** distinguish the format's legitimate empty-chain sentinel from invalid nonzero record offsets. Return None for truncated/out-of-range records at every hop. Use checked arithmetic and validate table/count spans before indexing; preserve valid empty-bucket behavior in a separate control test.

## Path handling observation

`lookup_entries("../outside", ..., fallback_roots=[private/pixmaps])` resolves `private/outside.png`. Relative slash components, absolute theme/section names and symlinks are not confined by these joins. However, shell assets deliberately supports absolute IconName paths (`assets.rs:117` and its absolute-path unit test), so the review does **not** equate root escape with cross-user file access or proven exfiltration. If theme-name lookup is intended to accept identifiers only, reject separators/parent/root components there and keep explicit filename loading as a separate supported branch. Regardless of that policy choice, every resolved file needs the resource bounds in SHELL-R2-1.

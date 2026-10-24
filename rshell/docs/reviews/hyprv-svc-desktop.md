# Adversarial review: hyprv-svc-desktop

Reviewed 2026-10-23. Review-only: production crates and real user configuration were not edited. [Scratch evidence](scratch/round3/README.md), [source fingerprints](scratch/round3/source-hashes.json), [results](scratch/round3/results.json), and [hyprv-svc-desktop probe log](scratch/round3/hyprv-svc-desktop.log) identify the tested source and expected failures. Shared source changed during review; affected snapshots were refreshed. A panel-borrow panic fixed concurrently was rechecked and excluded.

P1 denotes a high-priority data-loss, unintended-execution, configuration-availability or idle-loop defect; P2 denotes correctness, ownership, privacy or parity failure. These are deliberately failing regression probes, not failures attributed to the original normal unit suite. No real settings/hooks, desktop theme, service, GPU or compositor was changed.

Coverage also traced hook payload/source normalization, the focus-only agent action boundary, theme argv execution, Deskflow/Viewflow generation/rollback, and eGPU delegation. No shell evaluation of hook JSON/tool input or privileged-removal bypass was established. Theme one-shot user-action execs are permitted by the architecture clarification and are not reported merely for using subprocesses. All file-write/command probes use a rooted fake HOME and scratch victims.

<a id="desktop-1"></a>
## DESKTOP-1 [P1] Hook installation silently overwrites concurrent settings edits

**Location:** `crates/hyprv-svc-desktop/src/agent_island/install.rs:262` and `crates/hyprv-svc-desktop/src/agent_island/install.rs:238`.

**Failure scenario:** edit_json loads a complete JSON document, transforms that old value, and replaces the file without locking or checking that the original is still current. If another CLI/editor changes permissions or hooks after the read, installation writes the old document over the new settings. Atomic rename prevents torn bytes, but does not prevent this lost update. The backup copies the newer file and may temporarily permit recovery; it does not prevent the active-settings rollback, and a later changed install can overwrite that one backup.

**Verification:** review_desktop_stale_install_preserves_concurrent_settings inserts an external permissions update between load and save using edit_json's transform callback. Installation succeeds, but permissions.new disappears and permissions.old returns. The failure is a deterministic interleaving of the real read/modify/write code in a scratch settings.json.

**Suggested fix:** Serialize cooperating installers with a stable lock and detect conflicting external changes before replacement. Stage/rebase the managed-hook edit onto the latest document or abort explicitly on conflict; preserve unrelated settings and recoverable versioned backups. Preflight all target documents before committing changes. A rename alone is insufficient for a read/modify/write transaction.

<a id="desktop-2"></a>
## DESKTOP-2 [P2] Temporary and backup paths follow symlinks and clobber unrelated files

**Location:** `crates/hyprv-svc-desktop/src/util.rs:45` and `crates/hyprv-svc-desktop/src/agent_island/install.rs:246`.

**Failure scenario:** atomic_write uses a predictable .name.PID.counter pathname and std::fs::write, which opens/truncates existing entries and follows symlinks. A pre-existing temporary symlink redirects the write to another file; the later rename even installs that symlink as the final target. Separately, the fixed settings.json.hyprv-bak destination is copied through without rejecting symlinks, so a backup symlink overwrites an unrelated file. These failures require control of those directory entries (or stale/managed symlinks); the review does not establish cross-UID privilege escalation.

**Verification:** review_desktop_atomic_temp_cannot_redirect_writes creates exactly the next temporary symlink using the production counter, calls atomic_write, and observes victim contents replaced. review_desktop_backup_symlink_does_not_overwrite_other_file points the backup at an unrelated scratch file, calls save, and observes that victim replaced with the old settings. Tests run serially so the temporary-counter interleaving is exact.

**Suggested fix:** Create temporary files exclusively with O_CREAT|O_EXCL|O_NOFOLLOW, set restrictive permissions before writing, and retain directory/file FDs through commit. Back up through an exclusively created sibling file and atomically publish it without following an existing backup link. Never clean up an entry the writer did not create. Add file/directory fsync for durability; the tests here establish redirection/clobber, not a power-loss durability measurement.

<a id="desktop-3"></a>
## DESKTOP-3 [P1] A valid commented features table is rewritten into invalid Codex TOML

**Location:** `crates/hyprv-svc-desktop/src/agent_island/install.rs:155` (section recognition) and `crates/hyprv-svc-desktop/src/agent_island/install.rs:287`.

**Failure scenario:** The section scanner only recognizes a line ending literally with ]. Valid TOML such as [features] # personal switches is missed, so the installer appends another [features] table. The resulting config.toml has a duplicate table and cannot be parsed as TOML. Quoted table names and table-looking lines inside multiline values expose the same limitation of line-based editing. This can make the user's agent configuration unusable immediately after a reported successful hook install.

**Verification:** review_desktop_commented_features_stays_one_toml_table feeds that valid commented table into the actual editor and finds two declarations. The runner independently parses the input with Python tomllib successfully, then confirms the generated output fails with Cannot declare features twice at line 4. No real Codex configuration was opened or rewritten.

**Suggested fix:** Use a TOML syntax tree editor that preserves comments/order and handles quoted keys, multiline strings and all table forms. Parse and validate the resulting document before touching any configuration files; on an unsupported/invalid document, preserve it and return a clear error instead of guessing.

<a id="desktop-4"></a>
## DESKTOP-4 [P1] Installed hook commands interpolate executable paths as shell code

**Location:** `crates/hyprv-svc-desktop/src/agent_island/install.rs:50`.

**Failure scenario:** Native binary and Python script paths are inserted verbatim into the command string consumed by the agent's command hook runner. A binary installed under a path containing spaces fails to run correctly; semicolons/redirections in a path become extra shell commands on every hook event. current_exe and the supplied Python path are filesystem paths, not already-escaped shell fragments. This is a path-to-shell trust-boundary defect, not evidence that arbitrary hook JSON is evaluated as code.

**Verification:** review_desktop_hook_path_is_one_shell_argument creates a harmless fake file named hyprv-agent-hook;printf injected>injected-marker # inside scratch, installs it into a rooted fake HOME, and runs only the resulting scratch command using /bin/sh. The appended printf creates injected-marker even though no actual hook executable is launched. The expected no-marker assertion fails. No external message, real agent hook, or real settings file is involved.

**Suggested fix:** Quote each path as one shell argument using a tested shell-quoting routine, including embedded apostrophes, or use a documented structured argv hook API when available. Keep source/event arguments separately validated. Test spaces, quotes, semicolons, dollar substitutions and newlines for both Native and Python commands.

<a id="desktop-5"></a>
## DESKTOP-5 [P2] Marker substring matching removes unrelated user hook commands

**Location:** `crates/hyprv-svc-desktop/src/agent_island/install.rs:66` and `crates/hyprv-svc-desktop/src/agent_island/install.rs:72`.

**Failure scenario:** Any command containing agent-island-hook.py or hyprv-agent-hook anywhere is considered managed. An unrelated logger command whose message merely mentions the filename is deleted; if that was the only nested hook, its matcher entry is deleted too. The implementation/README deliberately use broad marker matching, so this is a data-preservation hazard in the installation contract rather than an unexplained mismatch with its current marker rule.

**Verification:** review_desktop_unrelated_hook_mention_is_preserved supplies a valid custom command logger 'hyprv-agent-hook failed'. remove_managed returns an empty list instead of preserving that entry. This is a pure transformation test; logger is never executed.

**Suggested fix:** Identify managed invocations by their executable/script position and recognized installed path/arguments, or an explicit durable ownership tag where the hook schema supports one. Preserve commands that only mention a marker in data/arguments. Provide a conservative migration path for actual legacy Python invocations.

<a id="desktop-6"></a>
## DESKTOP-6 [P2] Theme read errors are treated as empty input and destroy existing content

**Location:** `crates/hyprv-svc-desktop/src/theme/files.rs:104` .

**Failure scenario:** files::edit treats every read/UTF-8 error like an absent file, then passes an empty string to a narrow INI editor and atomically replaces the existing file without a backup. For example a file with undecodable bytes is silently replaced by a tiny Settings section; unrelated content is lost. An unreadable file with a writable parent can fail similarly. Atomic replacement does not make it safe to discard the failed read.

**Verification:** review_desktop_theme_read_error_preserves_original_bytes writes a three-byte scratch file containing invalid UTF-8, calls the real edit/set_ini_value path, and gets success with replacement content instead of an error plus unchanged bytes. It does not execute the desktop-wide theme applicator.

**Suggested fix:** Create defaults only for a confirmed NotFound error. Propagate all other I/O/encoding errors without touching the original. Preserve existing bytes until a successful parse/edit/validation and conflict check; retain a recoverable backup when replacing user-maintained configuration.

<a id="desktop-7"></a>
## DESKTOP-7 [P2] Theme edits broaden private configuration permissions

**Location:** `crates/hyprv-svc-desktop/src/theme/files.rs:105` and `crates/hyprv-svc-desktop/src/util.rs:46`.

**Failure scenario:** Theme edits pass mode=None, so their replacement files receive the process umask default rather than the original file's permissions. With umask 022, an existing 0600 configuration becomes 0644. On traversable parent directories this exposes previously private contents to other local users. Hook installation has an explicit mode-preservation path, but the theme writer does not.

**Verification:** review_desktop_theme_edit_preserves_private_file_mode creates a valid private 0600 INI, adds a theme value through files::edit and observes 0644. The runner uses umask 022 for reproducibility. The assertion compares actual filesystem mode bits, not inferred defaults.

**Suggested fix:** Preserve the existing owner/permissions (and intentional ACLs) on the held replacement file before writing/publishing; use 0600 for newly created user configuration unless an explicit contract needs broader access. Read metadata and contents from the same validated inode and handle replacement conflicts.


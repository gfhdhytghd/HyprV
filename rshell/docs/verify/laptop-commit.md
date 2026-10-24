# Laptop branch import — 2026-10-24

Worktree: `/home/wilf/.codex/worktrees/svc-sys-laptop-import` (svc-sys private scratch worktree area).
Branch: `svc-sys/laptop-import-20261024`.
Base: `origin/laptop`, pinned `24972f711735ce92e72ef1db5bdf27f85ae8cfc9`.
No push. The orchestrator owns review and push.

| Commit | Scope | Files added |
|---|---|---:|
| `2f40516c0f4ea88725ac8ca0aff51480f6d4494a` | Import reviewed Rust shell sources and canonical reference baseline | 5098 |
| `0f51881866fc7cf7978ff77cbe241eeedcb48993` | Add shared laptop profile selector and laptop QA inputs | 1270 |
| `728e3205ed17aa26a41cd483e83f4de038982ef6` | Add optional CPU-only helper and laptop deployment tooling | 94 |
| `HEAD` (this report's documentation commit) | Reviewed specifications, tasks, requests and verification summaries; import audit | 210 |

Total: **6,672 added files**, all under `rshell/`; no pre-existing laptop file modified.
The report commit is identified symbolically to avoid embedding its own changing hash.
The import stages the reviewed current files by ownership/dependency area; it does not
invent an historical desktop baseline or revert the approved per-service laptop changes.

## Copy boundary

Explicit source allowlist: root workspace manifests/lock/docs; crate sources, native and
vendored code/licenses, assets, tests and fixtures; xtask and source-only experiments;
parity harnesses, fixtures and canonical references; packaging and both desktop/laptop
cutover tooling; optional CPU-only helper and its canonical policy/unit files; docs.
Relative packaging symlinks are preserved and resolve inside the imported rshell.

Excluded: all Cargo targets, dist releases, private build/check/research caches,
review/verification scratch, raw captures, generated render/comparison artifacts,
performance captures, install journals/evidence, logs, Python caches and local tool state.
Historical Markdown reports remain historical: excluded raw links/old receipts do not
establish readiness for this branch. No release, receipt or hardware PASS was fabricated.
Reference `source_dir` fields retain original capture provenance, not deployment paths.

Required flickable CSV/meta/cases and capture sources formerly under review scratch are
promoted to `rshell/fixtures/{flickable-implementation,panels-flickable}`. Five consumers
(two Rust test modules, release fingerprints, parity policy and parity-policy tests) now
use those tracked paths. No regression oracle was removed or weakened. QML parity `out`
inputs consumed by existing tests and binary RGBA fixtures are retained as fixtures.

Source copy was checked before/after hashing, including reference manifests and images;
it stabilized on the first attempt. Pre-relocation input fingerprint:
`c73c8bd5c6d0aacba7df92e8ca5b09129822542494ea873e5b626d8ce2c6c6c7` (approximately 105 MB).

## Checks

- All **1,560** manifest-listed canonical reference images exist across eight manifests.
- All literal Rust include_bytes/include_str local inputs exist; eight package symlinks resolve.
- Cargo metadata succeeds (15 workspace packages); no compilation or live service action.
- `git apply --check rshell/cutover/laptop/hypr.patch` succeeds at the pinned laptop base.
  The patch is **not applied**; installer/cutover retains responsibility for applying it.
- The pre-existing `hypr/` tree and every path outside `rshell/` remain byte-identical to base.
- Isolated cutover suite: **150 run, 147 pass, 3 errors**, each due to intentionally absent
  `packaging/dist/{build.json,release/MANIFEST.sha256,release/release.json}`. The affected
  tests are CandidateAdmission.test_candidate_install_set_rejects_wrong_build_release_id,
  CandidateAdmission.test_new_cpu_policy_files_are_installed_and_manifest_verified, and
  ReleasePaths.test_preflight_checks_deployment_paths. Full release-gate readiness is
  **not claimed**. Tests were kept unchanged except scratch-fixture path relocation.
- Tests used a temporary ignored link to the existing shared Cargo target, then removed it;
  no private target, staged release or copied binary is included.
- Imported files have existing whitespace warnings (`git diff --check`), retained verbatim;
  patch context whitespace is preserved rather than rewriting reviewed artifacts.
- Main HEAD remains `a51ae435ef2adbb97eb4984c75ef552122b165fd`; its index SHA-256 remains
  `14cad57ade58f76162e40e9504535a48d6e699fcd9327e3849c704fe3c6a5975`.
  No source/index/config changes were made there. One test invocation accidentally used
  the source working directory after moving this worktree; it passed 150 tests and may
  have refreshed ignored Python caches. It performed no install or service operations.

Every import commit ends with the requested Claude Opus 5.5 co-author trailer.
No install, Hypr config application, compositor action or push occurred.

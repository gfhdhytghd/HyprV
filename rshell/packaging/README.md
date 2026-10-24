# Prepared packaging

See [cutover/README.md](../cutover/README.md) for exact destinations, deferred manual commands, two-phase cooling handoff, rollback and readiness gates.

`build-release.sh --dry-run`, `install-user.sh --dry-run`, and `install-root.sh` print their plans. `install-user.sh` defaults to dry-run; only explicit `--apply` changes anything. `install-root.sh` never installs: it refuses root and prints the staged procedure (root-owned copy of `dist/release`, `sha256sum` check against the release id, then `/usr/bin/python3 -I` on the staged `install.py`). Both installs are journalled transactions that restore every file on failure or after an interruption, and re-running them is idempotent. Before cutover the root install masks the helper; after cutover it never does. No installation, switch or rollback was executed for real during preparation.

Existing privhelper packaging entries are canonical symlinks maintained by that owner. They are consumed without modification, dereferenced by install. The production release profile is supplied via `--config packaging/release.toml`; the workspace profile is unchanged.

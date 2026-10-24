# Immutable cutover test inputs

Recovered read-only from the user cutover journal backup on 2026-10-24. Every patched file exactly matches the existing `cutover/hypr-inputs.json` before_sha256; no hash or patch was changed. Lua fixture modes are0644 and shell fixtures0755. The unchanged layer-rule fixture preserves namespace coverage. These are test data, never installed or applied to the live tree.

Tests validate the preimage hashes, apply the real patch in a temporary tree, validate every postimage hash, reverse it, and validate every preimage again. Drift/conflict rollback tests use the same immutable inputs. This removes dependence on whether the host has already cut over without bypassing any patch or rollback assertion. Release provenance includes the Lua fixtures as well as shell fixtures.

#!/bin/sh
# Isolated validation: all generated workspace files and targets remain in this crate.
set -eu
crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$crate_dir/.check/net"
cat > "$crate_dir/.check/Cargo.toml" <<'MANIFEST'
[workspace]
resolver = "2"
members = ["net"]
MANIFEST
sed 's|path = "../hyprv-core"|path = "../../../hyprv-core"|' "$crate_dir/Cargo.toml" > "$crate_dir/.check/net/Cargo.toml"
ln -sfn ../../src "$crate_dir/.check/net/src"
ln -sfn ../../tests "$crate_dir/.check/net/tests"
cargo test --offline --manifest-path "$crate_dir/.check/Cargo.toml" -p hyprv-svc-net "$@"
cargo clippy --offline --manifest-path "$crate_dir/.check/Cargo.toml" -p hyprv-svc-net --all-targets -- -D warnings

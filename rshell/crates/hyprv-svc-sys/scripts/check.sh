#!/bin/sh
# Validate within the assigned crate, without rewriting the root workspace lockfile.
set -eu
crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
check_dir="$crate_dir/.check"
mkdir -p "$check_dir/crates/hyprv-core/src" "$check_dir/crates/hyprv-svc-sys/src" "$check_dir/crates/hyprv-svc-sys/tests"
cp "$crate_dir/../hyprv-core/Cargo.toml" "$check_dir/crates/hyprv-core/"
cp "$crate_dir/../hyprv-core/src/"*.rs "$check_dir/crates/hyprv-core/src/"
cp "$crate_dir/Cargo.toml" "$check_dir/crates/hyprv-svc-sys/"
cp "$crate_dir/src/"*.rs "$check_dir/crates/hyprv-svc-sys/src/"
cp "$crate_dir/tests/"*.rs "$check_dir/crates/hyprv-svc-sys/tests/"
cat > "$check_dir/Cargo.toml" <<'MANIFEST'
[workspace]
resolver = "2"
members = ["crates/*"]
[workspace.package]
edition = "2021"
version = "0.1.0"
MANIFEST
command_name=${1:-test}
if [ "$#" -gt 0 ]; then shift; fi
exec cargo "$command_name" --manifest-path "$check_dir/Cargo.toml" -p hyprv-svc-sys "$@"

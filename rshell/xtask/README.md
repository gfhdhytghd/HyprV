# Pixel parity harness

From the repository root, without editing the shared workspace manifest or Cargo configuration:

```sh
PATH="$PWD/xtask:$PATH" cargo xtask parity tests/reference tests/actual --tolerance 2 --report xtask/parity-report
```

Alternatively, run `cargo xtask parity <reference-dir> <actual-dir>` from this directory (its local Cargo alias is provided), or `cargo run --manifest-path xtask/Cargo.toml -- parity ...` from the root. Relative paths resolve against the current directory. The integrator can add a root `.cargo/config.toml` alias `xtask = "run --manifest-path xtask/Cargo.toml --"` for an unconditional root shortcut; that file is outside the core agent's ownership.

This standalone workspace is intentional: root `Cargo.toml` currently includes only `crates/*`, and belongs to the coordinating agent. PNGs are compared recursively by relative name after palette/grayscale expansion and normalization to RGBA8. Sixteen-bit samples use their high byte. Tolerance is inclusive per channel; a pixel fails when any channel's absolute difference exceeds the tolerance. No mismatching pixels are allowed. Missing files, size mismatches and decode errors fail the comparison and appear in both `report.md` and `report.html`. Empty input is an error. Exit statuses: 0 parity passed, 1 comparison failed, 2 usage/input/report error.

This is the directory comparison stub. It does not capture the desktop, launch QML, or prove any component has pixel parity. Reference capture and rendering remain integration work.

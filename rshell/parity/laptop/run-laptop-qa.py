#!/usr/bin/env python3
"""Full laptop QA (docs/verify/laptop-qa-plan.md §6-§8): one command, one consolidated table.

  python3 parity/laptop/run-laptop-qa.py [--skip-e2e] [--skip-desktop-parity]

1. E2E: pins target/release/hyprv-shell to the scratch dir, runs
   e2e/run-laptop-e2e.py --desktop (laptop suite + desktop suite 204 under auto-detect).
2. Laptop parity against parity/laptop/reference (verify-laptop.py, desktop gate):
   - laptop-bar: hyprv-ui-bar examples/reference (spec-bar), full frames, island composed.
   - laptop-panels, three owners, merged:
       lap-cp-*            hyprv-ui-panels tests/laptop_parity.rs, card region
       lap-qa-*            hyprv-ui-panels tests/laptop_quick_adjust.rs, card region per fixture
       lap-battery-popup-* hyprv-ui-bar examples/reference on the laptop-panels snapshot, full frames
   - island: the laptop island is main's (laptop-island.md, D1), so it is gated by the desktop
     island references (verify.py island-gl); its laptop-only state (hidden while compact) is
     covered by lap.island.hidden.compact and the composed island in laptop-bar crops.
3. Desktop regression: verify.py --backend gl (island, bar, panels) vs the recorded baseline
   (island 380/380, bar 300 pass + 12 excluded, panels 300/300), plus the desktop e2e from step 1.

GL renders run in a private headless Weston (verify.PrivateWeston). Writes the consolidated
table into docs/verify/laptop-e2e.md and docs/verify/parity-laptop.md (between markers).
Nothing touches the live session.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SCRATCH = Path(os.environ.get("LAPTOP_QA_SCRATCH", tempfile.gettempdir()))
E2E_REPORT = ROOT / "docs/verify/laptop-e2e.md"
PARITY_REPORT = ROOT / "docs/verify/parity-laptop.md"
DESKTOP_BASELINE = {"island-gl": {"pass": 380}, "bar-gl": {"pass": 300, "excluded": 12}, "panels-gl": {"pass": 300}}
CP_REGION = [579., 273., 282., 354.]
START, END = "<!-- laptop-qa:consolidated -->", "<!-- /laptop-qa:consolidated -->"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


verify = load("parity_verify", ROOT / "parity/verify/verify.py")
CARGO_ENV = {"CARGO_TARGET_DIR": str(ROOT / "target"), "CARGO_INCREMENTAL": "0"}


def run(cmd, env=None, log=None, timeout=None):
    started = time.time()
    proc = subprocess.run(cmd, cwd=ROOT, env={**os.environ, **CARGO_ENV, **(env or {})},
                          capture_output=True, text=True, timeout=timeout)
    if log:
        Path(log).write_text(proc.stdout + "\n--- stderr ---\n" + proc.stderr)
    return proc.returncode, round(time.time() - started)


def qa_region(stem):
    """laptop_quick_adjust.rs region()/mapped_y(), logical px."""
    if stem == "lap-qa-toggle-while-shown":
        return [1150., 49., 262., 40.]
    y = 58. if stem in ("lap-qa-brightness-37", "lap-qa-step") else 25.
    return [1150., y - 2., 262., 66.] if y < 58. else [1150., y - 2., 282., 66.]


def stem_of(file):
    return file.split("--")[0]


def collect_mine(src, dest):
    dest.mkdir(parents=True, exist_ok=True)
    names = []
    for f in Path(src).glob("mine-*.png"):
        shutil.copyfile(f, dest / f.name[5:])
        names.append(f.name[5:])
    return names


def verify_laptop(component, actual, regions=None):
    args = [sys.executable, str(HERE / "verify-laptop.py"), "--component", component, "--actual", str(actual)]
    if regions is not None:
        rfile = actual.parent / f"{actual.name}-regions.json"
        rfile.write_text(json.dumps(regions))
        args += ["--regions", str(rfile)]
    out = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    return out.stdout.strip().splitlines()[0] if out.stdout.strip() else out.stderr.strip()[-300:]


def subset_counts(component, prefix):
    data = json.loads((HERE / "verify-out" / component / "results.json").read_text())
    rows = [r for r in data["rows"] if r["file"].startswith(prefix)]
    c = {k: sum(1 for r in rows if r["result"] == k) for k in ("pass", "fail", "missing")}
    return len(rows), c


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-e2e", action="store_true")
    ap.add_argument("--skip-desktop-parity", action="store_true")
    args = ap.parse_args()
    work = Path(tempfile.mkdtemp(prefix="laptop-qa.", dir=SCRATCH))
    rows, notes = [], []
    try:
        # 1. e2e ------------------------------------------------------------------------
        binary = ROOT / "target/release/hyprv-shell"
        pinned = work / "hyprv-shell"
        shutil.copyfile(binary, pinned)
        pinned.chmod(0o755)
        sha = hashlib.sha256(pinned.read_bytes()).hexdigest()[:16]
        built = time.strftime("%Y-%m-%d %H:%M", time.localtime(binary.stat().st_mtime))
        notes.append(f"shell binary: `target/release/hyprv-shell` built {built}, sha256 `{sha}…` (pinned copy)")
        if not args.skip_e2e:
            code, secs = run([sys.executable, "-I", str(HERE / "e2e/run-laptop-e2e.py"), "--shell", str(pinned),
                              "--desktop", "--timeout", "3600"], log=work / "e2e.log")
            text = E2E_REPORT.read_text()
            cases = [l for l in text.splitlines() if l.startswith("| `lap.") or l.startswith("| `ipc.")]
            status = [l.split("|")[4].strip() for l in cases]
            lap = [s for l, s in zip(cases, status) if "lap.desktop.regression" not in l]
            desk = [s for l, s in zip(cases, status) if "lap.desktop.regression" in l]
            fakes = [l for l in text.splitlines() if l.startswith("| fake") or l.startswith("| org.hyprv") or l.startswith("| hypr.controls")]
            fpass = sum(1 for l in fakes if "| PASS |" in l)
            rows.append(("e2e", "fakes self-test", f"{fpass}/{len(fakes)}", "PASS" if fpass == len(fakes) else "FAIL"))
            n = sum(1 for s in lap if s == "PASS")
            rows.append(("e2e", "laptop nested suite (`lap.*`, `ipc.shell.kill`)", f"{n}/{len(lap)}",
                         "PASS" if n == len(lap) else "FAIL"))
            ddetail = next((l.split("|")[5].strip() for l in cases if "lap.desktop.regression" in l), "")
            rows.append(("desktop regression", "desktop nested e2e (auto-detect)", ddetail.split(";")[0],
                         desk[0] if desk else "NOT RUN"))
        with verify.PrivateWeston() as socket:
            gl = {"HYPRV_PARITY_BACKEND": "gl", "WAYLAND_DISPLAY": socket}
            # 2a. laptop-bar --------------------------------------------------------------
            code, _ = run(["cargo", "build", "-q", "--release", "-p", "hyprv-ui-bar", "--example", "reference"],
                          log=work / "bar-build.log")
            example = ROOT / "target/release/examples/reference"
            assets = ROOT / "crates/hyprv-ui-bar/artifacts/parity/inputs/assets"
            for component, sub, filt in (("laptop-bar", "bar", None), ("laptop-panels", "battery", "lap-battery-popup")):
                out = work / sub
                out.mkdir()
                manifest = json.loads((HERE / "reference" / component / "manifest.json").read_text())
                cmd = [str(example)]
                if filt:
                    cmd.append(",".join(sorted({e["state"] for e in manifest["images"] if e["state"].startswith(filt)})))
                rc, _ = run(cmd, env={**gl, "HYPRV_BAR_REFERENCE_SNAPSHOT": str(HERE / "reference" / component),
                                      "HYPRV_BAR_ACTUAL_DIR": str(out), "HYPRV_BAR_FIXTURE_ASSET_ROOT": str(assets)},
                            log=work / f"{sub}.log", timeout=3600)
                if code:
                    notes.append(f"{sub}: example build failed (see log)")
                verify_laptop(component, out)
            total, c = subset_counts("laptop-bar", "lap-")
            rows.append(("laptop parity", "`laptop-bar` (spec-bar replay, full frames)", f"{c['pass']}/{total}",
                         "PASS" if c["pass"] == total else "FAIL"))
            # 2b. laptop-panels CP + QuickAdjust ------------------------------------------
            for sub, test, var, region in (("cp", "laptop_parity", "HYPRV_PANELS_LAPTOP_PARITY_OUT", lambda s: CP_REGION),
                                           ("qa", "laptop_quick_adjust", "HYPRV_PANELS_LAPTOP_QA_OUT", qa_region)):
                raw = work / f"{sub}-raw"
                run(["cargo", "test", "-q", "-p", "hyprv-ui-panels", "--no-default-features", "--test", test],
                    env={**gl, var: str(raw)}, log=work / f"{sub}.log", timeout=3600)
                names = collect_mine(raw, work / sub)
                verify_laptop("laptop-panels", work / sub, {n: region(stem_of(n)) for n in names})
            for prefix, label in (("lap-cp-", "`laptop-panels` control panel (card region)"),
                                  ("lap-qa-", "`laptop-panels` QuickAdjust (card region)"),
                                  ("lap-battery-popup-", "`laptop-panels` battery popup (spec-bar, full frames)")):
                total, c = subset_counts("laptop-panels", prefix)
                rows.append(("laptop parity", label, f"{c['pass']}/{total}" + (f" ({c['missing']} not rendered)" if c["missing"] else ""),
                             "PASS" if c["pass"] == total else "FAIL"))
        # 2c/3. island + desktop parity -------------------------------------------------
        comps = ["island"] if args.skip_desktop_parity else ["island", "bar", "panels"]
        cmd = [sys.executable, str(ROOT / "parity/verify/verify.py"), "--backend", "gl"]
        for comp in comps:
            cmd += ["--component", comp]
        run(cmd, log=work / "verify.log", timeout=7200)
        results = json.loads((ROOT / "parity/verify/out/results.json").read_text())["results"]
        for slot in [f"{c}-gl" for c in comps]:
            got = {}
            for r in results.get(slot, []):
                got[r["result"]] = got.get(r["result"], 0) + 1
            want = DESKTOP_BASELINE[slot]
            ok = got == want
            text = ", ".join(f"{v} {k}" for k, v in sorted(got.items()))
            if slot == "island-gl":
                rows.append(("laptop parity", "island (= main island, D1; desktop island references, GL)",
                             text, "PASS" if ok else "FAIL"))
            if slot != "island-gl" or not args.skip_desktop_parity:
                rows.append(("desktop regression", f"desktop parity `{slot}` (baseline {', '.join(f'{v} {k}' for k, v in want.items())})",
                             text, "PASS" if ok else "FAIL"))
    finally:
        logs = SCRATCH / "laptop-qa-logs"
        shutil.rmtree(logs, ignore_errors=True)
        logs.mkdir(parents=True)
        for f in work.glob("*.log"):
            shutil.copyfile(f, logs / f.name)
        shutil.rmtree(work, ignore_errors=True)

    # the per-component tables first (verify-laptop.py owns the rest of parity-laptop.md)
    subprocess.run([sys.executable, str(HERE / "verify-laptop.py"), "--report-only"], cwd=ROOT, capture_output=True)
    overall = "PASS" if rows and all(r[3] == "PASS" for r in rows) else "FAIL"
    table = [START, "## Consolidated laptop QA", "",
             f"Generated {time.strftime('%Y-%m-%d %H:%M %Z')} by `parity/laptop/run-laptop-qa.py`. Overall: **{overall}**.", ""]
    table += [f"- {n}" for n in notes] + ["", "| area | check | result | status |", "| --- | --- | --- | --- |"]
    table += [f"| {a} | {c} | {r} | {s} |" for a, c, r, s in rows] + [END]
    for report in (E2E_REPORT, PARITY_REPORT):
        text = report.read_text() if report.exists() else ""
        if START in text:
            text = text[:text.index(START)] + text[text.index(END) + len(END):].lstrip("\n")
        lines = text.splitlines()
        head = lines[:1]  # keep the title first
        report.write_text("\n".join(head + [""] + table + [""] + lines[1:]).rstrip() + "\n")
    print(f"laptop QA: {overall}")
    for r in rows:
        print("  ", " | ".join(r))
    return 0 if overall == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())

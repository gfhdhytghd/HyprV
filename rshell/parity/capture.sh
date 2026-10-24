#!/usr/bin/env bash
# Render reference PNGs from the ORIGINAL QML components, fully isolated from
# the live desktop. See parity/README.md.
#
#   parity/capture.sh                 # every component under fixtures/
#   parity/capture.sh island          # one component
#   parity/capture.sh island idle     # one fixture
#
# Env overrides: PARITY_SCALES="1 2", PARITY_THEMES="light dark",
#                QUICKSHELL_SRC=<dir>, PARITY_TICK_MS=16.6667,
#                PARITY_FIXTURES=<dir> PARITY_OUT=<dir> (debug runs outside the tree)
set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
src="${QUICKSHELL_SRC:-$HOME/.config/HyprV/quickshell}"
build="$here/build"
fixtures_root="${PARITY_FIXTURES:-$here/fixtures}"
out_root="${PARITY_OUT:-$here/reference}"
themes="${PARITY_THEMES:-light dark}"

die() { printf 'capture.sh: %s\n' "$*" >&2; exit 1; }

[[ -f "$src/DynamicIsland.qml" ]] || die "quickshell source not found at $src"

# --- fonts: same families the live shell resolves, or refuse ---------------
for family in "JetBrainsMono Nerd Font" "HarmonyOS Sans SC"; do
    got="$(fc-match -f '%{family}' "$family" 2>/dev/null || true)"
    [[ ",$got," == *",$family,"* ]] || die "font '$family' missing (fc-match gave '$got')"
done

# --- scales: 1 plus every live output scale (read-only hyprctl query) ------
detect_scales() {
    local scales="1" sig out
    if command -v hyprctl >/dev/null; then
        for dir in "${XDG_RUNTIME_DIR:-/run/user/$UID}"/hypr/*/; do
            sig="$(basename "$dir")"
            out="$(HYPRLAND_INSTANCE_SIGNATURE="$sig" timeout 2 hyprctl monitors -j 2>/dev/null || true)"
            [[ "$out" == "["* ]] || continue
            scales+=" $(printf '%s' "$out" | python3 -I -c 'import json,sys; print(" ".join(sorted({format(m["scale"], "g") for m in json.load(sys.stdin)})))')"
            break
        done
    fi
    printf '%s\n' $scales | sort -un | tr '\n' ' '
}
scales="${PARITY_SCALES:-$(detect_scales)}"

# --- build the host ---------------------------------------------------------
cmake -S "$here/host" -B "$build" -DCMAKE_BUILD_TYPE=Release -DCMAKE_EXPORT_COMPILE_COMMANDS=ON >/dev/null
cmake --build "$build" -j"$(nproc)" >/dev/null
python3 -I "$here/tools/make-assets.py" "$here/fixtures/assets"

# --- GL display ---------------------------------------------------------------
# Default: a private headless Weston on its own socket, used only so Qt can
# create an EGL/OpenGL context (the host renders through QQuickRenderControl
# into an offscreen texture; no surface is ever created or mapped).
# PARITY_DISPLAY=live: use the live compositor's display for the GL context
# instead, as experiments/text does; still nothing is mapped or shown.
runtime="$(mktemp -d "${TMPDIR:-/tmp}/hyprv-parity.XXXXXX")"
chmod 700 "$runtime"
weston_pid=""
cleanup() { [[ -n "$weston_pid" ]] && { kill "$weston_pid" 2>/dev/null || true; wait "$weston_pid" 2>/dev/null || true; }; rm -rf "$runtime" "$out_root"/.*.staging.$$; }
trap cleanup EXIT
if [[ "${PARITY_DISPLAY:-private}" == live ]]; then
    socket="${WAYLAND_DISPLAY:?PARITY_DISPLAY=live needs WAYLAND_DISPLAY}"
else
    command -v weston >/dev/null || die "weston is required (private headless GL display)"
    socket="parity-$$"
    weston --backend=headless --renderer=gl --shell=desktop --no-config --idle-time=0 \
        --width=1024 --height=768 --socket="$socket" >"$runtime/weston.log" 2>&1 &
    weston_pid=$!
    for _ in $(seq 100); do [[ -S "${XDG_RUNTIME_DIR}/$socket" ]] && break; sleep 0.05; done
    [[ -S "${XDG_RUNTIME_DIR}/$socket" ]] || { cat "$runtime/weston.log" >&2; die "headless weston did not start"; }
fi

# --- per-theme Qt platform-theme config --------------------------------------
# The live theme switch (apply-ui-state.sh) also flips qt6ct's colour scheme
# (simple.conf / darker.conf) and icon theme (Fluent-light / Fluent-dark), which
# affect theme icons (tray) and SystemPalette (StartupConfirm). Each theme gets
# a private XDG_CONFIG_HOME derived read-only from the user's qt6ct.conf; the
# user's fontconfig (hinting/AA settings) is linked in unchanged.
user_config="${XDG_CONFIG_HOME:-$HOME/.config}"
for theme in light dark; do
    cfg="$runtime/config-$theme"
    mkdir -p "$cfg/qt6ct"
    [[ -e "$user_config/fontconfig" ]] && ln -s "$user_config/fontconfig" "$cfg/fontconfig"
    if [[ $theme == dark ]]; then scheme=darker; icons=Fluent-dark; else scheme=simple; icons=Fluent-light; fi
    if [[ -f "$user_config/qt6ct/qt6ct.conf" ]]; then
        sed -e "s|^color_scheme_path=.*|color_scheme_path=/usr/share/qt6ct/colors/$scheme.conf|" \
            -e "s|^icon_theme=.*|icon_theme=$icons|" "$user_config/qt6ct/qt6ct.conf" > "$cfg/qt6ct/qt6ct.conf"
    fi
done

run_host() { # fixture theme scale outdir
    env -i HOME="$HOME" PATH="$PATH" XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" XDG_CONFIG_HOME="$runtime/config-$2" \
        LANG="${LANG:-C.UTF-8}" ${TZ:+TZ="$TZ"} \
        WAYLAND_DISPLAY="$socket" QT_QPA_PLATFORM=wayland \
        QT_WAYLAND_DISABLE_WINDOWDECORATION=1 QT_QPA_PLATFORMTHEME=qt6ct \
        QSG_RENDER_LOOP=basic QSG_RHI_BACKEND=opengl QT_FORCE_STDERR_LOGGING=1 ${PARITY_DEBUG:+PARITY_DEBUG=1} ${PARITY_DUMP_TEXT:+PARITY_DUMP_TEXT=1} ${PARITY_RAW_PM:+PARITY_RAW_PM=1} \
        "$build/parity-host" --fixture "$1" --scenes "$here/scenes" --source "$src" \
        --assets "$here/fixtures/assets" --out "$4" --theme "$2" --scale "$3" \
        ${PARITY_TICK_MS:+--tick-ms "$PARITY_TICK_MS"}
}

failures=()
components=()
if [[ $# -ge 1 ]]; then components=("$1"); else
    for d in "$fixtures_root"/*/; do [[ "$(basename "$d")" == assets ]] || components+=("$(basename "$d")"); done
fi

for component in "${components[@]}"; do
    fixdir="$fixtures_root/$component"
    [[ -d "$fixdir" ]] || die "no fixtures for component '$component'"
    # Atomic publish: render into a staging dir beside the published set and swap it in
    # with one renameat2(RENAME_EXCHANGE) only when every run succeeded, so readers (CI
    # snapshots, verify.py) never see a half-written reference set.
    published="$out_root/$component"
    out="$out_root/.$component.staging.$$"
    rm -rf "$out"
    failed_before=${#failures[@]}
    if [[ $# -ge 2 ]]; then
        files=("$fixdir/$2.json")
        if [[ -d "$published" ]]; then cp -a "$published" "$out"; else mkdir -p "$out"; fi
    else
        mkdir -p "$out"
        files=("$fixdir"/*.json)
    fi
    for fixture in "${files[@]}"; do
        [[ -f "$fixture" ]] || die "missing fixture $fixture"
        for theme in $themes; do
            for scale in $scales; do
                run_host "$fixture" "$theme" "$scale" "$out" \
                    || failures+=("$(basename "$fixture" .json) $theme @$scale")
            done
        done
    done
    # Merge per-run fragments into one manifest with provenance.
    PARITY_SOURCE_REF="${PARITY_SOURCE_REF:-}" PARITY_PROFILE="${PARITY_PROFILE:-}" \
        python3 -I - "$out" "$src" "$component" "$fixtures_root" "$here" <<'PY'
import hashlib, json, os, sys, datetime
from pathlib import Path
out, src, component = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
fixtures_rel = os.path.relpath(sys.argv[4], sys.argv[5])  # "fixtures" for the desktop set
entries = []
for frag in sorted(out.glob(".*.json")):
    entries.extend(json.loads(frag.read_text()))
sources = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(src.glob("*.qml"))}
for entry in entries:
    entry["fixture"] = f"{fixtures_rel}/{component}/{entry['state']}.json"
manifest = {
    "component": component,
    "generated": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
    "source_dir": str(src),
    "source_sha256": sources,
    "pixel_format": "RGBA8 straight alpha PNG, sRGB, transparent background",
    "renderer": "Qt Quick RHI OpenGL (QQuickRenderControl), virtual 60 Hz clock",
    "tolerance": "compare premultiplied RGBA; exact, except images with distance_field_text: per-channel <= 2",
    "images": sorted(entries, key=lambda e: e["file"]),
}
# Optional provenance (laptop captures): pinned source commit and profile.
for key, env in (("source_ref", "PARITY_SOURCE_REF"), ("profile", "PARITY_PROFILE")):
    if os.environ.get(env):
        manifest[key] = os.environ[env]
(out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1) + "\n")
print(f"{component}: {len(entries)} images, manifest.json written")
PY
    if [[ ${#failures[@]} -gt $failed_before ]]; then
        printf 'capture.sh: %s not published (failed runs); previous reference set kept\n' "$component" >&2
        rm -rf "$out"
    elif [[ -d "$published" ]]; then
        mv -T --exchange "$out" "$published"   # atomic swap; $out now holds the old set
        rm -rf "$out"
    else
        mv -T "$out" "$published"
    fi
done
if [[ ${#failures[@]} -gt 0 ]]; then
    printf 'capture.sh: %d run(s) FAILED:\n' "${#failures[@]}" >&2
    printf '  %s\n' "${failures[@]}" >&2
    exit 1
fi

# Laptop offline oracle inputs

Unmodified files extracted with `git show` from HyprV `origin/laptop` at
`24972f711735ce92e72ef1db5bdf27f85ae8cfc9`. Production does not load or execute
these files. Tests run them only against a temporary HOME/sysfs/runtime tree.

Battery oracle seams: the library's BAT glob reads HYPRV_FAKE_SYS; a private PATH
provides a fixed epoch date; XDG_RUNTIME_DIR selects the temporary history. The
original detail script is unmodified. Brightness oracle seams: a fake hl.timer,
package.preload for the pinned profile, and an injected initial_max. The generated
native transport Lua is separately executed against a fake hl API.

| Fixture | Original path | SHA-256 |
|---|---|---|
| laptop-battery-info.sh | `quickshell/scripts/battery-info.sh` | `95e233acb165b2951ca1e16f5ea5a6a61ed4033e961286bb41c59d874b9a82dc` |
| laptop-battery-lib.sh | `scripts/lib/battery.sh` | `e75a9ea2fc1a763e0fffd97ae221b3a647eab760293c728ef75b2f1c142fda1a` |
| laptop-controls.lua | `hypr/controls.lua` | `eb906f9c29037d07ff1e47a61e339f6d442248aeb1839abf9b5ed29fa597117a` |
| laptop-controls-profile.lua | `hypr/controls-profile.lua` | `65ddc645e3d60a19ec826e0229f9434841851025d78d2bd6288959b11c3315f5` |

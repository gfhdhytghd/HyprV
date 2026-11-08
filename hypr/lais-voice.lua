-- Copilot emits LeftMeta + LeftShift + KEY_F23. Use the physical FK23 code
-- because stock XKB may translate the shifted key to XF86Assistant.
local command = "gdbus call --session --dest org.fcitx.Fcitx5 --object-path /org/lais/Voice --method org.lais.FcitxVoice1."
hl.bind("SUPER + SHIFT + code:201", hl.dsp.exec_cmd(command .. "HoldStart"), {
    description = "LAIS voice: hold Copilot to record",
})
hl.bind("SUPER + SHIFT + ALT + code:201", hl.dsp.exec_cmd(command .. "HoldStartRaw"), {
    description = "LAIS voice: hold Alt+Copilot for raw ASR",
})
hl.bind("code:201", hl.dsp.exec_cmd(command .. "HoldStop"), {
    release = true,
    ignore_mods = true,
    description = "LAIS voice: release Copilot to finish",
})

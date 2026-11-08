-- One brightness level for the XPS panel and the desktop LG UltraFine 6K when it is attached.
-- Model: shown SDR white = panel hardware white (nits) * reference_luminance / base.
-- Below 70 the level picks a hardware white on the LG's 446 nit range; the XPS panel matches
-- it in nits up to its 400 nit backlight and raises its SDR reference for the rest.
-- Above 70 both run at full hardware and share the reference ramp; the XPS keeps the
-- 446/400 compensation so both still show the same nits.
local S = {}
S.lg = {
  desc = "LG Electronics LG ULTRAFINE 601INXJ74394",
  bus = 10,
  mode = "6144x3456@60.02",
  max_nits = 446, -- DDC 100; the desktop profile's peak for the same panel
  sdr_min = 0.2,
}
S.xps_max_nits = 400 -- intel_backlight max_brightness is in nits
S.base, S.peak = 173, 446 -- shared reference white ramp (desktop LG profile)

local function clamp(x, a, b) return math.max(a, math.min(b, x)) end
local function round(x) return math.floor(x + .5) end
-- Same hardware curve as controls.lua: linear 0..10, logarithmic 10..100 over UI 10..70.
local function percent(level)
  level = clamp(level, 0, 70)
  if level <= 10 then return level end
  return 10 * 10 ^ ((level - 10) / 60)
end
local function level_of(p)
  if p <= 10 then return p end
  return 10 + 60 * math.log(p / 10) / math.log(10)
end

function S.plan(level)
  local nits = S.lg.max_nits * percent(level) / 100
  local ref = S.base + (S.peak - S.base) * clamp((level - 70) / 30, 0, 1)
  local xps_nits = math.min(nits, S.xps_max_nits)
  return {
    xps_nits = xps_nits,
    xps_ref = round(ref * math.max(1, nits / S.xps_max_nits)),
    lg_ddc = clamp(round(100 * nits / S.lg.max_nits), 0, 100),
    lg_ref = round(ref),
  }
end

-- Inverse for slider read-back from the XPS backlight (nits) and its applied reference.
function S.level(xps_nits, xps_ref)
  if xps_nits < S.xps_max_nits then
    return round(level_of(100 * xps_nits / S.lg.max_nits))
  end
  local nits = S.xps_max_nits * xps_ref / S.base
  if nits <= S.lg.max_nits then
    return round(level_of(100 * nits / S.lg.max_nits))
  end
  local ref = xps_ref * S.xps_max_nits / S.lg.max_nits
  return round(70 + 30 * clamp((ref - S.base) / (S.peak - S.base), 0, 1))
end

function S.lg_monitor()
  for _, m in ipairs(hl.get_monitors()) do
    if m.description == S.lg.desc then return m end
  end
end

return S

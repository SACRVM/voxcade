-- The Arena: a yard to blow apart. The ready-made rules of the shooter
-- (conf.lua: c.kit) move and fight; this file only shows the keys.

local TEXT = { 240, 230, 204 }

-- The keys, shown as a run begins: the keyboard's or the pad's, as the
-- player plays, in the player's language.
local KEYS = {
  en = { keys = "ARROWS", pad = "STICK", "%s MOVE   {jump} JUMP   {strafe} STRAFE", "{fire} FIRE   {grenade} GRENADE" },
  de = { keys = "PFEILTASTEN", pad = "STICK", "%s LAUFEN   {jump} SPRINGEN   {strafe} SEITWÄRTS", "{fire} FEUER   {grenade} GRANATE" },
  fr = { keys = "FLÈCHES", pad = "STICK", "%s BOUGER   {jump} SAUT   {strafe} DE CÔTÉ", "{fire} TIR   {grenade} GRENADE" },
}
local function show_keys(frame)
  if world.tick >= 600 then return end
  local t = KEYS[world.language] or KEYS.en
  local move = controls.device == "gamepad" and t.pad or t.keys
  for i, line in ipairs(t) do
    line = line:format(move)
    local w, y = frame:text_width(line), frame.height - 46 + 11 * i
    local x = math.floor((frame.width - w) / 2 + 0.5)
    frame:text(line, x, y, TEXT)
    frame:box(x - 3, y - 2, w + 6, 11, { 20, 18, 16 })   -- dark behind it (drawn after: what comes first stays in front)
  end
end

function on_draw(frame)
  show_keys(frame)
end

-- The game's words in the player's language (world.language), and the
-- short notes the HUD shows for a few seconds (level.state.note: a word's
-- key and what fills it in, so a note speaks the language chosen later).
local LANGS = { de = require("lang.de"), en = require("lang.en"), fr = require("lang.fr") }
local text = {}

-- A word by its key in the player's language, its %d and %s filled in.
function text.say(key, ...)
  local words = LANGS[world.language] or LANGS.en
  local s = words[key] or LANGS.en[key] or key
  if select("#", ...) > 0 then
    return string.format(s, ...)
  end
  return s
end

-- A word of a group (crops, seeds, needs, weekdays) by its name or number.
function text.of(group, name)
  local words = LANGS[world.language] or LANGS.en
  return (words[group] or LANGS.en[group])[name]
end

-- A note for the HUD: a key and what fills it in (names of crops are
-- given as { "crops", "wheat" }, to be said in the language of the moment).
function text.note(key, ...)
  level.state.note = { key = key, args = { ... }, at = world.tick }
end

-- The note's text now.
function text.noted(note)
  local args = {}
  for i, a in ipairs(note.args) do
    args[i] = type(a) == "table" and text.of(a[1], a[2]) or a
  end
  return text.say(note.key, table.unpack(args))
end

return text

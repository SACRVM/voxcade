-- The sandbox's Lua side, run once in every Lua state the engine makes
-- (src/lua/sandbox.rs). It returns the safe functions a script sees and
-- the tools the engine uses: keys walked in order (`pairs`, `next`), no
-- memory addresses (`tostring`, `string.format`), metatables and raw
-- writes that cannot break the rules, read-only views, and the freezing of
-- a game's globals after its start (`on_world_start`, the first `on_level_start`). Everything here keeps a game the same on
-- every machine and after every restore.

local raw_next, raw_type, raw_getmetatable, raw_setmetatable = next, type, getmetatable, setmetatable
local raw_rawset, raw_rawget, raw_tostring, raw_error, raw_format = rawset, rawget, tostring, error, string.format
local sort, move, find, sub, pack, unpack = table.sort, table.move, string.find, string.sub, table.pack, table.unpack

-- Read-only views and the tables they show; tables raw writes may not
-- reach. Weak keys only let unused views go, nothing a game can see.
local shows = raw_setmetatable({}, { __mode = "k" })
local views = raw_setmetatable({}, { __mode = "k" })
local guarded = raw_setmetatable({}, { __mode = "k" })
-- A state's keys the engine keeps (`player.state.score`), by the state.
local engine_keys = raw_setmetatable({}, { __mode = "k" })

-- The keys of `t` (and those of `also` that `t` lacks) in order: numbers,
-- then strings, then false and true; numbers and strings each as Lua's own
-- `<` orders them (table.sort without a function compares as `<` does,
-- inside C, which is what makes it quick). The order is the keys' alone,
-- however and whenever they were put in. Any other key is a mistake of
-- the caller of `pairs` or `next` (level 4: this, `keys_of`, `pairs`).
-- `also`: the engine's keys of a state, names.
local function sorted_keys(t, also)
  local keys, strings, n, m, no, yes = {}, {}, 0, 0, false, false
  for k in raw_next, t do
    local kt = raw_type(k)
    if kt == "string" then
      m = m + 1
      strings[m] = k
    elseif kt == "number" then
      n = n + 1
      keys[n] = k
    elseif kt == "boolean" then
      if k then yes = true else no = true end
    else
      raw_error("keys walked in order are numbers, strings or booleans, not a " .. kt, 4)
    end
  end
  if also then
    for k in raw_next, also do
      if raw_rawget(t, k) == nil then
        m = m + 1
        strings[m] = k
      end
    end
  end
  sort(keys)
  sort(strings)
  move(strings, 1, m, n + 1, keys)
  n = n + m
  if no then
    n = n + 1
    keys[n] = false
  end
  if yes then
    n = n + 1
    keys[n] = true
  end
  return keys, n
end

-- The keys of big tables walked, in order, by the table, with each key's
-- place (`at`): kept while the table's keys stay the same, which every
-- walk checks (far quicker than sorting them again). What is kept is only
-- ever the order `sorted_keys` gives, so it changes no walk; tables the
-- game lets go are let go here too. Small tables are sorted afresh.
local walked = raw_setmetatable({}, { __mode = "k" })
local KEPT_FROM = 64

local function keys_of(t, also)
  local kept = also == nil and walked[t]
  if kept then
    local at, count = kept.at, 0
    for k in raw_next, t do
      if at[k] == nil then
        count = -1
        break
      end
      count = count + 1
    end
    -- Every key is one kept, and as many: the same keys.
    if count == kept.n then return kept.keys, kept.n, at end
  end
  local keys, n = sorted_keys(t, also)
  if also == nil and n >= KEPT_FROM then
    local at = {}
    for i = 1, n do at[keys[i]] = i end
    walked[t] = { keys = keys, n = n, at = at }
    return keys, n, at
  end
  if kept then walked[t] = nil end
  return keys, n
end

local readonly

-- `name`: what the table is to the script (`thing.props`) when it is
-- read-only everywhere, not only in on_draw.
readonly = function(t, name)
  if raw_type(t) ~= "table" or shows[t] then return t end
  local view = views[t]
  if view then return view end
  view = raw_setmetatable({}, {
    __index = function(_, k)
      local v = t[k]
      if raw_type(v) == "table" then return readonly(v, name and name .. "." .. raw_tostring(k)) end
      return v
    end,
    __newindex = function(_, k)
      if name then
        raw_error(name .. "." .. raw_tostring(k) .. ": only to read; what changes while playing goes in thing.state", 2)
      end
      raw_error("only to look at here (" .. raw_tostring(k) .. "): change the game in on_tick", 2)
    end,
    __len = function() return #t end,
    __metatable = false,
  })
  shows[view] = t
  views[t] = view
  return view
end

-- A state some of whose keys the engine keeps (`player.state.score`,
-- `thing.state.hp`): those read and written through `get` and `set`, the
-- rest kept in `backing`; `looking()` says whether the call only looks
-- (on_draw), so one view serves every call. Walked in order with its
-- engine keys.
local function engine_state(backing, keys, get, set, looking)
  local state = raw_setmetatable({}, {
    __index = function(_, k)
      if keys[k] then return get(k) end
      local v = backing[k]
      if raw_type(v) == "table" and looking() then return readonly(v) end
      return v
    end,
    __newindex = function(_, k, v)
      if looking() then raw_error("only to look at here (" .. raw_tostring(k) .. "): change the game in on_tick", 2) end
      if keys[k] then set(k, v) else backing[k] = v end
    end,
    __len = function() return #backing end,
    __metatable = false,
  })
  shows[state] = backing
  engine_keys[state] = keys
  return state
end

-- `pairs` and `next` walk the keys in order: numbers, strings, booleans.
local function ordered_pairs(t)
  local source = shows[t] or t
  local keys, n = keys_of(source, engine_keys[t])
  local i = 0
  return function()
    while i < n do
      i = i + 1
      local k = keys[i]
      local v = t[k]
      if v ~= nil then return k, v end
    end
    return nil
  end, t, nil
end

local function ordered_next(t, key)
  local source = shows[t] or t
  local keys, n, at = keys_of(source, engine_keys[t])
  local i = 0
  if key ~= nil then
    if at then
      i = at[key] or 0
    else
      for j = 1, n do
        if keys[j] == key then i = j break end
      end
    end
    if i == 0 then raw_error("next: the key is not in the table", 2) end
  end
  for j = i + 1, n do
    local v = t[keys[j]]
    if v ~= nil then return keys[j], v end
  end
  return nil
end

-- Tables and functions show no addresses.
local function safe_tostring(v)
  local t = raw_type(v)
  if t == "table" then
    local m = raw_getmetatable(v)
    if raw_type(m) == "table" and raw_rawget(m, "__tostring") ~= nil then return raw_tostring(v) end
    return "table"
  elseif t == "function" then
    return "function"
  elseif t == "thread" then
    return "thread"
  end
  return raw_tostring(v)
end

local function safe_format(fmt, ...)
  local i = 1
  while true do
    local s = find(fmt, "%", i, true)
    if not s then break end
    local e = find(fmt, "[%a%%]", s + 1)
    if not e then break end
    if sub(fmt, e, e) == "p" then
      raw_error("string.format: %p shows a memory address, which differs from run to run", 2)
    end
    i = e + 1
  end
  local args = pack(...)
  for k = 1, args.n do
    local t = raw_type(args[k])
    if t == "table" or t == "function" or t == "thread" or t == "userdata" then args[k] = safe_tostring(args[k]) end
  end
  return raw_format(fmt, unpack(args, 1, args.n))
end

local function safe_setmetatable(t, m)
  if raw_type(m) == "table" and (raw_rawget(m, "__gc") ~= nil or raw_rawget(m, "__mode") ~= nil) then
    raw_error("setmetatable: __gc and __mode depend on the garbage collector, which differs from run to run", 2)
  end
  return raw_setmetatable(t, m)
end

local function safe_rawset(t, k, v)
  if guarded[t] or shows[t] then raw_error("rawset: this table is only to look at", 2) end
  return raw_rawset(t, k, v)
end

-- After the start (`on_world_start`, the first `on_level_start`): the globals stay as they are. Reading them works as
-- before (tables as read-only views); writing one is a mistake.
local function freeze(env, base)
  local kept = {}
  for k, v in raw_next, env do kept[k] = v end
  for k in raw_next, kept do raw_rawset(env, k, nil) end
  guarded[env] = true
  raw_setmetatable(env, {
    __index = function(_, k)
      local v = kept[k]
      if v == nil then v = base[k] end
      if raw_type(v) == "table" then return readonly(v) end
      return v
    end,
    __newindex = function(_, k)
      raw_error("'" .. raw_tostring(k) .. "' is a global variable: what changes belongs in a state (world.state, player.state, thing.state)", 2)
    end,
    __metatable = false,
  })
  return kept
end

-- Whether `v` is plain data as a state may hold it, the engine's check
-- after every tick (copying a state takes far longer): nil, or what is
-- wrong and the keys down to it, the innermost first. `seen` holds the
-- tables met so far, `things` whether things may be in it, `is_thing`
-- tells a thing's handle ("thing") and a player's ("player") from other
-- userdata.
local function plain(v, seen, things, is_thing)
  local t = raw_type(v)
  if t == "table" then
    if raw_getmetatable(v) ~= nil then return "view" end
    if seen[v] then return "twice" end
    seen[v] = true
    for k, x in raw_next, v do
      local kt = raw_type(k)
      if kt ~= "string" and kt ~= "number" and kt ~= "boolean" then return "key " .. kt, {} end
      -- Numbers, strings and booleans need no call.
      local xt = raw_type(x)
      if xt ~= "number" and xt ~= "string" and xt ~= "boolean" then
        local why, keys = plain(x, seen, things, is_thing)
        if why then
          keys = keys or {}
          keys[#keys + 1] = k
          return why, keys
        end
      end
    end
  elseif t == "userdata" then
    -- A thing lives in its level; a player (a seat) the whole run.
    local what = is_thing(v)
    if not what then return "type userdata" end
    if what == "thing" and not things then return "thing" end
  elseif t == "function" or t == "thread" then
    return "type " .. t
  end
  return nil
end

-- A script's globals before they freeze: the game model's objects keep
-- their names.
local engine_objects = { world = true, level = true, player = true, input = true, screen = true, controls = true, music = true }
local function set_global(env, k, v)
  if engine_objects[k] then raw_error("'" .. raw_tostring(k) .. "' is the engine's (the game model's object): give yours another name", 2) end
  raw_rawset(env, k, v)
end

string.format = safe_format
local string_meta = raw_getmetatable("")
string_meta.__metatable = false

return {
  pairs = ordered_pairs,
  next = ordered_next,
  tostring = safe_tostring,
  setmetatable = safe_setmetatable,
  rawset = safe_rawset,
  readonly = readonly,
  engine_state = engine_state,
  freeze = freeze,
  set_global = set_global,
  plain = plain,
  guard = function(t) guarded[t] = true end,
}

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
local raw_pcall, match, gsub, number_type = pcall, string.match, string.gsub, math.type

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
  if raw_type(t) ~= "table" then raw_error("bad argument #1 to 'pairs' (table expected, got " .. raw_type(t) .. ")", 2) end
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
  if raw_type(t) ~= "table" then raw_error("bad argument #1 to 'next' (table expected, got " .. raw_type(t) .. ")", 2) end
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

-- What a wrapper's call of Lua's own function gave (`raw_pcall`'s), or its
-- mistake said at the line that called the wrapper (level 3: this, the
-- wrapper, its caller), under the name the script called (`format`, not
-- `raw_format`), never at the prelude's line.
local function said_by(name, ok, ...)
  if ok then return ... end
  local e = ...
  if raw_type(e) == "string" then
    e = match(e, "^prelude:%d+: (.*)$") or e
    e = gsub(e, "^(bad argument #%d+ to )'[^']*'", "%1'" .. name .. "'")
  end
  raw_error(e, 3)
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
  return (said_by("format", raw_pcall(raw_format, fmt, unpack(args, 1, args.n))))
end

local function safe_setmetatable(t, m)
  if raw_type(m) == "table" and (raw_rawget(m, "__gc") ~= nil or raw_rawget(m, "__mode") ~= nil) then
    raw_error("setmetatable: __gc and __mode depend on the garbage collector, which differs from run to run", 2)
  end
  return (said_by("setmetatable", raw_pcall(raw_setmetatable, t, m)))
end

local function safe_rawset(t, k, v)
  if guarded[t] or shows[t] then raw_error("rawset: this table is only to look at", 2) end
  return (said_by("rawset", raw_pcall(raw_rawset, t, k, v)))
end

-- A key as a path shows it: `.name`, or `[1]`.
local function key_text(k)
  if raw_type(k) == "string" and match(k, "^[%a_][%w_]*$") then return "." .. k end
  return "[" .. raw_tostring(k) .. "]"
end

-- A global table after the start, read-only wherever it is read: writing
-- into it is the mistake of changing a global (`path`: SPEEDS, CFG.a).
local global_views = raw_setmetatable({}, { __mode = "k" })
local function frozen(t, path, name)
  if raw_type(t) ~= "table" or shows[t] then return t end
  local view = global_views[t]
  if view then return view end
  view = raw_setmetatable({}, {
    __index = function(_, k)
      local v = t[k]
      if raw_type(v) == "table" then return frozen(v, path .. key_text(k), name) end
      return v
    end,
    __newindex = function(_, k)
      raw_error(path .. key_text(k) .. ": '" .. name .. "' is a global variable, kept as it was at the start: what changes belongs in a state (world.state, player.state, thing.state)", 2)
    end,
    __len = function() return #t end,
    __metatable = false,
  })
  shows[view] = t
  global_views[t] = view
  return view
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
      if v == nil then
        v = base[k]
        if raw_type(v) == "table" then return readonly(v) end
        return v
      end
      if raw_type(v) == "table" then return frozen(v, raw_tostring(k), raw_tostring(k)) end
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

-- `table.sort` as Lua 5.4's own (ltablib.c's quicksort, step for step: the
-- same order of a list, ties and an inconsistent order function too, the
-- same "invalid order function for sorting"), but for one thing: when a
-- big list partitions badly, Lua's draws its next pivots from the clock
-- and the time of day, and ties came out in another order from run to run
-- (Lua 5.4.9's ltablib.c, `l_randomizePivot`); this moves them by a
-- number of its own (`OFF_MIDDLE`). A list C's sort cannot order two ways (numbers
-- that are all whole, all fractions or all strings, no order function, a
-- plain table) is sorted in C, as quick as ever.
local function less(a, b) return a < b end

-- Lua's `l_randomizePivot` as its comment offers it, "~0": the pivot of
-- a badly partitioned big list moved off the middle the same way every
-- run (a list partitioned well keeps its middle pivot, as Lua's).
local OFF_MIDDLE = 0xFFFFFFFF

local function auxsort(t, lo, up, lt, rnd)
  while lo < up do
    local a, b = t[lo], t[up]
    if lt(b, a) then t[lo], t[up] = b, a end
    if up - lo == 1 then return end
    local p
    if up - lo < 100 or rnd == 0 then
      p = (lo + up) // 2
    else
      local r4 = (up - lo) // 4
      p = rnd % (r4 * 2) + (lo + r4)
    end
    local ap, alo = t[p], t[lo]
    if lt(ap, alo) then
      t[p], t[lo] = alo, ap
    else
      local aup = t[up]
      if lt(aup, ap) then t[p], t[up] = aup, ap end
    end
    if up - lo == 2 then return end
    local pivot = t[p]
    t[p], t[up - 1] = t[up - 1], pivot
    -- The partition: a[lo .. i - 1] <= P == a[i] <= a[i + 1 .. up].
    local i, j = lo, up - 1
    while true do
      i = i + 1
      local ai = t[i]
      while lt(ai, pivot) do
        if i == up - 1 then raw_error("invalid order function for sorting") end
        i = i + 1
        ai = t[i]
      end
      j = j - 1
      local aj = t[j]
      while lt(pivot, aj) do
        if j < i then raw_error("invalid order function for sorting") end
        j = j - 1
        aj = t[j]
      end
      if j < i then
        t[up - 1], t[i] = ai, pivot
        break
      end
      t[i], t[j] = aj, ai
    end
    p = i
    local n
    if p - lo < up - p then
      auxsort(t, lo, p - 1, lt, rnd)
      n = p - lo
      lo = p + 1
    else
      auxsort(t, p + 1, up, lt, rnd)
      n = up - p
      up = p - 1
    end
    if (up - lo) // 128 > n then rnd = OFF_MIDDLE end
  end
end

-- Whether C's sort orders the list `t` (1 to n) one way only: no two
-- elements alike for `<` that a game could tell apart (1 and 1.0, 0.0
-- and -0.0), no NaN, no metamethods.
local function one_way(t, n)
  if raw_getmetatable(t) ~= nil then return false end
  local first = number_type(t[1]) or raw_type(t[1])
  if first ~= "integer" and first ~= "float" and first ~= "string" then return false end
  for i = 1, n do
    local v = t[i]
    local kind = number_type(v) or raw_type(v)
    if kind ~= first then return false end
    if kind == "float" and (v ~= v or v == 0) then return false end
  end
  return true
end

local function safe_sort(t, lt)
  if raw_type(t) ~= "table" then
    raw_error("bad argument #1 to 'sort' (table expected, got " .. raw_type(t) .. ")", 2)
  end
  if lt ~= nil and raw_type(lt) ~= "function" then
    raw_error("bad argument #2 to 'sort' (function expected, got " .. raw_type(lt) .. ")", 2)
  end
  local n = #t
  if n < 2 then return end
  if lt == nil and not shows[t] and one_way(t, n) then return sort(t) end
  local ok, e = raw_pcall(auxsort, t, 1, n, lt or less, 0)
  if not ok then
    -- The sort's own mistakes (a comparison of a number and a string, an
    -- invalid order function) said where the game sorted.
    local said = raw_type(e) == "string" and match(e, "^prelude:%d+: (.*)$")
    if said then raw_error(said, 2) end
    raw_error(e, 0)
  end
end

table.sort = safe_sort
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
}

-- The boss at once, without the fifty bars of waves before it.
return {
  about = "The boss comes in two seconds",
  setup = function()
    table.insert(level.state.queue, { beat = 4, kind = "boss", path = "boss", x = 160 })
  end,
}

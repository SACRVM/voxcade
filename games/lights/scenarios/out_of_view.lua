-- A figure walking west before two low lamps out of the picture: its long
-- shadow stays in it.
return {
  about = "A figure walking out of view, its shadow stays",
  setup = function()
    world.state.subject = "out_of_view"
  end,
}

-- A wall a voxel thin whose voxels touch only at their edges: no light
-- leaks behind it.
return {
  about = "A lamp before a diagonal wall a voxel thin",
  setup = function()
    world.state.subject = "diagonal_wall"
  end,
}

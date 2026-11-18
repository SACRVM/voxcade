-- Fog with beams (density, glow, anisotropy, a light's fog): three beams
-- across the fog's stage, one cut by a post, one by a wall a voxel thin,
-- one free.
return {
  about = "Fog with beams: cut by a post and a thin wall",
  setup = function()
    world.state.subject = "fog"
  end,
}

//! `Dirt Wall Backgrounds`: the dirt walls that fill in behind the top of the ground.
//!
//! Transcribed from the `Dirt Wall Backgrounds` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:11895-11933`).
//!
//! One column at a time, the pass random-walks a depth `num` in 0..=10, then walks down from the
//! sky to `worldSurface + num`, and starts writing a wall only once it has seen six solid tiles in
//! a 2x3 block. That latch is what keeps the walls out of the underside of overhangs: the pass
//! paints from the first *solid* row down, not from the surface.
//!
//! The wall is whatever the last active tile in this column said it should be, so a column of snow
//! (147) ends up with snow walls (40) and everything else gets dirt walls (2). A tile already
//! holding wall 64 is left alone.
//!
//! Worth noting for the parity pipeline: this pass draws exactly one number per column and never
//! branches on what those columns contain, so its `RandNext` is a pure function of `maxTilesX`. It
//! is therefore the rare pass that can be verified before the terrain underneath it is right.

use super::passes::GenVars;

/// `Main.wallDirt`.
const DIRT_WALL: u16 = 2;
/// `Main.wallSnow`.
const SNOW_WALL: u16 = 40;
/// The one wall this pass refuses to overwrite.
const PRESERVED_WALL: u16 = 64;
/// `SnowBlock`, the only tile type that changes which wall is picked.
const SNOW_BLOCK: u16 = 147;

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    // `Main.worldSurface`, which is the highest surface row plus 25 - the pass paints down to
    // `worldSurface + 10`, so using `GenVars.worldSurface` here stops the walls ~64 rows early.
    let surface = f64::from(vars.world.surface);
    let rand = &mut vars.rand;
    let world = &mut vars.world;

    let mut draws = 0usize;
    // `num` is a running walk depth, not a per-column roll: vanilla declares it outside the column
    // loop and lets it carry over.
    let mut depth = 0i32;
    for column in 1..width - 1 {
        // `num += genRand.Next(-1, 2)`, then clamped to 0..=10 by the two `if`s vanilla writes.
        depth = (depth + rand.next_range(-1, 2)).clamp(0, 10);
        draws += 1;

        let mut wall = DIRT_WALL;
        let mut inside = false;
        let mut row = 0i32;
        // Two bounds, both from the original: strictly under `worldSurface + 10`, and not above
        // `worldSurface + num`. `num` is at most 10, so it is the one that usually bites, but it
        // can also be the smaller of the two by a fraction of a tile.
        while f64::from(row) < surface + 10.0 && f64::from(row) <= surface + f64::from(depth) {
            let here = world.tile(column, row);
            if here.is_active() {
                wall = if here.block != SNOW_BLOCK {
                    DIRT_WALL
                } else {
                    SNOW_WALL
                };
            }
            if inside && here.wall != PRESERVED_WALL {
                let mut edited = here;
                edited.wall = wall;
                world.set_tile(column, row, edited);
            }
            if here.is_active()
                && world.tile(column - 1, row).is_active()
                && world.tile(column + 1, row).is_active()
                && world.tile(column, row + 1).is_active()
                && world.tile(column - 1, row + 1).is_active()
                && world.tile(column + 1, row + 1).is_active()
            {
                inside = true;
            }
            row += 1;
        }
    }

    draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

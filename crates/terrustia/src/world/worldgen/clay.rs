//! `Clay`: the clay pockets under the surface, and the clay the surface itself is not allowed to keep.
//!
//! Transcribed from the `Clay` delegate in `WorldGen.AddPasses` (`WorldGen.cs:11997-12040`).
//!
//! Three `TileRunner` loops lay clay (type 40) at three depths, all with `addTile` false so they
//! convert ground that is already there. The remix branch that swaps them for a deeper, larger
//! set is not modelled; that flag is false for an ordinary seed.
//!
//! The fourth part of the pass draws nothing and is easy to miss: it walks every column from five
//! in to five from the edge, finds the first active tile, and rewrites that tile and the four
//! below it from clay back to dirt. So clay may be placed above the surface line but never *at*
//! it, which is why a vanilla surface never shows a clay crust.
//!
//! Like the other layer passes its draw count depends on the tiles it reads, through
//! `TileRunner`'s override test, so it is not verifiable on the RNG oracle alone.

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};

/// `TileID.Clay`.
const CLAY: u16 = 40;
/// `TileID.Dirt`, what the surface cleanup rewrites clay to.
const DIRT: u16 = 0;

/// A generator that also counts its draws.
struct Draws<'a> {
    rand: &'a mut UnifiedRandom,
    count: usize,
}

impl<'a> Draws<'a> {
    fn new(rand: &'a mut UnifiedRandom) -> Self {
        Self { rand, count: 0 }
    }

    fn next_range(&mut self, min: i32, max: i32) -> i32 {
        self.count += 1;
        self.rand.next_range(min, max)
    }
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    let ctx = RunnerContext {
        world_surface: vars.world_surface,
        water_line: vars.water_line,
    };
    let surface_low = vars.world_surface_low as i32;
    let surface_high = vars.world_surface_high as i32;
    let rock_high = vars.rock_layer_high as i32;
    // The crust walk stops one row above the surface line, as the original's loop bound says.
    let crust_limit = vars.world_surface - 1.0;
    let area = f64::from(width) * f64::from(height);

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    // Clay anywhere above the low surface line.
    let attempts = (area * 2e-5) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        // No `+ 1` on this bound, unlike the other two bands: vanilla's first loop is
        // `Next(0, (int)worldSurfaceLow)`.
        let y = rng.next_range(0, surface_low);
        let strength = f64::from(rng.next_range(4, 14));
        let steps = rng.next_range(10, 50);
        draws += runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    // The surface band.
    let attempts = (area * 5e-5) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let y = rng.next_range(surface_low, surface_high + 1);
        let strength = f64::from(rng.next_range(8, 14));
        let steps = rng.next_range(15, 45);
        draws += runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    // And the band down to the rock layer.
    let attempts = (area * 2e-5) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let y = rng.next_range(surface_high, rock_high + 1);
        let strength = f64::from(rng.next_range(8, 15));
        let steps = rng.next_range(5, 50);
        draws += runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    // The crust cleanup. Five tiles off each edge, first active tile of the column, and that tile
    // plus the four below it stop being clay. `break` is on the outside, so only the first solid
    // row of each column is reached at all.
    for column in 5..width - 5 {
        let mut row = 1i32;
        while f64::from(row) < crust_limit {
            if world.tile(column, row).is_active() {
                for clay_row in row..row + 5 {
                    let mut tile = world.tile(column, clay_row);
                    if tile.block == CLAY {
                        tile.block = DIRT;
                        world.set_tile(column, clay_row, tile);
                    }
                }
                break;
            }
            row += 1;
        }
    }

    rng.count + draws
}

/// `TileRunner(x, y, strength, steps, 40)` with the game's own defaults for the rest.
fn runner(
    world: &mut crate::world::World,
    rand: &mut UnifiedRandom,
    ctx: RunnerContext,
    x: i32,
    y: i32,
    strength: f64,
    steps: i32,
) -> usize {
    tile_runner(
        world, rand, ctx, x, y, strength, steps, CLAY, false, 2.0, 2.0, false, true, -1,
    )
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

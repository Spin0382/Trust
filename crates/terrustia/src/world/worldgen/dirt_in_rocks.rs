//! `Dirt In Rocks`: the dirt pockets inside the stone layer.
//!
//! Transcribed from the `Dirt In Rocks` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:11964-11996`).
//!
//! One loop, and it is the busiest of the three layer-mixing passes: `maxTilesX * maxTilesY *
//! 0.005` runners, 25,200 on the reference world's 4200x1200, each a small blob (strength 2 to 6,
//! 2 to 40 steps) of dirt placed anywhere from the rock layer down to the bottom of the world.
//! `addTile` is false, so this converts stone that is already there rather than building anything,
//! exactly as `Rocks In Dirt` does in the other direction one pass earlier.
//!
//! The `remixWorldGen` branch that follows the loop swaps every dirt and stone tile in the world
//! below the surface. It is not modelled: the flag is false for an ordinary seed.

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};

/// `TileRunner`'s `type` for dirt.
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
    let ctx = RunnerContext::ordinary(vars);
    // Truncations match vanilla's `(int)` cast on the layer double.
    let rock_low = vars.rock_layer_low as i32;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let attempts = (f64::from(width) * f64::from(height) * 0.005) as i32;
    let mut draws = 0usize;

    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let y = rng.next_range(rock_low, height);
        let strength = f64::from(rng.next_range(2, 6));
        let steps = rng.next_range(2, 40);
        // `TileRunner(x, y, strength, steps, 0)`, the rest at the game's own defaults, which are
        // `addTile: false` and both speeds 0.
        draws += tile_runner(
            world, rng.rand, ctx, x, y, strength, steps, DIRT, false, 0.0, 0.0, false, true, -1,
        );
    }

    rng.count + draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

//! `Small Holes`: the scattered pockets in the dirt and rock layers.
//!
//! Transcribed from the `Small Holes` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12046-12104`).
//!
//! Each of the `maxTilesX * maxTilesY * 0.0015` attempts (7,560 on the reference world) rolls one
//! liquid flag and then digs two separate holes with it: a small one, strength 2 to 4 over 2 to 19
//! steps, and a long one, strength 8 to 14 over 7 to 29 steps. Both are `TileRunner` calls with a
//! negative type, so they remove ground instead of laying any: `-1` most of the time, and `-2` for
//! one attempt in five, which is the one that leaves water behind.
//!
//! The position test that rerolls a spot is worth reading carefully because it mixes the two
//! surface numbers. A spot is rejected if it is inside the beach margin *and* above
//! `GenVars.worldSurfaceHigh`, or if it is in the middle tenth of the world *and* above
//! `GenVars.worldSurface`. Those are 306 and 267 on the reference world, not the 331 that
//! `Main.worldSurface` says, and the loop that rerolls draws two numbers each time it turns.
//!
//! The reroll loop is entered a second time for the second hole with fresh rolls, so an attempt
//! costs at least six draws plus whatever the two rerolls spend.

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};

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

    fn next_max(&mut self, max: i32) -> i32 {
        self.count += 1;
        self.rand.next_max(max)
    }
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    let ctx = RunnerContext::ordinary(vars);
    // Both surface numbers are the `GenVars.` pair here, and the pass uses them for different
    // tests: `worldSurfaceHigh` bounds the beach margin, `worldSurface` the middle of the world.
    let surface_high = vars.world_surface_high;
    let surface_high_row = surface_high as i32;
    let world_surface = vars.world_surface;
    let beach_avoidance = vars.small_holes_beach_avoidance;
    let mid_low = f64::from(width) * 0.45;
    let mid_high = f64::from(width) * 0.55;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    let attempts = (f64::from(width * height) * 0.0015) as i32;
    for _ in 0..attempts {
        // One flag per attempt, shared by both holes: one in five fills its holes with liquid.
        let carve_type = if rng.next_max(5) == 0 { -2 } else { -1 };

        // The two holes differ in size but not in how their spot is chosen.
        for (strength_roll, steps_roll) in [((2, 5), (2, 20)), ((8, 15), (7, 30))] {
            let mut x = rng.next_range(0, width);
            let mut y = rng.next_range(surface_high_row, height);
            while ((x < beach_avoidance || x > width - beach_avoidance)
                && f64::from(y) < surface_high)
                || ((f64::from(x) > mid_low && f64::from(x) < mid_high)
                    && f64::from(y) < world_surface)
            {
                x = rng.next_range(0, width);
                y = rng.next_range(surface_high_row, height);
            }
            let strength = f64::from(rng.next_range(strength_roll.0, strength_roll.1));
            let steps = rng.next_range(steps_roll.0, steps_roll.1);
            draws += tile_runner(
                world, rng.rand, ctx, x, y, strength, steps, carve_type, false, 0.0, 0.0, false,
                true, -1,
            );
        }
    }

    rng.count + draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

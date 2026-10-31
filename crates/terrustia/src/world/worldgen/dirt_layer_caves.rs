//! `Dirt Layer Caves`: the long tunnels through the dirt layer.
//!
//! Transcribed from the `Dirt Layer Caves` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12106-12145`).
//!
//! One carving runner per attempt, 151 attempts on the reference world, each strength 5 to 14 over
//! 30 to 199 steps - long and thin, which is what makes them read as caves rather than pockets.
//! One in six fills its hole with liquid (`-2`), which is why the dirt layer has water in it.
//!
//! The spot test mixes three different surface numbers, and the original is not consistent with
//! `Small Holes` about them: the beach margin is compared against `GenVars.worldSurfaceHigh` (306
//! here), the middle of the world against `Main.worldSurface` (331), and the middle test is
//! inclusive at both ends where `Small Holes` is exclusive. Each turn of that loop costs two
//! draws, so the distinction is visible in the manifest.
//!
//! The `GenVars.rockLayerHigh <= maxTilesY` guard around the body is kept as a comment rather than
//! a branch: 427 against a 1200-row world, it cannot be false for any world size the game offers,
//! and the remix doubling of the attempt count is not modelled because that flag is off.

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
    // Three different numbers, three different jobs.
    let surface_high = vars.world_surface_high;
    let surface_low_row = vars.world_surface_low as i32;
    let rock_high_row = vars.rock_layer_high as i32 + 1;
    let main_world_surface = f64::from(vars.world.surface);
    let beach_avoidance = vars.small_holes_beach_avoidance;
    let mid_low = f64::from(width) * 0.45;
    let mid_high = f64::from(width) * 0.55;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    let attempts = (f64::from(width * height) * 3e-5) as i32;
    for _ in 0..attempts {
        let carve_type = if rng.next_max(6) == 0 { -2 } else { -1 };
        let mut x = rng.next_range(0, width);
        let mut y = rng.next_range(surface_low_row, rock_high_row);
        while ((x < beach_avoidance || x > width - beach_avoidance) && f64::from(y) < surface_high)
            || ((f64::from(x) >= mid_low && f64::from(x) <= mid_high)
                && f64::from(y) < main_world_surface)
        {
            x = rng.next_range(0, width);
            y = rng.next_range(surface_low_row, rock_high_row);
        }
        let strength = f64::from(rng.next_range(5, 15));
        let steps = rng.next_range(30, 200);
        draws += tile_runner(
            world, rng.rand, ctx, x, y, strength, steps, carve_type, false, 0.0, 0.0, false, true,
            -1,
        );
    }

    rng.count + draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

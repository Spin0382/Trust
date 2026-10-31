//! `Sand Patches`: ore-shaped runners that turn existing underground blocks into sand.
//!
//! Transcribed from the `SandPatches` delegate in `WorldGen.AddPasses` (`WorldGen.cs:11725-11765`).
//! For every attempt it picks a column and a row between the surface and the rock layer, avoiding
//! a narrow band down the middle of the world near the surface, then calls
//! [`super::tile_runner::tile_runner`] with `type = 53` and `addTile = false`. That last flag is
//! what makes a patch a patch: the runner rewrites the type of blocks already there rather than
//! adding sand to open air.

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
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    // `Main.worldSurface` and `Main.rockLayer`, not `GenVars.worldSurface` and `GenVars.rockLayer`.
    // The two pairs differ by roughly 64 and 30 rows on the reference world (`Main.worldSurface` is
    // `worldSurfaceHigh + 25`), and this pass picks its depth from the `Main.` pair, so taking the
    // other one buries every patch in the dirt band instead of reaching the stone. The draw count
    // is unaffected, which is why the RNG oracle passed while the tiles did not.
    let main_world_surface = f64::from(vars.world.surface);
    let main_rock_layer = f64::from(vars.world.rock_layer);
    let ctx = RunnerContext::ordinary(vars);
    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    let attempts = (f64::from(width) * 0.013) as i32;
    for _ in 0..attempts {
        let mut x = rng.next_range(0, width);
        let mut y = rng.next_range(main_world_surface as i32, main_rock_layer as i32);
        while f64::from(x) > f64::from(width) * 0.46
            && f64::from(x) < f64::from(width) * 0.54
            && f64::from(y) < main_world_surface + 150.0
        {
            x = rng.next_range(0, width);
            y = rng.next_range(main_world_surface as i32, main_rock_layer as i32);
        }
        let strength = f64::from(rng.next_range(15, 70));
        let steps = rng.next_range(20, 130);
        draws += tile_runner(
            world, rng.rand, ctx, x, y, strength, steps, 53, false, 0.0, 0.0, false, true, -1,
        );
    }

    rng.count + draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

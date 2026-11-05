//! `Rock Layer Caves`: the caves in the stone layer.
//!
//! Transcribed from the `Rock Layer Caves` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12147-12189`).
//!
//! The largest cave pass of the three: 655 attempts on the reference world, each one strength 6 to
//! 19 over 50 to 299 steps, dug anywhere from the top of the rock layer to the bottom of the
//! world. That is 6,760,200 draws, the second biggest single-pass number in the reference manifest
//! after Jungle.
//!
//! The draw order is not the order the code reads. `type`, `strength` and `steps` are assigned
//! first, but the position is rolled inside the `TileRunner` argument list, so the sequence per
//! attempt is liquid flag, strength, steps, column, row - and a port that rolls the position first
//! will consume the same five numbers in the wrong shape and diverge at once.
//!
//! One attempt in ten fills with liquid. The `remixWorldGen` block that doubles the pass with
//! mirrored `noYChange` runners at a real speed is not modelled: that flag is off for an ordinary
//! seed.

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
    // `(int)GenVars.rockLayerHigh`, the top of the stone layer.
    let rock_high_row = vars.rock_layer_high as i32;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    let attempts = (f64::from(width * height) * 0.00013) as i32;
    for _ in 0..attempts {
        let carve_type = if rng.next_max(10) == 0 { -2 } else { -1 };
        let strength = f64::from(rng.next_range(6, 20));
        let steps = rng.next_range(50, 300);
        let x = rng.next_range(0, width);
        let y = rng.next_range(rock_high_row, height);
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

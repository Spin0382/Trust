//! `Rocks In Dirt`: the stone that sits inside the dirt layer.
//!
//! Transcribed from the `Rocks In Dirt` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:11934-11963`).
//!
//! Three `TileRunner` loops, each one converting ground that is already there into stone rather
//! than placing new tiles: `addTile` is false and `type` is 1. So this pass does not build
//! anything, it changes what the Terrain pass filled in, which is why it can afford 24,444 runners
//! - the reference world draws 3,448,247 numbers here.
//!
//! The three bands are the sky, the surface band, and the dirt above the rock layer:
//!
//! 1. `maxTilesX * maxTilesY * 0.00015` runners anywhere from row 0 down to `worldSurfaceLow`,
//!    strength 4 to 15 and 5 to 40 steps.
//! 2. `* 0.0002` runners in the `worldSurfaceLow` to `worldSurfaceHigh` band, and here the spot is
//!    rolled a second time if ten rows above it is air. That reroll is one draw and no more: it
//!    does not re-test, so a bad column can still end up starting in the sky.
//! 3. `* 0.0045` runners between `worldSurfaceHigh` and `rockLayerHigh`, the small ones (strength
//!    2 to 7, 2 to 23 steps) that make the dirt layer craggy.
//!
//! Band 2's test reads the world, so this pass's draw count depends on the terrain it was handed.
//! It is therefore not verifiable on the RNG oracle alone while the surface line is still wrong.

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};

/// `TileRunner`'s `type` for stone.
const STONE: u16 = 1;

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
    // Truncations match vanilla's `(int)` casts on the layer doubles.
    let surface_low = vars.world_surface_low as i32;
    let surface_high = vars.world_surface_high as i32;
    let rock_high = vars.rock_layer_high as i32;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let area = f64::from(width) * f64::from(height);
    let mut draws = 0usize;

    // Band 1: stone anywhere from the sky to the low surface line.
    let attempts = (area * 0.00015) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let y = rng.next_range(0, surface_low + 1);
        let strength = f64::from(rng.next_range(4, 15));
        let steps = rng.next_range(5, 40);
        draws += stone_runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    // Band 2: the surface band, rerolled once when the spot is too close to the sky.
    let attempts = (area * 0.0002) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let mut y = rng.next_range(surface_low, surface_high + 1);
        if !world.tile(x, y - 10).is_active() {
            y = rng.next_range(surface_low, surface_high + 1);
        }
        let strength = f64::from(rng.next_range(4, 10));
        let steps = rng.next_range(5, 30);
        draws += stone_runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    // Band 3: the craggy stone between the surface and the rock layer.
    let attempts = (area * 0.0045) as i32;
    for _ in 0..attempts {
        let x = rng.next_range(0, width);
        let y = rng.next_range(surface_high, rock_high + 1);
        let strength = f64::from(rng.next_range(2, 7));
        let steps = rng.next_range(2, 23);
        draws += stone_runner(world, rng.rand, ctx, x, y, strength, steps);
    }

    rng.count + draws
}

/// One runner: `TileRunner(x, y, strength, steps, 1)`, all the other arguments left at the
/// defaults the game declares (`addTile: false`, `speedX`/`speedY` 2, `noYChange: false`,
/// `overRide: true`, `ignoreTileType: -1`).
fn stone_runner(
    world: &mut crate::world::World,
    rand: &mut UnifiedRandom,
    ctx: RunnerContext,
    x: i32,
    y: i32,
    strength: f64,
    steps: i32,
) -> usize {
    tile_runner(
        world, rand, ctx, x, y, strength, steps, STONE, false, 2.0, 2.0, false, true, -1,
    )
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

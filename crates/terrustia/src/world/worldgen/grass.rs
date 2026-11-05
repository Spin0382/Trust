//! `Grass`: the two ways a dirt pocket becomes grass, and nothing else.
//!
//! Transcribed from the `Grass` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12466-12506`).
//!
//! Two rolls per attempt, `maxTilesX * maxTilesY * 0.002` attempts - 10,080 on the reference
//! world, and four draws each, which is the pass's whole 40,320. The first roll puts its spot in
//! the band between the surface's extremes, where grass joins the surface to the dirt underneath;
//! the second puts it between row 5 and the low surface line, where grass reaches up into the
//! hills.
//!
//! Both then apply the same rule, and it is a strict one: the tile is only converted when all four
//! of its neighbours are active **and dirt**. A pocket with any exposure at all is left alone, so
//! what gets grass is the inside of a blob, not its skin. That is also why the pass cannot change
//! a draw count by what it finds - the rolls come first, so the manifest sees 40,320 whatever the
//! ground looks like, and only the stepped tile check can tell whether the pockets were right.
//!
//! The two secret-seed hooks at the head of the pass (`digExtraHoles`, `roundLandmasses`) are not
//! modelled: both are off for an ordinary seed, and both draw.

use super::passes::GenVars;
use super::rand::UnifiedRandom;

/// `TileID.Grass`, which is dirt with a grass top for generation purposes.
const GRASS: u16 = 2;
/// `TileID.Dirt`, the only neighbour type the test accepts.
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

/// Would this spot take grass? All four neighbours must be active dirt.
fn enclosed(world: &crate::world::World, x: i32, y: i32) -> bool {
    [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
        .iter()
        .all(|(dx, dy)| {
            let tile = world.tile(x + dx, y + dy);
            tile.is_active() && tile.block == DIRT
        })
}

/// One roll: place grass if the spot qualifies.
fn try_grass(
    world: &mut crate::world::World,
    rng: &mut Draws<'_>,
    y_low: i32,
    y_high: i32,
    height: i32,
) {
    let x = rng.next_range(1, world.width() - 1);
    let mut y = rng.next_range(y_low, y_high);
    if y >= height {
        y = height - 2;
    }
    if enclosed(world, x, y) {
        let mut tile = world.tile(x, y);
        tile.block = GRASS;
        tile.flags.set(terrustia_proto::TileFlags::ACTIVE, true);
        world.set_tile(x, y, tile);
    }
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    let surface_low = vars.world_surface_low as i32;
    let surface_high = vars.world_surface_high as i32;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);

    // The bound is a double compared against the loop counter, as in the original - the place an
    // integer cast can quietly cost one attempt. Here the product is exact, so both agree.
    let attempts = f64::from(width) * f64::from(height) * 0.002;
    let mut index = 0i32;
    while f64::from(index) < attempts {
        index += 1;
        try_grass(world, &mut rng, surface_low, surface_high, height);
        try_grass(world, &mut rng, 5, surface_low, height);
    }

    rng.count
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

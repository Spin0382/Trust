//! `Ocean Sand`: the sand that lines the two oceans, and the pyramid sites the middle of each one
//! may record.
//!
//! Transcribed from the `OceanSand` delegate in `WorldGen.AddPasses` (`WorldGen.cs:11615-11725`).
//! The pass walks three bands: the left ocean (`0..leftBeachEnd`), a middle band that is rolled
//! for its draws and then skipped, and the right ocean (`rightBeachStart..width`). For each
//! column it finds the first active tile and converts the next `num7` tiles below it to sand,
//! with a random amount trimmed off each end.
//!
//! **The inner condition draws RNG.** `j > num4 + genRand.Next(5) && j < num5 - genRand.Next(5)`
//! is inside the per-depth loop, and `&&` short-circuits, so the second roll is only taken when
//! the first comparison is true. That is a draw whose count depends on the column and on the
//! first roll, and it is why this pass cannot be tiled without reproducing the stream exactly.

use super::passes::GenVars;
use super::rand::UnifiedRandom;

/// `TileID.Sand`.
const SAND: u16 = 53;

/// A generator that also counts its draws, for the pass oracle.
struct Draws<'a> {
    rand: &'a mut UnifiedRandom,
    count: usize,
}

impl<'a> Draws<'a> {
    fn new(rand: &'a mut UnifiedRandom) -> Self {
        Self { rand, count: 0 }
    }

    fn next_max(&mut self, max: i32) -> i32 {
        self.count += 1;
        self.rand.next_max(max)
    }

    fn next_range(&mut self, min: i32, max: i32) -> i32 {
        self.count += 1;
        self.rand.next_range(min, max)
    }
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let world_surface = vars.world_surface;
    let rock_layer = vars.rock_layer;
    let left_beach_end = vars.left_beach_end;
    let right_beach_start = vars.right_beach_start;

    let GenVars {
        rand,
        world,
        pyr_x,
        pyr_y,
        num_pyr,
        ..
    } = vars;
    let mut rng = Draws::new(rand);

    for band in 0..3 {
        // `int num = genRand.Next(Main.maxTilesX);` and its mid-world reroll.
        let mut centre = rng.next_max(width);
        while f64::from(centre) > f64::from(width) * 0.4
            && f64::from(centre) < f64::from(width) * 0.6
        {
            centre = rng.next_max(width);
        }

        // First half. The mid band adds one scaled roll, then both take the coin flip. The value
        // only feeds `num4 = centre - half`, which bands 0 and 2 replace with their beach bound
        // and band 1 never walks, so only the draws survive here.
        let _ = rng.next_range(35, 90);
        if band == 1 {
            let scale = f64::from(width) / 4200.0;
            let _ = (f64::from(rng.next_range(20, 40)) * scale) as i32;
        }
        let _ = rng.next_max(3);

        // Second half, the same way: one width roll and one coin flip.
        let _ = rng.next_range(35, 90);
        let _ = rng.next_max(3);

        let (from, to) = if band == 0 {
            (0, left_beach_end)
        } else if band == 2 {
            (right_beach_start, width)
        } else {
            // The middle band has taken every draw the other two do up to here, then skips its
            // own column walk (`else if (i == 1) { continue; }`).
            continue;
        };

        let mut depth = rng.next_range(50, 100);
        for column in from..to {
            if rng.next_max(2) == 0 {
                depth += rng.next_range(-1, 2);
                depth = depth.clamp(50, 200);
            }
            let limit = (world_surface + rock_layer) / 2.0;
            let mut row = 0;
            while f64::from(row) < limit {
                if !world.tile(column, row).is_active() {
                    row += 1;
                    continue;
                }
                if column == (from + to) / 2 {
                    // The draw is unconditional in vanilla (`Main.rand.Next(6) == 0` sits inside
                    // the column test), so it is taken even when the pyramid list is already full.
                    let picked = rng.next_max(6) == 0;
                    if picked && *num_pyr < pyr_x.len() {
                        pyr_x[*num_pyr] = column;
                        pyr_y[*num_pyr] = row;
                        *num_pyr += 1;
                    }
                }
                let mut thickness = depth;
                if column - from < thickness {
                    thickness = column - from;
                }
                if to - column < thickness {
                    thickness = to - column;
                }
                thickness += rng.next_max(5);
                for depth_row in row..row + thickness {
                    // The short-circuit is load-bearing: the second draw only happens when the
                    // first comparison is true.
                    if column > from + rng.next_max(5) && column < to - rng.next_max(5) {
                        let mut tile = world.tile(column, depth_row);
                        tile.block = SAND;
                        world.set_tile(column, depth_row, tile);
                    }
                }
                break;
            }
        }
    }

    rng.count
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

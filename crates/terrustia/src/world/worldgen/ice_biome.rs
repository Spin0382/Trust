//! `Generate Ice Biome`: the snow and ice columns, and the `snowMinX`/`snowMaxX` maps.
//!
//! Transcribed from the `IceBiome` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12355-12466`).
//!
//! The pass walks down the world one row at a time, random-walking the biome's left and right edges
//! and averaging each into the previous row - which is what makes the snow boundary a wobbly
//! near-vertical line rather than a straight one - and records the result in `GenVars.snowMinX` and
//! `GenVars.snowMaxX` for the later ice and snow passes to read back. It stops 140 rows above the
//! lava line, and writes `GenVars.snowBottom` as it goes.
//!
//! Three things about it are easy to get wrong.
//!
//! The dungeon bias is an `if`/`else` on which side the dungeon landed, and **both** branches draw
//! `Next(4)`; only the direction of the ±1 nudge differs. Reading it as "nudge only if the dungeon
//! is to the right" would drop a number on every one of the 692 rows.
//!
//! The `depth` walk lives *inside* the per-column loop, not beside it, so a snow drift's pile depth
//! random-walks once per column of every row below the snow line. That is where the bulk of this
//! pass's 43,148 draws come from, and it is why the buried ice is ragged in both directions.
//!
//! Conversion happens whether or not the tile is active, so the type of a tile above a cave's
//! ceiling changes too: dirt, mud, clay, sand and grass-bearing dirt become snow blocks (147),
//! stone becomes ice (161), and a dirt wall becomes a snow wall (40).
//!
//! The edges are indexed unclamped, which in the game means a walk that drifts off the world would
//! throw. Our accessor reads air and declines the write, so the two agree on every in-world tile;
//! the reference's own walk stays inside.

use super::passes::{DungeonSide, GenVars};
use super::rand::UnifiedRandom;
use crate::world::World;

/// `TileID.SnowBlock`.
const SNOW_BLOCK: u16 = 147;
/// `TileID.IceBlock`.
const ICE_BLOCK: u16 = 161;
/// `Main.wallDirt` and `Main.wallSnow`, the one wall conversion the pass makes.
const DIRT_WALL: u16 = 2;
const SNOW_WALL: u16 = 40;

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

/// The six conversions, plus the wall.
fn freeze(world: &mut World, column: i32, row: i32) {
    let mut tile = world.tile(column, row);
    if tile.wall == DIRT_WALL {
        tile.wall = SNOW_WALL;
    }
    tile.block = match tile.block {
        0 | 2 | 23 | 40 | 53 => SNOW_BLOCK,
        1 => ICE_BLOCK,
        other => other,
    };
    world.set_tile(column, row, tile);
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let lava_line = vars.lava_line;
    let dungeon_right = vars.dungeon_side == DungeonSide::Right;
    let mut left = vars.snow_origin_left;
    let mut right = vars.snow_origin_right;
    // `GenVars.snowTop`, which is `Main.worldSurface` rather than anything the biome walk says.
    vars.snow_top = i32::from(vars.world.surface);
    // `num2 - 140`, the last row the walk reaches. `num` is where the pile depth starts instead,
    // and its roll is this pass's first draw.
    let last_row = lava_line - 140;
    let pile_from = lava_line - vars.rand.next_range(160, 200);

    let GenVars {
        rand,
        world,
        snow_min_x,
        snow_max_x,
        snow_bottom,
        ..
    } = vars;
    let mut rng = Draws::new(rand);
    let mut depth = 10i32;

    for row in 0..=last_row {
        left += rng.next_range(-4, 4);
        right += rng.next_range(-3, 5);
        if row > 0 {
            // Averaged into the row above, so a single bad roll cannot tear the edge.
            left = (left + snow_min_x[(row - 1) as usize]) / 2;
            right = (right + snow_max_x[(row - 1) as usize]) / 2;
        }
        // Both branches draw; only the nudge differs. The dungeon is not to be buried.
        if dungeon_right {
            if rng.next_max(4) == 0 {
                left += 1;
                right += 1;
            }
        } else if rng.next_max(4) == 0 {
            left -= 1;
            right -= 1;
        }
        snow_min_x[row as usize] = left;
        snow_max_x[row as usize] = right;

        for column in left..right {
            if row < pile_from {
                freeze(world, column, row);
                continue;
            }
            // The depth walk, once per column. It can be re-rolled twice downward and is bounded at
            // both ends by a fresh roll rather than a clamp.
            depth += rng.next_range(-3, 4);
            if rng.next_max(3) == 0 {
                depth += rng.next_range(-4, 5);
                if rng.next_max(3) == 0 {
                    depth += rng.next_range(-6, 7);
                }
            }
            if depth < 0 {
                depth = rng.next_max(3);
            } else if depth > 50 {
                depth = 50 - rng.next_max(3);
            }
            for pile_row in row..row + depth {
                freeze(world, column, pile_row);
            }
        }
        if *snow_bottom < row {
            *snow_bottom = row;
        }
    }

    rng.count
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

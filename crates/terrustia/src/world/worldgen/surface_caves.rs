//! `Surface Caves`: the caves that open at or just under the surface.
//!
//! Transcribed from the `Surface Caves` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12203-12290`).
//!
//! Four bands of `TileRunner` worms and a fifth of [`super::caverer::caverer`] worms. Every band
//! picks its spot the same way - roll a column, reroll while it is in the middle of the world or
//! within twenty tiles of a beach, then walk down from row 0 to `GenVars.worldSurfaceHigh` and hand
//! the runner the first active tile it finds - and the scan itself draws nothing, so a column whose
//! ground sits deeper than that bound costs its position rolls and yields no cave.
//!
//! The bands differ in size, in how wide a middle they exclude, and in which way they are sent.
//! `speedX` is a fresh `Next(-10, 11) * 0.1` in the first three and a hard zero in the fourth, and
//! `speedY` is 1, 2, 4, 1: all four run downward, and the fourth alone is `noYChange`, which
//! flattens its vertical drift instead of letting it wander. The excluded middle widens band by
//! band (0.45-0.55, then 0.43-0.57, then 0.4-0.6), and the last two bands are a single attempt each
//! on a small world, which is why they read as occasions rather than a pattern.
//!
//! The attempt counts are `(int)(maxTilesX * fraction)` compared against an `int` loop variable, so
//! unlike `Rocks In Dirt` there is no double-bound off-by-one here. The fifth band's row range
//! carries a real quirk: its low bound is `GenVars.surfaceCavesBeachAvoidance2` while its high
//! bound is `surfaceCavesBeachAvoidance`, and that is transcribed as written.

use super::caverer::caverer;
use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};
use crate::world::World;

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

/// A band's column: roll, then reroll while the column is inside the excluded middle or within
/// `edge` tiles of either beach.
fn pick_column(
    rng: &mut Draws<'_>,
    width: i32,
    mid_low: f64,
    mid_high: f64,
    left_edge: i32,
    right_edge: i32,
) -> i32 {
    let mut column = rng.next_range(0, width);
    while (f64::from(column) > f64::from(width) * mid_low
        && f64::from(column) < f64::from(width) * mid_high)
        || column < left_edge
        || column > right_edge
    {
        column = rng.next_range(0, width);
    }
    column
}

/// The first active tile in `column` above `limit`, which is where a surface cave starts.
fn first_solid(world: &World, column: i32, limit: f64) -> Option<i32> {
    let mut row = 0i32;
    while f64::from(row) < limit {
        if world.tile(column, row).is_active() {
            return Some(row);
        }
        row += 1;
    }
    None
}

/// `TileRunner(x, y, strength, steps, -1, addTile: false, speedX, speedY, noYChange)`.
///
/// `rolled_drift` is whether `speedX` is drawn or fixed; the original draws it in the argument
/// list, after strength and steps, so the order below is the one the stream sees.
#[allow(clippy::too_many_arguments)]
fn worm(
    world: &mut World,
    rng: &mut Draws<'_>,
    ctx: RunnerContext,
    x: i32,
    y: i32,
    strength: (i32, i32),
    steps: (i32, i32),
    rolled_drift: bool,
    speed_y: f64,
    no_y_change: bool,
) -> usize {
    let strength = f64::from(rng.next_range(strength.0, strength.1));
    let steps = rng.next_range(steps.0, steps.1);
    let drift = if rolled_drift {
        f64::from(rng.next_range(-10, 11)) * 0.1
    } else {
        0.0
    };
    tile_runner(
        world,
        rng.rand,
        ctx,
        x,
        y,
        strength,
        steps,
        -1,
        false,
        drift,
        speed_y,
        no_y_change,
        true,
        -1,
    )
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    let ctx = RunnerContext::ordinary(vars);
    let surface_high = vars.world_surface_high;
    let left_edge = vars.left_beach_end + 20;
    let right_edge = vars.right_beach_start - 20;
    let rock_row = i32::from(vars.world.rock_layer);
    let deep_row = height - 400;
    let cave_floor = if rock_row >= deep_row {
        deep_row - 1
    } else {
        rock_row
    };
    // The original's fifth band is asymmetric by accident or by intent: the low bound is the
    // `...2` value and the high bound is not.
    let cave_left = vars.surface_caves_beach_avoidance2;
    let cave_right = vars.surface_caves_beach_avoidance;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);
    let mut draws = 0usize;

    // Band 1: eight shallow worms, each one dig.
    let attempts = (f64::from(width) * 0.002) as i32;
    for _ in 0..attempts {
        let column = pick_column(&mut rng, width, 0.45, 0.55, left_edge, right_edge);
        if let Some(row) = first_solid(world, column, surface_high) {
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (3, 6),
                (5, 50),
                true,
                1.0,
                false,
            );
        }
    }

    // Band 2: two longer ones, sent faster.
    let attempts = (f64::from(width) * 0.0007) as i32;
    for _ in 0..attempts {
        let column = pick_column(&mut rng, width, 0.43, 0.57, left_edge, right_edge);
        if let Some(row) = first_solid(world, column, surface_high) {
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (10, 15),
                (50, 130),
                true,
                2.0,
                false,
            );
        }
    }

    // Band 3: one column, three worms from the same start, in decreasing size.
    let attempts = (f64::from(width) * 0.0003) as i32;
    for _ in 0..attempts {
        let column = pick_column(&mut rng, width, 0.4, 0.6, left_edge, right_edge);
        if let Some(row) = first_solid(world, column, surface_high) {
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (12, 25),
                (150, 500),
                true,
                4.0,
                false,
            );
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (8, 17),
                (60, 200),
                true,
                2.0,
                false,
            );
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (5, 13),
                (40, 170),
                true,
                2.0,
                false,
            );
        }
    }

    // Band 4: one wide, flat, downward-poking worm. `noYChange` with a `speedY` of 1 is what makes
    // a horizontal gallery rather than a slope.
    let attempts = (f64::from(width) * 0.0004) as i32;
    for _ in 0..attempts {
        let column = pick_column(&mut rng, width, 0.4, 0.6, left_edge, right_edge);
        if let Some(row) = first_solid(world, column, surface_high) {
            draws += worm(
                world,
                &mut rng,
                ctx,
                column,
                row,
                (7, 12),
                (150, 250),
                false,
                1.0,
                true,
            );
        }
    }

    // Band 5: five `Caverer` worms anywhere between the rock layer and 400 rows from the bottom.
    let scale = f64::from(width) / 4200.0;
    let attempts = (5.0 * scale) as i32;
    for _ in 0..attempts {
        let column = rng.next_range(cave_left, width - cave_right);
        let row = rng.next_range(cave_floor, deep_row);
        draws += caverer(world, rng.rand, ctx, column, row);
    }

    rng.count + draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

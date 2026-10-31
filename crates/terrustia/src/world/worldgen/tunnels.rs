//! `Tunnels`: the surface tunnels that open into the underground.
//!
//! Transcribed from the `Tunnels` delegate in `WorldGen.AddPasses` (`WorldGen.cs:11766-11828`).
//! Each attempt picks a column, then walks ten parallel bores five to ten tiles apart. For every
//! column it scans down to the first active tile, backs up eleven to fifteen tiles, and runs two
//! `TileRunner` bores in opposite horizontal directions at a slight upward angle (`speedX` -2 or
//! +2, `speedY` -0.3). The scan's row is shared and only moves down, which is why the ten bores
//! follow the surface rather than restarting in the sky for each one.
//!
//! If any of the ten columns first meets sand, the whole ten-column block is rerolled. That loop
//! draws, so it is part of the stream even when it produces no tunnel.

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};

/// `GenVars.maxTunnels` (`GenVars.cs:234`).
const MAX_TUNNELS: usize = 50;

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
    let GenVars {
        rand,
        world,
        tunnel_x,
        num_tunnels,
        ..
    } = vars;
    let mut rng = Draws::new(rand);
    let mut runner_draws = 0usize;

    let attempts = (f64::from(width) * 0.0015) as i32;
    for _ in 0..attempts {
        if *num_tunnels >= MAX_TUNNELS - 1 {
            break;
        }
        let mut x = rng.next_range(450, width - 450);
        while f64::from(x) > f64::from(width) * 0.4 && f64::from(x) < f64::from(width) * 0.6 {
            x = rng.next_range(450, width - 450);
        }

        let mut columns = [0i32; 10];
        let mut tops = [0i32; 10];
        // The scan's row is deliberately not reset between the ten columns or between retries:
        // vanilla keeps `j` from one column to the next, so later bores start at the row the
        // first one reached.
        let mut row = 0i32;
        loop {
            let mut sandy = false;
            for slot in 0..10 {
                x %= width;
                while row < height && !world.tile(x, row).is_active() {
                    row += 1;
                }
                let row_clamped = row.min(height - 1);
                if world.tile(x, row_clamped).block == 53 {
                    sandy = true;
                }
                columns[slot] = x;
                tops[slot] = row_clamped - rng.next_range(11, 16);
                x += rng.next_range(5, 11);
            }
            if !sandy {
                break;
            }
        }

        tunnel_x[*num_tunnels] = columns[5];
        *num_tunnels += 1;
        for slot in 0..10 {
            let strength = f64::from(rng.next_range(5, 8));
            let steps = rng.next_range(6, 9);
            runner_draws += tile_runner(
                world,
                rng.rand,
                ctx,
                columns[slot],
                tops[slot],
                strength,
                steps,
                0,
                true,
                -2.0,
                -0.3,
                false,
                true,
                -1,
            );
            let strength = f64::from(rng.next_range(5, 8));
            let steps = rng.next_range(6, 9);
            runner_draws += tile_runner(
                world,
                rng.rand,
                ctx,
                columns[slot],
                tops[slot],
                strength,
                steps,
                0,
                true,
                2.0,
                -0.3,
                false,
                true,
                -1,
            );
        }
    }

    rng.count + runner_draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

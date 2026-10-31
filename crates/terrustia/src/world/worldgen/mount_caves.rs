//! `Mount Caves`: the dirt blobs buried under the surface mountains.
//!
//! Transcribed from the `Mount Caves` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:11821-11894`) and `WorldGen.Mountinater` (`WorldGen.cs:80664-80752`).
//!
//! There are only `maxTilesX * 0.001` attempts, four on a small world, and each one has to clear
//! two screens before anything is dug: the column must sit outside the middle 180 tiles, and it
//! must be at least 100 columns away from every mountain cave already placed. Then the first
//! active tile from the top down is taken, and a 100 by 50 box centred on it is scanned for sand
//! (53), ice (151) or icestone (274). Finding any of them marks the attempt rejected.
//!
//! That rejection is permanent for the attempt, because vanilla sets its `flag` inside the scan
//! but only ever tests it again after the scan. So once a box has caught sand, the column walk
//! keeps running to the bottom of the surface without ever calling `Mountinater` again. It costs
//! the scan draws and produces nothing, which is exactly what the reference world shows.
//!
//! What `Mountinater` digs is not a cave in the usual sense: it walks a shrinking circle upward
//! and fills the *air* inside it with dirt. The opening to the surface comes later, from the
//! `Mountain Cave Openings` pass.

use super::passes::GenVars;
use super::rand::UnifiedRandom;

/// `GenVars.mCaveX`/`mCaveY` are `new int[30]` (`GenVars.cs:230-232`).
const MAX_MOUNT_CAVES: usize = 30;

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

/// `WorldGen.Mountinater` (`WorldGen.cs:80664-80752`).
///
/// Returns the number of draws it took. The walk shrinks `strength` by a random 0 to 3 per step
/// and spends one unit of `height` per step, so it stops at whichever runs out first.
fn mountinater(
    world: &mut crate::world::World,
    rng: &mut Draws<'_>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> usize {
    let before = rng.count;

    // Both are doubles in vanilla. `budget` (`num3`) is a step count rather than a length, and it
    // is what places the walk's start below the surface.
    let mut strength = f64::from(rng.next_range(80, 120));
    let mut budget = f64::from(rng.next_range(40, 55));

    let mut px = f64::from(x);
    let mut py = f64::from(y) + budget / 2.0;
    let mut vx = f64::from(rng.next_range(-10, 11)) * 0.1;
    // The upward drift starts between -2.0 and -1.0 tiles per step, and is clamped to
    // [-1.5, -0.5] below, so the walk always rises.
    let mut vy = f64::from(rng.next_range(-20, -10)) * 0.1;

    while strength > 0.0 && budget > 0.0 {
        strength -= f64::from(rng.next_max(4));
        budget -= 1.0;

        let from_x = (px - strength * 0.5).max(0.0) as i32;
        let to_x = ((px + strength * 0.5) as i32).min(width);
        let from_y = (py - strength * 0.5).max(0.0) as i32;
        let to_y = ((py + strength * 0.5) as i32).min(height);

        // `num2` is recomputed from `num` every step, so the blob's width follows the shrinking
        // box rather than the walk. The value assigned before the loop is never read.
        let radius = strength * f64::from(rng.next_range(80, 120)) * 0.01;
        let limit = radius * 0.4;
        for column in from_x..to_x {
            for row in from_y..to_y {
                let dx = f64::from(column) - px;
                let dy = f64::from(row) - py;
                if (dx * dx + dy * dy).sqrt() < limit {
                    let mut tile = world.tile(column, row);
                    if !tile.is_active() {
                        tile.block = 0;
                        tile.flags.set(terrustia_proto::TileFlags::ACTIVE, true);
                        world.set_tile(column, row, tile);
                    }
                }
            }
        }

        px += vx;
        py += vy;
        vx = (vx + f64::from(rng.next_range(-10, 11)) * 0.05).clamp(-0.5, 0.5);
        // Vanilla clamps the vertical drift to [-1.5, -0.5] with four separate `if`s, which is a
        // clamp, not a wrap: the sign never flips.
        vy = (vy + f64::from(rng.next_range(-10, 11)) * 0.05).clamp(-1.5, -0.5);
    }

    rng.count - before
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    // `Main.worldSurface`, not `GenVars.worldSurface`: the walk runs to the highest surface row
    // plus 25, so a column whose ground sits below the last-painted line is still searched.
    let surface = f64::from(vars.world.surface);
    let GenVars {
        rand,
        world,
        mount_cave_x,
        mount_cave_y,
        num_mount_caves,
        ..
    } = vars;
    let mut rng = Draws::new(rand);
    *num_mount_caves = 0;

    let attempts = (f64::from(width) * 0.001) as i32;
    let mid_low = width / 2 - 90;
    let mid_high = width / 2 + 90;
    let give_up = width / 5;

    for _ in 0..attempts {
        let mut conflicts = 0i32;
        let mut rejected = false;
        let mut column = rng.next_range(width / 4, width * 3 / 4);
        // `flag2` is the "this column is far enough from the others" latch. A conflict resets it,
        // and `conflicts` is not cleared between retries, so a unlucky column can walk the loop
        // several times before reaching the give-up test.
        let mut accepted = false;
        while !accepted {
            accepted = true;
            while column > mid_low && column < mid_high {
                column = rng.next_range(width / 4, width * 3 / 4);
            }
            for seen_x in &mount_cave_x[..*num_mount_caves] {
                if (column - seen_x).abs() < 100 {
                    conflicts += 1;
                    accepted = false;
                    break;
                }
            }
            if conflicts >= give_up {
                rejected = true;
                break;
            }
        }
        if rejected {
            continue;
        }

        let mut row = 0i32;
        while f64::from(row) < surface {
            if world.tile(column, row).is_active() {
                // The 100 by 50 screen, drawn straight from the two `for`s at
                // `WorldGen.cs:11871-11874`. It reads tiles only, so it costs no draws.
                let mut sandy = false;
                for scan_x in column - 50..column + 50 {
                    for scan_y in row - 25..row + 25 {
                        let tile = world.tile(scan_x, scan_y);
                        if tile.is_active() && matches!(tile.block, 53 | 151 | 274) {
                            sandy = true;
                        }
                    }
                }
                // Vanilla's `flag` is shared between the column screen and this walk: the scan
                // sets it, and only `if (!flag) { Mountinater; break; }` reads it. So the first
                // sandy box kills the rest of the walk, and a later, cleaner row digs nothing.
                if sandy {
                    rejected = true;
                }
                if !rejected {
                    mountinater(world, &mut rng, column, row, width, height);
                    // Vanilla indexes `mCaveX[numMCaves]` unguarded, which would throw past 30.
                    // Four attempts can never reach it, so the guard is unreachable here.
                    if *num_mount_caves < MAX_MOUNT_CAVES {
                        mount_cave_x[*num_mount_caves] = column;
                        mount_cave_y[*num_mount_caves] = row;
                        *num_mount_caves += 1;
                    }
                    break;
                }
            }
            row += 1;
        }
    }

    rng.count
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

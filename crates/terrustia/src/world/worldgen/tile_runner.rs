//! `WorldGen.TileRunner` for an ordinary world, as far as the passes ported so far need it.
//!
//! The game's runner is one routine with a long tail of secret-seed, liquid and wall cases. This
//! is the ordinary-world spine: the tapered walk, the jittered diamond, the extra sub-steps a
//! strength above 50 takes, and the type >= 0 placement branch. `Sand Patches` calls it with
//! `type = 53` and `addTile = false`, so it rewrites the type of existing blocks rather than
//! activating air. Later passes add `addTile = true`, `type < 0` carving and the mud-wall branch;
//! those are left out rather than guessed at.
//!
//! Sources: `WorldGen.TileRunner` (`WorldGen.cs:77598-78090`), the `beachDistance` reset
//! (`WorldGen.cs:4071`), and `Main.tileCut` (`Main.cs:7503-10389`). `Main.tileFrameImportant` is
//! the generated `tile_sets::frame_important` table.

use terrustia_proto::tile_sets::frame_important;

use super::rand::UnifiedRandom;
use crate::world::World;

/// `WorldGen.beachDistance` (`WorldGen.cs:4071`).
const BEACH_DISTANCE: i32 = 380;

/// `Main.tileCut`: the frame-important tiles a runner may cut through (plants, vines, pots).
/// Extracted from the `tileCut[N] = true` assignments in `Main.cs:7503-10389`.
fn tile_cut(block: u16) -> bool {
    matches!(
        block,
        3 | 24
            | 28
            | 32
            | 51
            | 52
            | 61
            | 62
            | 69
            | 71
            | 73
            | 74
            | 82
            | 83
            | 84
            | 110
            | 113
            | 115
            | 184
            | 201
            | 205
            | 231
            | 236
            | 254
            | 352
            | 382
            | 444
            | 454
            | 484
            | 485
            | 518
            | 519
            | 528
            | 529
            | 549
            | 636
            | 637
            | 638
            | 654
            | 655
            | 711
    )
}

/// `TileID.Sets.CanBeClearedDuringGeneration` (`TileID.cs:321`), a default-true set whose listed
/// exceptions are the tiles terrain passes must leave alone: sandstone and hardened sand, granite
/// and marble, dungeon and temple bricks and cracked bricks, and desert fossil.
fn can_be_cleared(block: u16) -> bool {
    !matches!(
        block,
        396 | 400
            | 401
            | 397
            | 398
            | 399
            | 404
            | 368
            | 367
            | 41
            | 43
            | 44
            | 481
            | 482
            | 483
            | 226
            | 237
    )
}

/// `TileID.Sets.SaveSlopes`: every solid tile plus the non-solid tiles whose slope survives.
fn save_slope(block: u16) -> bool {
    terrustia_proto::tile_solid::solid(block)
        || matches!(block, 131 | 351 | 336 | 340 | 342 | 341 | 343 | 344)
}

/// Whether a runner may replace this tile.
///
/// `TileRunner`'s `flag4` switch (`WorldGen.cs:77845-77880`). The branches that draw are kept;
/// they only run for a runner of type 59 (mud), which is why a sand patch consumes none of them.
fn over_ride_skips(
    tile: &terrustia_proto::Tile,
    runner_type: u16,
    rand: &mut UnifiedRandom,
) -> bool {
    if !can_be_cleared(tile.block) {
        return true;
    }
    match tile.block {
        45 | 147 | 189 | 190 | 196 | 460 | 717 | 718 | 719 => true,
        396 | 397 => true,
        1 => {
            if runner_type == 59 {
                let _ = rand.next_range(-50, 50);
                true
            } else {
                false
            }
        }
        367 | 368 => runner_type == 59,
        _ => false,
    }
}

/// `Main.tileStone`: the gem and a few stone-like tiles whose replacement follows the stone rule.
fn tile_stone(block: u16) -> bool {
    matches!(block, 63 | 64 | 65 | 66 | 67 | 68 | 130 | 131 | 566)
}

/// What a [`TileRunner`] call needs from `GenVars` beyond the world and RNG.
#[derive(Clone, Copy)]
pub struct RunnerContext {
    pub world_surface: f64,
    pub water_line: i32,
}

/// `WorldGen.TileRunner` for the ordinary-world path, type >= 0 only.
///
/// Returns the number of RNG draws taken, so a pass can check its own total against the manifest.
/// `add_tile` false rewrites existing types; true also activates the tile and clears its liquid.
#[allow(clippy::too_many_arguments)]
pub fn tile_runner(
    world: &mut World,
    rand: &mut UnifiedRandom,
    ctx: RunnerContext,
    i: i32,
    j: i32,
    strength: f64,
    steps: i32,
    tile_type: u16,
    add_tile: bool,
    speed_x: f64,
    speed_y: f64,
    no_y_change: bool,
    over_ride: bool,
    ignore_tile_type: i32,
) -> usize {
    let mut draws = 0usize;
    macro_rules! next_max {
        ($max:expr) => {{
            draws += 1;
            rand.next_max($max)
        }};
    }
    macro_rules! next_range {
        ($min:expr, $max:expr) => {{
            draws += 1;
            rand.next_range($min, $max)
        }};
    }

    let mut strength = strength;
    let mut steps = steps;
    let mut num = strength;
    let mut num2 = f64::from(steps);
    let mut px = f64::from(i);
    let mut py = f64::from(j);
    let (mut vx, mut vy) = (
        f64::from(next_range!(-10, 11)) * 0.1,
        f64::from(next_range!(-10, 11)) * 0.1,
    );
    if speed_x != 0.0 || speed_y != 0.0 {
        vx = speed_x;
        vy = speed_y;
    }
    let clear_slope = !save_slope(tile_type);
    // The `lost` liquid kind. Ordinary worlds leave it at water; the draw still happens.
    let _liquid_kind = next_max!(4);

    while num > 0.0 && num2 > 0.0 {
        num = strength * (num2 / f64::from(steps));
        num2 -= 1.0;
        let x0 = ((px - num * 0.5) as i32).max(1);
        let x1 = ((px + num * 0.5) as i32).min(world.width() - 1);
        let y0 = ((py - num * 0.5) as i32).max(1);
        let y1 = ((py + num * 0.5) as i32).min(world.height() - 1);

        for k in x0..x1 {
            for l in y0..y1 {
                let tile = world.tile(k, l);
                if (tile.is_active() && frame_important(tile.block) && !tile_cut(tile.block))
                    || (ignore_tile_type >= 0
                        && tile.is_active()
                        && i32::from(tile.block) == ignore_tile_type)
                {
                    continue;
                }
                let jitter = 1.0 + f64::from(next_range!(-10, 11)) * 0.015;
                if (f64::from(k) - px).abs() + (f64::from(l) - py).abs() >= strength * 0.5 * jitter
                {
                    continue;
                }
                if k < BEACH_DISTANCE + 50 || k >= world.width() - BEACH_DISTANCE - 50 {
                    // Vanilla resets its liquid type here; for the type >= 0 path this has no
                    // observable effect beyond the reset itself.
                }
                let mut tile = world.tile(k, l);
                let mut skip = false;
                if over_ride && tile.is_active() {
                    skip = over_ride_skips(&tile, tile_type, rand);
                    if !skip && tile_stone(tile_type) && tile.block != 1 {
                        skip = true;
                    }
                }
                if !skip {
                    tile.block = tile_type;
                    if clear_slope {
                        tile.slope = 0;
                        tile.flags
                            .set(terrustia_proto::TileFlags::HALF_BRICK, false);
                    }
                }
                if add_tile {
                    tile.flags.set(terrustia_proto::TileFlags::ACTIVE, true);
                    tile.liquid = 0;
                    tile.liquid_kind = terrustia_proto::Liquid::Water;
                }
                if no_y_change && f64::from(l) < ctx.world_surface && tile_type != 59 {
                    tile.wall = 2;
                }
                if tile_type == 59 && l > ctx.water_line && tile.liquid > 0 {
                    tile.liquid = 0;
                    tile.liquid_kind = terrustia_proto::Liquid::Water;
                }
                world.set_tile(k, l, tile);
            }
        }

        px += vx;
        py += vy;
        if num > 50.0 {
            // A strength above 50 takes extra whole steps, each with its own two drift draws.
            let mut extra =
                |px: &mut f64, py: &mut f64, num2: &mut f64, vx: &mut f64, vy: &mut f64| {
                    *px += *vx;
                    *py += *vy;
                    *num2 -= 1.0;
                    *vy += f64::from(next_range!(-10, 11)) * 0.05;
                    *vx += f64::from(next_range!(-10, 11)) * 0.05;
                };
            extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
            if num > 100.0 {
                extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                if num > 150.0 {
                    extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                    if num > 200.0 {
                        extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                        if num > 250.0 {
                            extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                            if num > 300.0 {
                                extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                                if num > 400.0 {
                                    extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                                    if num > 500.0 {
                                        extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                                        if num > 600.0 {
                                            extra(&mut px, &mut py, &mut num2, &mut vx, &mut vy);
                                            if num > 700.0 {
                                                extra(
                                                    &mut px, &mut py, &mut num2, &mut vx, &mut vy,
                                                );
                                                if num > 800.0 {
                                                    extra(
                                                        &mut px, &mut py, &mut num2, &mut vx,
                                                        &mut vy,
                                                    );
                                                    if num > 900.0 {
                                                        extra(
                                                            &mut px, &mut py, &mut num2, &mut vx,
                                                            &mut vy,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        vx += f64::from(next_range!(-10, 11)) * 0.05;
        vx = vx.clamp(-1.0, 1.0);
        if !no_y_change {
            vy += f64::from(next_range!(-10, 11)) * 0.05;
            vy = vy.clamp(-1.0, 1.0);
        } else if tile_type != 59 && num < 3.0 {
            vy = vy.clamp(-1.0, 1.0);
        }
        if tile_type == 59 && !no_y_change {
            vy = vy.clamp(-0.5, 0.5);
        }
    }

    let _ = &mut strength;
    let _ = &mut steps;
    draws
}

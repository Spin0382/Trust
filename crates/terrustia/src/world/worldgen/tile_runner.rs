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
/// `TileRunner`'s `flag4` block (`WorldGen.cs:77833-77886`). Two things about its shape matter:
///
/// - The stone rule is the **initial value**, not an extra test: `flag4 = Main.tileStone[type] &&
///   tile.type != 1`, and the switch that follows can set `flag4` either way. Testing the arms
///   first and only ever escalating to "skip" loses the arms that clear it.
/// - The sand arm is what protects a beach. A runner over an existing sand tile does not paint
///   above `Main.worldSurface` unless it is carrying mud (59), and never paints clay (40) over
///   sand at all.
fn override_skip(
    tile: &terrustia_proto::Tile,
    runner_type: u16,
    column: i32,
    row: i32,
    ctx: RunnerContext,
    rand: &mut UnifiedRandom,
) -> bool {
    let mut skip = tile_stone(runner_type) && tile.block != 1;
    if !can_be_cleared(tile.block) {
        skip = true;
    }
    match tile.block {
        53 => {
            if runner_type == 59 && ctx.underground_desert.contains(column, row) {
                skip = true;
            }
            if runner_type == 40 {
                skip = true;
            }
            if (row as f64) < ctx.world_surface && runner_type != 59 {
                skip = true;
            }
        }
        45 | 147 | 189 | 190 | 196 | 460 | 717 | 718 | 719 => skip = true,
        // `flag4 = !TileID.Sets.Ore[type]`. Sandstone and hardened sand do not exist until the
        // Desert and Oasis passes, and no runner ported so far carries an ore type, so this is
        // indistinguishable from `true` for every pass that reaches it today. Re-read the set
        // before the first ore-carrying runner is registered.
        396 | 397 => skip = true,
        1 => {
            // The draw is inside the test in vanilla too, and `&&` short-circuits it the same way,
            // so a runner that is not mud consumes nothing here.
            if runner_type == 59
                && (row as f64) < ctx.world_surface + f64::from(rand.next_range(-50, 50))
            {
                skip = true;
            }
        }
        // Vanilla's `case 367 / case 368` only ever sets the flag, so the type test is an arm guard.
        367 | 368 if runner_type == 59 => skip = true,
        _ => {}
    }
    skip
}

/// `Main.tileStone`: the gem and a few stone-like tiles whose replacement follows the stone rule.
fn tile_stone(block: u16) -> bool {
    matches!(block, 63 | 64 | 65 | 66 | 67 | 68 | 130 | 131 | 566)
}

/// `TileID.Sand`, the one tile a carving runner is told never to remove.
const SAND: u16 = 53;

/// `Tile.liquidType(num)`, the number the game uses for a liquid.
fn liquid_from_num(num: i32) -> terrustia_proto::Liquid {
    match num {
        1 => terrustia_proto::Liquid::Lava,
        2 => terrustia_proto::Liquid::Honey,
        3 => terrustia_proto::Liquid::Shimmer,
        _ => terrustia_proto::Liquid::Water,
    }
}

/// `System.Drawing.Rectangle` as the generation code uses it: an origin and a size, exclusive on
/// the far edge.
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn contains(&self, column: i32, row: i32) -> bool {
        column >= self.x
            && column < self.x + self.width
            && row >= self.y
            && row < self.y + self.height
    }
}

/// What a [`TileRunner`] call needs from `GenVars` beyond the world and RNG.
#[derive(Clone, Copy)]
pub struct RunnerContext {
    /// `Main.worldSurface`, the highest surface row plus 25 - **not** `GenVars.worldSurface`,
    /// which is the surface of the last column Terrain painted. The two differ by roughly 64 rows
    /// on a small world, and `TileRunner` reads the `Main.` one in its sand and wall branches.
    pub world_surface: f64,
    pub water_line: i32,
    /// `GenVars.lavaLine`, the depth below which a carved hole fills with lava instead of the
    /// column's usual liquid.
    pub lava_line: i32,
    /// `GenVars.UndergroundDesertLocation`, empty until the Desert biome pass fills it in. Only
    /// the mud (59) runner branch reads it, so every pass ported so far sees it empty and the same
    /// either way.
    pub underground_desert: Rect,
}

impl RunnerContext {
    /// The context an ordinary-world pass sees: `Main.worldSurface`, the real water line, and no
    /// underground desert yet. Prefer this over building the struct by hand; picking
    /// `GenVars.worldSurface` here silently relocates every runner in the pass, and the draw count
    /// does not change, so the RNG oracle will not catch it.
    pub fn ordinary(vars: &super::passes::GenVars) -> Self {
        Self {
            world_surface: f64::from(vars.world.surface),
            water_line: vars.water_line,
            lava_line: vars.lava_line,
            underground_desert: Rect::default(),
        }
    }
}

/// `WorldGen.TileRunner` for the ordinary-world path.
///
/// Returns the number of RNG draws taken, so a pass can check its own total against the manifest.
/// A negative `tile_type` carves: `-1` removes the tile, `-2` also leaves liquid behind.
/// `add_tile` false rewrites existing types; true also activates the tile and clears its liquid.
/// The `drunk`/`remix`/`Don't Starve` branches are not modelled, and neither is the 367/368 wall
/// halo, which is called out where it would belong.
#[allow(clippy::too_many_arguments)]
pub fn tile_runner(
    world: &mut World,
    rand: &mut UnifiedRandom,
    ctx: RunnerContext,
    i: i32,
    j: i32,
    strength: f64,
    steps: i32,
    tile_type: i32,
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
    // `flag`: `type >= 0 && !TileID.Sets.SaveSlopes[type]`. A carving runner has no type to look
    // up, so it never clears a slope.
    let clear_slope = tile_type >= 0 && !save_slope(tile_type as u16);
    // `num3`, the liquid a carved hole fills with. Vanilla picks it from the seed flags: the
    // `Next(3)` branch belongs to the Don't Starve world and the `Next(4)` one to a "good" world,
    // and an ordinary seed takes neither, so the value is water while the draw is still consumed.
    let mut liquid_kind = 0i32;
    if next_max!(4) == 0 {
        // `getGoodWorldGen` is false outside the drunk/anniversary/remix combinations, so nothing
        // changes here yet; the draw is the point.
        liquid_kind = 0;
    }

    while num > 0.0 && num2 > 0.0 {
        num = strength * (num2 / f64::from(steps));
        num2 -= 1.0;
        let x0 = ((px - num * 0.5) as i32).max(1);
        let x1 = ((px + num * 0.5) as i32).min(world.width() - 1);
        let y0 = ((py - num * 0.5) as i32).max(1);
        let y1 = ((py + num * 0.5) as i32).min(world.height() - 1);

        for k in x0..x1 {
            // Vanilla resets the liquid kind once per column, before the row loop: a hole dug
            // within `beachDistance + 50` of either edge fills with water whatever the seed picked.
            // Being outside the row loop it fires for every column of the box, not only the ones
            // whose first tile passes the jitter test.
            if k < BEACH_DISTANCE + 50 || k >= world.width() - BEACH_DISTANCE - 50 {
                liquid_kind = 0;
            }
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
                let mut tile = world.tile(k, l);
                if tile_type < 0 {
                    // The carving path (`WorldGen.cs:77795-77820`). A runner never digs through
                    // sand, and `-2` means "leave liquid behind": a hole outside the lava line is
                    // filled to the brim with the column's liquid kind, and anything at or below
                    // the lava line turns to lava. The tile keeps its type - only `active` goes.
                    if tile.is_active() && tile.block == SAND {
                        continue;
                    }
                    if tile_type == -2
                        && tile.is_active()
                        && (l < ctx.water_line || l > ctx.lava_line)
                    {
                        tile.liquid = u8::MAX;
                        tile.liquid_kind = liquid_from_num(liquid_kind);
                        if l > ctx.lava_line {
                            tile.liquid_kind = terrustia_proto::Liquid::Lava;
                        }
                    }
                    tile.flags.set(terrustia_proto::TileFlags::ACTIVE, false);
                    world.set_tile(k, l, tile);
                    continue;
                }
                // Not ported: vanilla's `flag2`/`flag3`, which place wall 180 or 178 in a box
                // around the runner when its type is 368 or 367. Each of those consumes a
                // `Next(-10, 11)` per visited tile, so a pass with either type will diverge here.
                // No pass in the manifest up to Small Holes uses them; `Spider Caves` (367) and the
                // honey paths (368) are where this has to be finished.
                let mut skip = false;
                if over_ride && tile.is_active() {
                    skip = override_skip(&tile, tile_type as u16, k, l, ctx, rand);
                }
                if !skip {
                    tile.block = tile_type as u16;
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

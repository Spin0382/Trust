//! `Jungle`: the mud blob, its gem pockets, the tunnel to the surface, and the finishing touches.
//!
//! Transcribed from `JunglePass.ApplyPass` and its helpers
//! (`.scratch/decompiled/Terraria.GameContent.Biomes/JunglePass.cs:18-215`). This is the largest
//! single pass in the manifest: it draws more than 40 million numbers on the reference world, most
//! of them from the per-tile jitter inside `MudWallRunner` and the finishing-touches loops.
//!
//! The shape is three mud blobs walked out from the jungle's origin, each with its own gem pocket,
//! then one big `TileRunner` (type 59, strength 400..600 scaled) with `GenVars.mudWall` on, then
//! a worm from that blob up to the first open sky. The worm is `KillTile`-based rather than a
//! runner, which is why `tile_runner::kill_tile` exists; it removes tiles without drawing, but it
//! does consult the tile above it so a tree is not left floating over a hole.
//!
//! Two orderings are load-bearing and easy to "clean up" into a divergence:
//!
//! - `zero` accumulates the three blob centres and is divided by three *after* the third blob, so
//!   the big runner starts at their mean, not at the last blob.
//! - `GenerateHolesInMudWalls` picks a random point and redraws until it lands on a wall 15 or 64.
//!   That loop is unbounded in vanilla too, and it only terminates because the big runner has just
//!   painted those walls.
//!
//! `GenVars.JungleX` is written every iteration of the tunnel walk and read by later jungle passes;
//! it is kept on `GenVars` for that reason even though nothing here reads it back.

use super::passes::{DungeonSide, GenVars};
use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, kill_tile, tile_runner};
use crate::world::World;

/// `TileID.Mud`.
const MUD: u16 = 59;
/// `TileID.Stone`, the third finishing-touches loop's type.
const STONE: u16 = 1;
/// The wall types `GenerateHolesInMudWalls` looks for: `Main.wallMud` and `Main.wallJungle`.
const MUD_WALL: u16 = 15;
const JUNGLE_WALL: u16 = 64;

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

    fn add(&mut self, draws: usize) {
        self.count += draws;
    }
}

/// `JunglePass.ApplyRandomMovement` (`JunglePass.cs:104`).
#[allow(clippy::too_many_arguments)]
fn apply_random_movement(
    rng: &mut Draws<'_>,
    x: &mut i32,
    y: &mut i32,
    x_range: i32,
    y_range: i32,
    world_scale: f64,
    rock_layer: f64,
    height: i32,
) {
    *x += rng.next_range(
        ((-f64::from(x_range)) * world_scale) as i32,
        1 + (f64::from(x_range) * world_scale) as i32,
    );
    *y += rng.next_range(
        ((-f64::from(y_range)) * world_scale) as i32,
        1 + (f64::from(y_range) * world_scale) as i32,
    );
    *y = (*y).clamp(rock_layer as i32, height);
}

/// `JunglePass.PlaceGemsAt` (`JunglePass.cs:78`).
#[allow(clippy::too_many_arguments)]
fn place_gems_at(
    world: &mut World,
    rng: &mut Draws<'_>,
    ctx: RunnerContext,
    x: i32,
    y: i32,
    base_gem: u16,
    gem_variants: i32,
    world_scale: f64,
) {
    let mut i = 0i32;
    while f64::from(i) < 6.0 * world_scale {
        i += 1;
        let dx = rng.next_range(
            (-(125.0 * world_scale)) as i32,
            (125.0 * world_scale) as i32,
        );
        let dy = rng.next_range(
            (-(125.0 * world_scale)) as i32,
            (125.0 * world_scale) as i32,
        );
        let strength = rng.next_range(3, 7);
        let steps = rng.next_range(3, 8);
        let gem = rng.next_range(i32::from(base_gem), i32::from(base_gem) + gem_variants);
        let drawn = tile_runner(
            world,
            rng.rand,
            ctx,
            x + dx,
            y + dy,
            f64::from(strength),
            steps,
            gem,
            false,
            0.0,
            0.0,
            false,
            true,
            -1,
        );
        rng.add(drawn);
    }
}

/// `JunglePass.PlaceFirstPassMud` (`JunglePass.cs:89`).
#[allow(clippy::too_many_arguments)]
fn place_first_pass_mud(
    world: &mut World,
    rng: &mut Draws<'_>,
    ctx: RunnerContext,
    x: i32,
    y: i32,
    x_speed_scale: i32,
    world_scale: f64,
    dungeon_side: i32,
) {
    let strength = rng.next_range((250.0 * world_scale) as i32, (500.0 * world_scale) as i32);
    let steps = rng.next_range(50, 150);
    let drawn = tile_runner(
        world,
        rng.rand,
        ctx.with_mud_wall(),
        x,
        y,
        f64::from(strength),
        steps,
        i32::from(MUD),
        false,
        f64::from(dungeon_side * x_speed_scale),
        0.0,
        false,
        true,
        -1,
    );
    rng.add(drawn);
}

/// `JunglePass.GenerateTunnelToSurface` (`JunglePass.cs:122`).
#[allow(clippy::too_many_arguments)]
fn generate_tunnel_to_surface(
    world: &mut World,
    rng: &mut Draws<'_>,
    ctx: RunnerContext,
    i: i32,
    j: i32,
    world_surface: f64,
    height: i32,
    jungle_x: &mut i32,
) {
    let mut num = f64::from(rng.next_range(5, 11));
    let mut val_x = f64::from(i);
    let mut val_y = f64::from(j);
    let mut vx = f64::from(rng.next_range(-10, 11)) * 0.1;
    let mut vy = f64::from(rng.next_range(10, 20)) * 0.1;
    let mut steps_since_hole = 0i32;
    let mut walking = true;
    while walking {
        if val_y < world_surface {
            let mut value = val_x as i32;
            let mut value2 = val_y as i32;
            value = value.clamp(10, world.width() - 10);
            value2 = value2.clamp(10, height - 10);
            if value2 < 5 {
                value2 = 5;
            }
            if world.tile(value, value2).wall == 0
                && !world.tile(value, value2).is_active()
                && world.tile(value, value2 - 3).wall == 0
                && !world.tile(value, value2 - 3).is_active()
                && world.tile(value, value2 - 1).wall == 0
                && !world.tile(value, value2 - 1).is_active()
                && world.tile(value, value2 - 4).wall == 0
                && !world.tile(value, value2 - 4).is_active()
                && world.tile(value, value2 - 2).wall == 0
                && !world.tile(value, value2 - 2).is_active()
                && world.tile(value, value2 - 5).wall == 0
                && !world.tile(value, value2 - 5).is_active()
            {
                walking = false;
            }
        }
        *jungle_x = val_x as i32;
        num += f64::from(rng.next_range(-20, 21)) * 0.1;
        num = num.clamp(5.0, 10.0);
        let value3 = (val_x - num * 0.5) as i32;
        let mut value4 = (val_x + num * 0.5) as i32;
        let mut value5 = (val_y - num * 0.5) as i32;
        let mut value6 = (val_y + num * 0.5) as i32;
        let num3 = value3.clamp(10, world.width() - 10);
        value4 = value4.clamp(10, world.width() - 10);
        value5 = value5.clamp(10, height - 10);
        value6 = value6.clamp(10, height - 10);
        for k in num3..value4 {
            for l in value5..value6 {
                let radius = num * 0.5 * (1.0 + f64::from(rng.next_range(-10, 11)) * 0.015);
                if (f64::from(k) - val_x).abs() + (f64::from(l) - val_y).abs() < radius {
                    kill_tile(world, k, l);
                }
            }
        }
        steps_since_hole += 1;
        if steps_since_hole > 10 && rng.next_max(50) < steps_since_hole {
            steps_since_hole = 0;
            let mut num4 = -2;
            if rng.next_max(2) == 0 {
                num4 = 2;
            }
            let strength = rng.next_range(3, 20);
            let steps = rng.next_range(10, 100);
            let drawn = tile_runner(
                world,
                rng.rand,
                ctx,
                val_x as i32,
                val_y as i32,
                f64::from(strength),
                steps,
                -1,
                false,
                f64::from(num4),
                0.0,
                false,
                true,
                -1,
            );
            rng.add(drawn);
        }
        val_x += vx;
        val_y += vy;
        vy += f64::from(rng.next_range(-10, 11)) * 0.01;
        vy = vy.clamp(-2.0, 0.0);
        vx += f64::from(rng.next_range(-10, 11)) * 0.1;
        if val_x < f64::from(i - 200) {
            vx += f64::from(rng.next_range(5, 21)) * 0.1;
        }
        if val_x > f64::from(i + 200) {
            vx -= f64::from(rng.next_range(5, 21)) * 0.1;
        }
        vx = vx.clamp(-1.5, 1.5);
    }
}

/// `WorldGen.MudWallRunner` (`WorldGen.cs:78132`), the wall-only worm
/// `GenerateHolesInMudWalls` uses.
fn mud_wall_runner(world: &mut World, rng: &mut Draws<'_>, i: i32, j: i32, world_surface: f64) {
    let num = f64::from(rng.next_range(8, 21));
    let num2 = f64::from(rng.next_range(8, 33));
    let mut num3 = num2;
    let mut val_x = f64::from(i);
    let mut val_y = f64::from(j);
    let mut vx = f64::from(rng.next_range(-10, 11)) * 0.1;
    let mut vy = f64::from(rng.next_range(-10, 11)) * 0.1;
    while num > 0.0 && num3 > 0.0 {
        let num4 = num * (num3 / num2);
        num3 -= 1.0;
        let mut num5 = (val_x - num4 * 0.5) as i32;
        let mut num6 = (val_x + num4 * 0.5) as i32;
        let mut num7 = (val_y - num4 * 0.5) as i32;
        let mut num8 = (val_y + num4 * 0.5) as i32;
        num5 = num5.max(0);
        num6 = num6.min(world.width());
        num7 = num7.max(0);
        num8 = num8.min(world.height());
        for k in num5..num6 {
            for l in num7..num8 {
                let radius = num * 0.5 * (1.0 + f64::from(rng.next_range(-10, 11)) * 0.015);
                if (f64::from(k) - val_x).abs() + (f64::from(l) - val_y).abs() < radius
                    && f64::from(l) > world_surface
                {
                    let mut tile = world.tile(k, l);
                    tile.wall = 0;
                    world.set_tile(k, l, tile);
                }
            }
        }
        val_x += vx;
        val_y += vy;
        vx += f64::from(rng.next_range(-10, 11)) * 0.05;
        vx = vx.clamp(-1.0, 1.0);
        vy += f64::from(rng.next_range(-10, 11)) * 0.05;
        vy = vy.clamp(-1.0, 1.0);
    }
}

/// `JunglePass.GenerateHolesInMudWalls` (`JunglePass.cs:198`).
fn generate_holes_in_mud_walls(
    world: &mut World,
    rng: &mut Draws<'_>,
    gen_world_surface: f64,
    main_world_surface: f64,
    height: i32,
) {
    let underworld_layer = height - 200;
    for _ in 0..world.width() / 4 {
        let mut x = rng.next_range(20, world.width() - 20);
        let mut y = rng.next_range(gen_world_surface as i32 + 10, underworld_layer);
        while world.tile(x, y).wall != JUNGLE_WALL && world.tile(x, y).wall != MUD_WALL {
            x = rng.next_range(20, world.width() - 20);
            y = rng.next_range(gen_world_surface as i32 + 10, underworld_layer);
        }
        // The search box uses GenVars.worldSurface, but MudWallRunner itself tests
        // Main.worldSurface (WorldGen.cs:78172). The two differ by about 64 rows, and using the
        // wrong one silently erases the surface dirt wall over the whole jungle.
        mud_wall_runner(world, rng, x, y, main_world_surface);
    }
}

/// `JunglePass.GenerateFinishingTouches` (`JunglePass.cs:214`).
fn generate_finishing_touches(
    world: &mut World,
    rng: &mut Draws<'_>,
    ctx: RunnerContext,
    old_x: i32,
    old_y: i32,
    world_scale: f64,
) {
    let mut num = old_x;
    let mut num2 = old_y;

    let mut i = 0i32;
    while f64::from(i) <= 20.0 * world_scale {
        i += 1;
        num += rng.next_range((-5.0 * world_scale) as i32, (6.0 * world_scale) as i32);
        num2 += rng.next_range((-5.0 * world_scale) as i32, (6.0 * world_scale) as i32);
        let strength = rng.next_range(40, 100);
        let steps = rng.next_range(300, 500);
        let drawn = tile_runner(
            world,
            rng.rand,
            ctx,
            num,
            num2,
            f64::from(strength),
            steps,
            i32::from(MUD),
            false,
            0.0,
            0.0,
            false,
            true,
            -1,
        );
        rng.add(drawn);
    }

    let mut j = 0i32;
    while f64::from(j) <= 10.0 * world_scale {
        j += 1;
        num = old_x + rng.next_range((-600.0 * world_scale) as i32, (600.0 * world_scale) as i32);
        num2 = old_y + rng.next_range((-200.0 * world_scale) as i32, (200.0 * world_scale) as i32);
        while num < 1
            || num >= world.width() - 1
            || num2 < 1
            || num2 >= world.height() - 1
            || world.tile(num, num2).block != MUD
        {
            num =
                old_x + rng.next_range((-600.0 * world_scale) as i32, (600.0 * world_scale) as i32);
            num2 =
                old_y + rng.next_range((-200.0 * world_scale) as i32, (200.0 * world_scale) as i32);
        }
        let mut k = 0i32;
        while f64::from(k) < 8.0 * world_scale {
            k += 1;
            num += rng.next_range(-30, 31);
            num2 += rng.next_range(-30, 31);
            let mut tile_type = -1;
            if rng.next_max(7) == 0 {
                tile_type = -2;
            }
            let strength = rng.next_range(10, 20);
            let steps = rng.next_range(30, 70);
            let drawn = tile_runner(
                world,
                rng.rand,
                ctx,
                num,
                num2,
                f64::from(strength),
                steps,
                tile_type,
                false,
                0.0,
                0.0,
                false,
                true,
                -1,
            );
            rng.add(drawn);
        }
    }

    let mut l = 0i32;
    while f64::from(l) <= 300.0 * world_scale {
        l += 1;
        num = old_x + rng.next_range((-600.0 * world_scale) as i32, (600.0 * world_scale) as i32);
        num2 = old_y + rng.next_range((-200.0 * world_scale) as i32, (200.0 * world_scale) as i32);
        while num < 1
            || num >= world.width() - 1
            || num2 < 1
            || num2 >= world.height() - 1
            || world.tile(num, num2).block != MUD
        {
            num =
                old_x + rng.next_range((-600.0 * world_scale) as i32, (600.0 * world_scale) as i32);
            num2 =
                old_y + rng.next_range((-200.0 * world_scale) as i32, (200.0 * world_scale) as i32);
        }
        let strength = rng.next_range(4, 10);
        let steps = rng.next_range(5, 30);
        let drawn = tile_runner(
            world,
            rng.rand,
            ctx,
            num,
            num2,
            f64::from(strength),
            steps,
            i32::from(STONE),
            false,
            0.0,
            0.0,
            false,
            true,
            -1,
        );
        rng.add(drawn);
        if rng.next_max(4) == 0 {
            let gem = rng.next_range(63, 69);
            let dx = rng.next_range(-1, 2);
            let dy = rng.next_range(-1, 2);
            let strength = rng.next_range(3, 7);
            let steps = rng.next_range(4, 8);
            let drawn = tile_runner(
                world,
                rng.rand,
                ctx,
                num + dx,
                num2 + dy,
                f64::from(strength),
                steps,
                gem,
                false,
                0.0,
                0.0,
                false,
                true,
                -1,
            );
            rng.add(drawn);
        }
    }
}

/// Run the pass, returning how many draws it took.
pub fn run_counted(vars: &mut GenVars) -> usize {
    let width = vars.world.width();
    let height = vars.world.height();
    let world_surface = f64::from(vars.world.surface);
    let rock_layer = f64::from(vars.world.rock_layer);
    let gen_world_surface = vars.world_surface;
    let left_beach_end = vars.left_beach_end;
    let right_beach_start = vars.right_beach_start;
    let jungle_origin_x = vars.jungle_origin_x;
    let dungeon_side = match vars.dungeon_side {
        DungeonSide::Left => -1,
        DungeonSide::Right => 1,
    };
    let ctx = RunnerContext::ordinary(vars);
    let world_scale = f64::from(width) / 4200.0 * 1.5;

    let GenVars { rand, world, .. } = vars;
    let mut rng = Draws::new(rand);

    let mut x = jungle_origin_x;
    let mut y = ((f64::from(height) + rock_layer) / 2.0) as i32;
    let mut zero_x = 0i32;
    let mut zero_y = 0i32;

    apply_random_movement(
        &mut rng,
        &mut x,
        &mut y,
        100,
        100,
        world_scale,
        rock_layer,
        height,
    );
    zero_x += x;
    zero_y += y;
    place_first_pass_mud(world, &mut rng, ctx, x, y, 3, world_scale, dungeon_side);
    place_gems_at(world, &mut rng, ctx, x, y, 63, 2, world_scale);

    apply_random_movement(
        &mut rng,
        &mut x,
        &mut y,
        250,
        150,
        world_scale,
        rock_layer,
        height,
    );
    zero_x += x;
    zero_y += y;
    place_first_pass_mud(world, &mut rng, ctx, x, y, 0, world_scale, dungeon_side);
    place_gems_at(world, &mut rng, ctx, x, y, 65, 2, world_scale);

    let old_x = x;
    let old_y = y;
    apply_random_movement(
        &mut rng,
        &mut x,
        &mut y,
        400,
        150,
        world_scale,
        rock_layer,
        height,
    );
    zero_x += x;
    zero_y += y;
    place_first_pass_mud(world, &mut rng, ctx, x, y, -3, world_scale, dungeon_side);
    place_gems_at(world, &mut rng, ctx, x, y, 67, 2, world_scale);

    x = zero_x / 3;
    y = zero_y / 3;
    let num = rng.next_range((400.0 * world_scale) as i32, (600.0 * world_scale) as i32);
    let num2 = (25.0 * world_scale) as i32;
    x = x.clamp(
        left_beach_end + num / 2 + num2,
        right_beach_start - num / 2 - num2,
    );

    let drawn = tile_runner(
        world,
        rng.rand,
        ctx.with_mud_wall(),
        x,
        y,
        f64::from(num),
        10000,
        i32::from(MUD),
        false,
        0.0,
        -20.0,
        true,
        true,
        -1,
    );
    rng.add(drawn);

    // GenVars.mudWall is still true for the whole tunnel: JunglePass.cs:53-59 sets it before
    // the big runner, calls GenerateTunnelToSurface, and only then clears it. The tunnel's own
    // carving runners therefore take the mud-wall branch too, which is three draws per accepted
    // deep tile and the walls 15/64 those draws place.
    let mut jungle_x = 0i32;
    generate_tunnel_to_surface(
        world,
        &mut rng,
        ctx.with_mud_wall(),
        x,
        y,
        world_surface,
        height,
        &mut jungle_x,
    );

    generate_holes_in_mud_walls(world, &mut rng, gen_world_surface, world_surface, height);
    generate_finishing_touches(world, &mut rng, ctx, old_x, old_y, world_scale);

    let draws = rng.count;
    vars.jungle_x = jungle_x;
    draws
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

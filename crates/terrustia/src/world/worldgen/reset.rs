//! The state `WorldGen.Reset` hands to the first pass, transcribed for an ordinary world.
//!
//! Reset runs before any pass and fills in every parameter the passes share: the beach ends, the
//! dungeon's side and column, the jungle and snow origins, the ore tiers, the scenery styles.
//! Terrain reads two of them (the beach ends) and the rest are for later passes, but all of them
//! come out of the same draw stream, so a missing draw here moves every later value even though
//! the pass oracle cannot see it: `WorldGenerator.RunPass` reseeds the generator before every
//! pass (`WorldGenerator.cs:499`), so Reset's draws are invisible to `RandNext` and wrong reset
//! values only show up as wrong tiles.
//!
//! Transcribed from:
//!
//! - `WorldGen.Reset`, `WorldGen.cs:11136-11515`
//! - `WorldGen.RandomizeWeather`, `WorldGen.cs:7720-7730`
//! - `WorldGen.RandomizeMoonState`, `WorldGen.cs:7732-7741`
//! - `WorldGen.RandomizeBackgrounds`, `WorldGen.cs:7897-7960`
//! - `RandomizeBackgrounds_Corruption` / `_Desert` / `RollRandomForestBGStyle`,
//!   `WorldGen.cs:7962-7994`
//! - `WorldGen.RandomizeTreeStyle`, `WorldGen.cs:8066-8139`
//! - `WorldGen.RandomizeCaveBackgrounds`, `WorldGen.cs:8141-8209`
//!
//! Secret seeds, Remix, Celebrationmk10 and Skyblock are deliberately not transcribed. Every
//! branch written around one of those flags is omitted here rather than left as a stub, because
//! this file only claims to model the ordinary generator; the day a secret seed needs support,
//! its branch goes back where the source has it. The same goes for the anniversary world's wider
//! jungle-origin roll and the drunk world's dungeon-side swaps.

use super::passes::{DungeonSide, GenVars};
use super::rand::UnifiedRandom;
use crate::world::World;

/// Run Reset's draws and store what they produced.
///
/// `evil` is vanilla's `WorldGenParam_Evil`: `Some(true)` is a crimson world, `Some(false)` a
/// corruption world, and `None` lets the seed's own roll decide. The roll is taken either way,
/// exactly as `WorldGen.cs:11333-11344` does before applying the override.
pub fn reset(vars: &mut GenVars, evil: Option<bool>) {
    let width = vars.world.width();
    // Reset begins by pointing the generator at the world's seed (`WorldGen.cs:11171`).
    let mut rand = UnifiedRandom::new(vars.seed);

    // `GenVars.jungleHut = (ushort)genRand.Next(5)` (`WorldGen.cs:11194`), then the five-way
    // material mapping at `WorldGen.cs:11345-11365`.
    vars.jungle_hut = match rand.next_max(5) {
        0 => 119,
        1 => 120,
        2 => 158,
        3 => 175,
        _ => 45,
    };

    // `WorldGen.cs:11243-11250`. The roll is taken even when the world's evil is fixed.
    vars.crimson_left = rand.next_max(2) != 0;

    randomize_weather(&mut rand, vars);

    // `WorldGen.cs:11277-11286`: the five underworld chest items are shuffled by taking one at a
    // time out of a shrinking list.
    let mut chest_items = vec![274, 220, 112, 218, 3019];
    for slot in &mut vars.hell_chest_item {
        let index = rand.next_range(0, chest_items.len() as i32) as usize;
        *slot = chest_items.remove(index);
    }

    // Weather counters with no home in `World`, drawn because they are part of the stream.
    let _slime_rain_time = -rand.next_range(86_400 * 2, 86_400 * 3);
    let _cloud_bg_active = -rand.next_range(8_640, 86_400);

    // `WorldGen.cs:11294-11332`: each ore tier independently rolls its alternate, and the bar
    // form rolls with it.
    vars.copper = 7;
    vars.iron = 6;
    vars.silver = 9;
    vars.gold = 8;
    vars.copper_bar = 20;
    vars.iron_bar = 22;
    vars.silver_bar = 21;
    vars.gold_bar = 19;
    if rand.next_max(2) == 0 {
        vars.copper = 166;
        vars.copper_bar = 703;
    }
    if rand.next_max(2) == 0 {
        vars.iron = 167;
        vars.iron_bar = 704;
    }
    if rand.next_max(2) == 0 {
        vars.silver = 168;
        vars.silver_bar = 705;
    }
    if rand.next_max(2) == 0 {
        vars.gold = 169;
        vars.gold_bar = 706;
    }

    // `WorldGen.cs:11333-11344`: the seed's own evil, then the creation parameter's override.
    let rolled_crimson = rand.next_max(2) == 0;
    vars.world.crimson = evil.unwrap_or(rolled_crimson);
    vars.world.ore_tiers[0] = vars.copper as i16;
    vars.world.ore_tiers[1] = vars.iron as i16;
    vars.world.ore_tiers[2] = vars.silver as i16;
    vars.world.ore_tiers[3] = vars.gold as i16;

    // `WorldGen.cs:11365`: the world id.
    vars.world.id = rand.next_max(i32::MAX);

    randomize_tree_style(&mut rand, width, &mut vars.world);
    randomize_cave_backgrounds(&mut rand, width, &mut vars.world);
    randomize_backgrounds(&mut rand, &mut vars.world);

    // `WorldGen.cs:11369`: the moon phase style is one draw, because `guaranteeNewStyle` is false
    // when Reset calls it. It comes after the backgrounds, and that order is load-bearing.
    vars.world.moon_type = rand.next_max(9) as u8;

    // `WorldGen.cs:11372`: the first dungeon's side.
    vars.dungeon_side = if rand.next_max(2) == 0 {
        DungeonSide::Left
    } else {
        DungeonSide::Right
    };

    // `WorldGen.cs:11373-11404`: the jungle sits opposite the dungeon. The anniversary seed's
    // wider 25..35 roll is not modelled (see the module doc).
    let percent = rand.next_range(15, 30) as f64 * 0.01;
    vars.jungle_origin_x = if vars.dungeon_side == DungeonSide::Left {
        (width as f64 * (1.0 - percent)) as i32
    } else {
        (width as f64 * percent) as i32
    };

    // `WorldGen.cs:11406-11430`: a dungeon column inside its side's quarter, resampled until it
    // lands there.
    let mut dungeon_x = rand.next_max(width);
    if vars.dungeon_side == DungeonSide::Right {
        while (dungeon_x as f64) < width as f64 * 0.6 || (dungeon_x as f64) > width as f64 * 0.75 {
            dungeon_x = rand.next_max(width);
        }
    } else {
        while (dungeon_x as f64) < width as f64 * 0.25 || (dungeon_x as f64) > width as f64 * 0.4 {
            dungeon_x = rand.next_max(width);
        }
    }

    // `WorldGen.cs:11431-11446`: the snow biome straddles that column, half-width rolled twice.
    let scale = width as f64 / 4200.0;
    let mut half = rand.next_range(50, 90);
    half += (rand.next_range(20, 40) as f64 * scale) as i32;
    half += (rand.next_range(20, 40) as f64 * scale) as i32;
    let snow_left = dungeon_x - half;
    half = rand.next_range(50, 90);
    half += (rand.next_range(20, 40) as f64 * scale) as i32;
    half += (rand.next_range(20, 40) as f64 * scale) as i32;
    let snow_right = dungeon_x + half;
    vars.snow_origin_left = snow_left.max(0);
    vars.snow_origin_right = snow_right.min(width);

    // `WorldGen.cs:11448-11482`: the beach ends, then the dungeon's column. The dungeon's side
    // decides which beach gets the wider dungeon padding.
    vars.left_beach_end = rand.next_range(320 - 20, 320 + 20)
        + if vars.dungeon_side == DungeonSide::Right {
            40
        } else {
            20
        };
    vars.right_beach_start = width
        - rand.next_range(320 - 20, 320 + 20)
        - if vars.dungeon_side == DungeonSide::Left {
            40
        } else {
            20
        };
    vars.dungeon_location = if vars.dungeon_side == DungeonSide::Left {
        rand.next_range(vars.left_beach_end + 50, (width as f64 * 0.2) as i32)
    } else {
        rand.next_range((width as f64 * 0.8) as i32, vars.right_beach_start - 50)
    };

    // `WorldGen.cs:11209-11233` and `11266`/`11505-11511`: the constants and counters the passes
    // read.
    vars.sky_lakes = 1;
    if width > 8000 {
        vars.sky_lakes += 1;
    }
    if width > 6000 {
        vars.sky_lakes += 1;
    }
    vars.beach_borders_width = 275;
    vars.beach_sand_random_center = 275 + 5 + 40;
    vars.beach_sand_random_width_range = 20;
    vars.beach_sand_dungeon_extra_width = 40;
    vars.beach_sand_jungle_extra_width = 20;
    vars.ocean_water_start_random_min = 220;
    vars.ocean_water_start_random_max = 260;
    vars.ocean_water_forced_jungle_length = 275;
    vars.evil_biome_beach_avoidance = vars.beach_sand_random_center + 60;
    vars.evil_biome_avoidance_mid_fixer = 50;
    vars.lakes_beach_avoidance = vars.beach_sand_random_center + 20;
    vars.small_holes_beach_avoidance = vars.beach_sand_random_center + 20;
    vars.surface_caves_beach_avoidance = vars.beach_sand_random_center + 20;
    vars.surface_caves_beach_avoidance2 = vars.beach_sand_random_center + 20;

    // Reset leaves the generator where its last draw put it; nothing reads it before the next
    // pass reseeds, but keeping the state makes the stream checkable on its own.
    vars.rand = rand;
}

/// `WorldGen.RandomizeWeather`, `WorldGen.cs:7720-7730`.
fn randomize_weather(rand: &mut UnifiedRandom, vars: &mut GenVars) {
    vars.world.num_clouds = rand.next_range(10, 200) as u8;
    let mut wind = 0.0f32;
    while wind == 0.0 {
        wind = rand.next_float() * 0.35 * (rand.next_max(2) * 2 - 1) as f32;
    }
    vars.world.wind = wind;
}

/// `WorldGen.RandomizeTreeStyle`, `WorldGen.cs:8066-8139`.
///
/// Only the drawing and the assignments are reproduced; the `Main.treeMntBGSet*` texture sets
/// `setBG` derives from a style are render state, not world state.
fn randomize_tree_style(rand: &mut UnifiedRandom, width: i32, world: &mut World) {
    let width_f = width as f64;
    if width == 4200 {
        world.tree_x[0] = rand.next_range(
            (width_f * 0.5 - width_f * 0.25) as i32,
            (width_f * 0.5 + width_f * 0.25) as i32,
        );
        world.tree_style[0] = rand.next_max(6) as u8;
        world.tree_style[1] = rand.next_max(6) as u8;
        while world.tree_style[1] == world.tree_style[0] {
            world.tree_style[1] = rand.next_max(6) as u8;
        }
        world.tree_x[1] = width;
        world.tree_x[2] = width;
        for style in &mut world.tree_style[..2] {
            // The draw is inside the `style == 0` branch, so a non-zero style consumes nothing.
            if *style == 0 && rand.next_max(3) != 0 {
                *style = 4;
            }
        }
        return;
    }
    if width == 6400 {
        world.tree_x[0] = rand.next_range(
            (width_f * 0.334 - width_f * 0.2) as i32,
            (width_f * 0.334 + width_f * 0.2) as i32,
        );
        world.tree_x[1] = rand.next_range(
            (width_f * 0.667 - width_f * 0.2) as i32,
            (width_f * 0.667 + width_f * 0.2) as i32,
        );
        world.tree_style[0] = rand.next_max(6) as u8;
        world.tree_style[1] = rand.next_max(6) as u8;
        world.tree_style[2] = rand.next_max(6) as u8;
        while world.tree_style[1] == world.tree_style[0] {
            world.tree_style[1] = rand.next_max(6) as u8;
        }
        while world.tree_style[2] == world.tree_style[0]
            || world.tree_style[2] == world.tree_style[1]
        {
            world.tree_style[2] = rand.next_max(6) as u8;
        }
        world.tree_x[2] = width;
        for style in &mut world.tree_style[..3] {
            if *style == 0 && rand.next_max(3) != 0 {
                *style = 4;
            }
        }
        return;
    }
    world.tree_x[0] = rand.next_range(
        (width_f * 0.25 - width_f * 0.15) as i32,
        (width_f * 0.25 + width_f * 0.15) as i32,
    );
    world.tree_x[1] = rand.next_range(
        (width_f * 0.5 - width_f * 0.15) as i32,
        (width_f * 0.5 + width_f * 0.15) as i32,
    );
    world.tree_x[2] = rand.next_range(
        (width_f * 0.75 - width_f * 0.15) as i32,
        (width_f * 0.75 + width_f * 0.15) as i32,
    );
    for slot in &mut world.tree_style {
        *slot = rand.next_max(6) as u8;
    }
    while world.tree_style[1] == world.tree_style[0] {
        world.tree_style[1] = rand.next_max(6) as u8;
    }
    while world.tree_style[2] == world.tree_style[0] || world.tree_style[2] == world.tree_style[1] {
        world.tree_style[2] = rand.next_max(6) as u8;
    }
    while world.tree_style[3] == world.tree_style[0]
        || world.tree_style[3] == world.tree_style[1]
        || world.tree_style[3] == world.tree_style[2]
    {
        world.tree_style[3] = rand.next_max(6) as u8;
    }
    for style in &mut world.tree_style {
        if *style == 0 && rand.next_max(3) != 0 {
            *style = 4;
        }
    }
}

/// `WorldGen.RandomizeCaveBackgrounds`, `WorldGen.cs:8141-8209`.
fn randomize_cave_backgrounds(rand: &mut UnifiedRandom, width: i32, world: &mut World) {
    let width_f = width as f64;
    let max_style = 8;
    if width == 4200 {
        world.cave_back_x[0] = rand.next_range(
            (width_f * 0.5 - width_f * 0.25) as i32,
            (width_f * 0.5 + width_f * 0.25) as i32,
        );
        world.cave_back_x[1] = width;
        world.cave_back_x[2] = width;
        world.cave_back_style[0] = rand.next_max(max_style) as u8;
        world.cave_back_style[1] = rand.next_max(max_style) as u8;
        while world.cave_back_style[1] == world.cave_back_style[0] {
            world.cave_back_style[1] = rand.next_max(max_style) as u8;
        }
    } else if width == 6400 {
        world.cave_back_x[0] = rand.next_range(
            (width_f * 0.334 - width_f * 0.2) as i32,
            (width_f * 0.334 + width_f * 0.2) as i32,
        );
        world.cave_back_x[1] = rand.next_range(
            (width_f * 0.667 - width_f * 0.2) as i32,
            (width_f * 0.667 + width_f * 0.2) as i32,
        );
        world.cave_back_x[2] = width;
        world.cave_back_style[0] = rand.next_max(max_style) as u8;
        world.cave_back_style[1] = rand.next_max(max_style) as u8;
        world.cave_back_style[2] = rand.next_max(max_style) as u8;
        while world.cave_back_style[1] == world.cave_back_style[0] {
            world.cave_back_style[1] = rand.next_max(max_style) as u8;
        }
        while world.cave_back_style[2] == world.cave_back_style[0]
            || world.cave_back_style[2] == world.cave_back_style[1]
        {
            world.cave_back_style[2] = rand.next_max(max_style) as u8;
        }
    } else {
        world.cave_back_x[0] = rand.next_range(
            (width_f * 0.25 - width_f * 0.15) as i32,
            (width_f * 0.25 + width_f * 0.15) as i32,
        );
        world.cave_back_x[1] = rand.next_range(
            (width_f * 0.5 - width_f * 0.15) as i32,
            (width_f * 0.5 + width_f * 0.15) as i32,
        );
        world.cave_back_x[2] = rand.next_range(
            (width_f * 0.75 - width_f * 0.15) as i32,
            (width_f * 0.75 + width_f * 0.15) as i32,
        );
        for slot in &mut world.cave_back_style {
            *slot = rand.next_max(max_style) as u8;
        }
        while world.cave_back_style[1] == world.cave_back_style[0] {
            world.cave_back_style[1] = rand.next_max(max_style) as u8;
        }
        while world.cave_back_style[2] == world.cave_back_style[0]
            || world.cave_back_style[2] == world.cave_back_style[1]
        {
            world.cave_back_style[2] = rand.next_max(max_style) as u8;
        }
        while world.cave_back_style[3] == world.cave_back_style[0]
            || world.cave_back_style[3] == world.cave_back_style[1]
            || world.cave_back_style[3] == world.cave_back_style[2]
        {
            world.cave_back_style[3] = rand.next_max(max_style) as u8;
        }
    }
    // These three are at the end of the method and their order is load-bearing for every later
    // Reset draw: ice, hell, jungle, in that order (`WorldGen.cs:8205-8207`).
    world.ice_back_style = rand.next_max(4) as u8;
    world.hell_back_style = rand.next_max(3) as u8;
    world.jungle_back_style = rand.next_max(2) as u8;
}

/// `WorldGen.RandomizeBackgrounds`, `WorldGen.cs:7897-7960`.
fn randomize_backgrounds(rand: &mut UnifiedRandom, world: &mut World) {
    // The slots are packet 7's order (see `wld::Scenery`), not `setBG`'s numbers.
    world.backgrounds[0] = roll_random_forest_bg_style(rand);
    world.backgrounds[1] = roll_random_forest_bg_style(rand);
    while world.backgrounds[1] == world.backgrounds[0] {
        world.backgrounds[1] = roll_random_forest_bg_style(rand);
    }
    world.backgrounds[2] = roll_random_forest_bg_style(rand);
    while world.backgrounds[2] == world.backgrounds[0]
        || world.backgrounds[2] == world.backgrounds[1]
    {
        world.backgrounds[2] = roll_random_forest_bg_style(rand);
    }
    world.backgrounds[3] = roll_random_forest_bg_style(rand);
    while world.backgrounds[3] == world.backgrounds[0]
        || world.backgrounds[3] == world.backgrounds[1]
        || world.backgrounds[3] == world.backgrounds[2]
    {
        world.backgrounds[3] = roll_random_forest_bg_style(rand);
    }

    world.backgrounds[4] = randomize_backgrounds_corruption(rand);
    world.backgrounds[5] = rand.next_max(7) as u8;

    let mut snow = rand.next_max(9);
    if snow == 2 && rand.next_max(2) == 0 {
        snow = if rand.next_max(2) == 0 { 21 } else { 22 };
    }
    if snow == 3 && rand.next_max(2) == 0 {
        snow = if rand.next_max(2) == 0 { 31 } else { 32 };
    }
    if snow == 4 && rand.next_max(2) == 0 {
        snow = if rand.next_max(2) == 0 { 41 } else { 42 };
    }
    world.backgrounds[6] = snow as u8;

    world.backgrounds[7] = rand.next_max(6) as u8; // hallow
    world.backgrounds[8] = rand.next_max(7) as u8; // crimson
    world.backgrounds[9] = randomize_backgrounds_desert(rand);
    world.backgrounds[10] = rand.next_max(8) as u8; // ocean
    world.backgrounds[11] = rand.next_max(5) as u8; // mushroom
    world.backgrounds[12] = rand.next_max(3) as u8; // underworld
}

/// `WorldGen.RandomizeBackgrounds_Corruption`, `WorldGen.cs:7962-7970`.
fn randomize_backgrounds_corruption(rand: &mut UnifiedRandom) -> u8 {
    let mut style = rand.next_max(6);
    if style == 5 {
        style = if rand.next_max(2) == 0 { 51 } else { 52 };
    }
    style as u8
}

/// `WorldGen.RandomizeBackgrounds_Desert`, `WorldGen.cs:7972-7981`.
fn randomize_backgrounds_desert(rand: &mut UnifiedRandom) -> u8 {
    let mut style = rand.next_max(6);
    if style == 5 {
        let extra = rand.next_max(5);
        style = 51 + extra / 2;
    }
    style as u8
}

/// `WorldGen.RollRandomForestBGStyle`, `WorldGen.cs:7983-8006`.
fn roll_random_forest_bg_style(rand: &mut UnifiedRandom) -> u8 {
    let max = 14;
    let mut style = rand.next_max(max);
    if (style == 1 || style == 2) && rand.next_max(2) == 0 {
        style = rand.next_max(max);
    }
    if style == 0 {
        style = rand.next_max(max);
    }
    if style == 3 && rand.next_max(3) == 0 {
        style = 31;
    }
    if style == 5 && rand.next_max(2) == 0 {
        style = 51;
    }
    if style == 7 && rand.next_max(4) == 0 {
        style = rand.next_range(71, 74);
    }
    style as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    /// The reference world: 4200x1200, crimson, seed text "12345".
    ///
    /// These are the values the reference's own header records for the same seed (tree and cave
    /// styles, backgrounds, moon), plus the outputs Reset does not save (beach ends, origins).
    /// They are pinned because every one of them is downstream of the exact number and order of
    /// draws, which the pass oracle cannot see.
    #[test]
    fn the_reference_seed_resets_to_the_reference_values() {
        let mut vars = GenVars::new(12_345, World::empty(4200, 1200, "reference"));
        reset(&mut vars, Some(true));
        let world = &vars.world;

        assert!(world.crimson);
        assert_eq!(world.id, 1_904_902_962);
        assert_eq!(world.ore_tiers, [166, 167, 168, 8, -1, -1, -1]);
        assert_eq!(world.num_clouds, 157);
        assert_eq!(world.wind, 0.178_898_74_f32);
        assert_eq!(world.tree_x, [2858, 4200, 4200]);
        assert_eq!(world.tree_style, [2, 4, 0, 0]);
        assert_eq!(world.cave_back_x, [1803, 4200, 4200]);
        assert_eq!(world.cave_back_style, [5, 4, 0, 0]);
        assert_eq!(world.backgrounds, [51, 9, 2, 6, 1, 5, 0, 2, 3, 2, 5, 2, 0]);
        assert_eq!(world.moon_type, 2);
        assert_eq!(
            (
                world.ice_back_style,
                world.hell_back_style,
                world.jungle_back_style
            ),
            (2, 2, 1)
        );

        assert_eq!(vars.dungeon_side, DungeonSide::Right);
        assert_eq!(vars.dungeon_location, 3436);
        assert_eq!(vars.jungle_origin_x, 798);
        assert_eq!(
            (vars.snow_origin_left, vars.snow_origin_right),
            (2575, 2821)
        );
        assert_eq!((vars.left_beach_end, vars.right_beach_start), (342, 3843));
    }
}

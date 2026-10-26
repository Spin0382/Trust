//! `TerrainPass`: the first pass, which lays the surface line and the dirt/stone layers.
//!
//! Transcribed from `Terraria.GameContent.Biomes/TerrainPass.cs:48-282`. The pass itself is pure
//! geometry and one shared RNG; what makes it worth porting first is that it is the first entry
//! in every world's manifest, so matching its `RandNext` proves the pass draws exactly as
//! vanilla does.
//!
//! The configuration's `FlatBeachPadding` is a constant 5 in the shipped configuration; the JSON
//! lookup is replaced by [`FLAT_BEACH_PADDING`]. Remix, the secret-seed surface overrides and the
//! anniversary world are not modelled: every one of those flags is false for an ordinary world,
//! and the branches that test them are omitted rather than stubbed inline.

use terrustia_proto::Tile;

use super::passes::GenVars;
use super::rand::UnifiedRandom;
use super::tiles::{DIRT, STONE};
use crate::world::World;

/// `configuration.Get<int>("FlatBeachPadding")` in the shipped `Configuration.json`.
const FLAT_BEACH_PADDING: i32 = 5;

/// The five shapes the surface wanders through (`TerrainPass.cs:12-19`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerrainFeatureType {
    Plateau,
    Hill,
    Dale,
    Mountain,
    Valley,
}

impl TerrainFeatureType {
    /// `(TerrainFeatureType)Next(0, 5)` is `Next`'s 0..=4, in the enum's own order.
    fn from_roll(roll: i32) -> Self {
        match roll {
            0 => Self::Plateau,
            1 => Self::Hill,
            2 => Self::Dale,
            3 => Self::Mountain,
            _ => Self::Valley,
        }
    }
}

/// A rolling window of the last 500 surface heights (`TerrainPass.cs:21-55`).
///
/// The indexer is `(index + _index) % length`, so index 0 is the oldest sample and
/// `length - 1` the newest. Reproducing that rotation matters: `RetargetSurfaceHistory` reads
/// the newest sample by that index and then rewrites columns from it backwards.
struct SurfaceHistory {
    heights: [f64; 500],
    index: usize,
}

impl SurfaceHistory {
    fn new() -> Self {
        Self {
            heights: [0.0; 500],
            index: 0,
        }
    }

    fn get(&self, index: usize) -> f64 {
        self.heights[(index + self.index) % self.heights.len()]
    }

    fn set(&mut self, index: usize, value: f64) {
        let at = (index + self.index) % self.heights.len();
        self.heights[at] = value;
    }

    fn record(&mut self, height: f64) {
        self.heights[self.index] = height;
        self.index = (self.index + 1) % self.heights.len();
    }
}

/// Run the pass (`TerrainPass.ApplyPass`, `TerrainPass.cs:57-231`).
pub fn run(vars: &mut GenVars) {
    let max_x = vars.world.width();
    let max_y = vars.world.height();
    let left_beach_end = vars.left_beach_end;
    let right_beach_start = vars.right_beach_start;
    let GenVars { rand, world, .. } = vars;

    let mut feature = TerrainFeatureType::Plateau;
    let mut columns_left;

    // `num3` is the surface line and `num4` the rock line, both in the same units as tile rows.
    let mut surface = max_y as f64 * 0.3;
    surface *= rand.next_range(90, 110) as f64 * 0.005;
    let mut rock = surface + max_y as f64 * 0.2;
    rock *= rand.next_range(90, 110) as f64 * 0.01;

    let mut surface_low = surface;
    let mut surface_high = surface;
    let mut rock_low = rock;
    let mut rock_high = rock;

    let beach_surface_limit = max_y as f64 * 0.23;
    let mut history = SurfaceHistory::new();

    columns_left = left_beach_end + FLAT_BEACH_PADDING;
    for x in 0..max_x {
        surface_low = surface.min(surface_low);
        surface_high = surface.max(surface_high);
        rock_low = rock.min(rock_low);
        rock_high = rock.max(rock_high);

        if columns_left <= 0 {
            feature = TerrainFeatureType::from_roll(rand.next_range(0, 5));
            columns_left = rand.next_range(5, 40);
            if feature == TerrainFeatureType::Plateau {
                columns_left *= (rand.next_range(5, 30) as f64 * 0.2) as i32;
            }
        }
        columns_left -= 1;

        // The middle of the world is kept flat, and a mountain or valley that wanders into it is
        // retyped rather than trusted (`TerrainPass.cs:105-114`).
        if (x as f64) > max_x as f64 * 0.45
            && (x as f64) < max_x as f64 * 0.55
            && matches!(
                feature,
                TerrainFeatureType::Mountain | TerrainFeatureType::Valley
            )
        {
            feature = TerrainFeatureType::from_roll(rand.next_range(0, 3));
        }
        if (x as f64) > max_x as f64 * 0.48 && (x as f64) < max_x as f64 * 0.52 {
            feature = TerrainFeatureType::Plateau;
        }

        surface += generate_world_surface_offset(rand, feature);

        let mut low_fraction = 0.17;
        let high_fraction = 0.26;
        // `WorldGen.GetWorldSize() == 0` for a 4200-wide world (`WorldGen.cs:6507-6518`).
        if max_x <= 4200 {
            low_fraction += 0.02;
        }

        if x < left_beach_end + FLAT_BEACH_PADDING || x > right_beach_start - FLAT_BEACH_PADDING {
            surface = surface.clamp(max_y as f64 * low_fraction, beach_surface_limit);
        } else if surface < max_y as f64 * low_fraction {
            surface = max_y as f64 * low_fraction;
            columns_left = 0;
        } else if surface > max_y as f64 * high_fraction {
            surface = max_y as f64 * high_fraction;
            columns_left = 0;
        }

        // The rock line wanders independently, then is pulled back toward the surface.
        while rand.next_range(0, 3) == 0 {
            rock += rand.next_range(-2, 3) as f64;
        }
        if rock < surface + max_y as f64 * 0.06 {
            rock += 1.0;
        }
        if rock > surface + max_y as f64 * 0.35 {
            rock -= 1.0;
        }

        history.record(surface);
        fill_column(world, x, surface, rock);

        // The right beach is a plateau: the columns behind it are retargeted down to the beach
        // line rather than left as a cliff (`TerrainPass.cs:197-207`).
        if x == right_beach_start - FLAT_BEACH_PADDING {
            if surface > beach_surface_limit {
                retarget_surface_history(&mut history, world, x, beach_surface_limit);
            }
            feature = TerrainFeatureType::Plateau;
            columns_left = max_x - x;
        }
    }

    // `Main.worldSurface` and `Main.rockLayer` (`TerrainPass.cs:209-218`).
    let main_world_surface = (surface_high + 25.0) as i32;
    let mut main_rock_layer = rock_high;
    let step = ((main_rock_layer - main_world_surface as f64) / 6.0) as i32 * 6;
    main_rock_layer = (main_world_surface + step) as f64;

    let water_line = (main_rock_layer + max_y as f64) as i32 / 2 + rand.next_range(-100, 20);
    let lava_line = water_line + rand.next_range(50, 80);

    // The low rock line and high surface are pulled apart if they crossed while the loop ran.
    let mut rock_low_adjusted = rock_low;
    let mut surface_high_adjusted = surface_high;
    if rock_low_adjusted < surface_high_adjusted + 20.0 {
        let middle = (rock_low_adjusted + surface_high_adjusted) / 2.0;
        let mut spread = (rock_low_adjusted - surface_high_adjusted).abs();
        if spread < 20.0 {
            spread = 20.0;
        }
        rock_low_adjusted = middle + spread / 2.0;
        surface_high_adjusted = middle - spread / 2.0;
    }

    vars.rock_layer = rock;
    vars.rock_layer_high = rock_high;
    vars.rock_layer_low = rock_low_adjusted;
    vars.world_surface = surface;
    vars.world_surface_high = surface_high_adjusted;
    vars.world_surface_low = surface_low;
    vars.water_line = water_line;
    vars.lava_line = lava_line;
    // These four are unconditional in vanilla even though only Remix reads them.
    vars.remix_mushroom_layer_low = max_y - 350;
    vars.remix_mushroom_layer_high = max_y - 200; // `Main.UnderworldLayer`
    vars.remix_surface_layer_low = rock_low_adjusted as i32;
    vars.remix_surface_layer_high = max_y - 350;

    world.surface = main_world_surface as i16;
    world.rock_layer = main_rock_layer as i16;
}

/// `TerrainPass.FillColumn`, `TerrainPass.cs:233-258`.
fn fill_column(world: &mut World, x: i32, world_surface: f64, rock_layer: f64) {
    let surface_row = world_surface as i32;
    for y in 0..surface_row {
        // The world starts empty, so clearing is the same as writing air here. The game keeps the
        // old type under an inactive tile; nothing in Terrain depends on it.
        world.set_tile(x, y, Tile::AIR);
    }
    for y in surface_row..world.height() {
        let block = if (y as f64) < rock_layer { DIRT } else { STONE };
        world.set_tile(x, y, Tile::block(block));
    }
}

/// `TerrainPass.RetargetColumn`, `TerrainPass.cs:260-280`.
///
/// Below the new surface, stone is left alone and everything else becomes dirt. Above it, air.
fn retarget_column(world: &mut World, x: i32, world_surface: f64) {
    let surface_row = world_surface as i32;
    for y in 0..surface_row {
        world.set_tile(x, y, Tile::AIR);
    }
    for y in surface_row..world.height() {
        let tile = world.tile(x, y);
        if tile.block != STONE || !tile.is_active() {
            world.set_tile(x, y, Tile::block(DIRT));
        }
    }
}

/// `TerrainPass.GenerateWorldSurfaceOffset`, `TerrainPass.cs:282-397`.
///
/// The `drunk`/`getGood`/Remix branch at the top is omitted: all three flags are false for an
/// ordinary world, so only the second switch runs.
fn generate_world_surface_offset(rand: &mut UnifiedRandom, feature: TerrainFeatureType) -> f64 {
    let mut offset = 0.0;
    match feature {
        TerrainFeatureType::Plateau => {
            while rand.next_range(0, 7) == 0 {
                offset += rand.next_range(-1, 2) as f64;
            }
        }
        TerrainFeatureType::Hill => {
            while rand.next_range(0, 4) == 0 {
                offset -= 1.0;
            }
            while rand.next_range(0, 10) == 0 {
                offset += 1.0;
            }
        }
        TerrainFeatureType::Dale => {
            while rand.next_range(0, 4) == 0 {
                offset += 1.0;
            }
            while rand.next_range(0, 10) == 0 {
                offset -= 1.0;
            }
        }
        TerrainFeatureType::Mountain => {
            while rand.next_range(0, 2) == 0 {
                offset -= 1.0;
            }
            while rand.next_range(0, 6) == 0 {
                offset += 1.0;
            }
        }
        TerrainFeatureType::Valley => {
            while rand.next_range(0, 2) == 0 {
                offset += 1.0;
            }
            while rand.next_range(0, 5) == 0 {
                offset -= 1.0;
            }
        }
    }
    offset
}

/// `TerrainPass.RetargetSurfaceHistory`, `TerrainPass.cs:399-425`.
fn retarget_surface_history(
    history: &mut SurfaceHistory,
    world: &mut World,
    target_x: i32,
    target_height: f64,
) {
    let length = history.heights.len();
    for i in 0..length / 2 {
        if history.get(length - 1) <= target_height {
            break;
        }
        for j in 0..length - i * 2 {
            let mut height = history.get(length - j - 1);
            height -= 1.0;
            history.set(length - j - 1, height);
            if height <= target_height {
                break;
            }
        }
    }
    for k in 0..length {
        let world_surface = history.get(length - k - 1);
        retarget_column(world, target_x - k as i32, world_surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldgen::manifest::Manifest;
    use crate::world::worldgen::passes::{GenVars, compare_against};
    use crate::world::worldgen::reset;

    /// The reference world's Terrain entry, pinned end to end.
    ///
    /// The manifest records `620412521` after Terrain for seed `12345`. Reproducing it means
    /// Reset's two beach ends, Terrain's 17,106 draws and the sample the recorder takes all line
    /// up; the tiles themselves are checked by `genparity` against the real file.
    #[test]
    fn the_reference_terrain_sample_matches() {
        let mut vars = GenVars::new(12_345, World::empty(4200, 1200, "reference"));
        reset::reset(&mut vars, Some(true));
        // `compare_against` reseeds before every pass; this test drives the pass directly.
        vars.rand = UnifiedRandom::new(12_345);
        run(&mut vars);

        assert_eq!(
            i64::from(vars.rand.next()),
            620_412_521,
            "Terrain's draw count or Reset's beach ends are wrong"
        );
        assert_eq!(vars.world.surface, 331);
        assert_eq!(vars.world.rock_layer, 427);
        assert_eq!(vars.water_line, 831);
        assert_eq!(vars.lava_line, 898);
        assert_eq!(
            (
                vars.world_surface_low,
                vars.world_surface,
                vars.world_surface_high
            ),
            (163.8, 267.0, 306.0)
        );
        assert_eq!(
            (vars.rock_layer_low, vars.rock_layer, vars.rock_layer_high),
            (340.458, 396.458, 427.458)
        );
    }

    /// `SurfaceHistory` reads newest at `length - 1` and oldest at 0, wrapping as it records.
    #[test]
    fn surface_history_is_a_ring_of_five_hundred() {
        let mut history = SurfaceHistory::new();
        for i in 0..600 {
            history.record(i as f64);
        }
        assert_eq!(history.get(499), 599.0);
        assert_eq!(history.get(0), 100.0);
        history.set(499, 1.0);
        assert_eq!(history.get(499), 1.0);
    }

    /// `Terrain` is the first pass the registry runs, and the manifest's first entry is its name.
    #[test]
    fn terrain_is_the_first_registered_pass() {
        let manifest = Manifest::parse(
            r#"{"GenPassResults":[{"Name":"Terrain","DurationMs":1,"RandNext":620412521,"Hash":null,"Skipped":false}],"Version":"v1.4.5.8","FinalHash":null}"#,
        )
        .expect("a manifest");
        let mut reference = World::empty(4200, 1200, "reference");
        reference.crimson = true;
        let outcome = compare_against(&manifest, 12_345, &reference);
        assert_eq!(outcome.first_divergence, None);
        assert_eq!(outcome.checked, 1);
    }
}

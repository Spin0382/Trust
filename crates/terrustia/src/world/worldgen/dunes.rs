//! Surface sand dunes: the rolling hills a desert is made of, rather than a flat sand rectangle.
//!
//! Two separate code paths live here, on purpose:
//!
//! - [`scatter`] is the dune field the **shipping custom generator** lays over `layout.desert`.
//!   It is not vanilla-identical (the custom generator as a whole is not), and its call site in
//!   `mod.rs` is unchanged.
//! - [`run`] is the **faithful vanilla pass** (`GenPassNameID.DunesAndPyramidLocations`), written
//!   for the seed-parity effort and registered in [`super::passes::PASSES`] right after `Terrain`.
//!   Its tests drive it directly and against the reference world.
//!
//! The vanilla path is transcribed from the `DunesAndPyramidLocations` delegate
//! (`WorldGen.cs:11552-11610`), `DungeonCrawler.SetupDungeonGenVarVariables`
//! (`DungeonCrawler.cs:53-212`), `DunesBiome` (`.scratch/decompiled/Terraria.GameContent.Biomes/
//! DunesBiome.cs`, all 162 lines) and the `SurfaceMap` it depends on
//! (`.scratch/decompiled/Terraria.GameContent.Biomes.Desert/SurfaceMap.cs`, 75 lines). Tuning
//! comes from vanilla's shipped config (`Terraria.GameContent.WorldBuilding.Configuration.json`:
//! `Count` 1..2 scaled by world width, `ChanceOfPyramid` 0.8, `HeightScale` 1.0,
//! `SingleDunesWidth` 150-250, unscaled).
//!
//! # How a dune is drawn
//!
//! Each dune is a run of overlapping hills, and each hill is two quadratic Bezier curves sharing a
//! peak that is pushed downwind. Under every point of the curve, sand is filled from the curve
//! down to a depth that tapers toward the dune's edges (`sqrt(distance from edge) * 3`), and
//! anything non-sand in the ten tiles above is cleared out. Wind direction is one coin flip per
//! dune and decides which way every peak in it leans.
//!
//! # The draw stream, and the fourth setup draw
//!
//! The reference world's manifest records `2136736694` after Dunes for seed 12345, which is the
//! 113th draw: the pass consumes exactly 112. Getting there depends on
//! `SetupDungeonGenVarVariables` taking **four** draws, not three. Besides the style roll and the
//! two entrance rolls, `MakeDungeon_GetEntranceSettings` draws the entrance's `RandomSeed`
//! (`DungeonCrawler.cs:856`). With three setup draws the stream moves and the pass consumes 119
//! draws, placing a dune at x 1664 instead of the reference's fields; the tile comparison in
//! `vanilla_tests` is the independent evidence.
//!
//! # One deliberate guard vanilla does not have
//!
//! `PlaceCurvedLine` steps its parameter by `0.5 / (end.X - start.X)`. When the peak is pushed far
//! enough that `end.X <= start.X`, that step is zero or negative and vanilla's `for` loop never
//! terminates - a real hang, reachable only for a narrow hill whose peak offset exceeds its half
//! width. Here the loop is bounded by its own maximum step count and stops. `AGENTS.md` rule 6
//! says a generation pass may not take the process down, and an infinite loop is worse than a
//! panic: it hangs with no message.

use terrustia_proto::Tile;

use super::layout::Layout;
use super::rand::UnifiedRandom;
use super::structure_map::{Rect, StructureMap};
use crate::world::World;

/// Run the vanilla `Dunes` pass (`GenPassNameID.DunesAndPyramidLocations`).
///
/// This is the parity entry point, registered in [`super::passes::PASSES`] after `Terrain`. The
/// shipping custom generator still calls [`scatter`] below; the two are separate code paths.
pub fn run(vars: &mut super::passes::GenVars) {
    let _ = run_counted(vars);
}

/// The pass itself, returning the number of draws it took.
///
/// Kept separate from [`run`] so a test can assert the exact draw count the reference manifest
/// fingerprints, without a counting wrapper around the production path.
fn run_counted(vars: &mut super::passes::GenVars) -> usize {
    vanilla::run(vars)
}

/// The faithful transcription of the vanilla pass.
///
/// Sources:
///
/// - the `DunesAndPyramidLocations` delegate in `WorldGen.AddPasses`, `WorldGen.cs:11552-11610`
/// - `DungeonCrawler.SetupDungeonGenVarVariables`, `DungeonCrawler.cs:53-212`
/// - `DunesBiome`, `DunesBiome.cs:1-162`
/// - `SurfaceMap`, `SurfaceMap.cs:1-75`
/// - the shipped `Dunes`/`DunesBiome` tuning in `Configuration.json` (`Count` 1..2 scaled by world
///   width, `ChanceOfPyramid` 0.8, `SingleDunesWidth` 150..250 unscaled, `HeightScale` 1.0)
///
/// Secret seeds, Remix, Skyblock and dual dungeons are not modelled. Every branch that tests one
/// of those flags is omitted rather than stubbed, because this path only claims the ordinary
/// generator the reference world came from.
mod vanilla {
    use terrustia_proto::Tile;

    use crate::world::World;
    use crate::world::worldgen::passes::GenVars;
    use crate::world::worldgen::rand::UnifiedRandom;
    use crate::world::worldgen::structure_map::{Rect, StructureMap};

    /// `DunesBiome.SingleDunesWidth`: 150..250, `ScaleWith: None`.
    const SINGLE_DUNES_WIDTH: (i32, i32) = (150, 250);
    /// `DunesBiome.HeightScale`.
    const HEIGHT_SCALE: f64 = 1.0;
    /// `Dunes.ChanceOfPyramid`.
    const CHANCE_OF_PYRAMID: f64 = 0.8;
    /// `Dunes.Count`, `ScaleWith: WorldWidth`.
    const COUNT: (i32, i32) = (1, 2);
    /// Sand (`TileID.Sand`).
    const SAND: u16 = 53;
    /// `TileID.Sets.Clouds` (`TileID.cs:197`). A cloud is not ground, so the surface scan looks
    /// past it.
    const CLOUDS: [u16; 6] = [189, 196, 460, 717, 718, 719];

    /// A generator that also counts its draws.
    ///
    /// The count is the whole point of a pass oracle: the reference manifest records the draw
    /// *after* the pass, so one missing or extra draw here names the pass as wrong even when the
    /// tiles it wrote happen to look plausible.
    struct Draws<'a> {
        rand: &'a mut UnifiedRandom,
        count: usize,
    }

    impl<'a> Draws<'a> {
        fn new(rand: &'a mut UnifiedRandom) -> Self {
            Self { rand, count: 0 }
        }

        fn next(&mut self) -> i32 {
            self.count += 1;
            self.rand.next()
        }

        fn next_max(&mut self, max: i32) -> i32 {
            self.count += 1;
            self.rand.next_max(max)
        }

        fn next_range(&mut self, min: i32, max: i32) -> i32 {
            self.count += 1;
            self.rand.next_range(min, max)
        }

        fn next_double(&mut self) -> f64 {
            self.count += 1;
            self.rand.next_double()
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum WindDirection {
        Left,
        Right,
    }

    /// `SurfaceMap` (`SurfaceMap.cs:6-75`): the first solid, non-cloud tile in each column of a
    /// span.
    ///
    /// Vanilla stores the raw recorded height in `_heights`; only `Bottom`, `Top` and `Average`
    /// are derived, and only `Bottom` is clamped (to `Main.worldSurface - 10`). The indexer
    /// itself returns the raw height. That distinction is load-bearing: clamping every sample
    /// would flatten the dunes under a low-flying cloud or an overhang, and the hills read these
    /// heights directly.
    struct SurfaceMap {
        heights: Vec<i16>,
        x: i32,
        /// Vanilla's `Bottom`, the maximum recorded height clamped to `worldSurface - 10`.
        /// Nothing in this pass reads it, but it is kept (and tested) so the transcription is
        /// complete rather than only the part that happens to be used today.
        #[allow(dead_code)]
        bottom: i32,
        /// Vanilla's `Top`, the minimum recorded height, unclamped.
        #[allow(dead_code)]
        top: i32,
        /// Vanilla's `Average`, the mean recorded height, unclamped.
        #[allow(dead_code)]
        average: f64,
    }

    impl SurfaceMap {
        /// `SurfaceMap.FromArea` (`SurfaceMap.cs:44-74`).
        fn from_area(world: &World, start_x: i32, width: i32) -> Self {
            let scan = world.height() / 2;
            let width = width.max(0);
            let mut heights = Vec::with_capacity(width as usize);
            for i in start_x..start_x + width {
                let mut found = false;
                let mut height = 0;
                for j in 50..50 + scan {
                    let tile = world.tile(i, j);
                    if tile.is_active() {
                        if CLOUDS.contains(&tile.block) {
                            found = false;
                        } else if !found {
                            height = j;
                            found = true;
                        }
                    }
                    if !found {
                        height = scan + 50;
                    }
                }
                // `short[]` in vanilla; the scan can never exceed `maxTilesY / 2 + 50`.
                heights.push(height as i16);
            }

            let mut sum = 0i32;
            let mut max = 0i32;
            let mut min = i32::MAX;
            for &height in &heights {
                let height = i32::from(height);
                sum += height;
                max = max.max(height);
                min = min.min(height);
            }
            let world_surface = f64::from(world.surface);
            if f64::from(max) > world_surface - 10.0 {
                max = world_surface as i32 - 10;
            }
            let bottom = max;
            let top = if heights.is_empty() { 0 } else { min };
            let average = if heights.is_empty() {
                0.0
            } else {
                f64::from(sum) / heights.len() as f64
            };

            Self {
                heights,
                x: start_x,
                bottom,
                top,
                average,
            }
        }

        /// Vanilla's `this[int absoluteX]` (`SurfaceMap.cs:20`): the raw recorded height.
        ///
        /// Vanilla indexes without a bounds check. Every caller here stays inside the sampled
        /// span by construction, so an out-of-range read is a transcription bug; it falls back to
        /// the nearest recorded column rather than taking the process down, matching this
        /// workspace's rule against panics on a generation path.
        fn at(&self, absolute_x: i32) -> i32 {
            if self.heights.is_empty() {
                return 0;
            }
            let index = (absolute_x - self.x).clamp(0, self.heights.len() as i32 - 1);
            i32::from(self.heights[index as usize])
        }
    }

    /// `DunesDescription`, the per-field data `DunesBiome.Place` builds two of.
    struct Dune {
        area: Rect,
        surface: SurfaceMap,
        wind: WindDirection,
    }

    impl Dune {
        /// `DunesDescription.CreateFromPlacement` (`DunesBiome.cs:26-36`).
        fn new(
            world: &World,
            rng: &mut Draws<'_>,
            origin: (i32, i32),
            width: i32,
            height: i32,
        ) -> Self {
            let area = Rect::new(origin.0 - width / 2, origin.1 - height / 2, width, height);
            let surface = SurfaceMap::from_area(world, area.x - 20, area.width + 40);
            // The wind draw comes after the surface sample in the object initializer, so its
            // position in the stream is not interchangeable with the height/width draws.
            let wind = if rng.next_max(2) != 0 {
                WindDirection::Right
            } else {
                WindDirection::Left
            };
            Self {
                area,
                surface,
                wind,
            }
        }

        fn center_x(&self) -> i32 {
            self.area.x + self.area.width / 2
        }
    }

    /// `WorldGenRange.ScaleValue` (`WorldGenRange.cs:43-59`), for a 4200-wide reference world.
    fn scaled(value: i32, world_width: i32) -> i32 {
        ((world_width as f64 / 4200.0) * f64::from(value)) as i32
    }

    /// `WorldGen.RandomWorldPoint(top, right, bottom, left)` (`WorldGen.cs:27387-27390`).
    ///
    /// The arguments are in vanilla's order, and the x draw comes first.
    fn random_world_point(
        rng: &mut Draws<'_>,
        world: &World,
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    ) -> (i32, i32) {
        (
            rng.next_range(left, world.width() - right),
            rng.next_range(top, world.height() - bottom),
        )
    }

    /// `DungeonCrawler.SetupDungeonGenVarVariables` (`DungeonCrawler.cs:53-212`), for an ordinary
    /// world.
    ///
    /// It takes **four** draws, not the three the style and entrance rolls alone suggest:
    ///
    /// 1. the dungeon style, `genRand.Next(3)` (`:55`)
    /// 2. the dome-entrance roll, `genRand.Next(3)` (`:188`)
    /// 3. the tower-entrance roll, `genRand.Next(3)` (`:192`)
    /// 4. `PreGenDungeonEntranceSettings.RandomSeed = genRand.Next()` in
    ///    `MakeDungeon_GetEntranceSettings` (`DungeonCrawler.cs:856`)
    ///
    /// The fourth draw is easy to miss because it is one call away, in the entrance-settings
    /// factory, and it is what makes the reference's Dunes entry reconcile: with three draws the
    /// pass lands on `1608783339` after 119 draws and puts a dune at x 1664; with four it lands on
    /// `2136736694` after exactly 112 draws and puts the fields where the reference has sand.
    fn setup_dungeon_gen_var_variables(rng: &mut Draws<'_>) {
        // Style: the brick/colour tables it selects are the Dungeon pass's business, not this
        // one's, but the draw is part of the stream.
        let _style = rng.next_max(3);
        // The entrance loop runs once for an ordinary world: neither roll can fail the loop
        // again because `surfaceIsInSpace` is false.
        let _dome = rng.next_max(3);
        let _tower = rng.next_max(3);
        let _entrance_seed = rng.next();
    }

    /// `DunesBiome.Place` (`DunesBiome.cs:53-64`), reached from the delegate at
    /// `WorldGen.cs:11595`.
    ///
    /// Always returns `true`, as vanilla does.
    fn place(
        world: &mut World,
        structures: &mut StructureMap,
        rng: &mut Draws<'_>,
        origin: (i32, i32),
    ) -> bool {
        let height = (f64::from(rng.next_range(60, 100)) * HEIGHT_SCALE) as i32;
        let height_b = (f64::from(rng.next_range(60, 100)) * HEIGHT_SCALE) as i32;
        let width = rng.next_range(SINGLE_DUNES_WIDTH.0, SINGLE_DUNES_WIDTH.1 + 1);
        let width_b = rng.next_range(SINGLE_DUNES_WIDTH.0, SINGLE_DUNES_WIDTH.1 + 1);

        let first = Dune::new(
            world,
            rng,
            (origin.0 - width / 2 + 30, origin.1),
            width,
            height,
        );
        let second = Dune::new(
            world,
            rng,
            (origin.0 + width_b / 2 - 30, origin.1),
            width_b,
            height_b,
        );
        place_single(world, structures, rng, &first);
        place_single(world, structures, rng, &second);
        true
    }

    /// `DunesBiome.PlaceSingle` (`DunesBiome.cs:66-87`): a run of small hills, then one or two
    /// large ones over the middle.
    fn place_single(
        world: &mut World,
        structures: &mut StructureMap,
        rng: &mut Draws<'_>,
        dune: &Dune,
    ) {
        let count = rng.next_max(3) + 8;
        for i in 0..count - 1 {
            let span = (2.0 / f64::from(count) * f64::from(dune.area.width)) as i32;
            let mut x = (f64::from(i) / f64::from(count) * f64::from(dune.area.width)
                + f64::from(dune.area.x)) as i32
                + span * 2 / 5;
            x += rng.next_range(-5, 6);
            let along = f64::from(i) / f64::from(count - 2);
            let crest = 1.0 - (along - 0.5).abs() * 2.0;
            place_hill(
                world,
                rng,
                dune,
                x - span / 2,
                x + span / 2,
                (crest * 0.3 + 0.2) * HEIGHT_SCALE,
            );
        }
        let big = rng.next_max(2) + 1;
        for _ in 0..big {
            let half = dune.area.width / 2;
            let mut x = dune.area.x + dune.area.width / 2;
            x += rng.next_range(-10, 11);
            place_hill(
                world,
                rng,
                dune,
                x - half / 2,
                x + half / 2,
                0.8 * HEIGHT_SCALE,
            );
        }
        structures.add_structure(dune.area, 20);
    }

    /// `DunesBiome.PlaceHill` (`DunesBiome.cs:89-108`): a peak between two ends, leaned
    /// downwind, drawn as two quadratic curves.
    ///
    /// The lean draw is unconditional in vanilla. Both wind directions call
    /// `WorldGen.genRand.Next(minValue, num + 1)`; only the sign of the result differs. Guarding
    /// it would move every later draw.
    fn place_hill(
        world: &mut World,
        rng: &mut Draws<'_>,
        dune: &Dune,
        start_x: i32,
        end_x: i32,
        scale: f64,
    ) {
        let start = (start_x, dune.surface.at(start_x));
        let end = (end_x, dune.surface.at(end_x));
        let mut peak = (
            (start.0 + end.0) / 2,
            (start.1 + end.1) / 2 - (35.0 * scale) as i32,
        );
        let max_lean = (end.0 - peak.0) / 4;
        let min_lean = (end.0 - peak.0) / 16;
        let lean = rng.next_range(min_lean, max_lean + 1);
        match dune.wind {
            WindDirection::Left => peak.0 -= lean,
            WindDirection::Right => peak.0 += lean,
        }
        let down = (0, (scale * 12.0) as i32);
        let up = (down.0 / -2, down.1 / -2);
        let (first, second) = match dune.wind {
            WindDirection::Left => (down, up),
            WindDirection::Right => (up, down),
        };
        place_curved_line(world, dune, start, peak, first);
        place_curved_line(world, dune, peak, end, second);
    }

    fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }

    /// `DunesBiome.PlaceCurvedLine` (`DunesBiome.cs:110-161`): a quadratic Bezier, filling sand
    /// under every point it visits.
    ///
    /// The `step` is `0.5 / (end.X - start.X)`. Vanilla's `for` loop would never terminate for a
    /// backwards span; every hill this pass builds has `end.X > start.X`, so the guard below is
    /// unreachable here but keeps a malformed world from hanging generation.
    fn place_curved_line(
        world: &mut World,
        dune: &Dune,
        start: (i32, i32),
        end: (i32, i32),
        anchor_offset: (i32, i32),
    ) {
        let anchor = (
            (start.0 + end.0) / 2 + anchor_offset.0,
            (start.1 + end.1) / 2 + anchor_offset.1,
        );
        let a = (f64::from(start.0), f64::from(start.1));
        let b = (f64::from(end.0), f64::from(end.1));
        let c = (f64::from(anchor.0), f64::from(anchor.1));
        let step = 0.5 / (b.0 - a.0);
        if !step.is_finite() || step <= 0.0 {
            return;
        }
        // A forward span can never need more steps than this; the bound only matters for a
        // pathological input the guard above already rejected.
        let max_steps = (1.0 / step).ceil() as i64 + 2;

        let mut last = (-1, -1);
        let mut t = 0.0;
        let mut taken = 0i64;
        while t <= 1.0 && taken <= max_steps {
            taken += 1;
            let first = lerp(a, c, t);
            let second = lerp(c, b, t);
            let point = lerp(first, second, t);
            let point = (point.0 as i32, point.1 as i32);
            t += step;
            if point == last {
                continue;
            }
            last = point;

            let from_edge = dune.area.width / 2 - (point.0 - dune.center_x()).abs();
            let floor = dune.surface.at(point.0) + ((from_edge as f64).sqrt() * 3.0) as i32;

            for y in point.1 - 10..point.1 {
                let tile = world.tile(point.0, y);
                if tile.is_active() && tile.block != SAND {
                    // `ClearEverything` clears the wall too.
                    world.set_tile(point.0, y, Tile::AIR);
                }
            }
            for y in point.1..floor {
                // `ResetToType(53)` clears the wall in vanilla; the task's tile contract for
                // this port keeps the wall that was already under the sand.
                let mut sand = Tile::block(SAND);
                sand.wall = world.tile(point.0, y).wall;
                world.set_tile(point.0, y, sand);
            }
        }
    }

    /// Run the delegate body (`WorldGen.cs:11552-11610`), returning its exact draw count.
    pub(super) fn run(vars: &mut GenVars) -> usize {
        let width = vars.world.width();
        let height = vars.world.height();
        let size_scale = width as f64 / 4200.0;
        let jungle_origin_x = vars.jungle_origin_x;
        let snow_origin_left = vars.snow_origin_left;
        let snow_origin_right = vars.snow_origin_right;

        let GenVars {
            rand,
            world,
            structures,
            pyr_x,
            pyr_y,
            num_pyr,
            ..
        } = vars;
        let mut rng = Draws::new(rand);

        setup_dungeon_gen_var_variables(&mut rng);

        // `passConfig.Get<WorldGenRange>("Count").GetRandom(genRand)` (`WorldGen.cs:11569`).
        let count = rng.next_range(scaled(COUNT.0, width), scaled(COUNT.1, width) + 1);
        let chance_of_pyramid = CHANCE_OF_PYRAMID;

        // `GenVars.PyrX = new int[random + 3]` (`WorldGen.cs:11575-11577`).
        *pyr_x = vec![0; (count + 3).max(0) as usize];
        *pyr_y = vec![0; (count + 3).max(0) as usize];
        *num_pyr = 0;

        for _ in 0..count {
            let mut origin = (0, 0);
            let mut found = false;
            let mut attempts = 0;
            while !found {
                origin = random_world_point(&mut rng, world, 0, 500, 0, 500);
                let mut near_jungle =
                    (origin.0 - jungle_origin_x).abs() < (600.0 * size_scale) as i32;
                let near_center = (origin.0 - width / 2).abs() < 300;
                let mut near_snow =
                    origin.0 > snow_origin_left - 300 && origin.0 < snow_origin_right + 300;
                attempts += 1;
                if attempts >= width {
                    near_jungle = false;
                }
                if attempts >= width * 2 {
                    near_snow = false;
                }
                found = !(near_jungle || near_center || near_snow);
            }

            place(world, structures, &mut rng, origin);

            if rng.next_double() <= chance_of_pyramid {
                let x = rng.next_range(origin.0 - 200, origin.0 + 200);
                for y in 0..height {
                    if world.tile(x, y).is_active() {
                        if *num_pyr < pyr_x.len() {
                            pyr_x[*num_pyr] = x;
                            pyr_y[*num_pyr] = y + 20;
                            *num_pyr += 1;
                        }
                        break;
                    }
                }
            }
        }

        rng.count
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn shelf(width: i32, height: i32, surface: i32) -> World {
            let mut world = World::empty(width, height, "dunes");
            for x in 0..width {
                for y in surface..height {
                    world.set_tile(x, y, Tile::block(1));
                }
            }
            world
        }

        /// The indexer returns the raw recorded height; only `Bottom` is clamped to
        /// `worldSurface - 10`. This is the distinction a partial port got wrong by clamping
        /// every sample, which flattened any dune under a low-flying cloud.
        #[test]
        fn raw_heights_survive_and_only_bottom_is_clamped() {
            let mut world = shelf(400, 600, 180);
            world.surface = 331;
            // One column starts much lower, so its raw height exceeds the world surface. It is
            // still inside the 300-row scan window that starts at y 50.
            for y in 180..340 {
                world.set_tile(10, y, Tile::AIR);
            }

            let map = SurfaceMap::from_area(&world, 0, 300);
            assert_eq!(map.at(0), 180);
            assert_eq!(map.at(10), 340, "the indexer must not clamp a sample");
            assert_eq!(map.bottom, 321, "Bottom is worldSurface - 10");
            assert_eq!(map.top, 180);
            assert_eq!(map.average, (299.0 * 180.0 + 340.0) / 300.0);
        }

        /// `TileID.Sets.Clouds` is not ground: the scan keeps looking below it.
        #[test]
        fn a_cloud_does_not_end_the_surface_scan() {
            let mut world = shelf(400, 600, 180);
            for y in 90..95 {
                world.set_tile(10, y, Tile::block(189));
            }
            let map = SurfaceMap::from_area(&world, 0, 300);
            assert_eq!(map.at(10), 180, "the cloud must not count as the surface");
        }
    }
}

/// `SingleDunesWidth`, unscaled.
const SINGLE_DUNES_WIDTH: (i32, i32) = (150, 250);
/// `HeightScale`.
const HEIGHT_SCALE: f64 = 1.0;
/// Sand.
const SAND: u16 = 53;
/// `TileID.Sets.Clouds` (`TileID.cs:197`). A cloud is not ground, so the surface scan looks past it.
const CLOUDS: [u16; 6] = [189, 196, 460, 717, 718, 719];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wind {
    Left,
    Right,
}

/// `SurfaceMap`: the first solid, non-cloud tile in each column of a span.
struct SurfaceMap {
    heights: Vec<i32>,
    x: i32,
}

impl SurfaceMap {
    /// `SurfaceMap.FromArea` (`SurfaceMap.cs:43-74`).
    fn from_area(world: &World, start_x: i32, width: i32) -> Self {
        let half = world.height() / 2;
        let mut heights = Vec::with_capacity(width.max(0) as usize);
        for i in start_x..start_x + width {
            let mut found = false;
            let mut height = 0;
            for j in 50..50 + half {
                let t = world.tile(i, j);
                if t.is_active() {
                    if CLOUDS.contains(&t.block) {
                        found = false;
                    } else if !found {
                        height = j;
                        found = true;
                    }
                }
                if !found {
                    height = half + 50;
                }
            }
            heights.push(height);
        }
        Self {
            heights,
            x: start_x,
        }
    }

    /// Vanilla indexes this without a bounds check; clamping keeps a hill whose curve wanders past
    /// the sampled span reading a real height instead of faulting.
    fn at(&self, absolute_x: i32) -> i32 {
        if self.heights.is_empty() {
            return 0;
        }
        let idx = (absolute_x - self.x).clamp(0, self.heights.len() as i32 - 1);
        self.heights[idx as usize]
    }
}

/// `DunesDescription`.
struct Dune {
    area: Rect,
    surface: SurfaceMap,
    wind: Wind,
}

impl Dune {
    /// `DunesDescription.CreateFromPlacement` (`DunesBiome.cs:26-35`).
    fn new(
        world: &World,
        rand: &mut UnifiedRandom,
        origin: (i32, i32),
        width: i32,
        height: i32,
    ) -> Self {
        let area = Rect::new(origin.0 - width / 2, origin.1 - height / 2, width, height);
        let surface = SurfaceMap::from_area(world, area.x - 20, area.width + 40);
        let wind = if rand.next_max(2) != 0 {
            Wind::Right
        } else {
            Wind::Left
        };
        Self {
            area,
            surface,
            wind,
        }
    }

    fn center_x(&self) -> i32 {
        self.area.x + self.area.width / 2
    }
}

/// `DunesBiome.Place` (`:52-63`). Two dunes straddling the origin. Always returns true, as vanilla
/// does: the hills are drawn unconditionally once a site is chosen.
pub fn place(
    world: &mut World,
    structures: &mut StructureMap,
    rand: &mut UnifiedRandom,
    origin: (i32, i32),
) -> bool {
    let height_a = (f64::from(rand.next_range(60, 100)) * HEIGHT_SCALE) as i32;
    let height_b = (f64::from(rand.next_range(60, 100)) * HEIGHT_SCALE) as i32;
    let width_a = rand.next_range(SINGLE_DUNES_WIDTH.0, SINGLE_DUNES_WIDTH.1 + 1);
    let width_b = rand.next_range(SINGLE_DUNES_WIDTH.0, SINGLE_DUNES_WIDTH.1 + 1);

    let a = Dune::new(
        world,
        rand,
        (origin.0 - width_a / 2 + 30, origin.1),
        width_a,
        height_a,
    );
    let b = Dune::new(
        world,
        rand,
        (origin.0 + width_b / 2 - 30, origin.1),
        width_b,
        height_b,
    );
    place_single(world, structures, rand, &a);
    place_single(world, structures, rand, &b);
    true
}

/// `PlaceSingle` (`:65-86`): a run of small hills, then one or two large ones over the middle.
fn place_single(
    world: &mut World,
    structures: &mut StructureMap,
    rand: &mut UnifiedRandom,
    dune: &Dune,
) {
    let count = rand.next_max(3) + 8;
    for i in 0..count - 1 {
        let span = (2.0 / f64::from(count) * f64::from(dune.area.width)) as i32;
        let mut x = (f64::from(i) / f64::from(count) * f64::from(dune.area.width)
            + f64::from(dune.area.x)) as i32
            + span * 2 / 5;
        x += rand.next_range(-5, 6);
        let t = f64::from(i) / f64::from(count - 2);
        let peak = 1.0 - (t - 0.5).abs() * 2.0;
        place_hill(
            world,
            rand,
            dune,
            x - span / 2,
            x + span / 2,
            (peak * 0.3 + 0.2) * HEIGHT_SCALE,
        );
    }
    let big = rand.next_max(2) + 1;
    for _ in 0..big {
        let half = dune.area.width / 2;
        let mut x = dune.center_x();
        x += rand.next_range(-10, 11);
        place_hill(
            world,
            rand,
            dune,
            x - half / 2,
            x + half / 2,
            0.8 * HEIGHT_SCALE,
        );
    }
    structures.add_structure(dune.area, 20);
}

/// `PlaceHill` (`:88-106`): a peak between two ends, leaned downwind, drawn as two curves.
fn place_hill(
    world: &mut World,
    rand: &mut UnifiedRandom,
    dune: &Dune,
    start_x: i32,
    end_x: i32,
    scale: f64,
) {
    let start = (start_x, dune.surface.at(start_x));
    let end = (end_x, dune.surface.at(end_x));
    let mut peak = (
        (start.0 + end.0) / 2,
        (start.1 + end.1) / 2 - (35.0 * scale) as i32,
    );
    let max_lean = (end.0 - peak.0) / 4;
    let min_lean = (end.0 - peak.0) / 16;
    // Vanilla's `Next(min, max + 1)` faults when min > max; both derive from the same difference,
    // so they can only invert when it is negative, which a zero-width hill produces.
    if min_lean <= max_lean {
        let lean = rand.next_range(min_lean, max_lean + 1);
        match dune.wind {
            Wind::Left => peak.0 -= lean,
            Wind::Right => peak.0 += lean,
        }
    }
    let down = (0, (scale * 12.0) as i32);
    let up = (down.0 / -2, down.1 / -2);
    let (first, second) = match dune.wind {
        Wind::Left => (down, up),
        Wind::Right => (up, down),
    };
    place_curved_line(world, dune, start, peak, first);
    place_curved_line(world, dune, peak, end, second);
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// `PlaceCurvedLine` (`:108-161`): a quadratic Bezier, filling sand under every point it visits.
fn place_curved_line(
    world: &mut World,
    dune: &Dune,
    start: (i32, i32),
    end: (i32, i32),
    anchor_offset: (i32, i32),
) {
    let anchor = (
        (start.0 + end.0) / 2 + anchor_offset.0,
        (start.1 + end.1) / 2 + anchor_offset.1,
    );
    let a = (f64::from(start.0), f64::from(start.1));
    let b = (f64::from(end.0), f64::from(end.1));
    let c = (f64::from(anchor.0), f64::from(anchor.1));
    let step = 0.5 / (b.0 - a.0);
    // See the module doc: vanilla hangs here for a backwards span. The bound is the number of
    // steps a forward span could ever need, so it never truncates a curve vanilla would finish.
    if !step.is_finite() || step <= 0.0 {
        return;
    }
    let max_steps = ((1.0 / step).ceil() as i64 + 2).max(0);

    let mut last: Option<(i32, i32)> = None;
    let mut t = 0.0;
    let mut taken = 0i64;
    while t <= 1.0 && taken <= max_steps {
        taken += 1;
        let p = lerp(lerp(a, c, t), lerp(c, b, t), t);
        let point = (p.0 as i32, p.1 as i32);
        t += step;
        if Some(point) == last {
            continue;
        }
        last = Some(point);

        // Depth tapers toward the dune's edges.
        let from_edge = dune.area.width / 2 - (point.0 - dune.center_x()).abs();
        let floor = dune.surface.at(point.0) + (f64::from(from_edge.max(0)).sqrt() * 3.0) as i32;

        for y in point.1 - 10..point.1 {
            let t = world.tile(point.0, y);
            if t.is_active() && t.block != SAND {
                world.set_tile(point.0, y, Tile::AIR);
            }
        }
        for y in point.1..floor {
            world.set_tile(point.0, y, Tile::block(SAND));
        }
    }
}

/// `MaximumWidth` (`:50`), for a caller sizing a site.
pub fn maximum_width() -> i32 {
    SINGLE_DUNES_WIDTH.1 * 2
}

/// Shape the surface desert this world laid out.
///
/// Vanilla drives `DunesBiome` from inside the desert/pyramid pass, choosing sites with
/// `RandomWorldPoint` and rejecting any too near the jungle, the world centre or the snow
/// (`WorldGen.cs:11574-11590`). This generator does not scatter deserts that way: `layout.desert`
/// is a single decided band, so the dune field is placed over it rather than searched for. The
/// rejection tests have nothing to reject here, which is why they have no counterpart.
pub fn scatter(
    world: &mut World,
    layout: &Layout,
    structures: &mut StructureMap,
    rand: &mut UnifiedRandom,
) -> usize {
    let x = layout.desert.from + layout.desert.width() / 2;
    if layout.desert.width() < 100 || x - maximum_width() < 0 || x + maximum_width() >= layout.width
    {
        return 0;
    }
    if place(world, structures, rand, (x, layout.surface)) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat stone shelf with air above it: the surface map should find its top row.
    fn shelf(w: i32, h: i32, surface: i32) -> World {
        let mut world = World::empty(w, h, "dunes");
        for x in 0..w {
            for y in surface..h {
                world.set_tile(x, y, Tile::block(1));
            }
        }
        world
    }

    #[test]
    fn the_surface_map_finds_the_first_solid_row() {
        let world = shelf(400, 600, 180);
        let map = SurfaceMap::from_area(&world, 0, 300);
        assert_eq!(map.at(0), 180);
        assert_eq!(map.at(299), 180);
    }

    /// A cloud is not ground: the scan keeps going past it.
    #[test]
    fn the_surface_map_looks_past_clouds() {
        let mut world = shelf(400, 600, 180);
        for y in 90..95 {
            world.set_tile(10, y, Tile::block(189));
        }
        let map = SurfaceMap::from_area(&world, 0, 300);
        assert_eq!(map.at(10), 180, "the cloud must not count as the surface");
    }

    #[test]
    fn a_dune_lays_down_real_sand_above_the_old_surface() {
        let mut world = shelf(1200, 600, 200);
        let mut structures = StructureMap::new();
        let mut rand = UnifiedRandom::new(31337);
        assert!(place(&mut world, &mut structures, &mut rand, (600, 200)));

        let mut sand = 0;
        for x in 300..900 {
            for y in 150..220 {
                if world.tile(x, y).block == SAND {
                    sand += 1;
                }
            }
        }
        assert!(sand > 500, "expected a real dune, found {sand} sand tiles");
    }

    /// The backwards-span case vanilla hangs on. A zero-width hill drives the peak offset past the
    /// end, which makes the step non-positive.
    #[test]
    fn a_backwards_span_returns_instead_of_hanging() {
        let mut world = shelf(400, 600, 200);
        let dune = Dune {
            area: Rect::new(100, 180, 100, 40),
            surface: SurfaceMap::from_area(&world, 80, 140),
            wind: Wind::Left,
        };
        // end.X < start.X: vanilla's loop would step by a negative amount and never finish.
        place_curved_line(&mut world, &dune, (200, 200), (150, 200), (0, 0));
    }

    #[test]
    fn dunes_are_reproducible_from_the_seed() {
        let run = || {
            let mut world = shelf(1200, 600, 200);
            let mut structures = StructureMap::new();
            let mut rand = UnifiedRandom::new(808);
            place(&mut world, &mut structures, &mut rand, (600, 200));
            let mut fingerprint = Vec::new();
            for x in (300..900).step_by(3) {
                for y in (150..250).step_by(3) {
                    fingerprint.push(world.tile(x, y).block);
                }
            }
            fingerprint
        };
        assert_eq!(run(), run());
    }
}

#[cfg(test)]
mod vanilla_tests {
    use super::*;
    use crate::world::wld;
    use crate::world::worldgen::passes::GenVars;
    use crate::world::worldgen::{reset, terrain_pass};

    /// The reference world: 4200x1200, crimson, seed text "12345", generated by the real
    /// 1.4.5.8 dedicated server.
    ///
    /// `.scratch` sits at the repository root in a normal checkout. This test may run in a git
    /// worktree under `.qoder/worktrees/<name>`, so the file is found by walking up from the
    /// crate rather than assuming a fixed depth.
    fn reference_world() -> Option<World> {
        let mut directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
        loop {
            let candidate = directory.join(".scratch/vanilla.wld");
            if candidate.is_file() {
                return match wld::load(&candidate) {
                    Ok(world) => Some(world),
                    Err(error) => {
                        eprintln!(
                            "reference world {} unreadable: {error:?}",
                            candidate.display()
                        );
                        None
                    }
                };
            }
            if !directory.pop() {
                eprintln!("reference world .scratch/vanilla.wld not found above the crate");
                return None;
            }
        }
    }

    /// Reset, Terrain, and then reseed before Dunes, exactly as `WorldGenerator.RunPass` does.
    fn terrain_world() -> GenVars {
        let mut vars = GenVars::new(12_345, World::empty(4200, 1200, "reference"));
        reset::reset(&mut vars, Some(true));
        vars.rand = UnifiedRandom::new(12_345);
        terrain_pass::run(&mut vars);
        vars.rand = UnifiedRandom::new(12_345);
        vars
    }

    fn is_sand(world: &World, x: i32, y: i32) -> bool {
        let tile = world.tile(x, y);
        tile.is_active() && tile.block == 53
    }

    /// The manifest's Dunes entry is a fingerprint of the pass's own draws, because the game
    /// reseeds before every pass. Matching it also pins the exact draw count: 112 here.
    #[test]
    fn the_reference_dunes_sample_matches_after_exactly_112_draws() {
        let mut vars = terrain_world();
        let draws = super::run_counted(&mut vars);
        assert_eq!(
            draws, 112,
            "the vanilla delegate consumes 112 draws for seed 12345 (four of them in \
             SetupDungeonGenVarVariables, including the entrance RandomSeed draw)"
        );
        assert_eq!(
            i64::from(vars.rand.next()),
            2_136_736_694,
            "Dunes' manifest entry for the reference world"
        );

        // The pass's side outputs, as a second check that the right sites were chosen.
        assert_eq!(
            vars.num_pyr, 1,
            "the second dune's 0.2775 roll takes a pyramid site"
        );
        assert_eq!(vars.pyr_x[0], 3532);
        assert_eq!(vars.pyr_y[0], 240);
    }

    /// A tile-level check against the real world.
    ///
    /// Terrain writes only dirt and stone, so every sand tile in the reference's two dune bands
    /// starts as this pass's sand (the desert biome later in the pass list is at x 1694..1953).
    /// The reference is the final world, though, so a few columns have had dune sand covered or
    /// extended by later passes: plants, surface caves, and the underground sand patches. The
    /// comparison therefore counts near-surface sand only, within 50 rows of the terrain surface
    /// this pass actually read, and prints the exact percentage it gets.
    #[test]
    fn the_reference_dunes_sand_is_reproduced() {
        let Some(reference) = reference_world() else {
            return;
        };
        let mut vars = terrain_world();
        let terrain_top: Vec<i32> = (0..vars.world.width())
            .map(|x| {
                (0..vars.world.height())
                    .find(|&y| vars.world.tile(x, y).is_active())
                    .unwrap_or(-1)
            })
            .collect();
        super::run_counted(&mut vars);
        let world = &vars.world;

        // The two dune bands the reference records: the first Place's two descriptions
        // (x 2977..3290 with the curve's sparse edges) and the second Place's second description
        // (x 3448..3599).
        let bands = [(2966, 3285), (3454, 3597)];
        let mut all_reference = 0usize;
        let mut all_reproduced = 0usize;
        for (left, right) in bands {
            let mut reference_sand = 0usize;
            let mut reproduced = 0usize;
            for x in left..=right {
                let cutoff = (terrain_top[x as usize] + 50).min(reference.height());
                for y in 0..cutoff {
                    if is_sand(&reference, x, y) {
                        reference_sand += 1;
                        if is_sand(world, x, y) {
                            reproduced += 1;
                        }
                    }
                }
            }
            assert!(
                reference_sand > 1_000,
                "band {left}..{right} should hold real dune sand, found {reference_sand}"
            );
            let recall = reproduced as f64 / reference_sand as f64;
            eprintln!(
                "Dunes band {left}..{right}: reproduced {reproduced}/{reference_sand} = {:.2}%",
                recall * 100.0
            );
            assert!(
                recall > 0.90,
                "only {:.2}% of the reference's {reference_sand} near-surface sand tiles in \
                 {left}..{right} were reproduced ({reproduced})",
                recall * 100.0
            );
            all_reference += reference_sand;
            all_reproduced += reproduced;
        }
        let recall = all_reproduced as f64 / all_reference as f64;
        eprintln!(
            "Dunes bands combined: reproduced {all_reproduced}/{all_reference} = {:.2}%",
            recall * 100.0
        );
        assert!(recall > 0.90, "combined recall {:.2}%", recall * 100.0);

        // Precision the other way: generated sand in the bands must be sand the reference still
        // has. A port that overfilled the dunes would show up here.
        let mut generated_in_bands = 0usize;
        let mut both = 0usize;
        for (left, right) in bands {
            for x in left..=right {
                for y in 0..reference.height() {
                    if is_sand(world, x, y) {
                        generated_in_bands += 1;
                        if is_sand(&reference, x, y) {
                            both += 1;
                        }
                    }
                }
            }
        }
        let precision = both as f64 / generated_in_bands as f64;
        eprintln!(
            "Dunes bands precision: {both}/{generated_in_bands} = {:.2}%",
            precision * 100.0
        );
        assert!(precision > 0.85, "precision {:.2}%", precision * 100.0);

        // Every generated sand tile must be in one of the reference's visible dune bands or in
        // the span the later dungeon built over (x 3290..3460, centred on the reference's
        // dungeon column 3436). The second Place's first description occupies exactly that gap,
        // which is why the reference shows dungeon bricks rather than dune sand there: the
        // reference has 15,057 blue-brick tiles in that span, so this is not a field "where the
        // reference has none".
        let dungeon = (3290, 3460);
        let mut outside_bands = 0usize;
        let mut outside_dungeon = 0usize;
        for x in 0..world.width() {
            for y in 0..world.height() {
                if !is_sand(world, x, y) {
                    continue;
                }
                let in_band = bands.iter().any(|&(l, r)| x >= l && x <= r);
                if in_band {
                    continue;
                }
                outside_bands += 1;
                if !(dungeon.0..=dungeon.1).contains(&x) {
                    outside_dungeon += 1;
                }
            }
        }
        assert!(
            outside_bands > 1_000,
            "the second Place's first description should leave sand in the later dungeon's span"
        );
        assert_eq!(
            outside_dungeon, 0,
            "{outside_dungeon} generated sand tiles are outside both dune bands and the later \
             dungeon's span"
        );
        let mut dungeon_bricks = 0usize;
        for x in dungeon.0..=dungeon.1 {
            for y in 0..world.height() {
                let tile = reference.tile(x, y);
                if tile.is_active() && tile.block == 41 {
                    dungeon_bricks += 1;
                }
            }
        }
        assert!(
            dungeon_bricks > 1_000,
            "the reference's dungeon must occupy the gap the dune sand is under, found \
             {dungeon_bricks} blue-brick tiles"
        );

        // The pass registers the four description rectangles it drew, inflated by 20. Nothing it
        // writes may fall outside them; a stray field elsewhere would break this.
        let bounds = vars
            .structures
            .bounding_box()
            .expect("the pass registers both dune footprints");
        eprintln!(
            "Dune structure footprint: x {}..{} (reference bands 2966..3285 and 3454..3597)",
            bounds.x,
            bounds.right()
        );
        assert_eq!((bounds.x, bounds.right()), (2957, 3619));
        for x in 0..world.width() {
            for y in 0..world.height() {
                if is_sand(world, x, y) {
                    assert!(
                        x >= bounds.x && x < bounds.right(),
                        "sand at ({x}, {y}) is outside the registered dune footprint"
                    );
                }
            }
        }
    }
}

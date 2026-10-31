//! The generation passes, and the machinery that checks them against a real world.
//!
//! A world is built by a hundred and six passes run in a fixed order over shared mutable state.
//! They are not pure and cannot usefully be made so: each one reads what the last left behind,
//! and several write parameters that a pass forty steps later depends on. So they take a single
//! context and mutate it, exactly as the game's do.
//!
//! What makes that tractable is [`super::manifest`]: after every pass the game records what its
//! random generator reads next, and a port that has drawn the same numbers in the same order
//! reads the same value. Passes are therefore added one at a time, each checked before the next
//! is started.
//!
//! One detail of the recording matters more than it looks: `WorldGenerator.RunPass` points the
//! generator back at the world's seed at the *start of every pass* (`WorldGenerator.cs:499`), then
//! samples it once after the pass (`WorldGenerator.cs:515`). `RandNext` is therefore a
//! fingerprint of that one pass's draws, not of the whole prefix. A pass that draws nothing reads
//! the seed's first value every time (the reference world's "Wavy Caves" and "Mud Caves To Grass"
//! both record `143337951`, the first draw for seed 12345), which is how the reseed is visible in
//! the manifest. [`compare_against`] does the same reseed; without it the first pass would start
//! from the state [`super::reset`] left behind and diverge immediately.

use super::manifest::Manifest;
use super::rand::UnifiedRandom;
use super::structure_map::StructureMap;
use crate::world::World;

/// Which side the dungeon was placed on (`WorldGen.cs:11402-11405`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonSide {
    Left,
    Right,
}

/// Everything the passes share.
///
/// The game keeps this as a hundred and twenty static fields on `GenVars`. Gathering them here
/// changes nothing about how the passes work — they still read and write freely — but it means
/// the whole of a generation's state can be handed around, compared, and printed.
///
/// The fields below are the ones [`super::reset`] fills in. They are kept flat and in the game's
/// own names (snake-cased) rather than grouped, so a transcribed pass can be read beside its
/// vanilla original without translating a hierarchy.
pub struct GenVars {
    /// The generator the current pass draws from.
    ///
    /// Reseeded from [`Self::seed`] before every pass, exactly as `WorldGenerator.RunPass` does.
    pub rand: UnifiedRandom,
    /// The seed every pass is restarted from (`Main.ActiveWorldFileData.Seed`).
    pub seed: i32,
    /// The world being built.
    pub world: World,

    /// `GenVars.jungleHut`: the tile type a jungle hut is built from, already mapped.
    pub jungle_hut: u16,
    /// `GenVars.hellChestItem`: the five items distributed among the underworld chests.
    pub hell_chest_item: [i32; 5],
    /// The four pre-hardmode ore tiers, and the bar each one smelts into.
    pub copper: u16,
    pub iron: u16,
    pub silver: u16,
    pub gold: u16,
    pub copper_bar: u16,
    pub iron_bar: u16,
    pub silver_bar: u16,
    pub gold_bar: u16,
    /// `GenVars.crimsonLeft`: whether the crimson generated on the left of the world.
    pub crimson_left: bool,
    /// Where the first dungeon went.
    pub dungeon_side: DungeonSide,
    /// `GenVars.CurrentDungeonGenVars.dungeonLocation`: the dungeon's column.
    pub dungeon_location: i32,
    /// The jungle's centre column (`GenVars.jungleOriginX`).
    pub jungle_origin_x: i32,
    /// The snow biome's bounds (`GenVars.snowOriginLeft`/`Right`).
    pub snow_origin_left: i32,
    pub snow_origin_right: i32,
    /// Where the sand of each beach ends (`GenVars.leftBeachEnd`/`rightBeachStart`).
    pub left_beach_end: i32,
    pub right_beach_start: i32,
    /// The beach constants later passes read back.
    pub beach_borders_width: i32,
    pub beach_sand_random_center: i32,
    pub beach_sand_random_width_range: i32,
    pub beach_sand_dungeon_extra_width: i32,
    pub beach_sand_jungle_extra_width: i32,
    pub ocean_water_start_random_min: i32,
    pub ocean_water_start_random_max: i32,
    pub ocean_water_forced_jungle_length: i32,
    pub evil_biome_beach_avoidance: i32,
    pub evil_biome_avoidance_mid_fixer: i32,
    pub lakes_beach_avoidance: i32,
    pub small_holes_beach_avoidance: i32,
    pub surface_caves_beach_avoidance: i32,
    pub surface_caves_beach_avoidance2: i32,
    pub sky_lakes: i32,
    /// The layer boundaries Terrain computes, as the game keeps them (doubles, not row ints).
    pub world_surface_low: f64,
    pub world_surface: f64,
    pub world_surface_high: f64,
    pub rock_layer_low: f64,
    pub rock_layer: f64,
    pub rock_layer_high: f64,
    pub water_line: i32,
    pub lava_line: i32,
    pub remix_mushroom_layer_low: i32,
    pub remix_mushroom_layer_high: i32,
    pub remix_surface_layer_low: i32,
    pub remix_surface_layer_high: i32,
    /// `GenVars.structures`: the shared structure map every biome adds to and checks against.
    pub structures: StructureMap,
    /// `GenVars.PyrX`/`PyrY`/`numPyr`: the pyramid sites the Dunes pass records.
    pub pyr_x: Vec<i32>,
    pub pyr_y: Vec<i32>,
    pub num_pyr: usize,
}

impl GenVars {
    pub fn new(seed: i32, world: World) -> Self {
        Self {
            rand: UnifiedRandom::new(seed),
            seed,
            world,
            jungle_hut: 0,
            hell_chest_item: [0; 5],
            copper: 7,
            iron: 6,
            silver: 9,
            gold: 8,
            copper_bar: 20,
            iron_bar: 22,
            silver_bar: 21,
            gold_bar: 19,
            crimson_left: false,
            dungeon_side: DungeonSide::Left,
            dungeon_location: 0,
            jungle_origin_x: 0,
            snow_origin_left: 0,
            snow_origin_right: 0,
            left_beach_end: 0,
            right_beach_start: 0,
            beach_borders_width: 0,
            beach_sand_random_center: 0,
            beach_sand_random_width_range: 0,
            beach_sand_dungeon_extra_width: 0,
            beach_sand_jungle_extra_width: 0,
            ocean_water_start_random_min: 0,
            ocean_water_start_random_max: 0,
            ocean_water_forced_jungle_length: 0,
            evil_biome_beach_avoidance: 0,
            evil_biome_avoidance_mid_fixer: 0,
            lakes_beach_avoidance: 0,
            small_holes_beach_avoidance: 0,
            surface_caves_beach_avoidance: 0,
            surface_caves_beach_avoidance2: 0,
            sky_lakes: 0,
            world_surface_low: 0.0,
            world_surface: 0.0,
            world_surface_high: 0.0,
            rock_layer_low: 0.0,
            rock_layer: 0.0,
            rock_layer_high: 0.0,
            water_line: 0,
            lava_line: 0,
            remix_mushroom_layer_low: 0,
            remix_mushroom_layer_high: 0,
            remix_surface_layer_low: 0,
            remix_surface_layer_high: 0,
            structures: StructureMap::new(),
            pyr_x: Vec::new(),
            pyr_y: Vec::new(),
            num_pyr: 0,
        }
    }
}

/// One generation pass.
pub struct Pass {
    /// The name the world file records it under. This is how a pass is matched to its entry in
    /// the manifest, so it has to be the game's own spelling.
    pub name: &'static str,
    pub run: fn(&mut GenVars),
}

/// Every pass this port implements, in the order the game runs them.
///
/// The order is taken from the manifest of a real world rather than from the order the game
/// registers them in, because the two disagree in places and the manifest is what actually ran.
pub const PASSES: &[Pass] = &[
    Pass {
        name: "Terrain",
        run: super::terrain_pass::run,
    },
    Pass {
        name: "Dunes",
        run: super::dunes::run,
    },
];

/// Where a port stopped agreeing with a reference world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub index: usize,
    pub name: String,
    pub expected: i64,
    pub got: i64,
}

/// What a comparison found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// How many passes were run and checked.
    pub checked: usize,
    pub first_divergence: Option<Divergence>,
}

/// Run the implemented passes against a reference world's record.
///
/// Only the passes this port has are run, and only the reference entries for those are checked —
/// which is what lets the port grow one pass at a time instead of having to be finished before it
/// says anything.
///
/// The world's dimensions, evil side and seed are taken from the reference, then
/// [`super::reset`] runs once because no pass is a substitute for it. Before each pass the
/// generator is reset to the seed, mirroring `WorldGenerator.RunPass`.
pub fn compare_against(manifest: &Manifest, seed: i32, reference: &World) -> Outcome {
    let mut world = World::empty(
        reference.width(),
        reference.height(),
        reference.name.clone(),
    );
    world.crimson = reference.crimson;
    world.seed_text = reference.seed_text.clone();
    let mut vars = GenVars::new(seed, world);
    super::reset::reset(&mut vars, Some(reference.crimson));
    compare_with(manifest, seed, &mut vars, PASSES)
}

/// The comparison itself, with the pass registry injectable so its tests can use a stand-in.
fn compare_with(manifest: &Manifest, seed: i32, vars: &mut GenVars, passes: &[Pass]) -> Outcome {
    let mut checked = 0;
    for (index, expected) in manifest.ran().enumerate() {
        let Some(pass) = passes.get(index) else {
            break;
        };
        if pass.name != expected.name {
            return Outcome {
                checked,
                first_divergence: Some(Divergence {
                    index,
                    name: format!("{} (we have {:?} here)", expected.name, pass.name),
                    expected: expected.rand_next,
                    got: -1,
                }),
            };
        }
        // `WorldGenerator.RunPass` starts every pass from the world's seed
        // (`WorldGenerator.cs:499`), so a port that does not reseed here is comparing the wrong
        // stream from the first pass onward.
        vars.rand = UnifiedRandom::new(seed);
        (pass.run)(vars);
        // The game samples the generator here, and the sample itself advances it — so a port has
        // to take one at the same point or it diverges from the very first pass.
        let got = i64::from(vars.rand.next());
        if got != expected.rand_next {
            return Outcome {
                checked,
                first_divergence: Some(Divergence {
                    index,
                    name: expected.name.clone(),
                    expected: expected.rand_next,
                    got,
                }),
            };
        }
        checked += 1;
    }
    Outcome {
        checked,
        first_divergence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_of(entries: &[(&str, i64)]) -> Manifest {
        let body: Vec<String> = entries
            .iter()
            .map(|(name, next)| {
                format!(
                    "{{\"Name\":\"{name}\",\"DurationMs\":1,\"RandNext\":{next},\"Hash\":null,\"Skipped\":false}}"
                )
            })
            .collect();
        Manifest::parse(&format!(
            "{{\"GenPassResults\":[{}],\"Version\":\"test\",\"FinalHash\":null}}",
            body.join(",")
        ))
        .expect("a manifest")
    }

    /// With nothing implemented, nothing is checked and nothing is claimed.
    #[test]
    fn an_empty_port_checks_nothing() {
        let manifest = manifest_of(&[("Terrain", 1), ("Dunes", 2)]);
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &[]);
        assert_eq!(outcome.checked, 0);
        assert!(outcome.first_divergence.is_none());
    }

    /// The sample the game takes after each pass advances the generator, so a port that runs a
    /// pass drawing nothing still has to match the reference's *first* value.
    #[test]
    fn the_reference_value_is_the_draw_after_the_pass() {
        // What a pass that draws nothing leaves the generator reading.
        let mut r = UnifiedRandom::new(42);
        let after_nothing = i64::from(r.next());

        let manifest = manifest_of(&[("Terrain", after_nothing)]);
        fn nothing(_: &mut GenVars) {}
        let passes = [Pass {
            name: "Terrain",
            run: nothing,
        }];
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &passes);
        assert_eq!(outcome.checked, 1);
        assert!(outcome.first_divergence.is_none());
    }

    /// Each pass starts from the seed rather than from whatever the last one left, which is what
    /// `WorldGenerator.RunPass` does. Two passes drawing the same number therefore record the
    /// same value, not two values in sequence.
    #[test]
    fn every_pass_starts_from_the_seed() {
        fn one_draw(vars: &mut GenVars) {
            vars.rand.next_max(10);
        }
        let mut r = UnifiedRandom::new(42);
        r.next_max(10);
        let after_one_draw = i64::from(r.next());
        let manifest = manifest_of(&[("First", after_one_draw), ("Second", after_one_draw)]);
        let passes = [
            Pass {
                name: "First",
                run: one_draw,
            },
            Pass {
                name: "Second",
                run: one_draw,
            },
        ];
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &passes);
        assert_eq!(outcome.checked, 2);
        assert!(
            outcome.first_divergence.is_none(),
            "the second pass did not start from the seed"
        );
    }

    /// A pass that draws the wrong number of values is caught, and named.
    #[test]
    fn a_wrong_number_of_draws_is_caught() {
        fn two_draws(vars: &mut GenVars) {
            vars.rand.next_max(10);
            vars.rand.next_max(10);
        }
        let mut r = UnifiedRandom::new(42);
        r.next_max(10); // what the reference pass drew
        let expected = i64::from(r.next());
        let manifest = manifest_of(&[("Terrain", expected)]);
        let passes = [Pass {
            name: "Terrain",
            run: two_draws,
        }];
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &passes);
        let diverged = outcome
            .first_divergence
            .expect("an extra draw must be caught");
        assert_eq!(diverged.name, "Terrain");
        assert_eq!(diverged.index, 0);
        assert_ne!(
            diverged.got, expected,
            "an extra draw has to change what comes next, or the oracle is worthless"
        );
    }
}

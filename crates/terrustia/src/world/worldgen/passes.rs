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
    /// `GenVars.tunnelX`/`numTunnels`: the surface tunnels' centre columns.
    pub tunnel_x: Vec<i32>,
    pub num_tunnels: usize,
    /// `GenVars.mCaveX`/`mCaveY`/`numMCaves`: the mountain caves' entry columns and rows.
    pub mount_cave_x: Vec<i32>,
    pub mount_cave_y: Vec<i32>,
    pub num_mount_caves: usize,
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
            tunnel_x: vec![0; 50],
            num_tunnels: 0,
            mount_cave_x: vec![0; 30],
            mount_cave_y: vec![0; 30],
            num_mount_caves: 0,
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
    Pass {
        name: "Ocean Sand",
        run: super::ocean_sand::run,
    },
    Pass {
        name: "Sand Patches",
        run: super::sand_patches::run,
    },
    Pass {
        name: "Tunnels",
        run: super::tunnels::run,
    },
    Pass {
        name: "Mount Caves",
        run: super::mount_caves::run,
    },
    Pass {
        name: "Dirt Wall Backgrounds",
        run: super::dirt_wall_backgrounds::run,
    },
    Pass {
        name: "Rocks In Dirt",
        run: super::rocks_in_dirt::run,
    },
    Pass {
        name: "Dirt In Rocks",
        run: super::dirt_in_rocks::run,
    },
    Pass {
        name: "Clay",
        run: super::clay::run,
    },
    Pass {
        name: "Small Holes",
        run: super::small_holes::run,
    },
    Pass {
        name: "Dirt Layer Caves",
        run: super::dirt_layer_caves::run,
    },
    Pass {
        name: "Rock Layer Caves",
        run: super::rock_layer_caves::run,
    },
    Pass {
        name: "Surface Caves",
        run: super::surface_caves::run,
    },
    Pass {
        name: "Wavy Caves",
        run: super::wavy_caves::run,
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
    /// How many implemented passes were run and checked.
    pub checked: usize,
    /// How many of those read the generator exactly as the reference did.
    pub matches: usize,
    /// Every pass that did not, in order.
    ///
    /// The comparison does not stop at the first one. Each pass is reseeded from the world seed,
    /// so a pass's `RandNext` says about *that* pass alone, and a pass that matches is proven even
    /// when its neighbour does not. That is what makes a tile-independent pass checkable while the
    /// terrain before it is still wrong.
    pub divergences: Vec<Divergence>,
}

impl Outcome {
    /// The first pass that stopped matching, if any.
    pub fn first_divergence(&self) -> Option<&Divergence> {
        self.divergences.first()
    }
}

/// Run the first `count` implemented passes on a fresh world and hand back what they built.
///
/// This is the same driver [`compare_against`] uses, minus the comparison, and it exists because a
/// pass whose *draw count* depends on the tiles it reads can only be debugged by looking at those
/// tiles. The manifest says whether a pass is wrong; this says what it actually made.
///
/// `count` beyond [`PASSES`] simply stops at the end of the registry.
pub fn run_passes(
    seed: i32,
    width: i32,
    height: i32,
    name: &str,
    crimson: bool,
    count: usize,
) -> World {
    let mut world = World::empty(width, height, name);
    world.crimson = crimson;
    let mut vars = GenVars::new(seed, world);
    super::reset::reset(&mut vars, Some(crimson));
    for pass in PASSES.iter().take(count) {
        vars.rand = UnifiedRandom::new(seed);
        (pass.run)(&mut vars);
    }
    vars.world
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
    let mut matches = 0;
    let mut divergences = Vec::new();
    for (index, expected) in manifest.ran().enumerate() {
        let Some(pass) = passes.get(index) else {
            break;
        };
        if pass.name != expected.name {
            // A name out of place means every later index is lined up against the wrong entry, so
            // there is nothing further worth comparing. This is the one case that has to stop.
            divergences.push(Divergence {
                index,
                name: format!("{} (we have {:?} here)", expected.name, pass.name),
                expected: expected.rand_next,
                got: -1,
            });
            return Outcome {
                checked,
                matches,
                divergences,
            };
        }
        // `WorldGenerator.RunPass` starts every pass from the world's seed
        // (`WorldGen.cs:499`), so a port that does not reseed here is comparing the wrong
        // stream from the first pass onward.
        vars.rand = UnifiedRandom::new(seed);
        (pass.run)(vars);
        // The game samples the generator here, and the sample itself advances it — so a port has
        // to take one at the same point or it diverges from the very first pass.
        let got = i64::from(vars.rand.next());
        checked += 1;
        if got == expected.rand_next {
            matches += 1;
        } else {
            divergences.push(Divergence {
                index,
                name: expected.name.clone(),
                expected: expected.rand_next,
                got,
            });
        }
    }
    Outcome {
        checked,
        matches,
        divergences,
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
        assert!(outcome.first_divergence().is_none());
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
        assert!(outcome.first_divergence().is_none());
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
            outcome.first_divergence().is_none(),
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
            .first_divergence()
            .expect("an extra draw must be caught");
        assert_eq!(diverged.name, "Terrain");
        assert_eq!(diverged.index, 0);
        assert_ne!(
            diverged.got, expected,
            "an extra draw has to change what comes next, or the oracle is worthless"
        );
    }

    /// Each pass is reseeded, so a wrong pass does not invalidate the one after it and the
    /// comparison keeps going. Without this, one broken pass hides every later pass that is
    /// already right, which is exactly the state the port is in while the terrain is still off:
    /// a pass that draws once per column regardless of tiles cannot be checked at all if the
    /// walk stops at the pass before it.
    #[test]
    fn a_diverging_pass_does_not_stop_the_later_ones_being_checked() {
        fn one_draw(vars: &mut GenVars) {
            vars.rand.next_max(10);
        }
        fn wrong(vars: &mut GenVars) {
            vars.rand.next_max(10);
            vars.rand.next_max(10);
        }
        // The reference's own values, each taken from the stream position that a correct pass
        // would leave the generator at. The middle entry is what a pass drawing three numbers
        // reads, while our stand-in draws two, so only that one disagrees.
        let mut r = UnifiedRandom::new(42);
        r.next_max(10);
        let after_one = i64::from(r.next());
        let mut r = UnifiedRandom::new(42);
        for _ in 0..3 {
            r.next_max(10);
        }
        let after_three = i64::from(r.next());
        let manifest = manifest_of(&[
            ("First", after_one),
            ("Second", after_three),
            ("Third", after_one),
        ]);
        let passes = [
            Pass {
                name: "First",
                run: one_draw,
            },
            Pass {
                name: "Second",
                run: wrong,
            },
            Pass {
                name: "Third",
                run: one_draw,
            },
        ];
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &passes);
        assert_eq!(outcome.checked, 3);
        assert_eq!(outcome.matches, 2);
        assert_eq!(outcome.divergences.len(), 1);
        assert_eq!(outcome.divergences[0].name, "Second");
    }

    /// A registry that is out of step with the manifest stops rather than comparing every later
    /// pass against the wrong entry.
    #[test]
    fn a_misordered_registry_stops_instead_of_mismatching_everything() {
        let manifest = manifest_of(&[("First", 1), ("Second", 2)]);
        let passes = [
            Pass {
                name: "Second",
                run: |_: &mut GenVars| {},
            },
            Pass {
                name: "First",
                run: |_: &mut GenVars| {},
            },
        ];
        let mut vars = GenVars::new(42, World::empty(600, 400, "ours"));
        let outcome = compare_with(&manifest, 42, &mut vars, &passes);
        assert_eq!(outcome.checked, 0);
        assert_eq!(outcome.divergences.len(), 1);
        assert!(outcome.divergences[0].name.contains("we have"),);
    }

    /// The stepped reference worlds, found the same way `dunes` finds its reference: by walking
    /// up from the crate, because a worktree checkout is not at a fixed depth.
    ///
    /// Each `stopNNN.wld` is an ordinary dedicated server run with its pass list truncated to
    /// NNN, so it is vanilla's world exactly as it stands after that many passes. `.scratch` is
    /// not in the repository and making one needs a local Terraria install, so an absent directory
    /// skips rather than fails - the same arrangement `vanilla_tests::reference_world` uses.
    fn stepped_references() -> Vec<(usize, std::path::PathBuf)> {
        let mut directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
        let dir = loop {
            let candidate = directory.join(".scratch/oracle");
            if candidate.is_dir() {
                break candidate;
            }
            if !directory.pop() {
                return Vec::new();
            }
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut found: Vec<(usize, std::path::PathBuf)> = entries
            .filter_map(Result::ok)
            .map(|entry| (entry.path(), entry.file_name()))
            .filter_map(|(path, name)| {
                let text = name.to_string_lossy().into_owned();
                let digits = text
                    .strip_prefix("stop")?
                    .strip_suffix(".wld")?
                    .parse::<usize>()
                    .ok()?;
                Some((digits, path))
            })
            .collect();
        found.sort();
        found
    }

    /// Reproduce vanilla's world, tile for tile, at every step it can.
    ///
    /// This is the strongest check the port has, and it is deliberately not the same check as the
    /// manifest comparison above. `RandNext` proves a pass drew the right numbers; two passes can
    /// draw identically while painting different tiles, because a runner's decision to skip a tile
    /// feeds back into which tiles its successors read. Three separate bugs were found only here:
    /// `Main.worldSurface` (331) being passed where vanilla reads `GenVars.worldSurface` (267),
    /// `TileRunner`'s default speeds, and `(int)double.NaN` being `int.MinValue` in .NET rather
    /// than the 0 Rust's `as` saturates to. None of them moved a draw count.
    #[test]
    fn every_stepped_reference_world_is_reproduced_tile_for_tile() {
        use crate::world::wld;

        let steps = stepped_references();
        if steps.is_empty() {
            eprintln!(
                "no stepped reference worlds in .scratch/oracle; \
                 skipping the tile-for-tile check (see .scratch/oracle/README.md)"
            );
            return;
        }
        // One walk, compared after each pass, rather than re-running the whole prefix per step.
        // The passes are cumulative and the cheap ones are not cheap - Sand Patches draws 2.1
        // million times and Rock Layer Caves 6.8 million - so a per-step `run_passes` would repeat
        // all of them for every file in the directory.
        let total = steps.len();
        let step: std::collections::BTreeMap<usize, std::path::PathBuf> = steps
            .into_iter()
            .filter(|(count, _)| *count <= PASSES.len())
            .collect();
        let Some((_, first_path)) = step.iter().next() else {
            // The same arrangement as `vanilla_tests::reference_world`: the files need a local
            // install to make, so their absence is reported and skipped rather than failed.
            eprintln!(
                "no stepped reference within the {} registered passes ({total} in \
                 .scratch/oracle); skipping",
                PASSES.len()
            );
            return;
        };
        let shape = match wld::load(first_path) {
            Ok(world) => world,
            Err(error) => panic!("{} unreadable: {error:?}", first_path.display()),
        };
        let widest = step.keys().next_back().copied().unwrap_or(0);

        let mut vars = GenVars::new(
            12_345,
            World::empty(shape.width(), shape.height(), &shape.name),
        );
        crate::world::worldgen::reset::reset(&mut vars, Some(shape.crimson));
        let mut checked = 0usize;
        for (index, pass) in PASSES.iter().enumerate() {
            vars.rand = UnifiedRandom::new(12_345);
            (pass.run)(&mut vars);
            let Some(path) = step.get(&(index + 1)) else {
                if index + 1 >= widest {
                    break;
                }
                continue;
            };
            let reference = match wld::load(path) {
                Ok(world) => world,
                Err(error) => panic!("{} unreadable: {error:?}", path.display()),
            };
            let mut differences: Vec<String> = Vec::new();
            for column in 0..reference.width() {
                for row in 0..reference.height() {
                    let a = vars.world.tile(column, row);
                    let b = reference.tile(column, row);
                    if (a.is_active() != b.is_active()
                        || a.block != b.block
                        || a.wall != b.wall
                        || a.liquid != b.liquid
                        || a.liquid_kind != b.liquid_kind)
                        && differences.len() < 8
                    {
                        differences.push(format!(
                            "({column},{row}) ours active={} block={} wall={} liquid={} | \
                                 theirs active={} block={} wall={} liquid={}",
                            a.is_active(),
                            a.block,
                            a.wall,
                            a.liquid,
                            b.is_active(),
                            b.block,
                            b.wall,
                            b.liquid,
                        ));
                    }
                }
            }
            assert!(
                differences.is_empty(),
                "after {} passes ({:?}) our world differs from vanilla's {}: \
                 first differences:\n  {}",
                index + 1,
                pass.name,
                path.display(),
                differences.join("\n  ")
            );
            checked += 1;
            if index + 1 >= widest {
                break;
            }
        }
        assert_eq!(
            checked,
            step.len(),
            "only {checked} of {} stepped references were reached",
            step.len()
        );
    }
}

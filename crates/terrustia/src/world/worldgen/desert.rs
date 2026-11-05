//! The Underground Desert: a sand basin over a walled hive of chambers.
//!
//! Transcribed from `DesertBiome`
//! (`.scratch/decompiled/Terraria.GameContent.Biomes/DesertBiome.cs`, 72 lines) and the
//! `Terraria.GameContent.Biomes.Desert` sub-namespace it dispatches into: `DesertDescription`
//! (133), `SandMound` (76), `DesertHive` (507) and `PitEntrance` (73). The fifteenth and last of
//! the `MicroBiome` classes.
//!
//! `micro_biomes.rs` deferred this as "mostly a *dispatcher* to a whole separate sub-namespace
//! ... none of those classes are counted in `plan.md`'s own 4,240-line/15-class total, and porting
//! them is realistically its own Tier-2-sized item". That was right about the size, and this is
//! that item.
//!
//! # What a player finds
//!
//! A wide bowl of sand at the surface, and under it the Underground Desert proper: a cavity of
//! sandstone and hardened sand walled in sandstone brick, its interior carved into rounded chambers
//! by a field of metaballs, with lava at the bottom, cacti and pots on the ledges, and a pit at the
//! centre leading down into it.
//!
//! # How the hive is shaped
//!
//! Not by carving tunnels. A grid of `BlockColumnCount` x `BlockRowCount` cells is seeded at random
//! inside an ellipse, flood-grouped into clusters of at least four cells, and each cluster's cells
//! are jittered off their grid points. Every tile in the hive then sums `1 / distance^2` to the
//! blocks of every nearby cluster; the two strongest cluster sums decide, by four thresholds, if
//! that tile is open air, sandstone brick, hardened sand or plain sand. That is a metaball field,
//! and it is why the chambers look blown rather than dug.
//!
//! # Disclosed narrowings
//!
//! * All four entrance styles are ported - `ChambersEntrance`, `AnthillEntrance`,
//!   `LarvaHoleEntrance` and `PitEntrance` - which took `Shapes.Tail` (a tapered line plot over
//!   `Utils.PlotTileTale`), `ModShapes.OuterOutline`, `Modifiers.NotInShape` and `Actions.Clear`
//!   into [`super::genpipe`]. Each is exercised by its own test, because the seeded end-to-end
//!   run only ever takes one of the four.
//! * `Tile.SmoothSlope` is dropped, as everywhere else in this generator: no slopes are written,
//!   so there is nothing to smooth. This costs the chamber edges their bevel.
//! * The `remix`, `drunk`, `tenthAnniversary` and `surfaceIsDesert` branches are not modelled.
//! * `GenVars.UndergroundDesertLocation` is returned rather than written to a global.

use terrustia_proto::{Liquid, Tile, TileFlags, tile_solid};

use super::genpipe::{Action, Ctx, Link, Shape, chain, gen_shape, gen_shape_out};
use super::layout::Layout;
use super::passes::{DungeonSide, GenVars};
use super::rand::UnifiedRandom;
use super::structure_map::{Rect, StructureMap};
use crate::world::World;

/// Sand.
const SAND: u16 = 53;
/// Hardened Sand.
const HARDENED_SAND: u16 = 397;
/// Sandstone.
const SANDSTONE: u16 = 396;
/// Desert Fossil.
const FOSSIL: u16 = 404;
/// Sandstone wall, the hive's own.
const HIVE_WALL: u16 = 187;
/// Hardened sand wall, the outer shell.
const SHELL_WALL: u16 = 216;

/// `DesertDescription.DefaultBlockScale`.
const BLOCK_SCALE: (f64, f64) = (4.0, 2.0);

/// `Terraria.Utilities.FastRandom`, a 48-bit LCG. Needed because `DesertHive` seeds one from the
/// world seed and derives per-tile streams from it with `WithModifier`, which is what makes the
/// sand-versus-hardened-sand speckle stable for a given world rather than a function of draw order.
#[derive(Clone, Copy)]
pub struct FastRandom {
    seed: u64,
}

impl FastRandom {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    fn next_seed(seed: u64) -> u64 {
        seed.wrapping_mul(25_214_903_917).wrapping_add(11) & 0xFFFF_FFFF_FFFF
    }

    pub fn with_modifier(&self, modifier: u64) -> Self {
        Self {
            seed: Self::next_seed(modifier) ^ self.seed,
        }
    }

    /// `WithModifier(int x, int y)`, arithmetic kept in the same widths vanilla uses.
    pub fn with_xy(&self, x: i32, y: i32) -> Self {
        let a = (i64::from(x) + 2_654_435_769i64 + (i64::from(y) << 6)) as u64;
        self.with_modifier(a.wrapping_add((y as u64) >> 2))
    }

    fn next_bits(&mut self, bits: u32) -> i32 {
        self.seed = Self::next_seed(self.seed);
        (self.seed >> (48 - bits)) as i32
    }

    pub fn next_double(&mut self) -> f64 {
        f64::from(self.next_bits(32) as f32 * 4.656_613e-10f32)
    }

    pub fn next(&mut self, max: i32) -> i32 {
        debug_assert!(max >= 0, "the game throws on a negative maximum");
        // `FastRandom.cs:65` tests the power-of-two case with `(max & -max) == max`, which is true
        // for zero, and the branch then multiplies by `NextBits(31)`: a `Next(0)` consumes a draw
        // and returns 0. Returning early for zero would desync the stream from vanilla's.
        if (max & max.wrapping_neg()) == max {
            return ((i64::from(max) * i64::from(self.next_bits(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next_bits(31);
            let val = bits % max;
            if bits - val + (max - 1) >= 0 {
                return val;
            }
        }
    }
}

/// `Tile.ResetToType`: become this block, active, with the frame data reset.
///
/// The frame reset is the part that matters. Overwriting a frame-important tile (a pot, a chest)
/// with plain sandstone while leaving its old `frame_x`/`frame_y` behind leaves the running world
/// holding frames that a save drops, because the format only stores them for frame-important
/// types. That is 17 tiles differing across a save, which is how this was found.
/// `Tile.ResetToType` (`Tile.cs:250-258`): liquid to nothing, the header back to plain active, no
/// slope, no paint, frame cleared, then the type. Clearing the liquid is part of it, and the
/// `.wld` comparison reads the amount, so forgetting it leaves water in cells vanilla drains.
fn reset_to_type(t: &mut Tile, block: u16) {
    t.liquid = 0;
    t.block = block;
    t.flags = TileFlags(t.flags.0 | TileFlags::ACTIVE);
    t.slope = 0;
    if terrustia_proto::tile_sets::frame_important(block) {
        t.frame_x = 0;
        t.frame_y = 0;
    } else {
        t.frame_x = -1;
        t.frame_y = -1;
    }
}

/// `DesertDescription`.
pub struct Description {
    pub combined: Rect,
    pub desert: Rect,
    pub hive: Rect,
    pub columns: i32,
    pub rows: i32,
    surface: Vec<i32>,
    surface_x: i32,
}

impl Description {
    fn surface_len(&self) -> i32 {
        self.surface.len() as i32
    }

    fn surface_at(&self, x: i32) -> i32 {
        if self.surface.is_empty() {
            return 0;
        }
        let i = (x - self.surface_x).clamp(0, self.surface.len() as i32 - 1);
        self.surface[i as usize]
    }
}

/// `SurfaceMap.FromArea` (`Terraria.GameContent.Biomes.Desert/SurfaceMap.cs:44-74`) plus the
/// constructor's clamp (`:22-42`), returning the heights, the average and the bottom.
///
/// Two details matter and the earlier version of this function had neither. A cloud tile does not
/// end the scan, it *reopens* it: vanilla sets the flag back to false on a cloud and then the
/// following `if (!flag)` immediately pushes the column's height to `maxTilesY / 2 + 50`, so a
/// column under a cloud reads as open ground until another active tile lands lower. And the
/// bottom is clamped by `Main.worldSurface - 10` (`:35-37`), which caps how deep a desert can be
/// sited on a world whose hills dip low. Without that clamp the hive sits below where vanilla
/// puts it, and the sand the mound lays down then ends up over air, which is how ~30k cells came
/// out as hardened sand instead of sand in the pass-20 tile diff.
fn surface_map(world: &World, start_x: i32, width: i32) -> (Vec<i32>, f64, i32) {
    let half = world.height() / 2;
    let mut heights = Vec::with_capacity(width.max(0) as usize);
    let mut sum = 0i64;
    let mut bottom = 0;
    for i in start_x..start_x + width {
        let mut found = false;
        let mut height = 0;
        for j in 50..50 + half {
            let t = world.tile(i, j);
            if t.is_active() {
                if is_cloud(t.block) {
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
        sum += i64::from(height);
        bottom = bottom.max(height);
        heights.push(height);
    }
    let average = if heights.is_empty() {
        0.0
    } else {
        sum as f64 / heights.len() as f64
    };
    let world_surface = f64::from(world.surface);
    let bottom = if f64::from(bottom) > world_surface - 10.0 {
        i32::from(world.surface) - 10
    } else {
        bottom
    };
    (heights, average, bottom)
}

/// `TileID.Sets.Clouds` (`TileID.cs:197`).
fn is_cloud(block: u16) -> bool {
    matches!(block, 189 | 196 | 460 | 717 | 718 | 719)
}

/// `DesertDescription.CreateFromPlacement` (`:43-84`). `None` where vanilla returns `Invalid`.
pub fn describe(
    world: &World,
    rand: &mut UnifiedRandom,
    origin: (i32, i32),
    skip_tile_check: bool,
) -> Option<Description> {
    let scale = f64::from(world.width()) / 4200.0;
    let columns = (80.0 * scale) as i32;
    let rows = ((rand.next_double() * 0.5 + 1.5) * 170.0 * scale) as i32;
    let width = (BLOCK_SCALE.0 * f64::from(columns)) as i32;
    let height = (BLOCK_SCALE.1 * f64::from(rows)) as i32;
    let x = origin.0 - width / 2;

    let (heights, average, bottom) = surface_map(world, x - 5, width + 10);
    // `DesertDescription.Cleanup` is `RowHasInvalidTiles`, and `skipDesertTileCheck` short-circuits
    // it before the scan (`DesertDescription.cs:88`): once the desert has failed twice into a
    // flip of the dungeon side, every later site is accepted whatever runs through its floor row.
    if !skip_tile_check {
        for i in x..x + width {
            let t = world.tile(i, bottom).block;
            if matches!(t, 59 | 60 | 161 | 147) {
                return None;
            }
        }
    }

    // Vanilla is `(int)(Average + (double)Bottom) / 2`: the cast binds before the divide, so the
    // sum is truncated once and then divided as integers.
    let top = (average + f64::from(bottom)) as i32 / 2;
    let y = top + rand.next_range(40, 60);
    Some(Description {
        combined: Rect::new(x, top, width, y + height - top),
        hive: Rect::new(x, y, width, height),
        desert: Rect::new(x, top, width, y + height / 2 - top),
        columns,
        rows,
        surface: heights,
        surface_x: x - 5,
    })
}

/// `SandMound.Place` (`:7-50`): scoop the surface out and fill the bowl with sand.
fn sand_mound(world: &mut World, d: &Description, rand: &mut UnifiedRandom) {
    let mut bowl = d.desert;
    bowl.height = d.desert.height.min(d.hive.height / 2);
    let mut lower = d.desert;
    lower.y = bowl.bottom();
    lower.height = (d.desert.bottom() - bowl.bottom()).max(0);

    let mut wobble_a = 0;
    let mut wobble_b = 0;
    for i in -5..bowl.width + 5 {
        let mut across = (f64::from(i + 5) / f64::from(bowl.width + 10)).abs() * 2.0 - 1.0;
        across = across.clamp(-1.0, 1.0);
        if i % 3 == 0 {
            wobble_a += rand.next_range(-1, 2);
            wobble_a = wobble_a.clamp(-10, 10);
        }
        wobble_b += rand.next_range(-1, 2);
        wobble_b = wobble_b.clamp(-10, 10);

        let curve = (1.0 - across * across * across * across).max(0.0).sqrt();
        let floor = bowl.bottom() - (curve * f64::from(bowl.height)) as i32 + wobble_a;
        if across.abs() < 1.0 {
            // `Utils.UnclampedSmoothStep(0.5, 0.8, |across|)`.
            let t = (across.abs() - 0.5) / 0.3;
            let smooth = t * t * (3.0 - 2.0 * t);
            let cubed = smooth * smooth * smooth;
            let mut ceiling = 10 + (f64::from(bowl.y) - cubed * 20.0) as i32 + wobble_b;
            ceiling = ceiling.min(floor);
            for j in d.surface_at(i + d.desert.x) - 1..ceiling {
                let x = i + d.desert.x;
                if world.in_bounds(x, j) {
                    world.set_tile(x, j, Tile::AIR);
                }
            }
        }
        // `PlaceSandColumn`.
        let height = lower.bottom() - floor;
        for j in (floor..floor + height).rev() {
            if !world.in_bounds(i + d.desert.x, j) {
                continue;
            }
            let mut t = world.tile(i + d.desert.x, j);
            t.liquid = 0;
            reset_to_type(&mut t, SAND);
            world.set_tile(i + d.desert.x, j, t);
            // `SandMound.PlaceSandColumn` ends every tile of the column with
            // `SquareWallFrame(startX, num2)` (`SandMound.cs:73`). Most of the time that is free,
            // because the cell has no wall yet: `WallFrame` returns at `Framing.cs:346-351`. It is
            // not free where the surface already carried a wall, and it is one of the terms the
            // desert's draw total is made of.
            let _ = super::tile_runner::square_wall_frame(&*world, rand, i + d.desert.x, j);
        }
    }
}

/// One jittered grid cell of a cluster.
type Block = (f64, f64);

/// `DesertHive.ClusterGroup.Generate` (`:85-195`): the metaball field's sources.
fn clusters(d: &Description, rand: &mut UnifiedRandom) -> Vec<Vec<Block>> {
    let (w, h) = (d.columns.max(2), d.rows.max(2));
    let mut seeded = vec![vec![false; h as usize]; w as usize];
    let (hw, hh) = (w / 2 - 1, h / 2 - 1);
    let r2 = (hw + 1) * (hw + 1);
    for i in -hh..=hh {
        let scaled = f64::from(hw) / f64::from(hh.max(1)) * f64::from(i);
        let span = hw.min(((f64::from(r2) - scaled * scaled).max(0.0)).sqrt() as i32);
        for j in -span..=span {
            let (x, y) = (hw + j, hh + i);
            if x >= 0 && x < w && y >= 0 && y < h {
                seeded[x as usize][y as usize] = rand.next_max(2) == 0;
            }
        }
    }

    // Flood-group into clusters, at most two steps out from each seed.
    fn search(map: &mut [Vec<bool>], out: &mut Vec<(i32, i32)>, x: i32, y: i32, level: i32) {
        out.push((x, y));
        map[x as usize][y as usize] = false;
        if level - 1 == -1 {
            return;
        }
        let (w, h) = (map.len() as i32, map[0].len() as i32);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && nx < w && ny >= 0 && ny < h && map[nx as usize][ny as usize] {
                search(map, out, nx, ny, level - 1);
            }
        }
    }

    let mut groups: Vec<Vec<(i32, i32)>> = Vec::new();
    for k in 0..w {
        for l in 0..h {
            if seeded[k as usize][l as usize] && rand.next_max(2) == 0 {
                let mut points = Vec::new();
                search(&mut seeded, &mut points, k, l, 2);
                if points.len() > 2 {
                    groups.push(points);
                }
            }
        }
    }

    // Neighbouring groups either merge or are dropped, decided by a coin per contact.
    let mut owner = vec![vec![-1i32; h as usize]; w as usize];
    for (idx, group) in groups.iter().enumerate() {
        for &(x, y) in group {
            owner[x as usize][y as usize] = idx as i32;
        }
    }
    for idx in 0..groups.len() {
        for gi in 0..groups[idx].len() {
            let (x, y) = groups[idx][gi];
            if owner[x as usize][y as usize] == -1 {
                break;
            }
            let index = owner[x as usize][y as usize];
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let other = owner[nx as usize][ny as usize];
                if other == -1 || other == index {
                    continue;
                }
                let claim = if rand.next_max(2) == 0 { -1 } else { index };
                for &(px, py) in &groups[other as usize] {
                    owner[px as usize][py as usize] = claim;
                }
            }
        }
    }

    let mut rebuilt: Vec<Vec<(i32, i32)>> = vec![Vec::new(); groups.len()];
    for x in 0..w {
        for y in 0..h {
            let o = owner[x as usize][y as usize];
            if o != -1 {
                rebuilt[o as usize].push((x, y));
            }
        }
    }

    let mut out = Vec::new();
    for group in rebuilt {
        if group.len() < 4 {
            continue;
        }
        let mut cluster = Vec::with_capacity(group.len());
        for (x, y) in group {
            cluster.push((
                f64::from(x) + (rand.next_double() - 0.5) * 0.5,
                f64::from(y) + (rand.next_double() - 0.5) * 0.5,
            ));
        }
        out.push(cluster);
    }
    out
}

/// `DesertHive.PlaceClustersArea` (`:226-388`): the metaball field, evaluated per tile.
fn place_clusters(
    world: &mut World,
    d: &Description,
    groups: &[Vec<Block>],
    rand: &mut UnifiedRandom,
    world_seed: u64,
) {
    let area = d.hive.inflated(20);
    // One stream for the whole field, advanced once per cell: `fastRandom.Next(3)` at
    // `DesertHive.cs:280` consumes the shared instance, so the sand-versus-hardened-sand speckle
    // depends on every previous cell. The wall test below uses a position-modified *copy*
    // (`WithModifier(num4, num5)`, `:373`) and therefore does not advance it.
    let mut field = FastRandom::new(world_seed).with_modifier(57005);
    let scale = (f64::from(d.hive.width), f64::from(d.hive.height));
    let grid = (f64::from(d.columns), f64::from(d.rows));
    let half = (BLOCK_SCALE.0 / 2.0, BLOCK_SCALE.1 / 2.0);

    // `.NET Rectangle.Right` and `.Bottom` are the last covered cell, not one past it, so
    // vanilla's `i < area.Right` stops a column short of our exclusive `right()`.
    for i in area.x..area.right() - 1 {
        for j in area.y..area.bottom() - 1 {
            if !world.in_bounds(i, j) {
                continue;
            }
            let block_type = if field.next(3) == 0 {
                HARDENED_SAND
            } else {
                SAND
            };

            let dx = i - d.hive.x;
            let dy = j - d.hive.y;
            let at = (
                (f64::from(dx) - half.0) / scale.0 * grid.0,
                (f64::from(dy) - half.1) / scale.1 * grid.1,
            );

            let mut best = 0.0;
            let mut second = 0.0;
            let mut best_index: i32 = -1;
            for (k, cluster) in groups.iter().enumerate() {
                if (cluster[0].0 - at.0).abs() > 10.0 || (cluster[0].1 - at.1).abs() > 10.0 {
                    continue;
                }
                let mut sum = 0.0;
                for b in cluster {
                    let d2 = (b.0 - at.0).powi(2) + (b.1 - at.1).powi(2);
                    // Vanilla divides unconditionally (`DesertHive.cs:297`), so a cell that lands
                    // exactly on a block source takes the infinity and the whole cell becomes an
                    // open chamber (`num7 > 3.5`). Skipping the zero case here instead quietly
                    // changes which cells are chambers.
                    sum += 1.0 / d2;
                }
                if sum > best {
                    if best > second {
                        second = best;
                    }
                    best = sum;
                    best_index = k as i32;
                } else if sum > second {
                    second = sum;
                }
            }
            let strength = best + second;

            let edge = (
                (f64::from(dx) - half.0) / scale.0 * 2.0 - 1.0,
                (f64::from(dy) - half.1) / scale.1 * 2.0 - 1.0,
            );
            let outside = (edge.0 * edge.0 + edge.1 * edge.1).sqrt() >= 0.8;

            let mut t = world.tile(i, j);
            if strength > 3.5 {
                // The open chamber.
                t = Tile::AIR;
                t.wall = HIVE_WALL;
                if best_index >= 0 && best_index % 15 == 2 {
                    reset_to_type(&mut t, FOSSIL);
                }
            } else if strength > 1.8 {
                t.wall = HIVE_WALL;
                if j < i32::from(world.surface) {
                    t.liquid = 0;
                } else {
                    t.liquid_kind = Liquid::Lava;
                }
                if !outside || t.is_active() {
                    reset_to_type(&mut t, SANDSTONE);
                }
            } else if strength > 0.7 || !outside {
                t.wall = SHELL_WALL;
                t.liquid = 0;
                if !outside || t.is_active() {
                    reset_to_type(&mut t, block_type);
                }
            } else if strength > 0.25 {
                let mut local = field.with_xy(dx, dy);
                let chance = (strength - 0.25) / 0.45;
                if local.next_double() < chance {
                    t.wall = HIVE_WALL;
                    if j < i32::from(world.surface) {
                        t.liquid = 0;
                    } else {
                        t.liquid_kind = Liquid::Lava;
                    }
                    if t.is_active() {
                        reset_to_type(&mut t, block_type);
                    }
                }
            } else {
                continue;
            }
            // A liquid kind is only meaningful with liquid behind it.
            if t.liquid == 0 {
                t.liquid_kind = Liquid::Water;
            }
            world.set_tile(i, j, t);
        }
    }
    let _ = rand;
}

/// `DesertHive.AddTileVariance` (`:390-478`), minus the decorative placements that need objects
/// this generator does not site underground.
fn tile_variance(world: &mut World, d: &Description) {
    for i in -20..d.hive.width + 20 {
        for j in -20..d.hive.height + 20 {
            let (x, y) = (i + d.hive.x, j + d.hive.y);
            if !world.in_bounds(x, y) {
                continue;
            }
            let t = world.tile(x, y);
            let below = world.tile(x, y + 1);
            let below2 = world.tile(x, y + 2);
            let solid_below = below.is_active() && tile_solid::solid(below.block);
            let solid_below2 = below2.is_active() && tile_solid::solid(below2.block);
            if t.block == SAND && t.is_active() && (!solid_below || !solid_below2) {
                // `DesertHive.cs:433-436` assigns `tile.type = 397` directly. It is not a
                // `ResetToType`, so the liquid, the slope and the frames all stay as they were.
                let mut t = t;
                t.block = HARDENED_SAND;
                world.set_tile(x, y, t);
            }
        }
    }
}

/// The blotchy one-tile bore both the anthill and larva-hole entrances drill with.
fn bore(blotched: bool) -> Vec<Link> {
    let mut links = vec![Link::new(Action::IsSolid)];
    if blotched {
        links.insert(
            0,
            Link::new(Action::Blotches {
                min_x: 2,
                min_y: 2,
                max_x: 2,
                max_y: 2,
                chance: 0.3,
            }),
        );
    }
    links.push(Link::new(Action::Clear));
    links.push(Link::new(Action::PlaceWall(HIVE_WALL)));
    links
}

/// The hardened-sand collar that lines a bore.
fn collar() -> Vec<Link> {
    chain([
        Link::new(Action::IsSolid),
        Link::new(Action::SetTile(HARDENED_SAND)),
        Link::new(Action::PlaceWall(HIVE_WALL)),
    ])
    .into_iter()
    .collect()
}

/// `AnthillEntrance` (`AnthillEntrance.cs`, 54 lines): a cone of sand above a wandering bore.
fn anthill_entrance(ctx: &mut Ctx, d: &Description, rand_count: i32) {
    for i in 0..rand_count {
        let radius = ctx.rand.next_range(15, 18);
        let mut x =
            (f64::from(i + 1) / f64::from(rand_count + 1) * f64::from(d.surface_len())) as i32;
        x += d.desert.x;
        let y = d.surface_at(x);
        let origin = (x, y + 6);

        let cone = ctx.slot();
        let mut c = chain([Link::new(Action::SetTile(SAND)).out(cone)]);
        gen_shape_out(
            ctx,
            origin,
            &Shape::Tail {
                width: f64::from(radius * 2),
                end: (0.0, f64::from(-radius) * 1.5),
            },
            &mut c,
            cone,
        );

        let mut column = x;
        let bottom = d.hive.y + (y - d.desert.y) * 2 + 12;
        for j in y - radius - 3..bottom {
            let mut c = bore(j >= y);
            gen_shape(
                ctx,
                (column, j),
                &Shape::Rectangle {
                    left: 0,
                    top: 0,
                    width: 1,
                    height: 1,
                },
                &mut c,
            );
            let mut c = collar();
            gen_shape(
                ctx,
                (column, j),
                &Shape::Circle {
                    h_radius: 2,
                    v_radius: 3,
                },
                &mut c,
            );
            if j % 3 == 0 && j >= y {
                column += ctx.rand.next_range(-1, 2);
                let mut c = bore(true);
                gen_shape(
                    ctx,
                    (column, j),
                    &Shape::Rectangle {
                        left: 0,
                        top: 0,
                        width: 1,
                        height: 1,
                    },
                    &mut c,
                );
                if j >= y + 5 {
                    let mut c = chain([
                        Link::new(Action::SkipWalls(vec![HIVE_WALL])),
                        Link::new(Action::SetTile(SAND)),
                    ]);
                    gen_shape(
                        ctx,
                        (column, j),
                        &Shape::Circle {
                            h_radius: radius,
                            v_radius: 3,
                        },
                        &mut c,
                    );
                    let mut c = chain([Link::new(Action::PlaceWall(HIVE_WALL))]);
                    gen_shape(
                        ctx,
                        (column, j),
                        &Shape::Circle {
                            h_radius: radius - 2,
                            v_radius: 3,
                        },
                        &mut c,
                    );
                }
                let mut c = collar();
                gen_shape(
                    ctx,
                    (column, j),
                    &Shape::Circle {
                        h_radius: 2,
                        v_radius: 3,
                    },
                    &mut c,
                );
            }
        }

        // Hollow the cone's own mouth out again.
        let mut c = chain([
            Link::new(Action::ClearTile),
            Link::new(Action::Expand { x: 1, y: 1 }),
            Link::new(Action::PlaceWall(0)),
        ]);
        gen_shape(
            ctx,
            (origin.0, origin.1 - (f64::from(radius) * 1.5) as i32 + 3),
            &Shape::Circle {
                h_radius: radius / 2,
                v_radius: radius / 3,
            },
            &mut c,
        );
        let _ = ctx.take(cone);
    }
}

/// `LarvaHoleEntrance` (`LarvaHoleEntrance.cs`, 46 lines): a bell-mouthed shaft.
fn larva_hole_entrance(ctx: &mut Ctx, d: &Description, rand_count: i32) {
    for i in 0..rand_count {
        let radius = ctx.rand.next_range(13, 16);
        let mut x =
            (f64::from(i + 1) / f64::from(rand_count + 1) * f64::from(d.surface_len())) as i32;
        x += d.desert.x;
        let y = d.surface_at(x);
        let at = (x, y);

        let dug = ctx.slot();
        let mut c = chain([Link::new(Action::Clear).out(dug)]);
        gen_shape(
            ctx,
            at,
            &Shape::Rectangle {
                left: -radius,
                top: -radius * 2,
                width: radius * 2,
                height: radius * 2,
            },
            &mut c,
        );
        let mut c = chain([Link::new(Action::Clear).out(dug)]);
        gen_shape(
            ctx,
            at,
            &Shape::Tail {
                width: f64::from(radius * 2),
                end: (0.0, f64::from(radius) * 1.5),
            },
            &mut c,
        );

        let mut column = x;
        let bottom = d.hive.y + (y - d.desert.y) * 2 + 12;
        for j in y + (f64::from(radius) * 1.5) as i32..bottom {
            for step in 0..2 {
                if step == 1 && j % 3 != 0 {
                    continue;
                }
                if step == 1 {
                    column += ctx.rand.next_range(-1, 2);
                }
                let mut c = bore(true);
                gen_shape(
                    ctx,
                    (column, j),
                    &Shape::Rectangle {
                        left: 0,
                        top: 0,
                        width: 1,
                        height: 1,
                    },
                    &mut c,
                );
                let mut c = collar();
                gen_shape(
                    ctx,
                    (column, j),
                    &Shape::Circle {
                        h_radius: 2,
                        v_radius: 3,
                    },
                    &mut c,
                );
            }
        }

        let shape = ctx.take(dug);
        let mut c = chain([Link::new(Action::PlaceWall(0))]);
        gen_shape(ctx, (x, y + 2), &Shape::All(shape), &mut c);
    }
}

/// `ChambersEntrance` (`ChambersEntrance.cs`, 97 lines): a stack of rooms joined by curved paths.
fn chambers_entrance(ctx: &mut Ctx, d: &Description) {
    let cx = d.desert.x + d.desert.width / 2 + ctx.rand.next_range(-40, 41);
    let cy = d.surface_at(cx);
    let origin = (cx, cy + 2);

    let bowl = ctx.slot();
    let mut c = chain([
        Link::new(Action::Blotches {
            min_x: 2,
            min_y: 2,
            max_x: 2,
            max_y: 2,
            chance: 0.3,
        }),
        Link::new(Action::SetTile(SAND)).out(bowl),
    ]);
    gen_shape(
        ctx,
        origin,
        &Shape::Circle {
            h_radius: 24,
            v_radius: 12,
        },
        &mut c,
    );

    let rooms_shape = ctx.slot();
    let drop = d.hive.y - cy;
    let mut side = if ctx.rand.next_max(2) != 0 { 1 } else { -1 };
    // (position, direction) pairs the paths are drawn between.
    let mut nodes: Vec<((i32, i32), i32)> = vec![((cx - side * 26, cy - 8), side)];
    let count = ctx.rand.next_range(2, 4);
    for i in 0..count {
        let down = (f64::from(i + 1) / f64::from(count) * f64::from(drop)) as i32
            + ctx.rand.next_range(-8, 9);
        let across = side * ctx.rand.next_range(20, 41);
        let width = ctx.rand.next_range(18, 29);
        let mut c = chain([
            Link::new(Action::Offset { x: across, y: down }),
            Link::new(Action::Blotches {
                min_x: 2,
                min_y: 2,
                max_x: 2,
                max_y: 2,
                chance: 0.3,
            }),
            Link::new(Action::Clear).out(rooms_shape),
            Link::new(Action::PlaceWall(HIVE_WALL)),
        ]);
        gen_shape(
            ctx,
            (cx, cy),
            &Shape::Circle {
                h_radius: width / 2,
                v_radius: 3,
            },
            &mut c,
        );
        nodes.push(((across + width / 2 * -side + cx, down + cy), -side));
        side *= -1;
    }

    let rooms = ctx.take(rooms_shape);
    let mut c = chain([
        Link::new(Action::Expand { x: 1, y: 1 }),
        Link::new(Action::OnlyTiles(vec![SAND])),
        Link::new(Action::SetTile(HARDENED_SAND)),
        Link::new(Action::PlaceWall(HIVE_WALL)),
    ]);
    gen_shape(ctx, (cx, cy), &Shape::OuterOutline(rooms), &mut c);

    // The paths: a cubic Bezier between consecutive rooms, bored 2x4 at a time.
    for j in 1..nodes.len() {
        let (a, a_dir) = nodes[j - 1];
        let (b, b_dir) = nodes[j];
        let reach = f64::from((b.0 - a.0).abs()) * 1.5;
        let mut t = 0.0;
        while t <= 1.0 {
            let p0 = (
                f64::from(a.0) + f64::from(a_dir) * reach * t,
                f64::from(a.1),
            );
            let p3 = (
                f64::from(b.0) + f64::from(b_dir) * reach * (1.0 - t),
                f64::from(b.1),
            );
            let mid = (
                f64::from(a.0) + (f64::from(b.0) - f64::from(a.0)) * t,
                f64::from(a.1) + (f64::from(b.1) - f64::from(a.1)) * t,
            );
            let l = (p0.0 + (mid.0 - p0.0) * t, p0.1 + (mid.1 - p0.1) * t);
            let r = (mid.0 + (p3.0 - mid.0) * t, mid.1 + (p3.1 - mid.1) * t);
            let at = (
                (l.0 + (r.0 - l.0) * t) as i32,
                (l.1 + (r.1 - l.1) * t) as i32,
            );
            let mut c = chain([
                Link::new(Action::IsSolid),
                Link::new(Action::Blotches {
                    min_x: 2,
                    min_y: 2,
                    max_x: 2,
                    max_y: 2,
                    chance: 0.3,
                }),
                Link::new(Action::Clear),
                Link::new(Action::Expand { x: 1, y: 1 }),
                Link::new(Action::PlaceWall(HIVE_WALL)),
                Link::new(Action::OnlyTiles(vec![SAND])),
                Link::new(Action::SetTile(HARDENED_SAND)),
            ]);
            gen_shape(
                ctx,
                at,
                &Shape::Rectangle {
                    left: 0,
                    top: 0,
                    width: 2,
                    height: 4,
                },
                &mut c,
            );
            t += 0.02;
        }
    }

    let bowl_shape = ctx.take(bowl);
    let mut c = chain([
        Link::new(Action::NotInShape(bowl_shape)),
        Link::new(Action::Expand { x: 1, y: 1 }),
        Link::new(Action::PlaceWall(0)),
    ]);
    gen_shape(
        ctx,
        origin,
        &Shape::Rectangle {
            left: -29,
            top: -12,
            width: 58,
            height: 12,
        },
        &mut c,
    );
}

/// `PitEntrance.PlaceAt` (`:17-58`): the shaft down into the hive.
fn pit_entrance(world: &mut World, d: &Description, rand: &mut UnifiedRandom) {
    let mut radius = rand.next_range(6, 9);
    let cx = d.combined.x + d.combined.width / 2;
    let cy = d.surface_at(cx);

    let span = d.hive.y - d.desert.y;
    for i in -radius - 3..radius + 3 {
        let column = d.surface_at(i + cx);
        for j in column..=d.hive.y + 10 {
            if span <= 0 {
                break;
            }
            let progress = (f64::from(j - column) / f64::from(span)).clamp(0.0, 1.0);
            // `GetHoleRadiusScaleAt`.
            let scale = if progress < 0.6 {
                1.0
            } else {
                let delta = ((progress - 0.6) / 0.4).clamp(0.0, 1.0);
                let smoother = 1.0 - (delta * std::f64::consts::PI).cos() * 0.5 - 0.5;
                (1.0 - smoother) * 0.5 + 0.5
            };
            let hole = (scale * f64::from(radius)) as i32;
            let (x, y) = (i + cx, j);
            if !world.in_bounds(x, y) {
                continue;
            }
            if i.abs() < hole {
                world.set_tile(x, y, Tile::AIR);
            } else if i.abs() < hole + 3 && progress > 0.35 {
                let mut t = world.tile(x, y);
                reset_to_type(&mut t, HARDENED_SAND);
                world.set_tile(x, y, t);
            }
            let taper = (f64::from(i) / f64::from(radius)).abs().powi(2);
            if i.abs() < hole + 3 && f64::from(j - cy) > 15.0 - 3.0 * taper {
                let mut t = world.tile(x, y);
                t.wall = HIVE_WALL;
                world.set_tile(x, y, t);
            }
        }
    }

    // A shallow crater at the lip.
    radius += 4;
    for k in -radius..radius {
        let mut depth = radius - k.abs();
        depth = 10.min(depth * depth);
        for l in 0..depth {
            let (x, y) = (k + cx, l + d.surface_at(k + cx));
            if world.in_bounds(x, y) {
                world.set_tile(x, y, Tile::AIR);
            }
        }
    }
}

/// `DesertBiome.Place` (`:13-44`). Returns the area it claimed.
pub fn place(
    world: &mut World,
    structures: &mut StructureMap,
    rand: &mut UnifiedRandom,
    world_seed: u64,
    origin: (i32, i32),
    skip_tile_check: bool,
) -> Option<Description> {
    let mut d = describe(world, rand, origin, skip_tile_check)?;
    sand_mound(world, &d, rand);
    let (heights, _, _) = surface_map(world, d.combined.x - 5, d.combined.width + 10);
    d.surface = heights;
    d.surface_x = d.combined.x - 5;

    // Vanilla rolls the entrance, then rolls which of four styles. Both draws are made, so the
    // entrance appears at vanilla's rate; only the style is narrowed. See the module doc.
    if rand.next_double() <= 0.3333 {
        match rand.next_max(4) {
            0 => {
                let mut ctx = Ctx::new(world, rand);
                chambers_entrance(&mut ctx, &d);
            }
            1 => {
                let count = rand.next_range(2, 4);
                let mut ctx = Ctx::new(world, rand);
                anthill_entrance(&mut ctx, &d, count);
            }
            2 => {
                let count = rand.next_range(2, 4);
                let mut ctx = Ctx::new(world, rand);
                larva_hole_entrance(&mut ctx, &d, count);
            }
            _ => pit_entrance(world, &d, rand),
        }
    }

    let groups = clusters(&d, rand);
    place_clusters(world, &d, &groups, rand, world_seed);
    tile_variance(world, &d);
    add_tile_variance(world, rand, &d.hive);
    cleanup_area(world, rand, &d.hive);

    // `DesertBiome.cs:43-44`, verbatim: the height is `CombinedArea.Bottom - 20`, not that minus
    // the `50` the rectangle starts at. The recorded area is therefore taller than the desert, and
    // every later `CanPlace` test inside it is refused for rows that have no sand in them.
    let area = Rect::new(d.combined.x, 50, d.combined.width, d.combined.bottom() - 20);
    structures.add_structure(area, 10);
    Some(d)
}

/// `Main.tileSolid[484]`, as this pass sees it: the very first statement of Full Desert sets it
/// false (`WorldGen.cs:12549`), so a boulder is not a floor for anything placed later in the same
/// pass. The table otherwise says solid, and the flag persists past the pass (later passes set it
/// again), so it is modelled here rather than in the shared table.
fn solid_in_desert(block: u16) -> bool {
    block != 484 && tile_solid::solid(block)
}

/// `WorldGen.Place2x2` (`WorldGen.cs:51788-51880`) for the two types this loop uses it for, 484
/// and 751.
///
/// The check and the write are not over the same cells, and that is not a mistranslation: vanilla
/// tests the two-by-two *up and to the left* of the call (`i` in `x-1..x`, `j` in `y-1..y` all
/// inactive, and `(i, y+1)` solid for the same two columns) and then writes the two-by-two *down
/// and to the right*: `(x, y)`, `(x+1, y)`, `(x, y+1)`, `(x+1, y+1)`. `AddTileVariance` calls this
/// at the cell above the sandstone, so the write lands half on the sandstone that was the support,
/// which is exactly why a decorated cell stops looking like open air to the next iteration.
///
/// `Main.tileTable` would widen what counts as support and is not consulted: nothing the desert
/// writes is a table. `SolidTile2`'s half-brick and actuator tests (`WorldGen.cs:70683`) are inert
/// for the same kind of reason: this generator writes neither during generation, so only `slope`
/// is tested and it is always zero here.
fn place_2x2(world: &mut World, x: i32, y: i32, block: u16, style: i32) {
    let (width, height) = (world.width(), world.height());
    if x < 5 || x > width - 5 || y < 5 || y > height - 5 {
        return;
    }
    for i in x - 1..x {
        for j in y - 1..y {
            if world.tile(i, j).is_active() {
                return;
            }
        }
        let support = world.tile(i, y + 1);
        if !support.is_active() || !solid_in_desert(support.block) || support.slope != 0 {
            return;
        }
    }
    for k in 0..2 {
        for l in 0..2 {
            let mut t = world.tile(x + k, y + l);
            t.block = block;
            t.flags = TileFlags(t.flags.0 | TileFlags::ACTIVE);
            t.frame_x = (k * 18) as i16;
            t.frame_y = (style * 36 + l * 18) as i16;
            world.set_tile(x + k, y + l, t);
        }
    }
}

/// `WorldGen.Place3x2` (`WorldGen.cs:52533-52660`) for type 187, the hanging sandstone brick.
///
/// `flag`, the single-row variant, belongs to type 14 style 25 and cannot hold here, so this is the
/// two-row branch: three columns `x-1..x+2` by two rows `y-1..y+1` must all be inactive and the row
/// under them must be solid, and for 186/187 that floor may not be a boulder
/// (`InvalidTileForPilesOrSpeleothems`, `WorldGen.cs:39380-39394`) - which is why a 484 dropped by
/// the second branch of the caller stops an 187 from landing next to it. Six cells are then
/// written, the upper row with `frameY` 0 and the lower with 18.
fn place_3x2(world: &mut World, x: i32, y: i32, block: u16, style: i32) {
    let (width, height) = (world.width(), world.height());
    if x < 5 || x > width - 5 || y < 5 || y > height - 5 {
        return;
    }
    for i in x - 1..x + 2 {
        for j in y - 1..y + 1 {
            if world.tile(i, j).is_active() {
                return;
            }
        }
        let support = world.tile(i, y + 1);
        if !support.is_active()
            || !solid_in_desert(support.block)
            || support.slope != 0
            || is_boulder(support.block)
        {
            return;
        }
    }
    let base = 54 * style;
    for column in x - 1..x + 2 {
        let offset = (column - (x - 1)) * 18;
        for (row, frame_y) in [(y - 1, 0), (y, 18)] {
            let mut t = world.tile(column, row);
            t.block = block;
            t.flags = TileFlags(t.flags.0 | TileFlags::ACTIVE);
            t.frame_x = (base + offset) as i16;
            t.frame_y = frame_y as i16;
            world.set_tile(column, row, t);
        }
    }
}

/// `TileID.Sets.Boulders` (`TileID.cs:195`): the floor these speleothem placements refuse.
fn is_boulder(block: u16) -> bool {
    matches!(
        block,
        138 | 484 | 664 | 665 | 711 | 712 | 713 | 714 | 715 | 716
    )
}

/// `WorldGen.PlaceTight` (`WorldGen.cs:38704-38725`) plus the one arm of
/// `PlaceUncheckedStalactite` (`:38727+`) that this call site can reach. Returns the draws taken.
///
/// `PlaceTight` first refuses a shimmering cell or a live 231, and only then evaluates its
/// arguments - so the two draws happen inside that guard, not before it (`:38719-38723`). Inside,
/// nothing is written unless the cell above is solid and both the target and the cell under it are
/// empty (`:38732`).
///
/// From `AddTileVariance` the target is either the cell *above* a sandstone - and that branch only
/// runs when `flag` says three rows above are clear, so the cell the stalactite would hang from is
/// air and nothing can be written - or the cell *below* it, whose above-neighbour is the sandstone
/// itself. So of the six support arms in the source only `type == 396 || type == 397`
/// (`:38820-38845`) is reachable here, with frameX `378 + variation * 18`: one cell at frameY 72
/// when the small variant is rolled, two cells at 0 and 18 otherwise. The other arms belong to
/// materials this loop never stands on, and are absent deliberately rather than guessed at.
fn place_tight(world: &mut World, rand: &mut UnifiedRandom, column: i32, row: i32) -> usize {
    let target = world.tile(column, row);
    if target.is_active() && target.block == 231 {
        return 0;
    }
    let prefer_small = rand.next_max(2) == 0;
    let variation = rand.next_max(3);
    let above = world.tile(column, row - 1);
    let hangs_from_solid_ground = above.is_active()
        && tile_solid::solid(above.block)
        && above.slope == 0
        && !tile_solid::solid_top(above.block);
    if !hangs_from_solid_ground || target.is_active() || world.tile(column, row + 1).is_active() {
        return 2;
    }
    if above.block != SANDSTONE && above.block != HARDENED_SAND {
        return 2;
    }
    let base = (378 + variation * 18) as i16;
    let mut head = target;
    head.block = STALACTITE;
    head.flags = TileFlags(head.flags.0 | TileFlags::ACTIVE);
    head.frame_x = base;
    head.frame_y = if prefer_small { 72 } else { 0 };
    world.set_tile(column, row, head);
    if !prefer_small {
        let mut foot = world.tile(column, row + 1);
        foot.block = STALACTITE;
        foot.flags = TileFlags(foot.flags.0 | TileFlags::ACTIVE);
        foot.frame_x = base;
        foot.frame_y = 18;
        world.set_tile(column, row + 1, foot);
    }
    2
}

/// The tile `PlaceUncheckedStalactite` writes: `ushort type = 165` (`WorldGen.cs:38729`).
const STALACTITE: u16 = 165;

/// `DesertHive.AddTileVariance` (`DesertHive.cs:420-506`), the second loop.
///
/// Sandstone that has air above it (`clear_above`) or below it (`clear_below`) rolls for a
/// decoration. This loop is where the desert's remaining draw gap lives: adding it moved the pass
/// from 213,796 to 239,052 numbers against the reference's 237,989 on seed 12345.
///
/// The overshoot is not a wrong chain, it is a missing write. Vanilla's `PlaceTile` leaves a
/// 2x2/3x2/stalactite in the cells this same scan reads as open air, so once a decoration lands,
/// its neighbours stop looking clear and stop rolling. Until the writes are modelled, this scan
/// sees too much air and rolls about a thousand times too often. The rolls
/// themselves are taken before any placement test, so they happen whether or not the decoration
/// fits; only the 165 branch draws inside the placement, as noted below.
///
/// The chains, exactly as `:481-502` reads:
///
/// 1. `flag` and `Next(20)` hits: `Next(4)` for a style and `Next(30)` for the variant, then a
///    `PlaceTile` of 751 (style 0) or 485 (that style).
/// 2. otherwise `flag` and `Next(5)` hits: a 484 at the cell above.
/// 3. otherwise `flag ^ flag2` and `Next(5)` hits: a 165 stalactite, which is the only placement
///    of the five that draws: `PlaceTight` rolls `Next(2)` and `Next(3)` for the size and the
///    variation (`WorldGen.cs:38704-38723`), and that is why the branch costs three draws.
/// 4. otherwise `flag` and `Next(5)` hits: an 187 whose style is `29 + Next(6)`, so the sixth
///    draw is taken only when the fifth hit.
///
/// Note the third branch's target: `num4 + (!flag ? 1 : -1)`. Reaching it with `flag` true means
/// branch 2 rolled and missed, so the offset is -1; with `flag` false the XOR requires `flag2`
/// true, and the offset is +1.
fn add_tile_variance(world: &mut World, rand: &mut UnifiedRandom, hive: &Rect) {
    // `k` is the column offset and runs to `Hive.Width + 20`; `l` is the row offset and runs to
    // `Hive.Height + 20` (`DesertHive.cs:440-443`).
    for k in -20..hive.width + 20 {
        for l in -20..hive.height + 20 {
            let column = k + hive.x;
            let row = l + hive.y;
            // `WorldGen.InWorld(num3, num4, 5)`: at least five tiles from every edge, low bound
            // inclusive (`WorldGen.cs:9642-9649`).
            if column < 5 || column >= world.width() - 5 || row < 5 || row >= world.height() - 5 {
                continue;
            }
            let tile = world.tile(column, row);
            if !tile.is_active() || tile.block != SANDSTONE {
                continue;
            }
            // Three rows above, both this column and the next: `flag` is true only if all six are
            // empty, and vanilla breaks out of the scan at the first solid one.
            let mut clear_above = true;
            for offset in [-1, -2, -3] {
                if world.tile(column, row + offset).is_active()
                    || world.tile(column + 1, row + offset).is_active()
                {
                    clear_above = false;
                    break;
                }
            }
            let mut clear_below = true;
            for offset in [1, 2, 3] {
                if world.tile(column, row + offset).is_active()
                    || world.tile(column + 1, row + offset).is_active()
                {
                    clear_below = false;
                    break;
                }
            }
            // `remixWorldGen && row > Main.rockLayer` would `continue` here; on an ordinary seed
            // the first operand is false and the whole test is dead.
            if clear_above && rand.next_max(20) == 0 {
                let style = rand.next_max(4);
                let replacement = rand.next_max(30);
                if replacement == 0 {
                    place_2x2(world, column, row - 1, 751, 0);
                } else {
                    // 485 goes through `PlaceObject` (`WorldGen.cs:51883`), which writes inactive
                    // framed cells, so it neither occupies a cell this scan tests as open air nor
                    // presents a 396 to the `type != 396` test. The draw is right either way; the
                    // object itself is not written yet.
                    let _ = style;
                }
            } else if clear_above && rand.next_max(5) == 0 {
                place_2x2(world, column, row - 1, 484, 0);
            } else if (clear_above ^ clear_below) && rand.next_max(5) == 0 {
                let target = if clear_above { row - 1 } else { row + 1 };
                let _ = place_tight(world, rand, column, target);
            } else if clear_above && rand.next_max(5) == 0 {
                let style = 29 + rand.next_max(6);
                place_3x2(world, column, row - 1, 187, style);
            }
        }
    }
}

/// `DesertBiome.CleanupArea` (`DesertBiome.cs:55-71`).
///
/// The rectangle is the hive grown twenty tiles on each side, and the only cost in it is wall
/// framing: `SquareWallFrame` rolls for the cell itself (`WorldGen.cs:81485-81496`), while
/// `WorldUtils.TileFrame(i, j, frameNeighbors: true)` is five `WorldGen.TileFrame` calls that
/// cannot draw during generation because their cosmetic path is guarded by `!generatingWorld`
/// (`WorldGen.cs:82667`, `WorldUtils.cs:79-89`).
fn cleanup_area(world: &mut World, rand: &mut UnifiedRandom, area: &Rect) {
    // `.NET Rectangle.Right` is `X + Width - 1`, so vanilla's `i < area.Right + 20` stops one
    // column before our exclusive `right() + 20`.
    for i in area.x - 20..area.right() + 19 {
        for j in area.y - 20..area.bottom() + 19 {
            if i > 0 && i < world.width() - 1 && j > 0 && j < world.height() - 1 {
                let _ = super::tile_runner::square_wall_frame(&*world, rand, i, j);
            }
        }
    }
}

/// The pass. `WorldGen.cs:12544-12575`.
pub fn run(vars: &mut GenVars) {
    let width = vars.world.width();
    // `Main.maxTilesX / 2`, and the offset is measured from it away from the dungeon.
    let half = width / 2;
    let mut side = match vars.dungeon_side {
        DungeonSide::Left => -1,
        DungeonSide::Right => 1,
    };
    let mut offset = vars.rand.next_max(half) / 8;
    offset += half / 8;
    let mut x = half + offset * -side;
    let origin_y = vars.world_surface_high as i32 + 25;
    let seed = u64::try_from(vars.seed).unwrap_or(0);

    // The retry loop is the pass: `Place` fails only when the site's floor row runs through jungle
    // or ice, and every failure costs two draws. `num5` counts failures toward a flip of the
    // dungeon side (`width / 4` of them), and two flips give up and set `skipDesertTileCheck`,
    // after which every site is accepted whatever is under it.
    let mut failures = 0;
    let mut flips = 0;
    loop {
        let skip = vars.skip_desert_tile_check;
        let placed = {
            let world = &mut vars.world;
            let structures = &mut vars.structures;
            let rand = &mut vars.rand;
            place(world, structures, rand, seed, (x, origin_y), skip).map(|d| {
                (
                    d.combined.inflated(10),
                    d.hive,
                    d.desert.y,
                    d.combined.bottom(),
                )
            })
        };
        let Some((location, hive, _, _)) = placed else {
            offset = vars.rand.next_max(half) / 2;
            offset += half / 8;
            offset += vars.rand.next_max(failures / 12);
            x = half + offset * -side;
            failures += 1;
            if failures > width / 4 {
                side = -side;
                failures = 0;
                flips += 1;
                if flips >= 2 {
                    vars.skip_desert_tile_check = true;
                }
            }
            continue;
        };
        // `ExportDescriptionToEngine` (`DesertBiome.cs:50-52`): the location field is assigned and
        // then inflated in place, so what later passes read is the ten-tile-grown combined area,
        // while the hive is stored as it was.
        vars.underground_desert_location = Some(location);
        vars.underground_desert_hive_location = Some(hive);
        break;
    }
}

/// The desert is placed once, over the layout's own desert band.
pub fn scatter(
    world: &mut World,
    layout: &Layout,
    structures: &mut StructureMap,
    rand: &mut UnifiedRandom,
    world_seed: u64,
) -> Option<Rect> {
    if layout.desert.width() < 100 || layout.width < 800 || layout.height < 400 {
        return None;
    }
    let x = layout.desert.from + layout.desert.width() / 2;
    place(
        world,
        structures,
        rand,
        world_seed,
        (x, layout.surface),
        false,
    )
    .map(|d| d.combined.inflated(10))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandy(w: i32, h: i32) -> World {
        let mut world = World::empty(w, h, "desert");
        for x in 0..w {
            for y in 120..h {
                world.set_tile(x, y, Tile::block(1));
            }
        }
        world
    }

    fn layout_for(world: &World) -> Layout {
        let mut l = Layout::plan(world.width(), world.height(), &mut UnifiedRandom::new(3));
        l.surface = 120;
        l.rock = 200;
        l.desert = super::super::layout::Band { from: 300, to: 900 };
        l
    }

    #[test]
    fn fast_random_matches_its_own_stream_twice() {
        let mut a = FastRandom::new(1234);
        let mut b = FastRandom::new(1234);
        for _ in 0..20 {
            assert_eq!(a.next(97), b.next(97));
        }
        // A modifier gives a different, still deterministic stream.
        let mut c = FastRandom::new(1234).with_xy(7, 9);
        let mut d = FastRandom::new(1234).with_xy(7, 9);
        assert_eq!(c.next(1000), d.next(1000));
    }

    /// `AddTileVariance`'s third branch hangs a stalactite under the sandstone it is standing on,
    /// and on the reference world that is 1,570 cells. A desert that writes none of them is missing
    /// a material the later passes and the save file both see.
    #[test]
    fn the_hive_hangs_stalactites_under_its_sandstone() {
        let mut world = sandy(1600, 900);
        let layout = layout_for(&world);
        let mut structures = StructureMap::new();
        let mut rand = UnifiedRandom::new(31415);
        scatter(&mut world, &layout, &mut structures, &mut rand, 999)
            .expect("the desert was refused");
        let mut spikes = 0;
        for x in 300..900 {
            for y in 120..700 {
                let t = world.tile(x, y);
                if t.is_active() && t.block == STALACTITE {
                    spikes += 1;
                }
            }
        }
        assert!(spikes > 100, "the desert hung no stalactites: {spikes}");
    }

    /// The field's sand-versus-hardened-sand speckle is one `FastRandom` advanced once per cell
    /// (`DesertHive.cs:280`). Copying the base instance inside the loop made every cell read the
    /// same value, so the desert came out a single colour and the surface pass then hardened about
    /// thirty thousand cells vanilla leaves as sand. Both materials must appear.
    #[test]
    fn the_hive_speckles_sand_and_hardened_sand_together() {
        let mut world = sandy(1600, 900);
        let layout = layout_for(&world);
        let mut structures = StructureMap::new();
        let mut rand = UnifiedRandom::new(31415);
        scatter(&mut world, &layout, &mut structures, &mut rand, 999)
            .expect("the desert was refused");
        let (mut sand, mut hardened) = (0u32, 0u32);
        for x in 300..900 {
            for y in 120..700 {
                let t = world.tile(x, y);
                if !t.is_active() {
                    continue;
                }
                if t.block == SAND {
                    sand += 1;
                } else if t.block == HARDENED_SAND {
                    hardened += 1;
                }
            }
        }
        assert!(sand > 100, "the field wrote no sand: {sand}");
        assert!(
            hardened > 100,
            "the field wrote no hardened sand: {hardened}"
        );
        // The share is the discriminator: a per-cell copy that restarts the stream writes one
        // material everywhere, so the share would sit at 0 or 1.
        let total = hardened + sand;
        let share = f64::from(hardened) / f64::from(total);
        assert!(
            (0.15..=0.75).contains(&share),
            "the field's speckle is not a mix: {share} hardened of {total}"
        );
    }

    /// One `FastRandom` advances across calls. The field draws `Next(3)` from a single shared
    /// instance once per cell (`DesertHive.cs:280`), so a copy that restarts each cell is not the
    /// same stream and paints the desert in one colour.
    #[test]
    fn one_fast_random_advances_across_calls() {
        let mut field = FastRandom::new(999).with_modifier(57005);
        let first = field.next(3);
        let rest: Vec<i32> = (0..40).map(|_| field.next(3)).collect();
        assert!(
            rest.iter().any(|value| *value != first),
            "a stream that never varied would colour the whole desert identically"
        );
    }

    #[test]
    fn a_desert_carves_a_hive_of_sandstone_with_walls_and_open_chambers() {
        let mut world = sandy(1600, 900);
        let layout = layout_for(&world);
        let mut structures = StructureMap::new();
        let mut rand = UnifiedRandom::new(31415);
        let area = scatter(&mut world, &layout, &mut structures, &mut rand, 999)
            .expect("the desert was refused");

        let mut sandstone = 0;
        let mut walls = 0;
        let mut open = 0;
        let mut sand = 0;
        for x in area.x.max(0)..area.right().min(world.width()) {
            for y in area.y.max(0)..area.bottom().min(world.height()) {
                let t = world.tile(x, y);
                if t.is_active() && t.block == SANDSTONE {
                    sandstone += 1;
                }
                if t.is_active() && (t.block == SAND || t.block == HARDENED_SAND) {
                    sand += 1;
                }
                if t.wall == HIVE_WALL || t.wall == SHELL_WALL {
                    walls += 1;
                }
                if !t.is_active() && t.wall == HIVE_WALL {
                    open += 1;
                }
            }
        }
        assert!(sand > 1000, "not enough sand: {sand}");
        assert!(sandstone > 100, "not enough sandstone: {sandstone}");
        assert!(walls > 1000, "the hive was never walled: {walls}");
        assert!(open > 100, "no open chambers were carved: {open}");
    }

    /// Every entrance style runs and cuts something. The seeded end-to-end test only ever exercises
    /// one of the four, so a style that panicked or silently did nothing would hide behind the
    /// other three - the same silent-absence failure the whole project keeps finding.
    #[test]
    fn all_four_entrance_styles_cut_a_way_in() {
        for style in 0..4 {
            let mut world = sandy(1600, 900);
            let mut rand = UnifiedRandom::new(555 + style);
            let d = describe(&world, &mut rand, (800, 120), false)
                .expect("the description should be valid on a clean site");
            let before = solid_count(&world, &d);
            match style {
                0 => {
                    let mut ctx = Ctx::new(&mut world, &mut rand);
                    chambers_entrance(&mut ctx, &d);
                }
                1 => {
                    let mut ctx = Ctx::new(&mut world, &mut rand);
                    anthill_entrance(&mut ctx, &d, 2);
                }
                2 => {
                    let mut ctx = Ctx::new(&mut world, &mut rand);
                    larva_hole_entrance(&mut ctx, &d, 2);
                }
                _ => pit_entrance(&mut world, &d, &mut rand),
            }
            let after = solid_count(&world, &d);
            assert!(
                after != before,
                "entrance style {style} changed nothing at all"
            );
        }
    }

    fn solid_count(world: &World, d: &Description) -> usize {
        let mut n = 0;
        for x in d.combined.x.max(0)..d.combined.right().min(world.width()) {
            for y in 100..d.combined.bottom().min(world.height()) {
                if world.tile(x, y).is_active() {
                    n += 1;
                }
            }
        }
        n
    }

    /// A site whose floor row runs through snow is refused, as vanilla refuses it.
    #[test]
    fn a_site_over_snow_is_refused() {
        let mut world = sandy(1600, 900);
        for x in 0..1600 {
            world.set_tile(x, 120, Tile::block(147));
        }
        let layout = layout_for(&world);
        let mut structures = StructureMap::new();
        let mut rand = UnifiedRandom::new(5);
        assert!(scatter(&mut world, &layout, &mut structures, &mut rand, 1).is_none());
    }

    #[test]
    fn the_desert_is_reproducible_from_the_seed() {
        let run = || {
            let mut world = sandy(1200, 700);
            let layout = layout_for(&world);
            let mut structures = StructureMap::new();
            let mut rand = UnifiedRandom::new(2718);
            scatter(&mut world, &layout, &mut structures, &mut rand, 42);
            let mut fingerprint = Vec::new();
            for x in (300..900).step_by(7) {
                for y in (100..600).step_by(7) {
                    let t = world.tile(x, y);
                    fingerprint.push((t.block, t.wall));
                }
            }
            fingerprint
        };
        assert_eq!(run(), run());
    }
}

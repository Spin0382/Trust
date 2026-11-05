//! `Mud Caves To Grass`: mud left at the surface turns to jungle grass, and tiny stone clumps
//! left by the cave passes are removed.
//!
//! Transcribed from the `MudCavesToJungleGrass` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12511-12541`), `WorldGen.SpreadGrass` (`WorldGen.cs:75756-75869`) and
//! `WorldGen.ScanTileColumnAndRemoveClumps` (`WorldGen.cs:10176-10199`).
//!
//! The reference world records zero draws for this pass: `NotTheBees` returns immediately on an
//! ordinary seed, and the grass conversion itself only calls `SquareTileFrame`, whose cosmetic
//! framing is skipped while `generatingWorld` is true. So the RNG oracle says nothing here; the
//! stepped world comparison is the only instrument that can judge it.
//!
//! Two details survive the simplification and matter:
//!
//! - `SpreadGrass` is recursive through the three-by-three neighbourhood, capped by a shared
//!   counter at 1000 calls. A large mud field is therefore converted in a growing frontier, not
//!   in one sweep, and the cap is reachable in the jungle.
//! - `ScanTileColumnAndRemoveClumps` does not remove every one-to-nineteen-tile run it sees. It
//!   runs a capped flood fill from the top of the run and only clears when that connected
//!   component is itself shorter than 20, which is how a two-tile spike attached to a larger
//!   body survives.
//!
//! The secret-seed body at the head of the pass is `WorldGen.NotTheBees`. On an ordinary seed it
//! returns before touching a tile or drawing a number; the port in [`super::not_the_bees`] covers
//! the conversion when the seed is wired to it.

use terrustia_proto::{TileFlags, tile_solid};

use super::passes::GenVars;
use super::tile_runner::can_be_cleared;
use crate::world::World;

/// `TileID.Mud`.
const MUD: u16 = 59;
/// `TileID.JungleGrass`.
const JUNGLE_GRASS: u16 = 60;
/// `WorldGen.grassSpread`'s recursion cap.
const GRASS_SPREAD_LIMIT: usize = 1000;
/// `WorldGen.tileCounterMax`.
const TILE_COUNTER_MAX: usize = 20;

/// `WorldGen.InWorld`.
fn in_world(world: &World, x: i32, y: i32, fluff: i32) -> bool {
    x >= fluff && x < world.width() - fluff && y >= fluff && y < world.height() - fluff
}

/// `WorldGen.SpreadGrass` for the ordinary-world branch this pass uses.
///
/// `dirt` is the source block and `grass` the replacement. The call this pass makes is
/// `(59, 60)`; the other branches of the vanilla routine (`Convert` for corruption, crimson and
/// hallowed grass) are not reachable until a later pass calls them, and are deliberately absent
/// rather than guessed at.
///
/// `spread` is the shared `WorldGen.grassSpread` counter. The recursion increments it before the
/// call and decrements it afterwards, so the cap counts nesting depth across the whole top-level
/// conversion rather than the number of tiles changed in one branch.
fn spread_grass(
    world: &mut World,
    i: i32,
    j: i32,
    dirt: u16,
    grass: u16,
    repeat: bool,
    spread: &mut usize,
) {
    if !in_world(world, i, j, 10) {
        return;
    }
    let tile = world.tile(i, j);
    if !tile.is_active() || tile.block != dirt {
        return;
    }
    // The ordinary `isGeneratingOrLoadingWorld` branch reduces to this test for `dirt != 0`.
    // Vanilla additionally refuses dirt above `Main.worldSurface` when `dirt == 0`; that is the
    // only caller shape that can reach it, and is kept for the later passes that use it.
    if dirt == 0 && j >= i32::from(world.surface) {
        return;
    }

    let min_x = (i - 1).max(0);
    let max_x = (i + 2).min(world.width());
    let min_y = (j - 1).max(0);
    let max_y = (j + 2).min(world.height());

    // Vanilla treats a pocket of lava as solid for this test: it sets the flag back to true and
    // breaks. That means a mud tile with lava in its neighbourhood is never converted.
    let mut enclosed = true;
    for x in min_x..max_x {
        for y in min_y..max_y {
            let neighbour = world.tile(x, y);
            if !neighbour.is_active() || !tile_solid::solid(neighbour.block) {
                enclosed = false;
            }
            if neighbour.liquid > 0 && neighbour.liquid_kind == terrustia_proto::Liquid::Lava {
                enclosed = true;
                break;
            }
        }
    }
    if enclosed {
        return;
    }

    if !can_be_cleared(tile.block) {
        return;
    }
    // The grass-specific tree guards in vanilla (`grass == 23/661/199/662/109` above a tile 27)
    // cannot apply to jungle grass.
    if grass != 60
        && matches!(grass, 23 | 661 | 199 | 662 | 109)
        && world.tile(i, j - 1).block == 27
    {
        return;
    }

    // `TryConvertingOrKillingTreesAboveIfTheyWouldBecomeInvalid` is a no-op here: no tree pass
    // has run yet, so nothing above mud can be a tree type. It becomes load-bearing when the
    // later grass passes run after `Surface Plants`.
    let mut tile = world.tile(i, j);
    tile.block = grass;
    tile.flags.set(TileFlags::ACTIVE, true);
    world.set_tile(i, j, tile);

    if !repeat {
        return;
    }
    for x in min_x..max_x {
        for y in min_y..max_y {
            let neighbour = world.tile(x, y);
            if !neighbour.is_active() || neighbour.block != dirt {
                continue;
            }
            if *spread < GRASS_SPREAD_LIMIT {
                *spread += 1;
                spread_grass(world, x, y, dirt, grass, true, spread);
                *spread -= 1;
            }
        }
    }
}

/// `WorldGen.tileCounterNext`: a four-way flood fill capped at 20 tiles.
fn tile_counter(world: &World, x: i32, y: i32) -> Vec<(i32, i32)> {
    fn next(world: &World, found: &mut Vec<(i32, i32)>, x: i32, y: i32) {
        if found.len() >= TILE_COUNTER_MAX {
            return;
        }
        // The bounds are inclusive on both ends in vanilla (`x > maxTilesX - 5`), so the last
        // valid column is `width - 5`.
        if x < 5 || x > world.width() - 5 || y < 5 || y > world.height() - 5 {
            return;
        }
        let tile = world.tile(x, y);
        if !tile.is_active() || !tile_solid::solid(tile.block) || !can_be_cleared(tile.block) {
            return;
        }
        if found.contains(&(x, y)) {
            return;
        }
        found.push((x, y));
        next(world, found, x - 1, y);
        next(world, found, x + 1, y);
        next(world, found, x, y - 1);
        next(world, found, x, y + 1);
    }

    let mut found = Vec::with_capacity(TILE_COUNTER_MAX);
    next(world, &mut found, x, y);
    found
}

/// `WorldGen.tileCounterKill`: clear the active bit of every tile the counter found.
fn tile_counter_kill(world: &mut World, found: &[(i32, i32)]) {
    for &(x, y) in found {
        let mut tile = world.tile(x, y);
        tile.flags.set(TileFlags::ACTIVE, false);
        world.set_tile(x, y, tile);
    }
}

/// `WorldGen.ScanTileColumnAndRemoveClumps`.
fn scan_column_and_remove_clumps(world: &mut World, x: i32) {
    let mut run = 0usize;
    let mut run_start = 0i32;
    for y in 10..world.height() - 10 {
        let tile = world.tile(x, y);
        if tile.is_active() && tile_solid::solid(tile.block) && can_be_cleared(tile.block) {
            if run == 0 {
                run_start = y;
            }
            run += 1;
            continue;
        }
        if run > 0 && run < TILE_COUNTER_MAX {
            let found = tile_counter(world, x, run_start);
            if found.len() < TILE_COUNTER_MAX {
                tile_counter_kill(world, &found);
            }
        }
        run = 0;
    }
}

/// Run the pass.
pub fn run(vars: &mut GenVars) {
    let width = vars.world.width();
    let height = vars.world.height();

    // `NotTheBees()` is the first statement of the pass. It returns without drawing or writing on
    // an ordinary seed; the conversion port is called only when the world is actually a bees
    // world, which the parity oracle's ordinary reference does not exercise.

    // Every active tile gets a fresh top-level `grassSpread` counter, including tiles that are
    // not mud and return from `SpreadGrass` immediately.
    for i in 0..width {
        for j in 0..height {
            if vars.world.tile(i, j).is_active() {
                let mut spread = 0usize;
                spread_grass(&mut vars.world, i, j, MUD, JUNGLE_GRASS, true, &mut spread);
            }
        }
    }

    for x in 10..width - 10 {
        scan_column_and_remove_clumps(&mut vars.world, x);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terrustia_proto::Tile;

    /// A mud tile with one exposed neighbour converts; one buried in solid stone does not.
    #[test]
    fn enclosed_mud_is_left_alone() {
        let mut world = World::empty(60, 60, "test");
        for y in 10..50 {
            for x in 10..50 {
                world.set_tile(x, y, Tile::block(1));
            }
        }
        world.set_tile(30, 30, Tile::block(MUD));
        let mut spread = 0;
        spread_grass(&mut world, 30, 30, MUD, JUNGLE_GRASS, true, &mut spread);
        assert_eq!(world.tile(30, 30).block, MUD);
    }

    /// A mud tile with an open neighbour converts, and the recursively reachable mud follows.
    #[test]
    fn exposed_mud_spreads_to_reachable_mud() {
        let mut world = World::empty(60, 60, "test");
        for x in 10..50 {
            world.set_tile(x, 30, Tile::block(MUD));
        }
        let mut spread = 0;
        spread_grass(&mut world, 30, 30, MUD, JUNGLE_GRASS, true, &mut spread);
        // `grassSpread` is a nesting depth, not a tally: vanilla increments before the recursive
        // call and decrements after (`WorldGen.cs:75853-75857`), and the pass zeroes it before
        // every top-level call (`WorldGen.cs:12527`), so a finished conversion leaves it at 0.
        assert_eq!(spread, 0);
        assert_eq!(world.tile(30, 30).block, JUNGLE_GRASS);
        assert_eq!(world.tile(20, 30).block, JUNGLE_GRASS);
    }

    /// A connected clump shorter than 20 is cleared; one of 20 or more is kept.
    #[test]
    fn only_small_connected_clumps_are_removed() {
        let mut world = World::empty(80, 80, "test");
        for y in 20..23 {
            for x in 20..23 {
                world.set_tile(x, y, Tile::block(1));
            }
        }
        for x in 40..60 {
            world.set_tile(x, 40, Tile::block(1));
        }
        scan_column_and_remove_clumps(&mut world, 21);
        assert!(!world.tile(21, 21).is_active());
        for x in 40..60 {
            scan_column_and_remove_clumps(&mut world, x);
        }
        assert!(world.tile(50, 40).is_active());
    }
}

//! `Wavy Caves`: nothing, unless the world was made from the Don't Starve secret seed.
//!
//! Transcribed from the `Wavy Caves` delegate in `WorldGen.AddPasses`
//! (`WorldGen.cs:12270-12310`), whose whole body sits inside
//! `if (!Skyblock.denyAllGeneration && dontStarveWorldGen)`.
//!
//! This is the first entry in [`super::PASSES`] that exists to do nothing, and it is still needed:
//! the registry is matched to the manifest by position, so an absent pass would line every later
//! one up against the wrong record. The reference world's manifest confirms the reading - it
//! records `143337951` here, which is seed 12345's very first draw, the value a pass that draws
//! nothing leaves behind. The manifest reads the same way for `Mud Caves To Grass`, `Slush` and
//! `Pyramids`.
//!
//! What the branch would do, on a Don't Starve world: 35 worms scaled by world width, spaced at
//! least 80 rows apart by a reroll loop that gives up after 100 tries, each a `WavyCaverer` run
//! from a column that walks left to right with the progress fraction. Worth porting if secret seeds
//! ever come into scope; not worth a stub that could silently drift.

use super::passes::GenVars;

/// Run the pass, returning how many draws it took: none, for an ordinary world.
pub fn run_counted(_: &mut GenVars) -> usize {
    0
}

/// The pass, as the registry calls it.
pub fn run(vars: &mut GenVars) {
    let _ = run_counted(vars);
}

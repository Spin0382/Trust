//! `WorldGen.Caverer` and its `digTunnel`, the two functions behind `Surface Caves`.
//!
//! Transcribed from `WorldGen.cs:80190-80298` (`Caverer`) and `WorldGen.cs:80301-80370`
//! (`digTunnel`).
//!
//! These are the old worm-shaped caves, and they are a different animal from `TileRunner`: instead
//! of a tapered box walking a jittered drift, a tunnel is a diamond of radius `Size` steepled
//! around a moving point, and the radius itself breathes between 60% and 200% of its start as it
//! goes. `Caverer` is then a coin flip between two shapes:
//!
//! - **case 0**, seven or eight branches from one origin. Each branch digs a short wet-less
//!   tunnel, picks a *second, independent* direction and digs a long thin one from the same place,
//!   and finishes with a `TileRunner` pocket at the end of that second tunnel. The branch counter
//!   advances the origin by the first tunnel only - the second one's endpoint is used just for the
//!   pocket - so branches spread out like ribs rather than following one another.
//! - **case 1**, fifteen to twenty-nine steps of a single tunnel, always `Wet`, which is the one
//!   that leaves water in the cave it cuts.
//!
//! Two details are load-bearing for the RNG stream. Inside `digTunnel` the world-bounds test comes
//! *after* the jittered diamond test in the same `&&` chain, so an out-of-world cell still consumes
//! its `Next(-10, 11)`. And `Caverer`'s directions are built as `Next(100) * 0.01` with the second
//! component taken as `1.0 -` the first, then each sign flipped by its own `Next(2)`; the two sign
//! draws happen after the value draw, not before.
//!
//! `Caverer`'s body is wrapped in a `try`/`catch` in the original. Ours cannot throw: the world
//! accessor returns air outside its bounds rather than raising, and `digTunnel` clamps its own
//! starting point into the world first. So no exception can reach the handler, and there is no
//! counterpart to it here.

use super::rand::UnifiedRandom;
use super::tile_runner::{RunnerContext, tile_runner};
use crate::world::World;

/// A generator that also counts its draws, so the pass can check itself against the manifest.
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
}

/// One signed unit-ish direction pair: `d = Next(100) * 0.01`, companion `1.0 - d`, each sign
/// flipped by its own `Next(2)`.
fn random_direction(rng: &mut Draws<'_>) -> (f64, f64) {
    let mut x = f64::from(rng.next_max(100)) * 0.01;
    let mut y = 1.0 - x;
    if rng.next_max(2) == 0 {
        x = -x;
    }
    if rng.next_max(2) == 0 {
        y = -y;
    }
    (x, y)
}

/// `WorldGen.digTunnel`. Returns the tunnel's end point, which is what the caller continues from.
///
/// The radius `width` starts at `size` and drifts by `Next(-50, 51) * 0.03` per step inside
/// `[size * 0.6, size * 2]`. `wobble` is a separate accumulator from the caller's direction: the
/// caller's `xDir`/`yDir` are constants for the whole tunnel and the wobble is added to them, so
/// a tunnel keeps its heading and wanders around it.
///
/// The argument list is `digTunnel`'s own, plus the world and the counting generator that
/// `WorldGen`'s statics supply; as with `tile_runner`, the shape is kept to match the original.
#[allow(clippy::too_many_arguments)]
fn dig_tunnel(
    world: &mut World,
    rng: &mut Draws<'_>,
    x: f64,
    y: f64,
    dir_x: f64,
    dir_y: f64,
    steps: i32,
    size: i32,
    wet: bool,
) -> (f64, f64) {
    let width_tiles = f64::from(size);
    let mut px = x.clamp(
        width_tiles + 1.0,
        f64::from(world.width()) - width_tiles - 1.0,
    );
    let mut py = y.clamp(
        width_tiles + 1.0,
        f64::from(world.height()) - width_tiles - 1.0,
    );
    let mut width = width_tiles;
    let mut wobble_x = 0.0;
    let mut wobble_y = 0.0;

    for _ in 0..steps {
        let from_x = (px - width) as i32;
        let to_x = px + width;
        let from_y = (py - width) as i32;
        let to_y = py + width;
        for column in from_x.. {
            if f64::from(column) > to_x {
                break;
            }
            for row in from_y.. {
                if f64::from(row) > to_y {
                    break;
                }
                // The bounds test follows the diamond test, exactly as the original orders them, so
                // a cell outside the world still spends its `Next(-10, 11)`.
                let jitter = 1.0 + f64::from(rng.next_range(-10, 11)) * 0.005;
                if (f64::from(column) - px).abs() + (f64::from(row) - py).abs() < width * jitter
                    && column >= 0
                    && column < world.width()
                    && row >= 0
                    && row < world.height()
                {
                    let mut tile = world.tile(column, row);
                    tile.flags.set(terrustia_proto::TileFlags::ACTIVE, false);
                    if wet {
                        tile.liquid = u8::MAX;
                        tile.liquid_kind = terrustia_proto::Liquid::Water;
                    }
                    world.set_tile(column, row, tile);
                }
            }
        }

        width += f64::from(rng.next_range(-50, 51)) * 0.03;
        width = width.clamp(width_tiles * 0.6, f64::from(size * 2));
        wobble_x = (wobble_x + f64::from(rng.next_range(-20, 21)) * 0.01).clamp(-1.0, 1.0);
        wobble_y = (wobble_y + f64::from(rng.next_range(-20, 21)) * 0.01).clamp(-1.0, 1.0);
        px += (dir_x + wobble_x) * 0.6;
        py += (dir_y + wobble_y) * 0.6;
    }

    (px, py)
}

/// `WorldGen.Caverer`.
pub fn caverer(
    world: &mut World,
    rand: &mut UnifiedRandom,
    ctx: RunnerContext,
    x: i32,
    y: i32,
) -> usize {
    let mut rng = Draws::new(rand);
    // `TileRunner` draws from the same generator but reports its own count, so the two kinds add
    // up separately here.
    let mut runner_draws = 0usize;
    let mut px = f64::from(x);
    let mut py = f64::from(y);

    match rng.next_max(2) {
        // Seven or eight ribs from one origin, each a short thick tunnel followed by a long thin
        // one in an unrelated direction, with a pocket at that one's end.
        0 => {
            let branches = rng.next_range(7, 9);
            let (mut dir_x, mut dir_y) = random_direction(&mut rng);
            for _ in 0..branches {
                let steps = rng.next_range(6, 20);
                let size = rng.next_range(4, 9);
                (px, py) = dig_tunnel(world, &mut rng, px, py, dir_x, dir_y, steps, size, false);
                dir_x = (dir_x + f64::from(rng.next_range(-20, 21)) * 0.1).clamp(-1.5, 1.5);
                dir_y = (dir_y + f64::from(rng.next_range(-20, 21)) * 0.1).clamp(-1.5, 1.5);
                let (rib_x, rib_y) = random_direction(&mut rng);
                let steps = rng.next_range(30, 50);
                let size = rng.next_range(3, 6);
                let (tip_x, tip_y) =
                    dig_tunnel(world, &mut rng, px, py, rib_x, rib_y, steps, size, false);
                let strength = f64::from(rng.next_range(10, 20));
                let pocket_steps = rng.next_range(5, 10);
                runner_draws += tile_runner(
                    world,
                    rng.rand,
                    ctx,
                    tip_x as i32,
                    tip_y as i32,
                    strength,
                    pocket_steps,
                    -1,
                    false,
                    0.0,
                    0.0,
                    false,
                    true,
                    -1,
                );
            }
        }
        // One long wet worm: every step's tunnel continues from the last one's end.
        _ => {
            let steps = rng.next_range(15, 30);
            let (mut dir_x, mut dir_y) = random_direction(&mut rng);
            for _ in 0..steps {
                let tunnel_steps = rng.next_range(5, 15);
                let size = rng.next_range(2, 6);
                (px, py) = dig_tunnel(
                    world,
                    &mut rng,
                    px,
                    py,
                    dir_x,
                    dir_y,
                    tunnel_steps,
                    size,
                    true,
                );
                dir_x = (dir_x + f64::from(rng.next_range(-20, 21)) * 0.1).clamp(-1.5, 1.5);
                dir_y = (dir_y + f64::from(rng.next_range(-20, 21)) * 0.1).clamp(-1.5, 1.5);
            }
        }
    }

    rng.count + runner_draws
}

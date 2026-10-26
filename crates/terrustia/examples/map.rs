//! Print a downsampled ASCII map of a `.wld`, for spotting biome seams and large holes.
//!
//! ```sh
//! cargo run --release -p terrustia --example map -- world.wld [step]
//! ```

use std::{env, process::ExitCode};

use terrustia::world::wld;
use terrustia_proto::{Liquid, Tile};

fn glyph(tile: Tile) -> char {
    if tile.liquid > 0 {
        return if tile.liquid_kind == Liquid::Lava {
            'L'
        } else {
            '~'
        };
    }
    if !tile.is_active() {
        return ' ';
    }
    match tile.block {
        0 | 2 | 30 | 40 | 59 | 147 | 161 => '.',
        1 => '#',
        5 => 'W',
        7 => 'c',
        6 => 'i',
        9 => 'v',
        8 => 'g',
        22 => 'e',
        204 => 'E',
        21 => 'C',
        23 | 25 | 199 | 203 => 'x',
        41 | 43 | 44 => 'D',
        53 | 112 | 116 | 234 | 330 | 331 | 332 | 333 | 495 => 's',
        57 | 58 => 'H',
        60..=69 => 'J',
        226 => 'T',
        _ => '?',
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: map <world.wld> [step]");
        return ExitCode::FAILURE;
    };
    let step = args
        .get(1)
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(32);
    let world = match wld::load(std::path::Path::new(path)) {
        Ok(world) => world,
        Err(e) => {
            eprintln!("load failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "{} ({}x{}) step {step}; legend: # stone . soil s sand J jungle x evil D dungeon T temple H hell W tree ~ water L lava",
        world.name,
        world.width(),
        world.height()
    );
    for y in (0..world.height()).step_by(step as usize) {
        let mut row = String::with_capacity((world.width() / step + 1) as usize);
        for x in (0..world.width()).step_by(step as usize) {
            row.push(glyph(world.tile(x, y)));
        }
        println!("{row}");
    }
    let mut profile = String::with_capacity((world.width() / step + 1) as usize);
    for x in (0..world.width()).step_by(step as usize) {
        let mut y = 0;
        while y < world.height() && !world.tile(x, y).is_active() {
            y += 1;
        }
        profile.push(if y >= world.height() {
            '!'
        } else {
            glyph(world.tile(x, y))
        });
    }
    println!("surface: {profile}");
    ExitCode::SUCCESS
}

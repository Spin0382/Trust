//! A basic, interactive first-run setup wizard.
//!
//! Not the zero-flag default path — that stays exactly what it always was ("point it at nothing
//! and it generates a world," non-interactive, no prompts) — this is a *separate*, opt-in-
//! triggered flow for someone who wants a little more hand-holding: `terrustia --setup`
//! explicitly, or a first-run launch this module judges likely to be someone who just downloaded
//! the raw binary and ran it right where it landed (see [`should_auto_trigger`]).
//!
//! What it produces is nothing more exotic than an ordinary `terrustia.toml` — the wizard is a
//! way of writing one without knowing the file format, not a second configuration mechanism.
//! Once it has written that file, `main` proceeds exactly as if `--config <the file it wrote>`
//! had been passed, so every other precedence rule (environment, CLI flags) keeps working
//! unchanged from that point on.
//!
//! **The install-guard property this exists to provide**: double-clicking the raw binary must
//! never scatter a world file and a `terrustia.toml` into wherever it happens to sit (`~/Downloads`,
//! most likely). The wizard's dedicated directory holds the config, and the world is generated
//! through [`crate::worlds::new_world_path`] into the server's own `worlds/` directory (the same
//! place `--new` writes to), so neither file lands beside the executable. The dedicated directory is
//! refused outright if it already has anything in it, rather than writing into whatever is there.

use std::{
    io::{self, IsTerminal, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use crate::config::Config;

/// Terraria's three world-size presets, in tiles. Small is this project's default (and the game's
/// own default in `terrustia.toml.example`); Medium and Large are vanilla's other two sizes. All
/// three are whole numbers of 200x150 sections, which is what [`Config::validate`] requires of a
/// generated world. The Large preset is the one `docs/performance.md` measures against a real
/// 8400x2400 world.
const WORLD_SIZE_SMALL: (i32, i32) = (
    crate::world::worldgen::SMALL_WIDTH,
    crate::world::worldgen::SMALL_HEIGHT,
);
const WORLD_SIZE_MEDIUM: (i32, i32) = (6400, 1800);
const WORLD_SIZE_LARGE: (i32, i32) = (8400, 2400);

/// Whether a plain, zero-flag `terrustia` invocation should offer the wizard instead of falling
/// through to the ordinary non-interactive default (generate an ephemeral world, serve it).
///
/// The signal: the current directory is the same directory the running executable itself lives
/// in, and nothing terrustia-shaped is in it yet (no `terrustia.toml`, no `.wld` file). That
/// combination is what "downloaded a release archive, extracted it, and ran `./terrustia` from
/// right there" actually looks like at the process level — genuinely different from "installed
/// via a package manager to `/usr/local/bin`, then `cd`'d into `~/my-server` and ran `terrustia`
/// with no flags," which is the existing, legitimate zero-config path and must keep working
/// exactly as it always has. A double-clicked binary on Windows/Linux desktops lands in this same
/// shape (its own directory becomes the process's working directory), which is the actual thing
/// this project's own plan calls out by name.
pub fn should_auto_trigger(args_are_empty: bool) -> bool {
    if !args_are_empty {
        return false;
    }
    // An interactive wizard needs somebody to interact with, and the shape it recognises below is
    // also exactly the shape of a headless install: extract the release archive to /opt/terrustia
    // and start it under systemd, a container, or `nohup`, and the working directory is the
    // binary's own with nothing terrustia-shaped in it yet. Without this the wizard launched at
    // every boot of a real server and went straight to `io::stdin().read_line`, which on a closed
    // stdin returns `Ok(0)` for every prompt, so the run took silent defaults and a
    // `terrustia.toml` appeared from nowhere. Backgrounded with a tty still attached it is worse:
    // reading stdin raises `SIGTTIN` and the process stops, with nothing printed to say why.
    // Falling through returns the caller to the ordinary non-interactive zero-flag path, which
    // already does the right thing for a headless host. Same guard, and same reasoning, as
    // `console::spawn`'s: both ends, because output that nobody can read is as useless as input
    // nobody can give.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return false;
    }
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(exe_dir) = exe.parent() else {
        return false;
    };
    // Canonicalize both sides: `current_exe()` on some platforms returns a path through a symlink
    // (Homebrew's Cellar, for one), and a naive `==` would then never match even when the two
    // really are the same directory.
    let same_dir = match (cwd.canonicalize(), exe_dir.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => cwd == exe_dir,
    };
    if !same_dir {
        return false;
    }
    if cwd.join("terrustia.toml").exists() {
        return false;
    }
    has_no_world_files(&cwd)
}

fn has_no_world_files(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    !entries.flatten().any(|e| {
        e.path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wld"))
    })
}

#[derive(Debug, thiserror::Error)]
pub enum WizardError {
    #[error("{0}")]
    Cancelled(String),
    #[error("reading input: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Runs the interactive prompts on stdin/stdout and returns the path to the `terrustia.toml` it
/// wrote. Blocking by design — there is nothing else running yet for it to block, this is the
/// very first thing `main` does when it triggers — callers on an async runtime should still run
/// it inside `spawn_blocking` regardless, so a future caller earlier in the startup sequence does
/// not accidentally stall a worker thread other tasks depend on.
pub fn run_wizard() -> Result<PathBuf, WizardError> {
    println!("terrustia setup");
    println!("===============");
    println!(
        "A few questions, then this writes a terrustia.toml and gets out of your way. Press \
         Enter to accept the default shown in [brackets]."
    );
    println!();

    let default_dir = default_dedicated_dir();
    let dir = prompt(
        &format!(
            "Directory for terrustia's own config (must be empty or not yet exist) [{}]",
            default_dir.display()
        ),
        &default_dir.display().to_string(),
    )?;
    let dir = PathBuf::from(dir);
    ensure_empty_or_new(&dir)?;

    let world_name = prompt("World name", "Terrustia")?;
    let max_players = prompt_number("Max players", 8, 1, crate::config::MAX_PLAYERS)?;
    let panel_enabled = prompt_yes_no(
        "Enable the web admin panel (browser-based start/stop, player list, whitelist, \
         settings)? It only ever listens on this machine, never the network",
        true,
    )?;

    // Every default below is read from one Config::default(), so a plain Enter in the wizard and
    // the zero-flag path choose the same values.
    let defaults = Config::default();
    let world_size = prompt_world_size((defaults.world_width, defaults.world_height))?;
    let seed = prompt_seed(defaults.seed)?;
    let listen = prompt_listen(defaults.listen)?;
    let password = prompt("Server password (empty leaves the server open)", "")?;
    let autosave_secs = prompt_u64(
        "Autosave interval in seconds (0 disables automatic saves)",
        defaults.autosave_secs,
    )?;

    let world_path = crate::worlds::new_world_path(&world_name).map_err(WizardError::Invalid)?;
    if let Some(parent) = world_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if world_path.exists() {
        return Err(WizardError::Invalid(format!(
            "a world named \"{world_name}\" already exists at {} — run terrustia --setup again \
             with a different name, or --world \"{world_name}\" to serve the one you already have",
            world_path.display()
        )));
    }

    std::fs::create_dir_all(&dir)?;
    let config = Config {
        world_name: world_name.clone(),
        save_file: Some(world_path.clone()),
        max_players,
        panel_enabled,
        world_width: world_size.0,
        world_height: world_size.1,
        seed,
        listen,
        password,
        autosave_secs,
        ..defaults
    };
    // The prompts already apply the same rules one at a time, but the real check is the same
    // `Config::validate` every other entry point runs, against the final combination rather than
    // against each answer in isolation.
    config
        .validate()
        .map_err(|e| WizardError::Invalid(e.to_string()))?;
    let config_path = dir.join("terrustia.toml");
    write_config(&config_path, &config)?;

    println!();
    println!("Written to {}", config_path.display());
    println!(
        "The world \"{world_name}\" will be generated at {} on first start.",
        world_path.display()
    );
    println!("Starting terrustia now — Ctrl-C to stop it, same as any other run.");
    println!();

    Ok(config_path)
}

/// The user-facing label for a size, when it is one of the presets the prompt offers. A custom
/// `WIDTHxHEIGHT` default falls back to showing its dimensions instead.
fn world_size_name(size: (i32, i32)) -> Option<&'static str> {
    if size == WORLD_SIZE_SMALL {
        Some("small")
    } else if size == WORLD_SIZE_MEDIUM {
        Some("medium")
    } else if size == WORLD_SIZE_LARGE {
        Some("large")
    } else {
        None
    }
}

fn prompt_world_size(default: (i32, i32)) -> Result<(i32, i32), WizardError> {
    let fallback = world_size_name(default)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}x{}", default.0, default.1));
    loop {
        let value = prompt(
            "World size (small, medium, large, or WIDTHxHEIGHT in tiles)",
            &fallback,
        )?;
        let size = match parse_world_size(&value) {
            Ok(size) => size,
            Err(message) => {
                println!("{message}");
                continue;
            }
        };
        if let Err(message) = validate_world_size(size) {
            println!("{message}");
            continue;
        }
        return Ok(size);
    }
}

/// Preset names, their 1-based menu positions, and `WIDTHxHEIGHT` all resolve here. Anything that
/// parses still goes through [`Config::validate`] before it is accepted, so this does not grow a
/// second, looser copy of the size rules.
fn parse_world_size(value: &str) -> Result<(i32, i32), String> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "small" | "s" | "1" => Ok(WORLD_SIZE_SMALL),
        "medium" | "m" | "2" => Ok(WORLD_SIZE_MEDIUM),
        "large" | "l" | "3" => Ok(WORLD_SIZE_LARGE),
        _ => {
            let Some((width, height)) = value.split_once('x') else {
                return Err(
                    "please choose small, medium or large, or give a size like 6400x1800".into(),
                );
            };
            let width = width
                .trim()
                .parse::<i32>()
                .map_err(|_| "world width must be a whole number of tiles".to_string())?;
            let height = height
                .trim()
                .parse::<i32>()
                .map_err(|_| "world height must be a whole number of tiles".to_string())?;
            Ok((width, height))
        }
    }
}

/// The same `Config::validate` a config file gets, applied to one candidate size. A generated
/// world has no `world_file`, so the size checks in `validate` are the ones that run.
fn validate_world_size(size: (i32, i32)) -> Result<(), String> {
    Config {
        world_width: size.0,
        world_height: size.1,
        ..Config::default()
    }
    .validate()
    .map_err(|e| e.to_string())
}

/// A seed is any text, exactly as `--seed` accepts it: a plain number reproduces that number, and
/// anything else is hashed into one (see `worldgen::secret_seed::numeric_seed`). This writes the
/// resulting number to `seed`, so a text seed typed here reproduces the same generated world as a
/// later `--seed` with the same text. Vanilla's secret-seed names are only recognised on `--seed`
/// itself, which is why a non-numeric answer gets a note.
fn prompt_seed(default: u64) -> Result<u64, WizardError> {
    let value = prompt(
        "World seed (a number, or any text, which is hashed; empty for 0)",
        &default.to_string(),
    )?;
    if let Ok(seed) = value.parse::<u64>() {
        return Ok(seed);
    }
    println!(
        "note: text seeds are hashed into a number here; vanilla's secret-seed names (for \
         example \"for the worthy\") are only recognised by --seed on the command line"
    );
    Ok(crate::world::worldgen::secret_seed::numeric_seed(&value))
}

fn prompt_listen(default: SocketAddr) -> Result<SocketAddr, WizardError> {
    loop {
        let value = prompt(
            "Listen address (IP:port, or just a port on the default address)",
            &default.to_string(),
        )?;
        match parse_listen(&value, default) {
            Ok(addr) => return Ok(addr),
            Err(message) => println!("{message}"),
        }
    }
}

fn parse_listen(value: &str, default: SocketAddr) -> Result<SocketAddr, String> {
    let value = value.trim();
    if let Ok(addr) = value.parse::<SocketAddr>() {
        return Ok(addr);
    }
    // A bare port is the common answer to "what port should this listen on", and it keeps the
    // address `Config::default` chose rather than inventing a second default.
    if let Ok(port) = value.parse::<u16>() {
        return Ok(SocketAddr::new(default.ip(), port));
    }
    Err("please enter an address like 0.0.0.0:7777, or a port like 7777".into())
}

fn prompt_u64(question: &str, default: u64) -> Result<u64, WizardError> {
    loop {
        let value = prompt(question, &default.to_string())?;
        match value.parse::<u64>() {
            Ok(n) => return Ok(n),
            Err(_) => println!("please enter a whole number, 0 or greater"),
        }
    }
}

fn default_dedicated_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var_os("HOME").map(PathBuf::from);
    home.unwrap_or_else(|| PathBuf::from("."))
        .join("terrustia-server")
}

/// Refuses outright if `dir` exists and already has anything in it — the whole point of a
/// *dedicated* directory is that it is not also wherever a stray download or an old install left
/// something behind. Creating it is left to the caller, once every other prompt has succeeded
/// too, so an early cancellation or a later mistake never leaves a half-set-up empty directory.
fn ensure_empty_or_new(dir: &Path) -> Result<(), WizardError> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(WizardError::Invalid(format!(
                    "{} already exists and is not empty — pick an empty or new directory, so \
                     nothing already there is at risk of being overwritten",
                    dir.display()
                )));
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(WizardError::Io(e)),
    }
}

fn write_config(path: &Path, config: &Config) -> Result<(), WizardError> {
    // Only the fields the wizard actually asked about, written explicitly — a full
    // `toml::to_string(config)` would also freeze every other field's current default into the
    // file forever, silently opting this install out of any default this project changes later.
    let mut out = String::new();
    out.push_str(
        "# Written by `terrustia --setup`. Every key is optional — see \
                   terrustia.toml.example for the rest.\n\n",
    );
    out.push_str(&format!("world_name = {:?}\n", config.world_name));
    out.push_str(&format!(
        "save_file = {:?}\n",
        config
            .save_file
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
    ));
    out.push_str(&format!("max_players = {}\n", config.max_players));
    out.push_str(&format!("panel_enabled = {}\n", config.panel_enabled));
    out.push_str(&format!("world_width = {}\n", config.world_width));
    out.push_str(&format!("world_height = {}\n", config.world_height));
    out.push_str(&format!("seed = {}\n", config.seed));
    out.push_str(&format!("listen = \"{}\"\n", config.listen));
    out.push_str(&format!("password = {:?}\n", config.password));
    out.push_str(&format!("autosave_secs = {}\n", config.autosave_secs));
    // Through the same temp-file-and-rename every other writer here uses. The wizard writes into a
    // directory it has just checked is empty, so there is rarely a previous file to protect - but
    // `--setup` re-run against an existing install is a supported thing to do, and a half-written
    // `terrustia.toml` is a server that will not start at all. The failure is explained for the
    // same reason: this is the one moment the operator is definitely sitting at the terminal.
    crate::safe_write::write_atomic("writing the config file", path, out.as_bytes())?;
    Ok(())
}

fn prompt(question: &str, default: &str) -> Result<String, WizardError> {
    print!("{question} [{default}]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_number(
    question: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, WizardError> {
    loop {
        let value = prompt(question, &default.to_string())?;
        match value.parse::<usize>() {
            Ok(n) if n >= min && n <= max => return Ok(n),
            _ => println!("please enter a number between {min} and {max}"),
        }
    }
}

fn prompt_yes_no(question: &str, default: bool) -> Result<bool, WizardError> {
    let default_str = if default { "Y/n" } else { "y/N" };
    loop {
        let value = prompt(question, default_str)?.to_lowercase();
        match value.as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            v if v == default_str.to_lowercase() => return Ok(default),
            _ => println!("please answer y or n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_that_does_not_match_the_executables_own_never_auto_triggers() {
        // `should_auto_trigger` reads the *real* `current_exe`/`current_dir` — in a `cargo test`
        // run those are never the same directory (the test binary lives under `target/debug/
        // deps/`, while the working directory `cargo test` sets is the crate root), so this
        // exercises the real function against its real environment rather than a mock of it: the
        // "cwd == exe's own directory" guard must read false here, and nothing after it should
        // ever run (in particular, it must not touch the filesystem looking for `.wld` files).
        assert!(!should_auto_trigger(true));
    }

    #[test]
    fn args_present_always_skips_auto_trigger_without_touching_the_filesystem() {
        assert!(!should_auto_trigger(false));
    }

    #[test]
    fn an_empty_directory_is_accepted_and_a_nonexistent_one_too() {
        let dir =
            std::env::temp_dir().join(format!("terrustia-setup-test-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Does not exist yet:
        ensure_empty_or_new(&dir).expect("a directory that does not exist yet must be accepted");
        // Exists and is empty:
        std::fs::create_dir_all(&dir).unwrap();
        ensure_empty_or_new(&dir).expect("an existing empty directory must be accepted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_nonempty_directory_is_refused_rather_than_written_into() {
        let dir = std::env::temp_dir().join(format!(
            "terrustia-setup-test-nonempty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("something-already-here.txt"), b"do not touch").unwrap();
        let result = ensure_empty_or_new(&dir);
        assert!(
            result.is_err(),
            "a directory with something already in it must be refused"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("something-already-here.txt")).unwrap(),
            "do not touch",
            "the pre-existing file must be completely untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn has_no_world_files_is_true_only_when_none_exist() {
        let dir =
            std::env::temp_dir().join(format!("terrustia-setup-test-wld-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(has_no_world_files(&dir), "a fresh empty directory has none");
        std::fs::write(dir.join("SomeWorld.wld"), b"").unwrap();
        assert!(
            !has_no_world_files(&dir),
            "a directory with a .wld file must be detected"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_written_config_round_trips_through_the_real_toml_parser() {
        let dir =
            std::env::temp_dir().join(format!("terrustia-setup-test-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("terrustia.toml");
        let listen: SocketAddr = "127.0.0.1:7789".parse().unwrap();
        let config = Config {
            world_name: "A Test World".to_string(),
            save_file: Some(dir.join("A_Test_World.wld")),
            max_players: 16,
            panel_enabled: true,
            world_width: WORLD_SIZE_MEDIUM.0,
            world_height: WORLD_SIZE_MEDIUM.1,
            seed: 424_242,
            listen,
            password: "s3cret pass".to_string(),
            autosave_secs: 45,
            ..Config::default()
        };
        write_config(&config_path, &config).unwrap();

        let loaded = Config::load(&config_path).expect("the file this wizard wrote must load");
        assert_eq!(loaded.world_name, "A Test World");
        assert_eq!(loaded.save_file, config.save_file);
        assert_eq!(loaded.max_players, 16);
        assert!(loaded.panel_enabled);
        assert_eq!(loaded.world_width, WORLD_SIZE_MEDIUM.0);
        assert_eq!(loaded.world_height, WORLD_SIZE_MEDIUM.1);
        assert_eq!(loaded.seed, 424_242);
        assert_eq!(loaded.listen, listen);
        assert_eq!(loaded.password, "s3cret pass");
        assert_eq!(loaded.autosave_secs, 45);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every preset the size prompt offers must satisfy the same `Config::validate` a hand-written
    /// `terrustia.toml` gets, or the wizard could write a config the server refuses at boot.
    #[test]
    fn every_world_size_preset_passes_the_config_validation() {
        for size in [WORLD_SIZE_SMALL, WORLD_SIZE_MEDIUM, WORLD_SIZE_LARGE] {
            validate_world_size(size).unwrap_or_else(|e| {
                panic!("{size:?} is offered by the wizard but rejected by Config: {e}")
            });
        }
    }

    #[test]
    fn world_size_input_accepts_names_menu_numbers_and_dimensions() {
        assert_eq!(parse_world_size("small").unwrap(), WORLD_SIZE_SMALL);
        assert_eq!(parse_world_size("M").unwrap(), WORLD_SIZE_MEDIUM);
        assert_eq!(parse_world_size("3").unwrap(), WORLD_SIZE_LARGE);
        assert_eq!(parse_world_size("6400X1800").unwrap(), WORLD_SIZE_MEDIUM);
        assert_eq!(parse_world_size("400x300").unwrap(), (400, 300));
        assert!(parse_world_size("large please").is_err());
        // Parsing is not enough; the same size rules `Config::validate` applies still run.
        assert!(validate_world_size((399, 300)).is_err());
        assert!(validate_world_size((401, 300)).is_err());
        assert!(validate_world_size((400, 299)).is_err());
        assert!(validate_world_size((400, 300)).is_ok());
        assert!(validate_world_size((i32::from(i16::MAX) + 1, 300)).is_err());
    }

    #[test]
    fn listen_input_accepts_an_address_or_a_bare_port() {
        let default: SocketAddr = "0.0.0.0:7777".parse().unwrap();
        assert_eq!(
            parse_listen("127.0.0.1:1234", default).unwrap(),
            "127.0.0.1:1234".parse().unwrap()
        );
        assert_eq!(
            parse_listen("1234", default).unwrap(),
            "0.0.0.0:1234".parse().unwrap(),
            "a bare port keeps the default address"
        );
        assert_eq!(parse_listen("0", default).unwrap().port(), 0);
        assert!(parse_listen("localhost:1234", default).is_err());
        assert!(parse_listen("70000", default).is_err());
        assert!(parse_listen("", default).is_err());
    }

    /// A config the wizard cannot write must be refused out loud, with the previous one intact.
    ///
    /// `--setup` re-run against an existing install is a supported thing to do, and half a
    /// `terrustia.toml` is a server that will not start at all.
    #[cfg(unix)]
    #[test]
    fn a_config_that_cannot_be_written_leaves_the_previous_one_byte_identical() {
        let dir = crate::safe_write::tests::temp_dir("setup-readonly");
        let config_path = dir.join("terrustia.toml");
        let config = Config {
            world_name: "The Good One".to_string(),
            max_players: 8,
            ..Config::default()
        };
        write_config(&config_path, &config).expect("the first, good write");
        let before = std::fs::read(&config_path).expect("reading it back");

        let Some(_guard) = crate::safe_write::tests::ReadOnlyDir::new(&dir) else {
            eprintln!("skipping: this environment cannot make a directory read-only");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        };
        let replacement = Config {
            world_name: "The Doomed One".to_string(),
            ..config
        };
        let e = write_config(&config_path, &replacement)
            .expect_err("a read-only directory must refuse the write");
        let message = e.to_string();
        drop(_guard);

        assert!(
            message.contains("config file") && message.contains("writable by the account"),
            "the failure must name what it was doing and what to check, got: {message}"
        );
        assert_eq!(
            std::fs::read(&config_path).expect("reading it back"),
            before,
            "a failed write must leave the previous config byte-identical"
        );
        assert!(
            !crate::safe_write::temp_path(&config_path).exists(),
            "a failed write must clean up its own scratch file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The wizard's own directory disappearing between the prompts and the write is reported.
    #[test]
    fn a_vanished_directory_is_reported_rather_than_fatal() {
        let dir = crate::safe_write::tests::temp_dir("setup-vanished");
        let config_path = dir.join("terrustia.toml");
        std::fs::remove_dir_all(&dir).expect("removing the directory out from under it");

        let e = write_config(&config_path, &Config::default())
            .expect_err("a missing directory must refuse the write");
        assert!(
            e.to_string().contains("directory no longer exists"),
            "got: {e}"
        );
    }
}

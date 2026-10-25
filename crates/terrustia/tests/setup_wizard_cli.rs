//! `terrustia --setup` end to end: a real invocation of the compiled binary answering real
//! prompts over a real piped stdin, not a unit test of the individual pieces (`setup.rs`'s own
//! `#[cfg(test)]` module already covers those in isolation). What isn't proven anywhere else is
//! that the wizard actually reaches the filesystem the way an operator would experience it: a
//! `terrustia.toml` really lands in the dedicated directory, the world it names really gets
//! generated (into the platform's own Terraria world directory, the same place `--new` writes
//! to), and a second run against a directory that already has something in it is refused rather
//! than written into.
//!
//! Same scratch-`HOME`/`XDG_DATA_HOME`/`USERPROFILE` pattern `new_world_cli.rs` already
//! established, for the same reason: never touch the machine's real Terraria world directory.

mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use terrustia::config::Config;

/// Reads `stdout` on a background thread, forwarding each line to the returned channel. Needed
/// because a graceful `SIGTERM` only does anything once `main.rs`'s own `stop_signal()` has
/// actually registered its signal handler — which happens once the server reaches its main
/// `tokio::select!`, not the instant the process starts. A fixed sleep before signalling raced
/// that exact window in this test's first draft: the process was still doing synchronous world
/// generation when the signal arrived, so `SIGTERM`'s *default* disposition (terminate
/// immediately, no handler installed yet) killed it before any save ever ran. Waiting for the
/// real `"accepting connections"` log line — the same poll-don't-sleep discipline
/// `wait_for_file` above already uses — is what actually proves the handler is live.
fn stream_stdout_lines(stdout: std::process::ChildStdout) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_for_line(rx: &mpsc::Receiver<String>, needle: &str, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match rx.recv_timeout(remaining) {
            Ok(line) if line.contains(needle) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

fn scratch_home(label: &str) -> PathBuf {
    support::scratch_dir(&format!("terrustia-setup-cli-{label}"))
}

fn find_named(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(find_named(&path, name));
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            found.push(path);
        }
    }
    found
}

/// Same poll-rather-than-sleep reasoning as `new_world_cli.rs`'s own `wait_for_file`: a real
/// subprocess's first autosave landing inside a fixed wall-clock window is load-sensitive.
fn wait_for_file(dir: &Path, name: &str, timeout: Duration) -> Vec<PathBuf> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let found = find_named(dir, name);
        if found
            .iter()
            .any(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0))
        {
            return found;
        }
        if std::time::Instant::now() >= deadline {
            return found;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Poll for the wizard's config file, the same way [`wait_for_file`] polls for the world. A
/// fixed sleep here would be another load-sensitive window: the prompts are answered as soon as
/// the pipe accepts them, but the write itself is behind `safe_write`'s temp-file-and-rename.
fn wait_for_config(path: &Path, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while !path.exists() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    true
}

fn spawn_setup(home: &Path, listen: &str) -> std::process::Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_terrustia"));
    // Windows has no `kill -TERM`. The only way one process can ask another to stop *gracefully*
    // is a console control event, and `GenerateConsoleCtrlEvent` can only address a process group,
    // so the child has to be put in one of its own at spawn time. `CREATE_NEW_PROCESS_GROUP` is
    // 0x0000_0200. Without this the test's only option is `Child::kill`, which is
    // `TerminateProcess`: the moral equivalent of `SIGKILL`, no handler runs, and the world this
    // test then waits for is never written, because only a graceful stop writes it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0000_0200);
    }
    command
        .args(["--setup", "--listen", listen])
        .current_dir(home)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("xdg"))
        .env("USERPROFILE", home)
        .env_remove("TERRUSTIA_LOG")
        // No test may depend on the network. Both of these are `tokio::spawn`ed at boot, so left
        // on, every server spawned here makes a real GitHub request and multicasts for a UPnP
        // gateway. This is hygiene, not a fix for anything: the CLI-test flake recorded in TODO.md
        // was measured with both already off and still failed 5 runs in 8, so the cause is elsewhere.
        .env("TERRUSTIA_UPDATE_CHECK_ENABLED", "false")
        .env("TERRUSTIA_UPNP_ENABLED", "false")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn terrustia --setup")
}

#[test]
fn the_wizard_writes_a_config_and_generates_the_world_it_named() {
    let home = scratch_home("happy-path");
    std::fs::create_dir_all(&home).expect("scratch home");
    let dedicated_dir = home.join("dedicated-config-dir");

    // Killed on drop, so a failing assertion below cannot leave it holding its port.
    let mut child = support::Reaper(spawn_setup(&home, &support::free_addr()));
    let stdout_lines = stream_stdout_lines(child.stdout.take().expect("piped stdout"));
    {
        let stdin = child.stdin.as_mut().expect("piped stdin");
        // In prompt order: dedicated directory, world name, max players, panel enabled, then the
        // five world/server questions. An empty line accepts each shown default, which is what this
        // test is proving: the wizard's defaults are real, accepted values, not just labels.
        writeln!(stdin, "{}", dedicated_dir.display()).unwrap();
        writeln!(stdin, "CLI Wizard World").unwrap();
        writeln!(stdin, "4").unwrap();
        writeln!(stdin, "n").unwrap();
        writeln!(stdin).unwrap(); // world size: small
        writeln!(stdin).unwrap(); // world seed: 0
        writeln!(stdin).unwrap(); // listen: 0.0.0.0:7777
        writeln!(stdin).unwrap(); // password: none
        writeln!(stdin).unwrap(); // autosave: 300 seconds
    }

    let config_path = dedicated_dir.join("terrustia.toml");
    assert!(
        wait_for_config(&config_path, Duration::from_secs(30)),
        "the wizard should have written {} by now",
        config_path.display()
    );
    let written = std::fs::read_to_string(&config_path).expect("read the written config");
    assert!(
        written.contains("CLI Wizard World"),
        "expected the world name in the written config, got:\n{written}"
    );
    assert!(
        written.contains("max_players = 4"),
        "expected the chosen player count in the written config, got:\n{written}"
    );
    assert!(
        written.contains("panel_enabled = false"),
        "expected the chosen panel answer in the written config, got:\n{written}"
    );
    let defaults = Config::default();
    for (what, expected) in [
        (
            "world width",
            format!("world_width = {}", defaults.world_width),
        ),
        (
            "world height",
            format!("world_height = {}", defaults.world_height),
        ),
        ("world seed", format!("seed = {}", defaults.seed)),
        (
            "listen address",
            format!("listen = \"{}\"", defaults.listen),
        ),
        ("password", format!("password = {:?}", defaults.password)),
        (
            "autosave interval",
            format!("autosave_secs = {}", defaults.autosave_secs),
        ),
    ] {
        assert!(
            written.contains(&expected),
            "expected the default {what} ({expected:?}) in the written config, got:\n{written}"
        );
    }

    // The wizard hands off to an ordinary boot after writing the file. The world it named is not
    // written to disk until the first autosave or a clean shutdown. The autosave prompt's default
    // is 300 seconds, so unlike `new_world_cli.rs`'s own test (which pre-seeds a fast
    // `autosave_secs = 1`) this one asks for a real, graceful shutdown instead of waiting out that
    // default. `child.kill()` sends `SIGKILL` on Unix, which skips the save path entirely -
    // `SIGTERM`, via the real `kill` binary, is what `main.rs`'s own `stop_signal()` actually
    // listens for, but only *after* it has registered that handler inside its main
    // `tokio::select!`: sending it any earlier hits `SIGTERM`'s default disposition instead
    // (terminate immediately, no save) - this test's own first draft raced exactly that window with
    // a fixed sleep. `"accepting connections"` is the real, observed proof the handler is live,
    // the same poll-don't-sleep discipline `wait_for_file` above already applies to the filesystem
    // side of this.
    assert!(
        wait_for_line(
            &stdout_lines,
            "accepting connections",
            Duration::from_secs(30)
        ),
        "the server should have reached its main loop by now"
    );
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status();
    }
    // ...and Windows' own equivalent: a real Ctrl+Break to the child's process group, which
    // `stop_signal` listens for alongside the close and shutdown events. `Child::kill` here would
    // be `TerminateProcess`, which runs no handler at all, so the graceful save this test is
    // waiting on could never happen and the test could never pass on Windows. It never had to:
    // until 2026-09-05 the suite had only ever run on Linux.
    #[cfg(windows)]
    {
        // SAFETY: `GenerateConsoleCtrlEvent` takes an event id and a process group id and touches
        // nothing else. The group id is the child's own pid, which is what
        // `CREATE_NEW_PROCESS_GROUP` at spawn made it. `CTRL_BREAK_EVENT` is 1.
        #[allow(unsafe_code)]
        unsafe {
            unsafe extern "system" {
                fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
            }
            GenerateConsoleCtrlEvent(1, child.id());
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
    }

    let found = wait_for_file(&home, "CLI_Wizard_World.wld", Duration::from_secs(60));

    let _ = child.wait();

    assert_eq!(
        found.len(),
        1,
        "expected exactly one CLI_Wizard_World.wld under {}, found {:?}",
        home.display(),
        found
    );
    assert!(
        !found[0].starts_with(&dedicated_dir),
        "the world must not land inside the dedicated config directory"
    );

    let _ = std::fs::remove_dir_all(&home);
}

/// The same transcript, but answering every new question explicitly. The world size is given as
/// the smallest size `Config::validate` accepts (400x300) rather than a preset, so generation stays
/// quick enough to prove the written config is what the server actually boots from.
#[test]
fn the_wizard_writes_every_world_and_server_choice_it_asked_about() {
    let home = scratch_home("explicit-choices");
    std::fs::create_dir_all(&home).expect("scratch home");
    let dedicated_dir = home.join("dedicated-config-dir");
    let cli_listen = support::free_addr();
    let config_listen = format!("127.0.0.1:{}", support::free_port());

    let mut child = support::Reaper(spawn_setup(&home, &cli_listen));
    let stdout_lines = stream_stdout_lines(child.stdout.take().expect("piped stdout"));
    {
        let stdin = child.stdin.as_mut().expect("piped stdin");
        // In prompt order: dedicated directory, world name, max players, panel enabled, world
        // size, world seed, listen address, password, autosave interval.
        writeln!(stdin, "{}", dedicated_dir.display()).unwrap();
        writeln!(stdin, "Explicit Choices World").unwrap();
        writeln!(stdin, "6").unwrap();
        writeln!(stdin, "n").unwrap();
        writeln!(stdin, "400x300").unwrap();
        writeln!(stdin, "424242").unwrap();
        writeln!(stdin, "{config_listen}").unwrap();
        writeln!(stdin, "s3cret pass").unwrap();
        writeln!(stdin, "45").unwrap();
    }

    let config_path = dedicated_dir.join("terrustia.toml");
    assert!(
        wait_for_config(&config_path, Duration::from_secs(30)),
        "the wizard should have written {} by now",
        config_path.display()
    );
    let written = std::fs::read_to_string(&config_path).expect("read the written config");
    for expected in [
        "world_name = \"Explicit Choices World\"",
        "max_players = 6",
        "panel_enabled = false",
        "world_width = 400",
        "world_height = 300",
        "seed = 424242",
        "autosave_secs = 45",
        "password = \"s3cret pass\"",
    ] {
        assert!(
            written.contains(expected),
            "expected {expected:?} in the written config, got:\n{written}"
        );
    }
    assert!(
        written.contains(&format!("listen = \"{config_listen}\"")),
        "expected the answered listen address in the written config, got:\n{written}"
    );

    // The config is not just text: the server reads it and reaches its main loop. The CLI's
    // `--listen` still wins over the address written here, which is the existing precedence rule
    // this change deliberately does not alter.
    assert!(
        wait_for_line(
            &stdout_lines,
            "accepting connections",
            Duration::from_secs(30)
        ),
        "the server should have booted from the config the wizard wrote"
    );
    let _ = child.kill();
    let _ = child.wait();

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_wizard_refuses_a_dedicated_directory_that_already_has_something_in_it() {
    let home = scratch_home("nonempty-guard");
    std::fs::create_dir_all(&home).expect("scratch home");
    let dedicated_dir = home.join("already-has-stuff");
    std::fs::create_dir_all(&dedicated_dir).unwrap();
    std::fs::write(dedicated_dir.join("do-not-touch.txt"), b"pre-existing").unwrap();

    let mut child = support::Reaper(spawn_setup(&home, &support::free_addr()));
    {
        let stdin = child.stdin.as_mut().expect("piped stdin");
        writeln!(stdin, "{}", dedicated_dir.display()).unwrap();
    }
    let status = child.wait().expect("wait for the wizard to exit");
    assert!(
        !status.success(),
        "the wizard must refuse a non-empty dedicated directory rather than write into it"
    );

    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("captured stdout")
        .read_to_string(&mut stdout)
        .expect("read stdout");
    assert!(
        stdout.contains("already exists") || stdout.contains("not empty"),
        "expected a clear refusal on stdout, got: {stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(dedicated_dir.join("do-not-touch.txt")).unwrap(),
        "pre-existing",
        "the pre-existing file must be completely untouched"
    );
    assert!(
        !dedicated_dir.join("terrustia.toml").exists(),
        "no config should have been written into a refused directory"
    );

    let _ = std::fs::remove_dir_all(&home);
}
